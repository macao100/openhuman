//! System prompt builder for agent turns.
//! Extracted from turn.rs — Sprint 2.

use super::types::Agent;
use crate::openhuman::agent_tool_policy::render_tool_policy_boundary;
use crate::openhuman::context::prompt::{LearnedContextData, PromptContext, PromptTool};
use crate::openhuman::tools::Tool;

use anyhow::Result;

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
