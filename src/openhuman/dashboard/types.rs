//! Types for the dashboard domain.
//!
//! Re-exports [`DashboardConfig`] from the config schema module so callers
//! can import everything dashboard-related from one place.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Re-export the dashboard config struct from the shared config schema.
pub use crate::openhuman::config::schema::dashboard::DashboardConfig;

/// Enumeration of dashboard-relevant event kinds.
///
/// Each variant maps to a slug stored in the `kind` column of the
/// `dashboard_events` SQLite table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DashboardEventKind {
    GuardianBlocked,
    N2Blocked,
    N2Escalated,
    N3Result,
    PlanValidated,
    InjectionBlocked,
    ToolExecutionStarted,
    ToolExecutionCompleted,
    AgentTurnStarted,
    AgentTurnCompleted,
    SkillExecuted,
    MemoryStored,
    MemoryRecalled,
    ChannelConnected,
    ChannelDisconnected,
    SystemStartup,
    SystemShutdown,
}

impl DashboardEventKind {
    /// Human-readable slug for storage and display.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GuardianBlocked => "guardian_blocked",
            Self::N2Blocked => "n2_blocked",
            Self::N2Escalated => "n2_escalated",
            Self::N3Result => "n3_result",
            Self::PlanValidated => "plan_validated",
            Self::InjectionBlocked => "injection_blocked",
            Self::ToolExecutionStarted => "tool_started",
            Self::ToolExecutionCompleted => "tool_completed",
            Self::AgentTurnStarted => "agent_turn_started",
            Self::AgentTurnCompleted => "agent_turn_completed",
            Self::SkillExecuted => "skill_executed",
            Self::MemoryStored => "memory_stored",
            Self::MemoryRecalled => "memory_recalled",
            Self::ChannelConnected => "channel_connected",
            Self::ChannelDisconnected => "channel_disconnected",
            Self::SystemStartup => "system_startup",
            Self::SystemShutdown => "system_shutdown",
        }
    }
}

/// A single persisted dashboard event.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StoredDashboardEvent {
    /// UUID v4.
    pub id: String,
    /// Event kind slug (see [`DashboardEventKind::as_str`]).
    pub kind: String,
    /// JSON-serialised event payload whose shape varies by `kind`.
    pub payload: serde_json::Value,
    /// ISO-8601 UTC timestamp of when the event was recorded.
    pub recorded_at: String,
}

/// Aggregate statistics returned by the dashboard stats RPC.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct DashboardStats {
    /// Total number of events currently stored.
    pub total_events: u64,
    /// Guardian N1 blocks.
    pub guardian_blocked: u64,
    /// Guardian N2 heuristic blocks.
    pub n2_blocked: u64,
    /// N3 LLM verdicts: allowed.
    pub n3_approved: u64,
    /// N3 LLM verdicts: blocked.
    pub n3_rejected: u64,
    /// Tool executions (started + completed).
    pub tool_count: u64,
    /// Memory stores.
    pub memory_count: u64,
    /// Skill executions.
    pub skill_count: u64,
    /// Active skills.
    pub active_skill_count: u64,
}

/// Summary of an installed skill for dashboard display.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SkillSummary {
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub gpg_verified: bool,
    pub description: Option<String>,
}
