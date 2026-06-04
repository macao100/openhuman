//! Controller schemas and handlers for Guardian N1 introspection RPC.

use serde_json::{Map, Value};

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};
use crate::rpc::RpcOutcome;

pub fn all_controller_schemas() -> Vec<ControllerSchema> {
    vec![
        schemas("rules_list"),
        schemas("rules_reload"),
        schemas("evaluate"),
    ]
}

pub fn all_registered_controllers() -> Vec<RegisteredController> {
    vec![
        RegisteredController {
            schema: schemas("rules_list"),
            handler: handle_rules_list,
        },
        RegisteredController {
            schema: schemas("rules_reload"),
            handler: handle_rules_reload,
        },
        RegisteredController {
            schema: schemas("evaluate"),
            handler: handle_evaluate,
        },
    ]
}

pub fn schemas(function: &str) -> ControllerSchema {
    match function {
        "rules_list" => ControllerSchema {
            namespace: "guardian",
            function: "rules_list",
            description: "List all active Guardian N1 rules (Rust compiled + YAML loaded).",
            inputs: vec![],
            outputs: vec![FieldSchema {
                name: "rules",
                ty: TypeSchema::Json,
                comment: "Array of active rule names and their types.",
                required: true,
            }],
        },
        "rules_reload" => ControllerSchema {
            namespace: "guardian",
            function: "rules_reload",
            description: "Reload YAML rules from the configured rules file.",
            inputs: vec![FieldSchema {
                name: "yaml_path",
                ty: TypeSchema::String,
                comment: "Optional path to YAML rules file (uses default if omitted).",
                required: false,
            }],
            outputs: vec![FieldSchema {
                name: "loaded_count",
                ty: TypeSchema::Int,
                comment: "Number of YAML rules loaded after reload.",
                required: true,
            }],
        },
        "evaluate" => ControllerSchema {
            namespace: "guardian",
            function: "evaluate",
            description: "Evaluate the N1 pipeline for a given tool invocation (debugging).",
            inputs: vec![
                FieldSchema {
                    name: "tool_name",
                    ty: TypeSchema::String,
                    comment: "Tool name (e.g. 'file_write', 'shell').",
                    required: true,
                },
                FieldSchema {
                    name: "args",
                    ty: TypeSchema::Json,
                    comment: "JSON arguments for the tool.",
                    required: true,
                },
                FieldSchema {
                    name: "command",
                    ty: TypeSchema::String,
                    comment: "Optional shell command string.",
                    required: false,
                },
                FieldSchema {
                    name: "file_path",
                    ty: TypeSchema::String,
                    comment: "Optional file path.",
                    required: false,
                },
            ],
            outputs: vec![FieldSchema {
                name: "result",
                ty: TypeSchema::Json,
                comment: "N1Result with allowed, rule_results, and latency_us.",
                required: true,
            }],
        },
        _ => ControllerSchema {
            namespace: "guardian",
            function: "unknown",
            description: "Unknown guardian controller function.",
            inputs: vec![],
            outputs: vec![],
        },
    }
}

// ── Handlers ──────────────────────────────────────────────────────────

fn handle_rules_list(_params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async {
        log::debug!("[guardian][rpc] rules_list enter");
        match crate::openhuman::guardian::ops::get_active_rules().await {
            Ok(outcome) => {
                log::debug!("[guardian][rpc] rules_list ok");
                to_json(outcome)
            }
            Err(err) => {
                log::warn!("[guardian][rpc] rules_list failed: {err}");
                Err(err)
            }
        }
    })
}

fn handle_rules_reload(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async {
        log::debug!("[guardian][rpc] rules_reload enter");
        let yaml_path = params.get("yaml_path").and_then(|v| v.as_str());
        match crate::openhuman::guardian::ops::reload_yaml_rules(yaml_path).await {
            Ok(outcome) => {
                log::debug!("[guardian][rpc] rules_reload ok");
                to_json(outcome)
            }
            Err(err) => {
                log::warn!("[guardian][rpc] rules_reload failed: {err}");
                Err(err)
            }
        }
    })
}

fn handle_evaluate(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async {
        log::debug!("[guardian][rpc] evaluate enter");
        let tool_name = params
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let args = params.get("args").cloned().unwrap_or(Value::Null);
        let command = params.get("command").and_then(|v| v.as_str());
        let file_path = params.get("file_path").and_then(|v| v.as_str());

        match crate::openhuman::guardian::ops::evaluate_pipeline(tool_name, args, command, file_path).await {
            Ok(outcome) => {
                log::debug!("[guardian][rpc] evaluate ok");
                to_json(outcome)
            }
            Err(err) => {
                log::warn!("[guardian][rpc] evaluate failed: {err}");
                Err(err)
            }
        }
    })
}

fn to_json<T: serde::Serialize>(outcome: RpcOutcome<T>) -> Result<Value, String> {
    outcome.into_cli_compatible_json()
}
