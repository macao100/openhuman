//! Controller schemas for the `dadou_session_context` namespace.
//!
//! Exposes three controllers:
//! - `get_state` — returns the current saved session state (or null if none).
//! - `clear_state` — deletes the saved session state.
//! - `update_state` — upserts session state from provided fields.
//!
//! All operations use the global session context state and the memory DB
//! via the workspace directory stored at startup.

use serde_json::{json, Map, Value};

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};
use crate::openhuman::session_context;
use crate::openhuman::session_context::ops;
use crate::openhuman::session_context::store;
use crate::rpc::RpcOutcome;

/// Public schema exporter — returns all controller metadata for registration.
pub fn all_controller_schemas() -> Vec<ControllerSchema> {
    vec![
        schema("get_state"),
        schema("clear_state"),
        schema("update_state"),
    ]
}

/// Public controller exporter — returns all registered handlers.
pub fn all_registered_controllers() -> Vec<RegisteredController> {
    vec![
        RegisteredController {
            schema: schema("get_state"),
            handler: handle_get_state,
        },
        RegisteredController {
            schema: schema("clear_state"),
            handler: handle_clear_state,
        },
        RegisteredController {
            schema: schema("update_state"),
            handler: handle_update_state,
        },
    ]
}

fn schema(function: &str) -> ControllerSchema {
    match function {
        "get_state" => ControllerSchema {
            namespace: "dadou_session_context",
            function: "get_state",
            description: "Return the current saved session state (or null if none saved).",
            inputs: vec![],
            outputs: vec![FieldSchema {
                name: "state",
                ty: TypeSchema::Object {
                    fields: vec![
                        FieldSchema {
                            name: "active_project",
                            ty: TypeSchema::Option(Box::new(TypeSchema::String)),
                            comment: "Active project name.",
                            required: false,
                        },
                        FieldSchema {
                            name: "active_phase",
                            ty: TypeSchema::Option(Box::new(TypeSchema::String)),
                            comment: "Active phase within the project.",
                            required: false,
                        },
                        FieldSchema {
                            name: "last_topic",
                            ty: TypeSchema::Option(Box::new(TypeSchema::String)),
                            comment: "Last conversation topic summary.",
                            required: false,
                        },
                        FieldSchema {
                            name: "last_activity_at",
                            ty: TypeSchema::String,
                            comment: "ISO 8601 timestamp of last activity.",
                            required: true,
                        },
                        FieldSchema {
                            name: "version",
                            ty: TypeSchema::U64,
                            comment: "Schema version.",
                            required: true,
                        },
                    ],
                },
                comment: "The current session state, or null if none has been saved.",
                required: false,
            }],
        },
        "clear_state" => ControllerSchema {
            namespace: "dadou_session_context",
            function: "clear_state",
            description: "Delete the saved session state.",
            inputs: vec![],
            outputs: vec![FieldSchema {
                name: "cleared",
                ty: TypeSchema::Bool,
                comment: "True if a session was removed, false if none existed.",
                required: true,
            }],
        },
        "update_state" => ControllerSchema {
            namespace: "dadou_session_context",
            function: "update_state",
            description: "Update the current session state (upsert).",
            inputs: vec![
                FieldSchema {
                    name: "active_project",
                    ty: TypeSchema::Option(Box::new(TypeSchema::String)),
                    comment: "Active project name.",
                    required: false,
                },
                FieldSchema {
                    name: "active_phase",
                    ty: TypeSchema::Option(Box::new(TypeSchema::String)),
                    comment: "Active phase within the project.",
                    required: false,
                },
                FieldSchema {
                    name: "last_topic",
                    ty: TypeSchema::Option(Box::new(TypeSchema::String)),
                    comment: "Last conversation topic summary.",
                    required: false,
                },
            ],
            outputs: vec![FieldSchema {
                name: "updated",
                ty: TypeSchema::Bool,
                comment: "True if the state was upserted.",
                required: true,
            }],
        },
        _ => ControllerSchema {
            namespace: "dadou_session_context",
            function: "unknown",
            description: "Unknown dadou_session_context controller function.",
            inputs: vec![FieldSchema {
                name: "function",
                ty: TypeSchema::String,
                comment: "Unknown function requested for schema lookup.",
                required: true,
            }],
            outputs: vec![FieldSchema {
                name: "error",
                ty: TypeSchema::String,
                comment: "Lookup error details.",
                required: true,
            }],
        },
    }
}

fn handle_get_state(_params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let state = session_context::current_state();
        let result = json!({
            "state": {
                "active_project": state.active_project,
                "active_phase": state.active_phase,
                "last_topic": state.last_topic,
                "last_activity_at": state.last_activity_at,
                "version": state.version,
            }
        });
        to_json(RpcOutcome::new(result, vec![]))
    })
}

