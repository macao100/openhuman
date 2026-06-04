//! DADOU session context — cross-session continuity via save/restore.
//!
//! This domain captures the active working context at each session boundary
//! (active project, phase, last topic) and persists it to SQLite so DADOU
//! can resume where it left off after a restart.
//!
//! ## Modules
//!
//! - [`types`] — `SessionState` struct.
//! - [`store`] — SQLite CRUD via `rusqlite::Connection`.
//! - [`ops`] — Save/restore orchestration and periodic save loop.
//! - [`schemas`] — Controller schemas for JSON-RPC / CLI.
//!
//! ## Global State
//!
//! - `RESTORED_STATE` — Optionally holds a `SessionState` recovered from the
//!   database at startup. The agent checks this slot at session init and
//!   injects a continuity message if present.
//! - `CURRENT_STATE` — Holds the most recently saved (or default) session
//!   state so shutdown hooks and periodic saves can persist it without
//!   re-assembling the fields every time.
//! - `WORKSPACE_DIR` — Stored at startup so shutdown hooks can open the
//!   memory DB without capturing a reference to Config.
//!
//! ## Namespace
//!
//! All session context data is stored in the `dadou_session_context` SQLite
//! table under a single KV key `"dadou:active_session"`.

pub mod ops;
pub mod schemas;
pub mod store;
pub mod types;

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use parking_lot::Mutex as ParkingMutex;

// ── Global state slots ─────────────────────────────────────────────────────────

/// Holds a `SessionState` recovered from the database at startup, if any.
///
/// The agent checks this slot at session init and injects a continuity
/// message when present. Cleared (taken) on first read.
static RESTORED_STATE: OnceLock<Mutex<Option<types::SessionState>>> = OnceLock::new();

/// Holds the most recently saved session state for periodic save / shutdown.
///
/// Updated by `save_session_context()` and the agent. The periodic save loop
/// and shutdown hook persist whatever is in this slot.
static CURRENT_STATE: OnceLock<ParkingMutex<types::SessionState>> = OnceLock::new();

/// Workspace directory path, stored at startup so shutdown hooks can open
/// the memory DB without a Config reference.
static WORKSPACE_DIR: OnceLock<PathBuf> = OnceLock::new();

// ── Public API for global state ────────────────────────────────────────────────

/// Store a restored session state so the agent can retrieve it at session init.
pub fn set_restored_state(state: types::SessionState) {
    let slot = RESTORED_STATE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(state);
    }
}

/// Take the restored session state (if any), clearing the slot.
///
/// Returns `None` if no state was restored or if it has already been taken.
pub fn take_restored_state() -> Option<types::SessionState> {
    let slot = RESTORED_STATE.get_or_init(|| Mutex::new(None));
    let mut guard = slot.lock().ok()?;
    guard.take()
}

/// Update the current (live) session state — used by the agent and periodic save.
pub fn update_current_state(state: types::SessionState) {
    let slot = CURRENT_STATE.get_or_init(|| ParkingMutex::new(types::SessionState::default()));
    *slot.lock() = state;
}

/// Read a clone of the current session state.
pub fn current_state() -> types::SessionState {
    let slot = CURRENT_STATE.get_or_init(|| ParkingMutex::new(types::SessionState::default()));
    slot.lock().clone()
}

/// Store the workspace directory path for use by shutdown hooks.
pub fn set_workspace_dir(path: PathBuf) {
    let _ = WORKSPACE_DIR.set(path);
}

/// Retrieve the stored workspace directory path.
pub fn workspace_dir() -> Option<&'static PathBuf> {
    WORKSPACE_DIR.get()
}
