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

// ── Structured Plan types (INJ-04) ────────────────────────────────────────────

/// A structured action plan emitted by the LLM and validated by the Guardian
/// before any step executes.
///
/// The LLM emits this as JSON before making multi-step tool calls. The Guardian
/// validates the complete plan structure, caps the step count, and runs each
/// step through the N1 -> N2 -> N3 pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredPlan {
    /// High-level description of what the plan aims to achieve.
    pub goal: String,
    /// Ordered list of steps to execute.
    pub steps: Vec<PlanStep>,
}

/// A single step within a structured plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Tool name to invoke (e.g. "file_read", "shell", "web_fetch").
    pub tool: String,
    /// Arguments to pass to the tool (as a JSON object).
    pub args: serde_json::Value,
    /// Why this step is necessary (for the Guardian's intent check).
    pub rationale: String,
}

/// Result of validating a `StructuredPlan` through the Guardian pipeline.
///
/// Contains the final allow/block decision, which validation stage rejected it,
/// and per-step pipeline results.
#[derive(Debug, Clone, Serialize)]
pub struct PlanValidationResult {
    /// Whether the plan is approved for execution.
    pub allowed: bool,
    /// Which level rejected the plan:
    /// `"structure"`, `"step_n1"`, `"step_n2"`, `"step_n3"`, or `"none"`.
    pub blocked_by: String,
    /// Human-readable explanation of the validation outcome.
    pub reasoning: String,
    /// Indices of rejected steps (if any).
    pub rejected_steps: Vec<usize>,
    /// Per-step pipeline results (only evaluated steps).
    pub step_results: Vec<GuardianPipelineResult>,
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

    // ── StructuredPlan tests (INJ-04) ──

    #[test]
    fn structured_plan_serde_roundtrip() {
        let plan = StructuredPlan {
            goal: "Read project documentation".into(),
            steps: vec![
                PlanStep {
                    tool: "file_read".into(),
                    args: json!({"path": "README.md"}),
                    rationale: "Understand the project structure".into(),
                },
                PlanStep {
                    tool: "glob".into(),
                    args: json!({"pattern": "src/**/*.rs"}),
                    rationale: "List all Rust source files".into(),
                },
            ],
        };
        let json = serde_json::to_string(&plan).unwrap();
        let parsed: StructuredPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.goal, "Read project documentation");
        assert_eq!(parsed.steps.len(), 2);
        assert_eq!(parsed.steps[0].tool, "file_read");
        assert_eq!(parsed.steps[1].tool, "glob");
        assert_eq!(
            parsed.steps[0].rationale,
            "Understand the project structure"
        );
    }

    #[test]
    fn structured_plan_empty_steps() {
        let plan = StructuredPlan {
            goal: "Empty plan test".into(),
            steps: vec![],
        };
        let json = serde_json::to_string(&plan).unwrap();
        let parsed: StructuredPlan = serde_json::from_str(&json).unwrap();
        assert!(parsed.steps.is_empty());
        assert_eq!(parsed.goal, "Empty plan test");
    }

    #[test]
    fn structured_plan_max_steps() {
        let steps: Vec<PlanStep> = (0..10)
            .map(|i| PlanStep {
                tool: "file_read".into(),
                args: json!({"path": format!("file_{}.md", i)}),
                rationale: "Test step".into(),
            })
            .collect();
        let plan = StructuredPlan {
            goal: "Ten step plan".into(),
            steps,
        };
        let json = serde_json::to_string(&plan).unwrap();
        let parsed: StructuredPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.steps.len(), 10);
    }

    #[test]
    fn plan_validation_result_allowed() {
        let result = PlanValidationResult {
            allowed: true,
            blocked_by: "none".into(),
            reasoning: "All steps passed".into(),
            rejected_steps: vec![],
            step_results: vec![],
        };
        assert!(result.allowed);
        assert_eq!(result.blocked_by, "none");
        assert!(result.rejected_steps.is_empty());
    }

    #[test]
    fn plan_validation_result_blocked() {
        let result = PlanValidationResult {
            allowed: false,
            blocked_by: "step_n1".into(),
            reasoning: "Step 0 blocked by N1".into(),
            rejected_steps: vec![0],
            step_results: vec![],
        };
        assert!(!result.allowed);
        assert_eq!(result.blocked_by, "step_n1");
        assert_eq!(result.rejected_steps, vec![0]);
    }

    #[test]
    fn plan_validation_result_serde() {
        let result = PlanValidationResult {
            allowed: true,
            blocked_by: "none".into(),
            reasoning: "All good".into(),
            rejected_steps: vec![],
            step_results: vec![],
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["allowed"], true);
        assert_eq!(parsed["blocked_by"], "none");
        assert_eq!(parsed["reasoning"], "All good");
    }
}
