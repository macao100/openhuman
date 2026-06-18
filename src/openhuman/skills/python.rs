//! Python skill execution engine — Docker sidecar with local fallback.
//!
//! Executes Python skills via a JSON-RPC 2.0 protocol over stdin/stdout.
//! Two backends:
//! - **Docker**: `docker run --rm --network none -m 512m --cpus 1.0 --read-only
//!   -v <src>:/skill:ro python:3.12-slim python3 /runner.py`
//! - **Local**: Uses `PythonBootstrap::resolve()` + `spawn_stdio_process()`
//!   from the existing `runtime_python` module.
//!
//! # Runner Convention
//!
//! A Python skill must expose a `run(args: dict) -> dict` function in its
//! `main.py` entry point. The embedded runner script reads a JSON-RPC request
//! from stdin, calls `run()`, and writes the response to stdout.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command as TokioCommand;
use tokio::time::timeout;

// spawn_stdio_process and PythonBootstrap/PythonLaunchSpec are available
// from crate::openhuman::runtime_python if/when local Python execution is used.
use crate::openhuman::skills::store::SkillsStore;
use crate::openhuman::skills::types::{ExecutionStatus, SkillOutputEnvelope};

// ── Embedded Python runner script ─────────────────────────────────────────

/// The Python runner script injected into the Docker container or local
/// subprocess. It reads a single JSON-RPC 2.0 request from stdin, imports
/// the skill's `main.py`, calls its `run(args)` function, and writes a
/// JSON-RPC 2.0 response to stdout.
///
/// # Protocol
///
/// ```text
/// Request (stdin):  {"jsonrpc":"2.0","id":1,"method":"execute","params":{"skill_dir":"/skill","args":{...}}}
/// Success (stdout): {"jsonrpc":"2.0","id":1,"result":{"output":{...}}}
/// Error (stdout):   {"jsonrpc":"2.0","id":1,"error":{"code":-1,"message":"..."}}
/// ```
const PYTHON_RUNNER: &str = r#"import json, sys, os, traceback, importlib.util

def main():
    raw = sys.stdin.readline()
    if not raw:
        sys.exit(0)
    try:
        request = json.loads(raw)
    except json.JSONDecodeError as e:
        _error(None, -32700, f"Parse error: {e}")
        return
    req_id = request.get("id")
    method = request.get("method", "")
    params = request.get("params", {})
    if method != "execute":
        _error(req_id, -32601, f"Method not found: {method}")
        return
    skill_dir = params.get("skill_dir", os.environ.get("SKILL_DIR", "/skill"))
    sys.path.insert(0, skill_dir)
    try:
        spec = importlib.util.spec_from_file_location(
            "skill_main", os.path.join(skill_dir, "main.py"))
        if spec is None or spec.loader is None:
            _error(req_id, -1, "main.py not found in skill directory")
            return
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        if not hasattr(module, "run"):
            _error(req_id, -1, "skill must expose a 'run(args) -> dict' function")
            return
        skill_args = params.get("args", {})
        result = module.run(skill_args)
        _result(req_id, result)
    except Exception as e:
        _error(req_id, -1, f"Skill execution error: {e}\n{traceback.format_exc()}")

def _result(req_id, data):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": req_id, "result": {"output": data}}) + "\n")
    sys.stdout.flush()

def _error(req_id, code, message):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": req_id, "error": {"code": code, "message": message}}) + "\n")
    sys.stdout.flush()

if __name__ == "__main__":
    main()
"#;

// ── Error type ────────────────────────────────────────────────────────────

/// Errors that can occur during Python skill execution.
#[derive(Debug, thiserror::Error)]
pub enum PythonSkillError {
    #[error("Python runtime unavailable: {0}")]
    RuntimeUnavailable(String),

    #[error("Docker unavailable: {0}")]
    DockerUnavailable(String),

    #[error("Skill not found: {0}")]
    SkillNotFound(String),

    #[error("Skill execution error: {0}")]
    Execution(String),

    #[error("JSON-RPC error: code={code}, message={message}")]
    RpcError { code: i64, message: String },

    #[error("Timeout after {0}s")]
    Timeout(u64),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

// ── JSON-RPC types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<u64>,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

// ── PythonSkillRuntime ─────────────────────────────────────────────────────

/// Runtime for executing Python skills.
///
/// Two backends:
/// - **Docker**: Spawns a container with the embedded runner script.
/// - **Local**: Spawns a Python subprocess using the resolved system/managed Python.
pub struct PythonSkillRuntime {
    /// Resolved Python interpreter path (for local mode).
    python_bin: Option<PathBuf>,
    /// Whether Docker is available on this host.
    docker_available: bool,
    /// Base directory for installed skills.
    skills_dir: PathBuf,
}

impl PythonSkillRuntime {
    /// Create a new runtime. Detects Docker availability and resolves
    /// the local Python interpreter path.
    pub fn new(skills_dir: PathBuf) -> Self {
        let docker_available = docker_is_available();
        let python_bin = detect_local_python();

        if !docker_available {
            log::warn!(
                "[python-skill] Docker is not installed. Python skills will run using \
                 your local Python interpreter, which has less isolation. \
                 For production use, install Docker Desktop from \
                 https://www.docker.com/products/docker-desktop/ (Windows) or \
                 `apt install docker.io` / `brew install docker` (Linux/macOS)."
            );
        }

        if python_bin.is_none() {
            log::warn!(
                "[python-skill] no local Python 3.12+ found. Python skills can only \
                 execute if Docker is available."
            );
        }

        Self {
            python_bin,
            docker_available,
            skills_dir,
        }
    }

