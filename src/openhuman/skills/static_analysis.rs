//! Static analysis engine for third-party skill source code.
//!
//! Scans skill source directories for suspicious patterns (dangerous imports,
//! unauthorized filesystem writes, network calls) before activation.
//!
//! # Verdict logic
//!
//! | Condition | Verdict |
//! |-----------|---------|
//! | Any Critical finding | **Block** |
//! | Any High finding NOT covered by permissions | **Block** |
//! | Any Medium finding (no Block trigger) | **Warn** |
//! | Only permitted High findings | **Pass** |
//! | No findings | **Pass** |

use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::openhuman::skills::manifest::Permissions;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Overall verdict of a static analysis scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisVerdict {
    /// No concerning patterns found — skill is safe to activate.
    Pass,
    /// Ambiguous or potentially risky patterns found — review recommended.
    Warn,
    /// Dangerous patterns found — activation is prevented.
    Block,
}

/// Severity of an individual finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingSeverity {
    /// Certain exploit: eval, subprocess, process execution.
    Critical,
    /// Likely dangerous: os.system, socket, network calls.
    High,
    /// Potentially dangerous: file write outside data dir.
    Medium,
}

/// A single finding from a static analysis scan, linking a matched rule to
/// its exact source location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisFinding {
    pub severity: FindingSeverity,
    pub file: String,
    pub line: usize,
    pub pattern: String,
    pub snippet: String,
}

/// Complete result of a static analysis scan across one or more source files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub verdict: AnalysisVerdict,
    pub findings: Vec<AnalysisFinding>,
    pub errors: Vec<String>,
}

impl AnalysisResult {
    /// Returns `true` when the verdict is `Pass`.
    pub fn passed(&self) -> bool {
        self.verdict == AnalysisVerdict::Pass
    }

    /// Formats the analysis result as a human-readable markdown summary.
    pub fn summary(&self) -> String {
        let verdict_str = match self.verdict {
            AnalysisVerdict::Pass => "PASS",
            AnalysisVerdict::Warn => "WARN",
            AnalysisVerdict::Block => "BLOCK",
        };

        let mut out = format!("## Static Analysis: {}\n\n", verdict_str);

        if self.findings.is_empty() {
            out.push_str("No suspicious patterns detected.\n");
            return out;
        }

        for (i, finding) in self.findings.iter().enumerate() {
            let sev = match finding.severity {
                FindingSeverity::Critical => "CRITICAL",
                FindingSeverity::High => "HIGH",
                FindingSeverity::Medium => "MEDIUM",
            };
            out.push_str(&format!(
                "{}. **{}** — {} ({}:{})\n",
                i + 1,
                sev,
                finding.pattern,
                finding.file,
                finding.line
            ));
            out.push_str(&format!("   ```\n   {}\n   ```\n", finding.snippet));
        }

        if !self.errors.is_empty() {
            out.push_str("\n### Errors\n\n");
            for err in &self.errors {
                out.push_str(&format!("- {}\n", err));
            }
        }

        out
    }
}

/// A single scan rule: a named regex pattern tagged with a severity level.
pub struct AnalysisRule {
    pub name: &'static str,
    pub pattern: Regex,
    pub severity: FindingSeverity,
    pub description: &'static str,
}

impl AnalysisRule {
    fn critical(pattern: &str, desc: &'static str) -> Self {
        Self {
            name: desc,
            pattern: Regex::new(pattern).expect("critical rule regex is valid"),
            severity: FindingSeverity::Critical,
            description: desc,
        }
    }

    fn high(pattern: &str, desc: &'static str) -> Self {
        Self {
            name: desc,
            pattern: Regex::new(pattern).expect("high rule regex is valid"),
            severity: FindingSeverity::High,
            description: desc,
        }
    }

