//! Confidence decay scheduler for memory provenance.
//!
//! Periodically scans `memory_docs` for entries whose `confidence` level
//! should be reduced or removed based on configurable time thresholds.
//!
//! - `Verified` entries older than `VERIFIED_DECAY_DAYS` are demoted to `Inferred`.
//! - `External` entries older than `EXTERNAL_EXPIRY_DAYS` are deleted.

use anyhow::Context as _;
use rusqlite::Connection;

use crate::openhuman::memory::provenance::types::{ConfidenceLevel, Provenance};

/// Number of days after which a `Verified` entry is demoted to `Inferred`
/// if it has not been re-confirmed by the user.
pub const VERIFIED_DECAY_DAYS: i64 = 30;

/// Number of days after which an `External` entry is deleted entirely.
pub const EXTERNAL_EXPIRY_DAYS: i64 = 7;

/// Report produced by a single decay pass.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct DecayReport {
    /// Number of entries demoted from Verified to Inferred.
    pub verified_demoted: usize,
    /// Number of External entries removed.
    pub external_removed: usize,
    /// Total entries affected by this decay pass.
    pub entries_affected: usize,
}

/// Seconds per day for timestamp comparisons.
const SECS_PER_DAY: f64 = 86_400.0;

/// Runs a single decay pass over all memory entries with provenance.
///
/// Scans `memory_docs` WHERE `provenance_json IS NOT NULL`, parses the
/// JSON, compares `confidence` + `updated_at` against thresholds, and
/// either demotes confidence or deletes the row.
pub fn decay_pass(conn: &Connection) -> anyhow::Result<DecayReport> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    // Collect entries that need decay processing.
    let mut stmt = conn
        .prepare(
            "SELECT document_id, provenance_json, updated_at
             FROM memory_docs
             WHERE provenance_json IS NOT NULL",
        )
        .context("prepare decay_pass select")?;

    let rows: Vec<(String, String, f64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })
        .context("query memory_docs for decay")?
        .filter_map(|r| r.ok())
        .collect();

    let mut verified_demoted = 0usize;
    let mut external_removed = 0usize;

    for (doc_id, prov_json, updated_at) in &rows {
        let provenance: Provenance = match serde_json::from_str(prov_json) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("[provenance::decay] failed to parse provenance_json for {doc_id}: {e}");
                continue;
            }
        };

        let age_days = (now - updated_at) / SECS_PER_DAY;

        match provenance.confidence {
            ConfidenceLevel::Verified if age_days > VERIFIED_DECAY_DAYS as f64 => {
                // Demote to Inferred
                let new_prov = Provenance {
                    confidence: ConfidenceLevel::Inferred,
                    ..provenance
                };
                let new_json =
                    serde_json::to_string(&new_prov).context("serialize demoted provenance")?;
                conn.execute(
                    "UPDATE memory_docs SET provenance_json = ?1 WHERE document_id = ?2",
                    rusqlite::params![new_json, doc_id],
                )
                .context("update demoted provenance")?;
                verified_demoted += 1;
                log::debug!(
                    "[provenance::decay] demoted {doc_id} from Verified to Inferred (age={age_days:.1}d)"
                );
            }
            ConfidenceLevel::External if age_days > EXTERNAL_EXPIRY_DAYS as f64 => {
                // Delete the entry
                conn.execute(
                    "DELETE FROM memory_docs WHERE document_id = ?1",
                    rusqlite::params![doc_id],
                )
                .context("delete expired external entry")?;
                external_removed += 1;
                log::debug!(
                    "[provenance::decay] removed expired external {doc_id} (age={age_days:.1}d)"
                );
            }
            _ => {
                // Within thresholds — no action needed.
            }
        }
    }

    let report = DecayReport {
        verified_demoted,
        external_removed,
        entries_affected: verified_demoted + external_removed,
    };

    if report.entries_affected > 0 {
        log::info!(
            "[provenance::decay] pass complete: demoted={verified_demoted}, removed={external_removed}"
        );
    } else {
        log::debug!("[provenance::decay] pass complete: no entries affected");
    }

    Ok(report)
}

