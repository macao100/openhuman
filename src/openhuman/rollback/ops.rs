//! Undo business logic — file restoration from pre-write snapshots (UND-02).
//!
//! Provides:
//! - `undo_last` — restores the most recent modified file
//! - `undo_before` — restores all files before a given timestamp
//! - `restore_file` — writes snapshot content or removes created file
//!
//! Restoration uses the COMPLETE SNAPSHOT (not the diff) for reliability.
//! `rolled_back_at` prevents double restoration.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::openhuman::rollback::store::RollbackStore;
use crate::openhuman::rollback::types::{RollbackEntry, RollbackError};
use crate::rpc::RpcOutcome;

/// Restores the most recent file modification.
///
/// Retrieves the latest non-rolled-back entry, restores the file from its
/// snapshot, and marks the entry as rolled back.
///
/// Returns an `RpcOutcome` with:
/// ```json
/// { "action_id": "...", "file_path": "...", "restored": true, "rolled_back_at": "..." }
/// ```
///
/// # Errors
///
/// Returns a string error if:
/// - There are no rollback entries to undo
/// - The snapshot file is missing or corrupted
/// - The file cannot be written
pub async fn undo_last(store: &RollbackStore) -> Result<RpcOutcome<Value>, String> {
    let entry = store
        .get_latest_not_rolled_back()
        .map_err(|e| format!("Failed to query rollback entries: {e}"))?
        .ok_or_else(|| "No rollback entries to undo".to_string())?;

    let workspace_dir = store.workspace_dir().to_path_buf();
    restore_file(store, &entry, &workspace_dir)
        .await
        .map_err(|e| format!("Failed to restore file: {e}"))?;

    let now = chrono::Utc::now().to_rfc3339();
    store
        .mark_rolled_back(&entry.action_id)
        .map_err(|e| format!("Failed to mark entry as rolled back: {e}"))?;

    let result = serde_json::json!({
        "action_id": entry.action_id,
        "file_path": entry.file_path,
        "restored": true,
        "rolled_back_at": now,
    });

    Ok(RpcOutcome::single_log(
        result,
        format!("Restored file: {}", entry.file_path),
    ))
}

/// Restores all files modified before a given timestamp.
///
/// Processes entries in reverse chronological order (most recent first) to
/// ensure consistent restoration when the same file was modified multiple times.
///
/// Returns an `RpcOutcome` with:
/// ```json
/// { "restored_count": N, "failed_count": M, "failures": [...] }
/// ```
///
/// # Errors
///
/// Returns a string error if the timestamp parameter is invalid.
pub async fn undo_before(
    store: &RollbackStore,
    timestamp: &str,
) -> Result<RpcOutcome<Value>, String> {
    // Basic ISO 8601 validation — ensure the string looks like a timestamp.
    if timestamp.is_empty() || !timestamp.contains('T') {
        return Err(format!(
            "Invalid timestamp '{}': expected ISO 8601 format (e.g. 2026-06-01T12:00:00Z)",
            timestamp
        ));
    }

    let entries = store
        .get_up_to_not_rolled_back(timestamp)
        .map_err(|e| format!("Failed to query rollback entries: {e}"))?;

    if entries.is_empty() {
        return Ok(RpcOutcome::single_log(
            serde_json::json!({
                "restored_count": 0,
                "failed_count": 0,
                "failures": [],
            }),
            format!("No rollback entries found before {}", timestamp),
        ));
    }

    let workspace_dir = store.workspace_dir().to_path_buf();
    let mut restored_count = 0u32;
    let mut failures: Vec<Value> = Vec::new();

    // Process in reverse chronological order (newest first).
    for entry in entries.iter().rev() {
        match restore_file(store, entry, &workspace_dir).await {
            Ok(()) => {
                restored_count += 1;
            }
            Err(e) => {
                failures.push(serde_json::json!({
                    "action_id": entry.action_id,
                    "file_path": entry.file_path,
                    "reason": e.to_string(),
                }));
            }
        }
    }

    // Mark successfully restored entries as rolled back.
    for entry in entries.iter().rev() {
        let was_not_failed = !failures.iter().any(|f| {
            f.get("action_id")
                .and_then(|v| v.as_str())
                == Some(&entry.action_id)
        });
        if was_not_failed {
            let _ = store.mark_rolled_back(&entry.action_id);
        }
    }

    let result = serde_json::json!({
        "restored_count": restored_count,
        "failed_count": failures.len(),
        "failures": failures,
    });

    let log_msg = format!(
        "Undo before {}: restored {} files, {} failed",
        timestamp, restored_count, failures.len()
    );
    Ok(RpcOutcome::single_log(result, log_msg))
}