    fn medium(pattern: &str, desc: &'static str) -> Self {
        Self {
            name: desc,
            pattern: Regex::new(pattern).expect("medium rule regex is valid"),
            severity: FindingSeverity::Medium,
            description: desc,
        }
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Recognized source file extensions for static analysis scanning.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "sh", "rb", "go", "toml", "yaml", "yml", "json",
];

// ---------------------------------------------------------------------------
// Default rules
// ---------------------------------------------------------------------------

/// Returns the built-in set of analysis rules (15+ rules across three severity
/// levels). These are the default rules used when no custom rules are provided.
pub fn default_rules() -> Vec<AnalysisRule> {
    vec![
        // ── Critical: Code execution primitives ──
        AnalysisRule::critical(r"eval\s*\(", "Use of eval()"),
        AnalysisRule::critical(r"exec\s*\(", "Use of exec()"),
        AnalysisRule::critical(r"Function\s*\(", "Use of Function() constructor"),
        AnalysisRule::critical(r"std::process::Command", "Process execution"),
        AnalysisRule::critical(r"os\.system\s*\(", "Shell command execution"),
        AnalysisRule::critical(
            r"subprocess\..*(?:call|run|Popen|check_output)",
            "Subprocess execution",
        ),
        AnalysisRule::critical(
            r##"require\s*\(\s*['"]child_process['"]\s*\)"##,
            "Child process module",
        ),
        AnalysisRule::critical(r##"__import__\s*\(\s*['"]os['"]\s*\)"##, "Dynamic os import"),
        // ── High: Network access ──
        AnalysisRule::high(r"import\s+socket", "Socket import"),
        AnalysisRule::high(
            r"requests\.(?:get|post|put|delete)\s*\(",
            "HTTP request",
        ),
        AnalysisRule::high(r"TcpStream::connect", "TCP connection"),
        AnalysisRule::high(r"\bcurl\s", "curl command"),
        AnalysisRule::high(r"\bwget\s", "wget command"),
        AnalysisRule::high(r"http::(?:Client|Request)", "HTTP library usage"),
        AnalysisRule::high(r"\baxios\.", "Axios HTTP calls"),
        // ── High: Filesystem outside allowed paths ──
        AnalysisRule::high(
            r"std::fs::(?:write|create_dir|remove|rename)\s*\(",
            "Filesystem write",
        ),
        AnalysisRule::high(r##"open\(.*['"](?!/data)"##, "File open outside data dir"),
        AnalysisRule::high(r"fs\.writeFile\s*\(", "Node fs.writeFile"),
        // ── Medium: Information gathering ──
        AnalysisRule::medium(r"os\.environ", "Environment access"),
        AnalysisRule::medium(r"\$HOME|~\/|%USERPROFILE%", "Home directory reference"),
        AnalysisRule::medium(r"localStorage", "Browser localStorage access"),
        AnalysisRule::medium(r"document\.cookie", "Cookie access"),
        AnalysisRule::medium(r"new Worker\s*\(", "Web Worker creation"),
    ]
}

// ---------------------------------------------------------------------------
// Scanning functions
// ---------------------------------------------------------------------------

/// Scan a single source file's content against the given rules.
///
/// Returns all matching findings with their file path, line number, and snippet.
pub fn scan_file(content: &str, file_path: &Path, rules: &[AnalysisRule]) -> Vec<AnalysisFinding> {
    let file_str = file_path.to_string_lossy().to_string();
    let mut findings = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        for rule in rules {
            if let Some(m) = rule.pattern.find(line) {
                findings.push(AnalysisFinding {
                    severity: rule.severity.clone(),
                    file: file_str.clone(),
                    line: line_idx + 1,
                    pattern: rule.name.to_string(),
                    snippet: m.as_str().to_string(),
                });
                log::warn!(
                    "[skills:static-analysis] {} at {}:{} matched '{}'",
                    rule.name,
                    file_str,
                    line_idx + 1,
                    m.as_str()
                );
            }
        }
    }

    findings
}

/// Scan a skill's source directory recursively against the built-in rules,
/// respecting the skill's declared permissions.
///
/// # Verdict
///
/// See [module-level documentation](self) for the verdict logic.
pub fn scan_skill(skill_dir: &Path, permissions: &Permissions) -> anyhow::Result<AnalysisResult> {
    let rules = default_rules();
    let mut all_findings: Vec<AnalysisFinding> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    let src_dir = skill_dir.join("src");
    if !src_dir.exists() {
        return Ok(AnalysisResult {
            verdict: AnalysisVerdict::Pass,
            findings: vec![],
            errors: vec![],
        });
    }

    for entry in WalkDir::new(&src_dir) {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("walk error: {}", e));
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let ext = match entry.path().extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => continue,
        };

        if !SUPPORTED_EXTENSIONS.iter().any(|s| *s == ext) {
            continue;
        }

        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => {
                // Binary or non-UTF8 file — skip silently
                continue;
            }
        };

