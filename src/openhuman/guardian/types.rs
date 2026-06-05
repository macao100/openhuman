//! Core types for the Guardian N1 deterministic rule engine.
//!
//! This module defines the fundamental types used across the Guardian domain:
//! rule actions, evaluation results, context, and the pipeline result type.

use serde::{Deserialize, Serialize};

pub use crate::openhuman::guardian::n2::types::N2Result;
pub use crate::openhuman::guardian::n3::types::N3Result;

/// Action taken by a Guardian rule after evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleAction {
    Allow,
    Block,
}

/// Result of evaluating a single Guardian rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleResult {
    pub action: RuleAction,
    pub rule_name: String,
    pub reason: String,
}

impl RuleResult {
    /// Create an Allow result for the given rule.
    pub fn allowed(name: impl Into<String>) -> Self {
        Self {
            action: RuleAction::Allow,
            rule_name: name.into(),
            reason: String::new(),
        }
    }

    /// Create a Block result for the given rule with a reason.
    pub fn blocked(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            action: RuleAction::Block,
            rule_name: name.into(),
            reason: reason.into(),
        }
    }
}

/// Context passed to each Guardian rule for evaluation.
///
/// Carries the tool invocation details that rules inspect to make
/// allow/block decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleContext {
    /// The name of the tool being invoked (e.g. "file_write", "shell").
    pub tool_name: String,
    /// The full JSON arguments passed to the tool.
    pub tool_args: serde_json::Value,
    /// The shell command string, if the tool is a shell executor.
    pub command: Option<String>,
    /// The file path being accessed, if the tool operates on files.
    pub file_path: Option<String>,
}

/// Result of the full Guardian N1 pipeline.
///
/// Contains the final allow/block decision, the individual rule results,
/// and the total latency in microseconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct N1Result {
    /// Whether the action is allowed (true) or blocked (false).
    pub allowed: bool,
    /// Results from every rule evaluated, in evaluation order.
    pub rule_results: Vec<RuleResult>,
    /// Total pipeline latency in microseconds.
    pub latency_us: u64,
}

/// Result of the combined Guardian pipeline (N1 + N2 + N3).
///
/// Contains the final decision, which level blocked the action, and the
/// individual results from each stage. Stages that were not evaluated
/// due to early exit contain `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianPipelineResult {
    /// Whether the action is ultimately allowed.
    pub allowed: bool,
    /// Which level blocked the action: `"n1"`, `"n2"`, `"n3"`, or `"none"`.
    pub blocked_by: String,
    /// The N1 deterministic rule result (always present).
    pub n1: N1Result,
    /// The N2 heuristic classifier result, if evaluated.
    pub n2: Option<N2Result>,
    /// The N3 LLM validator result, if evaluated.
    pub n3: Option<N3Result>,
}

/// Trait for an individual Guardian rule.
///
/// Each rule implements `evaluate()` which takes a `RuleContext` and
/// returns a `RuleResult`. Rules are `Send + Sync` so they can be
/// evaluated concurrently from multiple agent sessions.
pub trait GuardianRule: Send + Sync {
    /// Human-readable name for this rule (used in results and logs).
    fn name(&self) -> &str;
    /// Evaluate this rule against the given context.
    fn evaluate(&self, ctx: &RuleContext) -> RuleResult;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rule_action_serde_roundtrip() {
        let json = serde_json::to_string(&RuleAction::Block).unwrap();
        assert_eq!(json, "\"Block\"");
        let parsed: RuleAction = serde_json::from_str("\"Allow\"").unwrap();
        assert_eq!(parsed, RuleAction::Allow);
    }

    #[test]
    fn rule_result_allowed_creates_allow_action() {
        let r = RuleResult::allowed("test-rule");
        assert_eq!(r.action, RuleAction::Allow);
        assert_eq!(r.rule_name, "test-rule");
        assert!(r.reason.is_empty());
    }

    #[test]
    fn rule_result_blocked_creates_block_action() {
        let r = RuleResult::blocked("test-rule", "dangerous operation");
        assert_eq!(r.action, RuleAction::Block);
        assert_eq!(r.rule_name, "test-rule");
        assert_eq!(r.reason, "dangerous operation");
    }

    #[test]
    fn rule_context_holds_tool_invocation_data() {
        let ctx = RuleContext {
            tool_name: "file_write".into(),
            tool_args: json!({"path": "/tmp/test.txt"}),
            command: Some("echo hello".into()),
            file_path: Some("/tmp/test.txt".into()),
        };
        assert_eq!(ctx.tool_name, "file_write");
        assert!(ctx.command.is_some());
        assert!(ctx.file_path.is_some());
    }

    #[test]
    fn n1_result_aggregates_pipeline_output() {
        let result = N1Result {
            allowed: false,
            rule_results: vec![RuleResult::blocked("block-rules", "blocked")],
            latency_us: 42,
        };
        assert!(!result.allowed);
        assert_eq!(result.rule_results.len(), 1);
        assert_eq!(result.latency_us, 42);
    }
}