/// Runs a decay pass using a config-aware wrapper.
///
/// This can be called from cron or RPC handlers. It creates a minimal
/// in-memory config with default thresholds. In production, the caller
/// should provide the persisted config.
///
/// TODO(MEM-04): Wire into cron scheduler and use persisted config values
/// from `openhuman::config` for `verified_decay_days` and `external_expiry_days`.
pub fn run_decay(conn: &Connection) -> anyhow::Result<DecayReport> {
    decay_pass(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn create_memory_docs(conn: &Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memory_docs (
               document_id TEXT PRIMARY KEY,
               namespace TEXT NOT NULL,
               key TEXT NOT NULL,
               title TEXT NOT NULL,
               content TEXT NOT NULL,
               source_type TEXT NOT NULL,
               priority TEXT NOT NULL,
               tags_json TEXT NOT NULL,
               metadata_json TEXT NOT NULL,
               category TEXT NOT NULL,
               session_id TEXT,
               created_at REAL NOT NULL,
               updated_at REAL NOT NULL,
               markdown_rel_path TEXT NOT NULL,
               UNIQUE(namespace, key)
             );",
        )?;
        Ok(())
    }

    fn insert_doc(
        conn: &Connection,
        id: &str,
        provenance_json: &str,
        updated_at: f64,
    ) -> anyhow::Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO memory_docs
             (document_id, namespace, key, title, content, source_type, priority,
              tags_json, metadata_json, category, created_at, updated_at, markdown_rel_path,
              provenance_json)
             VALUES (?1, 'test', ?1, 'Test', 'content', 'manual', 'normal',
                     '[]', '{}', 'core', 0.0, ?2, '', ?3)",
            rusqlite::params![id, updated_at, provenance_json],
        )?;
        Ok(())
    }

    fn provenance_json(confidence: &str) -> String {
        format!(r#"{{"source":"chat_history","confidence":"{confidence}","source_detail":""}}"#)
    }

    fn now_ts() -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }

    // ── Test 1: No expired entries returns zero affected ──

    #[test]
    #[cfg_attr(windows, ignore = "Windows filesystem semantics differ")]
    fn decay_pass_no_expired_entries() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        create_memory_docs(&conn)?;

        // Recent entries — should NOT be affected
        let now = now_ts();
        insert_doc(&conn, "recent-verified", &provenance_json("verified"), now)?;
        insert_doc(&conn, "recent-external", &provenance_json("external"), now)?;

        let report = decay_pass(&conn)?;
        assert_eq!(report.entries_affected, 0);
        assert_eq!(report.verified_demoted, 0);
        assert_eq!(report.external_removed, 0);
        Ok(())
    }

    // ── Test 2: Verified -> Inferred when older than 30 days ──

    #[test]
    #[cfg_attr(windows, ignore = "Windows filesystem semantics differ")]
    fn decay_pass_demotes_old_verified() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        create_memory_docs(&conn)?;

        let old_ts = now_ts() - (VERIFIED_DECAY_DAYS as f64 + 1.0) * SECS_PER_DAY;
        insert_doc(&conn, "old-verified", &provenance_json("verified"), old_ts)?;

        let report = decay_pass(&conn)?;
        assert_eq!(report.verified_demoted, 1);
        assert_eq!(report.entries_affected, 1);

        // Verify the row was demoted
        let json: String = conn.query_row(
            "SELECT provenance_json FROM memory_docs WHERE document_id = 'old-verified'",
            [],
            |r| r.get(0),
        )?;
        let prov: Provenance = serde_json::from_str(&json)?;
        assert_eq!(prov.confidence, ConfidenceLevel::Inferred);
        Ok(())
    }

    // ── Test 3: External removed when older than 7 days ──

    #[test]
    #[cfg_attr(windows, ignore = "Windows filesystem semantics differ")]
    fn decay_pass_removes_old_external() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        create_memory_docs(&conn)?;

        let old_ts = now_ts() - (EXTERNAL_EXPIRY_DAYS as f64 + 1.0) * SECS_PER_DAY;
        insert_doc(&conn, "old-external", &provenance_json("external"), old_ts)?;

        let report = decay_pass(&conn)?;
        assert_eq!(report.external_removed, 1);
        assert_eq!(report.entries_affected, 1);

        // Verify the row was deleted
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memory_docs WHERE document_id = 'old-external'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(count, 0);
        Ok(())
    }

    // ── Test 4: Entries within thresholds are untouched ──

    #[test]
    #[cfg_attr(windows, ignore = "Windows filesystem semantics differ")]
    fn decay_pass_does_not_touch_recent_entries() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        create_memory_docs(&conn)?;

        let now = now_ts();
        // Verified entry just under the threshold
        let almost_old = now - (VERIFIED_DECAY_DAYS as f64 - 1.0) * SECS_PER_DAY;
        insert_doc(
            &conn,
            "almost-old-verified",
            &provenance_json("verified"),
            almost_old,
        )?;

        // External entry just under the threshold
        let almost_expired = now - (EXTERNAL_EXPIRY_DAYS as f64 - 1.0) * SECS_PER_DAY;
        insert_doc(
            &conn,
            "almost-expired-external",
            &provenance_json("external"),
            almost_expired,
        )?;

        let report = decay_pass(&conn)?;
        assert_eq!(report.entries_affected, 0);

        // Both should still exist with unchanged confidence
        Ok(())
    }

    // ── Test 5: Malformed provenance JSON is skipped ──

    #[test]
    #[cfg_attr(windows, ignore = "Windows filesystem semantics differ")]
    fn decay_pass_skips_malformed_json() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        create_memory_docs(&conn)?;

        let old_ts = now_ts() - (EXTERNAL_EXPIRY_DAYS as f64 + 1.0) * SECS_PER_DAY;
        insert_doc(
            &conn,
            "bad-json",
            r#"{"source":"chat_history","confidence":"not_a_real_confidence"}"#,
            old_ts,
        )?;

        // Should not crash — just log a warning
        let report = decay_pass(&conn)?;
        assert_eq!(report.entries_affected, 0);
        Ok(())
    }
}
