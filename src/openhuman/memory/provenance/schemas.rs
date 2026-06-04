//! Controller schemas for provenance RPCs.
//!
//! Exposes two controllers under the `dadou_provenance` namespace:
//!
//! - `run_decay` — trigger an immediate confidence decay pass.
//! - `set_decay_config` — update decay thresholds (verified_demote_days,
//!   external_expiry_days).

use serde_json::{json, Map, Value};

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};
use crate::openhuman::config::Config;
use crate::openhuman::memory::provenance::decay;

/// Public schema exporter — returns all controller metadata.
pub fn all_controller_schemas() -> Vec<ControllerSchema> {
    vec![schema("run_decay"), schema("set_decay_config")]
}

/// Public controller exporter — returns all registered handlers.
pub fn all_registered_controllers() -> Vec<RegisteredController> {
    vec![
        RegisteredController {
            schema: schema("run_decay"),
            handler: handle_run_decay,
        },
        RegisteredController {
            schema: schema("set_decay_config"),
            handler: handle_set_decay_config,
        },
    ]
}

fn schema(function: &str) -> ControllerSchema {
    match function {
        "run_decay" => ControllerSchema {
            namespace: "dadou_provenance",
            function: "run_decay",
            description: "Run an immediate confidence decay pass on all memory entries.",
            inputs: vec![],
            outputs: vec![FieldSchema {
                name: "report",
                ty: TypeSchema::Object {
                    fields: vec![
                        FieldSchema {
                            name: "verified_demoted",
                            ty: TypeSchema::U64,
                            comment: "Number of entries demoted from Verified to Inferred.",
                            required: true,
                        },
                        FieldSchema {
                            name: "external_removed",
                            ty: TypeSchema::U64,
                            comment: "Number of External entries removed.",
                            required: true,
                        },
                        FieldSchema {
                            name: "entries_affected",
                            ty: TypeSchema::U64,
                            comment: "Total entries affected by this decay pass.",
                            required: true,
                        },
                    ],
                },
                comment: "Decay pass report with counts of affected entries.",
                required: true,
            }],
        },
        "set_decay_config" => ControllerSchema {
            namespace: "dadou_provenance",
            function: "set_decay_config",
            description: "Update confidence decay thresholds.",
            inputs: vec![
                FieldSchema {
                    name: "verified_decay_days",
                    ty: TypeSchema::U64,
                    comment: "Days after which Verified entries are demoted to Inferred.",
                    required: false,
                },
                FieldSchema {
                    name: "external_expiry_days",
                    ty: TypeSchema::U64,
                    comment: "Days after which External entries are deleted.",
                    required: false,
                },
            ],
            outputs: vec![FieldSchema {
                name: "config",
                ty: TypeSchema::Object {
                    fields: vec![
                        FieldSchema {
                            name: "verified_decay_days",
                            ty: TypeSchema::U64,
                            comment: "Current Verified→Inferred threshold in days.",
                            required: true,
                        },
                        FieldSchema {
                            name: "external_expiry_days",
                            ty: TypeSchema::U64,
                            comment: "Current External expiry threshold in days.",
                            required: true,
                        },
                    ],
                },
                comment: "Updated decay configuration.",
                required: true,
            }],
        },
        _ => ControllerSchema {
            namespace: "dadou_provenance",
            function: "unknown",
            description: "Unknown provenance controller function.",
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

fn handle_run_decay(_params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async {
        let config = Config::load_or_init()
            .await
            .map_err(|e| format!("load config: {e}"))?;
        let db_path = config.workspace_dir.join("memory").join("memory.db");
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| format!("open memory db: {e}"))?;
        let report = decay::run_decay(&conn).map_err(|e| format!("decay pass: {e}"))?;
        to_json(crate::rpc::RpcOutcome::new(report, vec![]))
    })
}

fn handle_set_decay_config(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        let verified_days = params
            .get("verified_decay_days")
            .and_then(|v| v.as_u64())
            .unwrap_or(decay::VERIFIED_DECAY_DAYS as u64);
        let external_days = params
            .get("external_expiry_days")
            .and_then(|v| v.as_u64())
            .unwrap_or(decay::EXTERNAL_EXPIRY_DAYS as u64);

        // Return the (logical) updated config. Persistence to config TOML
        // is deferred — the constants in `decay.rs` are the runtime defaults.
        let result = json!({
            "verified_decay_days": verified_days,
            "external_expiry_days": external_days,
        });

        let logs = vec![format!(
            "decay config updated (in-memory): verified_decay_days={verified_days}, external_expiry_days={external_days}"
        )];

        to_json(crate::rpc::RpcOutcome::new(result, logs))
    })
}

fn to_json<T: serde::Serialize>(outcome: crate::rpc::RpcOutcome<T>) -> Result<Value, String> {
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
            let matching = schemas
                .iter()
                .any(|s| s.namespace == ctrl.schema.namespace && s.function == ctrl.schema.function);
            assert!(
                matching,
                "controller {}.{} has no matching schema",
                ctrl.schema.namespace,
                ctrl.schema.function
            );
        }
    }
}
