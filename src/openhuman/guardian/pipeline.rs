//! Guardian N1 pipeline — deterministic rule evaluation.
//!
//! Wraps the existing [`SecurityPolicy`] with the Guardian rule engine
//! to form the complete N1 evaluation pipeline:
//!
//! **classify → gate → validate**
//!
//! The pipeline:
//! 1. Evaluates all compiled Rust + additive YAML rules (RuleSet).
//! 2. Wraps `SecurityPolicy::check_gated_command` for shell tools.
//! 3. Wraps `SecurityPolicy::validate_path` / `validate_parent_path` for file tools.
//! 4. Measures and reports latency in microseconds.
//!
//! Target latency: <1000μs (<1ms) per evaluation (D-03).

use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use crate::openhuman::guardian::n2::GuardianN2;
use crate::openhuman::guardian::n3::GuardianN3;
use crate::openhuman::guardian::n3::types::N3Config;
use crate::openhuman::guardian::rules::RuleSet;
use crate::openhuman::guardian::types::{
    GuardianPipelineResult, N1Result, N2Result, RuleAction, RuleContext, RuleResult,
};
use crate::openhuman::security::policy::SecurityPolicy;

/// Global singleton for the N1 pipeline, accessible from anywhere in the process.
static GLOBAL_GUARDIAN: OnceLock<Arc<GuardianN1>> = OnceLock::new();

/// The complete Guardian N1 pipeline.
///
/// Combines the compiled + YAML rule set with the existing `SecurityPolicy`
/// to provide a single entry point for deterministic action validation.
pub struct GuardianN1 {
    /// The compiled + YAML rule set.
    rule_set: RuleSet,
    /// Reference to the existing security policy (path validation, command gating).
    policy: Arc<SecurityPolicy>,
}

