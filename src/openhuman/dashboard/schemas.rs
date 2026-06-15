//! Dashboard RPC controller schemas and handlers.
//!
//! Exposes 4 RPC methods:
//! - `dashboard.get_stats` — aggregate event counts
//! - `dashboard.get_recent_events` — recent event timeline
//! - `dashboard.get_skills` — installed skill summaries
//! - `dashboard.get_memory_stats` — memory event statistics

use serde_json::Map;

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema};
use crate::openhuman::dashboard::store;

/// Local wire-format for a skill displayed in the dashboard.
#[derive(serde::Serialize)]
struct SkillSummary {
    name: String,
    version: String,
    enabled: bool,
    gpg_verified: bool,
    description: Option<String>,
}

// ── Schema helpers ────────────────────────────────────────────────────────

fn to_json(value: serde_json::Value) -> Result<serde_json::Value, String> {
    Ok(value)
}

fn deserialize_params<T: serde::de::DeserializeOwned>(
    params: Map<String, serde_json::Value>,
) -> Result<T, String> {
    let value = serde_json::to_value(params).map_err(|e| format!("serialize params: {e}"))?;
    serde_json::from_value(value).map_err(|e| format!("invalid params: {e}"))
}

fn controller_schema(
    function: &'static str,
    description: &'static str,
    inputs: Vec<FieldSchema>,
    outputs: Vec<FieldSchema>,
) -> ControllerSchema {
    ControllerSchema {
        namespace: "dashboard",
        function,
        description,
        inputs,
        outputs,
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────

fn handle_get_stats(_params: Map<String, serde_json::Value>) -> ControllerFuture {
    Box::pin(async move {
        let store = store::global().ok_or_else(|| "dashboard store not initialised".to_string())?;
        let store = store.lock().map_err(|e| format!("lock: {e}"))?;

        let mut stats = store.get_stats().map_err(|e| format!("get_stats: {e}"))?;

        // Augment with active skill count from the skills toml store.
        match crate::openhuman::skills::store::SkillsStore::load() {
            Ok(skills_store) => {
                stats.active_skill_count = skills_store
                    .installed()
                    .iter()
                    .filter(|s| s.enabled)
                    .count() as u64;
            }
            Err(e) => {
                log::debug!("[dashboard] could not load skills store for stats: {e}");
            }
        }

        to_json(serde_json::to_value(&stats).map_err(|e| format!("serialize: {e}"))?)
    })
}

fn handle_get_recent_events(params: Map<String, serde_json::Value>) -> ControllerFuture {
    Box::pin(async move {
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50)
            .min(500);

        let kind_filter = params
            .get("kind")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let store = store::global().ok_or_else(|| "dashboard store not initialised".to_string())?;
        let store = store.lock().map_err(|e| format!("lock: {e}"))?;

        let events = store
            .list_recent(limit, kind_filter.as_deref())
            .map_err(|e| format!("list_recent: {e}"))?;

        to_json(serde_json::to_value(&events).map_err(|e| format!("serialize: {e}"))?)
    })
}

fn handle_get_skills(_params: Map<String, serde_json::Value>) -> ControllerFuture {
    Box::pin(async move {
        let skills = match crate::openhuman::skills::store::SkillsStore::load() {
            Ok(skills_store) => skills_store
                .installed()
                .iter()
                .map(|s| SkillSummary {
                    name: s.name.clone(),
                    version: s.version.clone(),
                    enabled: s.enabled,
                    gpg_verified: s.gpg_fingerprint.is_some(),
                    description: None, // Could be enriched from manifest later
                })
                .collect::<Vec<_>>(),
            Err(e) => {
                log::warn!("[dashboard] could not load skills store: {e}");
                Vec::new()
            }
        };

        to_json(serde_json::to_value(&skills).map_err(|e| format!("serialize: {e}"))?)
    })
}

fn handle_get_memory_stats(_params: Map<String, serde_json::Value>) -> ControllerFuture {
    Box::pin(async move {
        let store = store::global().ok_or_else(|| "dashboard store not initialised".to_string())?;
        let store = store.lock().map_err(|e| format!("lock: {e}"))?;

        // Count events using a simple in-memory aggregation from recent events.
        let events = store
            .list_recent(10_000, None)
            .map_err(|e| format!("list_recent: {e}"))?;

        let memory_stored = events.iter().filter(|e| e.kind == "memory_stored").count() as u64;
        let memory_recalled = events
            .iter()
            .filter(|e| e.kind == "memory_recalled")
            .count() as u64;
        let total_memories = memory_stored + memory_recalled;

        let result = serde_json::json!({
            "total_memory_events": total_memories,
            "stored": memory_stored,
            "recalled": memory_recalled,
        });

        to_json(result)
    })
}

// ── Public exports (consumed by `src/core/all.rs`) ────────────────────────

pub fn all_dashboard_controller_schemas() -> Vec<ControllerSchema> {
    vec![
        controller_schema(
            "get_stats",
            "Aggregate dashboard statistics",
            vec![],
            vec![],
        ),
        controller_schema(
            "get_recent_events",
            "Recent dashboard events, optionally filtered by kind",
            vec![],
            vec![],
        ),
        controller_schema("get_skills", "Installed skills with status", vec![], vec![]),
        controller_schema(
            "get_memory_stats",
            "Memory event statistics",
            vec![],
            vec![],
        ),
    ]
}

pub fn all_dashboard_registered_controllers() -> Vec<RegisteredController> {
    vec![
        RegisteredController {
            schema: controller_schema(
                "get_stats",
                "Aggregate dashboard statistics including Guardian, tool, and memory counts",
                vec![],
                vec![],
            ),
            handler: handle_get_stats,
        },
        RegisteredController {
            schema: controller_schema(
                "get_recent_events",
                "Recent dashboard events, optionally filtered by kind",
                vec![],
                vec![],
            ),
            handler: handle_get_recent_events,
        },
        RegisteredController {
            schema: controller_schema(
                "get_skills",
                "Installed skills with name, version, and enabled status",
                vec![],
                vec![],
            ),
            handler: handle_get_skills,
        },
        RegisteredController {
            schema: controller_schema(
                "get_memory_stats",
                "Memory event statistics (stored, recalled, total)",
                vec![],
                vec![],
            ),
            handler: handle_get_memory_stats,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_have_unique_functions() {
        let schemas = all_dashboard_controller_schemas();
        let mut names: Vec<&str> = schemas.iter().map(|s| s.function.as_str()).collect();
        names.sort();
        let len_before = names.len();
        names.dedup();
        assert_eq!(
            len_before,
            names.len(),
            "duplicate function names in schemas"
        );
    }

    #[test]
    fn registered_controllers_match_schemas_count() {
        let schemas = all_dashboard_controller_schemas();
        let controllers = all_dashboard_registered_controllers();
        assert_eq!(
            schemas.len(),
            controllers.len(),
            "schema count must match registered controller count"
        );
    }

    #[test]
    fn all_registered_functions_are_in_schemas() {
        let schemas = all_dashboard_controller_schemas();
        let controllers = all_dashboard_registered_controllers();
        for c in &controllers {
            assert!(
                schemas.iter().any(|s| s.function == c.schema.function),
                "registered function '{}' missing from schemas",
                c.schema.function
            );
        }
    }
}
