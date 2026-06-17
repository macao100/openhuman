//! Connected integrations and delegation-tool synthesis.
//! Extracted from turn.rs — Sprint 2.

use super::types::Agent;
use std::sync::Arc;

impl Agent {
    /// Fetches the list of connected integrations (Composio, etc.) and
    /// caches them on the agent struct so the system prompt, tool policy,
    /// and tool surface all share one consistent snapshot for the turn.
    ///
    /// Call once per turn before building the system prompt. On the first
    /// turn of a fresh session this also primes the hash-based change
    /// detection used by [`Self::refresh_delegation_tools`].
    pub async fn fetch_connected_integrations(&mut self) {
        let config = match self.integration_runtime_config.clone() {
            Some(config) => config,
            None => match crate::openhuman::config::Config::load_or_init().await {
                Ok(config) => config,
                Err(e) => {
                    log::debug!(
                        "[agent] skipping connected integrations fetch: config load failed: {e}"
                    );
                    return;
                }
            },
        };
        self.connected_integrations =
            crate::openhuman::composio::fetch_connected_integrations(&config).await;
        self.connected_integrations_initialized = true;
    }

    /// Re-synthesise `delegate_*` tools for the orchestrator's `subagents`
    /// declaration using the live `connected_integrations` slice, and
    /// reconcile the resulting set into `self.tools` / `self.tool_specs` /
    /// `self.visible_tool_specs` / `self.visible_tool_names`.
    ///
    /// **Reconciliation strategy** — full rebuild of the synthesised
    /// subset:
    ///
    ///   1. Drop every tool whose name was in [`Self::synthesized_tool_names`]
    ///      from the previous synthesis. Direct tools (`query_memory`,
    ///      `cron_add`, …) are untouched because their names are not in
    ///      that set.
    ///   2. Append the freshly collected synthesis output verbatim.
    ///   3. Replace `synthesized_tool_names` with the new set so the
    ///      next refresh has a clean mask to undo.
    ///
    /// This is safer than appending-only or strict-diff reconcile:
    ///
    ///   * Stale tools after a revoke can never leak — anything from the
    ///     previous synthesis is unconditionally dropped, the new set is
    ///     authoritative.
    ///   * Direct tools can never be accidentally removed — only names
    ///     in `synthesized_tool_names` are touched.
    ///   * Duplicate registration is impossible — retain+extend
    ///     guarantees every final entry is either a non-synthesised
    ///     direct tool or a member of the fresh `synthed` set.
    ///
    /// **When to call**: on turn 1 only when the session was built
    /// without a prewarmed Composio cache snapshot, and on any
    /// subsequent turn where the connection set has changed since the
    /// last reconcile (detected via
    /// [`Self::last_seen_integrations_hash`] vs.
    /// [`crate::openhuman::composio::cached_active_integrations`]).
    ///
    /// **Atomicity**: when `Arc::get_mut` fails (a sub-agent or other
    /// caller has already captured a clone of the tool list), we restore
    /// the previous `synthesized_tool_names` and bail. The next refresh
    /// attempt will re-apply the full transition cleanly rather than
    /// resuming from a partial state. This should never happen on a turn
    /// boundary in production — sub-agents always drop their snapshots
    /// before the parent's next turn — but it's defended against anyway.
    ///
    /// **Return value** — `true` when the agent's tool surface is now
    /// consistent with `self.connected_integrations` (either because a
    /// successful reconcile applied, or because no reconcile was needed).
    /// `false` only when the Arc was shared and the reconcile was
    /// aborted; callers should treat this as "retry next turn" and
    /// **not** advance any signal they use to gate future refreshes
    /// (e.g. `last_seen_integrations_hash`) — otherwise a one-shot
    /// shared-Arc collision could suppress further reconciliation until
    /// another integration event happened to bump the hash again.
    pub fn refresh_delegation_tools(&mut self) -> bool {
        use crate::openhuman::agent::harness::definition::AgentDefinitionRegistry;
        use crate::openhuman::tools::orchestrator_tools::collect_orchestrator_tools;

        let Some(reg) = AgentDefinitionRegistry::global() else {
            return true;
        };
        let Some(def) = reg.get(&self.agent_definition_id) else {
            log::debug!(
                "[agent] refresh_delegation_tools: definition '{}' not in registry — skipping",
                self.agent_definition_id
            );
            return true;
        };
        if def.subagents.is_empty() {
            return true;
        }

        let synthed = collect_orchestrator_tools(def, reg, &self.connected_integrations);
        let synthed_names: std::collections::HashSet<String> =
            synthed.iter().map(|t| t.name().to_string()).collect();
        let synthed_specs: Vec<crate::openhuman::tools::ToolSpec> =
            synthed.iter().map(|t| t.spec()).collect();

        if self.synthesized_tool_names.is_empty() && synthed_names.is_empty() {
            return true;
        }

        let old_synth = std::mem::take(&mut self.synthesized_tool_names);

        match (
            Arc::get_mut(&mut self.tools),
            Arc::get_mut(&mut self.tool_specs),
        ) {
            (Some(tools_vec), Some(specs_vec)) => {
                tools_vec.retain(|t| !old_synth.contains(t.name()));
                specs_vec.retain(|s| !old_synth.contains(&s.name));
                tools_vec.extend(synthed);
                specs_vec.extend(synthed_specs);
            }
            _ => {
                log::warn!(
                    "[agent] refresh_delegation_tools: tools/tool_specs Arc is shared — \
                     cannot reconcile delegation surface (would have produced {} synthesised tool(s)). \
                     Restoring previous synthesized_tool_names so the next refresh retries cleanly.",
                    synthed_names.len()
                );
                self.synthesized_tool_names = old_synth;
                return false;
            }
        }

        if !self.visible_tool_names.is_empty() {
            for name in &old_synth {
                self.visible_tool_names.remove(name);
            }
            for name in &synthed_names {
                self.visible_tool_names.insert(name.clone());
            }
        }

        self.rebuild_tool_policy_session();

        let added: Vec<String> = synthed_names
            .iter()
            .filter(|n| !old_synth.contains(n.as_str()))
            .cloned()
            .collect();
        let removed: Vec<String> = old_synth
            .iter()
            .filter(|n| !synthed_names.contains(n.as_str()))
            .cloned()
            .collect();

        self.synthesized_tool_names = synthed_names;

        log::info!(
            "[agent] refresh_delegation_tools: reconciled delegation surface for agent '{}' (display='{}'); now {} synthesised tool(s); added={:?} removed={:?}",
            self.agent_definition_id,
            self.agent_definition_name,
            self.synthesized_tool_names.len(),
            added,
            removed
        );
        true
    }
}
