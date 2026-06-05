//! Core types for the Guardian N2 dangerosity classifier engine.
//!
//! N2 extends the Guardian pipeline with heuristic-based detection of
//! exfiltration patterns, entropy anomalies, and hidden payloads in
//! tool arguments and shell commands.
//!
//! Each detector returns an [`N2Score`]; the engine aggregates all scores
//! and combines them with threshold logic to produce an [`N2Result`].

// ── Threshold constants ────────────────────────────────────────────────

/// Default threshold beyond which an action is **blocked** immediately.
/// Corresponds to high-confidence malicious patterns (e.g. reverse shell).
pub const BLOCK_THRESHOLD: f64 = 0.7;

/// Default threshold beyond which an action is **escalated** to N3.
/// Corresponds to suspicious patterns that need LLM validation (e.g. DNS
/// exfiltration, base64 decode of non-image data).
pub const ESCALATE_THRESHOLD: f64 = 0.3;

/// Maximum number of input characters analysed by any N2 detector.
/// Inputs exceeding this limit are silently truncated to prevent DoS (T-03-03).
pub const MAX_INPUT_CHARS: usize = 10_000;

// ── Types ──────────────────────────────────────────────────────────────

/// Score returned by a single N2 detector.
///
/// Each detector evaluates tool arguments / commands and produces a suspicion
/// score between 0.0 (safe) and 1.0 (definitely malicious), along with a
/// human-readable reason and the detector name.
#[derive(Debug, Clone, PartialEq)]
pub struct N2Score {
    /// Suspicion score: 0.0 (safe) through 1.0 (definitely malicious).
    pub score: f64,
    /// Human-readable explanation of why this score was assigned.
    pub reason: String,
    /// Name of the detector that produced this score
    /// (e.g. `"exfiltration"`, `"entropy"`, `"hidden_payloads"`).
    pub triggered_by: String,
}

impl N2Score {
    /// Create a new N2Score.
    pub fn new(score: f64, reason: impl Into<String>, triggered_by: impl Into<String>) -> Self {
        debug_assert!(
            (0.0..=1.0).contains(&score),
            "N2Score must be in [0.0, 1.0], got {score}"
        );
        Self {
            score,
            reason: reason.into(),
            triggered_by: triggered_by.into(),
        }
    }

    /// Returns `true` if this score exceeds or meets the block threshold.
    ///
    /// A blocking score causes the pipeline to reject the action immediately,
    /// without consulting N3.
    pub fn is_blocking(&self, threshold: f64) -> bool {
        self.score >= threshold
    }

    /// Returns `true` if this score is in the *escalation* range:
    /// at or above the given threshold but strictly below `BLOCK_THRESHOLD`.
    ///
    /// An escalating score causes the pipeline to defer to N3 (LLM validator).
    pub fn is_escalating(&self, threshold: f64) -> bool {
        self.score >= threshold && self.score < BLOCK_THRESHOLD
    }
}

/// Aggregated result of the full N2 evaluation pipeline.
///
/// Contains the final allow/block decision, escalation flag, individual
/// detector scores, and total latency.
#[derive(Debug, Clone, PartialEq)]
pub struct N2Result {
    /// Whether the action is allowed (`true`) or blocked (`false`).
    ///
    /// `false` when **any** detector score >= `block_threshold`.
    pub allowed: bool,
    /// Whether the action should be escalated to N3 for LLM validation.
    ///
    /// `true` when no detector blocks but **any** score >= `escalate_threshold`.
    pub escalate: bool,
    /// Individual scores returned by each detector, in evaluation order
    /// (exfiltration, entropy, hidden_payloads).
    pub scores: Vec<N2Score>,
    /// Total N2 pipeline latency in microseconds.
    pub latency_us: u64,
}

impl N2Result {
    /// Create a new N2Result by aggregating scores with the given thresholds.
    ///
    /// This is the canonical constructor used by [`GuardianN2::evaluate`].
    pub fn from_scores(scores: Vec<N2Score>, latency_us: u64, block_threshold: f64, escalate_threshold: f64) -> Self {
        let has_blocking = scores.iter().any(|s| s.is_blocking(block_threshold));
        let has_escalating = scores.iter().any(|s| s.is_escalating(escalate_threshold));

        Self {
            allowed: !has_blocking,
            escalate: !has_blocking && has_escalating,
            scores,
            latency_us,
        }
    }
}

/// Configuration for the N2 engine.
///
/// All fields have sensible defaults; custom thresholds can be injected
/// via [`N2EngineConfig::new`] or from config.toml (planned D-41).
#[derive(Debug, Clone, PartialEq)]
pub struct N2EngineConfig {
    /// Score threshold for immediate blocking (default: 0.7).
    pub block_threshold: f64,
    /// Score threshold for escalation to N3 (default: 0.3).
    pub escalate_threshold: f64,
    /// Maximum number of input characters to analyse (default: 10_000).
    /// Prevents DoS from excessively large tool arguments (T-03-03).
    pub max_input_chars: usize,
}

