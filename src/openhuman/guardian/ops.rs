//! Guardian N1 operations — business logic for RPC handlers.
//!
//! Provides the functions that `schemas.rs` handlers delegate to:
//! rule introspection, YAML reload, and pipeline evaluation.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::openhuman::guardian::n2::types::N2EngineConfig;
use crate::openhuman::guardian::n2::GuardianN2;
use crate::openhuman::guardian::n3::GuardianN3;
use crate::openhuman::guardian::pipeline::GuardianN1;
use crate::openhuman::guardian::rules;
use crate::openhuman::guardian::types::StructuredPlan;
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

    log::info!(
        "[guardian] Reloaded {count} YAML rules from {}",
        path.display()
    );

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

    let result = guardian
        .evaluate(tool_name, &args, command, file_path)
        .await;

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

/// Evaluate the N2 classifier for debugging purposes.
pub async fn n2_evaluate(
    tool_name: &str,
    args: Value,
    command: Option<&str>,
    file_path: Option<&str>,
) -> Result<RpcOutcome<Value>, String> {
    let config = N2EngineConfig::default();
    let n2 = GuardianN2::new(config);
    let result = n2.evaluate(tool_name, &args, command, file_path);
    let payload = json!({
        "allowed": result.allowed,
        "escalate": result.escalate,
        "scores": result.scores.iter().map(|s| json!({
            "score": s.score,
            "reason": s.reason,
            "triggered_by": s.triggered_by,
        })).collect::<Vec<_>>(),
        "latency_us": result.latency_us,
    });
    Ok(RpcOutcome::new(payload, vec![]))
}

/// Get N3 validator status.
pub async fn n3_status() -> Result<RpcOutcome<Value>, String> {
    let n3 = GuardianN3::with_defaults();
    let payload = json!({
        "enabled": n3.config().enabled,
        "cache_size": 100,
        "max_tokens": n3.config().max_tokens,
        "timeout_ms": n3.config().timeout_ms,
        "model": n3.config().model_override.as_deref().unwrap_or("default"),
        "version": "1.0",
    });
    Ok(RpcOutcome::new(payload, vec![]))
}

/// Get full pipeline status.
pub async fn pipeline_status() -> Result<RpcOutcome<Value>, String> {
    let n1_rules = get_active_rules().await?;
    let payload = json!({
        "n1": {
            "rules": n1_rules.value,
            "pipeline": "active",
        },
        "n2": {
            "enabled": true,
            "block_threshold": 0.7,
            "escalate_threshold": 0.3,
        },
        "n3": {
            "enabled": true,
            "max_tokens": 256,
            "timeout_ms": 450,
        },
        "pipeline_order": ["n1", "n2", "n3"],
    });
    Ok(RpcOutcome::new(payload, vec![]))
}

/// Validate a structured plan through the Guardian pipeline.
///
/// Parses the plan from a JSON value, runs it through
/// `GuardianPipeline::evaluate_plan()`, and returns the result. This is the
/// operation function backing the `guardian.plan_validate` RPC endpoint.
pub async fn validate_plan(plan_value: Value) -> Result<RpcOutcome<Value>, String> {
    log::debug!("[guardian:plan] validate_plan enter");
    let plan: StructuredPlan = serde_json::from_value(plan_value).map_err(|e| {
        log::warn!("[guardian:plan] Invalid plan JSON: {e}");
        format!("Invalid plan JSON: {e}")
    })?;

    let pipeline = crate::openhuman::guardian::GuardianPipeline::try_global().ok_or_else(|| {
        log::warn!("[guardian:plan] GuardianPipeline not initialized");
        "GuardianPipeline not initialized".to_string()
    })?;

    let result = pipeline.evaluate_plan(&plan).await;

    let payload = json!({
        "allowed": result.allowed,
        "blocked_by": result.blocked_by,
        "reasoning": result.reasoning,
        "rejected_steps": result.rejected_steps,
        "step_count": result.step_results.len(),
    });

    Ok(RpcOutcome::new(payload, vec![]))
}
