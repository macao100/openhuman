//! Semantic router RPC controller schemas and handlers.
//!
//! Exposes 3 RPC methods:
//! - `semantic_router.route_query` — find matching skills for a query
//! - `semantic_router.get_index_status` — current index state
//! - `semantic_router.rebuild_index` — force index rebuild

use std::collections::HashMap;

use crate::core::types::{ControllerSchema, FieldSchema, RegisteredController};
use crate::core::ControllerFuture;

use super::ops;

// ── Helpers ───────────────────────────────────────────────────────────────

fn to_json(value: serde_json::Value) -> Result<serde_json::Value, String> {
    Ok(value)
}

fn controller_schema(function: &str, description: &str) -> ControllerSchema {
    ControllerSchema {
        namespace: "semantic_router".to_string(),
        function: function.to_string(),
        description: description.to_string(),
        inputs: vec![],
        outputs: vec![],
        input_type_hint: None,
        output_type_hint: None,
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────

fn handle_route_query(params: HashMap<String, serde_json::Value>) -> ControllerFuture {
    Box::pin(async move {
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing 'query' parameter".to_string())?;

        let top_k = params
            .get("top_k")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize;

        let router = ops::global()
            .ok_or_else(|| "semantic router not initialised".to_string())?;

        let result = router.route_query(query, top_k);

        to_json(serde_json::to_value(&result).map_err(|e| format!("serialize: {e}"))?)
    })
}

fn handle_get_index_status(_params: HashMap<String, serde_json::Value>) -> ControllerFuture {
    Box::pin(async move {
        let router = ops::global()
            .ok_or_else(|| "semantic router not initialised".to_string())?;

        let index = router.index.read().map_err(|e| format!("lock: {e}"))?;
        let status = serde_json::json!({
            "skill_count": index.len(),
            "has_embedder": router.has_embedder,
        });

        to_json(status)
    })
}

fn handle_rebuild_index(_params: HashMap<String, serde_json::Value>) -> ControllerFuture {
    Box::pin(async move {
        let skills_store = crate::openhuman::skills::store::SkillsStore::load()
            .map_err(|e| format!("failed to load skills store: {e}"))?;

        ops::rebuild_index(&skills_store)?;

        to_json(serde_json::json!({"status": "ok", "skill_count": skills_store.installed().len() as u64}))
    })
}

// ── Public exports ────────────────────────────────────────────────────────

pub fn all_semantic_router_controller_schemas() -> Vec<ControllerSchema> {
    vec![
        controller_schema("route_query", "Find matching skills for a user query using embedding or keyword similarity"),
        controller_schema("get_index_status", "Current skill index state (count, embedder)"),
        controller_schema("rebuild_index", "Force rebuild of the skill embedding index"),
    ]
}

pub fn all_semantic_router_registered_controllers() -> Vec<RegisteredController> {
    vec![
        RegisteredController {
            schema: controller_schema(
                "route_query",
                "Find matching skills for a user query using embedding or keyword similarity",
            ),
            handler: handle_route_query,
        },
        RegisteredController {
            schema: controller_schema(
                "get_index_status",
                "Current skill index state (skill count, embedder status)",
            ),
            handler: handle_get_index_status,
        },
        RegisteredController {
            schema: controller_schema(
                "rebuild_index",
                "Force rebuild of the skill embedding index from the skills store",
            ),
            handler: handle_rebuild_index,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_have_unique_functions() {
        let schemas = all_semantic_router_controller_schemas();
        let mut names: Vec<&str> = schemas.iter().map(|s| s.function.as_str()).collect();
        names.sort();
        let len_before = names.len();
        names.dedup();
        assert_eq!(len_before, names.len());
    }

    #[test]
    fn registered_controllers_match_schemas_count() {
        assert_eq!(
            all_semantic_router_controller_schemas().len(),
            all_semantic_router_registered_controllers().len(),
        );
    }
}
