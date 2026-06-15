//! Controller schemas for the `dadou_project_context` namespace.
//!
//! Exposes three controllers:
//! - `upsert_fact` — create or update a project fact.
//! - `list_facts` — list facts, optionally filtered to one project.
//! - `delete_fact` — remove a single fact.
//!
//! All operations use the global `MemoryClient` singleton.

use chrono::Utc;
use serde_json::{json, Map, Value};

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};
use crate::openhuman::memory::project_context::{store, types::ProjectFact};
use crate::rpc::RpcOutcome;

/// Public schema exporter — returns all controller metadata for registration.
pub fn all_controller_schemas() -> Vec<ControllerSchema> {
    vec![
        schema("upsert_fact"),
        schema("list_facts"),
        schema("delete_fact"),
    ]
}

/// Public controller exporter — returns all registered handlers.
pub fn all_registered_controllers() -> Vec<RegisteredController> {
    vec![
        RegisteredController {
            schema: schema("upsert_fact"),
            handler: handle_upsert_fact,
        },
        RegisteredController {
            schema: schema("list_facts"),
            handler: handle_list_facts,
        },
        RegisteredController {
            schema: schema("delete_fact"),
            handler: handle_delete_fact,
        },
    ]
}

fn schema(function: &str) -> ControllerSchema {
    match function {
        "upsert_fact" => ControllerSchema {
            namespace: "dadou_project_context",
            function: "upsert_fact",
            description: "Create or update a project fact in dadou_project_context.",
            inputs: vec![
                FieldSchema {
                    name: "project_name",
                    ty: TypeSchema::String,
                    comment: "Project name, e.g. 'openhuman-backend'.",
                    required: true,
                },
                FieldSchema {
                    name: "fact_key",
                    ty: TypeSchema::String,
                    comment: "Unique fact key within the project, e.g. 'version'.",
                    required: true,
                },
                FieldSchema {
                    name: "fact_value",
                    ty: TypeSchema::String,
                    comment: "The fact value as a free-form string.",
                    required: true,
                },
                FieldSchema {
                    name: "category",
                    ty: TypeSchema::String,
                    comment: "Category label: 'goal', 'architecture', 'decision', 'issue', 'version', etc.",
                    required: false,
                },
                FieldSchema {
                    name: "source",
                    ty: TypeSchema::String,
                    comment: "How this fact was obtained: 'user', 'agent_analysis', etc.",
                    required: false,
                },
            ],
            outputs: vec![
                FieldSchema {
                    name: "key",
                    ty: TypeSchema::String,
                    comment: "Storage key in the dadou_project_context namespace.",
                    required: true,
                },
                FieldSchema {
                    name: "updated_at",
                    ty: TypeSchema::String,
                    comment: "RFC 3339 timestamp of when the fact was stored.",
                    required: true,
                },
            ],
        },
        "list_facts" => ControllerSchema {
            namespace: "dadou_project_context",
            function: "list_facts",
            description: "List all project facts, optionally filtered to one project.",
            inputs: vec![FieldSchema {
                name: "project",
                ty: TypeSchema::String,
                comment: "Optional project name filter. Omit to list all projects.",
                required: false,
            }],
            outputs: vec![FieldSchema {
                name: "facts",
                ty: TypeSchema::Array(Box::new(TypeSchema::Object {
                    fields: vec![
                        FieldSchema {
                            name: "project_name",
                            ty: TypeSchema::String,
                            comment: "Project name.",
                            required: true,
                        },
                        FieldSchema {
                            name: "fact_key",
                            ty: TypeSchema::String,
                            comment: "Fact key within the project.",
                            required: true,
                        },
                        FieldSchema {
                            name: "fact_value",
                            ty: TypeSchema::String,
                            comment: "The fact value.",
                            required: true,
                        },
                        FieldSchema {
                            name: "category",
                            ty: TypeSchema::String,
                            comment: "Category label.",
                            required: true,
                        },
                        FieldSchema {
                            name: "source",
                            ty: TypeSchema::String,
                            comment: "Source of the fact.",
                            required: true,
                        },
                        FieldSchema {
                            name: "updated_at",
                            ty: TypeSchema::String,
                            comment: "RFC 3339 timestamp of last update.",
                            required: true,
                        },
                    ],
                })),
                comment: "Array of project facts, newest-first.",
                required: true,
            }],
        },
        "delete_fact" => ControllerSchema {
            namespace: "dadou_project_context",
            function: "delete_fact",
            description: "Delete a single project fact.",
            inputs: vec![
                FieldSchema {
                    name: "project_name",
                    ty: TypeSchema::String,
                    comment: "Project name.",
                    required: true,
                },
                FieldSchema {
                    name: "fact_key",
                    ty: TypeSchema::String,
                    comment: "Fact key to delete.",
                    required: true,
                },
            ],
            outputs: vec![FieldSchema {
                name: "deleted",
                ty: TypeSchema::Bool,
                comment: "True if a fact was removed, false if it did not exist.",
                required: true,
            }],
        },
        _ => ControllerSchema {
            namespace: "dadou_project_context",
            function: "unknown",
            description: "Unknown dadou_project_context controller function.",
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

fn handle_upsert_fact(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let project_name = params
            .get("project_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing required param 'project_name'".to_string())?;
        let fact_key = params
            .get("fact_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing required param 'fact_key'".to_string())?;
        let fact_value = params
            .get("fact_value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing required param 'fact_value'".to_string())?;
        let category = params
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("general");
        let source = params
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("user");

        let client = crate::openhuman::memory::global::client()
            .map_err(|e| format!("memory client: {e}"))?;

        let fact = ProjectFact {
            project_name: project_name.to_string(),
            fact_key: fact_key.to_string(),
            fact_value: fact_value.to_string(),
            category: category.to_string(),
            source: source.to_string(),
            updated_at: Utc::now(),
        };

        store::upsert_fact(&client, &fact)
            .await
            .map_err(|e| format!("upsert fact: {e}"))?;

        let result = json!({
            "key": format!("{project_name}:{fact_key}"),
            "updated_at": fact.updated_at.to_rfc3339(),
        });

        to_json(RpcOutcome::new(result, vec![]))
    })
}

fn handle_list_facts(_params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let project = _params.get("project").and_then(|v| v.as_str());

        let client = crate::openhuman::memory::global::client()
            .map_err(|e| format!("memory client: {e}"))?;

        let facts = store::list_facts(&client, project)
            .await
            .map_err(|e| format!("list facts: {e}"))?;

        let fact_values: Vec<Value> = facts
            .into_iter()
            .map(|f| {
                json!({
                    "project_name": f.project_name,
                    "fact_key": f.fact_key,
                    "fact_value": f.fact_value,
                    "category": f.category,
                    "source": f.source,
                    "updated_at": f.updated_at.to_rfc3339(),
                })
            })
            .collect();

        let result = json!({ "facts": fact_values });
        to_json(RpcOutcome::new(result, vec![]))
    })
}

