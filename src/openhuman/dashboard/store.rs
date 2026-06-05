//! SQLite-backed event store for the dashboard domain.
//!
//! Stores a rolling window of [`DomainEvent`]s for the dashboard UI.
//! Follows the vault store's `with_connection` pattern.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::openhuman::config::Config;

use super::types::{DashboardStats, StoredDashboardEvent};

/// Global singleton — initialised once at startup via [`init_global`].
static GLOBAL_DASHBOARD_STORE: OnceLock<Arc<Mutex<DashboardEventStore>>> = OnceLock::new();

// ── Public API ────────────────────────────────────────────────────────────

/// Initialise the global dashboard event store.
///
/// Must be called once during core bootstrap. Subsequent calls are no-ops.
pub fn init_global(config: &Config) -> Result<()> {
    if GLOBAL_DASHBOARD_STORE.get().is_some() {
        log::debug!("[dashboard] store already initialised — skipping");
        return Ok(());
    }

    let store = DashboardEventStore::open(config)?;
    GLOBAL_DASHBOARD_STORE
        .set(Arc::new(Mutex::new(store)))
        .map_err(|_| anyhow::anyhow!("[dashboard] store already initialised (race)"))?;

    log::info!("[dashboard] event store initialised");
    Ok(())
}

/// Return a cloned `Arc` to the global store.
///
/// Returns `None` when [`init_global`] has not been called.
pub fn global() -> Option<Arc<Mutex<DashboardEventStore>>> {
    GLOBAL_DASHBOARD_STORE.get().cloned()
}

// ── Store ──────────────────────────────────────────────────────────────────

/// Append-only event log stored in `~/.openhuman/dashboard/dashboard.db`.
pub struct DashboardEventStore {
    db_path: PathBuf,
}

impl DashboardEventStore {
    /// Open (or create) the dashboard database and ensure the schema is current.
    pub fn open(config: &Config) -> Result<Self> {
        let db_dir = config.workspace_dir.join("dashboard");
        std::fs::create_dir_all(&db_dir)
            .with_context(|| format!("Failed to create dashboard dir: {}", db_dir.display()))?;

        let db_path = db_dir.join("dashboard.db");
        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open dashboard DB: {}", db_path.display()))?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = OFF;
             CREATE TABLE IF NOT EXISTS dashboard_events (
                 id          TEXT PRIMARY KEY,
                 kind        TEXT NOT NULL,
                 payload     TEXT NOT NULL,
                 recorded_at TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_dashboard_events_recorded_at
                 ON dashboard_events(recorded_at DESC);
             CREATE INDEX IF NOT EXISTS idx_dashboard_events_kind
                 ON dashboard_events(kind);",
        )
        .context("Failed to initialise dashboard schema")?;

        log::debug!("[dashboard] store opened at {}", db_path.display());
        Ok(Self { db_path })
    }

