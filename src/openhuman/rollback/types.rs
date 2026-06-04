//! Rollback domain types.
//!
//! Defines [`RollbackEntry`] (the pre-write snapshot metadata stored in SQLite),
//! [`RollbackError`] (typed errors for the domain), and [`FileSnapshot`] (the
//! actual content read before a write).

use serde::{Deserialize, Serialize};

/// A single rollback entry stored in the SQLite index.
///
/// Each entry records one pre-write file snapshot: what the file looked like
/// *before* a tool wrote to it. The `action_id` (UUID v4) is shared across
/// every file touched by the same tool invocation (D-11: reserved for v2
/// action-level undo). The actual diff content lives in a separate file under
/// `.dadou/history/{action_id}.diff`.
///
/// `rolled_back_at` is set by undo operations to prevent double restoration
/// (UND-02).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackEntry {
    /// Unique identifier (UUID v4) for this rollback entry (D-11).
    pub action_id: String,
    /// Workspace-relative path of the modified file.
    pub file_path: String,
    /// SHA-256 hex digest of the pre-write content.
    pub checksum_sha256: String,
    /// Size of the pre-write content in bytes.
    pub content_size_bytes: i64,
    /// ISO 8601 UTC timestamp of the snapshot.
    pub timestamp_utc: String,
    /// Filename (relative to `.dadou/history/`) where the diff is stored.
    pub diff_filename: String,
    /// Tool that performed the write: "file_write", "edit", or "apply_patch".
    pub tool_name: String,
    /// Optional JSON metadata for future extensions (D-11).
    pub metadata: Option<serde_json::Value>,
    /// ISO 8601 UTC timestamp when this entry was rolled back, or None if not yet undone (UND-02).
    pub rolled_back_at: Option<String>,
}

/// A snapshot of file content captured before a write operation.
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    /// SHA-256 hex digest of the content.
    pub checksum: String,
    /// Content size in bytes.
    pub size_bytes: i64,
    /// Raw file content.
    pub content: Vec<u8>,
}

/// Typed errors for the rollback domain.
#[derive(Debug, thiserror::Error)]
pub enum RollbackError {
    /// Generic store error (e.g. SQLite failure).
    #[error("Rollback store error: {0}")]
    Store(String),

    /// Snapshot-related error.
    #[error("Snapshot error: {0}")]
    Snapshot(String),

    /// I/O error forwarded from std::io.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Entry not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// SQLite error forwarded from rusqlite.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Serde JSON error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<String> for RollbackError {
    fn from(msg: String) -> Self {
        RollbackError::Store(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_rollback_entry_creates_with_valid_uuid() {
        let uuid = Uuid::new_v4().to_string();
        let entry = RollbackEntry {
            action_id: uuid.clone(),
            file_path: "src/main.rs".into(),
            checksum_sha256: "abc123".into(),
            content_size_bytes: 1024,
            timestamp_utc: "2026-01-01T00:00:00Z".into(),
            diff_filename: format!("{}.diff", &uuid),
            tool_name: "file_write".into(),
            metadata: None,
            rolled_back_at: None,
        };
        // Verify the UUID is valid by parsing it back
        let parsed = Uuid::parse_str(&entry.action_id);
        assert!(parsed.is_ok(), "action_id must be a valid UUID v4");
        assert_eq!(entry.action_id, uuid);
        assert_eq!(entry.tool_name, "file_write");
        assert!(entry.metadata.is_none());
    }

    #[test]
    fn test_rollback_entry_json_roundtrip() {
        let uuid = Uuid::new_v4().to_string();
        let entry = RollbackEntry {
            action_id: uuid.clone(),
            file_path: "src/lib.rs".into(),
            checksum_sha256: "def456".into(),
            content_size_bytes: 2048,
            timestamp_utc: "2026-02-02T12:30:00Z".into(),
            diff_filename: format!("{}.diff", &uuid),
            tool_name: "edit".into(),
            metadata: Some(serde_json::json!({"source": "agent"})),
            rolled_back_at: None,
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        let deserialized: RollbackEntry =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.action_id, entry.action_id);
        assert_eq!(deserialized.file_path, entry.file_path);
        assert_eq!(deserialized.checksum_sha256, entry.checksum_sha256);
        assert_eq!(deserialized.content_size_bytes, entry.content_size_bytes);
        assert_eq!(deserialized.timestamp_utc, entry.timestamp_utc);
        assert_eq!(deserialized.diff_filename, entry.diff_filename);
        assert_eq!(deserialized.tool_name, entry.tool_name);
        assert!(deserialized.metadata.is_some());
        assert_eq!(
            deserialized.metadata.unwrap().get("source").unwrap(),
            "agent"
        );
    }

    #[test]
    fn test_rollback_error_kinds() {
        let store_err = RollbackError::Store("db locked".into());
        assert!(store_err.to_string().contains("db locked"));

        let io_err = RollbackError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        assert!(io_err.to_string().contains("file not found"));

        let not_found = RollbackError::NotFound("entry missing".into());
        assert!(not_found.to_string().contains("entry missing"));
    }

    #[test]
    fn test_rollback_entry_with_metadata() {
        let uuid = Uuid::new_v4().to_string();
        let meta = serde_json::json!({
            "tool_args": ["--force"],
            "exit_code": 0,
            "user_id": "u123"
        });
        let entry = RollbackEntry {
            action_id: uuid.clone(),
            file_path: "config.json".into(),
            checksum_sha256: "789ghi".into(),
            content_size_bytes: 512,
            timestamp_utc: "2026-03-03T08:00:00Z".into(),
            diff_filename: format!("{}.diff", &uuid),
            tool_name: "apply_patch".into(),
            metadata: Some(meta),
            rolled_back_at: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: RollbackEntry = serde_json::from_str(&json).unwrap();
        let m = deserialized.metadata.unwrap();
        assert_eq!(m["tool_args"][0], "--force");
        assert_eq!(m["exit_code"], 0);
    }
}
