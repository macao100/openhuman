//! Controller schemas for the `dadou_contradiction` namespace.
//!
//! Exposes two controllers:
//! - `check` — run contradiction detection for a given value in a namespace.
//! - `resolve` — resolve a detected contradiction (replace / merge / dismiss).
//!
//! All operations use the global `MemoryClient` singleton for storage.

use serde_json::{json, Map, Value};

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};
use crate::openhuman::memory::contradiction::{
    check_for_contradictions, resolve_contradiction, ContradictionAction, ContradictionResolution,
};
use crate::rpc::RpcOutcome;

/// Default vector similarity threshold for contradiction detection (0.6).
/// Mirrors `CONTRADICTION_SIMILARITY` from `preferences.rs`.
const DEFAULT_MIN_SIMILARITY: f64 = 0.6;

/// Public schema exporter — returns all controller metadata for registration.
pub fn all_controller_schemas() -> Vec<ControllerSchema> {
    vec![schema("check"), schema("resolve")]
}

/// Public controller exporter — returns all registered handlers.
pub fn all_registered_controllers() -> Vec<RegisteredController> {
    vec![
        RegisteredController {
            schema: schema("check"),
            handler: handle_check,
        },
        RegisteredController {
            schema: schema("resolve"),
            handler: handle_resolve,
        },
    ]
}

fn schema(function: &str) -> ControllerSchema {
    match function {
        "check" => ControllerSchema {
            namespace: "dadou_contradiction",
            function: "check",
            description: "Check if a new value contradicts any existing verified memory entry.",
            inputs: vec![
                FieldSchema {
                    name: "namespace",
                    ty: TypeSchema::String,
                    comment: "Memory namespace to check, e.g. 'user_pref_general'.",
                    required: true,
                },
                FieldSchema {
                    name: "value",
                    ty: TypeSchema::String,
                    comment: "The new value to check for contradictions.",
                    required: true,
                },
                FieldSchema {
                    name: "min_similarity",
                    ty: TypeSchema::F64,
                    comment: "Minimum vector similarity threshold (default 0.6).",
                    required: false,
                },
            ],
            outputs: vec![FieldSchema {
                name: "report",
                ty: TypeSchema::Object {
                    fields: vec![
                        FieldSchema {
                            name: "has_contradictions",
                            ty: TypeSchema::Bool,
                            comment: "True if at least one contradiction candidate was found.",
                            required: true,
                        },
                        FieldSchema {
                            name: "candidates",
                            ty: TypeSchema::Array(Box::new(TypeSchema::Object {
                                fields: vec![
                                    FieldSchema {
                                        name: "existing_key",
                                        ty: TypeSchema::String,
                                        comment: "Key of the existing verified entry.",
                                        required: true,
                                    },
                                    FieldSchema {
                                        name: "existing_content",
                                        ty: TypeSchema::String,
                                        comment: "Content of the existing verified entry.",
                                        required: true,
                                    },
                                    FieldSchema {
                                        name: "new_value",
                                        ty: TypeSchema::String,
                                        comment: "The new value that conflicts.",
                                        required: true,
                                    },
                                    FieldSchema {
                                        name: "similarity",
                                        ty: TypeSchema::F64,
                                        comment: "Vector similarity between the entries.",
                                        required: true,
                                    },
                                ],
                            })),
                            comment: "Array of contradiction candidates.",
                            required: true,
                        },
                        FieldSchema {
                            name: "checked_against",
                            ty: TypeSchema::U64,
                            comment: "Number of existing entries checked.",
                            required: true,
                        },
                        FieldSchema {
                            name: "elapsed_ms",
                            ty: TypeSchema::U64,
                            comment: "Detection wall-clock time in milliseconds.",
                            required: true,
                        },
                    ],
                },
                comment: "Contradiction detection report.",
                required: true,
            }],
        },
        "resolve" => ControllerSchema {
            namespace: "dadou_contradiction",
            function: "resolve",
            description: "Resolve a contradiction by replacing, merging, or dismissing it.",
            inputs: vec![
                FieldSchema {
                    name: "namespace",
                    ty: TypeSchema::String,
                    comment: "Memory namespace of the existing entry.",
                    required: true,
                },
                FieldSchema {
                    name: "existing_key",
                    ty: TypeSchema::String,
                    comment: "Key of the existing entry to resolve.",
                    required: true,
                },
                FieldSchema {
                    name: "action",
                    ty: TypeSchema::String,
                    comment: "Resolution action: 'replace', 'merge', or 'dismiss'.",
                    required: true,
                },
                FieldSchema {
                    name: "new_value",
                    ty: TypeSchema::String,
                    comment: "The new value involved in the contradiction.",
                    required: true,
                },
            ],
            outputs: vec![
                FieldSchema {
                    name: "status",
                    ty: TypeSchema::String,
                    comment: "Human-readable status of the resolution.",
                    required: true,
                },
                FieldSchema {
                    name: "action",
                    ty: TypeSchema::String,
                    comment: "The action that was applied.",
                    required: true,
                },
            ],
        },
        _ => ControllerSchema {
            namespace: "dadou_contradiction",
            function: "unknown",
            description: "Unknown dadou_contradiction controller function.",
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

/// Parse action from params — accepts snake_case string.
fn parse_action(value: &str) -> Result<ContradictionAction, String> {
    value.parse::<ContradictionAction>()
        .map_err(|e| format!("invalid action '{value}': {e}"))
}

pub fn handle_check(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let namespace = params
            .get("namespace")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing required param 'namespace'".to_string())?;
        let value = params
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing required param 'value'".to_string())?;
        let min_similarity = params
            .get("min_similarity")
            .and_then(|v| v.as_f64())
            .unwrap_or(DEFAULT_MIN_SIMILARITY);

        let memory = get_memory_handle()?;

        // For the check controller we don't have a new_provenance, so we pass
        // `None` — the detector will skip provenance filtering and run the
        // check against all semantically-close entries regardless of their
        // confidence.
        let report = check_for_contradictions(
            &memory,
            namespace,
            value,
            None,
            min_similarity,
        )
        .await
        .map_err(|e| format!("contradiction check: {e}"))?;

        let candidates_json: Vec<Value> = report
            .candidates
            .into_iter()
            .map(|c| {
                json!({
                    "existing_key": c.existing_entry.key,
                    "existing_content": c.existing_entry.content,
                    "new_value": c.new_value,
                    "similarity": c.similarity,
                })
            })
            .collect();

        let result = json!({
            "report": {
                "has_contradictions": !candidates_json.is_empty(),
                "candidates": candidates_json,
                "checked_against": report.checked_against,
                "elapsed_ms": report.elapsed_ms,
            }
        });

        to_json(RpcOutcome::new(result, vec![]))
    })
}

pub fn handle_resolve(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let namespace = params
            .get("namespace")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing required param 'namespace'".to_string())?;
        let existing_key = params
            .get("existing_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing required param 'existing_key'".to_string())?;
        let action_str = params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing required param 'action'".to_string())?;
        let new_value = params
            .get("new_value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing required param 'new_value'".to_string())?;

        let action = parse_action(action_str)?;
        let memory = get_memory_handle()?;

        let resolution = ContradictionResolution {
            action,
            namespace: namespace.to_string(),
            existing_key: existing_key.to_string(),
            new_value: new_value.to_string(),
        };

        let status = resolve_contradiction(&memory, &resolution)
            .await
            .map_err(|e| format!("resolve contradiction: {e}"))?;

        let result = json!({
            "status": status,
            "action": resolution.action.as_str(),
        });

        to_json(RpcOutcome::new(result, vec![]))
    })
}