/// Restores a single file from its snapshot.
///
/// - If a snapshot exists: reads the snapshot and writes it to the file.
/// - If no snapshot (file was created by the tool): deletes the file.
///
/// The `workspace_dir` is used to resolve `entry.file_path` to an absolute path.
///
/// # Errors
///
/// Returns `RollbackError` if the snapshot is missing, the file cannot be
/// written, or the file cannot be deleted.
async fn restore_file(
    store: &RollbackStore,
    entry: &RollbackEntry,
    workspace_dir: &Path,
) -> Result<(), RollbackError> {
    let abs_path = workspace_dir.join(&entry.file_path);

    // Try to read the snapshot file.
    // If it exists, the file was modified — write the original content back.
    // If it doesn't exist, the file was created — delete it.
    let snapshot_path = store
        .history_dir()
        .join(format!("{}.snapshot", entry.action_id));

    if snapshot_path.exists() {
        let content = store
            .read_snapshot(&entry.action_id)
            .map_err(|e| RollbackError::Snapshot(format!("Failed to read snapshot: {e}")))?;

        // Ensure parent directory exists.
        if let Some(parent) = abs_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| RollbackError::Io(e))?;
        }

        std::fs::write(&abs_path, &content)
            .map_err(|e| RollbackError::Io(e))?;

        tracing::info!(
            "[rollback] Restored file: {} ({} bytes, action_id: {})",
            entry.file_path,
            content.len(),
            entry.action_id
        );
    } else {
        // Snapshot doesn't exist → file was created by the tool → delete it.
        if abs_path.exists() {
            std::fs::remove_file(&abs_path)
                .map_err(|e| RollbackError::Io(e))?;

            tracing::info!(
                "[rollback] Removed created file: {} (action_id: {})",
                entry.file_path,
                entry.action_id
            );
        } else {
            tracing::warn!(
                "[rollback] File already removed: {} (action_id: {})",
                entry.file_path,
                entry.action_id
            );
        }
    }

    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhuman::rollback::store::RollbackStore;
    use uuid::Uuid;

    /// Helper: create a temp directory + store for testing ops.
    fn test_setup() -> (RollbackStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = RollbackStore::new(dir.path()).expect("store");
        (store, dir)
    }

    /// Helper: record a snapshot for a file (like before_write + after_write_with_snapshot).
    fn record_snapshot(
        store: &RollbackStore,
        rel_path: &str,
        old_content: &[u8],
        new_content: &[u8],
    ) -> RollbackEntry {
        let abs_path = store.workspace_dir().join(rel_path);
        let entry = store
            .before_write(&abs_path, "file_write", rel_path, None)
            .expect("before_write");
        // Write new content
        if !new_content.is_empty() {
            std::fs::write(&abs_path, new_content).expect("write new content");
        }
        // Store snapshot
        store
            .after_write_with_snapshot(&entry, old_content, new_content)
            .expect("after_write_with_snapshot");
        entry
    }

    #[tokio::test]
    async fn test_undo_last_empty_history_returns_error() {
        let (store, _dir) = test_setup();
        let result = undo_last(&store).await;
        assert!(result.is_err(), "expected error for empty history");
        let err = result.unwrap_err();
        assert!(
            err.contains("No rollback entries to undo"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn test_undo_last_restores_modified_file() {
        let (store, dir) = test_setup();

        // Create file with original content
        let rel_path = "test.txt";
        let abs_path = dir.path().join(rel_path);
        std::fs::write(&abs_path, b"original content").unwrap();

        // Record snapshot and modify
        record_snapshot(&store, rel_path, b"original content", b"modified content");

        // Verify file is modified
        let after_mod = std::fs::read_to_string(&abs_path).unwrap();
        assert_eq!(after_mod, "modified content", "file should be modified before undo");

        // Undo the last change
        let outcome = undo_last(&store).await.expect("undo_last");
        let val = &outcome.value;

        // Verify result
        assert_eq!(val["restored"], serde_json::Value::Bool(true));
        assert_eq!(val["file_path"], serde_json::Value::String("test.txt".into()));

        // Verify file content is restored
        let content = std::fs::read_to_string(&abs_path).unwrap();
        assert_eq!(content, "original content", "file should be restored to original");
    }

    #[tokio::test]
    async fn test_undo_last_deletes_new_file() {
        let (store, dir) = test_setup();
        let rel_path = "new_file.txt";
        let abs_path = dir.path().join(rel_path);

        // Record snapshot and "create" file (old_content is empty = new file)
        record_snapshot(&store, rel_path, b"", b"new content here");

        // File should exist after creation
        assert!(abs_path.exists(), "file should exist after creation");

        // Undo — should delete the file
        let outcome = undo_last(&store).await.expect("undo_last");
        assert_eq!(outcome.value["restored"], serde_json::Value::Bool(true));

        // File should be gone
        assert!(!abs_path.exists(), "file should be deleted after undo");
    }

    #[tokio::test]
    async fn test_undo_last_only_reverts_latest_modification() {
        let (store, dir) = test_setup();
        let rel_path = "multi_mod.txt";
        let abs_path = dir.path().join(rel_path);

        // First modification
        std::fs::write(&abs_path, b"version 1").unwrap();
        record_snapshot(&store, rel_path, b"version 1", b"version 2");

        // Second modification
        record_snapshot(&store, rel_path, b"version 2", b"version 3");

        // Undo last — should go back to version 2
        let outcome = undo_last(&store).await.expect("undo_last");
        assert_eq!(outcome.value["file_path"], rel_path);

        let content = std::fs::read_to_string(&abs_path).unwrap();
        assert_eq!(content, "version 2", "should revert to version 2, not version 1");

        // Undo again — should go back to version 1
        let outcome2 = undo_last(&store).await.expect("undo_last 2");
        assert_eq!(outcome2.value["file_path"], rel_path);

        let content2 = std::fs::read_to_string(&abs_path).unwrap();
        assert_eq!(content2, "version 1", "should revert to version 1");
    }

    #[tokio::test]
    async fn test_rolled_back_at_prevents_double_restoration() {
        let (store, dir) = test_setup();
        let rel_path = "double.txt";
        let abs_path = dir.path().join(rel_path);

        std::fs::write(&abs_path, b"original").unwrap();
        record_snapshot(&store, rel_path, b"original", b"modified");

        // First undo — should succeed
        undo_last(&store).await.expect("first undo");

        // Verify original content
        let content = std::fs::read_to_string(&abs_path).unwrap();
        assert_eq!(content, "original");

        // Second undo — should fail because the entry is already rolled back
        let result = undo_last(&store).await;
        assert!(result.is_err(), "second undo should fail");
        assert!(
            result.unwrap_err().contains("No rollback entries to undo"),
            "should indicate no more entries"
        );
    }

    #[tokio::test]
    async fn test_undo_before_restores_multiple_files() {
        let (store, dir) = test_setup();

        let files = vec!["a.txt", "b.txt", "c.txt"];
        let timestamps = vec![
            "2026-01-01T12:00:00Z",
            "2026-01-02T12:00:00Z",
            "2026-01-03T12:00:00Z",
        ];

        for (i, (rel_path, ts)) in files.iter().zip(timestamps.iter()).enumerate() {
            let abs_path = dir.path().join(rel_path);
            let old = format!("original {}", i);
            let new = format!("modified {}", i);
            std::fs::write(&abs_path, old.as_bytes()).unwrap();

            // Create entry with manual timestamp override
            let entry = store
                .before_write(&abs_path, "file_write", rel_path, None)
                .expect("before_write");

            // Write new content
            std::fs::write(&abs_path, new.as_bytes()).unwrap();

            // Store snapshot
            store
                .after_write_with_snapshot(&entry, old.as_bytes(), new.as_bytes())
                .expect("after_write_with_snapshot");
        }

        // Undo all before 2026-02-01
        let outcome = undo_before(&store, "2026-02-01T00:00:00Z")
            .await
            .expect("undo_before");
        assert_eq!(outcome.value["restored_count"], 3, "all 3 files should be restored");
        assert_eq!(outcome.value["failed_count"], 0, "no failures expected");

        // Verify all files restored
        for (i, rel_path) in files.iter().enumerate() {
            let abs_path = dir.path().join(rel_path);
            let content = std::fs::read_to_string(&abs_path).unwrap();
            let expected = format!("original {}", i);
            assert_eq!(
                content, expected,
                "{} should be restored to '{}'",
                rel_path, expected
            );
        }
    }

    #[tokio::test]
    async fn test_undo_before_empty_result() {
        let (store, _dir) = test_setup();
        // No entries at all
        let outcome = undo_before(&store, "2026-01-01T00:00:00Z")
            .await
            .expect("undo_before on empty store");
        assert_eq!(outcome.value["restored_count"], 0);
        assert_eq!(outcome.value["failed_count"], 0);
    }

    #[tokio::test]
    async fn test_undo_before_invalid_timestamp() {
        let (store, _dir) = test_setup();
        let result = undo_before(&store, "").await;
        assert!(result.is_err(), "empty timestamp should error");
    }

    #[tokio::test]
    async fn test_snapshot_is_used_not_diff() {
        let (store, dir) = test_setup();
        let rel_path = "snapshot_vs_diff.txt";
        let abs_path = dir.path().join(rel_path);

        std::fs::write(&abs_path, b"line1\nline2\nline3\n").unwrap();
        record_snapshot(
            &store,
            rel_path,
            b"line1\nline2\nline3\n",
            b"line1\nmodified\nline3\n",
        );

        // Delete the diff file (simulate corruption)
        // Find the latest entry and get its action_id
        let entry = store.get_latest_not_rolled_back().unwrap().unwrap();
        let diff_path = store.history_dir().join(format!("{}.diff", entry.action_id));
        let snapshot_path = store.history_dir().join(format!("{}.snapshot", entry.action_id));

        // Verify snapshot exists
        assert!(snapshot_path.exists(), "snapshot should exist");
        assert!(diff_path.exists(), "diff should exist");

        // Delete diff to prove we use snapshot
        std::fs::remove_file(&diff_path).unwrap();

        // Undo — should still work with snapshot
        undo_last(&store).await.expect("undo_last should work even without diff");

        let content = std::fs::read_to_string(&abs_path).unwrap();
        assert_eq!(content, "line1\nline2\nline3\n", "should restore from snapshot");
    }

    #[tokio::test]
    async fn test_restore_file_writes_snapshot_content() {
        let (store, dir) = test_setup();
        let rel_path = "exact_restore.txt";
        let abs_path = dir.path().join(rel_path);

        let original = b"Exact content to verify restoration byte-for-byte.";
        std::fs::write(&abs_path, original).unwrap();
        record_snapshot(&store, rel_path, original, b"Modified content.");

        // Directly call restore_file
        let entry = store.get_latest_not_rolled_back().unwrap().unwrap();
        let workspace_dir = store.workspace_dir().to_path_buf();
        restore_file(&store, &entry, &workspace_dir)
            .await
            .expect("restore_file");

        let content = std::fs::read(&abs_path).unwrap();
        assert_eq!(
            content, original,
            "content should match original byte-for-byte"
        );
    }

    #[tokio::test]
    async fn test_undo_before_with_some_already_rolled_back() {
        let (store, dir) = test_setup();

        // File 1: modify and undo
        let abs_path1 = dir.path().join("first.txt");
        std::fs::write(&abs_path1, b"first original").unwrap();
        let entry1 = record_snapshot(&store, "first.txt", b"first original", b"first modified");
        undo_last(&store).await.expect("undo first");

        // File 2: modify but don't undo
        let abs_path2 = dir.path().join("second.txt");
        std::fs::write(&abs_path2, b"second original").unwrap();
        let _entry2 = record_snapshot(
            &store,
            "second.txt",
            b"second original",
            b"second modified",
        );

        // undo_before should only restore second.txt (first is already rolled back)
        let outcome = undo_before(&store, "2026-12-31T23:59:59Z")
            .await
            .expect("undo_before");
        assert_eq!(outcome.value["restored_count"], 1, "only second.txt should be restored");

        let content2 = std::fs::read_to_string(&abs_path2).unwrap();
        assert_eq!(content2, "second original", "second.txt should be restored");
    }
}