fn handle_clear_state(_params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let ws_dir = match session_context::workspace_dir() {
            Some(d) => d.clone(),
            None => return Err("workspace directory not initialised".to_string()),
        };

        let db_path = ws_dir.join("memory/memory.db");
        if !db_path.exists() {
            return to_json(RpcOutcome::new(json!({"cleared": false}), vec![]));
        }

        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| format!("open memory db: {e}"))?;
        store::init_table(&conn).map_err(|e| format!("init table: {e}"))?;
        let cleared = store::delete_session(&conn).map_err(|e| format!("delete session: {e}"))?;

        // Also reset the in-memory state
        session_context::update_current_state(session_context::types::SessionState::default());

        to_json(RpcOutcome::new(json!({"cleared": cleared}), vec![]))
    })
}

fn handle_update_state(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let mut state = session_context::current_state();

        if let Some(v) = params.get("active_project").and_then(|v| v.as_str()) {
            state.active_project = Some(v.to_string());
        }
        if let Some(v) = params.get("active_phase").and_then(|v| v.as_str()) {
            state.active_phase = Some(v.to_string());
        }
        if let Some(v) = params.get("last_topic").and_then(|v| v.as_str()) {
            state.last_topic = Some(v.to_string());
        }
        state.last_activity_at = chrono::Utc::now().to_rfc3339();

        session_context::update_current_state(state.clone());

        // Persist to DB immediately
        let ws_dir = match session_context::workspace_dir() {
            Some(d) => d.clone(),
            None => {
                // No workspace set yet — just update in-memory
                return to_json(RpcOutcome::new(json!({"updated": true}), vec![]));
            }
        };

        let db_path = ws_dir.join("memory/memory.db");
        if db_path.exists() {
            if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                let _ = ops::save_session_context(&conn);
            }
        }

        to_json(RpcOutcome::new(json!({"updated": true}), vec![]))
    })
}

fn to_json<T: serde::Serialize>(outcome: RpcOutcome<T>) -> Result<Value, String> {
    outcome.into_cli_compatible_json()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_schemas_count_matches_registered() {
        let schemas = all_controller_schemas();
        let controllers = all_registered_controllers();
        assert_eq!(
            schemas.len(),
            controllers.len(),
            "each schema needs a registered handler"
        );
        for ctrl in &controllers {
            let matching = schemas.iter().any(|s| {
                s.namespace == ctrl.schema.namespace && s.function == ctrl.schema.function
            });
            assert!(
                matching,
                "controller {}.{} has no matching schema",
                ctrl.schema.namespace,
                ctrl.schema.function
            );
        }
    }

    #[test]
    fn get_state_schema_has_no_inputs() {
        let schemas = all_controller_schemas();
        let get = schemas
            .iter()
            .find(|s| s.function == "get_state")
            .expect("get_state schema must exist");
        assert!(get.inputs.is_empty(), "get_state has no inputs");
    }

    #[test]
    fn clear_state_schema_has_no_inputs() {
        let schemas = all_controller_schemas();
        let clear = schemas
            .iter()
            .find(|s| s.function == "clear_state")
            .expect("clear_state schema must exist");
        assert!(clear.inputs.is_empty(), "clear_state has no inputs");
    }

    #[test]
    fn update_state_schema_has_optional_inputs() {
        let schemas = all_controller_schemas();
        let update = schemas
            .iter()
            .find(|s| s.function == "update_state")
            .expect("update_state schema must exist");
        let required = update.inputs.iter().filter(|i| i.required).count();
        assert_eq!(required, 0, "update_state has no required inputs");
    }

    #[test]
    fn get_state_handler_returns_state_from_current() {
        // Set a known state
        let state = session_context::types::SessionState {
            active_project: Some("test-project".to_string()),
            active_phase: None,
            last_topic: Some("Testing".to_string()),
            last_activity_at: "2026-06-04T12:00:00Z".to_string(),
            version: 1,
            extensions: serde_json::Value::Null,
        };
        session_context::update_current_state(state);

        let params = Map::new();
        let result = handle_get_state(params).await;
        assert!(result.is_ok(), "get_state should succeed: {:?}", result.err());

        let value = result.unwrap();
        let state_obj = value
            .get("state")
            .expect("response should have 'state' key");
        assert_eq!(
            state_obj.get("active_project").and_then(|v| v.as_str()),
            Some("test-project")
        );
        assert_eq!(
            state_obj.get("last_topic").and_then(|v| v.as_str()),
            Some("Testing")
        );
    }
}
