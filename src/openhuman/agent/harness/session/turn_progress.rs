//! Progress emission for agent turns.
//! Extracted from turn.rs — Sprint 2.

use super::types::Agent;
use crate::openhuman::agent::progress::AgentProgress;
use crate::openhuman::inference::provider::ChatMessage;
use anyhow::Result;

impl Agent {
    /// Emit a lifecycle progress event. Uses `send().await` so control
    /// events (turn/iteration boundaries, tool_call_started/completed,
    /// turn_completed) survive downstream backpressure from the
    /// higher-frequency streamed deltas that share the same `on_progress`
    /// channel — dropping one of these would desync the web-channel
    /// progress bridge (e.g. a tool row stuck in `running` forever).
    /// A closed sink is logged and ignored; no progress subscriber is
    /// equivalent to success.
    pub(crate) async fn emit_progress(&self, event: AgentProgress) {
        if let Some(ref tx) = self.on_progress {
            if let Err(e) = tx.send(event).await {
                log::warn!("[agent] progress sink closed while emitting lifecycle event: {e}");
            }
        }
    }
}
