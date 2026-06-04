//! Guardian N1 operations — business logic for RPC handlers.
//!
//! Provides the functions that `schemas.rs` handlers delegate to:
//! rule introspection, YAML reload, and pipeline evaluation.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::openhuman::guardian::pipeline::GuardianN1;
use crate::openhuman::guardian::rules;
use crate::rpc::RpcOutcome;

/// Default path for YAML rules: `~/.dadou/guardian-rules.yaml`.
fn default_yaml_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".dadou")
        .join("guardian-rules.yaml")
}

/// Return the list of active rules in the current N1 pipeline.
pub async fn get_active_rules() -> Result<RpcOutcome<Value>, String> {
    let yaml_path = default_yaml_path();
    let yaml_rules = rules::load_yaml_rules(&yaml_path);

    let rust_rule_names: Vec<String> = rules::default_rust_rules()
        .iter()
        .map(|r| r.name().to_string())
        .collect();

    let yaml_rule_names: Vec<String> = yaml_rules.iter().map(|r| r.name().to_string()).collect();

    let payload = json!({
        "rust_rules": rust_rule_names,
        "yaml_rules": yaml_rule_names,
        "yaml_path": yaml_path.to_string_lossy(),
        "total_rules": rust_rule_names.len() + yaml_rule_names.len(),
    });

    Ok(RpcOutcome::new(payload, vec![]))
}

/// Reload YAML rules from the given path (or the default).
pub async fn reload_yaml_rules(yaml_path: Option<&str>) -> Result<RpcOutcome<Value>, String> {
    let path = match yaml_path {
        Some(p) => PathBuf::from(p),
        None => default_yaml_path(),
    };

    let loaded = rules::load_yaml_rules(&path);
    let count = loaded.len();

    log::info!("[guardian] Reloaded {count} YAML rules from {}", path.display());

    let payload = json!({
        "loaded_count": count,
        "yaml_path": path.to_string_lossy(),
    });

    Ok(RpcOutcome::new(payload, vec![]))
}

/// Evaluate the N1 pipeline for debugging purposes.
pub async fn evaluate_pipeline(
    tool_name: &str,
    args: Value,
    command: Option<&str>,
    file_path: Option<&str>,
) -> Result<RpcOutcome<Value>, String> {
    let policy = Arc::new(crate::openhuman::security::policy::SecurityPolicy::default());
    let yaml_path = default_yaml_path();
    let guardian = GuardianN1::new(policy, Some(&yaml_path));

    let result = guardian.evaluate(tool_name, &args, command, file_path).await;

    let payload = json!({
        "allowed": result.allowed,
        "rule_results": result.rule_results.iter().map(|r| json!({
            "rule_name": r.rule_name,
            "action": if r.action == crate::openhuman::guardian::types::RuleAction::Allow { "allow" } else { "block" },
            "reason": r.reason,
        })).collect::<Vec<_>>(),
        "latency_us": result.latency_us,
    });

    Ok(RpcOutcome::new(payload, vec![]))
}
