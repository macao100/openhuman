use crate::openhuman::agent::cost::TurnCost;
use crate::openhuman::agent::multimodal;
use crate::openhuman::agent::progress::AgentProgress;
use crate::openhuman::agent::stop_hooks::{current_stop_hooks, StopDecision, TurnState};
use crate::openhuman::inference::provider::{
    ChatMessage, ChatRequest, Provider, ProviderCapabilityError, ProviderDelta,
};
use crate::openhuman::tools::policy::{DefaultToolPolicy, PolicyDecision, ToolPolicy};
use crate::openhuman::tools::traits::ToolScope;
use crate::openhuman::tools::Tool;
use anyhow::Result;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::Write as _;

use super::credentials::scrub_credentials;
use super::memory_context_safety::wrap_external_data;
use super::parse::{build_native_assistant_history, parse_structured_tool_calls, parse_tool_calls};
use super::payload_summarizer::PayloadSummarizer;
use crate::openhuman::context::guard::{ContextCheckResult, ContextGuard};
use crate::openhuman::inference::model_context::context_window_for_model;

use super::token_budget::trim_chat_messages_to_budget;

/// Minimum characters per chunk when relaying LLM text to a streaming draft.
const STREAM_CHUNK_MIN_CHARS: usize = 80;

/// Default maximum agentic tool-use iterations per user message to prevent runaway loops.
/// Used as a safe fallback when `max_tool_iterations` is unset or configured as zero.
pub(crate) const DEFAULT_MAX_TOOL_ITERATIONS: usize = 10;

/// Repeated-failure circuit breaker. The plain iteration cap lets an agent grind
/// the same dead-end (e.g. re-running `pip install` when there is no pip) until
/// `max_iterations`, then return an opaque `MaxIterationsExceeded` that the caller
/// just re-spawns — losing the failure context. These thresholds let the loop bail
/// EARLY with a root-cause summary instead.
///
/// If the SAME `(tool, args)` call fails this many times, the agent is repeating a
/// known-failed action verbatim — stop.
pub(crate) const REPEAT_FAILURE_THRESHOLD: u32 = 3;
/// If this many tool calls fail back-to-back with no success in between (even with
/// varied args), the agent is making no progress — stop.
pub(crate) const NO_PROGRESS_FAILURE_THRESHOLD: u32 = 6;
/// Hard policy rejections (a security block or a gate denial) are deterministic:
/// the identical `(tool, args)` call provably cannot succeed. Halt on the FIRST
/// verbatim repeat — i.e. the second identical attempt — rather than letting the
/// agent burn the generic [`REPEAT_FAILURE_THRESHOLD`] on a doomed call. The first
/// occurrence is allowed through so the model can read the "do not retry" reason
/// and pivot to a different, allowed approach.
pub(crate) const HARD_REJECT_REPEAT_THRESHOLD: u32 = 2;

/// Classification of a deterministic, recognizable policy rejection, detected via
/// the stable markers the security/approval layers emit
/// ([`crate::openhuman::security::POLICY_BLOCKED_MARKER`] /
/// [`POLICY_DENIED_MARKER`](crate::openhuman::security::POLICY_DENIED_MARKER)).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum HardReject {
    /// Permanent for this tier (read-only write, forbidden/credential path,
    /// disallowed command) — never succeeds on retry.
    Blocked,
    /// User denied / approval timed out this turn — re-asking the identical call
    /// only re-prompts.
    Denied,
}

/// Recognize a hard policy rejection from a tool result. Matches anywhere in the
/// string (not just the prefix) so it survives the `Error: …` wrapping the tool
/// layer adds. `Blocked` takes precedence over `Denied` if both somehow appear.
pub(crate) fn hard_reject_kind(result: &str) -> Option<HardReject> {
    if result.contains(crate::openhuman::security::POLICY_BLOCKED_MARKER) {
        Some(HardReject::Blocked)
    } else if result.contains(crate::openhuman::security::POLICY_DENIED_MARKER) {
        Some(HardReject::Denied)
    } else {
        None
    }
}

/// Shared repeated-failure circuit breaker, used by BOTH agent loops
/// (`run_tool_call_loop` here and `run_inner_loop` in `subagent_runner`) so they
/// can't drift. Tracks per-`(tool,args)`-signature failure counts and a
/// consecutive-failure run within a single agent turn; [`Self::record`] returns
/// a root-cause halt summary once a threshold trips.
#[derive(Default)]
pub(crate) struct RepeatFailureGuard {
    sig_counts: std::collections::HashMap<String, u32>,
    consecutive: u32,
}

impl RepeatFailureGuard {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Record one tool-call outcome. `args_sig` is a stable string form of the
    /// arguments (e.g. the command). Returns `Some(summary)` when the breaker
    /// trips — the caller should stop the loop and return that summary as the
    /// agent's result instead of grinding to `max_iterations`.
    pub(crate) fn record(
        &mut self,
        tool: &str,
        args_sig: &str,
        success: bool,
        result: &str,
    ) -> Option<String> {
        if success {
            self.consecutive = 0;
            return None;
        }
        self.consecutive += 1;
        let count = {
            let c = self
                .sig_counts
                .entry(format!("{tool}|{args_sig}"))
                .or_insert(0);
            *c += 1;
            *c
        };
        // Hard policy rejections trip on the first verbatim repeat; everything
        // else uses the generic identical-retry threshold.
        let hard = hard_reject_kind(result);
        let repeat_threshold = if hard.is_some() {
            HARD_REJECT_REPEAT_THRESHOLD
        } else {
            REPEAT_FAILURE_THRESHOLD
        };
        if count >= repeat_threshold {
            return Some(match hard {
                Some(HardReject::Blocked) => format!(
                    "Stopping: the `{tool}` call is blocked by the security policy and was \
                     re-issued with identical arguments — it can never succeed this way. \
                     Reason:\n{}\n\nDo not repeat this call; use an allowed alternative or report \
                     that it can't be done here.",
                    truncate_for_halt(result),
                ),
                Some(HardReject::Denied) => format!(
                    "Stopping: the `{tool}` call was denied and re-issued unchanged — re-asking \
                     will not change the answer. Reason:\n{}\n\nDo not repeat this call; take a \
                     different approach or report that it can't be done here.",
                    truncate_for_halt(result),
                ),
                None => format!(
                    "Stopping: the `{tool}` call was retried {count} times with identical \
                     arguments and kept failing — repeating it will not help. Last error:\n{}\n\n\
                     This looks unrecoverable in the current environment (e.g. a missing \
                     tool/dependency that cannot be installed here). Report this back instead of \
                     retrying.",
                    truncate_for_halt(result),
                ),
            });
        }
        if self.consecutive >= NO_PROGRESS_FAILURE_THRESHOLD {
            return Some(format!(
                "Stopping: {} tool calls in a row failed with no progress. Last error (from \
                 `{tool}`):\n{}\n\nDifferent commands are all failing — the goal looks unreachable \
                 in this environment. Report this back instead of retrying.",
                self.consecutive,
                truncate_for_halt(result),
            ));
        }
        None
    }
}

/// Clamp the last-error text embedded in a circuit-breaker halt summary so a huge
/// tool error (already capped at 1MB upstream) can't blow up the agent's result.
pub(crate) fn truncate_for_halt(s: &str) -> String {
    const MAX: usize = 600;
    if s.chars().count() <= MAX {
        return s.to_string();
    }
    let head: String = s.chars().take(MAX).collect();
    format!("{head}\n… [truncated]")
}

