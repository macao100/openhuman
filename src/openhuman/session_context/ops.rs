//! Save/restore orchestration for session context.
//!
//! Provides the entry points called at startup (restore), shutdown (save),
//! and periodically (periodic save loop). All I/O goes through
//! `super::store` which operates on a direct `rusqlite::Connection`.

use std::path::Path;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::store;
use super::types::SessionState;
use crate::openhuman::session_context;

/// Path to the memory DB relative to the workspace directory.
const MEMORY_DB_RELATIVE: &str = "memory/memory.db";

/// Build the absolute path to the memory SQLite DB from a workspace dir.
fn memory_db_path(workspace_dir: &Path) -> std::path::PathBuf {
    workspace_dir.join(MEMORY_DB_RELATIVE)
}

/// Open a connection to the memory DB.
///
/// Returns `None` if the DB file does not exist (e.g. first-ever startup
/// before any memory subsystem created it).
fn open_memory_db(workspace_dir: &Path) -> anyhow::Result<Option<rusqlite::Connection>> {
    let db_path = memory_db_path(workspace_dir);
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = rusqlite::Connection::open(&db_path)?;
    Ok(Some(conn))
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Save the current session state to persistent storage.
///
/// Called on shutdown and periodically. Reads from the `CURRENT_STATE`
/// global slot so callers don't need to construct a `SessionState`.
pub fn save_session_context(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    let state = session_context::current_state();
    store::init_table(conn)?;
    store::save_session(conn, &state)?;
    log::info!(
        "[session_context] saved (project={:?}, phase={:?})",
        state.active_project,
        state.active_phase
    );
    Ok(())
}

/// Restore session state from the database on startup.
///
/// Returns `None` if no saved session exists (clean first start).
/// On success, stores the state in `RESTORED_STATE` for the agent.
pub fn restore_session_context(workspace_dir: &Path) -> anyhow::Result<Option<SessionState>> {
    let conn = match open_memory_db(workspace_dir)? {
        Some(c) => c,
        None => {
            log::info!("[session_context] no memory DB yet — skipping restore");
            return Ok(None);
        }
    };

    store::init_table(&conn)?;
    let state = store::load_session(&conn)?;

    if let Some(ref s) = state {
        log::info!(
            "[session_context] restored (project={:?}, phase={:?}, topic={:?})",
            s.active_project,
            s.active_phase,
            s.last_topic
        );
        session_context::set_restored_state(s.clone());
        session_context::update_current_state(s.clone());
    } else {
        log::info!("[session_context] no saved session found — starting fresh");
    }

    Ok(state)
}

/// Initialise the session context subsystem at startup.
///
/// Stores the workspace dir, initialises the DB table, and restores the
/// previous session if one exists.  Called after `memory::global::init()`.
pub fn init_session_context(workspace_dir: &Path) {
    session_context::set_workspace_dir(workspace_dir.to_path_buf());

    match open_memory_db(workspace_dir) {
        Ok(Some(conn)) => {
            if let Err(e) = store::init_table(&conn) {
                log::warn!("[session_context] table init failed: {e}");
                return;
            }
            match store::load_session(&conn) {
                Ok(Some(state)) => {
                    log::info!(
                        "[session_context] restored (project={:?}, phase={:?})",
                        state.active_project,
                        state.active_phase
                    );
                    session_context::set_restored_state(state.clone());
                    session_context::update_current_state(state);
                }
                Ok(None) => {
                    log::info!("[session_context] no saved session — fresh start");
                }
                Err(e) => {
                    log::warn!("[session_context] load failed: {e}");
                }
            }
        }
        Ok(None) => {
            log::info!("[session_context] memory DB not found — session context deferred");
        }
        Err(e) => {
            log::warn!("[session_context] open memory DB failed: {e}");
        }
    }
}

/// Save session context on shutdown.
///
/// Opens the memory DB from the stored workspace dir and persists the
/// current session state.  Called from cleanup code after the RPC server
/// stops (or from a shutdown hook).
pub fn save_on_shutdown() {
    let ws_dir = match session_context::workspace_dir() {
        Some(d) => d.clone(),
        None => {
            log::warn!("[session_context] save_on_shutdown: workspace_dir not set");
            return;
        }
    };

    let conn = match open_memory_db(&ws_dir) {
        Ok(Some(c)) => c,
        Ok(None) => {
            log::info!("[session_context] save_on_shutdown: memory DB missing");
            return;
        }
        Err(e) => {
            log::warn!("[session_context] save_on_shutdown: open error: {e}");
            return;
        }
    };

    if let Err(e) = save_session_context(&conn) {
        log::warn!("[session_context] save_on_shutdown failed: {e}");
    }
}

/// Periodic save loop — runs every 5 minutes to persist current context.
///
/// `cancel` fires on shutdown to break the loop cleanly.
pub async fn periodic_save_loop(cancel: CancellationToken) {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    // Skip the first immediate tick
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let ws_dir = match session_context::workspace_dir() {
                    Some(d) => d.clone(),
                    None => continue,
                };
                let conn = match open_memory_db(&ws_dir) {
                    Ok(Some(c)) => c,
                    _ => continue,
                };
                if let Err(e) = save_session_context(&conn) {
                    log::warn!("[session_context] periodic save failed: {e}");
                }
            }
            _ = cancel.cancelled() => {
                log::info!("[session_context] periodic save loop stopped");
                break;
            }
        }
    }
}

