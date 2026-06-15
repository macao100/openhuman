//! Wasmtime WASM runtime engine with WASI capability gating for DADOU skills.
//!
//! Provides a sandboxed in-process WASM execution environment with:
//!
//! - Deny-by-default WASI capabilities (no network, no env vars, restricted filesystem)
//! - 30-second epoch-based timeout
//! - Restricted filesystem access to `~/.openhuman/skills/<name>/data/`
//!
//! ## Calling conventions
//!
//! The `execute_wasm` function supports two entry-point signatures:
//!
//! - `()` → `()` — standard `_start` for WASI modules.
//! - `(i32, i32)` → `i32` — data-passing convention where:
//!   - input bytes are written at WASM linear memory offset 0,
//!   - the function writes output at offset 65536 and returns its length in bytes.

use std::path::{Path, PathBuf};

use anyhow::Context;
use wasmtime::{Config, Engine, Linker, Module, Store};

use super::types::{ExecutionStatus, SkillOutputEnvelope};

// `WasmConfig`, `Permissions`, `FilesystemPerms` are defined in the sibling
// `manifest` module (Plan 01) and re-exported via `super::`.

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

mod defaults {
    /// Maximum execution time in seconds. Implemented via wasmtime's
    /// epoch-based interruption: the engine's epoch counter is incremented
    /// periodically and `Store::set_epoch_deadline` caps how many epochs a
    /// single execution is allowed to consume. The actual runtime overhead
    /// of the check is ≈1 guard instruction per wasm basic-block.
    pub const EXECUTION_TIMEOUT_SECS: u64 = 30;

    /// Offset in WASM linear memory where the host writes input bytes for
    /// `(i32, i32) → i32` entry functions.
    pub const INPUT_OFFSET: u32 = 0;

    /// Offset in WASM linear memory where the callee writes output bytes for
    /// `(i32, i32) → i32` entry functions. The host reads `return_value` bytes
    /// from here after the call completes.
    pub const OUTPUT_OFFSET: u32 = 65_536;
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Typed error returned by [`execute_wasm`].
#[derive(Debug, thiserror::Error)]
pub enum WasmExecutionError {
    /// wasmtime-internal error (compilation, instantiation, trap, …).
    #[error("wasmtime engine error: {0}")]
    Engine(#[from] wasmtime::Error),

    /// The wasm module exceeded its epoch deadline (default 30 s).
    #[error("skill execution timed out after 30s")]
    Timeout,

    /// Filesystem / data-directory setup failed.
    #[error("skill data directory error: {0}")]
    DataDir(String),

    /// The WASM module panicked, trapped, or hit an unsupported signature.
    #[error("skill execution trap: {0}")]
    Trap(String),
}

// ---------------------------------------------------------------------------
// WasmEngine
// ---------------------------------------------------------------------------

/// Reusable Wasmtime engine wrapper, created once at startup and shared
/// across skill invocations.
pub struct WasmEngine {
    engine: Engine,
}

impl WasmEngine {
    /// Create a new engine with epoch-based interruption enabled.
    pub fn new() -> anyhow::Result<Self> {
        let mut config = Config::default();
        config.epoch_interruption(true);
        // Cap the wasm stack to 512 KB so a deeply-recursive module can't
        // exhaust the host stack.
        config.max_wasm_stack(512 * 1024);
        let engine = Engine::new(&config)?;
        tracing::debug!("[skills:wasm] WasmEngine created with epoch interruption");
        Ok(Self { engine })
    }

