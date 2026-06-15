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
        schemas("n2_evaluate"),
        schemas("n3_status"),
        schemas("pipeline_status"),
        schemas("plan_validate"),
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
        RegisteredController {
            schema: schemas("n2_evaluate"),
            handler: handle_n2_evaluate,
        },
        RegisteredController {
            schema: schemas("n3_status"),
            handler: handle_n3_status,
        },
        RegisteredController {
            schema: schemas("pipeline_status"),
            handler: handle_pipeline_status,
        },
        RegisteredController {
            schema: schemas("plan_validate"),
            handler: handle_plan_validate,
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
                ty: TypeSchema::I64,
                comment: "Number of YAML rules loaded after reload.",
                required: true,
            }],
        },
        "n2_evaluate" => ControllerSchema {
            namespace: "guardian",
            function: "n2_evaluate",
            description: "Evaluate the N2 classifier for a given tool invocation (debugging).",
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
                comment: "N2Result with scores and decision.",
                required: true,
            }],
        },
        "n3_status" => ControllerSchema {
            namespace: "guardian",
            function: "n3_status",
            description: "Get N3 validator status (enabled, cache size, last latency).",
            inputs: vec![],
            outputs: vec![FieldSchema {
                name: "status",
                ty: TypeSchema::Json,
                comment: "N3 configuration and cache stats.",
                required: true,
            }],
        },
        "pipeline_status" => ControllerSchema {
            namespace: "guardian",
            function: "pipeline_status",
            description: "Get full pipeline status: N1 rules, N2 scores, N3 verdicts for the last invocation.",
            inputs: vec![],
            outputs: vec![FieldSchema {
                name: "status",
                ty: TypeSchema::Json,
                comment: "Pipeline configuration and last result.",
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
        "plan_validate" => ControllerSchema {
            namespace: "guardian",
            function: "plan_validate",
            description: "Validate a structured JSON plan through the full Guardian pipeline (N1+N2+N3).",
            inputs: vec![FieldSchema {
                name: "plan",
                ty: TypeSchema::Json,
                comment: "StructuredPlan JSON with goal and steps.",
                required: true,
            }],
            outputs: vec![FieldSchema {
                name: "result",
                ty: TypeSchema::Json,
                comment: "PlanValidationResult with allowed, blocked_by, reasoning, rejected_steps.",
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
    Box::pin(async move {
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
    Box::pin(async move {
        log::debug!("[guardian][rpc] evaluate enter");
        let tool_name = params
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let args = params.get("args").cloned().unwrap_or(Value::Null);
        let command = params.get("command").and_then(|v| v.as_str());
        let file_path = params.get("file_path").and_then(|v| v.as_str());

        match crate::openhuman::guardian::ops::evaluate_pipeline(
            tool_name, args, command, file_path,
        )
        .await
        {
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

fn handle_n2_evaluate(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        log::debug!("[guardian][rpc] n2_evaluate enter");
        let tool_name = params
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let args = params.get("args").cloned().unwrap_or(Value::Null);
        let command = params.get("command").and_then(|v| v.as_str());
        let file_path = params.get("file_path").and_then(|v| v.as_str());
        match crate::openhuman::guardian::ops::n2_evaluate(tool_name, args, command, file_path)
            .await
        {
            Ok(outcome) => {
                log::debug!("[guardian][rpc] n2_evaluate ok");
                to_json(outcome)
            }
            Err(err) => {
                log::warn!("[guardian][rpc] n2_evaluate failed: {err}");
                Err(err)
            }
        }
    })
}

fn handle_n3_status(_params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async {
        log::debug!("[guardian][rpc] n3_status enter");
        match crate::openhuman::guardian::ops::n3_status().await {
            Ok(outcome) => {
                log::debug!("[guardian][rpc] n3_status ok");
                to_json(outcome)
            }
            Err(err) => {
                log::warn!("[guardian][rpc] n3_status failed: {err}");
                Err(err)
            }
        }
    })
}

fn handle_pipeline_status(_params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async {
        log::debug!("[guardian][rpc] pipeline_status enter");
        match crate::openhuman::guardian::ops::pipeline_status().await {
            Ok(outcome) => {
                log::debug!("[guardian][rpc] pipeline_status ok");
                to_json(outcome)
            }
            Err(err) => {
                log::warn!("[guardian][rpc] pipeline_status failed: {err}");
                Err(err)
            }
        }
    })
}

fn handle_plan_validate(params: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move {
        log::debug!("[guardian][rpc] plan_validate enter");
        let plan_value = params.get("plan").cloned().unwrap_or(Value::Null);
        match crate::openhuman::guardian::ops::validate_plan(plan_value).await {
            Ok(outcome) => {
                log::debug!("[guardian][rpc] plan_validate ok");
                to_json(outcome)
            }
            Err(err) => {
                log::warn!("[guardian][rpc] plan_validate failed: {err}");
                Err(err)
            }
        }
    })
}

fn to_json<T: serde::Serialize>(outcome: RpcOutcome<T>) -> Result<Value, String> {
    outcome.into_cli_compatible_json()
}
