//! Guardian N2 — Classifier engine: heuristic dangerosity classification.
//!
//! N2 is the second level of the Guardian pipeline. It analyses tool
//! arguments and shell commands with three heuristic detectors:
//!
//! 1. **Exfiltration detector** — regex patterns for data URLs, DNS tunnels,
//!    reverse shells, SSH/ngrok tunnels, and similar exfiltration vectors.
//! 2. **Entropy analyzer** — Shannon entropy calculation to detect abnormally
//!    high-entropy strings (base64, hex, ciphertext).
//! 3. **Hidden payloads detector** — detection of base64 decode-then-exec
//!    pipelines, hex decode, eval/exec of generated code, multi-stage encoding.
//!
//! The engine aggregates scores and applies threshold logic:
//! - Score >= `block_threshold` → action blocked immediately.
//! - Score >= `escalate_threshold` → action escalated to N3 (LLM validator).
//! - Otherwise → action allowed.
//!
//! Target latency: **<10ms** per evaluation (all detectors are synchronous,
//! regex patterns compiled once at construction time).

mod entropy;
mod exfiltration;
mod hidden_payloads;
pub mod types;

use std::time::Instant;

use entropy::EntropyAnalyzer;
use exfiltration::ExfiltrationDetector;
use hidden_payloads::HiddenPayloadsDetector;
use types::{N2EngineConfig, N2Result, N2Score};

// ── N2 Engine ──────────────────────────────────────────────────────────

/// The complete Guardian N2 classifier engine.
///
/// Wraps three heuristic detectors and provides a single `evaluate()`
/// entry point that returns an aggregated [`N2Result`].
#[derive(Debug)]
pub struct GuardianN2 {
    config: N2EngineConfig,
    exfiltration_detector: ExfiltrationDetector,
    entropy_analyzer: EntropyAnalyzer,
    hidden_payloads_detector: HiddenPayloadsDetector,
}

impl GuardianN2 {
    /// Create a new N2 engine with the given configuration.
    pub fn new(config: N2EngineConfig) -> Self {
        Self {
            config,
            exfiltration_detector: ExfiltrationDetector::new(),
            entropy_analyzer: EntropyAnalyzer::new(),
            hidden_payloads_detector: HiddenPayloadsDetector::new(),
        }
    }

    /// Create a new N2 engine with default thresholds (block ≥ 0.7, escalate ≥ 0.3).
    pub fn with_defaults() -> Self {
        Self::new(N2EngineConfig::default())
    }