fn handle_delete_fact(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let project_name = params
            .get("project_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing required param 'project_name'".to_string())?;
        let fact_key = params
            .get("fact_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing required param 'fact_key'".to_string())?;

        let client = crate::openhuman::memory::global::client()
            .map_err(|e| format!("memory client: {e}"))?;

        let deleted = store::delete_fact(&client, project_name, fact_key)
            .await
            .map_err(|e| format!("delete fact: {e}"))?;

        to_json(RpcOutcome::new(json!({ "deleted": deleted }), vec![]))
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
                ctrl.schema.namespace, ctrl.schema.function
            );
        }
    }

    #[test]
    fn upsert_fact_schema_has_required_inputs() {
        let schemas = all_controller_schemas();
        let upsert = schemas
            .iter()
            .find(|s| s.function == "upsert_fact")
            .expect("upsert_fact schema must exist");

        let required_names: Vec<&str> = upsert
            .inputs
            .iter()
            .filter(|i| i.required)
            .map(|i| i.name)
            .collect();
        assert!(
            required_names.contains(&"project_name"),
            "project_name is required"
        );
        assert!(required_names.contains(&"fact_key"), "fact_key is required");
        assert!(
            required_names.contains(&"fact_value"),
            "fact_value is required"
        );
    }

    #[test]
    fn delete_fact_schema_has_required_inputs() {
        let schemas = all_controller_schemas();
        let delete = schemas
            .iter()
            .find(|s| s.function == "delete_fact")
            .expect("delete_fact schema must exist");

        let required_names: Vec<&str> = delete
            .inputs
            .iter()
            .filter(|i| i.required)
            .map(|i| i.name)
            .collect();
        assert!(required_names.contains(&"project_name"));
        assert!(required_names.contains(&"fact_key"));
    }

    #[test]
    fn list_facts_schema_has_no_required_inputs() {
        let schemas = all_controller_schemas();
        let list = schemas
            .iter()
            .find(|s| s.function == "list_facts")
            .expect("list_facts schema must exist");

        let required = list.inputs.iter().filter(|i| i.required).count();
        assert_eq!(required, 0, "list_facts has no required inputs");
    }
}