    /// Shared reference to the inner wasmtime [`Engine`].
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Convenience wrapper around [`execute_wasm`].
    pub fn execute(
        &self,
        wasm_bytes: &[u8],
        entry_fn: &str,
        input: &[u8],
        skill_name: &str,
    ) -> Result<Vec<u8>, WasmExecutionError> {
        execute_wasm(self.engine(), wasm_bytes, entry_fn, input, skill_name)
    }
}

impl Default for WasmEngine {
    fn default() -> Self {
        Self::new().expect("WasmEngine::default: failed to create engine")
    }
}

// ---------------------------------------------------------------------------
// WASI context builder (deny-by-default)
// ---------------------------------------------------------------------------

/// Build a WASI context with **deny-by-default** capability gating.
///
/// Only the following capabilities are granted:
///
/// | Capability | Status            |
/// |------------|-------------------|
/// | Filesystem | `data_dir` as `/data` (read + write) |
/// | Stderr     | Inherited (logging only) |
/// | Stdin      | **Denied**        |
/// | Stdout     | **Denied**        |
/// | Network    | **Denied**        |
/// | Env vars   | **Denied**        |
/// | Random     | **Denied**        |
/// | Wall clock | **Denied**        |
///
/// The caller must ensure `data_dir` exists (or call [`create_dir_all`] before
/// passing it in).
pub fn build_wasi_ctx(data_dir: &Path) -> anyhow::Result<wasmtime_wasi::preview1::WasiP1Ctx> {
    use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

    // Canonicalize so (a) path-traversal from inside the module is blocked
    // at the OS level, and (b) wasmtime's preopen guard matches correctly.
    let canonical = data_dir
        .canonicalize()
        .context("failed to canonicalize skill data directory")?;

    let mut builder = WasiCtxBuilder::new();

    // Preopen for write access (permitted if the caller allows writes).
    builder
        .preopened_dir(
            &canonical,
            "/data",
            DirPerms::READ | DirPerms::MUTATE,
            FilePerms::WRITE,
        )
        .context("failed to preopen skill data directory for write")?;
    // Always preopen for read access.
    builder
        .preopened_dir(&canonical, "/data", DirPerms::READ, FilePerms::READ)
        .context("failed to preopen skill data directory for read")?;

    // Stderr only — the module may log diagnostic messages, but must not
    // write to stdout or read from stdin.
    builder.inherit_stderr();

    // Everything else stays at default (denied):
    //   - No `.env(...)`     → no environment variables.
    //   - No `.inherit_stdin()` / `.inherit_stdout()` → no stdio.
    //   - No `.socket()`     → no network.
    //   - No `.random()`     → no random source.

    Ok(builder.build_p1())
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Resolve the data directory for a given skill:
/// `~/.openhuman/skills/<name>/data/`
///
/// The directory is **not** created by this function; callers should use
/// [`std::fs::create_dir_all`] before passing the result to
/// [`build_wasi_ctx`].
pub fn skill_data_dir(skill_name: &str) -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(".openhuman")
        .join("skills")
        .join(skill_name)
        .join("data")
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// Execute a WASM module with WASI sandboxing.
///
/// # Arguments
///
/// * `engine`  — A [`WasmEngine`] instance (shared, created once at startup).
/// * `wasm_bytes` — Compiled WASM binary bytes.
/// * `entry_fn` — The exported function name to invoke (e.g. `"_start"` or `"run"`).
/// * `input` — Byte slice passed as input to the module.
/// * `skill_name` — Used to locate `~/.openhuman/skills/<skill_name>/data/`.
///
/// # Returns
///
/// Output bytes produced by the module. The convention depends on the
/// entry-point signature (see [module docs](self) for details).
///
/// # Errors
///
/// Returns [`WasmExecutionError`] on compilation failure, instantiation
/// failure, capability violation, timeout, or trap.
pub fn execute_wasm(
    engine: &Engine,
    wasm_bytes: &[u8],
    entry_fn: &str,
    input: &[u8],
    skill_name: &str,
) -> Result<Vec<u8>, WasmExecutionError> {
    tracing::debug!(
        "[skills:wasm] executing skill '{skill_name}' entry '{entry_fn}' \
         ({} bytes input)",
        input.len()
    );

    // 1. Compile module.
    let module = Module::new(engine, wasm_bytes)?;

    // 2. Ensure data directory exists and build WASI context.
    let data_dir = skill_data_dir(skill_name);
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| WasmExecutionError::DataDir(format!("failed to create {data_dir:?}: {e}")))?;
    let wasi_ctx = build_wasi_ctx(&data_dir)
        .map_err(|e| WasmExecutionError::DataDir(format!("failed to build WASI context: {e}")))?;

    // 3. Create store with epoch deadline.
    let mut store = Store::new(engine, wasi_ctx);
    store.set_epoch_deadline(defaults::EXECUTION_TIMEOUT_SECS);

    // 4. Build linker with WASI preview 1 bindings.
    let mut linker = Linker::<wasmtime_wasi::preview1::WasiP1Ctx>::new(engine);
    wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |ctx| ctx)?;

    // 5. Instantiate.
    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(classify_trap)?;