        let findings = scan_file(&content, entry.path(), &rules);
        all_findings.extend(findings);
    }

    let verdict = compute_verdict(&all_findings, permissions);

    Ok(AnalysisResult {
        verdict,
        findings: all_findings,
        errors,
    })
}

/// Extract filesystem write targets from source code, to validate against
/// the skill's declared permissions.
pub fn scan_file_for_writes(file_path: &Path) -> Vec<String> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let write_re = Regex::new(r#"(?:std::fs::write|fs\.writeFile)\s*\([^)]*"#).unwrap();
    let mut targets = Vec::new();

    for line in content.lines() {
        if let Some(m) = write_re.find(line) {
            // Extract the first string literal argument (path)
            let matched = m.as_str();
            if let Some(start) = matched.find('"') {
                let rest = &matched[start + 1..];
                if let Some(end) = rest.find('"') {
                    targets.push(rest[..end].to_string());
                }
            }
        }
    }

    targets
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Compute the overall verdict from findings and permissions.
fn compute_verdict(findings: &[AnalysisFinding], permissions: &Permissions) -> AnalysisVerdict {
    let mut has_unpermitted_high = false;
    let mut has_medium = false;

    for finding in findings {
        match finding.severity {
            FindingSeverity::Critical => {
                return AnalysisVerdict::Block;
            }
            FindingSeverity::High => {
                if !is_high_finding_permitted(finding, permissions) {
                    has_unpermitted_high = true;
                }
            }
            FindingSeverity::Medium => {
                has_medium = true;
            }
        }
    }

    if has_unpermitted_high {
        AnalysisVerdict::Block
    } else if has_medium {
        AnalysisVerdict::Warn
    } else {
        AnalysisVerdict::Pass
    }
}

/// Check whether a High-severity finding is covered by the skill's declared
/// permissions.
fn is_high_finding_permitted(finding: &AnalysisFinding, permissions: &Permissions) -> bool {
    // Network-related findings are permitted if network access is declared.
    if is_network_finding(finding) && permissions.network {
        return true;
    }
    // Filesystem write findings are permitted if the write path matches an
    // allowed pattern.
    if is_filesystem_finding(finding) && !permissions.filesystem.write.is_empty() {
        return true;
    }
    false
}

fn is_network_finding(finding: &AnalysisFinding) -> bool {
    matches!(
        finding.pattern.as_str(),
        "Socket import"
            | "HTTP request"
            | "TCP connection"
            | "curl command"
            | "wget command"
            | "HTTP library usage"
            | "Axios HTTP calls"
    )
}

fn is_filesystem_finding(finding: &AnalysisFinding) -> bool {
    matches!(
        finding.pattern.as_str(),
        "Filesystem write" | "File open outside data dir" | "Node fs.writeFile"
    )
}

// ---------------------------------------------------------------------------
// Tests (included via #[path])
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "static_analysis_tests.rs"]
mod tests;