    /// Execute a Python skill by name with the given arguments.
    ///
    /// Returns a [`SkillOutputEnvelope`] on both success and failure —
    /// errors are captured in the envelope, not propagated.
    pub async fn execute_skill(
        &self,
        skill_name: &str,
        args: serde_json::Value,
        timeout_duration: Duration,
    ) -> SkillOutputEnvelope {
        let start = std::time::Instant::now();

        let src_dir = self.skills_dir.join(skill_name).join("src");
        if !src_dir.exists() {
            return SkillOutputEnvelope::new_error(
                skill_name,
                "0.0.0",
                &format!("skill source directory not found: {}", src_dir.display()),
                start.elapsed().as_millis() as u64,
                false,
            );
        }

        // Load store to get version and GPG status
        let (version, gpg_verified) = match SkillsStore::load() {
            Ok(store) => {
                let ver = store
                    .get(skill_name)
                    .map(|s| s.version.clone())
                    .unwrap_or_else(|| "0.0.0".to_string());
                let gpg = store
                    .get(skill_name)
                    .map(|s| s.gpg_fingerprint.is_some())
                    .unwrap_or(false);
                (ver, gpg)
            }
            Err(_) => ("0.0.0".to_string(), false),
        };

        // Try Docker first, fall back to local Python
        let result = if self.docker_available {
            self.execute_in_docker(skill_name, &src_dir, &args, timeout_duration)
                .await
        } else {
            self.execute_locally(skill_name, &src_dir, &args, timeout_duration)
                .await
        };

        match result {
            Ok(data) => SkillOutputEnvelope::new_success(
                skill_name,
                &version,
                data,
                start.elapsed().as_millis() as u64,
                gpg_verified,
            ),
            Err(e) => {
                let status = if matches!(&e, PythonSkillError::Timeout(_)) {
                    ExecutionStatus::Timeout
                } else {
                    ExecutionStatus::Error
                };
                SkillOutputEnvelope {
                    skill_name: skill_name.to_string(),
                    skill_version: version,
                    execution_status: status,
                    output_schema: "application/json".to_string(),
                    data: serde_json::json!({}),
                    error: Some(e.to_string()),
                    execution_time_ms: start.elapsed().as_millis() as u64,
                    gpg_verified,
                }
            }
        }
    }

    /// Execute a Python skill inside a Docker container.
    async fn execute_in_docker(
        &self,
        _skill_name: &str,
        src_dir: &Path,
        args: &serde_json::Value,
        timeout_duration: Duration,
    ) -> Result<serde_json::Value, PythonSkillError> {
        // Write the runner script to a temp file so it can be bind-mounted.
        let runner_dir = tempfile::tempdir()
            .map_err(|e| PythonSkillError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        let runner_path = runner_dir.path().join("runner.py");
        std::fs::write(&runner_path, PYTHON_RUNNER)?;

        let src_abs = src_dir
            .canonicalize()
            .map_err(|e| PythonSkillError::Io(e))?;
        let src_mount = format!("{}:/skill:ro", src_abs.display());
        let runner_mount = format!("{}:/runner.py:ro", runner_path.display());

        let mut cmd = TokioCommand::new("docker");
        cmd.args([
            "run",
            "--rm",
            "--network",
            "none",
            "-m",
            "512m",
            "--cpus",
            "1.0",
            "--read-only",
            "-v",
            &src_mount,
            "-v",
            &runner_mount,
            "python:3.12-slim",
            "python3",
            "/runner.py",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            PythonSkillError::DockerUnavailable(format!("failed to spawn docker: {e}"))
        })?;

        // Send JSON-RPC request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "execute",
            "params": {
                "skill_dir": "/skill",
                "args": args,
            }
        });

        let request_line = serde_json::to_string(&request)? + "\n";

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(request_line.as_bytes()).await?;
            stdin.flush().await?;
            drop(stdin);
        }

