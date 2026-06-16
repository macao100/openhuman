//! Extracted from turn.rs — Sprint 2 split.

//! Turn lifecycle: running a single interaction, executing tools, and
//! wiring the context pipeline + sub-agent harness around them.
//!
//! This file owns the "hot path" methods on `Agent`:
//!
//! - [`Agent::turn`] — the big one. Orchestrates system-prompt build,
//!   memory-context injection, the provider loop, tool dispatch, and
//!   the context pipeline (tool-result budget → microcompact →
//!   autocompact signal → session-memory extraction trigger).
//! - [`Agent::execute_tool_call`] / [`Agent::execute_tools`] — the
//!   per-call runners.
//! - [`Agent::build_parent_execution_context`] — snapshot helper for
//!   the parent-context task-local that sub-agents read.
//! - [`Agent::trim_history`], [`Agent::fetch_learned_context`],
//!   [`Agent::build_system_prompt`] — the small helpers `turn()` leans
//!   on every call.
//! - [`Agent::spawn_session_memory_extraction`] — the fire-and-forget
//!   background archivist fork.

use super::transcript;
use super::types::Agent;
use crate::core::event_bus::{publish_global, DomainEvent};
use crate::openhuman::agent::dispatcher::{ParsedToolCall, ToolExecutionResult};
use crate::openhuman::agent::harness;
use crate::openhuman::agent::hooks::{self, ToolCallRecord, TurnContext};
use crate::openhuman::agent::memory_loader::collect_recall_citations;
use crate::openhuman::agent::progress::AgentProgress;
use crate::openhuman::agent::tool_policy::{
    ToolCallContext, ToolPolicyDecision, ToolPolicyRequest,
};
use crate::openhuman::agent_experience::{
    prepend_experience_block, render_experience_hits, AgentExperienceStore, ExperienceQuery,
};
use crate::openhuman::agent_tool_policy::render_tool_policy_boundary;
use crate::openhuman::context::prompt::{LearnedContextData, PromptContext, PromptTool};
use crate::openhuman::context::{ReductionOutcome, ARCHIVIST_EXTRACTION_PROMPT};
use crate::openhuman::inference::model_context::context_window_for_model;
use crate::openhuman::inference::provider::{
    ChatMessage, ChatRequest, ConversationMessage, ProviderDelta, UsageInfo,
};
use crate::openhuman::memory::MemoryCategory;
use crate::openhuman::tools::traits::ToolCallOptions;
use crate::openhuman::tools::Tool;
use crate::openhuman::util::truncate_with_ellipsis;

use crate::openhuman::agent::harness::token_budget::{
    trim_chat_messages_to_budget, trim_conversation_history_to_budget,
};
use anyhow::Result;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Instruction appended (as a synthetic user turn) to the provider
/// messages when a turn hits the tool-call iteration cap. Asks the model
/// to wrap up with a resumable checkpoint instead of letting the turn die.
/// Native tools are disabled for this call so the model produces prose,
/// not yet another tool call. See bug-report-2026-05-26 A1.
const MAX_ITER_CHECKPOINT_INSTRUCTION: &str = "\
You have reached the maximum number of tool calls allowed for this single turn, so you cannot call any more tools right now. \
Do not attempt another tool call. Instead, write a short progress checkpoint for the user with two clearly labelled parts:\n\
1. **Done so far** — what you have accomplished in this turn, grounded in the tool results above.\n\
2. **Next steps** — exactly what you plan to do next.\n\
Write it so you can pick up seamlessly where you left off when the user replies. Be concise.";

/// Build a deterministic checkpoint summary from this turn's tool-call
/// records. Used only as a safety net when the model-written checkpoint
/// call fails or returns empty, so a capped turn can never be left without
/// a well-formed assistant message — which is what silently wedged the
/// thread before (bug-report-2026-05-26 A1).
fn build_deterministic_checkpoint(records: &[ToolCallRecord], max_iterations: usize) -> String {
    let mut out = format!(
        "I reached the tool-call limit for this turn ({max_iterations} steps), so I paused here.\n\n**Done so far:**\n"
    );
    if records.is_empty() {
        out.push_str("- (no tools completed yet)\n");
    } else {
        for r in records {
            let status = if r.success { "ok" } else { "failed" };
            out.push_str(&format!("- `{}` — {}\n", r.name, status));
        }
    }
    out.push_str(
        "\n**Next steps:** I'll continue from here — just reply (e.g. \"continue\") and I'll pick up where I left off.",
    );
    out
}

impl Agent {
    async fn inject_agent_experience_context(
        &self,
        user_message: &str,
        enriched: String,
    ) -> String {
        const MAX_EXPERIENCE_HITS: usize = 3;
        const MAX_EXPERIENCE_BLOCK_BYTES: usize = 2048;

        if !self.learning_enabled {
            return enriched;
        }

        let tools = self
            .visible_tool_specs
            .iter()
            .map(|spec| spec.name.clone())
            .collect();
        let store = AgentExperienceStore::new(self.memory.clone());
        let query = ExperienceQuery {
            query: user_message.to_string(),
            tools,
            tags: Vec::new(),
            agent_id: Some(self.agent_definition_id.clone()).filter(|id| !id.trim().is_empty()),
            entrypoint: Some(self.event_channel.clone())
                .filter(|entrypoint| !entrypoint.trim().is_empty()),
            max_hits: MAX_EXPERIENCE_HITS,
        };

        match store.retrieve(query).await {
            Ok(hits) => {
                let matched_hits: Vec<_> = hits
                    .into_iter()
                    .filter(|hit| !hit.match_reasons.is_empty())
                    .collect();
                let block = render_experience_hits(&matched_hits, MAX_EXPERIENCE_BLOCK_BYTES);
                if block.is_empty() {
                    return enriched;
                }
                log::debug!(
                    "[agent-experience] injected {} experience hit(s) bytes={}",
                    matched_hits.len(),
                    block.len()
                );
                prepend_experience_block(&enriched, &block)
            }
            Err(err) => {
                log::warn!("[agent-experience] retrieval failed (non-fatal): {err}");
                enriched
            }
        }
    }
}