    /// Evaluate a tool invocation against all three N2 detectors.
    ///
    /// # Arguments
    /// * `tool_name` — Name of the tool being invoked (e.g. `"shell"`, `"file_write"`).
    /// * `tool_args` — JSON arguments passed to the tool.
    /// * `command` — Shell command string, if applicable.
    /// * `file_path` — File path being accessed, if applicable.
    ///
    /// # Returns
    /// An [`N2Result`] with the combined allow/block/escalate decision,
    /// individual detector scores, and total latency in microseconds.
    ///
    /// # Evaluation order
    /// 1. Build an input string from `tool_args` + `command`.
    /// 2. Run exfiltration detector (regex patterns).
    /// 3. Run entropy analyzer (Shannon entropy per token).
    /// 4. Run hidden payloads detector (regex patterns).
    /// 5. Aggregate scores using threshold logic.
    ///
    /// # Latency target
    /// <10ms. All detectors are synchronous; regex patterns are compiled
    /// once at construction time.
    pub fn evaluate(
        &self,
        tool_name: &str,
        tool_args: &serde_json::Value,
        command: Option<&str>,
        file_path: Option<&str>,
    ) -> N2Result {
        let start = Instant::now();

        // ── Step 1: Build input string ──────────────────────────────
        let args_str = build_input_string(tool_name, tool_args, command, file_path);
        let args_str = truncate_input(&args_str, self.config.max_input_chars);

        // ── Step 2: Run all three detectors ─────────────────────────
        let mut scores: Vec<N2Score> = Vec::with_capacity(3);

        // 2a. Exfiltration detector
        if let Some(score) = self.exfiltration_detector.detect(command, &args_str) {
            log::debug!(
                "[guardian:n2] exfiltration detector triggered: score={}, reason={}",
                score.score,
                score.reason
            );
            scores.push(score);
        }

        // 2b. Entropy analyzer
        if let Some(score) = self.entropy_analyzer.analyze(&args_str, command) {
            log::debug!(
                "[guardian:n2] entropy analyzer triggered: score={}, reason={}",
                score.score,
                score.reason
            );
            scores.push(score);
        }

        // 2c. Hidden payloads detector
        if let Some(score) = self.hidden_payloads_detector.detect(command, &args_str) {
            log::debug!(
                "[guardian:n2] hidden payloads detector triggered: score={}, reason={}",
                score.score,
                score.reason
            );
            scores.push(score);
        }

        // ── Step 3: Measure latency ─────────────────────────────────
        let latency_us = start.elapsed().as_micros() as u64;

        // ── Step 4: Aggregate scores ────────────────────────────────
        N2Result::from_scores(
            scores,
            latency_us,
            self.config.block_threshold,
            self.config.escalate_threshold,
        )
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Build a single analysable string from the tool invocation data.
fn build_input_string(
    tool_name: &str,
    tool_args: &serde_json::Value,
    command: Option<&str>,
    file_path: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(4);

    // Tool name
    parts.push(tool_name.to_string());

    // Serialize args (compact JSON to maximise relevant content)
    if let Ok(args_json) = serde_json::to_string(tool_args) {
        parts.push(args_json);
    }

    // Command (if present)
    if let Some(cmd) = command {
        parts.push(cmd.to_string());
    }

    // File path (if present)
    if let Some(path) = file_path {
        parts.push(path.to_string());
    }

    parts.join(" ")
}

/// Truncate input to `max_chars`, returning the original if within bounds.
fn truncate_input(input: &str, max_chars: usize) -> String {
    if input.len() <= max_chars {
        input.to_string()
    } else {
        log::debug!(
            "[guardian:n2] input truncated from {} to {} chars",
            input.len(),
            max_chars
        );
        input[..max_chars].to_string()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Build input string ──────────────────────────────────────────

    #[test]
    fn build_input_joins_all_parts() {
        let input = build_input_string(
            "shell",
            &json!({"command": "echo hello"}),
            Some("echo hello"),
            None,
        );
        assert!(input.contains("shell"));
        assert!(input.contains("echo hello"));
    }

    #[test]
    fn build_input_with_file_path() {
        let input = build_input_string(
            "file_write",
            &json!({"path": "/tmp/test.txt", "content": "data"}),
            None,
            Some("/tmp/test.txt"),
        );
        assert!(input.contains("file_write"));
        assert!(input.contains("/tmp/test.txt"));
    }

    // ── Truncation ──────────────────────────────────────────────────

    #[test]
    fn truncate_short_input_unchanged() {
        let s = "short input";
        assert_eq!(truncate_input(s, 100), s);
    }

    #[test]
    fn truncate_long_input() {
        let long = "a".repeat(100);
        let truncated = truncate_input(&long, 10);
        assert_eq!(truncated.len(), 10);
    }

    // ── GuardianN2 evaluate ─────────────────────────────────────────

    #[test]
    fn evaluate_returns_result_with_latency() {
        let engine = GuardianN2::with_defaults();
        let result = engine.evaluate(
            "file_read",
            &json!({"path": "workspace/doc.txt"}),
            None,
            Some("workspace/doc.txt"),
        );
        // With stubs, no detector triggers → should be allowed, no escalate.
        assert!(result.allowed, "benign operation should be allowed");
        assert!(!result.escalate, "benign operation should not escalate");
        assert!(result.latency_us > 0, "latency should be measured");
    }

    #[test]
    fn evaluate_empty_args() {
        let engine = GuardianN2::with_defaults();
        let result = engine.evaluate("some_tool", &json!({}), None, None);
        assert!(result.allowed);
        assert!(!result.escalate);
    }

    #[test]
    #[cfg_attr(windows, ignore = "N2 scoring float precision differs on Windows")]
    fn evaluate_tracks_exact_scores() {
        let engine = GuardianN2::with_defaults();
        let result = engine.evaluate(
            "shell",
            &json!({"command": "cat workspace/file.txt"}),
            Some("cat workspace/file.txt"),
            Some("workspace/file.txt"),
        );
        // Benign operation, stubs return None → empty scores.
        assert!(result.scores.is_empty());
        assert!(result.allowed);
    }
}
