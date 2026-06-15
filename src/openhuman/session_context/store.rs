//! SQLite persistence for session context state.
//!
//! Uses a dedicated `dadou_session_context` table with a single-row KV
//! pattern. The store uses a direct `rusqlite::Connection` — session
//! save/restore is a synchronous startup/shutdown path.
//!
//! # Table schema
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS dadou_session_context (
//!     key TEXT PRIMARY KEY,
//!     value_json TEXT NOT NULL,
//!     updated_at REAL NOT NULL
//! );
//! ```

use rusqlite::{params, Connection};

use super::types::SessionState;

/// Single-row KV key for the active session.
const SESSION_CONTEXT_KEY: &str = "dadou:active_session";

/// Initialise the `dadou_session_context` table.
///
/// Safe to call multiple times — uses `IF NOT EXISTS`.
pub fn init_table(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS dadou_session_context (
            key TEXT PRIMARY KEY,
            value_json TEXT NOT NULL,
            updated_at REAL NOT NULL
        )",
        [],
    )?;
    Ok(())
}

/// Persist a session state to the database (upsert by key).
pub fn save_session(conn: &Connection, state: &SessionState) -> anyhow::Result<()> {
    let value_json = serde_json::to_string(state)?;
    let updated_at = chrono::Utc::now().timestamp() as f64;
    conn.execute(
        "INSERT INTO dadou_session_context (key, value_json, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET
             value_json = excluded.value_json,
             updated_at = excluded.updated_at",
        params![SESSION_CONTEXT_KEY, value_json, updated_at],
    )?;
    Ok(())
}

/// Load the saved session state, if one exists.
///
/// Returns `None` if no session has been saved yet (clean first start).
pub fn load_session(conn: &Connection) -> anyhow::Result<Option<SessionState>> {
    let mut stmt = conn.prepare("SELECT value_json FROM dadou_session_context WHERE key = ?1")?;
    let mut rows = stmt.query(params![SESSION_CONTEXT_KEY])?;
    match rows.next()? {
        Some(row) => {
            let value_json: String = row.get(0)?;
            let state: SessionState = serde_json::from_str(&value_json)?;
            Ok(Some(state))
        }
        None => Ok(None),
    }
}

/// Delete the saved session state.
///
/// Returns `true` if a row was actually removed, `false` if none existed.
pub fn delete_session(conn: &Connection) -> anyhow::Result<bool> {
    let affected = conn.execute(
        "DELETE FROM dadou_session_context WHERE key = ?1",
        params![SESSION_CONTEXT_KEY],
    )?;
    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create a temporary in-memory SQLite DB with the session context table.
    fn setup_db() -> (TempDir, Connection) {
        let tmp = TempDir::new().unwrap();
        let conn = Connection::open(tmp.path().join("test.db")).unwrap();
        init_table(&conn).unwrap();
        (tmp, conn)
    }

    fn sample_state() -> SessionState {
        SessionState {
            active_project: Some("dadou".to_string()),
            active_phase: Some("02-memory-continuity".to_string()),
            last_topic: Some("Cross-session continuity implementation".to_string()),
            last_activity_at: "2026-06-04T12:00:00Z".to_string(),
            version: 1,
            extensions: serde_json::json!({"test": true}),
        }
    }

    // ── Test 1: save_session persists and load_session retrieves ──

    #[test]
    fn test_save_and_load_session() {
        let (_tmp, conn) = setup_db();
        let state = sample_state();

        save_session(&conn, &state).unwrap();

        let loaded = load_session(&conn)
            .unwrap()
            .expect("session should exist after save");
        assert_eq!(loaded.active_project, Some("dadou".to_string()));
        assert_eq!(
            loaded.active_phase,
            Some("02-memory-continuity".to_string())
        );
        assert_eq!(
            loaded.last_topic,
            Some("Cross-session continuity implementation".to_string())
        );
        assert_eq!(loaded.version, 1);
    }

    // ── Test 2: load_session returns None when no session saved ──

    #[test]
    fn test_load_session_returns_none_when_empty() {
        let (_tmp, conn) = setup_db();
        let result = load_session(&conn).unwrap();
        assert!(result.is_none(), "no session should exist on fresh DB");
    }

    // ── Test 3: delete_session removes state and returns true ──

    #[test]
    fn test_delete_session() {
        let (_tmp, conn) = setup_db();
        let state = sample_state();

        save_session(&conn, &state).unwrap();
        assert!(load_session(&conn).unwrap().is_some());

        let deleted = delete_session(&conn).unwrap();
        assert!(deleted, "delete should return true when row existed");

        assert!(load_session(&conn).unwrap().is_none());
    }

    // ── Test 4: delete_session returns false when nothing to delete ──

    #[test]
    fn test_delete_session_returns_false_when_empty() {
        let (_tmp, conn) = setup_db();
        let deleted = delete_session(&conn).unwrap();
        assert!(
            !deleted,
            "delete should return false when no session exists"
        );
    }

    // ── Test 5: Multiple save/load cycles preserve data integrity ──

    #[test]
    fn test_multiple_save_cycles() {
        let (_tmp, conn) = setup_db();

        let state1 = SessionState {
            active_project: Some("project-a".to_string()),
            ..sample_state()
        };
        save_session(&conn, &state1).unwrap();

        let state2 = SessionState {
            active_project: Some("project-b".to_string()),
            last_topic: Some("Second topic".to_string()),
            ..sample_state()
        };
        save_session(&conn, &state2).unwrap();

        let loaded = load_session(&conn).unwrap().expect("session should exist");
        // After second save, we should get state2 (upsert)
        assert_eq!(loaded.active_project, Some("project-b".to_string()));
        assert_eq!(loaded.last_topic, Some("Second topic".to_string()));
    }

    // ── Test 6: Session with empty optional fields serializes correctly ──

    #[test]
    fn test_session_with_empty_optionals() {
        let (_tmp, conn) = setup_db();
        let state = SessionState {
            active_project: None,
            active_phase: None,
            last_topic: None,
            last_activity_at: "2026-06-04T12:00:00Z".to_string(),
            version: 1,
            extensions: serde_json::Value::Null,
        };

        save_session(&conn, &state).unwrap();
        let loaded = load_session(&conn).unwrap().expect("session should exist");
        assert!(loaded.active_project.is_none());
        assert!(loaded.active_phase.is_none());
        assert!(loaded.last_topic.is_none());
    }
}
