//! Rollback controller schemas — stubs for Plan 06.
//!
//! Exposes `rollback.undo_last`, `rollback.undo_before`, and
//! `rollback.history_list` JSON-RPC methods.  The actual undo logic will
//! be implemented in Plan 06; currently each handler returns a
//! "not yet implemented" placeholder.

use serde_json::{Map, Value};

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};

const NAMESPACE: &str = "rollback";

/// All rollback controller schemas, used by the registry to advertise
/// inputs/outputs to CLI + JSON-RPC consumers.
pub fn all_controller_schemas() -> Vec<ControllerSchema> {
    vec![
        schemas("undo_last"),
        schemas("undo_before"),
        schemas("history_list"),
    ]
}

/// Registered rollback controllers (schema + handler pairs) wired into
/// `core::all`.
pub fn all_registered_controllers() -> Vec<RegisteredController> {
    vec![
        RegisteredController {
            schema: schemas("undo_last"),
            handler: handle_undo_last,
        },
        RegisteredController {
            schema: schemas("undo_before"),
            handler: handle_undo_before,
        },
        RegisteredController {
            schema: schemas("history_list"),
            handler: handle_history_list,
        },
    ]
}

/// Build a [`ControllerSchema`] for the given function name.
pub fn schemas(function: &str) -> ControllerSchema {
    match function {
        "undo_last" => ControllerSchema {
            namespace: NAMESPACE,
            function: "undo_last",
            description: "Undo the most recent file modification.",
            inputs: vec![],
            outputs: vec![
                FieldSchema {
                    name: "action_id",
                    ty: TypeSchema::String,
                    comment: "UUID of the undone action.",
                    required: true,
                },
                FieldSchema {
                    name: "file_path",
                    ty: TypeSchema::String,
                    comment: "Path of the restored file.",
                    required: true,
                },
                FieldSchema {
                    name: "restored",
                    ty: TypeSchema::Bool,
                    comment: "Whether the file was successfully restored.",
                    required: true,
                },
            ],
        },
        "undo_before" => ControllerSchema {
            namespace: NAMESPACE,
            function: "undo_before",
            description: "Roll back all modifications before a given timestamp.",
            inputs: vec![FieldSchema {
                name: "timestamp",
                ty: TypeSchema::String,
                comment: "ISO 8601 cutoff timestamp.",
                required: true,
            }],
            outputs: vec![
                FieldSchema {
                    name: "restored_count",
                    ty: TypeSchema::I64,
                    comment: "Number of files successfully restored.",
                    required: true,
                },
                FieldSchema {
                    name: "failed_count",
                    ty: TypeSchema::I64,
                    comment: "Number of files that could not be restored.",
                    required: true,
                },
            ],
        },
        "history_list" => ControllerSchema {
            namespace: NAMESPACE,
            function: "history_list",
            description: "List recent rollback history entries.",
            inputs: vec![FieldSchema {
                name: "limit",
                ty: TypeSchema::I64,
                comment: "Maximum number of entries to return (default 20).",
                required: false,
            }],
            outputs: vec![FieldSchema {
                name: "entries",
                ty: TypeSchema::Array(Box::new(TypeSchema::Json)),
                comment: "List of rollback entries.",
                required: true,
            }],
        },
        _ => ControllerSchema {
            namespace: NAMESPACE,
            function: "unknown",
            description: "Unknown rollback controller function.",
            inputs: vec![],
            outputs: vec![],
        },
    }
}

// ── Stub handlers (implemented in Plan 06) ──────────────────────────────

fn handle_undo_last(_params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async {
        log::warn!("[rollback] undo_last — not yet implemented");
        Err("undo_last is not yet implemented (planned for security phase plan 06)".into())
    })
}

fn handle_undo_before(_params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async {
        log::warn!("[rollback] undo_before — not yet implemented");
        Err("undo_before is not yet implemented (planned for security phase plan 06)".into())
    })
}

fn handle_history_list(_params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async {
        log::warn!("[rollback] history_list — not yet implemented");
        Err("history_list is not yet implemented (planned for security phase plan 06)".into())
    })
}