    /// Open a connection to the database and run a closure.
    fn with_connection<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("Failed to open dashboard DB: {}", self.db_path.display()))?;
        f(&conn)
    }

    /// Insert a single dashboard event.
    pub fn insert(&self, id: &str, kind: &str, payload: &serde_json::Value, recorded_at: &str) -> Result<()> {
        self.with_connection(|conn| {
            conn.execute(
                "INSERT INTO dashboard_events (id, kind, payload, recorded_at) VALUES (?1, ?2, ?3, ?4)",
                params![id, kind, payload.to_string(), recorded_at],
            )?;
            Ok(())
        })
    }

    /// Return the most recent events, optionally filtered by kind.
    pub fn list_recent(
        &self,
        limit: u64,
        kind_filter: Option<&str>,
    ) -> Result<Vec<StoredDashboardEvent>> {
        self.with_connection(|conn| {
            let mut stmt = if let Some(kind) = kind_filter {
                let mut s = conn.prepare(
                    "SELECT id, kind, payload, recorded_at FROM dashboard_events
                     WHERE kind = ?1
                     ORDER BY recorded_at DESC LIMIT ?2",
                )?;
                let rows = s.query_map(params![kind, limit], |row| {
                    Ok(StoredDashboardEvent {
                        id: row.get(0)?,
                        kind: row.get(1)?,
                        payload: {
                            let raw: String = row.get(2)?;
                            serde_json::from_str(&raw).unwrap_or_default()
                        },
                        recorded_at: row.get(3)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
            } else {
                let mut s = conn.prepare(
                    "SELECT id, kind, payload, recorded_at FROM dashboard_events
                     ORDER BY recorded_at DESC LIMIT ?1",
                )?;
                let rows = s.query_map(params![limit], |row| {
                    Ok(StoredDashboardEvent {
                        id: row.get(0)?,
                        kind: row.get(1)?,
                        payload: {
                            let raw: String = row.get(2)?;
                            serde_json::from_str(&raw).unwrap_or_default()
                        },
                        recorded_at: row.get(3)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
            };
            stmt.map_err(|e| anyhow::anyhow!("Failed to list dashboard events: {e}"))
        })
    }

    /// Compute aggregate statistics from the stored events.
    pub fn get_stats(&self) -> Result<DashboardStats> {
        self.with_connection(|conn| {
            let mut stats = DashboardStats::default();

            stats.total_events = conn
                .query_row("SELECT COUNT(*) FROM dashboard_events", [], |row| {
                    row.get::<_, u64>(0)
                })
                .unwrap_or(0);

            stats.guardian_blocked = conn
                .query_row(
                    "SELECT COUNT(*) FROM dashboard_events WHERE kind = 'guardian_blocked'",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap_or(0);

            stats.n2_blocked = conn
                .query_row(
                    "SELECT COUNT(*) FROM dashboard_events WHERE kind = 'n2_blocked'",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap_or(0);

            stats.n3_approved = conn
                .query_row(
                    "SELECT COUNT(*) FROM dashboard_events WHERE kind = 'n3_result' AND json_extract(payload, '$.verdict') = 'allow'",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap_or(0);

            stats.n3_rejected = conn
                .query_row(
                    "SELECT COUNT(*) FROM dashboard_events WHERE kind = 'n3_result' AND json_extract(payload, '$.verdict') = 'block'",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap_or(0);

            stats.tool_count = conn
                .query_row(
                    "SELECT COUNT(*) FROM dashboard_events WHERE kind IN ('tool_started', 'tool_completed')",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap_or(0);

            stats.memory_count = conn
                .query_row(
                    "SELECT COUNT(*) FROM dashboard_events WHERE kind IN ('memory_stored', 'memory_recalled')",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap_or(0);

            stats.skill_count = conn
                .query_row(
                    "SELECT COUNT(*) FROM dashboard_events WHERE kind = 'skill_executed'",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap_or(0);

            Ok(stats)
        })
    }

    /// Delete events older than `retention_days`.
    pub fn prune_older_than(&self, retention_days: u64) -> Result<usize> {
        self.with_connection(|conn| {
            // SQLite doesn't have INTERVAL, so we compute the cutoff in Rust.
            let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
            let cutoff_str = cutoff.to_rfc3339();

            let deleted = conn.execute(
                "DELETE FROM dashboard_events WHERE recorded_at < ?1",
                params![cutoff_str],
            )?;

            log::debug!("[dashboard] pruned {deleted} events older than {retention_days} days");
            Ok(deleted)
        })
    }

    /// Delete the oldest events when the count exceeds `max_events`.
    pub fn enforce_max_events(&self, max_events: u64) -> Result<usize> {
        self.with_connection(|conn| {
            let count: u64 = conn
                .query_row("SELECT COUNT(*) FROM dashboard_events", [], |row| {
                    row.get(0)
                })?;

            if count <= max_events {
                return Ok(0);
            }

            let excess = count - max_events;
            conn.execute(
                "DELETE FROM dashboard_events WHERE id IN (
                     SELECT id FROM dashboard_events ORDER BY recorded_at ASC LIMIT ?1
                 )",
                params![excess],
            )?;

            log::debug!("[dashboard] pruned {excess} events to respect max_events={max_events}");
            Ok(excess as usize)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(dir: &TempDir) -> Config {
        let mut cfg = Config::default();
        cfg.workspace_dir = dir.path().to_path_buf();
        cfg
    }

    #[test]
    fn open_creates_database() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = test_config(&dir);
        let store = DashboardEventStore::open(&cfg).unwrap();
        assert!(store.db_path.exists());
    }

    #[test]
    fn insert_and_list_recent() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = test_config(&dir);
        let store = DashboardEventStore::open(&cfg).unwrap();

        let payload = serde_json::json!({"tool_name": "shell", "reason": "blocked"});
        store
            .insert("evt-1", "guardian_blocked", &payload, "2026-06-05T00:00:00Z")
            .unwrap();

        let events = store.list_recent(10, None).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "evt-1");
        assert_eq!(events[0].kind, "guardian_blocked");
    }

    #[test]
    fn list_recent_respects_kind_filter() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = test_config(&dir);
        let store = DashboardEventStore::open(&cfg).unwrap();

        store
            .insert("a", "guardian_blocked", &serde_json::json!({}), "2026-01-01T00:00:00Z")
            .unwrap();
        store
            .insert("b", "n2_blocked", &serde_json::json!({}), "2026-01-02T00:00:00Z")
            .unwrap();

        let filtered = store.list_recent(10, Some("guardian_blocked")).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "a");
    }

    #[test]
    fn get_stats_returns_zeros_for_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = test_config(&dir);
        let store = DashboardEventStore::open(&cfg).unwrap();

        let stats = store.get_stats().unwrap();
        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.guardian_blocked, 0);
    }

    #[test]
    fn get_stats_aggregates_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = test_config(&dir);
        let store = DashboardEventStore::open(&cfg).unwrap();

        store
            .insert("1", "guardian_blocked", &serde_json::json!({}), "2026-01-01T00:00:00Z")
            .unwrap();
        store
            .insert("2", "guardian_blocked", &serde_json::json!({}), "2026-01-02T00:00:00Z")
            .unwrap();
        store
            .insert("3", "n3_result", &serde_json::json!({"verdict": "block"}), "2026-01-03T00:00:00Z")
            .unwrap();
        store
            .insert("4", "n3_result", &serde_json::json!({"verdict": "allow"}), "2026-01-04T00:00:00Z")
            .unwrap();

        let stats = store.get_stats().unwrap();
        assert_eq!(stats.total_events, 4);
        assert_eq!(stats.guardian_blocked, 2);
        assert_eq!(stats.n3_rejected, 1);
        assert_eq!(stats.n3_approved, 1);
    }

    #[test]
    fn prune_deletes_old_events() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = test_config(&dir);
        let store = DashboardEventStore::open(&cfg).unwrap();

        let old = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        let recent = chrono::Utc::now().to_rfc3339();

        store
            .insert("old", "guardian_blocked", &serde_json::json!({}), &old)
            .unwrap();
        store
            .insert("new", "guardian_blocked", &serde_json::json!({}), &recent)
            .unwrap();

        let pruned = store.prune_older_than(14).unwrap();
        assert!(pruned >= 1, "expected at least 1 pruned, got {pruned}");

        let remaining = store.list_recent(10, None).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "new");
    }

    #[test]
    fn enforce_max_events_keeps_most_recent() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = test_config(&dir);
        let store = DashboardEventStore::open(&cfg).unwrap();

        for i in 0..5u64 {
            store
                .insert(
                    &format!("evt-{i}"),
                    "guardian_blocked",
                    &serde_json::json!({}),
                    &format!("2026-01-0{}T00:00:00Z", i + 1),
                )
                .unwrap();
        }

        let removed = store.enforce_max_events(3).unwrap();
        assert!(removed > 0);

        let remaining = store.list_recent(10, None).unwrap();
        assert_eq!(remaining.len(), 3);
        // Most recent (highest timestamp) should be kept
        assert!(remaining.iter().any(|e| e.id == "evt-4"));
        assert!(remaining.iter().any(|e| e.id == "evt-3"));
        assert!(remaining.iter().any(|e| e.id == "evt-2"));
    }

    #[test]
    fn init_global_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::default();
        cfg.workspace_dir = dir.path().to_path_buf();

        // First call — succeeds.
        init_global(&cfg).unwrap();
        // Second call — no-op, no error.
        init_global(&cfg).unwrap();

        let store = global();
        assert!(store.is_some());
    }
}
