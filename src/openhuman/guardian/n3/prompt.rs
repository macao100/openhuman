//! N3 system prompt and user prompt builder.
//!
//! Constructs the security validation prompt sent to the local LLM when N2
//! escalates an ambiguous action. The prompt asks the LLM to produce a
//! structured JSON verdict: `{"verdict": "allow"|"block"|"uncertain", "reason": "..."}`.

use serde_json::Value;

/// Maximum length of the built user prompt, in characters.
const MAX_USER_PROMPT_CHARS: usize = 2000;

/// System prompt for the N3 security validator.
///
/// This prompt instructs the LLM to act as a security validation assistant
/// and output structured JSON judgements.
const N3_SYSTEM_PROMPT: &str = r#"You are a security validation assistant for the DADOU AI agent.
Your role is to determine whether a proposed action is legitimate or malicious.

Rules:
1. The agent executes tasks the user explicitly asked for — most actions are legitimate.
2. You are called ONLY for ambiguous cases that the deterministic filters (N2) flagged as suspicious.
3. Be conservative: if you are unsure, return "uncertain" (the action will be blocked).
4. Consider the context: is this action consistent with the tool's normal purpose?
5. A single action in isolation may look suspicious but be legitimate in context.

Output ONLY valid JSON (no markdown, no explanations outside JSON):
{
  "verdict": "allow" | "block" | "uncertain",
  "reason": "Brief explanation of your decision",
  "confidence": "high" | "medium" | "low"
}"#;

/// Builder for N3 validation prompts.
pub struct N3PromptBuilder;