/// Register a shutdown hook (via `core::shutdown`) that saves session context
/// on graceful exit in standalone mode.
///
/// In embedded mode the save happens in `run_server_inner` cleanup; this
/// covers the standalone / CLI path.
pub fn register_shutdown_hook() {
    crate::core::shutdown::register(|| async {
        log::info!("[session_context] shutdown hook: saving session");
        save_on_shutdown();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create a temp workspace dir with an initialised memory DB.
    fn setup_workspace() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let db_dir = tmp.path().join("memory");
        std::fs::create_dir_all(&db_dir).unwrap();
        let conn = rusqlite::Connection::open(db_dir.join("memory.db")).unwrap();
        store::init_table(&conn).unwrap();
        tmp
    }

    fn sample_state() -> SessionState {
        SessionState {
            active_project: Some("dadou".to_string()),
            active_phase: Some("02-memory-continuity".to_string()),
            last_topic: Some("Test topic".to_string()),
            last_activity_at: "2026-06-04T12:00:00Z".to_string(),
            version: 1,
            extensions: serde_json::Value::Null,
        }
    }

    // ── Test 1: save_session_context writes state to the store ──

    #[test]
    fn test_save_session_context_writes_state() {
        let tmp = setup_workspace();
        let db_path = tmp.path().join("memory/memory.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        store::init_table(&conn).unwrap();

        session_context::update_current_state(sample_state());
        save_session_context(&conn).unwrap();

        let loaded = store::load_session(&conn)
            .unwrap()
            .expect("state should exist");
        assert_eq!(loaded.active_project, Some("dadou".to_string()));
    }

    // ── Test 2: restore_session_context reads state from the store ──

    #[test]
    fn test_restore_session_context_reads_state() {
        let tmp = setup_workspace();
        let db_path = tmp.path().join("memory/memory.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        store::init_table(&conn).unwrap();
        store::save_session(&conn, &sample_state()).unwrap();
        drop(conn);

        let restored = restore_session_context(tmp.path())
            .unwrap()
            .expect("should restore state");
        assert_eq!(restored.active_project, Some("dadou".to_string()));
    }

    // ── Test 3: When no session exists, restore returns None (not error) ──

    #[test]
    fn test_restore_returns_none_when_empty() {
        let tmp = TempDir::new().unwrap();
        let db_dir = tmp.path().join("memory");
        std::fs::create_dir_all(&db_dir).unwrap();
        let conn = rusqlite::Connection::open(db_dir.join("memory.db")).unwrap();
        store::init_table(&conn).unwrap();
        drop(conn);

        let result = restore_session_context(tmp.path()).unwrap();
        assert!(result.is_none(), "should be None when no session saved");
    }

    // ── Test 4: init_session_context restores existing state ──

    #[test]
    fn test_init_session_context_restores() {
        let tmp = setup_workspace();
        let db_path = tmp.path().join("memory/memory.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        store::init_table(&conn).unwrap();
        store::save_session(&conn, &sample_state()).unwrap();
        drop(conn);

        init_session_context(tmp.path());

        let taken = session_context::take_restored_state();
        assert!(taken.is_some(), "restored state should be available");
        assert_eq!(taken.unwrap().active_project, Some("dadou".to_string()));
    }
}