/// Execute a single turn of the agent loop: send messages, parse tool calls,
/// execute tools, and loop until the LLM produces a final text response.
/// When `silent` is true, suppresses stdout (for channel use).
///
/// This is a thin wrapper around [`run_tool_call_loop`] with the per-agent
/// filter and extra-tool plumbing disabled — i.e. the LLM sees the entire
/// `tools_registry` unchanged. Used by legacy call sites and harness tests
/// that don't need agent-aware scoping.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn agent_turn(
    provider: &dyn Provider,
    history: &mut Vec<ChatMessage>,
    tools_registry: &[Box<dyn Tool>],
    provider_name: &str,
    model: &str,
    temperature: f64,
    silent: bool,
    multimodal_config: &crate::openhuman::config::MultimodalConfig,
    max_tool_iterations: usize,
    payload_summarizer: Option<&dyn PayloadSummarizer>,
) -> Result<String> {
    let default_policy = DefaultToolPolicy;
    run_tool_call_loop(
        provider,
        history,
        tools_registry,
        provider_name,
        model,
        temperature,
        silent,
        "channel",
        multimodal_config,
        max_tool_iterations,
        None,
        None,
        &[],
        None,
        payload_summarizer,
        &default_policy,
    )
    .await
}

/// Execute a single turn of the agent loop: send messages, parse tool calls,
/// execute tools, and loop until the LLM produces a final text response.
///
/// # Per-agent tool scoping
///
/// The last two parameters support per-agent tool filtering without
/// requiring callers to build a filtered copy of the (non-`Clone`able)
/// tool registry:
///
/// * `visible_tool_names` — optional whitelist of tool names that are
///   allowed to reach the LLM. When `Some(set)`, only tools whose
///   `name()` is present in the set contribute to the function-calling
///   schema and are eligible for execution; every other tool in the
///   registry is hidden from the model and rejected if the model
///   somehow emits a call for it. When `None`, no filtering is applied
///   and every tool in the combined registry is visible (the legacy
///   behaviour used by CLI/REPL and harness tests).
///
/// * `extra_tools` — per-turn synthesised tools to splice alongside the
///   persistent `tools_registry`. The agent-dispatch path uses this to
///   surface delegation tools (`research`, `plan`,
///   `delegate_to_integrations_agent`, …) that are synthesised fresh
///   per turn from the active agent's `subagents` field and the
///   current Composio integration list, and therefore are not
///   registered in the global startup-time registry.
///
/// The combined tool list seen by the LLM this turn is
/// `tools_registry.iter().chain(extra_tools.iter())`, further narrowed
/// by `visible_tool_names` when supplied.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_tool_call_loop(
    provider: &dyn Provider,
    history: &mut Vec<ChatMessage>,
    tools_registry: &[Box<dyn Tool>],
    provider_name: &str,
    model: &str,
    temperature: f64,
    silent: bool,
    // Retained in the harness signature (callers pass their channel) but no
    // longer consumed here since the legacy CLI approval prompt was removed —
    // approval now flows through the process-global `ApprovalGate`.
    _channel_name: &str,
    multimodal_config: &crate::openhuman::config::MultimodalConfig,
    max_tool_iterations: usize,
    on_delta: Option<tokio::sync::mpsc::Sender<String>>,
    visible_tool_names: Option<&HashSet<String>>,
    extra_tools: &[Box<dyn Tool>],
    on_progress: Option<tokio::sync::mpsc::Sender<AgentProgress>>,
    payload_summarizer: Option<&dyn PayloadSummarizer>,
    tool_policy: &dyn ToolPolicy,
) -> Result<String> {
    let max_iterations = if max_tool_iterations == 0 {
        DEFAULT_MAX_TOOL_ITERATIONS
    } else {
        max_tool_iterations
    };

    // Is a given tool name visible to the model this turn? `None`
    // means no filter (legacy behaviour = everything visible).
    let is_visible = |name: &str| -> bool {
        match visible_tool_names {
            Some(set) => set.contains(name),
            None => true,
        }
    };

    // Filter to visible tools, then dedup by name before sending to the
    // provider. Registry tools may collide with per-turn synthesised
    // extra_tools (e.g. an `ArchetypeDelegationTool` whose
    // `delegate_name = "research"` shadowing a same-named skill). Some
    // providers (Anthropic, OpenHuman cloud after the uniqueness-enforcement
    // rollout) 400 on duplicate tool names — see TAURI-RUST-4.
    let filtered_specs: Vec<crate::openhuman::tools::ToolSpec> = tools_registry
        .iter()
        .chain(extra_tools.iter())
        .filter(|tool| is_visible(tool.name()))
        .map(|tool| tool.spec())
        .collect();
    let tool_specs =
        crate::openhuman::agent::harness::session::dedup_visible_tool_specs(filtered_specs);
    let use_native_tools = provider.supports_native_tools() && !tool_specs.is_empty();

    log::debug!(
        "[tool-loop] Registry has {} tool(s), extra {} tool(s), filter={} — {} visible in schema: [{}]",
        tools_registry.len(),
        extra_tools.len(),
        visible_tool_names
            .map(|s| format!("whitelist({})", s.len()))
            .unwrap_or_else(|| "none".to_string()),
        tool_specs.len(),
        tool_specs
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut context_guard = context_window_for_model(model)
        .map(ContextGuard::with_context_window)
        .unwrap_or_else(ContextGuard::new);
    let mut turn_cost = TurnCost::new();

    // Announce turn start to progress subscribers (if any). We use
    // `send().await` for lifecycle (turn/iteration) events so they
    // survive downstream backpressure — dropping one of these would
    // desync the web-channel progress bridge. High-volume delta events
    // use the same backpressure discipline (see below).
    if let Some(ref sink) = on_progress {
        if let Err(e) = sink.send(AgentProgress::TurnStarted).await {
            log::warn!("[agent_loop] progress sink closed at TurnStarted: {e}");
        }
    }

    let stop_hooks = current_stop_hooks();
    // Repeated-failure circuit breaker — halts with a root cause rather than
    // grinding to `max_iterations` (shared with the subagent loop).
    let mut failure_guard = RepeatFailureGuard::new();
    let mut halt_reason: Option<String> = None;
    for iteration in 0..max_iterations {
        if let Some(ref sink) = on_progress {
            if let Err(e) = sink
                .send(AgentProgress::IterationStarted {
                    iteration: (iteration + 1) as u32,
                    max_iterations: max_iterations as u32,
                })
                .await
            {
                log::warn!("[agent_loop] progress sink closed at IterationStarted: {e}");
            }
        }

        // ── Stop hooks: policy check before the next LLM call ──
        if !stop_hooks.is_empty() {
            let state = TurnState {
                iteration: (iteration + 1) as u32,
                max_iterations: max_iterations as u32,
                cost: &turn_cost,
                model,
            };
            for hook in &stop_hooks {
                match hook.check(&state).await {
                    StopDecision::Continue => {}
                    StopDecision::Stop { reason } => {
                        tracing::warn!(
                            iteration = (iteration + 1),
                            hook = hook.name(),
                            reason = %reason,
                            "[agent_loop] stop hook triggered — aborting turn"
                        );
                        anyhow::bail!("Agent turn stopped by hook '{}': {reason}", hook.name());
                    }
                }
            }
        }

        // ── Context guard: check utilization before each LLM call ──
        match context_guard.check() {
            ContextCheckResult::Ok => {}
            ContextCheckResult::CompactionNeeded => {
                tracing::warn!(
                    iteration,
                    "[agent_loop] context guard: compaction needed (>{:.0}% full)",
                    crate::openhuman::context::guard::COMPACTION_TRIGGER_THRESHOLD * 100.0
                );
                // Compaction is handled by history management upstream;
                // log and continue so the caller can act on it.
            }
            ContextCheckResult::ContextExhausted {
                utilization_pct,
                reason,
            } => {
                let msg = format!("Context window exhausted ({utilization_pct}% full): {reason}");
                crate::core::observability::report_error(
                    msg.as_str(),
                    "agent",
                    "context_exhausted",
                    &[
                        ("provider", provider_name),
                        ("model", model),
                        ("utilization_pct", &utilization_pct.to_string()),
                    ],
                );
                anyhow::bail!(msg);
            }
        }

        if let Some(context_window) = context_window_for_model(model) {
            let budget_outcome = trim_chat_messages_to_budget(history, context_window);
            if budget_outcome.trimmed {
                log::warn!(
                    "[agent_loop] pre-dispatch history trimmed model={} context_window={} original_tokens={} final_tokens={} messages_removed={}",
                    model,
                    context_window,
                    budget_outcome.original_tokens,
                    budget_outcome.final_tokens,
                    budget_outcome.messages_removed
                );
            } else {
                tracing::debug!(
                    iteration,
                    model,
                    context_window,
                    estimated_tokens = budget_outcome.final_tokens,
                    "[agent_loop] pre-dispatch token budget ok"
                );
            }
        }

        tracing::debug!(iteration, "[agent_loop] sending LLM request");
        let image_marker_count = multimodal::count_image_markers(history);
        if image_marker_count > 0 && !provider.supports_vision() {
            let cap_err = ProviderCapabilityError {
                provider: provider_name.to_string(),
                capability: "vision".to_string(),
                message: format!(
                    "received {image_marker_count} image marker(s), but this provider does not support vision input"
                ),
            };
            crate::core::observability::report_error(
                &cap_err,
                "agent",
                "provider_capability",
                &[
                    ("provider", provider_name),
                    ("capability", "vision"),
                    ("model", model),
                ],
            );
            return Err(cap_err.into());
        }

        let prepared_messages =
            multimodal::prepare_messages_for_provider(history, multimodal_config).await?;

        // Unified path via Provider::chat so provider-specific native tool logic
        // (OpenAI/Anthropic/OpenRouter/compatible adapters) is honored.
        let request_tools = if use_native_tools {
            Some(tool_specs.as_slice())
        } else {
            None
        };

        // Wire up a ProviderDelta → AgentProgress forwarder for this
        // iteration when a progress sink exists. Senders dropped after
        // the chat call so the forwarder task exits cleanly.
        let iteration_for_stream = (iteration + 1) as u32;
        let (delta_tx_opt, delta_forwarder) = if let Some(progress_sink) = on_progress.clone() {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<ProviderDelta>(128);
            let forwarder = tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    let mapped = match event {
                        ProviderDelta::TextDelta { delta } => AgentProgress::TextDelta {
                            delta,
                            iteration: iteration_for_stream,
                        },
                        ProviderDelta::ThinkingDelta { delta } => AgentProgress::ThinkingDelta {
                            delta,
                            iteration: iteration_for_stream,
                        },
                        ProviderDelta::ToolCallStart { call_id, tool_name } => {
                            AgentProgress::ToolCallArgsDelta {
                                call_id,
                                tool_name,
                                delta: String::new(),
                                iteration: iteration_for_stream,
                            }
                        }
                        ProviderDelta::ToolCallArgsDelta { call_id, delta } => {
                            AgentProgress::ToolCallArgsDelta {
                                call_id,
                                tool_name: String::new(),
                                delta,
                                iteration: iteration_for_stream,
                            }
                        }
                    };
                    // Await backpressure rather than dropping deltas so
                    // partial streamed text/args stays consistent with the
                    // eventual ToolCallStarted / ToolCallCompleted events.
                    if progress_sink.send(mapped).await.is_err() {
                        // Downstream closed — abandon the forwarder.
                        break;
                    }
                }
            });
            (Some(tx), Some(forwarder))
        } else {
            (None, None)
        };

        let chat_result = provider
            .chat(
                ChatRequest {
                    messages: &prepared_messages.messages,
                    tools: request_tools,
                    stream: delta_tx_opt.as_ref(),
                },
                model,
                temperature,
            )
            .await;

        drop(delta_tx_opt);
        if let Some(handle) = delta_forwarder {
            let _ = handle.await;
        }

        let (response_text, parsed_text, tool_calls, assistant_history_content, native_tool_calls) =
            match chat_result {
                Ok(resp) => {
                    // Update context guard with token usage from this response.
                    if let Some(ref usage) = resp.usage {
                        context_guard.update_usage(usage);
                        turn_cost.add_call(model, usage);
                        tracing::debug!(
                            iteration,
                            input_tokens = usage.input_tokens,
                            output_tokens = usage.output_tokens,
                            context_window = usage.context_window,
                            cumulative_usd = turn_cost.total_usd(),
                            "[agent_loop] LLM response received"
                        );
                        if let Some(ref sink) = on_progress {
                            let event = AgentProgress::TurnCostUpdated {
                                model: model.to_string(),
                                iteration: (iteration + 1) as u32,
                                input_tokens: turn_cost.input_tokens,
                                output_tokens: turn_cost.output_tokens,
                                cached_input_tokens: turn_cost.cached_input_tokens,
                                total_usd: turn_cost.total_usd(),
                            };
                            if let Err(e) = sink.send(event).await {
                                log::warn!(
                                    "[agent_loop] progress sink closed at TurnCostUpdated: {e}"
                                );
                            }
                        }
                    } else {
                        tracing::debug!(
                            iteration,
                            "[agent_loop] LLM response received (no usage info)"
                        );
                    }

                    let response_text = resp.text_or_empty().to_string();
                    let mut calls = parse_structured_tool_calls(&resp.tool_calls);
                    let mut parsed_text = String::new();

                    if calls.is_empty() {
                        let (fallback_text, fallback_calls) = parse_tool_calls(&response_text);
                        if !fallback_text.is_empty() {
                            parsed_text = fallback_text;
                        }
                        calls = fallback_calls;
                    }

                    tracing::debug!(
                        iteration,
                        native_tool_calls = resp.tool_calls.len(),
                        parsed_tool_calls = calls.len(),
                        "[agent_loop] tool calls parsed"
                    );

                    // Preserve native tool call IDs in assistant history so role=tool
                    // follow-up messages can reference the exact call id.
                    let assistant_history_content = if resp.tool_calls.is_empty() {
                        response_text.clone()
                    } else {
                        build_native_assistant_history(&response_text, &resp.tool_calls)
                    };

                    let native_calls = resp.tool_calls;
                    (
                        response_text,
                        parsed_text,
                        calls,
                        assistant_history_content,
                        native_calls,
                    )
                }
                Err(e) => {
                    // Transient upstream failures (rate-limit, gateway 5xx, "no
                    // healthy upstream", etc.) are already classified + retried
                    // by reliable.rs and produce an aggregate Sentry event only
                    // when every provider/model is exhausted. Reporting each
                    // per-iteration provider_chat error here duplicates the
                    // signal and floods Sentry — see OPENHUMAN-TAURI-3Y/3Z
                    // (~46 events combined) and the underlying TAURI-2E/84/T
                    // (~3300 events from raw per-attempt 429/503/504 reports).
                    let transient = crate::openhuman::inference::provider::reliable::is_rate_limited(
                        &e,
                    )
                        || crate::openhuman::inference::provider::reliable::is_upstream_unhealthy(
                            &e,
                        );
                    if transient {
                        tracing::warn!(
                            domain = "agent",
                            operation = "provider_chat",
                            provider = provider_name,
                            model = model,
                            iteration = iteration + 1,
                            error = %format!("{e:#}"),
                            "[agent] transient provider_chat failure — retried upstream; \
                             aggregated all-providers-exhausted will report if applicable"
                        );
                    } else {
                        crate::core::observability::report_error_or_expected(
                            &e,
                            "agent",
                            "provider_chat",
                            &[
                                ("provider", provider_name),
                                ("model", model),
                                ("iteration", &(iteration + 1).to_string()),
                            ],
                        );
                    }
                    return Err(e);
                }
            };

        let display_text = if parsed_text.is_empty() {
            response_text.clone()
        } else {
            parsed_text
        };

        if tool_calls.is_empty() {
            tracing::debug!(
                iteration,
                "[agent_loop] no tool calls — returning final response"
            );
            // No tool calls — this is the final response.
            // If a streaming sender is provided, relay the text in small chunks
            // so the channel can progressively update the draft message.
            if let Some(ref tx) = on_delta {
                // Split on whitespace boundaries, accumulating chunks of at least
                // STREAM_CHUNK_MIN_CHARS characters for progressive draft updates.
                let mut chunk = String::new();
                for word in display_text.split_inclusive(char::is_whitespace) {
                    chunk.push_str(word);
                    if chunk.len() >= STREAM_CHUNK_MIN_CHARS
                        && tx.send(std::mem::take(&mut chunk)).await.is_err()
                    {
                        break; // receiver dropped
                    }
                }
                if !chunk.is_empty() {
                    let _ = tx.send(chunk).await;
                }
            }
            history.push(ChatMessage::assistant(response_text.clone()));
            log::info!(
                "[agent_loop] turn complete: iters={} provider_calls={} tokens_in={} tokens_out={} cached_in={} usd={:.4}",
                (iteration + 1),
                turn_cost.call_count,
                turn_cost.input_tokens,
                turn_cost.output_tokens,
                turn_cost.cached_input_tokens,
                turn_cost.total_usd(),
            );
            if let Some(ref sink) = on_progress {
                if let Err(e) = sink
                    .send(AgentProgress::TurnCompleted {
                        iterations: (iteration + 1) as u32,
                    })
                    .await
                {
                    log::warn!("[agent_loop] progress sink closed at TurnCompleted: {e}");
                }
            }
            return Ok(display_text);
        }

        // Print any text the LLM produced alongside tool calls (unless silent)
        if !silent && !display_text.is_empty() {
            print!("{display_text}");
            let _ = std::io::stdout().flush();
        }

        // Execute each tool call and build results.
        // `individual_results` tracks per-call output so that native-mode history
        // can emit one `role: tool` message per tool call with the correct ID.
        let mut tool_results = String::new();
        let mut individual_results: Vec<String> = Vec::new();
        for (call_idx, call) in tool_calls.iter().enumerate() {
            // Stable id threaded through the start/complete pair (and
            // any preceding args-delta events) so consumers can
            // reconcile tool rows by id. The fallback includes
            // `call_idx` to stay unique when the same tool name
            // appears multiple times in one iteration.
            let progress_call_id = call
                .id
                .clone()
                .unwrap_or_else(|| format!("loop-{iteration}-{call_idx}-{}", call.name));
            // Emit `ToolCallStarted` for every parsed call, even ones
            // that will be rejected below (approval denied, CliRpcOnly,
            // unknown) — the client-side row was created from the
            // streamed args and needs a terminal event to resolve.
            if let Some(ref sink) = on_progress {
                if let Err(e) = sink
                    .send(AgentProgress::ToolCallStarted {
                        call_id: progress_call_id.clone(),
                        tool_name: call.name.clone(),
                        arguments: call.arguments.clone(),
                        iteration: (iteration + 1) as u32,
                    })
                    .await
                {
                    log::warn!(
                        "[agent_loop] progress sink closed while emitting ToolCallStarted: {e}"
                    );
                }
            }

            // Helper: emit a failed `ToolCallCompleted` for an
            // early-exit path (denied / CliRpcOnly / unknown) so the
            // client row flips to `error` instead of staying running.
            let emit_failed_completion = |message: &str| {
                let call_id = progress_call_id.clone();
                let tool_name = call.name.clone();
                let output_chars = message.chars().count();
                let iteration_u32 = (iteration + 1) as u32;
                let sink_opt = on_progress.clone();
                async move {
                    if let Some(sink) = sink_opt {
                        if let Err(e) = sink
                            .send(AgentProgress::ToolCallCompleted {
                                call_id,
                                tool_name,
                                success: false,
                                output_chars,
                                elapsed_ms: 0,
                                iteration: iteration_u32,
                            })
                            .await
                        {
                            log::warn!(
                                "[agent_loop] progress sink closed while emitting early-exit ToolCallCompleted: {e}"
                            );
                        }
                    }
                }
            };

            // ── Tool policy check (#2131) ─────────────────
            // Evaluate the pluggable ToolPolicy before any approval or
            // execution. If the policy denies the call, skip everything
            // (including approval side-effects) and return the denial
            // reason as a tool error to the model.
            if let PolicyDecision::Deny(reason) = tool_policy.evaluate(&call.name, &call.arguments)
            {
                tracing::debug!(
                    iteration,
                    tool = call.name.as_str(),
                    reason = %reason,
                    "[agent_loop] tool policy denied tool call"
                );
                let denied = format!("Tool '{}' denied by policy: {reason}", call.name);
                emit_failed_completion(&denied).await;
                individual_results.push(denied.clone());
                let _ = writeln!(
                    tool_results,
                    "<tool_result name=\"{}\">\n{denied}\n</tool_result>",
                    call.name
                );
                // Record so a re-issued identical call halts the turn rather than
                // repeating a deterministic policy denial to max_iterations.
                if let Some(halt) =
                    failure_guard.record(&call.name, &call.arguments.to_string(), false, &denied)
                {
                    halt_reason = Some(halt);
                }
                continue;
            }

            // Look up the tool by name in the combined registry + extras,
            // subject to the visibility whitelist. If the model hallucinated
            // a filtered-out tool name we treat it as unknown — the error
            // path below produces a structured error message the LLM can
            // correct in the next iteration.
            let tool_opt: Option<&dyn Tool> = tools_registry
                .iter()
                .chain(extra_tools.iter())
                .find(|t| t.name() == call.name && is_visible(t.name()))
                .map(|b| b.as_ref());
            tracing::debug!(
                iteration,
                tool = call.name.as_str(),
                found = tool_opt.is_some(),
                "[agent_loop] executing tool"
            );

            // Scope check: CliRpcOnly tools cannot run in the autonomous agent loop.
            if let Some(tool) = tool_opt {
                if tool.scope() == ToolScope::CliRpcOnly {
                    tracing::warn!(
                        iteration,
                        tool = call.name.as_str(),
                        "[agent_loop] tool scope is CliRpcOnly — denied in agent loop"
                    );
                    let denied = format!(
                        "Tool '{}' is only available via explicit CLI/RPC invocation, not in the autonomous agent loop.",
                        call.name
                    );
                    emit_failed_completion(&denied).await;
                    individual_results.push(denied.clone());
                    let _ = writeln!(
                        tool_results,
                        "<tool_result name=\"{}\">\n{denied}\n</tool_result>",
                        call.name
                    );
                    if let Some(halt) = failure_guard.record(
                        &call.name,
                        &call.arguments.to_string(),
                        false,
                        &denied,
                    ) {
                        halt_reason = Some(halt);
                    }
                    continue;
                }
            }

            // ── External-effect approval gate (#1339, #2135) ──
            // Tools whose `external_effect()` returns true route
            // through the process-global `ApprovalGate` so the UI
            // can prompt the user before `execute()` runs. The gate
            // is `None` when supervised mode is disabled or in test
            // envs — behavior matches the pre-#1339 path.
            //
            // `approval_request_id` carries the persisted row id
            // forward so we can stamp the terminal execution
            // outcome onto the same `pending_approvals` row after
            // the tool finishes (issue #2135). `None` means the
            // tool was either not gated (no supervised gate, not
            // external-effect), was session-allowlist-shortcutted,
            // or was denied — none of which produce an audit row
            // that needs an "after" entry.
            let mut approval_request_id: Option<String> = None;
            let mut approval_gate_for_audit: Option<
                std::sync::Arc<crate::openhuman::approval::ApprovalGate>,
            > = None;
            if let Some(tool) = tool_opt {
                if tool.external_effect_with_args(&call.arguments) {
                    if let Some(gate) = crate::openhuman::approval::ApprovalGate::try_global() {
                        let summary = crate::openhuman::approval::summarize_action(
                            &call.name,
                            &call.arguments,
                        );
                        let redacted = crate::openhuman::approval::redact_args(&call.arguments);
                        let (outcome, request_id) =
                            gate.intercept_audited(&call.name, &summary, redacted).await;
                        match outcome {
                            crate::openhuman::approval::GateOutcome::Allow => {
                                approval_request_id = request_id;
                                if approval_request_id.is_some() {
                                    approval_gate_for_audit = Some(gate);
                                }
                            }
                            crate::openhuman::approval::GateOutcome::Deny { reason } => {
                                tracing::warn!(
                                    iteration,
                                    tool = call.name.as_str(),
                                    reason = %reason,
                                    "[agent_loop] approval gate denied tool call"
                                );
                                emit_failed_completion(&reason).await;
                                individual_results.push(reason.clone());
                                let _ = writeln!(
                                    tool_results,
                                    "<tool_result name=\"{}\">\n{reason}\n</tool_result>",
                                    call.name
                                );
                                // Record the denial in the shared breaker (the
                                // gate's `[policy-denied]` marker makes it a
                                // hard reject) so a re-issued identical call
                                // halts the turn instead of re-prompting
                                // forever — the normal record path below is
                                // skipped by this `continue`.
                                if let Some(halt) = failure_guard.record(
                                    &call.name,
                                    &call.arguments.to_string(),
                                    false,
                                    &reason,
                                ) {
                                    halt_reason = Some(halt);
                                }
                                continue;
                            }
                        }
                    }
                }
            }

            let (result, call_succeeded) = if let Some(tool) = tool_opt {
                // ── Guardian pipeline interception (N1 -> N2 -> N3) ──
                if let Some(pipeline) =
                    crate::openhuman::guardian::GuardianPipeline::try_global()
                {
                    let command = if call.name == "bash" || call.name == "shell" {
                        call.arguments.get("command").and_then(|v| v.as_str())
                    } else {
                        None
                    };
                    let file_path = match call.name.as_str() {
                        "file_write" | "edit" | "file_read" | "glob" | "grep"
                        | "list_files" | "glob_search" | "read_diff"
                        | "run_linter" | "run_tests" => {
                            call.arguments.get("path").and_then(|v| v.as_str())
                        }
                        "apply_patch" => call
                            .arguments
                            .get("edits")
                            .and_then(|v| v.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|edit| edit.get("path"))
                            .and_then(|v| v.as_str()),
                        _ => None,
                    };

                    let pipeline_result = pipeline
                        .evaluate(&call.name, &call.arguments, command, file_path)
                        .await;

                    if !pipeline_result.allowed {
                        let reason = build_pipeline_block_reason(&pipeline_result);
                        tracing::warn!(
                            tool = call.name.as_str(),
                            blocked_by = pipeline_result.blocked_by,
                            %reason,
                            "[guardian] Pipeline blocked action"
                        );

                        // Publish the appropriate event based on which level blocked.
                        match pipeline_result.blocked_by.as_str() {
                            "n1" => {
                                crate::core::event_bus::bus::publish_global(
                                    crate::core::event_bus::DomainEvent::GuardianBlocked {
                                        tool_name: call.name.clone(),
                                        reason: reason.clone(),
                                        latency_us: pipeline_result.n1.latency_us,
                                    },
                                );
                            }
                            "n2" => {
                                let scores_json = pipeline_result
                                    .n2
                                    .as_ref()
                                    .map(|n2| serde_json::to_string(&n2.scores).unwrap_or_default())
                                    .unwrap_or_default();
                                crate::core::event_bus::bus::publish_global(
                                    crate::core::event_bus::DomainEvent::N2Blocked {
                                        tool_name: call.name.clone(),
                                        reason: reason.clone(),
                                        scores_json,
                                        latency_us: pipeline_result
                                            .n2
                                            .as_ref()
                                            .map_or(0, |r| r.latency_us),
                                    },
                                );
                            }
                            "n3" => {
                                let verdict = pipeline_result
                                    .n3
                                    .as_ref()
                                    .map(|r| format!("{:?}", r.verdict))
                                    .unwrap_or_default();
                                crate::core::event_bus::bus::publish_global(
                                    crate::core::event_bus::DomainEvent::N3Result {
                                        tool_name: call.name.clone(),
                                        verdict,
                                        reason: reason.clone(),
                                        latency_us: pipeline_result
                                            .n3
                                            .as_ref()
                                            .map_or(0, |r| r.latency_us),
                                    },
                                );
                            }
                            _ => {}
                        }

                        let _ = writeln!(
                            tool_results,
                            "<tool_result name=\"{}\">\n{reason}\n</tool_result>",
                            call.name
                        );
                        emit_failed_completion(&reason).await;
                        if let Some(halt) = failure_guard.record(
                            &call.name,
                            &call.arguments.to_string(),
                            false,
                            &reason,
                        ) {
                            halt_reason = Some(halt);
                        }
                        individual_results.push(reason);
                        continue;
                    }

                    // If N2 escalated but N3 allowed, publish N2Escalated.
                    if let Some(ref n2) = pipeline_result.n2 {
                        if n2.escalate {
                            let scores_json = serde_json::to_string(&n2.scores).unwrap_or_default();
                            crate::core::event_bus::bus::publish_global(
                                crate::core::event_bus::DomainEvent::N2Escalated {
                                    tool_name: call.name.clone(),
                                    scores_json,
                                    latency_us: n2.latency_us,
                                },
                            );
                            tracing::info!(
                                tool = call.name.as_str(),
                                "[guardian] N2 escalated, N3 allowed (proceeding)"
                            );
                        }
                    }
                }
                // ── Fin interception pipeline ─────────────────────────

                let tool_deadline =
                    crate::openhuman::tool_timeout::tool_execution_timeout_duration();
                let timeout_secs = crate::openhuman::tool_timeout::tool_execution_timeout_secs();
                let tool_started = std::time::Instant::now();
                let outcome =
                    tokio::time::timeout(tool_deadline, tool.execute(call.arguments.clone())).await;
                let elapsed_ms = tool_started.elapsed().as_millis() as u64;
                let (result_text, success) = match outcome {
                    Ok(Ok(r)) => {
                        let output = r.output();
                        let success = !r.is_error;
                        if success {
                            tracing::debug!(
                                iteration,
                                tool = call.name.as_str(),
                                output_len = output.len(),
                                "[agent_loop] tool succeeded"
                            );
                            let mut scrubbed = scrub_credentials(&output);
                            let (compacted, tj_stats) =
                                crate::openhuman::tokenjuice::compact_tool_output(
                                    &call.name,
                                    Some(&call.arguments),
                                    &scrubbed,
                                    Some(0),
                                );
                            if tj_stats.applied {
                                log::debug!(
                                    "[agent_loop] tokenjuice applied tool={} rule={} {}->{} bytes",
                                    call.name,
                                    tj_stats.rule_id,
                                    tj_stats.original_bytes,
                                    tj_stats.compacted_bytes
                                );
                                scrubbed = compacted;
                            }

                            // Per-tool max_result_size_chars cap. When
                            // a tool sets it and the (post-tokenjuice)
                            // body still exceeds the cap, truncate
                            // here and skip the global payload
                            // summarizer for this call — the cap is
                            // fast and deterministic, the summarizer
                            // is the fallback for tools that don't
                            // know their own size budget.
                            let mut hit_per_tool_cap = false;
                            if let Some(cap) = tool.max_result_size_chars() {
                                let char_count = scrubbed.chars().count();
                                if char_count > cap {
                                    let truncated: String = scrubbed.chars().take(cap).collect();
                                    let dropped = char_count - cap;
                                    log::info!(
                                        "[agent_loop] per-tool cap applied tool={} cap_chars={} original_chars={} dropped_chars={}",
                                        call.name,
                                        cap,
                                        char_count,
                                        dropped,
                                    );
                                    scrubbed = format!(
                                        "{truncated}\n\n[truncated by tool cap: {dropped} more chars not shown]"
                                    );
                                    hit_per_tool_cap = true;
                                }
                            }

                            if !hit_per_tool_cap {
                                if let Some(summarizer) = payload_summarizer {
                                    log::debug!(
                                        "[agent_loop] payload_summarizer intercepting tool={} bytes={}",
                                        call.name,
                                        scrubbed.len()
                                    );
                                    match summarizer
                                        .maybe_summarize(&call.name, None, &scrubbed)
                                        .await
                                    {
                                        Ok(Some(payload)) => {
                                            log::info!(
                                                "[agent_loop] payload_summarizer compressed tool={} {}->{} bytes",
                                                call.name,
                                                payload.original_bytes,
                                                payload.summary_bytes
                                            );
                                            scrubbed = payload.summary;
                                        }
                                        Ok(None) => {
                                            log::debug!(
                                                "[agent_loop] payload_summarizer pass-through tool={} bytes={}",
                                                call.name,
                                                scrubbed.len()
                                            );
                                        }
                                        Err(e) => {
                                            log::warn!(
                                                "[agent_loop] payload_summarizer error tool={} err={} (passing raw payload through)",
                                                call.name,
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            (scrubbed, true)
                        } else {
                            tracing::warn!(
                                iteration,
                                tool = call.name.as_str(),
                                "[agent_loop] tool returned error: {output}"
                            );
                            let scrubbed = scrub_credentials(&output);
                            let (compacted, _) = crate::openhuman::tokenjuice::compact_tool_output(
                                &call.name,
                                Some(&call.arguments),
                                &scrubbed,
                                Some(1),
                            );
                            (format!("Error: {compacted}"), false)
                        }
                    }
                    Ok(Err(e)) => {
                        crate::core::observability::report_error(
                            &e,
                            "tool",
                            "execute",
                            &[
                                ("tool", call.name.as_str()),
                                ("outcome", "failed"),
                                ("iteration", &(iteration + 1).to_string()),
                            ],
                        );
                        (format!("Error executing {}: {e}", call.name), false)
                    }
                    Err(_) => {
                        let msg = format!(
                            "tool '{}' timed out after {} seconds",
                            call.name, timeout_secs
                        );
                        crate::core::observability::report_error(
                            msg.as_str(),
                            "tool",
                            "execute",
                            &[
                                ("tool", call.name.as_str()),
                                ("outcome", "timeout"),
                                ("timeout_secs", &timeout_secs.to_string()),
                                ("iteration", &(iteration + 1).to_string()),
                            ],
                        );
                        (
                            format!(
                                "Error: tool '{}' timed out after {} seconds",
                                call.name, timeout_secs
                            ),
                            false,
                        )
                    }
                };
                if let Some(ref sink) = on_progress {
                    if let Err(e) = sink
                        .send(AgentProgress::ToolCallCompleted {
                            call_id: progress_call_id.clone(),
                            tool_name: call.name.clone(),
                            success,
                            output_chars: result_text.chars().count(),
                            elapsed_ms,
                            iteration: (iteration + 1) as u32,
                        })
                        .await
                    {
                        log::warn!("[agent_loop] progress sink closed while emitting ToolCallCompleted: {e}");
                    }
                }
                // ── Approval audit after-action row (#2135) ────
                // Stamp the terminal status onto the same
                // `pending_approvals` row the gate created before
                // execution, so the audit trail carries both the
                // before (approval) and after (executed_at +
                // outcome). Best-effort: a write failure here is
                // logged but not propagated to the agent.
                if let (Some(gate), Some(req_id)) = (
                    approval_gate_for_audit.as_ref(),
                    approval_request_id.as_ref(),
                ) {
                    let exec_outcome = if success {
                        crate::openhuman::approval::ExecutionOutcome::Success
                    } else {
                        crate::openhuman::approval::ExecutionOutcome::Failure
                    };
                    let err_text = if success {
                        None
                    } else {
                        Some(result_text.as_str())
                    };
                    gate.record_execution(req_id, exec_outcome, err_text);
                }
                (result_text, success)
            } else {
                tracing::warn!(
                    iteration,
                    tool = call.name.as_str(),
                    "[agent_loop] unknown tool requested"
                );
                let msg = format!("Unknown tool: {}", call.name);
                emit_failed_completion(&msg).await;
                (msg, false)
            };

            // ── Structured skill output envelope (INJ-02) ───────────
            // Skill execution tools (dadou.skill_execute, etc.) get their
            // raw output wrapped in a SkillOutputEnvelope JSON envelope
            // FIRST. Only the structured `data` field passes forward to
            // the INJ-01 <external_data> wrapping below. This prevents raw
            // skill output text (which may contain injection payloads)
            // from reaching the LLM prompt.
            //
            // The envelope metadata (version, gpg_verified) is consumed
            // here for trust decisions but never injected into the LLM
            // prompt — the LLM only sees `data` wrapped in <external_data>.
            let result = if call_succeeded && should_wrap_skill_output(&call.name) {
                let skill_name = call
                    .arguments
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let envelope = crate::openhuman::skills::SkillOutputEnvelope::new_success(
                    skill_name,
                    "0.0.0", // manifest version not loaded in tool loop
                    serde_json::json!({"output": &result}),
                    0,    // elapsed_ms not tracked per-call in this path
                    false, // GPG status unknown in tool loop
                );
                log::debug!(
                    "[skills:output] INJ-02 envelope for tool='{}' skill='{}'",
                    call.name,
                    skill_name,
                );
                // Only the data field passes to the LLM — structured JSON,
                // never raw text.
                envelope.data_json_line()
            } else {
                result.clone()
            };

            // ── External data wrapping (INJ-01) ──
            // Wrap tool results that originate from external sources (skills,
            // web fetches, file reads) in `<external_data>` tags so the LLM
            // sees the trust boundary. Only successful results with non-empty
            // output are wrapped; errors and internal tool results pass through
            // unchanged.
            let wrapped_result = if call_succeeded && !result.is_empty() {
                match should_wrap_external_data(&call.name, &call.arguments) {
                    Some((source, content_type)) => {
                        log::debug!(
                            "[anti-injection] wrapping tool='{}' source='{}' ctype='{}'",
                            call.name,
                            source,
                            content_type
                        );
                        wrap_external_data(&result, source, Some(content_type))
                    }
                    None => result.clone(),
                }
            } else {
                result.clone()
            };

            individual_results.push(wrapped_result.clone());
            let _ = writeln!(
                tool_results,
                "<tool_result name=\"{}\">\n{}\n</tool_result>",
                call.name, wrapped_result
            );

            // Repeated-failure circuit breaker (shared guard) — halt with a root
            // cause instead of grinding to `max_iterations` on a doomed action.
            if let Some(reason) = failure_guard.record(
                &call.name,
                &call.arguments.to_string(),
                call_succeeded,
                &result,
            ) {
                tracing::warn!(
                    iteration,
                    tool = call.name.as_str(),
                    "[agent_loop] circuit breaker tripped — halting with root cause"
                );
                halt_reason = Some(reason);
            }
        }

        // Add assistant message with tool calls + tool results to history.
        // Native mode: use JSON-structured messages so convert_messages() can
        // reconstruct proper OpenAI-format tool_calls and tool result messages.
        // Prompt mode: use XML-based text format as before.
        history.push(ChatMessage::assistant(assistant_history_content));
        if native_tool_calls.is_empty() {
            history.push(ChatMessage::user(format!("[Tool results]\n{tool_results}")));
        } else {
            for (native_call, result) in native_tool_calls.iter().zip(individual_results.iter()) {
                let tool_msg = serde_json::json!({
                    "tool_call_id": native_call.id,
                    "content": result,
                });
                history.push(ChatMessage::tool(tool_msg.to_string()));
            }
        }

        // Circuit breaker tripped this iteration: return the root-cause summary
        // as the agent's result instead of looping to `max_iterations`. The
        // tool results are already in `history` above, so the caller still has
        // full context if it wants it.
        if let Some(reason) = halt_reason.take() {
            // Mirror the normal-completion path: emit TurnCompleted before the
            // early return, otherwise progress consumers stay "in-flight"
            // indefinitely when the circuit breaker trips.
            if let Some(ref sink) = on_progress {
                if let Err(e) = sink
                    .send(AgentProgress::TurnCompleted {
                        iterations: (iteration + 1) as u32,
                    })
                    .await
                {
                    log::warn!("[agent_loop] progress sink closed at TurnCompleted: {e}");
                }
            }
            return Ok(reason);
        }
    }

    // Return the typed `AgentError::MaxIterationsExceeded` variant (boxed
    // through `anyhow::Error`) so downstream wrappers — notably
    // `Agent::run_single` in `harness/session/runtime.rs` — can downcast and
    // suppress Sentry emission for this deterministic agent-state outcome
    // (OPENHUMAN-TAURI-99 / -98). The `Display` text is preserved verbatim so
    // any caller that already inspects the string (UI chat surface, tests)
    // continues to work.
    Err(anyhow::Error::new(
        crate::openhuman::agent::error::AgentError::MaxIterationsExceeded {
            max: max_iterations,
        },
    ))
}

/// Build a human-readable block reason from a [`GuardianPipelineResult`].
///
/// The reason is prefixed with `[policy-blocked]` so the agent loop's
/// hard-reject detection (HARD_REJECT_REPEAT_THRESHOLD) recognizes it as a
/// deterministic, unrecoverable block.
///
/// The message includes WHICH level blocked (`[N1]` / `[N2]` / `[N3]`) and
/// the specific rules or scores that triggered the block.
fn build_pipeline_block_reason(
    result: &crate::openhuman::guardian::GuardianPipelineResult,
) -> String {
    match result.blocked_by.as_str() {
        "n1" => {
            let blocks: Vec<String> = result
                .n1
                .rule_results
                .iter()
                .filter(|r| {
                    r.action
                        == crate::openhuman::guardian::RuleAction::Block
                })
                .map(|r| format!("[{}: {}]", r.rule_name, r.reason))
                .collect();
            format!(
                "[policy-blocked] Guardian N1 blocked: {}",
                blocks.join("; ")
            )
        }
        "n2" => {
            let blocks: Vec<String> = result
                .n2
                .as_ref()
                .map(|n2| {
                    n2.scores
                        .iter()
                        .filter(|s| s.score >= 0.7) // BLOCK_THRESHOLD
                        .map(|s| {
                            format!(
                                "[{}: {} (score={})]",
                                s.triggered_by, s.reason, s.score
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            format!(
                "[policy-blocked] Guardian N2 blocked: {}",
                blocks.join("; ")
            )
        }
        "n3" => {
            let detail = result
                .n3
                .as_ref()
                .map(|n3| format!("[verdict={:?}] {}", n3.verdict, n3.reason))
                .unwrap_or_else(|| "unknown".to_string());
            format!("[policy-blocked] Guardian N3 blocked: {}", detail)
        }
        _ => "[policy-blocked] Guardian blocked: unknown reason".to_string(),
    }
}

/// Determine whether a tool result should be wrapped in `<external_data>`
/// markers before it reaches the LLM.
///
/// Returns `Some((source, content_type))` when the tool call originates
/// from an external data source:
/// - `dadou.*` skills → `("dadou_skill", "skill_output")`
/// - Web fetches / searches → `("web", "web_content")`
/// - File reads → `("file", "file_content")`
/// - All other tools → `None` (no wrapping)
///
/// Only successful results with non-empty output are wrapped; the caller
/// checks `call_succeeded` and empty output before calling this.
pub(crate) fn should_wrap_external_data(
    tool_name: &str,
    arguments: &serde_json::Value,
) -> Option<(&'static str, &'static str)> {
    match tool_name {
        // Skill outputs: any tool in the dadou.* namespace
        n if n.starts_with("dadou.") => Some(("dadou_skill", "skill_output")),
        // Web fetch / search tools
        "fetch" | "web_search" | "web_fetch" | "webpage" => Some(("web", "web_content")),
        // File reads — conservative: wrap all file reads regardless of path
        "file_read" => Some(("file", "file_content")),
        // Everything else: no wrapping
        _ => None,
    }
}

/// Determine whether a tool's output should be wrapped in a
/// [`SkillOutputEnvelope`] before INJ-01 `<external_data>` tagging.
///
/// Returns `true` for skill execution tools whose raw text output could
/// contain injection payloads. Skill management tools (install, update,
/// list, etc.) are excluded — they return metadata, not executed output.
///
/// # Matching logic
///
/// - Tools in the `dadou.*` namespace whose name ends with `_execute`
///   are wrapped. This covers `dadou.skill_execute` and any future
///   domain-specific execution tools.
/// - Management tools (`dadou.skill_install`, `dadou.skill_list`, etc.)
///   pass through unwrapped since they return structured metadata.
pub(crate) fn should_wrap_skill_output(tool_name: &str) -> bool {
    // Only wrap actual skill execution tools — not management tools.
    // Management tools end with _install, _update, _audit, _remove,
    // _list, _trust_author, _enable, _disable.
    if tool_name.starts_with("dadou.") && tool_name.ends_with("_execute") {
        return true;
    }
    // Additional patterns can be added here as new skill execution
    // tools are introduced.
    false
}

/// Conservative check for whether a path is outside the workspace directory.
///
/// For v1, this uses simple heuristics: checks for `..` components and
/// absolute-path indicators. In practice, file_read wrapping is always
/// applied (the caller in `should_wrap_external_data` returns
/// `Some(...)` for all `file_read` calls), so this function is available
/// for future refinement but not currently used in the wrapping decision.
#[allow(dead_code)]
fn is_outside_workspace(path: &str) -> bool {
    path.contains("..") || path.starts_with('/') || path.starts_with("\\\\")
        || (path.len() > 2
            && path.as_bytes()[1] == b':'
            && matches!(path.as_bytes()[0], b'a'..=b'z' | b'A'..=b'Z'))
}

#[cfg(test)]
mod injection_tests {
    use super::*;

    #[test]
    fn skill_tool_names_are_wrapped() {
        assert_eq!(
            should_wrap_external_data("dadou.skill_execute", &serde_json::json!({})),
            Some(("dadou_skill", "skill_output"))
        );
        assert_eq!(
            should_wrap_external_data("dadou.skill_install", &serde_json::json!({})),
            Some(("dadou_skill", "skill_output"))
        );
    }

    #[test]
    fn web_tool_names_are_wrapped() {
        assert_eq!(
            should_wrap_external_data("fetch", &serde_json::json!({"url": "https://example.com"})),
            Some(("web", "web_content"))
        );
        assert_eq!(
            should_wrap_external_data("web_search", &serde_json::json!({"query": "rust"})),
            Some(("web", "web_content"))
        );
        assert_eq!(
            should_wrap_external_data("webpage", &serde_json::json!({"url": "https://x"})),
            Some(("web", "web_content"))
        );
    }

    #[test]
    fn file_read_tool_is_wrapped() {
        assert_eq!(
            should_wrap_external_data("file_read", &serde_json::json!({"path": "/etc/passwd"})),
            Some(("file", "file_content"))
        );
    }

    #[test]
    fn internal_tools_are_not_wrapped() {
        assert_eq!(
            should_wrap_external_data("bash", &serde_json::json!({"command": "ls"})),
            None
        );
        assert_eq!(
            should_wrap_external_data("shell", &serde_json::json!({"command": "echo hi"})),
            None
        );
        assert_eq!(
            should_wrap_external_data("edit", &serde_json::json!({"path": "main.rs"})),
            None
        );
        assert_eq!(
            should_wrap_external_data("glob", &serde_json::json!({"pattern": "**/*.rs"})),
            None
        );
        assert_eq!(
            should_wrap_external_data("grep", &serde_json::json!({"pattern": "fn main"})),
            None
        );
    }

    #[test]
    fn is_outside_workspace_detects_absolute_paths() {
        assert!(is_outside_workspace("/etc/passwd"));
        assert!(is_outside_workspace("C:\\Windows\\system32"));
        assert!(is_outside_workspace("../relative/escape"));
        assert!(!is_outside_workspace("src/main.rs"));
        assert!(!is_outside_workspace("./local/file.txt"));
    }

    #[test]
    fn wrap_external_data_produces_valid_tag() {
        let result = "some skill output here";
        let wrapped = wrap_external_data(result, "dadou_skill", Some("skill_output"));
        assert!(wrapped.starts_with("<external_data"));
        assert!(wrapped.contains("source=\"dadou_skill\""));
        assert!(wrapped.contains("trusted=\"false\""));
        assert!(wrapped.contains("content_type=\"skill_output\""));
        assert!(wrapped.contains("some skill output here"));
        assert!(wrapped.trim_end().ends_with("</external_data>"));
    }

    // ── Skill output envelope tests (INJ-02) ────────────────────────

    #[test]
    fn should_wrap_skill_output_matches_execute_tools() {
        assert!(should_wrap_skill_output("dadou.skill_execute"));
        assert!(should_wrap_skill_output("dadou.agent_execute"));
        assert!(should_wrap_skill_output("dadou.wasm_execute"));
    }

    #[test]
    fn should_wrap_skill_output_does_not_match_management_tools() {
        assert!(!should_wrap_skill_output("dadou.skill_install"));
        assert!(!should_wrap_skill_output("dadou.skill_list"));
        assert!(!should_wrap_skill_output("dadou.skill_update"));
        assert!(!should_wrap_skill_output("dadou.skill_audit"));
        assert!(!should_wrap_skill_output("dadou.skill_remove"));
        assert!(!should_wrap_skill_output("dadou.skill_trust_author"));
    }

    #[test]
    fn should_wrap_skill_output_does_not_match_non_dadou_tools() {
        assert!(!should_wrap_skill_output("bash"));
        assert!(!should_wrap_skill_output("file_read"));
        assert!(!should_wrap_skill_output("web_search"));
        assert!(!should_wrap_skill_output("edit"));
    }

    #[test]
    fn skill_output_envelope_data_is_structured_json() {
        // Verify that the data field produced by the envelope is
        // valid JSON with the expected structure.
        use crate::openhuman::skills::SkillOutputEnvelope;

        let envelope = SkillOutputEnvelope::new_success(
            "test-skill",
            "1.0.0",
            serde_json::json!({"output": "hello world"}),
            42,
            false,
        );
        let data_line = envelope.data_json_line();
        let parsed: serde_json::Value = serde_json::from_str(&data_line).unwrap();
        assert_eq!(parsed["output"], "hello world");
    }

    #[test]
    fn skill_output_envelope_metadata_not_in_data() {
        // Verify metadata fields are NOT included in data_json_line.
        use crate::openhuman::skills::SkillOutputEnvelope;

        let envelope = SkillOutputEnvelope::new_success(
            "secret-skill",
            "2.0.0",
            serde_json::json!({"output": "data"}),
            100,
            true,
        );
        let data_line = envelope.data_json_line();
        let parsed: serde_json::Value = serde_json::from_str(&data_line).unwrap();
        // The `output` field IS present
        assert_eq!(parsed["output"], "data");
        // Metadata fields should NOT leak
        assert!(parsed.get("skill_name").is_none());
        assert!(parsed.get("skill_version").is_none());
        assert!(parsed.get("gpg_verified").is_none());
        assert!(parsed.get("execution_status").is_none());
    }
}

#[cfg(test)]
#[path = "tool_loop_tests.rs"]
mod tests;