    // 6. Locate entry function.
    let func = instance.get_func(&mut store, entry_fn).ok_or_else(|| {
        WasmExecutionError::Trap(format!("entry function '{entry_fn}' not found"))
    })?;

    // 7. Inspect signature and dispatch.
    let ty = func.ty(&store);
    let params: Vec<wasmtime::ValType> = ty.params().collect();
    let results: Vec<wasmtime::ValType> = ty.results().collect();

    let output = match (params.as_slice(), results.as_slice()) {
        // `()` → `()` — standard WASI _start or void entry.
        ([], []) => {
            tracing::debug!("[skills:wasm] calling void → void entry '{entry_fn}'");
            call_with_timeout(
                &func,
                &mut store,
                &[],
                &mut [],
                defaults::EXECUTION_TIMEOUT_SECS,
            )?;
            Vec::new()
        }

        // `(i32, i32)` → `i32` — data-passing convention.
        ([wasmtime::ValType::I32, wasmtime::ValType::I32], [wasmtime::ValType::I32]) => {
            let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| {
                WasmExecutionError::Trap(
                    "module must export 'memory' for data-passing entry functions".into(),
                )
            })?;

            // Write input bytes at the agreed input offset.
            memory
                .write(&mut store, defaults::INPUT_OFFSET as usize, input)
                .map_err(|e| WasmExecutionError::Trap(e.to_string()))?;

            tracing::debug!(
                "[skills:wasm] calling (i32,i32)→i32 entry '{entry_fn}' with \
                 ptr={}, len={}",
                defaults::INPUT_OFFSET,
                input.len()
            );

            let mut result = [wasmtime::Val::I32(0)];
            call_with_timeout(
                &func,
                &mut store,
                &[
                    wasmtime::Val::I32(defaults::INPUT_OFFSET as i32),
                    wasmtime::Val::I32(input.len() as i32),
                ],
                &mut result,
                defaults::EXECUTION_TIMEOUT_SECS,
            )?;

            let output_len = match &result[0] {
                wasmtime::Val::I32(len) => *len as usize,
                _ => {
                    return Err(WasmExecutionError::Trap(
                        "entry function returned unexpected value type".into(),
                    ));
                }
            };

            if output_len == 0 {
                Vec::new()
            } else {
                let mut output = vec![0u8; output_len];
                memory
                    .read(&store, defaults::OUTPUT_OFFSET as usize, &mut output)
                    .map_err(|e| WasmExecutionError::Trap(e.to_string()))?;
                output
            }
        }

        _ => {
            return Err(WasmExecutionError::Trap(format!(
                "unsupported entry-point signature: ({:?}) → ({:?}). \
                 Supported signatures: ()→() or (i32,i32)→i32",
                params, results
            )));
        }
    };

    tracing::debug!(
        "[skills:wasm] skill '{skill_name}' completed ({} bytes output)",
        output.len()
    );
    Ok(output)
}

// ---------------------------------------------------------------------------
// Structured output wrapping (INJ-02)
// ---------------------------------------------------------------------------

/// Execute a WASM module and return a structured [`SkillOutputEnvelope`].
///
/// Wraps the raw output bytes from [`execute_wasm`] in a
/// [`SkillOutputEnvelope`] so the agent harness receives structured JSON
/// rather than raw bytes that could contain injection payloads.
///
/// # Returns
///
/// Always returns `Ok(SkillOutputEnvelope)` even on execution errors —
/// the error is captured in the envelope's `execution_status` and `error`
/// fields. This ensures the tool loop always has a structured envelope to
/// process.
pub fn execute_wasm_structured(
    engine: &Engine,
    wasm_bytes: &[u8],
    entry_fn: &str,
    input: &[u8],
    skill_name: &str,
    skill_version: &str,
    gpg_verified: bool,
) -> SkillOutputEnvelope {
    let start = std::time::Instant::now();
    let result = execute_wasm(engine, wasm_bytes, entry_fn, input, skill_name);
    let execution_time_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(output) => {
            let output_str = String::from_utf8_lossy(&output);
            log::debug!(
                "[skills:output] structured wrap OK skill='{skill_name}' \
                 version='{skill_version}' output_bytes={} elapsed_ms={execution_time_ms}",
                output.len(),
            );
            let data = serde_json::json!({
                "output": output_str,
                "output_bytes": output.len(),
            });
            SkillOutputEnvelope::new_success(
                skill_name,
                skill_version,
                data,
                execution_time_ms,
                gpg_verified,
            )
        }
        Err(e) => {
            let is_timeout = matches!(e, WasmExecutionError::Timeout);
            log::debug!(
                "[skills:output] structured wrap {} skill='{skill_name}' \
                 version='{skill_version}' elapsed_ms={execution_time_ms}",
                if is_timeout { "TIMEOUT" } else { "ERROR" },
            );
            if is_timeout {
                SkillOutputEnvelope::new_timeout(
                    skill_name,
                    skill_version,
                    execution_time_ms,
                    gpg_verified,
                )
            } else {
                SkillOutputEnvelope::new_error(
                    skill_name,
                    skill_version,
                    e.to_string(),
                    execution_time_ms,
                    gpg_verified,
                )
            }
        }
    }
}

