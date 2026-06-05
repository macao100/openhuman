//! Core types for the Guardian N3 LLM validator.
//!
//! Defines the verdict enum, result struct, and configuration used by N3
//! to evaluate ambiguous tool actions via the local LLM.

use serde::{Deserialize, Serialize};

/// Verdict returned by the N3 LLM validator.
///
/// Serializes to lowercase JSON for easy LLM output parsing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum N3Verdict {
    /// Action is legitimate — allow execution.
    Allow,
    /// Action is clearly malicious — block execution.
    Block,
    /// Cannot determine safely — block execution (fail-closed).
    Uncertain,
}

/// Result of a full N3 validation cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct N3Result {
    /// The LLM's verdict on the action.
    pub verdict: N3Verdict,
    /// Human-readable explanation of the verdict.
    pub reason: String,
    /// Total validation latency in microseconds.
    pub latency_us: u64,
    /// Whether this result was served from the LRU cache.
    pub cached: bool,
    /// Which model was used for the validation.
    pub model_used: String,
}

impl N3Result {
    /// Fail-closed: `Block` and `Uncertain` both cause the action to be
    /// blocked. Only `Allow` lets the action through.
    pub fn should_block(&self) -> bool {
        matches!(self.verdict, N3Verdict::Block | N3Verdict::Uncertain)
    }

    /// Parse an LLM JSON response into an `N3Result`.
    ///
    /// The LLM is expected to return a JSON object like:
    /// ```json
    /// {"verdict": "allow", "reason": "Action is safe"}
    /// ```
    ///
    /// This method is lenient — it searches for the first `{` and last `}`
    /// in the response, so it handles markdown-wrapped or extra-text responses.
    /// Returns `None` if no valid JSON object can be extracted.
    pub fn from_llm_response(response: &str) -> Option<Self> {
        let start = response.find('{')?;
        let end = response.rfind('}')?;
        if end <= start {
            return None;
        }
        let json_str = &response[start..=end];
        let parsed: LlmResponse = serde_json::from_str(json_str).ok()?;
        Some(N3Result {
            verdict: parsed.verdict,
            reason: parsed.reason,
            latency_us: 0,
            cached: false,
            model_used: String::new(),
        })
    }
}

/// Expected response format from the LLM.
#[derive(Debug, Deserialize)]
struct LlmResponse {
    verdict: N3Verdict,
    #[serde(default)]
    reason: String,
}

/// Configuration for the N3 LLM validator.
#[derive(Debug, Clone)]
pub struct N3Config {
    /// Whether N3 validation is enabled.
    pub enabled: bool,
    /// Maximum tokens for LLM response (default: 256).
    pub max_tokens: u32,
    /// Timeout in milliseconds (default: 450, target <500ms).
    pub timeout_ms: u64,
    /// Maximum LRU cache entries (default: 100).
    pub cache_size: usize,
    /// Optional model override — uses default model when None.
    pub model_override: Option<String>,
}

impl Default for N3Config {
    fn default() -> Self {
        Self {
            enabled: true,
            max_tokens: 256,
            timeout_ms: 450,
            cache_size: 100,
            model_override: None,
        }
    }
}

