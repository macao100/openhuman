//! Idempotent SQLite schema migration for the `provenance_json` column.
//!
//! Follows the established `PRAGMA user_version` pattern from
//! `memory_store/chunks/store.rs` (lines 1488-1566).
//!
//! Adds a `provenance_json TEXT DEFAULT NULL` column to the `memory_docs`
//! table. Safe to call on every boot — the migration checks the current
//! `user_version` and skips if already applied.

use anyhow::Context as _;
use rusqlite::Connection;

/// Migration version for the DADOU provenance schema.
/// Bump this when adding a new DADOU-specific migration to the memory DB.
pub const DADOU_PROVENANCE_MIGRATION_VERSION: i64 = 1;

/// Target column name added by this migration.
const PROVENANCE_COLUMN: &str = "provenance_json";

/// Applies the provenance column migration idempotently.
///
/// 1. Reads `PRAGMA user_version`. If `>= DADOU_PROVENANCE_MIGRATION_VERSION`,
///    returns immediately (no-op).
/// 2. Checks if `provenance_json` already exists on `memory_docs` via
///    `PRAGMA table_info`.
/// 3. If missing, runs `ALTER TABLE memory_docs ADD COLUMN provenance_json TEXT DEFAULT NULL`.
/// 4. Bumps `PRAGMA user_version` to `DADOU_PROVENANCE_MIGRATION_VERSION`.
pub fn migrate_dadou_provenance(conn: &Connection) -> anyhow::Result<()> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .context("read PRAGMA user_version for provenance migration")?;

    if version >= DADOU_PROVENANCE_MIGRATION_VERSION {
        log::debug!(
            "[provenance] migration already applied (user_version={version}), skipping"
        );
        return Ok(());
    }

    let column_exists: bool = conn
        .prepare("PRAGMA table_info(memory_docs)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .any(|name| name.map_or(false, |n| n == PROVENANCE_COLUMN));

    if !column_exists {
        log::info!("[provenance] adding column `{PROVENANCE_COLUMN}` to memory_docs");
        conn.execute(
            &format!(
                "ALTER TABLE memory_docs ADD COLUMN {PROVENANCE_COLUMN} TEXT DEFAULT NULL"
            ),
            [],
        )
        .context("ALTER TABLE memory_docs ADD COLUMN provenance_json")?;
    } else {
        log::debug!(
            "[provenance] column `{PROVENANCE_COLUMN}` already exists, skipping ALTER"
        );
    }

    conn.pragma_update(
        None,
        "user_version",
        DADOU_PROVENANCE_MIGRATION_VERSION,
    )
    .context("set PRAGMA user_version after provenance migration")?;

    log::info!(
        "[provenance] migration complete, user_version={DADOU_PROVENANCE_MIGRATION_VERSION}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn create_minimal_memory_docs(conn: &Connection) -> anyhow::Result<()> {
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

    /// Helper: read the set of column names for `memory_docs`.
    fn column_names(conn: &Connection) -> Vec<String> {
        conn.prepare("PRAGMA table_info(memory_docs)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    // ── Test 1: Fresh DB gets provenance_json column and bumped user_version ──

    #[test]
    fn migrate_adds_column_and_bumps_version() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        create_minimal_memory_docs(&conn)?;

        // Before migration
        let version_before: i64 =
            conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        assert_eq!(version_before, 0, "fresh DB starts at user_version 0");

        migrate_dadou_provenance(&conn)?;

        // After migration
        let cols = column_names(&conn);
        assert!(
            cols.contains(&"provenance_json".to_string()),
            "provenance_json column should exist, got: {cols:?}"
        );
        let version_after: i64 =
            conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        assert_eq!(version_after, DADOU_PROVENANCE_MIGRATION_VERSION);
        Ok(())
    }

    // ── Test 2: Already-migrated DB is a no-op ──

    #[test]
    fn migrate_twice_is_noop() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        create_minimal_memory_docs(&conn)?;

        migrate_dadou_provenance(&conn)?;
        let cols_after_first = column_names(&conn);
        let version_after_first: i64 =
            conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

        // Run again
        migrate_dadou_provenance(&conn)?;

        let cols_after_second = column_names(&conn);
        let version_after_second: i64 =
            conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

        assert_eq!(cols_after_first, cols_after_second);
        assert_eq!(version_after_first, version_after_second);
        Ok(())
    }

    // ── Test 3: Existing rows have provenance_json = NULL after migration ──

    #[test]
    fn existing_rows_get_null_provenance() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        create_minimal_memory_docs(&conn)?;

        // Insert a pre-migration row
        conn.execute(
            "INSERT INTO memory_docs (document_id, namespace, key, title, content, source_type, priority, tags_json, metadata_json, category, created_at, updated_at, markdown_rel_path)
             VALUES ('doc-1', 'global', 'test-key', 'Test', 'Hello', 'manual', 'normal', '[]', '{}', 'core', 0.0, 0.0, '')",
            [],
        )?;

        migrate_dadou_provenance(&conn)?;

        // Verify the row has NULL provenance_json
        let val: Option<String> = conn.query_row(
            "SELECT provenance_json FROM memory_docs WHERE document_id = 'doc-1'",
            [],
            |r| r.get(0),
        )?;
        assert!(val.is_none(), "existing row should have NULL provenance_json");
        Ok(())
    }

    // ── Test 4: ALTER TABLE ADD COLUMN is idempotent via column check ──

    #[test]
    fn column_already_exists_does_not_error() -> anyhow::Result<()> {
        let conn = Connection::open_in_memory()?;
        create_minimal_memory_docs(&conn)?;

        // Manually add the column
        conn.execute(
            "ALTER TABLE memory_docs ADD COLUMN provenance_json TEXT DEFAULT NULL",
            [],
        )?;

        // Migration should handle it gracefully
        migrate_dadou_provenance(&conn)?;

        let cols = column_names(&conn);
        let count = cols.iter().filter(|c| *c == "provenance_json").count();
        assert_eq!(count, 1, "column should appear exactly once");
        Ok(())
    }
}
