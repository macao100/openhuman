//! Compiled rule engine for Guardian N1.
//!
//! Implements compile-time rules (path whitelist, regex patterns, command blocklist),
//! an additive YAML rule loader, and an aggregate rule set evaluator.
//! This is the deterministic core of the N1 pipeline — all Rust rules are evaluated
//! before any YAML rules, and Rust rules always take precedence (fail-closed, D-04/D-05).

use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;

use crate::openhuman::guardian::types::{GuardianRule, RuleAction, RuleContext, RuleResult};

/// Rule that allows access only to paths matching one or more glob patterns.
///
/// Patterns use the `glob` crate syntax. In path mode (`matches_path`),
/// `**` matches across path separators, so a pattern like `workspace/**`
/// matches any file under the `workspace/` directory.
///
/// # Evaluation
/// - If the context carries no `file_path`, the rule allows (not applicable).
/// - If the file path matches any whitelist pattern, the rule allows.
/// - Otherwise, the rule blocks.
pub struct PathWhitelistRule {
    name: String,
    patterns: Arc<[glob::Pattern]>,
}

impl PathWhitelistRule {
    pub fn new(name: impl Into<String>, patterns: Vec<glob::Pattern>) -> Self {
        Self {
            name: name.into(),
            patterns: patterns.into(),
        }
    }
}

impl GuardianRule for PathWhitelistRule {
    fn name(&self) -> &str {
        &self.name
    }

    fn evaluate(&self, ctx: &RuleContext) -> RuleResult {
        let Some(ref path) = ctx.file_path else {
            return RuleResult::allowed(self.name());
        };
        if self.patterns.iter().any(|p| p.matches_path(path.as_ref())) {
            RuleResult::allowed(self.name())
        } else {
            RuleResult::blocked(
                self.name(),
                format!("path does not match whitelist patterns"),
            )
        }
    }
}

/// Rule that blocks tool invocations whose arguments or command match
/// any of the configured regex patterns.
///
/// # Evaluation
/// - Checks the command string (if present) against all patterns.
/// - Checks the serialized JSON args against all patterns.
/// - If any pattern matches either input, the rule blocks.
pub struct RegexPatternRule {
    name: String,
    patterns: Arc<[regex::Regex]>,
}

impl RegexPatternRule {
    pub fn new(name: impl Into<String>, patterns: Vec<regex::Regex>) -> Self {
        Self {
            name: name.into(),
            patterns: patterns.into(),
        }
    }
}

impl GuardianRule for RegexPatternRule {
    fn name(&self) -> &str {
        &self.name
    }

    fn evaluate(&self, ctx: &RuleContext) -> RuleResult {
        if let Some(ref cmd) = ctx.command {
            if self.patterns.iter().any(|p| p.is_match(cmd)) {
                return RuleResult::blocked(
                    self.name(),
                    format!("command matches blocked pattern"),
                );
            }
        }
        let args_str = ctx.tool_args.to_string();
        if self.patterns.iter().any(|p| p.is_match(&args_str)) {
            return RuleResult::blocked(self.name(), format!("arguments match blocked pattern"));
        }
        RuleResult::allowed(self.name())
    }
}

/// Rule that blocks shell commands matching a list of blocked command names.
///
/// The blocked list is matched case-insensitively as a substring of the
/// full command string.
pub struct BlocklistRule {
    name: String,
    blocked_commands: Arc<[String]>,
}

impl BlocklistRule {
    pub fn new(name: impl Into<String>, blocked_commands: Vec<String>) -> Self {
        Self {
            name: name.into(),
            blocked_commands: blocked_commands
                .into_iter()
                .map(|c| c.to_ascii_lowercase())
                .collect::<Vec<_>>()
                .into(),
        }
    }
}

impl GuardianRule for BlocklistRule {
    fn name(&self) -> &str {
        &self.name
    }

