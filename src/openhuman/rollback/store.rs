//! SQLite-backed rollback store.
//!
//! Manages the `rollback_history` index table and diff file I/O under
//! `.dadou/history/`.  Each pre-write snapshot is recorded in SQLite with
//! an action_id, checksum, timestamp, and reference to the diff file.

use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::OnceLock;

use crate::openhuman::rollback::types::{RollbackEntry, RollbackError};

/// SQLite filename created under `{workspace_dir}/.dadou/`.
const ROLLBACK_DB_FILENAME: &str = "rollback.db";

/// Directory name for diff files, relative to `workspace_dir`.
const HISTORY_DIRNAME: &str = ".dadou/history";

/// Internal diff operation emitted by the LCS backtracking.
#[derive(Debug, Clone)]
enum DiffOp<'a> {
    /// Line unchanged — present in both old and new.
    Eq {
        line: &'a str,
        old_idx: usize,
        new_idx: usize,
    },
    /// Line removed from old.
    Del {
        line: &'a str,
        old_idx: usize,
    },
    /// Line added in new.
    Ins {
        line: &'a str,
        new_idx: usize,
    },
}

/// Global singleton instance of `RollbackStore`.
static GLOBAL_STORE: OnceLock<RollbackStore> = OnceLock::new();

/// Persistent SQLite-backed rollback store.
///
/// Thread-safe via interior `Mutex<Connection>` — each public method acquires
/// the lock, performs the operation, and releases it.
pub struct RollbackStore {
    conn: Mutex<Connection>,
    history_dir: PathBuf,
    /// Workspace root directory, used by restore operations to resolve relative paths.
    workspace_dir: PathBuf,
}

impl RollbackStore {
    /// Creates or opens the rollback store at `{workspace_dir}/.dadou/rollback.db`.
    ///
    /// Creates `.dadou/history/` if it does not exist.  Panic-free: all
    /// fallible operations return a [`RollbackError`].
    pub fn new(workspace_dir: &Path) -> Result<Self, RollbackError> {
        let dadou_dir = workspace_dir.join(".dadou");
        std::fs::create_dir_all(&dadou_dir)?;

        let db_path = dadou_dir.join(ROLLBACK_DB_FILENAME);
        let conn = Connection::open(&db_path)?;

        let history_dir = workspace_dir.join(HISTORY_DIRNAME);
        std::fs::create_dir_all(&history_dir)?;

        let store = Self {
            conn: Mutex::new(conn),
            history_dir,
            workspace_dir: workspace_dir.to_path_buf(),
        };
        store.ensure_schema()?;
        Ok(store)
    }

