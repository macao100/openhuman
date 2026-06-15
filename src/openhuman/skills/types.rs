//! Shared tool result types used by the tool and node runtime surfaces.

use serde::{Deserialize, Serialize};

/// Result of executing a tool, containing content blocks and error status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// List of content blocks returned by the tool.
    pub content: Vec<ToolContent>,
    /// Indicates if the tool encountered an error during execution.
    #[serde(default)]
    pub is_error: bool,
    /// Optional markdown rendering of the result. When the agent loop
    /// is configured with `prefer_markdown`, this is sent to the LLM
    /// instead of the JSON-serialised content blocks. Mirrors the
    /// `markdownFormatted` field on Composio's backend responses
    /// (see #1165) — markdown is significantly cheaper than JSON in
    /// the model context window.
    #[serde(
        default,
        rename = "markdownFormatted",
        skip_serializing_if = "Option::is_none"
    )]
    pub markdown_formatted: Option<String>,
}

impl ToolResult {
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text { text: text.into() }],
            is_error: false,
            markdown_formatted: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text {
                text: message.into(),
            }],
            is_error: true,
            markdown_formatted: None,
        }
    }

    pub fn json(data: serde_json::Value) -> Self {
        Self {
            content: vec![ToolContent::Json { data }],
            is_error: false,
            markdown_formatted: None,
        }
    }

    /// Construct a successful result that carries both a JSON payload
    /// (for programmatic consumers / debugging) and a markdown rendering
    /// (preferred by the agent loop when `prefer_markdown` is on).
    pub fn success_with_markdown(data: serde_json::Value, markdown: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Json { data }],
            is_error: false,
            markdown_formatted: Some(markdown.into()),
        }
    }

    /// Attach (or replace) the markdown rendering on an existing result.
    pub fn with_markdown(mut self, markdown: impl Into<String>) -> Self {
        self.markdown_formatted = Some(markdown.into());
        self
    }

    /// Returns the markdown rendering when present and non-empty,
    /// otherwise falls back to [`Self::output`]. Used by the agent loop
    /// when token-saving markdown output is requested.
    pub fn output_for_llm(&self, prefer_markdown: bool) -> String {
        if prefer_markdown {
            if let Some(md) = self.markdown_formatted.as_deref() {
                let trimmed = md.trim();
                if !trimmed.is_empty() {
                    return md.to_string();
                }
            }
        }
        self.output()
    }

    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                ToolContent::Text { text } => Some(text.as_str()),
                ToolContent::Json { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn output(&self) -> String {
        self.content
            .iter()
            .map(|c| match c {
                ToolContent::Text { text } => text.clone(),
                ToolContent::Json { data } => {
                    serde_json::to_string_pretty(data).unwrap_or_default()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A single content block within a `ToolResult`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolContent {
    Text { text: String },
    Json { data: serde_json::Value },
}

// ---------------------------------------------------------------------------
// Structured output envelope for WASM skill results (INJ-02)
// ---------------------------------------------------------------------------

/// Execution status of a WASM skill call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// The skill completed successfully.
    Success,
    /// The skill returned an error.
    Error,
    /// The skill exceeded its execution time limit.
    Timeout,
}

/// Structured JSON envelope for WASM skill outputs (INJ-02).
///
/// Wraps raw skill execution results in a typed, serializable envelope so
/// the LLM receives structured data (`data` field) rather than raw text
/// that could contain injection payloads. Metadata fields (`skill_version`,
/// `execution_time_ms`, `gpg_verified`) enable trust decisions in the
/// tool loop without exposing raw output to the LLM.
///
/// # LLM-facing presentation
///
/// Only the `data` field is rendered to the LLM, wrapped in an
/// `<external_data>` tag. The envelope metadata is consumed by the agent
/// harness for policy/trust gating.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOutputEnvelope {
    /// Name of the skill that produced this output (from manifest).
    pub skill_name: String,
    /// Version of the skill that produced this output.
    pub skill_version: String,
    /// Execution status (Success, Error, or Timeout).
    pub execution_status: ExecutionStatus,
    /// Description of the output format (e.g. "application/json", "text/plain").
    #[serde(default = "default_output_schema")]
    pub output_schema: String,
    /// Structured data payload — never raw text. Contains the skill's
    /// actual output under keys like `output`, `output_bytes`, etc.
    pub data: serde_json::Value,
    /// Error message when execution_status is Error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Wall-clock execution time in milliseconds.
    pub execution_time_ms: u64,
    /// Whether the GPG signature was verified (Phase 4).
    pub gpg_verified: bool,
}

fn default_output_schema() -> String {
    "text/plain".to_string()
}

impl SkillOutputEnvelope {
    /// Create a success envelope from structured data.
    ///
    /// The `data` field is presented to the LLM inside `<external_data>`.
    pub fn new_success(
        skill_name: impl Into<String>,
        skill_version: impl Into<String>,
        data: serde_json::Value,
        execution_time_ms: u64,
        gpg_verified: bool,
    ) -> Self {
        Self {
            skill_name: skill_name.into(),
            skill_version: skill_version.into(),
            execution_status: ExecutionStatus::Success,
            output_schema: default_output_schema(),
            data,
            error: None,
            execution_time_ms,
            gpg_verified,
        }
    }

    /// Create an error envelope.
    pub fn new_error(
        skill_name: impl Into<String>,
        skill_version: impl Into<String>,
        error_msg: impl Into<String>,
        execution_time_ms: u64,
        gpg_verified: bool,
    ) -> Self {
        Self {
            skill_name: skill_name.into(),
            skill_version: skill_version.into(),
            execution_status: ExecutionStatus::Error,
            output_schema: default_output_schema(),
            data: serde_json::Value::Null,
            error: Some(error_msg.into()),
            execution_time_ms,
            gpg_verified,
        }
    }

    /// Create a timeout envelope.
    pub fn new_timeout(
        skill_name: impl Into<String>,
        skill_version: impl Into<String>,
        execution_time_ms: u64,
        gpg_verified: bool,
    ) -> Self {
        Self {
            skill_name: skill_name.into(),
            skill_version: skill_version.into(),
            execution_status: ExecutionStatus::Timeout,
            output_schema: default_output_schema(),
            data: serde_json::Value::Null,
            error: Some("skill execution timed out".to_string()),
            execution_time_ms,
            gpg_verified,
        }
    }

    /// Render the envelope as a single compact JSON line.
    ///
    /// Suitable for embedding in tool result blocks and context tags.
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!(
                "{{\"error\":\"serialization failed\",\"skill_name\":\"{}\"}}",
                self.skill_name
            )
        })
    }

    /// Return the `data` field formatted as a compact JSON line.
    ///
    /// This is what the LLM sees — structured data, never raw text.
    /// Returns `data` as a JSON string, or `"null"` when data is Null.
    pub fn data_json_line(&self) -> String {
        serde_json::to_string(&self.data)
            .unwrap_or_else(|_| "{{\"error\":\"data serialization failed\"}}".to_string())
    }

    /// Extract the envelope metadata-section (everything except `data`)
    /// for trust/audit decisions in the agent harness.
    pub fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "skill_name": self.skill_name,
            "skill_version": self.skill_version,
            "execution_status": self.execution_status,
            "output_schema": self.output_schema,
            "error": self.error,
            "execution_time_ms": self.execution_time_ms,
            "gpg_verified": self.gpg_verified,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_result_success() {
        let r = ToolResult::success("done");
        assert!(!r.is_error);
        assert_eq!(r.text(), "done");
        assert_eq!(r.output(), "done");
    }

    #[test]
    fn tool_result_error() {
        let r = ToolResult::error("failed");
        assert!(r.is_error);
        assert_eq!(r.text(), "failed");
    }

    #[test]
    fn tool_result_json() {
        let r = ToolResult::json(json!({"key": "value"}));
        assert!(!r.is_error);
        assert!(r.text().is_empty()); // text() skips JSON blocks
        assert!(r.output().contains("key"));
    }

    #[test]
    fn tool_result_mixed_content() {
        let r = ToolResult {
            content: vec![
                ToolContent::Text {
                    text: "line1".into(),
                },
                ToolContent::Json {
                    data: json!({"a": 1}),
                },
                ToolContent::Text {
                    text: "line2".into(),
                },
            ],
            is_error: false,
            markdown_formatted: None,
        };
        assert_eq!(r.text(), "line1\nline2");
        let output = r.output();
        assert!(output.contains("line1"));
        assert!(output.contains("line2"));
        assert!(output.contains("\"a\""));
    }

    #[test]
    fn tool_result_serde_roundtrip() {
        let r = ToolResult::success("hello");
        let json = serde_json::to_string(&r).unwrap();
        let back: ToolResult = serde_json::from_str(&json).unwrap();
        assert!(!back.is_error);
        assert_eq!(back.text(), "hello");
    }

    #[test]
    fn tool_content_text_serde() {
        let c = ToolContent::Text {
            text: "test".into(),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        let back: ToolContent = serde_json::from_str(&json).unwrap();
        match back {
            ToolContent::Text { text } => assert_eq!(text, "test"),
            _ => panic!("expected Text variant"),
        }
    }

    #[test]
    fn tool_content_json_serde() {
        let c = ToolContent::Json {
            data: json!({"x": 1}),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"type\":\"json\""));
        let back: ToolContent = serde_json::from_str(&json).unwrap();
        match back {
            ToolContent::Json { data } => assert_eq!(data["x"], 1),
            _ => panic!("expected Json variant"),
        }
    }

    #[test]
    fn tool_result_empty_content() {
        let r = ToolResult {
            content: vec![],
            is_error: false,
            markdown_formatted: None,
        };
        assert!(r.text().is_empty());
        assert!(r.output().is_empty());
    }

    #[test]
    fn output_for_llm_prefers_markdown_when_requested() {
        let r =
            ToolResult::success_with_markdown(json!({"items": [{"id": 1}, {"id": 2}]}), "- 1\n- 2");
        assert_eq!(r.output_for_llm(true), "- 1\n- 2");
        // When prefer_markdown is false, falls back to JSON pretty-print.
        let raw = r.output_for_llm(false);
        assert!(raw.contains("\"items\""));
    }

    #[test]
    fn output_for_llm_falls_back_to_output_when_markdown_missing() {
        let r = ToolResult::success("plain");
        assert_eq!(r.output_for_llm(true), "plain");
        assert_eq!(r.output_for_llm(false), "plain");
    }

    #[test]
    fn output_for_llm_falls_back_when_markdown_blank() {
        let r = ToolResult::success("plain").with_markdown("   \n  ");
        assert_eq!(r.output_for_llm(true), "plain");
    }

    #[test]
    fn markdown_field_serde_roundtrip() {
        let r = ToolResult::success_with_markdown(json!({"a": 1}), "**a**: 1");
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("markdownFormatted"));
        let back: ToolResult = serde_json::from_str(&s).unwrap();
        assert_eq!(back.markdown_formatted.as_deref(), Some("**a**: 1"));
    }

    // ── SkillOutputEnvelope tests (INJ-02) ────────────────────────────

    #[test]
    fn skill_output_envelope_new_success_has_correct_fields() {
        let data = json!({"output": "hello", "output_bytes": 5});
        let envelope =
            SkillOutputEnvelope::new_success("test-skill", "1.0.0", data.clone(), 42, false);

        assert_eq!(envelope.skill_name, "test-skill");
        assert_eq!(envelope.skill_version, "1.0.0");
        assert_eq!(envelope.execution_status, ExecutionStatus::Success);
        assert_eq!(envelope.execution_time_ms, 42);
        assert!(!envelope.gpg_verified);
        assert!(envelope.error.is_none());
        assert_eq!(envelope.data, data);
    }

    #[test]
    fn skill_output_envelope_new_error_has_correct_fields() {
        let envelope =
            SkillOutputEnvelope::new_error("test-skill", "1.0.0", "something broke", 100, true);

        assert_eq!(envelope.skill_name, "test-skill");
        assert_eq!(envelope.execution_status, ExecutionStatus::Error);
        assert!(envelope.gpg_verified);
        assert_eq!(envelope.error.as_deref(), Some("something broke"));
        assert_eq!(envelope.execution_time_ms, 100);
        assert_eq!(envelope.data, serde_json::Value::Null);
    }

    #[test]
    fn skill_output_envelope_new_timeout_has_correct_fields() {
        let envelope = SkillOutputEnvelope::new_timeout("slow-skill", "0.5.0", 30_000, false);

        assert_eq!(envelope.skill_name, "slow-skill");
        assert_eq!(envelope.skill_version, "0.5.0");
        assert_eq!(envelope.execution_status, ExecutionStatus::Timeout);
        assert!(!envelope.gpg_verified);
        assert_eq!(envelope.execution_time_ms, 30_000);
        assert!(envelope.error.as_deref().unwrap().contains("timed out"));
    }

    #[test]
    fn skill_output_envelope_json_round_trip() {
        let data = json!({"output": "hello world"});
        let envelope = SkillOutputEnvelope::new_success("test", "2.0.0", data, 500, true);

        let json = serde_json::to_string(&envelope).unwrap();
        let deserialized: SkillOutputEnvelope = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.skill_name, "test");
        assert_eq!(deserialized.skill_version, "2.0.0");
        assert_eq!(deserialized.execution_status, ExecutionStatus::Success);
        assert_eq!(deserialized.execution_time_ms, 500);
        assert!(deserialized.gpg_verified);
        assert!(deserialized.error.is_none());
    }

    #[test]
    fn skill_output_envelope_to_json_line_produces_valid_json() {
        let data = json!({"output": "test"});
        let envelope = SkillOutputEnvelope::new_success("s", "1.0.0", data, 10, false);

        let line = envelope.to_json_line();
        let reparsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reparsed["skill_name"], "s");
        assert_eq!(reparsed["skill_version"], "1.0.0");
        assert_eq!(reparsed["execution_status"], "success");
        assert_eq!(reparsed["data"]["output"], "test");
    }

    #[test]
    fn skill_output_envelope_data_json_line_returns_only_data() {
        let data = json!({"output": "secret content", "bytes": 14});
        let envelope = SkillOutputEnvelope::new_success("s", "1.0.0", data, 10, true);

        let data_line = envelope.data_json_line();
        let parsed: serde_json::Value = serde_json::from_str(&data_line).unwrap();
        assert_eq!(parsed["output"], "secret content");
        assert_eq!(parsed["bytes"], 14);
    }

    #[test]
    fn skill_output_envelope_metadata_omits_data_field() {
        let data = json!({"output": "should not appear"});
        let envelope = SkillOutputEnvelope::new_success("s", "1.0.0", data, 10, true);

        let meta = envelope.metadata();
        assert_eq!(meta["skill_name"], "s");
        assert_eq!(meta["skill_version"], "1.0.0");
        assert_eq!(meta["execution_status"], "success");
        assert_eq!(meta["gpg_verified"], true);
        // The `data` field should NOT be in metadata
        assert!(meta.get("data").is_none());
    }

    #[test]
    fn skill_output_envelope_constructors_set_default_output_schema() {
        let s = SkillOutputEnvelope::new_success("s", "1", json!(null), 0, false);
        assert_eq!(s.output_schema, "text/plain");

        let e = SkillOutputEnvelope::new_error("s", "1", "err", 0, false);
        assert_eq!(e.output_schema, "text/plain");

        let t = SkillOutputEnvelope::new_timeout("s", "1", 0, false);
        assert_eq!(t.output_schema, "text/plain");
    }

    #[test]
    fn skill_output_envelope_serde_omits_error_when_none() {
        let envelope = SkillOutputEnvelope::new_success("s", "1", json!({"ok": true}), 0, false);
        let json = serde_json::to_string(&envelope).unwrap();
        // Should NOT contain "error" key when error is None
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("error").is_none() || parsed["error"].is_null());
    }
}