    fn evaluate(&self, ctx: &RuleContext) -> RuleResult {
        let Some(ref command) = ctx.command else {
            return RuleResult::allowed(self.name());
        };
        let cmd_lower = command.to_ascii_lowercase();
        if self
            .blocked_commands
            .iter()
            .any(|blocked| cmd_lower.contains(blocked.as_str()))
        {
            RuleResult::blocked(self.name(), format!("command is blocked"))
        } else {
            RuleResult::allowed(self.name())
        }
    }
}

/// Aggregate of multiple GuardianRule instances.
///
/// Evaluates all rules in registration order. The caller interprets
/// the results: if any rule returns `Block`, the overall decision is
/// blocked (fail-closed).
pub struct RuleSet {
    rules: Vec<Box<dyn GuardianRule>>,
}

impl RuleSet {
    pub fn new(rules: Vec<Box<dyn GuardianRule>>) -> Self {
        Self { rules }
    }

    /// Evaluate all rules in order, returning all results.
    pub fn evaluate_all(&self, ctx: &RuleContext) -> Vec<RuleResult> {
        self.rules.iter().map(|rule| rule.evaluate(ctx)).collect()
    }

    /// Returns `true` if any rule blocked the context.
    pub fn is_blocked(&self, ctx: &RuleContext) -> bool {
        self.rules
            .iter()
            .any(|rule| rule.evaluate(ctx).action == RuleAction::Block)
    }
}

// ---------------------------------------------------------------------------
// YAML rule loader (additive overrides, fail-closed)
// ---------------------------------------------------------------------------

/// A single rule loaded from `~/.dadou/guardian-rules.yaml`.
///
/// YAML rules are **additive only** — they can add new restrictions or explicit
/// allows, but they can never disable a compiled Rust rule (D-05).
#[derive(Debug, Clone, Deserialize)]
struct YamlGuardianRuleDef {
    name: String,
    #[serde(default)]
    description: Option<String>,
    action: String, // "deny" | "allow"
    #[serde(rename = "match")]
    match_: YamlMatch,
}

#[derive(Debug, Clone, Deserialize)]
struct YamlMatch {
    #[serde(default)]
    path_glob: Option<String>,
    #[serde(default)]
    command_regex: Option<String>,
    #[serde(default)]
    tool: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct YamlRulesDoc {
    #[serde(default)]
    rules: Vec<YamlGuardianRuleDef>,
}

/// Wraps a YAML-defined rule so it implements [`GuardianRule`].
struct YamlGuardianRule {
    name: String,
    action: RuleAction,
    path_pattern: Option<glob::Pattern>,
    command_regex: Option<regex::Regex>,
    tool_filter: Option<String>,
}

impl GuardianRule for YamlGuardianRule {
    fn name(&self) -> &str {
        &self.name
    }