impl Default for N2EngineConfig {
    fn default() -> Self {
        Self {
            block_threshold: BLOCK_THRESHOLD,
            escalate_threshold: ESCALATE_THRESHOLD,
            max_input_chars: MAX_INPUT_CHARS,
        }
    }
}

impl N2EngineConfig {
    /// Create a new config with custom thresholds.
    pub fn new(block_threshold: f64, escalate_threshold: f64, max_input_chars: usize) -> Self {
        Self {
            block_threshold,
            escalate_threshold,
            max_input_chars,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── N2Score ─────────────────────────────────────────────────────

    #[test]
    fn n2_score_creation() {
        let score = N2Score::new(0.5, "suspicious pattern detected", "exfiltration");
        assert!((score.score - 0.5).abs() < f64::EPSILON);
        assert_eq!(score.reason, "suspicious pattern detected");
        assert_eq!(score.triggered_by, "exfiltration");
    }

    #[test]
    fn n2_score_with_max_score_is_blocking() {
        let score = N2Score::new(1.0, "reverse shell", "exfiltration");
        assert!(score.is_blocking(0.7));
    }

    #[test]
    fn n2_score_escalating_threshold_logic() {
        let score = N2Score::new(0.5, "suspicious", "entropy");
        // 0.5 >= 0.3 && 0.5 < 0.7 → true
        assert!(score.is_escalating(0.3));
        // 0.5 < 0.7 → false
        assert!(!score.is_escalating(0.7));
    }

    #[test]
    fn n2_score_high_score_never_escalating() {
        // Scores above BLOCK_THRESHOLD (0.7) are blocking, not escalating.
        let score = N2Score::new(0.9, "high confidence block", "exfiltration");
        assert!(!score.is_escalating(0.3));
        assert!(score.is_blocking(0.7));
    }

    #[test]
    fn n2_score_low_score_neither_blocking_nor_escalating() {
        let score = N2Score::new(0.1, "benign", "entropy");
        assert!(!score.is_blocking(0.7));
        assert!(!score.is_escalating(0.3));
    }

    // ── N2Result ────────────────────────────────────────────────────

    #[test]
    fn n2_result_blocking_scores_block_action() {
        let scores = vec![
            N2Score::new(0.2, "low", "entropy"),
            N2Score::new(0.9, "exfil detected", "exfiltration"),
        ];
        let result = N2Result::from_scores(scores, 42, 0.7, 0.3);
        assert!(!result.allowed, "should block when any score >= 0.7");
        assert!(!result.escalate, "blocked actions don't escalate");
    }

    #[test]
    fn n2_result_escalating_scores_escalate_to_n3() {
        let scores = vec![
            N2Score::new(0.5, "suspicious entropy", "entropy"),
            N2Score::new(0.2, "benign", "exfiltration"),
        ];
        let result = N2Result::from_scores(scores, 42, 0.7, 0.3);
        assert!(result.allowed, "no blocking score, should be allowed");
        assert!(result.escalate, "should escalate to N3 when score >= 0.3");
    }

    #[test]
    fn n2_result_low_scores_allow_without_escalation() {
        let scores = vec![
            N2Score::new(0.1, "benign", "entropy"),
            N2Score::new(0.2, "benign", "exfiltration"),
        ];
        let result = N2Result::from_scores(scores, 42, 0.7, 0.3);
        assert!(result.allowed, "low scores should be allowed");
        assert!(!result.escalate, "low scores should not escalate");
    }

    #[test]
    fn n2_result_tracks_latency() {
        let scores = vec![N2Score::new(0.1, "benign", "entropy")];
        let result = N2Result::from_scores(scores, 12345, 0.7, 0.3);
        assert_eq!(result.latency_us, 12345);
    }

    // ── N2EngineConfig ──────────────────────────────────────────────

    #[test]
    fn n2_engine_config_defaults() {
        let cfg = N2EngineConfig::default();
        assert!((cfg.block_threshold - 0.7).abs() < f64::EPSILON);
        assert!((cfg.escalate_threshold - 0.3).abs() < f64::EPSILON);
        assert_eq!(cfg.max_input_chars, 10_000);
    }

    #[test]
    fn n2_engine_config_custom_values() {
        let cfg = N2EngineConfig::new(0.8, 0.4, 5_000);
        assert!((cfg.block_threshold - 0.8).abs() < f64::EPSILON);
        assert!((cfg.escalate_threshold - 0.4).abs() < f64::EPSILON);
        assert_eq!(cfg.max_input_chars, 5_000);
    }

    #[test]
    fn n2_result_empty_scores_allows() {
        let result = N2Result::from_scores(vec![], 0, 0.7, 0.3);
        assert!(result.allowed, "no scores = no reason to block");
        assert!(!result.escalate, "no scores = no reason to escalate");
    }
}