impl GuardianN1 {
    /// Initialize the global GuardianN1 singleton. Must be called once at startup.
    ///
    /// Returns an error if already initialized.
    pub fn init_global(guardian: GuardianN1) -> Result<(), &'static str> {
        GLOBAL_GUARDIAN
            .set(Arc::new(guardian))
            .map_err(|_| "GuardianN1 already initialized")
    }

    /// Get the global GuardianN1 singleton, if initialized.
    pub fn try_global() -> Option<Arc<GuardianN1>> {
        GLOBAL_GUARDIAN.get().cloned()
    }

    /// Create a new N1 pipeline with the given policy and optional YAML path.
    ///
    /// If `yaml_path` is `None`, only compiled Rust rules are used.
    pub fn new(policy: Arc<SecurityPolicy>, yaml_path: Option<&Path>) -> Self {
        let rule_set = crate::openhuman::guardian::rules::compile_ruleset(yaml_path);
        Self { rule_set, policy }
    }

    /// Evaluate the full N1 pipeline for a tool invocation.
    ///
    /// # Arguments
    /// * `tool_name` — the name of the tool being invoked (e.g. "file_write", "shell").
    /// * `args` — the JSON arguments passed to the tool.
    /// * `command` — the shell command string, if applicable.
    /// * `file_path` — the file path being accessed, if applicable.
    ///
    /// # Returns
    /// An [`N1Result`] with the allow/block decision, individual rule results,
    /// and total latency in microseconds.
    pub async fn evaluate(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        command: Option<&str>,
        file_path: Option<&str>,
    ) -> N1Result {
        let start = Instant::now();

        // Step 1: Build the rule context and evaluate the RuleSet.
        let ctx = RuleContext {
            tool_name: tool_name.to_string(),
            tool_args: args.clone(),
            command: command.map(|s| s.to_string()),
            file_path: file_path.map(|s| s.to_string()),
        };

        let mut rule_results = self.rule_set.evaluate_all(&ctx);

        // Check if any Rust rule blocked (fail-closed — even if YAML allows).
        let any_block = rule_results
            .iter()
            .any(|r| r.action == RuleAction::Block);

        // Step 2: Shell command gating via SecurityPolicy (only if not already blocked).
        if !any_block {
            if let Some(cmd) = command {
                if tool_name == "shell" || tool_name == "bash" {
                    match self.policy.check_gated_command(cmd) {
                        Ok(_class) => {
                            // Command passed the gate; SecurityPolicy classified it as
                            // acceptable for the current autonomy level.
                        }
                        Err(reason) => {
                            rule_results.push(RuleResult {
                                action: RuleAction::Block,
                                rule_name: "security-policy:check_gated_command".into(),
                                reason,
                            });
                        }
                    }
                }
            }
        }

        // Step 3: Path validation via SecurityPolicy (only if not already blocked).
        let still_allowed = !rule_results
            .iter()
            .any(|r| r.action == RuleAction::Block);

        if still_allowed {
            if let Some(path) = file_path {
                match tool_name {
                    // Tools that create files (need parent dir validation).
                    "file_write" | "apply_patch" => {
                        if let Err(reason) = self.policy.validate_parent_path(path).await {
                            rule_results.push(RuleResult {
                                action: RuleAction::Block,
                                rule_name: "security-policy:validate_parent_path".into(),
                                reason,
                            });
                        }
                    }
                    // Tools that read/modify existing files.
                    "file_read" | "edit" | "glob" | "grep" | "read_diff" | "run_linter"
                    | "run_tests" => {
                        if let Err(reason) = self.policy.validate_path(path).await {
                            rule_results.push(RuleResult {
                                action: RuleAction::Block,
                                rule_name: "security-policy:validate_path".into(),
                                reason,
                            });
                        }
                    }
                    _ => {
                        // Not a file tool — skip path validation.
                    }
                }
            }
        }

        // Step 4: Measure latency.
        let latency_us = start.elapsed().as_micros() as u64;

        // Step 5: Final decision.
        let allowed = !rule_results
            .iter()
            .any(|r| r.action == RuleAction::Block);

        N1Result {
            allowed,
            rule_results,
            latency_us,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// GuardianPipeline — Combined N1 -> N2 -> N3 pipeline
// ═══════════════════════════════════════════════════════════════════════════

/// Global singleton for the combined Guardian pipeline (N1 + N2 + N3).
static GLOBAL_PIPELINE: OnceLock<Arc<GuardianPipeline>> = OnceLock::new();

/// The complete Guardian pipeline: N1 -> N2 -> N3 with early exit.
///
/// Evaluates tool actions sequentially through three stages:
///
/// 1. **N1** — Deterministic rules (Rust + YAML). Fast, <1ms.
/// 2. **N2** — Heuristic classifiers (exfiltration, entropy, hidden payloads). <10ms.
/// 3. **N3** — LLM validator (only if N2 is uncertain ~2% of actions). <500ms.
///
/// If N1 blocks, N2 and N3 are not evaluated (early exit). If N2 blocks,
/// N3 is not evaluated. N3 is only called when N2 escalates.
///
/// When N3 is disabled and N2 escalates, the action is **blocked** (fail-closed).
pub struct GuardianPipeline {
    n1: Arc<GuardianN1>,
    n2: Arc<GuardianN2>,
    n3: Arc<GuardianN3>,
}

impl GuardianPipeline {
    /// Create a new Guardian pipeline.
    pub fn new(n1: GuardianN1, n2: GuardianN2, n3: GuardianN3) -> Self {
        Self {
            n1: Arc::new(n1),
            n2: Arc::new(n2),
            n3: Arc::new(n3),
        }
    }

    /// Initialize the global GuardianPipeline singleton. Must be called once at startup.
    ///
    /// Returns an error if already initialized.
    pub fn init_global(pipeline: GuardianPipeline) -> Result<(), &'static str> {
        GLOBAL_PIPELINE
            .set(Arc::new(pipeline))
            .map_err(|_| "GuardianPipeline already initialized")
    }

    /// Get the global GuardianPipeline singleton, if initialized.
    pub fn try_global() -> Option<Arc<GuardianPipeline>> {
        GLOBAL_PIPELINE.get().cloned()
    }

    /// Evaluate the full pipeline (N1 -> N2 -> N3) for a tool invocation.
    ///
    /// # Early exit logic
    ///
    /// | Condition | Blocked by | N2 called? | N3 called? |
    /// |-----------|-----------|-----------|------------|
    /// | N1 blocks | `"n1"` | No | No |
    /// | N2 blocks | `"n2"` | Yes | No |
    /// | N2 escalates, N3 blocks | `"n3"` | Yes | Yes |
    /// | N2 escalates, N3 disabled | `"n2"` (fail-closed) | Yes | No |
    /// | All pass | `"none"` | Yes | Maybe |
    pub async fn evaluate(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        command: Option<&str>,
        file_path: Option<&str>,
    ) -> GuardianPipelineResult {
        // ── Stage 1: N1 (deterministic rules, always evaluated) ──────
        let n1_result = self.n1.evaluate(tool_name, args, command, file_path).await;

        if !n1_result.allowed {
            log::debug!(
                "[guardian:pipeline] N1 blocked tool={} latency={}μs",
                tool_name,
                n1_result.latency_us,
            );
            return GuardianPipelineResult {
                allowed: false,
                blocked_by: "n1".into(),
                n1: n1_result,
                n2: None,
                n3: None,
            };
        }

        // ── Stage 2: N2 (heuristic classifiers, synchronous) ─────────
        let n2_result = self.n2.evaluate(tool_name, args, command, file_path);

        if !n2_result.allowed {
            log::warn!(
                "[guardian:pipeline] N2 blocked tool={} latency={}μs scores={:?}",
                tool_name,
                n2_result.latency_us,
                n2_result.scores,
            );
            return GuardianPipelineResult {
                allowed: false,
                blocked_by: "n2".into(),
                n1: n1_result,
                n2: Some(n2_result),
                n3: None,
            };
        }

        // ── Stage 3: N3 (LLM validator, only if N2 escalates) ────────
        let n3_result = if n2_result.escalate {
            if self.n3.config().enabled {
                log::info!(
                    "[guardian:pipeline] N2 escalated tool={} — calling N3",
                    tool_name,
                );
                let n2_scores: Vec<(String, f64)> = n2_result
                    .scores
                    .iter()
                    .map(|s| (s.triggered_by.clone(), s.score))
                    .collect();
                let result = self
                    .n3
                    .evaluate(tool_name, args, command, file_path, &n2_scores)
                    .await;
                Some(result)
            } else {
                // N3 disabled + N2 escalated → fail-closed: block.
                log::warn!(
                    "[guardian:pipeline] N2 escalated but N3 disabled — blocking tool={}",
                    tool_name,
                );
                return GuardianPipelineResult {
                    allowed: false,
                    blocked_by: "n2".into(),
                    n1: n1_result,
                    n2: Some(n2_result),
                    n3: None,
                };
            }
        } else {
            None
        };

        // ── Final decision ───────────────────────────────────────────
        let n3_blocks = n3_result.as_ref().map_or(false, |r| r.should_block());

        if n3_blocks {
            log::warn!(
                "[guardian:pipeline] N3 blocked tool={} verdict={:?} n3_latency={}μs",
                tool_name,
                n3_result.as_ref().map(|r| &r.verdict),
                n3_result.as_ref().map_or(0, |r| r.latency_us),
            );
        } else {
            log::debug!(
                "[guardian:pipeline] All stages allowed tool={}",
                tool_name,
            );
        }

        GuardianPipelineResult {
            allowed: !n3_blocks,
            blocked_by: if n3_blocks { "n3".into() } else { "none".into() },
            n1: n1_result,
            n2: Some(n2_result),
            n3: n3_result,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhuman::security::policy::SecurityPolicy;
    use serde_json::json;

    fn test_policy() -> Arc<SecurityPolicy> {
        Arc::new(SecurityPolicy::default())
    }

    #[tokio::test]
    async fn pipeline_blocks_on_rust_rule() {
        let guardian = GuardianN1::new(test_policy(), None);
        let result = guardian
            .evaluate(
                "shell",
                &json!({"command": "rm -rf /etc"}),
                Some("rm -rf /etc"),
                None,
            )
            .await;
        assert!(
            !result.allowed,
            "should block rm -rf (Rust regex rule block-rm-rf-absolute)"
        );
        assert!(result.latency_us > 0, "latency should be measured");
    }

    #[tokio::test]
    async fn pipeline_reports_latency() {
        let guardian = GuardianN1::new(test_policy(), None);
        let result = guardian
            .evaluate(
                "file_read",
                &json!({"path": "workspace/readme.md"}),
                None,
                Some("workspace/readme.md"),
            )
            .await;
        assert!(result.latency_us > 0, "should measure latency in μs");
    }

    #[tokio::test]
    async fn pipeline_combines_rust_and_yaml() {
        // Create a temp YAML that blocks /tmp.
        let dir = std::env::temp_dir();
        let yaml_path = dir.join(format!("guardian-pipeline-test-{}.yaml", uuid::Uuid::new_v4()));
        let yaml = r#"
rules:
  - name: "block-tmp"
    action: deny
    match:
      path_glob: "/tmp/**"
"#;
        std::fs::write(&yaml_path, yaml).unwrap();

        let guardian = GuardianN1::new(test_policy(), Some(&yaml_path));
        let _ = std::fs::remove_file(&yaml_path);

        // File write to /tmp should be blocked by YAML rule.
        let result = guardian
            .evaluate(
                "file_write",
                &json!({"path": "/tmp/evil.txt"}),
                None,
                Some("/tmp/evil.txt"),
            )
            .await;
        let yaml_block = result
            .rule_results
            .iter()
            .any(|r| r.action == RuleAction::Block && r.rule_name == "block-tmp");
        assert!(yaml_block, "YAML rule should block /tmp writes");
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn pipeline_allows_safe_operation() {
        let guardian = GuardianN1::new(test_policy(), None);
        let result = guardian
            .evaluate(
                "file_read",
                &json!({"path": "workspace/src/main.rs"}),
                None,
                Some("workspace/src/main.rs"),
            )
            .await;
        // No rules should block a simple file read in the workspace.
        assert!(
            result.allowed,
            "safe file_read should be allowed (blocked by: {:?})",
            result
                .rule_results
                .iter()
                .filter(|r| r.action == RuleAction::Block)
                .map(|r| &r.rule_name)
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn pipeline_handles_no_command_no_path() {
        let guardian = GuardianN1::new(test_policy(), None);
        let result = guardian
            .evaluate("some_tool", &json!({}), None, None)
            .await;
        assert!(result.allowed, "tools without command or path should be allowed");
        assert!(result.latency_us > 0);
    }

    // ── Combined pipeline tests ─────────────────────────────────────

    /// Helper: create a test pipeline with given configs.
    fn test_pipeline(
        n2_block_threshold: f64,
        n2_escalate_threshold: f64,
        n3_enabled: bool,
    ) -> GuardianPipeline {
        let n1 = GuardianN1::new(test_policy(), None);
        let n2_config =
            crate::openhuman::guardian::n2::types::N2EngineConfig::new(
                n2_block_threshold, n2_escalate_threshold, 10000,
            );
        let n2 = GuardianN2::new(n2_config);
        let n3_config = N3Config {
            enabled: n3_enabled,
            timeout_ms: 5,  // Quick timeout for tests
            max_tokens: 10,
            cache_size: 0,
            model_override: None,
        };
        let n3 = GuardianN3::new(n3_config);
        GuardianPipeline::new(n1, n2, n3)
    }

    #[tokio::test]
    async fn pipeline_blocks_on_n1_n2_and_n3_not_called() {
        let pipeline = test_pipeline(0.7, 0.3, true);
        let result = pipeline
            .evaluate(
                "shell",
                &json!({"command": "rm -rf /etc"}),
                Some("rm -rf /etc"),
                None,
            )
            .await;
        assert!(!result.allowed, "N1 should block rm -rf");
        assert_eq!(result.blocked_by, "n1", "should be blocked by n1");
        assert!(result.n2.is_none(), "N2 should not be called when N1 blocks");
        assert!(result.n3.is_none(), "N3 should not be called when N1 blocks");
    }

    #[tokio::test]
    async fn pipeline_blocks_on_n2_n3_not_called() {
        let pipeline = test_pipeline(0.7, 0.3, true);
        // A base64 string triggers entropy detection with score 0.7 (blocks).
        let b64 = "SGVsbG8gV29ybGQgVGhpcyBpcyBhIGJhc2U2NCBlbmNvZGVkIHN0cmluZw==";
        let command = format!("echo {}", b64);

        let result = pipeline
            .evaluate(
                "shell",
                &json!({"command": command}),
                Some(&command),
                None,
            )
            .await;
        assert!(!result.allowed, "N2 should block base64 command");
        assert_eq!(result.blocked_by, "n2", "should be blocked by n2");
        assert!(result.n2.is_some(), "N2 should have been evaluated");
        assert!(result.n3.is_none(), "N3 should not be called when N2 blocks");
    }

    #[tokio::test]
    async fn pipeline_calls_n3_when_n2_escalates() {
        // N2 with block_threshold=1.0 (nothing blocks at N2) and
        // escalate_threshold=0.0 (any non-zero entropy triggers escalation).
        // A hex string gives entropy ~4.0-4.5 → score 0.2 (>0.0, <0.7).
        let pipeline = test_pipeline(1.0, 0.0, true);
        let hex = "48656c6c6f20576f726c64205468697320697320612068657820656e636f64656420737472696e67";
        let command = format!("echo {}", hex);

        let result = pipeline
            .evaluate(
                "shell",
                &json!({"command": command}),
                Some(&command),
                None,
            )
            .await;
        assert!(result.n2.is_some(), "N2 should have been evaluated");
        assert!(
            result.n2.as_ref().unwrap().escalate,
            "N2 should escalate for hex entropy score 0.2 >= 0.0"
        );
        // N3 is called and returns Uncertain (no real LLM in test).
        // Fail-closed: Uncertain → should_block() = true.
        assert!(
            result.n3.is_some(),
            "N3 should have been called when N2 escalates"
        );
        assert!(!result.allowed, "N3 should block (fail-closed in test)");
        assert_eq!(result.blocked_by, "n3", "should be blocked by N3");
    }

    #[tokio::test]
    async fn pipeline_blocks_when_n3_disabled_and_n2_escalates() {
        // N3 disabled + N2 escalates → fail-closed: block by n2.
        let pipeline = test_pipeline(1.0, 0.0, false);
        let hex = "48656c6c6f20576f726c64205468697320697320612068657820656e636f64656420737472696e67";
        let command = format!("echo {}", hex);

        let result = pipeline
            .evaluate(
                "shell",
                &json!({"command": command}),
                Some(&command),
                None,
            )
            .await;
        assert!(!result.allowed, "should block when N3 disabled and N2 escalates");
        assert_eq!(result.blocked_by, "n2", "should be blocked by n2 (fail-closed)");
        assert!(result.n2.is_some(), "N2 should have been evaluated");
        assert!(result.n3.is_none(), "N3 should not be called when disabled");
    }

    #[tokio::test]
    async fn pipeline_allows_safe_operation() {
        let pipeline = test_pipeline(0.7, 0.3, true);
        let result = pipeline
            .evaluate(
                "file_read",
                &json!({"path": "workspace/readme.md"}),
                None,
                Some("workspace/readme.md"),
            )
            .await;
        assert!(result.allowed, "safe operation should be allowed");
        assert_eq!(result.blocked_by, "none");
        assert!(result.n2.is_some(), "N2 should have been evaluated");
        assert!(result.n3.is_none(), "N3 should not be called when N2 does not escalate");
    }
}