    /// Creates an in-memory store for testing, with an explicit `history_dir`.
    ///
    /// The history directory is created if it does not exist.
    /// `workspace_dir` defaults to the history dir's grandparent (for restore_file tests).
    pub fn new_in_memory(history_dir: &Path) -> Result<Self, RollbackError> {
        std::fs::create_dir_all(history_dir)?;
        let conn = Connection::open_in_memory()?;
        // Default workspace_dir to grandparent of history_dir
        let workspace_dir = history_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| history_dir.to_path_buf());
        let store = Self {
            conn: Mutex::new(conn),
            history_dir: history_dir.to_path_buf(),
            workspace_dir,
        };
        store.ensure_schema()?;
        Ok(store)
    }

    // ── Schema ──────────────────────────────────────────────────────────

    /// Creates the `rollback_history` table and indexes if they do not exist.
    ///
    /// Also runs schema migrations (adds `rolled_back_at` column for UND-02).
    fn ensure_schema(&self) -> Result<(), RollbackError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS rollback_history (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                action_id       TEXT NOT NULL UNIQUE,
                file_path       TEXT NOT NULL,
                checksum_sha256 TEXT NOT NULL,
                content_size_bytes INTEGER NOT NULL,
                timestamp_utc   TEXT NOT NULL,
                diff_filename   TEXT NOT NULL,
                tool_name       TEXT NOT NULL,
                metadata        TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_rollback_timestamp
                ON rollback_history(timestamp_utc);
            CREATE INDEX IF NOT EXISTS idx_rollback_file
                ON rollback_history(file_path);
            CREATE INDEX IF NOT EXISTS idx_rollback_action
                ON rollback_history(action_id);",
        )?;
        // Migration: add rolled_back_at column for UND-02 (ignore if already exists).
        let _ = conn.execute_batch(
            "ALTER TABLE rollback_history ADD COLUMN rolled_back_at TEXT NULL;",
        );
        Ok(())
    }

    /// Returns the names of all columns in `rollback_history`, or an error
    /// if the table does not exist.  Exposed for schema verification in tests.
    pub fn column_names(&self) -> Result<Vec<String>, RollbackError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM rollback_history LIMIT 0")?;
        let cols: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(|c| c.to_string())
            .collect();
        Ok(cols)
    }

    /// Returns the names of all indexes on `rollback_history`.
    pub fn index_names(&self) -> Result<Vec<String>, RollbackError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'index'
             AND tbl_name = 'rollback_history' ORDER BY name",
        )?;
        let names: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(names)
    }

    // ── CRUD ────────────────────────────────────────────────────────────

    /// Inserts a [`RollbackEntry`] into the index.
    ///
    /// Returns [`RollbackError::Store`] if the `action_id` already exists
    /// (UNIQUE constraint violation).
    pub fn save_entry(&self, entry: &RollbackEntry) -> Result<(), RollbackError> {
        let metadata_json = entry
            .metadata
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_default();
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "INSERT INTO rollback_history
                (action_id, file_path, checksum_sha256, content_size_bytes,
                 timestamp_utc, diff_filename, tool_name, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                entry.action_id,
                entry.file_path,
                entry.checksum_sha256,
                entry.content_size_bytes,
                entry.timestamp_utc,
                entry.diff_filename,
                entry.tool_name,
                metadata_json,
            ],
        )?;
        if affected == 0 {
            return Err(RollbackError::Store("no rows inserted".into()));
        }
        Ok(())
    }

    /// Retrieves a single entry by `action_id`.
    pub fn get_by_action_id(
        &self,
        action_id: &str,
    ) -> Result<Option<RollbackEntry>, RollbackError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT action_id, file_path, checksum_sha256, content_size_bytes,
                    timestamp_utc, diff_filename, tool_name, metadata,
                    rolled_back_at
             FROM rollback_history
             WHERE action_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![action_id], Self::row_to_entry)?;
        match rows.next() {
            Some(Ok(entry)) => Ok(Some(entry)),
            Some(Err(e)) => Err(RollbackError::Sqlite(e)),
            None => Ok(None),
        }
    }

    /// Returns the most recent `limit` entries, newest first.
    pub fn list_recent(&self, limit: usize) -> Result<Vec<RollbackEntry>, RollbackError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT action_id, file_path, checksum_sha256, content_size_bytes,
                    timestamp_utc, diff_filename, tool_name, metadata,
                    rolled_back_at
             FROM rollback_history
             ORDER BY id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], Self::row_to_entry)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(RollbackError::Sqlite)
    }

    /// Returns all entries at or after the given ISO 8601 timestamp.
    pub fn get_since(&self, timestamp: &str) -> Result<Vec<RollbackEntry>, RollbackError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT action_id, file_path, checksum_sha256, content_size_bytes,
                    timestamp_utc, diff_filename, tool_name, metadata,
                    rolled_back_at
             FROM rollback_history
             WHERE timestamp_utc >= ?1
             ORDER BY timestamp_utc ASC",
        )?;
        let rows = stmt.query_map(params![timestamp], Self::row_to_entry)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(RollbackError::Sqlite)
    }

    /// Returns all entries for the given `file_path`, most recent first.
    pub fn get_by_path(
        &self,
        file_path: &str,
    ) -> Result<Vec<RollbackEntry>, RollbackError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT action_id, file_path, checksum_sha256, content_size_bytes,
                    timestamp_utc, diff_filename, tool_name, metadata,
                    rolled_back_at
             FROM rollback_history
             WHERE file_path = ?1
             ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![file_path], Self::row_to_entry)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(RollbackError::Sqlite)
    }

    /// Deletes entries older than `days` days.  Returns the number of rows
    /// deleted.
    ///
    /// Does **not** remove the corresponding diff files — callers should run
    /// a separate cleanup pass via [`Self::prune_diff_files`].
    pub fn prune_older_than(&self, days: i64) -> Result<usize, RollbackError> {
        let conn = self.conn.lock().unwrap();
        // Compute the cutoff as "now minus `days` days" expressed in ISO 8601.
        // We use SQLite's date/time functions for consistency.
        let affected = conn.execute(
            "DELETE FROM rollback_history
             WHERE timestamp_utc < datetime('now', ?1)",
            params![format!("-{} days", days)],
        )?;
        Ok(affected)
    }

    /// Returns the path to the history directory where diff files live.
    pub fn history_dir(&self) -> &Path {
        &self.history_dir
    }

    /// Returns the workspace root directory.
    pub fn workspace_dir(&self) -> &Path {
        &self.workspace_dir
    }

    // ── Snapshot I/O ──────────────────────────────────────────────────────

    /// After a write completes, stores the snapshot content and generates the diff.
    ///
    /// Writes two files under `.dadou/history/`:
    /// - `{action_id}.snapshot` — full pre-write content (used for restoration)
    /// - `{action_id}.diff` — unified diff (used for display)
    ///
    /// `old_content` is the pre-write content (empty vec for newly created files).
    /// `new_content` is the post-write content.
    pub fn after_write_with_snapshot(
        &self,
        entry: &RollbackEntry,
        old_content: &[u8],
        new_content: &[u8],
    ) -> Result<(), RollbackError> {
        // Write snapshot file (full content for reliable restoration)
        let snapshot_path = self.history_dir.join(format!("{}.snapshot", entry.action_id));
        std::fs::write(&snapshot_path, old_content)?;

        // Generate and write diff
        let diff = Self::generate_diff(old_content, new_content);
        self.write_diff_file(&entry.action_id, &diff)?;

        Ok(())
    }

    /// Reads the snapshot file for an action_id (full pre-write content).
    ///
    /// Returns `RollbackError::NotFound` if the snapshot does not exist.
    pub fn read_snapshot(&self, action_id: &str) -> Result<Vec<u8>, RollbackError> {
        let snapshot_path = self.history_dir.join(format!("{}.snapshot", action_id));
        if !snapshot_path.exists() {
            return Err(RollbackError::NotFound(format!(
                "Snapshot for action_id '{}' not found",
                action_id
            )));
        }
        std::fs::read(&snapshot_path).map_err(RollbackError::Io)
    }

    /// Reads the diff file for an action_id.
    ///
    /// Returns `RollbackError::NotFound` if the diff does not exist.
    pub fn read_diff(&self, action_id: &str) -> Result<String, RollbackError> {
        self.read_diff_file(action_id)?
            .ok_or_else(|| RollbackError::NotFound(format!(
                "Diff for action_id '{}' not found",
                action_id
            )))
    }

    // ── Undo operations ───────────────────────────────────────────────────

    /// Marks an entry as rolled back by setting `rolled_back_at` to the current UTC time.
    ///
    /// Once marked, the entry will not be returned by undo operations, preventing
    /// double restoration (UND-02).
    pub fn mark_rolled_back(&self, action_id: &str) -> Result<(), RollbackError> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE rollback_history SET rolled_back_at = ?1 WHERE action_id = ?2",
            params![now, action_id],
        )?;
        Ok(())
    }

    /// Returns the most recent entry that has NOT been rolled back.
    ///
    /// Used by `undo_last` to find the entry to restore.
    pub fn get_latest_not_rolled_back(&self) -> Result<Option<RollbackEntry>, RollbackError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT action_id, file_path, checksum_sha256, content_size_bytes,
                    timestamp_utc, diff_filename, tool_name, metadata,
                    rolled_back_at
             FROM rollback_history
             WHERE rolled_back_at IS NULL
             ORDER BY id DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map([], Self::row_to_entry)?;
        match rows.next() {
            Some(Ok(entry)) => Ok(Some(entry)),
            Some(Err(e)) => Err(RollbackError::Sqlite(e)),
            None => Ok(None),
        }
    }

    /// Returns all entries up to and including `timestamp` that have NOT been rolled back,
    /// ordered from oldest to newest.
    ///
    /// Used by `undo_before` to find entries to restore (caller reverses for restoration order).
    pub fn get_up_to_not_rolled_back(
        &self,
        timestamp: &str,
    ) -> Result<Vec<RollbackEntry>, RollbackError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT action_id, file_path, checksum_sha256, content_size_bytes,
                    timestamp_utc, diff_filename, tool_name, metadata,
                    rolled_back_at
             FROM rollback_history
             WHERE timestamp_utc <= ?1 AND rolled_back_at IS NULL
             ORDER BY timestamp_utc ASC",
        )?;
        let rows = stmt.query_map(params![timestamp], Self::row_to_entry)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(RollbackError::Sqlite)
    }

    // ── Global singleton ──────────────────────────────────────────────────

    /// Initialises the global `RollbackStore` singleton.
    ///
    /// Panic-free: returns `RollbackError::Store` if the store is already initialised.
    pub fn init_global(workspace_dir: &Path) -> Result<(), RollbackError> {
        let store = Self::new(workspace_dir)?;
        GLOBAL_STORE
            .set(store)
            .map_err(|_| RollbackError::Store("Global rollback store already initialised".into()))
    }

    /// Returns a reference to the global `RollbackStore`, or `None` if not yet initialised.
    pub fn global() -> Option<&'static RollbackStore> {
        GLOBAL_STORE.get()
    }

    /// Removes orphaned diff files that have no matching entry in the index.
    /// Returns the number of deleted files.
    pub fn prune_diff_files(&self) -> Result<usize, RollbackError> {
        let conn = self.conn.lock().unwrap();
        let mut count = 0usize;
        if let Ok(entries) = std::fs::read_dir(&self.history_dir) {
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map(|e| e == "diff").unwrap_or(false) {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let exists: bool = conn
                            .query_row(
                                "SELECT COUNT(*) FROM rollback_history WHERE action_id = ?1",
                                params![stem],
                                |row| row.get::<_, i64>(0),
                            )
                            .map(|c| c > 0)
                            .unwrap_or(false);
                        if !exists {
                            std::fs::remove_file(&path)?;
                            count += 1;
                        }
                    }
                }
            }
        }
        Ok(count)
    }

    // ── Snapshot / Diff ─────────────────────────────────────────────────

    /// Captures the current content of a file at `abs_path`.
    ///
    /// Returns `Some((sha256_hex, size_bytes, content))` if the file exists,
    /// or `None` if it does not (pre-write snapshot for a new file).
    pub fn capture_snapshot(
        &self,
        abs_path: &Path,
    ) -> Result<Option<(String, i64, Vec<u8>)>, RollbackError> {
        if !abs_path.exists() {
            return Ok(None);
        }
        let content = std::fs::read(abs_path)?;
        let size = content.len() as i64;
        let checksum = Self::sha256_hex(&content);
        Ok(Some((checksum, size, content)))
    }

    /// Generates a unified-format diff between `old` and `new` content.
    ///
    /// Uses a simple line-based LCS algorithm.  For files larger than 1 MB
    /// the diff is truncated with a note.
    pub fn generate_diff(old: &[u8], new: &[u8]) -> String {
        const MAX_DIFF_INPUT_SIZE: usize = 1_048_576; // 1 MB
        if old.len() > MAX_DIFF_INPUT_SIZE || new.len() > MAX_DIFF_INPUT_SIZE {
            return format!(
                "[diff truncated: file too large (old={}, new={})]",
                old.len(),
                new.len()
            );
        }
        if old == new {
            return String::new();
        }
        // Convert to lines for line-based diff.
        let old_str = String::from_utf8_lossy(old);
        let new_str = String::from_utf8_lossy(new);
        let old_lines: Vec<&str> = old_str.lines().collect();
        let new_lines: Vec<&str> = new_str.lines().collect();

        // Compute LCS table and derive edit operations.
        let lcs_table = Self::lcs_lengths(&old_lines, &new_lines);
        let ops = Self::backtrack_ops(&lcs_table, &old_lines, &new_lines);
        Self::format_unified_diff(&old_lines, &new_lines, &ops)
    }

    /// Writes a diff string to `{history_dir}/{action_id}.diff`.
    pub fn write_diff_file(&self, action_id: &str, diff: &str) -> Result<(), RollbackError> {
        let diff_path = self.history_dir.join(format!("{}.diff", action_id));
        std::fs::write(&diff_path, diff)?;
        Ok(())
    }

    /// Reads the diff file for `action_id` from the history directory.
    pub fn read_diff_file(&self, action_id: &str) -> Result<Option<String>, RollbackError> {
        let diff_path = self.history_dir.join(format!("{}.diff", action_id));
        if !diff_path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&diff_path)?;
        Ok(Some(content))
    }

    /// Pre-write hook: captures the file snapshot, generates an action_id,
    /// and saves a [`RollbackEntry`] **without** a diff (the diff is added
    /// by [`Self::after_write`]).
    ///
    /// Returns the partial entry so the caller can pass it to `after_write`.
    pub fn before_write(
        &self,
        abs_path: &Path,
        tool_name: &str,
        workspace_relative_path: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<RollbackEntry, RollbackError> {
        let snapshot = self.capture_snapshot(abs_path)?;
        let (checksum, size) = match &snapshot {
            Some((cs, sz, _)) => (cs.clone(), *sz),
            None => (
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
                0,
            ),
        };
        let action_id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        let entry = RollbackEntry {
            action_id: action_id.clone(),
            file_path: workspace_relative_path.into(),
            checksum_sha256: checksum,
            content_size_bytes: size,
            timestamp_utc: timestamp,
            diff_filename: format!("{}.diff", action_id),
            tool_name: tool_name.into(),
            metadata,
            rolled_back_at: None,
        };
        self.save_entry(&entry)?;
        Ok(entry)
    }

    /// Like [`before_write`], but the content is already in memory
    /// (no file read). Use when the caller has already read the file content
    /// (e.g., `edit_file` reads then edits in memory before writing).
    ///
    /// The `content` parameter is the current file content — it will be
    /// checksummed and recorded as the pre-write snapshot without re-reading
    /// the file from disk.
    ///
    /// `_abs_path` is accepted for API consistency with [`before_write`] but
    /// is not used — the snapshot comes from the caller.
    pub fn before_write_with_content(
        &self,
        _abs_path: &Path,
        tool_name: &str,
        content: &[u8],
        workspace_relative_path: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<RollbackEntry, RollbackError> {
        let checksum = Self::sha256_hex(content);
        let size = content.len() as i64;

        let action_id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        let entry = RollbackEntry {
            action_id: action_id.clone(),
            file_path: workspace_relative_path.into(),
            checksum_sha256: checksum,
            content_size_bytes: size,
            timestamp_utc: timestamp,
            diff_filename: format!("{}.diff", action_id),
            tool_name: tool_name.into(),
            metadata,
            rolled_back_at: None,
        };
        self.save_entry(&entry)?;
        Ok(entry)
    }

    /// Post-write hook: reads the new content, generates a diff against the
    /// pre-write snapshot, writes it to `{history_dir}/{action_id}.diff`,
    /// and updates the entry's diff status.
    ///
    /// `old_content` is the byte content captured by `before_write` (use an
    /// empty vec for newly created files).
    pub fn after_write(
        &self,
        entry: &RollbackEntry,
        old_content: &[u8],
        new_content: &[u8],
    ) -> Result<(), RollbackError> {
        let diff = Self::generate_diff(old_content, new_content);
        self.write_diff_file(&entry.action_id, &diff)?;
        Ok(())
    }

    /// Computes SHA-256 hex digest of `data`.
    fn sha256_hex(data: &[u8]) -> String {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    // ── LCS diff internals ──────────────────────────────────────────────

    /// Builds the LCS length table between `a` and `b`.
    fn lcs_lengths(a: &[&str], b: &[&str]) -> Vec<Vec<usize>> {
        let m = a.len();
        let n = b.len();
        let mut table = vec![vec![0usize; n + 1]; m + 1];
        for i in 1..=m {
            for j in 1..=n {
                if a[i - 1] == b[j - 1] {
                    table[i][j] = table[i - 1][j - 1] + 1;
                } else {
                    table[i][j] = table[i - 1][j].max(table[i][j - 1]);
                }
            }
        }
        table
    }

    /// Backtrack through the LCS table to produce (op, old_line, new_line)
    /// operations: "eq" (unchanged), "del" (removed from old), "ins" (added).
    fn backtrack_ops<'a>(
        table: &[Vec<usize>],
        a: &[&'a str],
        b: &[&'a str],
    ) -> Vec<DiffOp<'a>> {
        let mut ops = Vec::new();
        let mut i = a.len();
        let mut j = b.len();
        // We'll build in reverse then flip.
        let mut rev = Vec::new();
        while i > 0 || j > 0 {
            if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
                rev.push(DiffOp::Eq {
                    line: a[i - 1],
                    old_idx: i - 1,
                    new_idx: j - 1,
                });
                i -= 1;
                j -= 1;
            } else if j > 0 && (i == 0 || table[i][j - 1] >= table[i - 1][j]) {
                rev.push(DiffOp::Ins {
                    line: b[j - 1],
                    new_idx: j - 1,
                });
                j -= 1;
            } else if i > 0 {
                rev.push(DiffOp::Del {
                    line: a[i - 1],
                    old_idx: i - 1,
                });
                i -= 1;
            }
        }
        rev.reverse();
        ops.extend(rev);
        ops
    }

    /// Formats edit operations into a unified-diff string.
    fn format_unified_diff(a: &[&str], b: &[&str], ops: &[DiffOp]) -> String {
        if ops.is_empty() {
            return String::new();
        }
        // Group operations into hunks of context + changes.
        const CONTEXT: usize = 3;
        let mut hunks: Vec<Vec<(usize, &DiffOp)>> = Vec::new();
        let mut current_hunk = Vec::new();
        // Track the last "eq" line position so we can break hunks when
        // context gap is too large.
        let mut last_eq_idx: Option<usize> = None;

        for (idx, op) in ops.iter().enumerate() {
            match op {
                DiffOp::Eq { .. } => {
                    if current_hunk.is_empty() {
                        // Not inside a hunk yet — capture context if we've
                        // seen "eq" lines before an eventual change.
                        if last_eq_idx.is_none() || idx - last_eq_idx.unwrap() <= CONTEXT {
                            current_hunk.push((idx, op));
                        }
                    } else {
                        // We're inside a hunk; add context lines.
                        current_hunk.push((idx, op));
                        // Count trailing eq lines inside this hunk.
                        let eq_count = current_hunk
                            .iter()
                            .rev()
                            .take_while(|(_, o)| matches!(o, DiffOp::Eq { .. }))
                            .count();
                        if eq_count > CONTEXT {
                            // Trim to CONTEXT trailing eq lines and start a new hunk.
                            let keep = current_hunk.len() - (eq_count - CONTEXT);
                            let hunk: Vec<_> = current_hunk.drain(..keep).collect();
                            if !hunk.is_empty() {
                                hunks.push(hunk);
                            }
                            // Keep the last CONTEXT eq lines as start of next hunk.
                            let tail: Vec<_> = current_hunk
                                .iter()
                                .rev()
                                .take(CONTEXT)
                                .cloned()
                                .collect();
                            current_hunk.clear();
                            current_hunk.extend(tail.into_iter().rev());
                        }
                    }
                    last_eq_idx = Some(idx);
                }
                DiffOp::Del { .. } | DiffOp::Ins { .. } => {
                    current_hunk.push((idx, op));
                }
            }
        }
        if !current_hunk.is_empty() {
            hunks.push(current_hunk);
        }

        if hunks.is_empty() {
            return String::new();
        }

        let mut output = String::new();
        for hunk in &hunks {
            let first_op_idx = hunk.first().unwrap().0;
            let last_op_idx = hunk.last().unwrap().0;

            // Compute old/new line ranges for the @@ header.
            // Find the first old/new index among non-eq ops for the hunk start.
            let mut old_start = 1usize;
            let mut new_start = 1usize;
            for (idx, op) in hunk {
                match op {
                    DiffOp::Eq { old_idx, new_idx, .. } => {
                        old_start = *old_idx;
                        new_start = *new_idx;
                    }
                    DiffOp::Del { old_idx, .. } => {
                        old_start = *old_idx;
                        break;
                    }
                    DiffOp::Ins { new_idx, .. } => {
                        new_start = *new_idx;
                        break;
                    }
                }
            }

            // Count old lines in the hunk (eq + del) and new lines (eq + ins).
            let old_count: usize = hunk
                .iter()
                .map(|(_, op)| match op {
                    DiffOp::Eq { .. } | DiffOp::Del { .. } => 1,
                    DiffOp::Ins { .. } => 0,
                })
                .sum();
            let new_count: usize = hunk
                .iter()
                .map(|(_, op)| match op {
                    DiffOp::Eq { .. } | DiffOp::Ins { .. } => 1,
                    DiffOp::Del { .. } => 0,
                })
                .sum();

            output.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                old_start + 1,
                old_count,
                new_start + 1,
                new_count
            ));

            for (_, op) in hunk {
                match op {
                    DiffOp::Eq { line, .. } => {
                        output.push(' ');
                        output.push_str(line);
                        output.push('\n');
                    }
                    DiffOp::Del { line, .. } => {
                        output.push('-');
                        output.push_str(line);
                        output.push('\n');
                    }
                    DiffOp::Ins { line, .. } => {
                        output.push('+');
                        output.push_str(line);
                        output.push('\n');
                    }
                }
            }
        }
        output
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<RollbackEntry> {
        let metadata_raw: Option<String> = row.get(7)?;
        let metadata = metadata_raw
            .filter(|s| !s.is_empty())
            .and_then(|s| serde_json::from_str(&s).ok());
        Ok(RollbackEntry {
            action_id: row.get(0)?,
            file_path: row.get(1)?,
            checksum_sha256: row.get(2)?,
            content_size_bytes: row.get(3)?,
            timestamp_utc: row.get(4)?,
            diff_filename: row.get(5)?,
            tool_name: row.get(6)?,
            metadata,
            rolled_back_at: row.get(8)?,
        })
    }
}