/// Obtain the global memory handle (Arc<dyn Memory>) for the RPC handler.
fn get_memory_handle() -> Result<std::sync::Arc<dyn Memory>, String> {
    let client = crate::openhuman::memory::global::client()
        .map_err(|e| format!("memory client: {e}"))?;
    Ok(client.memory_handle())
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
    fn check_schema_has_required_inputs() {
        let schemas = all_controller_schemas();
        let check = schemas
            .iter()
            .find(|s| s.function == "check")
            .expect("check schema must exist");

        let required_names: Vec<&str> = check
            .inputs
            .iter()
            .filter(|i| i.required)
            .map(|i| i.name)
            .collect();
        assert!(required_names.contains(&"namespace"));
        assert!(required_names.contains(&"value"));
    }

    #[test]
    fn resolve_schema_has_required_inputs() {
        let schemas = all_controller_schemas();
        let resolve = schemas
            .iter()
            .find(|s| s.function == "resolve")
            .expect("resolve schema must exist");

        let required_names: Vec<&str> = resolve
            .inputs
            .iter()
            .filter(|i| i.required)
            .map(|i| i.name)
            .collect();
        assert!(required_names.contains(&"namespace"));
        assert!(required_names.contains(&"existing_key"));
        assert!(required_names.contains(&"action"));
        assert!(required_names.contains(&"new_value"));
    }

    #[test]
    fn parse_action_accepts_valid_actions() {
        assert!(parse_action("replace").is_ok());
        assert!(parse_action("merge").is_ok());
        assert!(parse_action("dismiss").is_ok());
    }

    #[test]
    fn parse_action_rejects_invalid_action() {
        assert!(parse_action("delete").is_err());
        assert!(parse_action("").is_err());
        assert!(parse_action("overwrite").is_err());
    }
}
