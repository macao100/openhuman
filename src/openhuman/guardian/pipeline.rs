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

use crate::openhuman::guardian::rules::RuleSet;
use crate::openhuman::guardian::types::{N1Result, RuleAction, RuleContext, RuleResult};
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
}