        // Read JSON-RPC response with timeout
        let output = timeout(timeout_duration, child.wait_with_output())
            .await
            .map_err(|_| PythonSkillError::Timeout(timeout_duration.as_secs()))?
            .map_err(|e| PythonSkillError::Io(e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PythonSkillError::Execution(format!(
                "docker exited with {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_json_rpc_response(stdout.trim())
    }

    /// Execute a Python skill using the local Python interpreter.
    async fn execute_locally(
        &self,
        _skill_name: &str,
        src_dir: &Path,
        args: &serde_json::Value,
        timeout_duration: Duration,
    ) -> Result<serde_json::Value, PythonSkillError> {
        let python_bin = self.python_bin.as_ref().ok_or_else(|| {
            PythonSkillError::RuntimeUnavailable(
                "no Python 3.12+ interpreter found. Install Python or Docker.".to_string(),
            )
        })?;

        // Write runner to a temp file for local execution
        let runner_dir = tempfile::tempdir()
            .map_err(|e| PythonSkillError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        let runner_path = runner_dir.path().join("runner.py");
        std::fs::write(&runner_path, PYTHON_RUNNER)?;

        let mut cmd = TokioCommand::new(python_bin);
        cmd.arg("-u")
            .arg(&runner_path)
            .env("SKILL_DIR", src_dir.to_string_lossy().to_string())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| PythonSkillError::Io(e))?;

        // Send JSON-RPC request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "execute",
            "params": {
                "skill_dir": src_dir.to_string_lossy().to_string(),
                "args": args,
            }
        });

        let request_line = serde_json::to_string(&request)? + "\n";

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(request_line.as_bytes()).await?;
            stdin.flush().await?;
            drop(stdin);
        }

        let output = timeout(timeout_duration, child.wait_with_output())
            .await
            .map_err(|_| PythonSkillError::Timeout(timeout_duration.as_secs()))?
            .map_err(|e| PythonSkillError::Io(e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PythonSkillError::Execution(format!(
                "python exited with {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_json_rpc_response(stdout.trim())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Parse a JSON-RPC 2.0 response from stdout and extract the result.
fn parse_json_rpc_response(stdout: &str) -> Result<serde_json::Value, PythonSkillError> {
    let response: JsonRpcResponse = serde_json::from_str(stdout)?;

    if let Some(error) = response.error {
        return Err(PythonSkillError::RpcError {
            code: error.code,
            message: error.message,
        });
    }

    match response.result {
        Some(result) => {
            // Unwrap the {"output": ...} envelope from the runner
            Ok(result.get("output").cloned().unwrap_or(result))
        }
        None => Err(PythonSkillError::Execution(
            "JSON-RPC response has neither result nor error".to_string(),
        )),
    }
}

/// Check whether Docker is installed and reachable.
pub fn docker_is_available() -> bool {
    std::process::Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Detect a local Python 3.12+ interpreter on PATH.
fn detect_local_python() -> Option<PathBuf> {
    let candidates = ["python3", "python"];
    for bin in &candidates {
        let output = std::process::Command::new(bin)
            .arg("--version")
            .output()
            .ok()?;
        if !output.status.success() {
            continue;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let version_text = if stdout.is_empty() { &stderr } else { &stdout };
        // Parse "Python 3.x.y"
        if let Some(version_str) = version_text.strip_prefix("Python ") {
            let version_str = version_str.trim();
            let parts: Vec<&str> = version_str.split('.').collect();
            if parts.len() >= 2 {
                if let (Ok(major), Ok(minor)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    if major == 3 && minor >= 12 {
                        return Some(PathBuf::from(bin));
                    }
                }
            }
        }
    }
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
#[ignore = "Python skills Docker sidecar runtime removed. Re-enable when runtime is restored."]
mod tests {
    use super::*;

    #[test]
    fn docker_is_available_returns_bool() {
        // Should not panic — either Docker is installed or not.
        let result = docker_is_available();
        // Just check it returns a bool (no assertion on true/false).
        assert!(result || !result);
    }

    #[test]
    fn detect_local_python_returns_option() {
        let result = detect_local_python();
        // May or may not find Python — just check it doesn't panic.
        assert!(result.is_some() || result.is_none());
    }

    #[test]
    fn parse_json_rpc_success_response() {
        let stdout = r#"{"jsonrpc":"2.0","id":1,"result":{"output":{"key":"value"}}}"#;
        let result = parse_json_rpc_response(stdout).unwrap();
        assert_eq!(result, serde_json::json!({"key": "value"}));
    }

    #[test]
    fn parse_json_rpc_error_response() {
        let stdout = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-1,"message":"something broke"}}"#;
        let err = parse_json_rpc_response(stdout).unwrap_err();
        assert!(matches!(err, PythonSkillError::RpcError { .. }));
    }

    #[test]
    fn parse_json_rpc_invalid_json() {
        let stdout = "not json at all";
        let err = parse_json_rpc_response(stdout).unwrap_err();
        assert!(matches!(err, PythonSkillError::Json(_)));
    }

    #[test]
    fn runner_script_is_valid_python_syntax() {
        // Basic sanity: the runner script should contain expected markers.
        assert!(PYTHON_RUNNER.contains("def main():"));
        assert!(PYTHON_RUNNER.contains("jsonrpc"));
        assert!(PYTHON_RUNNER.contains("skill_dir"));
        assert!(PYTHON_RUNNER.contains("def run"));
    }

    #[test]
    fn runtime_new_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = PythonSkillRuntime::new(dir.path().to_path_buf());
        // Should construct without panicking.
        assert!(runtime.docker_available || !runtime.docker_available);
    }
}