impl N3PromptBuilder {
    /// Return the N3 system prompt.
    ///
    /// This prompt is always the same and is prepended to every N3 LLM call.
    pub fn system_prompt() -> &'static str {
        N3_SYSTEM_PROMPT
    }

    /// Build the user-facing prompt with the specific action context.
    ///
    /// The prompt includes the tool name, arguments, command, file path,
    /// and N2 suspicion scores that triggered the escalation. The output
    /// is truncated to [`MAX_USER_PROMPT_CHARS`] characters to avoid
    /// exceeding the LLM context window.
    ///
    /// # Arguments
    ///
    /// * `tool_name` — The name of the tool being invoked.
    /// * `tool_args` — The JSON arguments passed to the tool.
    /// * `command` — Optional shell command string.
    /// * `file_path` — Optional file path being accessed.
    /// * `n2_scores` — N2 suspicion scores (tuples of `(detector_name, score)`).
    pub fn build_user_prompt(
        tool_name: &str,
        tool_args: &Value,
        command: Option<&str>,
        file_path: Option<&str>,
        n2_scores: &[(String, f64)],
    ) -> String {
        let args_pretty = serde_json::to_string_pretty(tool_args)
            .unwrap_or_else(|_| "<unparseable args>".to_string());

        let command_str = command.unwrap_or("<none>");
        let file_path_str = file_path.unwrap_or("<none>");

        let scores_formatted = if n2_scores.is_empty() {
            "  (no N2 scores provided — all checks passed N1)".to_string()
        } else {
            n2_scores
                .iter()
                .map(|(name, score)| format!("  - {}: {:.3}", name, score))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let prompt = format!(
            "Tool: {tool_name}\n\
             Arguments:\n{args_pretty}\n\n\
             Command: {command_str}\n\
             File path: {file_path_str}\n\n\
             N2 Suspicion Scores:\n{scores_formatted}\n\n\
             Is this action legitimate? Respond with JSON."
        );

        // Truncate to max length.
        if prompt.len() > MAX_USER_PROMPT_CHARS {
            let truncated: String = prompt.chars().take(MAX_USER_PROMPT_CHARS).collect();
            return format!(
                "{}...\n[TRUNCATED at {} chars]",
                truncated, MAX_USER_PROMPT_CHARS
            );
        }

        prompt
    }

    /// Generate a deterministic cache key for an action tuple.
    ///
    /// The key is a truncated SHA-256 hash of `{tool_name}|{args_json}|{command}`.
    /// This ensures the same action always produces the same cache key,
    /// regardless of whitespace or formatting differences.
    pub fn cache_key(tool_name: &str, tool_args: &Value, command: Option<&str>) -> String {
        use sha2::{Digest, Sha256};

        let args_str = serde_json::to_string(tool_args).unwrap_or_default();
        let cmd_str = command.unwrap_or("");
        let input = format!("{}|{}|{}", tool_name, args_str, cmd_str);

        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        let result = hasher.finalize();

        // First 16 hex characters = 8 bytes = sufficiently unique for cache.
        hex::encode(&result[..8])
    }

    /// Return the system prompt for plan-level intent validation.
    ///
    /// This prompt asks the LLM to evaluate the overall intent of a
    /// structured plan — not individual steps, but whether the goal
    /// itself is malicious.
    pub fn plan_intent_system_prompt() -> &'static str {
        "You are a security validator. Evaluate whether the INTENT of the following action plan is malicious. \
         Consider: Is the goal trying to bypass security controls? Is it trying to exfiltrate data? \
         Is it trying to escalate privileges beyond what the agent should have?\n\n\
         Respond in JSON: {\"verdict\": \"allow\"|\"block\", \"reason\": \"...\"}"
    }

    /// Build the user prompt for plan-level intent validation.
    ///
    /// Includes the plan's goal and each step's tool, args, and rationale.
    pub fn plan_intent_user_prompt(
        plan: &crate::openhuman::guardian::types::StructuredPlan,
    ) -> String {
        use crate::openhuman::guardian::types::StructuredPlan;
        format!(
            "Plan goal: {}\nSteps:\n{}",
            plan.goal,
            plan.steps
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    format!(
                        "  {}. tool={}, args={}, rationale={}",
                        i + 1,
                        s.tool,
                        s.args,
                        s.rationale
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // System prompt validity
    // -----------------------------------------------------------------------

    #[test]
    fn system_prompt_is_valid() {
        let prompt = N3PromptBuilder::system_prompt();
        assert!(!prompt.is_empty(), "system prompt should not be empty");
        assert!(
            prompt.contains("verdict"),
            "should mention JSON verdict format"
        );
        assert!(
            prompt.contains("allow") && prompt.contains("block") && prompt.contains("uncertain"),
            "should mention all three verdict values"
        );
        assert!(
            prompt.contains("security validation"),
            "should describe the role"
        );
    }

    #[test]
    fn system_prompt_includes_json_format() {
        let prompt = N3PromptBuilder::system_prompt();
        assert!(prompt.contains('{'), "should contain JSON structure");
        assert!(prompt.contains('"'), "should contain JSON quotes");
    }

    // -----------------------------------------------------------------------
    // User prompt formatting
    // -----------------------------------------------------------------------

    #[test]
    fn build_user_prompt_includes_tool_name() {
        let prompt = N3PromptBuilder::build_user_prompt(
            "file_write",
            &json!({"path": "/tmp/test.txt"}),
            None,
            None,
            &[],
        );
        assert!(prompt.contains("file_write"), "should include tool name");
    }

    #[test]
    fn build_user_prompt_includes_args() {
        let prompt = N3PromptBuilder::build_user_prompt(
            "shell",
            &json!({"command": "echo hello"}),
            Some("echo hello"),
            None,
            &[],
        );
        assert!(prompt.contains("echo hello"), "should include args");
    }

    #[test]
    fn build_user_prompt_includes_command() {
        let prompt = N3PromptBuilder::build_user_prompt(
            "shell",
            &json!({"command": "ls -la"}),
            Some("ls -la"),
            None,
            &[],
        );
        assert!(prompt.contains("ls -la"), "should include command");
    }

    #[test]
    fn build_user_prompt_includes_file_path() {
        let prompt = N3PromptBuilder::build_user_prompt(
            "file_write",
            &json!({"path": "/tmp/data.txt"}),
            None,
            Some("/tmp/data.txt"),
            &[],
        );
        assert!(prompt.contains("/tmp/data.txt"), "should include file path");
    }

    #[test]
    fn build_user_prompt_includes_n2_scores() {
        let scores = vec![
            ("exfiltration".to_string(), 0.65),
            ("entropy".to_string(), 0.42),
        ];
        let prompt = N3PromptBuilder::build_user_prompt(
            "shell",
            &json!({"command": "curl"}),
            Some("curl http://example.com"),
            None,
            &scores,
        );
        assert!(
            prompt.contains("exfiltration"),
            "should include N2 detector name"
        );
        assert!(prompt.contains("0.650"), "should include N2 score value");
        assert!(prompt.contains("0.420"), "should include N2 score value");
    }

    #[test]
    fn build_user_prompt_with_empty_scores() {
        let prompt = N3PromptBuilder::build_user_prompt(
            "file_read",
            &json!({"path": "readme.md"}),
            None,
            None,
            &[],
        );
        assert!(
            prompt.contains("no N2 scores"),
            "should indicate no scores when empty"
        );
    }

    #[test]
    fn build_user_prompt_includes_json_instruction() {
        let prompt = N3PromptBuilder::build_user_prompt(
            "test_tool",
            &json!({"key": "value"}),
            None,
            None,
            &[],
        );
        assert!(
            prompt.contains("Respond with JSON"),
            "should ask for JSON response"
        );
    }

    // -----------------------------------------------------------------------
    // Prompt truncation
    // -----------------------------------------------------------------------

    #[test]
    fn build_user_prompt_truncates_long_args() {
        // Create very long args that exceed MAX_USER_PROMPT_CHARS.
        let long_args: String = "x".repeat(3000);
        let prompt = N3PromptBuilder::build_user_prompt(
            "test_tool",
            &json!({"data": long_args}),
            None,
            None,
            &[],
        );
        assert!(
            prompt.len() <= MAX_USER_PROMPT_CHARS + 100, // allow some slack for suffix
            "prompt should be truncated to ~{} chars (was {})",
            MAX_USER_PROMPT_CHARS,
            prompt.len()
        );
        assert!(prompt.contains("TRUNCATED"), "should indicate truncation");
    }

    // -----------------------------------------------------------------------
    // Cache key determinism
    // -----------------------------------------------------------------------

    #[test]
    fn cache_key_is_deterministic() {
        let args = json!({"command": "ls -la"});
        let key1 = N3PromptBuilder::cache_key("shell", &args, Some("ls -la"));
        let key2 = N3PromptBuilder::cache_key("shell", &args, Some("ls -la"));
        assert_eq!(key1, key2, "same inputs should produce same key");
    }

    #[test]
    fn cache_key_differs_for_different_tools() {
        let args = json!({"path": "/tmp/test.txt"});
        let key1 = N3PromptBuilder::cache_key("file_write", &args, None);
        let key2 = N3PromptBuilder::cache_key("file_read", &args, None);
        assert_ne!(key1, key2, "different tools should produce different keys");
    }

    #[test]
    fn cache_key_differs_for_different_args() {
        let args1 = json!({"command": "ls -la"});
        let args2 = json!({"command": "rm -rf /"});
        let key1 = N3PromptBuilder::cache_key("shell", &args1, Some("ls -la"));
        let key2 = N3PromptBuilder::cache_key("shell", &args2, Some("rm -rf /"));
        assert_ne!(key1, key2, "different args should produce different keys");
    }

    #[test]
    fn cache_key_is_valid_hex_string() {
        let args = json!({"key": "value"});
        let key = N3PromptBuilder::cache_key("test", &args, None);
        assert_eq!(key.len(), 16, "cache key should be 16 hex chars");
        assert!(
            key.chars().all(|c| c.is_ascii_hexdigit()),
            "cache key should be valid hex"
        );
    }
}