    fn evaluate(&self, ctx: &RuleContext) -> RuleResult {
        // Tool filter: if set and doesn't match, this rule is not applicable.
        if let Some(ref tool) = self.tool_filter {
            if ctx.tool_name != *tool {
                return RuleResult::allowed(self.name());
            }
        }

        // Path glob check.
        if let Some(ref pat) = self.path_pattern {
            if let Some(ref path) = ctx.file_path {
                if pat.matches_path(path.as_ref()) {
                    return match self.action {
                        RuleAction::Block => RuleResult::blocked(
                            self.name(),
                            format!("YAML rule: path matches blocked glob"),
                        ),
                        RuleAction::Allow => RuleResult::allowed(self.name()),
                    };
                }
            }
        }

        // Command regex check.
        if let Some(ref re) = self.command_regex {
            if let Some(ref cmd) = ctx.command {
                if re.is_match(cmd) {
                    return match self.action {
                        RuleAction::Block => RuleResult::blocked(
                            self.name(),
                            format!("YAML rule: command matches blocked regex"),
                        ),
                        RuleAction::Allow => RuleResult::allowed(self.name()),
                    };
                }
            }
        }

        // Neither path nor command matched — rule doesn't apply.
        RuleResult::allowed(self.name())
    }
}

/// Loads YAML rules from `~/.dadou/guardian-rules.yaml`.
///
/// Returns an empty `Vec` if the file does not exist (silent skip).
/// Logs a warning and returns an empty `Vec` if the file is malformed.
pub fn load_yaml_rules(path: &Path) -> Vec<Box<dyn GuardianRule>> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            log::debug!(
                "[guardian] No YAML rules file at {} — using compiled rules only",
                path.display()
            );
            return Vec::new();
        }
        Err(e) => {
            log::warn!(
                "[guardian] Cannot read YAML rules file {}: {e}",
                path.display()
            );
            return Vec::new();
        }
    };

    let doc: YamlRulesDoc = match serde_yaml::from_str(&content) {
        Ok(d) => d,
        Err(e) => {
            log::warn!(
                "[guardian] Malformed YAML rules file {}: {e}",
                path.display()
            );
            return Vec::new();
        }
    };

    let mut rules: Vec<Box<dyn GuardianRule>> = Vec::with_capacity(doc.rules.len());

    for def in doc.rules {
        let action = match def.action.as_str() {
            "deny" => RuleAction::Block,
            "allow" => RuleAction::Allow,
            other => {
                log::warn!(
                    "[guardian] YAML rule '{}' has unknown action '{}' — skipping",
                    def.name,
                    other
                );
                continue;
            }
        };

        let path_pattern = match def.match_.path_glob {
            Some(ref g) => match glob::Pattern::new(g) {
                Ok(p) => Some(p),
                Err(e) => {
                    log::warn!(
                        "[guardian] YAML rule '{}' has invalid path_glob '{}': {e} — skipping",
                        def.name,
                        g
                    );
                    continue;
                }
            },
            None => None,
        };

        let command_regex = match def.match_.command_regex {
            Some(ref r) => match regex::Regex::new(r) {
                Ok(re) => Some(re),
                Err(e) => {
                    log::warn!(
                        "[guardian] YAML rule '{}' has invalid command_regex '{}': {e} — skipping",
                        def.name,
                        r
                    );
                    continue;
                }
            },
            None => None,
        };

        if path_pattern.is_none() && command_regex.is_none() {
            log::warn!(
                "[guardian] YAML rule '{}' has no path_glob or command_regex — skipping",
                def.name
            );
            continue;
        }

        rules.push(Box::new(YamlGuardianRule {
            name: def.name,
            action,
            path_pattern,
            command_regex,
            tool_filter: def.match_.tool,
        }));
    }

    log::info!(
        "[guardian] Loaded {} YAML rules from {}",
        rules.len(),
        path.display()
    );
    rules
}

/// Build the default compiled Rust ruleset.
///
/// These rules are **always enforced** regardless of YAML configuration
/// (fail-closed, D-04).
pub fn default_rust_rules() -> Vec<Box<dyn GuardianRule>> {
    vec![
        // Block destructive shell patterns.
        Box::new(RegexPatternRule::new(
            "block-rm-rf-absolute",
            vec![regex::Regex::new(r"rm\s+-rf\s+[/~]").unwrap()],
        )),
        Box::new(RegexPatternRule::new(
            "block-curl-pipe-shell",
            vec![regex::Regex::new(r"curl\s+.*\|\s*(ba)?sh").unwrap()],
        )),
        Box::new(RegexPatternRule::new(
            "block-device-write",
            vec![regex::Regex::new(r">\s*/dev/").unwrap()],
        )),
        // Block dangerous commands.
        Box::new(BlocklistRule::new(
            "block-dangerous-commands",
            vec![
                "shutdown".into(),
                "format".into(),
                "mkfs".into(),
                "dd if=".into(),
                ":(){ :|:& };:".into(), // fork bomb
            ],
        )),
    ]
}

