//! System prompt builder for agent turns.
//! Extracted from turn.rs — Sprint 2.

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

impl Agent {
    /// Builds the system prompt for the current turn, including tool
    /// instructions and learned context.
    pub fn build_system_prompt(&self, learned: LearnedContextData) -> Result<String> {
        let tools_slice: &[Box<dyn Tool>] = self.tools.as_slice();
        let instructions = self
            .tool_dispatcher
            .prompt_instructions_for_specs(self.visible_tool_specs.as_slice())
            .unwrap_or_else(|| self.tool_dispatcher.prompt_instructions(tools_slice));
        // Adapt the owned Box<dyn Tool> slice into the shared PromptTool
        // shape that every prompt-building call-site uses. Temporary vec
        // borrows from `tools_slice` and lives for the duration of the
        // prompt build.
        let prompt_tools = PromptTool::from_tools(tools_slice);
        let prompt_visible_tool_names = self.tool_policy_session.visible_tool_names_for_prompt();
        let ctx = PromptContext {
            workspace_dir: &self.workspace_dir,
            model_name: &self.model_name,
            agent_id: &self.agent_definition_name,
            tools: &prompt_tools,
            skills: &self.skills,
            dispatcher_instructions: &instructions,
            learned,
            visible_tool_names: &prompt_visible_tool_names,
            tool_call_format: self.tool_dispatcher.tool_call_format(),
            connected_integrations: &self.connected_integrations,
            connected_identities_md: crate::openhuman::agent::prompts::render_connected_identities(
            ),
            include_profile: !self.omit_profile,
            include_memory_md: !self.omit_memory_md,
            curated_snapshot: None,
            user_identity: crate::openhuman::app_state::peek_cached_current_user_identity(),
        };
        // Route through the global context manager so every
        // prompt-building call-site — main agent, sub-agent runner,
        // channel runtimes — shares one builder configuration.
        let mut prompt = self.context.build_system_prompt(&ctx)?;
        if let Some(boundary) = render_tool_policy_boundary(&self.tool_policy_session, 2048) {
            prompt = format!("{boundary}\n\n{prompt}");
        }
        Ok(prompt)
    }
}