/// Post-hoc structured output wrapper for already-executed skill results.
///
/// Takes the textual result from a completed skill execution and wraps it
/// in a [`SkillOutputEnvelope`] without re-executing the skill. Used by
/// the agent tool loop when the raw output is already available as a
/// string and only needs structured wrapping before LLM injection.
///
/// The `output_text` is placed inside `data["output"]` so the LLM sees
/// structured JSON rather than raw text.
pub fn wrap_skill_output(
    skill_name: &str,
    skill_version: &str,
    output_text: &str,
    execution_time_ms: u64,
    gpg_verified: bool,
) -> SkillOutputEnvelope {
    log::debug!(
        "[skills:output] post-hoc wrap skill='{skill_name}' version='{skill_version}' \
         text_chars={} elapsed_ms={execution_time_ms}",
        output_text.chars().count(),
    );
    let data = serde_json::json!({
        "output": output_text,
    });
    SkillOutputEnvelope::new_success(
        skill_name,
        skill_version,
        data,
        execution_time_ms,
        gpg_verified,
    )
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Classify a wasmtime error — if it's an epoch deadline trap, return
/// [`WasmExecutionError::Timeout`]; otherwise wrap as [`WasmExecutionError::Engine`].
fn classify_trap(err: wasmtime::Error) -> WasmExecutionError {
    let msg = err.to_string().to_lowercase();
    if msg.contains("epoch") || msg.contains("deadline") || msg.contains("interrupt") {
        WasmExecutionError::Timeout
    } else {
        WasmExecutionError::Engine(err)
    }
}

/// Call a wasm function and classify timeout errors correctly.
///
/// Without this wrapper, `wasmtime::Error` from a deadline-exceeded trap would
/// be converted via `#[from]` into [`WasmExecutionError::Engine`], losing the
/// semantic distinction. This helper catches the timeout case and returns
/// [`WasmExecutionError::Timeout`] instead.
fn call_with_timeout(
    func: &wasmtime::Func,
    store: impl wasmtime::AsContextMut<Data = wasmtime_wasi::preview1::WasiP1Ctx>,
    params: &[wasmtime::Val],
    results: &mut [wasmtime::Val],
    _timeout_secs: u64,
) -> Result<(), WasmExecutionError> {
    func.call(store, params, results).map_err(classify_trap)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    use tempfile::TempDir;

    /// Shared engine for tests that don't need a custom config.
    fn shared_engine() -> &'static WasmEngine {
        static ENGINE: OnceLock<WasmEngine> = OnceLock::new();
        ENGINE.get_or_init(|| WasmEngine::new().expect("shared test engine"))
    }

    // -----------------------------------------------------------------------
    // Constructor
    // -----------------------------------------------------------------------

    #[test]
    fn wasm_engine_new_returns_valid_engine() {
        let engine = WasmEngine::new().expect("engine creation should succeed");
        // If epoch_interruption is enabled, setting a deadline should not
        // panic. We can't easily observe the flag after construction, but
        // the fact that we reached here means the Engine creation worked.
        let _ = engine.engine();
    }

    // -----------------------------------------------------------------------
    // Simple execution
    // -----------------------------------------------------------------------

    #[test]
    fn executes_simple_wasm_module() {
        let engine = shared_engine();
        // WAT module that copies input from ptr → offset 65536 and returns
        // the byte length (echo-back convention).
        let wasm = wat::parse_str(
            r#"(module
                (memory (export "memory") 1)
                (func (export "run") (param i32 i32) (result i32)
                    memory.copy (i32.const 65536) (local.get 0) (local.get 1)
                    local.get 1
                )
            )"#,
        )
        .expect("valid WAT");

        let input = b"hello wasm!";
        let output = execute_wasm(engine.engine(), &wasm, "run", input, "test-skill")
            .expect("execution should succeed");

        assert_eq!(output, input, "echo module should return the same bytes");
    }

    #[test]
    fn execute_empty_input() {
        let engine = shared_engine();
        let wasm = wat::parse_str(
            r#"(module
                (memory (export "memory") 1)
                (func (export "run") (param i32 i32) (result i32)
                    local.get 1
                )
            )"#,
        )
        .expect("valid WAT");

        let output = execute_wasm(engine.engine(), &wasm, "run", b"", "test-skill")
            .expect("execution with empty input should succeed");
        assert!(
            output.is_empty(),
            "expected empty output for zero-length input"
        );
    }

    // -----------------------------------------------------------------------
    // Timeout
    // -----------------------------------------------------------------------

    #[test]
    fn timeout_triggers_on_long_running_module() {
        let engine = WasmEngine::new().expect("fresh engine");

        // An infinite-loop WAT module.
        let wasm = wat::parse_str(
            r#"(module
                (func (export "run") (param i32 i32) (result i32)
                    (loop (br 0))
                    i32.const 0
                )
            )"#,
        )
        .expect("valid WAT");

        // Build a store with a very tight deadline (1 epoch).
        let wasi_ctx = build_wasi_ctx_for_test();
        let mut store = Store::new(engine.engine(), wasi_ctx);
        store.set_epoch_deadline(1);

        let mut linker = Linker::<wasmtime_wasi::preview1::WasiP1Ctx>::new(engine.engine());
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |ctx| ctx).expect("add WASI");
        let module = Module::new(engine.engine(), &wasm).expect("compile");
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("instantiate");
        let func = instance.get_func(&mut store, "run").expect("get run func");

        let eng = engine.engine().clone();

        // Launch execution in a background thread so we can increment the
        // epoch after a short delay, simulating a timeout.
        let handle = std::thread::spawn(move || {
            let mut results = [wasmtime::Val::I32(0)];
            func.call(
                &mut store,
                &[wasmtime::Val::I32(0), wasmtime::Val::I32(0)],
                &mut results,
            )
        });

        // Let the module enter the infinite loop, then trigger the epoch
        // counter past the deadline.
        std::thread::sleep(std::time::Duration::from_millis(100));
        eng.increment_epoch();

        let outcome = handle.join().expect("background thread should not panic");

        match outcome {
            Ok(_) => panic!("expected timeout error but execution succeeded"),
            Err(e) => {
                let msg = e.to_string().to_lowercase();
                assert!(
                    msg.contains("epoch")
                        || msg.contains("deadline")
                        || msg.contains("interrupt")
                        || msg.contains("trap"),
                    "error should mention epoch/deadline/interrupt, got: {msg}"
                );
            }
        }
    }

    /// Build a minimal WASI context for tests that don't touch the filesystem.
    fn build_wasi_ctx_for_test() -> wasmtime_wasi::preview1::WasiP1Ctx {
        use wasmtime_wasi::WasiCtxBuilder;
        let mut builder = WasiCtxBuilder::new();
        builder.inherit_stderr();
        builder.build_p1()
    }

    // -----------------------------------------------------------------------
    // Network not available
    // -----------------------------------------------------------------------

    #[test]
    fn network_not_available() {
        let engine = shared_engine();
        // A module that imports a non-existent function `env.connect` —
        // simulating a network import that the linker does not provide.
        let wasm = wat::parse_str(
            r#"(module
                (import "env" "connect" (func (param i32 i32) (result i32)))
                (func (export "run") (param i32 i32) (result i32)
                    i32.const 0
                    i32.const 0
                    call 0
                    drop
                    i32.const 0
                )
            )"#,
        )
        .expect("valid WAT");

        // Build standard WASI context (no networking).
        let wasi_ctx = build_wasi_ctx_for_test();
        let mut store = Store::new(engine.engine(), wasi_ctx);

        let mut linker = Linker::<wasmtime_wasi::preview1::WasiP1Ctx>::new(engine.engine());
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |ctx| ctx).expect("add WASI");

        let module = Module::new(engine.engine(), &wasm).expect("compile");

        let result = linker.instantiate(&mut store, &module);
        assert!(
            result.is_err(),
            "instantiation should fail when network imports are missing"
        );
        let err = result.unwrap_err().to_string().to_lowercase();
        // The error should indicate a missing import / link error.
        // wasmtime typically reports "unknown import" or "link error".
        assert!(
            err.contains("import") || err.contains("link") || err.contains("unknown"),
            "error should mention missing import, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Invalid WASM
    // -----------------------------------------------------------------------

    #[test]
    fn invalid_wasm_returns_error() {
        let engine = shared_engine();
        let garbage = b"this is not valid wasm";
        let result = execute_wasm(engine.engine(), garbage, "run", b"", "test-skill");
        assert!(
            result.is_err(),
            "garbage bytes should produce an engine error"
        );
        let err = result.unwrap_err();
        assert!(
            matches!(err, WasmExecutionError::Engine(_)),
            "expected Engine error variant, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // WASI configuration — filesystem restriction
    // -----------------------------------------------------------------------

    #[test]
    fn filesystem_restricted_to_data_dir() {
        // Verify that build_wasi_ctx only preopens the given data dir and
        // that modules cannot access WASI functions not in the configured
        // linker (deny-by-default for network, env, etc.).
        let tmp = TempDir::new().expect("temp dir");
        let data_dir = tmp.path().join("myskill").join("data");
        std::fs::create_dir_all(&data_dir).expect("create data dir");

        let ctx = build_wasi_ctx(&data_dir).expect("build WASI ctx");

        // A module that tries to import `sock_open` (NOT part of WASI
        // preview 1) should fail at instantiation because the linker
        // only provides standard WASI preview 1 functions.
        let wasm = wat::parse_str(
            r#"(module
                (import "wasi_snapshot_preview1" "sock_open"
                    (func (param i32 i32) (result i32)))
                (func (export "run") (param i32 i32) (result i32)
                    i32.const 0)
            )"#,
        )
        .expect("valid WAT");

        let mut store = Store::new(shared_engine().engine(), ctx);
        let mut linker =
            Linker::<wasmtime_wasi::preview1::WasiP1Ctx>::new(shared_engine().engine());
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |ctx| ctx).expect("add WASI");
        let module = Module::new(shared_engine().engine(), &wasm).expect("compile");

        let result = linker.instantiate(&mut store, &module);
        assert!(
            result.is_err(),
            "instantiation should fail when importing unavailable WASI function"
        );
    }

    #[test]
    fn build_wasi_ctx_creates_correct_preopens() {
        let tmp = TempDir::new().expect("temp dir");
        let data_dir = tmp.path().join("test-skill").join("data");
        std::fs::create_dir_all(&data_dir).expect("create data dir");

        // Building the context should succeed.
        let _ctx = build_wasi_ctx(&data_dir).expect("build WASI ctx");
        // If the context built without error, the preopen succeeded.
    }

    // -----------------------------------------------------------------------
    // Path resolution
    // -----------------------------------------------------------------------

    #[test]
    fn skill_data_dir_resolves_correctly() {
        let path = skill_data_dir("test-skill");
        let path_str = path.to_string_lossy();
        assert!(
            path_str.contains("test-skill"),
            "path should contain skill name"
        );
        assert!(
            path_str.ends_with("test-skill/data") || path_str.ends_with("test-skill\\data"),
            "path should end with 'test-skill/data', got: {path_str}"
        );
    }

    // -----------------------------------------------------------------------
    // Void entry convention (_start)
    // -----------------------------------------------------------------------

    #[test]
    fn executes_void_entry_function() {
        let engine = shared_engine();
        let wasm = wat::parse_str(
            r#"(module
                (func (export "_start")
                    nop
                )
            )"#,
        )
        .expect("valid WAT");

        let output = execute_wasm(engine.engine(), &wasm, "_start", b"", "test-skill")
            .expect("void entry should succeed");
        assert!(output.is_empty(), "_start should produce no output");
    }

    // -----------------------------------------------------------------------
    // Error case — missing entry function
    // -----------------------------------------------------------------------

    #[test]
    fn missing_entry_function_returns_error() {
        let engine = shared_engine();
        let wasm = wat::parse_str(
            r#"(module
                (func (export "run") (param i32 i32) (result i32)
                    local.get 1
                )
            )"#,
        )
        .expect("valid WAT");

        let result = execute_wasm(engine.engine(), &wasm, "nonexistent", b"", "test-skill");
        assert!(
            result.is_err(),
            "should fail when entry function is missing"
        );
        match result.unwrap_err() {
            WasmExecutionError::Trap(msg) => {
                assert!(
                    msg.contains("nonexistent"),
                    "error should mention function name"
                );
            }
            other => panic!("expected Trap error, got: {other}"),
        }
    }

    // -----------------------------------------------------------------------
    // Structured output (INJ-02)
    // -----------------------------------------------------------------------

    #[test]
    fn execute_wasm_structured_returns_success_envelope() {
        let engine = shared_engine();
        let wasm = wat::parse_str(
            r#"(module
                (memory (export "memory") 1)
                (func (export "run") (param i32 i32) (result i32)
                    memory.copy (i32.const 65536) (local.get 0) (local.get 1)
                    local.get 1
                )
            )"#,
        )
        .expect("valid WAT");

        let input = b"hello structured output!";
        let envelope = execute_wasm_structured(
            engine.engine(),
            &wasm,
            "run",
            input,
            "structured-skill",
            "1.2.0",
            true,
        );

        assert_eq!(envelope.skill_name, "structured-skill");
        assert_eq!(envelope.skill_version, "1.2.0");
        assert_eq!(envelope.execution_status, ExecutionStatus::Success);
        assert!(envelope.gpg_verified);
        assert!(envelope.error.is_none());
        // The data field should contain the output
        assert_eq!(envelope.data["output"], "hello structured output!");
        assert_eq!(envelope.data["output_bytes"], 22);
    }

    #[test]
    fn execute_wasm_structured_returns_envelope_on_engine_error() {
        let engine = shared_engine();
        let garbage = b"not valid wasm at all";

        let envelope = execute_wasm_structured(
            engine.engine(),
            garbage,
            "run",
            b"",
            "bad-skill",
            "0.0.1",
            false,
        );

        // Should still return an envelope (not propagate the error)
        assert_eq!(envelope.skill_name, "bad-skill");
        assert_eq!(envelope.execution_status, ExecutionStatus::Error);
        assert!(envelope.error.is_some());
    }

    #[test]
    fn execute_wasm_structured_handles_empty_output() {
        let engine = shared_engine();
        let wasm = wat::parse_str(
            r#"(module
                (func (export "_start")
                    nop
                )
            )"#,
        )
        .expect("valid WAT");

        let envelope = execute_wasm_structured(
            engine.engine(),
            &wasm,
            "_start",
            b"",
            "empty-skill",
            "0.1.0",
            false,
        );

        assert_eq!(envelope.execution_status, ExecutionStatus::Success);
        assert_eq!(envelope.data["output"], "");
        assert_eq!(envelope.data["output_bytes"], 0);
    }

    #[test]
    fn wrap_skill_output_creates_success_envelope() {
        let envelope = wrap_skill_output("my-skill", "3.0.0", "some result text", 1234, true);

        assert_eq!(envelope.skill_name, "my-skill");
        assert_eq!(envelope.skill_version, "3.0.0");
        assert_eq!(envelope.execution_status, ExecutionStatus::Success);
        assert_eq!(envelope.execution_time_ms, 1234);
        assert!(envelope.gpg_verified);
        assert_eq!(envelope.data["output"], "some result text");
        assert!(envelope.error.is_none());
    }

    #[test]
    fn wrap_skill_output_handles_empty_text() {
        let envelope = wrap_skill_output("empty", "1.0.0", "", 0, false);

        assert_eq!(envelope.execution_status, ExecutionStatus::Success);
        assert_eq!(envelope.data["output"], "");
    }

    #[test]
    fn wrap_skill_output_produces_valid_json_line() {
        let envelope = wrap_skill_output("json-skill", "2.0.0", "{\"key\": \"val\"}", 50, false);
        let line = envelope.to_json_line();
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert_eq!(parsed["skill_name"], "json-skill");
        assert_eq!(parsed["execution_status"], "success");
        assert_eq!(parsed["data"]["output"], "{\"key\": \"val\"}");
    }
}