/// Combine compiled Rust rules with additive YAML rules into a single
/// [`RuleSet`].
///
/// Rust rules are evaluated **first** in the ruleset; if any Rust rule
/// blocks, YAML `allow` rules cannot override it (D-05 fail-closed).
pub fn compile_ruleset(yaml_path: Option<&Path>) -> RuleSet {
    let mut all: Vec<Box<dyn GuardianRule>> = default_rust_rules();

    if let Some(path) = yaml_path {
        all.extend(load_yaml_rules(path));
    }

    RuleSet::new(all)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use glob::Pattern;
    use regex::Regex;
    use serde_json::json;

    /// Rule that always blocks -- used as a sentinel in RuleSet tests.
    struct AlwaysBlockRule {
        name: String,
    }

    impl GuardianRule for AlwaysBlockRule {
        fn name(&self) -> &str {
            &self.name
        }

        fn evaluate(&self, _ctx: &RuleContext) -> RuleResult {
            RuleResult::blocked(self.name(), "always blocks")
        }
    }

    /// Rule that always allows -- used as a baseline in RuleSet tests.
    struct AlwaysAllowRule {
        name: String,
    }

    impl GuardianRule for AlwaysAllowRule {
        fn name(&self) -> &str {
            &self.name
        }

        fn evaluate(&self, _ctx: &RuleContext) -> RuleResult {
            RuleResult::allowed(self.name())
        }
    }

    fn test_ctx() -> RuleContext {
        RuleContext {
            tool_name: "test".into(),
            tool_args: json!({}),
            command: None,
            file_path: None,
        }
    }

    // -- Test 1: GuardianRule with Block action rejects -------------------

    #[test]
    fn rule_action_block_rejects_action() {
        let rule = AlwaysBlockRule {
            name: "block-all".into(),
        };
        let ctx = test_ctx();
        let result = rule.evaluate(&ctx);
        assert_eq!(result.action, RuleAction::Block);
        assert_eq!(result.rule_name, "block-all");
        assert!(!result.reason.is_empty(), "block reason should be present");
    }

    // -- Test 2: RegexPatternRule detects blocked pattern -----------------

    #[test]
    fn regex_pattern_detects_blocked_pattern_in_command() {
        let re = Regex::new("rm\\s+-rf").unwrap();
        let rule = RegexPatternRule::new("block-rm-rf", vec![re]);
        let ctx = RuleContext {
            tool_name: "shell".into(),
            tool_args: json!({}),
            command: Some("rm -rf /home/user/data".into()),
            file_path: None,
        };
        let result = rule.evaluate(&ctx);
        assert_eq!(
            result.action,
            RuleAction::Block,
            "should block rm -rf patterns"
        );
    }

    // -- Test 3: PathWhitelistRule accepts allowed path -------------------

    #[test]
    fn path_whitelist_accepts_allowed_path() {
        let pat = Pattern::new("workspace/**").unwrap();
        let rule = PathWhitelistRule::new("whitelist-workspace", vec![pat]);
        let ctx = RuleContext {
            tool_name: "file_read".into(),
            tool_args: json!({}),
            command: None,
            file_path: Some("workspace/src/main.rs".into()),
        };
        let result = rule.evaluate(&ctx);
        assert_eq!(
            result.action,
            RuleAction::Allow,
            "path under workspace should be allowed"
        );
    }

    // -- Test 4: PathWhitelistRule rejects outside path -------------------

    #[test]
    fn path_whitelist_rejects_outside_path() {
        let pat = Pattern::new("workspace/**").unwrap();
        let rule = PathWhitelistRule::new("whitelist-workspace", vec![pat]);
        let ctx = RuleContext {
            tool_name: "file_read".into(),
            tool_args: json!({}),
            command: None,
            file_path: Some("/etc/shadow".into()),
        };
        let result = rule.evaluate(&ctx);
        assert_eq!(
            result.action,
            RuleAction::Block,
            "path outside workspace should be blocked"
        );
    }

    // -- Test 5: BlocklistRule detects blocked command --------------------

    #[test]
    fn blocklist_detects_blocked_command() {
        let rule = BlocklistRule::new("block-shutdown", vec!["shutdown".into()]);
        let ctx = RuleContext {
            tool_name: "shell".into(),
            tool_args: json!({}),
            command: Some("shutdown -h now".into()),
            file_path: None,
        };
        let result = rule.evaluate(&ctx);
        assert_eq!(
            result.action,
            RuleAction::Block,
            "should block shutdown command"
        );
    }

    // -- Test 6a: RuleSet -- all pass = allow ----------------------------

    #[test]
    fn ruleset_all_pass_allows() {
        let rules = RuleSet::new(vec![
            Box::new(AlwaysAllowRule {
                name: "allow-1".into(),
            }),
            Box::new(AlwaysAllowRule {
                name: "allow-2".into(),
            }),
        ]);
        let ctx = test_ctx();
        let results = rules.evaluate_all(&ctx);
        assert!(
            results.iter().all(|r| r.action == RuleAction::Allow),
            "all rules passed, should all be Allow"
        );
        assert!(!rules.is_blocked(&ctx), "should not be blocked");
    }

    // -- Test 6b: RuleSet -- any block = blocked ------------------------

    #[test]
    fn ruleset_any_block_blocks() {
        let rules = RuleSet::new(vec![
            Box::new(AlwaysAllowRule {
                name: "allow-1".into(),
            }),
            Box::new(AlwaysBlockRule {
                name: "block-1".into(),
            }),
        ]);
        let ctx = test_ctx();
        let results = rules.evaluate_all(&ctx);
        let has_block = results.iter().any(|r| r.action == RuleAction::Block);
        assert!(has_block, "should have at least one Block result");
        assert!(rules.is_blocked(&ctx), "should be blocked");
    }

    // ── YAML loader tests ──────────────────────────────────────────

    /// Helper: write a YAML string to a temp file and return the path.
    fn write_temp_yaml(content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("guardian-test-{}.yaml", uuid::Uuid::new_v4()));
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn yaml_loads_valid_deny_rule() {
        let yaml = r#"
rules:
  - name: "block-node-modules"
    description: "Block writes to node_modules"
    action: deny
    match:
      path_glob: "**/node_modules/**"
"#;
        let path = write_temp_yaml(yaml);
        let rules = super::load_yaml_rules(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name(), "block-node-modules");
    }

    #[test]
    fn yaml_deny_rule_blocks_matching_path() {
        let yaml = r#"
rules:
  - name: "block-tmp"
    action: deny
    match:
      path_glob: "/tmp/**"
"#;
        let path = write_temp_yaml(yaml);
        let rules = super::load_yaml_rules(&path);
        let _ = std::fs::remove_file(&path);
        let ctx = RuleContext {
            tool_name: "file_write".into(),
            tool_args: json!({}),
            command: None,
            file_path: Some("/tmp/secret.txt".into()),
        };
        let result = rules[0].evaluate(&ctx);
        assert_eq!(result.action, RuleAction::Block);
    }

    #[test]
    fn yaml_missing_file_returns_empty() {
        let rules =
            super::load_yaml_rules(std::path::Path::new("/nonexistent/guardian-rules.yaml"));
        assert!(rules.is_empty());
    }

    #[test]
    fn yaml_malformed_file_returns_empty() {
        let yaml = "this: is: not: valid: [[[ yaml";
        let path = write_temp_yaml(yaml);
        let rules = super::load_yaml_rules(&path);
        let _ = std::fs::remove_file(&path);
        assert!(rules.is_empty());
    }

    #[test]
    fn yaml_allow_rule_allows_explicitly() {
        let yaml = r#"
rules:
  - name: "allow-tmp-reads"
    action: allow
    match:
      path_glob: "/tmp/**"
      tool: "file_read"
"#;
        let path = write_temp_yaml(yaml);
        let rules = super::load_yaml_rules(&path);
        let _ = std::fs::remove_file(&path);
        let ctx = RuleContext {
            tool_name: "file_read".into(),
            tool_args: json!({}),
            command: None,
            file_path: Some("/tmp/log.txt".into()),
        };
        let result = rules[0].evaluate(&ctx);
        assert_eq!(result.action, RuleAction::Allow);
    }

    #[test]
    fn yaml_tool_filter_excludes_non_matching() {
        let yaml = r#"
rules:
  - name: "only-file-write"
    action: deny
    match:
      path_glob: "*.secret"
      tool: "file_write"
"#;
        let path = write_temp_yaml(yaml);
        let rules = super::load_yaml_rules(&path);
        let _ = std::fs::remove_file(&path);
        // file_read should NOT be blocked by this rule.
        let ctx = RuleContext {
            tool_name: "file_read".into(),
            tool_args: json!({}),
            command: None,
            file_path: Some("passwords.secret".into()),
        };
        let result = rules[0].evaluate(&ctx);
        assert_eq!(
            result.action,
            RuleAction::Allow,
            "rule should not apply to file_read"
        );
    }

    #[test]
    fn yaml_command_regex_blocks_matching() {
        let yaml = r#"
rules:
  - name: "block-powershell-download"
    action: deny
    match:
      command_regex: "Invoke-WebRequest|Invoke-RestMethod"
"#;
        let path = write_temp_yaml(yaml);
        let rules = super::load_yaml_rules(&path);
        let _ = std::fs::remove_file(&path);
        let ctx = RuleContext {
            tool_name: "shell".into(),
            tool_args: json!({}),
            command: Some("Invoke-WebRequest -Uri http://evil.com".into()),
            file_path: None,
        };
        let result = rules[0].evaluate(&ctx);
        assert_eq!(result.action, RuleAction::Block);
    }

    #[test]
    fn compile_ruleset_combines_rust_and_yaml() {
        let yaml = r#"
rules:
  - name: "block-tmp-yaml"
    action: deny
    match:
      path_glob: "/tmp/**"
"#;
        let path = write_temp_yaml(yaml);
        let ruleset = super::compile_ruleset(Some(&path));
        let _ = std::fs::remove_file(&path);
        // Rust rules should be present (at least block-rm-rf-absolute).
        let all_results = ruleset.evaluate_all(&RuleContext {
            tool_name: "shell".into(),
            tool_args: json!({}),
            command: Some("rm -rf /etc".into()),
            file_path: None,
        });
        let has_rust_block = all_results
            .iter()
            .any(|r| r.action == RuleAction::Block && r.rule_name == "block-rm-rf-absolute");
        assert!(has_rust_block, "Rust rule block-rm-rf-absolute should fire");
        // YAML rule should also be present.
        let yaml_block = all_results
            .iter()
            .any(|r| r.action == RuleAction::Block && r.rule_name == "block-tmp-yaml");
        // The command doesn't have a path, so YAML path_glob rule won't fire here.
        // But it should be evaluated (as Allow since no path).
        let yaml_evaluated = all_results.iter().any(|r| r.rule_name == "block-tmp-yaml");
        assert!(yaml_evaluated, "YAML rule should be in the ruleset");
    }

    #[test]
    fn rust_rule_blocks_even_if_yaml_allows() {
        // D-05: A YAML "allow" rule for /tmp should NOT override the Rust
        // regex rule that blocks "rm -rf /" patterns.
        let yaml = r#"
rules:
  - name: "allow-tmp"
    action: allow
    match:
      path_glob: "/tmp/**"
"#;
        let path = write_temp_yaml(yaml);
        let ruleset = super::compile_ruleset(Some(&path));
        let _ = std::fs::remove_file(&path);
        // Command: rm -rf targeting a path under /tmp — the Rust regex rule
        // for "rm -rf /" should still block.
        let ctx = RuleContext {
            tool_name: "shell".into(),
            tool_args: json!({}),
            command: Some("rm -rf /tmp/data".into()),
            file_path: Some("/tmp/data".into()),
        };
        assert!(
            ruleset.is_blocked(&ctx),
            "Rust rule should block even though YAML allows /tmp (D-05 fail-closed)"
        );
    }
}