/// Returns a reference to the global `RollbackStore`, if initialised.
///
/// Convenience function used by controller handlers in `schemas.rs`.
pub fn global_store() -> Option<&'static RollbackStore> {
    RollbackStore::global()
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhuman::rollback::types::RollbackEntry;
    use uuid::Uuid;

    /// Helper: build a minimal `RollbackEntry` with the given `action_id`.
    fn make_entry(action_id: &str, file_path: &str, ts: &str) -> RollbackEntry {
        RollbackEntry {
            action_id: action_id.into(),
            file_path: file_path.into(),
            checksum_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .into(),
            content_size_bytes: 0,
            timestamp_utc: ts.into(),
            diff_filename: format!("{}.diff", action_id),
            tool_name: "file_write".into(),
            metadata: None,
            rolled_back_at: None,
        }
    }

    /// Helper: create a temp-backed store for tests.
    fn test_store() -> (RollbackStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let history = dir.path().join(".dadou/history");
        let store = RollbackStore::new_in_memory(&history).expect("store");
        (store, dir)
    }

    #[test]
    fn test_schema_creates_rollback_history_table() {
        let (store, _dir) = test_store();
        let cols = store.column_names().expect("column_names");
        assert!(
            cols.contains(&"action_id".to_string()),
            "missing action_id column: {:?}",
            cols
        );
        assert!(cols.contains(&"file_path".to_string()));
        assert!(cols.contains(&"checksum_sha256".to_string()));
        assert!(cols.contains(&"content_size_bytes".to_string()));
        assert!(cols.contains(&"timestamp_utc".to_string()));
        assert!(cols.contains(&"diff_filename".to_string()));
        assert!(cols.contains(&"tool_name".to_string()));
        assert!(cols.contains(&"metadata".to_string()));
        assert_eq!(cols.len(), 10, "expected 10 columns, got {:?}", cols);
    }

    #[test]
    fn test_schema_creates_required_indexes() {
        let (store, _dir) = test_store();
        let indexes = store.index_names().expect("index_names");
        assert!(indexes.contains(&"idx_rollback_timestamp".to_string()));
        assert!(indexes.contains(&"idx_rollback_file".to_string()));
        assert!(indexes.contains(&"idx_rollback_action".to_string()));
    }

    #[test]
    fn test_save_and_get_by_action_id() {
        let (store, _dir) = test_store();
        let entry = make_entry(
            &Uuid::new_v4().to_string(),
            "src/main.rs",
            "2026-01-01T00:00:00Z",
        );
        store.save_entry(&entry).expect("save");
        let retrieved = store
            .get_by_action_id(&entry.action_id)
            .expect("get")
            .expect("entry should exist");
        assert_eq!(retrieved.action_id, entry.action_id);
        assert_eq!(retrieved.file_path, entry.file_path);
        assert_eq!(retrieved.checksum_sha256, entry.checksum_sha256);
    }

    #[test]
    fn test_list_recent_returns_newest_first() {
        let (store, _dir) = test_store();
        let id1 = Uuid::new_v4().to_string();
        let id2 = Uuid::new_v4().to_string();
        store
            .save_entry(&make_entry(&id1, "a.txt", "2026-01-01T00:00:00Z"))
            .unwrap();
        store
            .save_entry(&make_entry(&id2, "b.txt", "2026-01-02T00:00:00Z"))
            .unwrap();
        let recent = store.list_recent(10).unwrap();
        assert_eq!(recent.len(), 2);
        // Most recent first (by insertion order = id DESC).
        assert_eq!(recent[0].action_id, id2);
        assert_eq!(recent[1].action_id, id1);
    }

    #[test]
    fn test_list_recent_respects_limit() {
        let (store, _dir) = test_store();
        for i in 0..5 {
            store
                .save_entry(&make_entry(
                    &Uuid::new_v4().to_string(),
                    &format!("f{}.rs", i),
                    &format!("2026-01-{:02}T00:00:00Z", i + 1),
                ))
                .unwrap();
        }
        assert_eq!(store.list_recent(3).unwrap().len(), 3);
        assert_eq!(store.list_recent(100).unwrap().len(), 5);
    }

    #[test]
    fn test_get_by_path() {
        let (store, _dir) = test_store();
        let id1 = Uuid::new_v4().to_string();
        let id2 = Uuid::new_v4().to_string();
        let id3 = Uuid::new_v4().to_string();
        store
            .save_entry(&make_entry(&id1, "config.json", "2026-01-01T00:00:00Z"))
            .unwrap();
        store
            .save_entry(&make_entry(&id2, "config.json", "2026-01-02T00:00:00Z"))
            .unwrap();
        store
            .save_entry(&make_entry(&id3, "other.txt", "2026-01-03T00:00:00Z"))
            .unwrap();
        let entries = store.get_by_path("config.json").unwrap();
        assert_eq!(entries.len(), 2);
        // Most recent first
        assert_eq!(entries[0].action_id, id2);
        assert_eq!(entries[1].action_id, id1);
    }

    #[test]
    fn test_get_since() {
        let (store, _dir) = test_store();
        store
            .save_entry(&make_entry(
                &Uuid::new_v4().to_string(),
                "a.txt",
                "2026-01-01T00:00:00Z",
            ))
            .unwrap();
        store
            .save_entry(&make_entry(
                &Uuid::new_v4().to_string(),
                "b.txt",
                "2026-01-15T00:00:00Z",
            ))
            .unwrap();
        store
            .save_entry(&make_entry(
                &Uuid::new_v4().to_string(),
                "c.txt",
                "2026-02-01T00:00:00Z",
            ))
            .unwrap();
        let entries = store.get_since("2026-01-10T00:00:00Z").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].file_path, "b.txt");
        assert_eq!(entries[1].file_path, "c.txt");
    }

    #[test]
    fn test_action_id_unique() {
        let (store, _dir) = test_store();
        let dup_id = Uuid::new_v4().to_string();
        store
            .save_entry(&make_entry(&dup_id, "a.txt", "2026-01-01T00:00:00Z"))
            .unwrap();
        let result =
            store.save_entry(&make_entry(&dup_id, "b.txt", "2026-01-02T00:00:00Z"));
        match result {
            Err(RollbackError::Sqlite(_)) => {} // OK — unique constraint violation
            other => panic!("expected Sqlite error for duplicate action_id, got {:?}", other),
        }
    }

    #[test]
    fn test_get_by_action_id_nonexistent() {
        let (store, _dir) = test_store();
        let result = store.get_by_action_id("nonexistent-uuid").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_prune_older_than() {
        let (store, _dir) = test_store();
        store
            .save_entry(&make_entry(
                &Uuid::new_v4().to_string(),
                "old.txt",
                "2025-01-01T00:00:00Z",
            ))
            .unwrap();
        store
            .save_entry(&make_entry(
                &Uuid::new_v4().to_string(),
                "new.txt",
                "2026-06-05T00:00:00Z",
            ))
            .unwrap();
        // Prune everything older than 1 day — the 2025 entry should be removed.
        let deleted = store.prune_older_than(1).unwrap();
        assert!(deleted >= 1, "expected at least 1 deleted, got {}", deleted);
        let remaining = store.list_recent(10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].file_path, "new.txt");
    }

    #[test]
    fn test_history_dir_created() {
        let dir = tempfile::tempdir().expect("tempdir");
        let history = dir.path().join(".dadou/history");
        let store = RollbackStore::new_in_memory(&history).expect("store");
        assert!(store.history_dir().exists());
        assert!(store.history_dir().is_dir());
    }

    #[test]
    fn test_save_entry_with_metadata() {
        let (store, _dir) = test_store();
        let uuid = Uuid::new_v4().to_string();
        let entry = RollbackEntry {
            action_id: uuid.clone(),
            file_path: "data.json".into(),
            checksum_sha256: "abc".into(),
            content_size_bytes: 100,
            timestamp_utc: "2026-04-01T00:00:00Z".into(),
            diff_filename: format!("{}.diff", uuid),
            tool_name: "apply_patch".into(),
            metadata: Some(serde_json::json!({"retries": 3})),
        };
        store.save_entry(&entry).unwrap();
        let retrieved = store.get_by_action_id(&uuid).unwrap().unwrap();
        let meta = retrieved.metadata.unwrap();
        assert_eq!(meta["retries"], 3);
    }

    // ── Snapshot / Diff tests (Task 2) ──────────────────────────────────

    #[test]
    fn test_capture_snapshot_existing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, b"hello world").unwrap();
        let history = dir.path().join(".dadou/history");
        let store = RollbackStore::new_in_memory(&history).expect("store");

        let result = store.capture_snapshot(&file_path).expect("capture");
        assert!(result.is_some());
        let (checksum, size, content) = result.unwrap();
        assert_eq!(size, 11);
        assert_eq!(&content, b"hello world");
        // SHA-256 of "hello world"
        assert_eq!(
            checksum,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_capture_snapshot_nonexistent_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("nonexistent.txt");
        let history = dir.path().join(".dadou/history");
        let store = RollbackStore::new_in_memory(&history).expect("store");

        let result = store.capture_snapshot(&file_path).expect("capture");
        assert!(result.is_none());
    }

    #[test]
    fn test_generate_diff_basic() {
        let old = b"line1\nline2\nline3\n";
        let new = b"line1\nmodified2\nline3\n";
        let diff = RollbackStore::generate_diff(old, new);
        assert!(!diff.is_empty(), "diff should not be empty");
        assert!(diff.contains("-line2"), "diff should show removed line");
        assert!(diff.contains("+modified2"), "diff should show added line");
    }

    #[test]
    fn test_generate_diff_identical_content() {
        let content = b"line1\nline2\nline3\n";
        let diff = RollbackStore::generate_diff(content, content);
        assert!(diff.is_empty(), "identical content should produce empty diff");
    }

    #[test]
    fn test_generate_diff_empty_new_file() {
        let old: &[u8] = b"";
        let new = b"new content\n";
        let diff = RollbackStore::generate_diff(old, new);
        assert!(!diff.is_empty());
        assert!(diff.contains("+new content"));
    }

    #[test]
    fn test_generate_diff_large_file_truncated() {
        let large = vec![b'A'; 2_000_000]; // > 1 MB
        let diff = RollbackStore::generate_diff(&large, &large);
        assert!(
            diff.contains("diff truncated"),
            "large identical files should still be truncated: got: {}",
            diff
        );
    }

    #[test]
    fn test_write_and_read_diff_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let history = dir.path().join(".dadou/history");
        let store = RollbackStore::new_in_memory(&history).expect("store");
        let action_id = uuid::Uuid::new_v4().to_string();
        let diff_content = "@@ -1,3 +1,3 @@\n line1\n-line2\n+modified2\n line3\n";

        store
            .write_diff_file(&action_id, diff_content)
            .expect("write diff");
        let read_back = store
            .read_diff_file(&action_id)
            .expect("read diff")
            .expect("diff should exist");
        assert_eq!(read_back, diff_content);
        assert!(history.join(format!("{}.diff", action_id)).exists());
    }

    #[test]
    fn test_read_nonexistent_diff() {
        let dir = tempfile::tempdir().expect("tempdir");
        let history = dir.path().join(".dadou/history");
        let store = RollbackStore::new_in_memory(&history).expect("store");
        let result = store
            .read_diff_file("nonexistent-action")
            .expect("read diff");
        assert!(result.is_none());
    }

    #[test]
    fn test_before_write_and_after_write_cycle() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, b"old content").unwrap();
        let history = dir.path().join(".dadou/history");
        let store = RollbackStore::new_in_memory(&history).expect("store");

        // Step 1: before_write captures the snapshot
        let entry = store
            .before_write(&file_path, "file_write", "test.txt", None)
            .expect("before_write");
        assert!(!entry.action_id.is_empty());
        assert_eq!(entry.file_path, "test.txt");
        assert_eq!(entry.tool_name, "file_write");
        // SHA-256 of "old content"
        assert_eq!(
            entry.checksum_sha256,
            "cbfe1a8f681ea09ca0b45ec4a6f08d4b2a11267d995d4c404707140efc0c18d6"
        );

        // Step 2: file is modified
        std::fs::write(&file_path, b"new content").unwrap();

        // Step 3: after_write generates the diff
        store
            .after_write(&entry, b"old content", b"new content")
            .expect("after_write");

        // Verify diff file exists
        let diff = store
            .read_diff_file(&entry.action_id)
            .expect("read")
            .expect("diff should exist");
        assert!(diff.contains("-old content"), "diff should contain old: {}", diff);
        assert!(diff.contains("+new content"), "diff should contain new: {}", diff);

        // Verify entry is in SQLite
        let saved = store
            .get_by_action_id(&entry.action_id)
            .expect("get")
            .expect("entry should exist");
        assert_eq!(saved.checksum_sha256, entry.checksum_sha256);
    }

    #[test]
    fn test_before_write_new_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("new.txt"); // does not exist yet
        let history = dir.path().join(".dadou/history");
        let store = RollbackStore::new_in_memory(&history).expect("store");

        let entry = store
            .before_write(&file_path, "file_write", "new.txt", None)
            .expect("before_write");
        // New file should get the empty-content checksum
        assert_eq!(
            entry.checksum_sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(entry.content_size_bytes, 0);
    }

    #[test]
    fn test_after_write_new_file_creates_diff() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("new.txt");
        let history = dir.path().join(".dadou/history");
        let store = RollbackStore::new_in_memory(&history).expect("store");

        let entry = store
            .before_write(&file_path, "file_write", "new.txt", None)
            .expect("before_write");

        // "After" writing new content
        store
            .after_write(&entry, &[], b"new file content\nwith two lines\n")
            .expect("after_write");

        let diff = store
            .read_diff_file(&entry.action_id)
            .expect("read")
            .expect("diff should exist");
        assert!(diff.contains("+new file content"));
        assert!(diff.contains("+with two lines"));
    }

    #[test]
    fn test_before_write_with_metadata() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, b"data").unwrap();
        let history = dir.path().join(".dadou/history");
        let store = RollbackStore::new_in_memory(&history).expect("store");

        let meta = serde_json::json!({"source": "agent", "turn": 5});
        let entry = store
            .before_write(&file_path, "edit", "test.txt", Some(meta.clone()))
            .expect("before_write");
        let saved = store
            .get_by_action_id(&entry.action_id)
            .expect("get")
            .expect("entry");
        assert_eq!(saved.metadata.unwrap()["source"], "agent");
    }
}
