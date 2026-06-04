//! Types for the `dadou_session_context` namespace.
//!
//! `SessionState` captures the active working context at a point in time:
//! which project and phase the user was working on, the last conversation
//! topic, and a timestamp. It survives restarts via SQLite persistence.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Persistent session state that survives restarts.
///
/// Written on graceful shutdown (and periodically every 5 minutes) so the
/// agent can resume context on the next startup — active project, phase,
/// last topic, and an opaque `extensions` JSON blob for future metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionState {
    /// Active project name (e.g. "openhuman", "dadou").
    pub active_project: Option<String>,
    /// Active phase within the project (e.g. "02-memory-continuity").
    pub active_phase: Option<String>,
    /// Last active conversation topic (1-2 sentence summary).
    pub last_topic: Option<String>,
    /// ISO 8601 timestamp of the last agent turn.
    pub last_activity_at: String,
    /// Version tag for schema evolution.
    pub version: u32,
    /// Opaque JSON blob for future extension (pending decisions, working context).
    #[serde(default)]
    pub extensions: serde_json::Value,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            active_project: None,
            active_phase: None,
            last_topic: None,
            last_activity_at: Utc::now().to_rfc3339(),
            version: 1,
            extensions: serde_json::Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test 1: SessionState serializes/deserializes all fields ──

    #[test]
    fn session_state_round_trips_all_fields() {
        let state = SessionState {
            active_project: Some("dadou".to_string()),
            active_phase: Some("02-memory-continuity".to_string()),
            last_topic: Some("Working on cross-session continuity".to_string()),
            last_activity_at: "2026-06-04T12:00:00Z".to_string(),
            version: 1,
            extensions: serde_json::json!({"pending_decision": "choose DB path"}),
        };

        let json = serde_json::to_string(&state).unwrap();
        let back: SessionState = serde_json::from_str(&json).unwrap();

        assert_eq!(back, state);
    }

    // ── Test 2: SessionState with None optionals ──

    #[test]
    fn session_state_with_none_optionals() {
        let state = SessionState {
            active_project: None,
            active_phase: None,
            last_topic: None,
            last_activity_at: "2026-06-04T12:00:00Z".to_string(),
            version: 1,
            extensions: serde_json::Value::Null,
        };

        let json = serde_json::to_string(&state).unwrap();
        let back: SessionState = serde_json::from_str(&json).unwrap();

        assert_eq!(back, state);
        assert!(back.active_project.is_none());
    }

    // ── Test 3: Default provides sane defaults ──

    #[test]
    fn session_state_default_is_valid() {
        let state = SessionState::default();
        assert_eq!(state.version, 1);
        assert!(state.active_project.is_none());
        assert!(state.active_phase.is_none());
        assert!(state.last_topic.is_none());
        assert!(state.extensions.is_null());
        // last_activity_at should be a valid RFC 3339 timestamp
        assert!(
            DateTime::parse_from_rfc3339(&state.last_activity_at).is_ok(),
            "default timestamp must be valid RFC 3339"
        );
    }

    // ── Test 4: extensions defaults to null when missing in JSON ──

    #[test]
    fn extensions_defaults_to_null_when_missing() {
        let json = r#"{
            "active_project": null,
            "active_phase": null,
            "last_topic": null,
            "last_activity_at": "2026-06-04T12:00:00Z",
            "version": 1
        }"#;

        let state: SessionState = serde_json::from_str(json).unwrap();
        assert!(state.extensions.is_null());
    }
}