impl From<crate::openhuman::config::schema::types::GuardianN3Config> for N3Config {
    fn from(cfg: crate::openhuman::config::schema::types::GuardianN3Config) -> Self {
        Self {
            enabled: cfg.enabled,
            max_tokens: cfg.max_tokens,
            timeout_ms: cfg.timeout_ms,
            cache_size: cfg.cache_size,
            model_override: cfg.model_override,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // N3Verdict serialisation / deserialisation
    // -----------------------------------------------------------------------

    #[test]
    fn n3_verdict_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&N3Verdict::Allow).unwrap(), "\"allow\"");
        assert_eq!(serde_json::to_string(&N3Verdict::Block).unwrap(), "\"block\"");
        assert_eq!(
            serde_json::to_string(&N3Verdict::Uncertain).unwrap(),
            "\"uncertain\""
        );
    }

    #[test]
    fn n3_verdict_deserializes_lowercase() {
        assert_eq!(
            serde_json::from_str::<N3Verdict>("\"allow\"").unwrap(),
            N3Verdict::Allow
        );
        assert_eq!(
            serde_json::from_str::<N3Verdict>("\"block\"").unwrap(),
            N3Verdict::Block
        );
        assert_eq!(
            serde_json::from_str::<N3Verdict>("\"uncertain\"").unwrap(),
            N3Verdict::Uncertain
        );
    }

    #[test]
    fn n3_verdict_serde_roundtrip() {
        for verdict in &[N3Verdict::Allow, N3Verdict::Block, N3Verdict::Uncertain] {
            let json = serde_json::to_string(verdict).unwrap();
            let parsed: N3Verdict = serde_json::from_str(&json).unwrap();
            assert_eq!(*verdict, parsed);
        }
    }

    // -----------------------------------------------------------------------
    // N3Result should_block behaviour (fail-closed semantics)
    // -----------------------------------------------------------------------

    #[test]
    fn n3_result_allow_does_not_block() {
        let result = N3Result {
            verdict: N3Verdict::Allow,
            reason: "safe operation".into(),
            latency_us: 100,
            cached: false,
            model_used: "test".into(),
        };
        assert!(!result.should_block());
    }

    #[test]
    fn n3_result_block_should_block() {
        let result = N3Result {
            verdict: N3Verdict::Block,
            reason: "malicious pattern detected".into(),
            latency_us: 100,
            cached: false,
            model_used: "test".into(),
        };
        assert!(result.should_block());
    }

    #[test]
    fn n3_result_uncertain_should_block() {
        let result = N3Result {
            verdict: N3Verdict::Uncertain,
            reason: "ambiguous action".into(),
            latency_us: 100,
            cached: false,
            model_used: "test".into(),
        };
        assert!(
            result.should_block(),
            "Uncertain should block (fail-closed)"
        );
    }

    // -----------------------------------------------------------------------
    // N3Config defaults
    // -----------------------------------------------------------------------

    #[test]
    fn n3_config_defaults_are_reasonable() {
        let config = N3Config::default();
        assert!(config.enabled);
        assert_eq!(config.max_tokens, 256);
        assert_eq!(config.timeout_ms, 450);
        assert_eq!(config.cache_size, 100);
        assert!(config.model_override.is_none());
    }

    #[test]
    fn n3_config_custom_values() {
        let config = N3Config {
            enabled: false,
            max_tokens: 128,
            timeout_ms: 500,
            cache_size: 50,
            model_override: Some("llama3.2:3b".into()),
        };
        assert!(!config.enabled);
        assert_eq!(config.max_tokens, 128);
        assert_eq!(config.timeout_ms, 500);
        assert_eq!(config.cache_size, 50);
        assert_eq!(
            config.model_override,
            Some("llama3.2:3b".to_string())
        );
    }

    // -----------------------------------------------------------------------
    // LLM response parsing
    // -----------------------------------------------------------------------

    #[test]
    fn from_llm_response_parses_valid_json() {
        let response = r#"{"verdict": "allow", "reason": "This action is safe"}"#;
        let result = N3Result::from_llm_response(response);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.verdict, N3Verdict::Allow);
        assert_eq!(result.reason, "This action is safe");
    }

    #[test]
    fn from_llm_response_parses_block_verdict() {
        let response = r#"{"verdict": "block", "reason": "Suspicious pattern detected"}"#;
        let result = N3Result::from_llm_response(response);
        assert!(result.is_some());
        assert_eq!(result.unwrap().verdict, N3Verdict::Block);
    }

    #[test]
    fn from_llm_response_parses_uncertain_verdict() {
        let response = r#"{"verdict": "uncertain", "reason": "Cannot determine safety"}"#;
        let result = N3Result::from_llm_response(response);
        assert!(result.is_some());
        assert_eq!(result.unwrap().verdict, N3Verdict::Uncertain);
    }

    #[test]
    fn from_llm_response_handles_malformed_json() {
        let result = N3Result::from_llm_response("This is not JSON at all");
        assert!(result.is_none(), "malformed JSON should return None");
    }

    #[test]
    fn from_llm_response_handles_empty_string() {
        let result = N3Result::from_llm_response("");
        assert!(result.is_none());
    }

    #[test]
    fn from_llm_response_extracts_json_from_markdown() {
        let response = "Here is my analysis:\n\n```json\n{\"verdict\": \"allow\", \"reason\": \"Safe\"}\n```\n";
        let result = N3Result::from_llm_response(response);
        assert!(result.is_some());
        assert_eq!(result.unwrap().verdict, N3Verdict::Allow);
    }

    #[test]
    fn from_llm_response_extracts_json_from_text_surrounding() {
        let response =
            "Some text before {\"verdict\": \"allow\", \"reason\": \"ok\"} and after";
        let result = N3Result::from_llm_response(response);
        assert!(result.is_some());
        assert_eq!(result.unwrap().verdict, N3Verdict::Allow);
    }

    #[test]
    fn from_llm_response_empty_reason_default() {
        let response = r#"{"verdict": "uncertain"}"#;
        let result = N3Result::from_llm_response(response).unwrap();
        assert_eq!(result.verdict, N3Verdict::Uncertain);
        assert!(result.reason.is_empty());
    }

    #[test]
    fn from_llm_response_handles_confidence_field() {
        // The LLM may include extra fields like "confidence" — we should
        // gracefully ignore them since they're not in LlmResponse.
        let response = r#"{"verdict": "block", "reason": "Bad", "confidence": "high"}"#;
        let result = N3Result::from_llm_response(response);
        assert!(result.is_some());
        assert_eq!(result.unwrap().verdict, N3Verdict::Block);
    }

    #[test]
    fn from_llm_response_empty_object_no_reason() {
        // Empty verdict should fail to parse (not a valid variant).
        let response = r#"{}"#;
        let result = N3Result::from_llm_response(response);
        assert!(result.is_none());
    }
}
