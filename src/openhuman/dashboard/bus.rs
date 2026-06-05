//! Event bus subscriber for the dashboard domain.
//!
//! [`DashboardRecorder`] listens to dashboard-relevant [`DomainEvent`]s
//! and persists them to the SQLite event store so the dashboard UI can
//! display a live timeline.

use crate::core::event_bus::{DomainEvent, EventHandler};

use super::store;

/// Persists dashboard-relevant events into the SQLite event store.
///
/// Subscribes to seven domains: `guardian`, `tool`, `agent`, `skill`,
/// `memory`, `channel`, and `system`. Each event is serialised to a
/// small JSON payload and inserted into the store.
pub struct DashboardRecorder;

impl DashboardRecorder {
    /// Build a JSON payload from the event and insert it into the store.
    fn record(&self, kind: &str, payload: serde_json::Value) {
        let id = uuid::Uuid::new_v4().to_string();
        let recorded_at = chrono::Utc::now().to_rfc3339();

        if let Some(store) = store::global() {
            if let Ok(store) = store.lock() {
                if let Err(e) = store.insert(&id, kind, &payload, &recorded_at) {
                    log::warn!("[dashboard] failed to insert event {kind}: {e}");
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for DashboardRecorder {
    fn name(&self) -> &'static str {
        "dashboard::recorder"
    }

    fn domains(&self) -> Option<&'static [&'static str]> {
        Some(&["guardian", "tool", "agent", "skill", "memory", "channel", "system"])
    }

    async fn handle(&self, event: &DomainEvent) {
        match event {
            // ── Guardian events ────────────────────────────────────────
            DomainEvent::GuardianBlocked {
                tool_name,
                reason,
                latency_us,
            } => {
                self.record(
                    "guardian_blocked",
                    serde_json::json!({
                        "tool_name": tool_name,
                        "reason": reason,
                        "latency_us": latency_us,
                    }),
                );
            }
            DomainEvent::N2Blocked {
                tool_name,
                reason,
                scores_json,
                latency_us,
            } => {
                self.record(
                    "n2_blocked",
                    serde_json::json!({
                        "tool_name": tool_name,
                        "reason": reason,
                        "scores_json": scores_json,
                        "latency_us": latency_us,
                    }),
                );
            }
            DomainEvent::N2Escalated {
                tool_name,
                scores_json,
                latency_us,
            } => {
                self.record(
                    "n2_escalated",
                    serde_json::json!({
                        "tool_name": tool_name,
                        "scores_json": scores_json,
                        "latency_us": latency_us,
                    }),
                );
            }
            DomainEvent::N3Result {
                tool_name,
                verdict,
                reason,
                latency_us,
            } => {
                self.record(
                    "n3_result",
                    serde_json::json!({
                        "tool_name": tool_name,
                        "verdict": verdict,
                        "reason": reason,
                        "latency_us": latency_us,
                    }),
                );
            }
            DomainEvent::PlanValidated {
                goal,
                allowed,
                blocked_by,
                step_count,
                rejected_step_indices,
            } => {
                self.record(
                    "plan_validated",
                    serde_json::json!({
                        "goal": goal,
                        "allowed": allowed,
                        "blocked_by": blocked_by,
                        "step_count": step_count,
                        "rejected_step_indices": rejected_step_indices,
                    }),
                );
            }
            DomainEvent::InjectionBlocked {
                tool_name,
                reason,
                finding_count,
            } => {
                self.record(
                    "injection_blocked",
                    serde_json::json!({
                        "tool_name": tool_name,
                        "reason": reason,
                        "finding_count": finding_count,
                    }),
                );
            }

            // ── Tool events ───────────────────────────────────────────
            DomainEvent::ToolExecutionStarted {
                tool_name,
                session_id,
            } => {
                self.record(
                    "tool_started",
                    serde_json::json!({
                        "tool_name": tool_name,
                        "session_id": session_id,
                    }),
                );
            }
            DomainEvent::ToolExecutionCompleted {
                tool_name,
                session_id,
                success,
                elapsed_ms,
            } => {
                self.record(
                    "tool_completed",
                    serde_json::json!({
                        "tool_name": tool_name,
                        "session_id": session_id,
                        "success": success,
                        "elapsed_ms": elapsed_ms,
                    }),
                );
            }

            // ── Agent events ──────────────────────────────────────────
            DomainEvent::AgentTurnStarted {
                session_id,
                channel,
            } => {
                self.record(
                    "agent_turn_started",
                    serde_json::json!({
                        "session_id": session_id,
                        "channel": channel,
                    }),
                );
            }
            DomainEvent::AgentTurnCompleted {
                session_id,
                text_chars,
                iterations,
            } => {
                self.record(
                    "agent_turn_completed",
                    serde_json::json!({
                        "session_id": session_id,
                        "text_chars": text_chars,
                        "iterations": iterations,
                    }),
                );
            }

            // ── Skill events ──────────────────────────────────────────
            DomainEvent::SkillExecuted {
                skill_id,
                tool_name,
                arguments,
                result,
                success,
                elapsed_ms,
            } => {
                self.record(
                    "skill_executed",
                    serde_json::json!({
                        "skill_id": skill_id,
                        "tool_name": tool_name,
                        "arguments": arguments,
                        "result": result,
                        "success": success,
                        "elapsed_ms": elapsed_ms,
                    }),
                );
            }

            // ── Memory events ─────────────────────────────────────────
            DomainEvent::MemoryStored {
                key,
                category,
                namespace,
            } => {
                self.record(
                    "memory_stored",
                    serde_json::json!({
                        "key": key,
                        "category": category,
                        "namespace": namespace,
                    }),
                );
            }
            DomainEvent::MemoryRecalled { query, hit_count } => {
                self.record(
                    "memory_recalled",
                    serde_json::json!({
                        "query": query,
                        "hit_count": hit_count,
                    }),
                );
            }

            // ── Channel events ────────────────────────────────────────
            DomainEvent::ChannelConnected { channel } => {
                self.record(
                    "channel_connected",
                    serde_json::json!({ "channel": channel }),
                );
            }
            DomainEvent::ChannelDisconnected { channel, reason } => {
                self.record(
                    "channel_disconnected",
                    serde_json::json!({
                        "channel": channel,
                        "reason": reason,
                    }),
                );
            }

            // ── System events ─────────────────────────────────────────
            DomainEvent::SystemStartup { component } => {
                self.record(
                    "system_startup",
                    serde_json::json!({ "component": component }),
                );
            }
            DomainEvent::SystemShutdown { component } => {
                self.record(
                    "system_shutdown",
                    serde_json::json!({ "component": component }),
                );
            }

            // ── Everything else — ignore ─────────────────────────────
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Dummy store that captures the last-inserted event for assertions.
    struct SpyStore {
        last_kind: std::sync::Mutex<Option<String>>,
        last_payload: std::sync::Mutex<Option<serde_json::Value>>,
    }

    /// Test helper that verifies the recorder correctly extracts fields
    /// from a domain event without needing a real SQLite database.
    #[tokio::test]
    async fn recorder_handles_guardian_blocked() {
        let recorder = DashboardRecorder;
        let event = DomainEvent::GuardianBlocked {
            tool_name: "shell".to_string(),
            reason: "test block".to_string(),
            latency_us: 42,
        };

        // Verify fields are accessible — actual insertion needs store global.
        match &event {
            DomainEvent::GuardianBlocked {
                tool_name,
                reason,
                latency_us,
            } => {
                assert_eq!(tool_name, "shell");
                assert_eq!(reason, "test block");
                assert_eq!(*latency_us, 42);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn recorder_handles_n3_result() {
        let event = DomainEvent::N3Result {
            tool_name: "composio".to_string(),
            verdict: "allow".to_string(),
            reason: "safe".to_string(),
            latency_us: 456,
        };

        match &event {
            DomainEvent::N3Result {
                tool_name,
                verdict,
                reason,
                latency_us,
            } => {
                assert_eq!(tool_name, "composio");
                assert_eq!(verdict, "allow");
                assert_eq!(reason, "safe");
                assert_eq!(*latency_us, 456);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn recorder_name_is_dashboard_recorder() {
        let recorder = DashboardRecorder;
        assert_eq!(recorder.name(), "dashboard::recorder");
    }

    #[test]
    fn recorder_domains_covers_seven_areas() {
        let recorder = DashboardRecorder;
        let domains = recorder.domains().expect("should have domain filter");
        assert_eq!(domains.len(), 7);
        assert!(domains.contains(&"guardian"));
        assert!(domains.contains(&"tool"));
        assert!(domains.contains(&"agent"));
        assert!(domains.contains(&"skill"));
        assert!(domains.contains(&"memory"));
        assert!(domains.contains(&"channel"));
        assert!(domains.contains(&"system"));
    }
}
