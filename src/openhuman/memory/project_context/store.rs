//! CRUD operations for project context facts in the `dadou_project_context` namespace.
//!
//! All operations go through the `Memory` trait (via `MemoryClient::memory_handle`),
//! not raw SQL. Facts are serialised as JSON content and keyed by
//! `"{project_name}:{fact_key}"` within the namespace.

use chrono::Utc;
use std::sync::Arc;

use crate::openhuman::memory::{Memory, MemoryCategory};
use crate::openhuman::memory_store::MemoryClient;

use super::types::ProjectFact;

/// Namespace used for all project context facts.
pub const PROJECT_CONTEXT_NAMESPACE: &str = "dadou_project_context";

/// Internal key separator between project name and fact key.
const KEY_SEP: char = ':';

/// Build the storage key for a project fact.
fn storage_key(project: &str, fact_key: &str) -> String {
    format!("{project}{KEY_SEP}{fact_key}")
}

/// Parse a storage key back into `(project_name, fact_key)`.
fn parse_storage_key(key: &str) -> Option<(String, String)> {
    let idx = key.find(KEY_SEP)?;
    let project = key[..idx].to_string();
    let fact_key = key[idx + 1..].to_string();
    Some((project, fact_key))
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Deserialise a `ProjectFact` from stored memory content + metadata.
fn fact_from_content(key: &str, content: &str, timestamp: &str) -> Option<ProjectFact> {
    let (project_name, fact_key) = parse_storage_key(key)?;
    // The content is stored as the fact value directly (plain string).
    // Category, source, and updated_at are encoded in a JSON block at
    // the end of the content, separated by a newline.
    //
    // Format:
    //   <fact_value>
    //   __meta__:{"category":"...","source":"...","updated_at":"..."}
    //
    // This keeps the content human-readable for the memory recall path
    // while preserving structured metadata.

    let (fact_value, meta) = if let Some(idx) = content.rfind("\n__meta__:") {
        let value = content[..idx].to_string();
        let meta_str = &content[idx + 10..]; // skip "\n__meta__:"
        let parsed: serde_json::Value = serde_json::from_str(meta_str).ok()?;
        (value, parsed)
    } else {
        // Legacy: no metadata block — treat entire content as the value.
        (content.to_string(), serde_json::json!({}))
    };

    let category = meta
        .get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("general")
        .to_string();
    let source = meta
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let updated_at = meta
        .get("updated_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| {
            chrono::DateTime::parse_from_rfc3339(timestamp)
                .unwrap_or_else(|_| Utc::now().into())
                .with_timezone(&Utc)
        });

    Some(ProjectFact {
        project_name,
        fact_key,
        fact_value,
        category,
        source,
        updated_at,
    })
}

/// Build the stored content string from a `ProjectFact`.
fn content_from_fact(fact: &ProjectFact) -> String {
    let meta = serde_json::json!({
        "category": fact.category,
        "source": fact.source,
        "updated_at": fact.updated_at.to_rfc3339(),
    });
    format!("{}\n__meta__:{}", fact.fact_value, meta)
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Insert or update a project fact.
///
/// If a fact with the same `(project_name, fact_key)` already exists, it is
/// overwritten. Returns the storage document ID on success.
pub async fn upsert_fact(client: &MemoryClient, fact: &ProjectFact) -> anyhow::Result<String> {
    let key = storage_key(&fact.project_name, &fact.fact_key);
    let content = content_from_fact(fact);
    let memory: Arc<dyn Memory> = client.memory_handle();
    memory
        .store(PROJECT_CONTEXT_NAMESPACE, &key, &content, MemoryCategory::Core, None)
        .await?;
    Ok(key)
}

/// Retrieve a single fact by project name and fact key.
///
/// Returns `None` if the fact does not exist.
pub async fn get_fact(
    client: &MemoryClient,
    project: &str,
    fact_key: &str,
) -> anyhow::Result<Option<ProjectFact>> {
    let key = storage_key(project, fact_key);
    let memory: Arc<dyn Memory> = client.memory_handle();
    match memory.get(PROJECT_CONTEXT_NAMESPACE, &key).await? {
        Some(entry) => Ok(fact_from_content(&key, &entry.content, &entry.timestamp)),
        None => Ok(None),
    }
}

/// List all facts, optionally filtered to a single project.
///
/// Returns facts sorted newest-first by `updated_at`.
pub async fn list_facts(
    client: &MemoryClient,
    project: Option<&str>,
) -> anyhow::Result<Vec<ProjectFact>> {
    let memory: Arc<dyn Memory> = client.memory_handle();
    let entries = memory
        .list(Some(PROJECT_CONTEXT_NAMESPACE), None, None)
        .await?;

    let mut facts: Vec<ProjectFact> = entries
        .into_iter()
        .filter_map(|entry| {
            let f = fact_from_content(&entry.key, &entry.content, &entry.timestamp)?;
            // Apply project filter if specified.
            if let Some(p) = project {
                if f.project_name != p {
                    return None;
                }
            }
            Some(f)
        })
        .collect();

    // Sort newest-first by updated_at.
    facts.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(facts)
}

/// Delete a single fact by project name and fact key.
///
/// Returns `true` if a fact was actually removed, `false` if it did not exist.
pub async fn delete_fact(
    client: &MemoryClient,
    project: &str,
    fact_key: &str,
) -> anyhow::Result<bool> {
    let key = storage_key(project, fact_key);
    let memory: Arc<dyn Memory> = client.memory_handle();
    memory.forget(PROJECT_CONTEXT_NAMESPACE, &key).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhuman::embeddings::NoopEmbedding;
    use crate::openhuman::memory_store::UnifiedMemory;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Helper: create a temporary in-memory store and wrap in a MemoryClient.
    fn setup_client() -> (TempDir, MemoryClient) {
        let tmp = TempDir::new().unwrap();
        let mem = Arc::new(
            UnifiedMemory::new(tmp.path(), Arc::new(NoopEmbedding), None).unwrap(),
        );
        let client = MemoryClient::from_workspace_dir(tmp.path().join("ws")).unwrap();
        (tmp, client)
    }

    fn make_fact(
        project: &str,
        key: &str,
        value: &str,
        category: &str,
        source: &str,
    ) -> ProjectFact {
        ProjectFact {
            project_name: project.to_string(),
            fact_key: key.to_string(),
            fact_value: value.to_string(),
            category: category.to_string(),
            source: source.to_string(),
            updated_at: Utc::now(),
        }
    }

    // ── Test 1: upsert_fact stores a fact and get_fact retrieves it ──

    #[tokio::test]
    async fn test_upsert_and_get_fact() {
        let (_tmp, client) = setup_client();
        let fact = make_fact("my-project", "version", "0.1.0", "version", "user");

        upsert_fact(&client, &fact).await.unwrap();

        let retrieved = get_fact(&client, "my-project", "version")
            .await
            .unwrap()
            .expect("fact should exist");
        assert_eq!(retrieved.fact_value, "0.1.0");
        assert_eq!(retrieved.category, "version");
        assert_eq!(retrieved.source, "user");
    }

    // ── Test 2: list_facts returns all facts for a given project, newest first ──

    #[tokio::test]
    async fn test_list_facts_newest_first() {
        let (_tmp, client) = setup_client();

        let early = {
            let mut f = make_fact("proj", "goal", "Build AI", "goal", "user");
            f.updated_at = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc);
            f
        };
        let late = {
            let mut f = make_fact("proj", "version", "v2", "version", "user");
            f.updated_at = chrono::DateTime::parse_from_rfc3339("2026-06-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc);
            f
        };

        upsert_fact(&client, &early).await.unwrap();
        upsert_fact(&client, &late).await.unwrap();

        let facts = list_facts(&client, Some("proj")).await.unwrap();
        assert_eq!(facts.len(), 2);
        // Newest first.
        assert_eq!(facts[0].fact_key, "version");
        assert_eq!(facts[1].fact_key, "goal");
    }

    // ── Test 3: delete_fact removes a fact and returns true; missing fact returns false ──

    #[tokio::test]
    async fn test_delete_fact() {
        let (_tmp, client) = setup_client();
        let fact = make_fact("proj", "delete-me", "value", "test", "user");
        upsert_fact(&client, &fact).await.unwrap();

        // Exists before delete.
        assert!(get_fact(&client, "proj", "delete-me")
            .await
            .unwrap()
            .is_some());

        // Delete returns true.
        let deleted = delete_fact(&client, "proj", "delete-me").await.unwrap();
        assert!(deleted);

        // Now gone.
        assert!(get_fact(&client, "proj", "delete-me")
            .await
            .unwrap()
            .is_none());

        // Delete non-existent returns false.
        let missing = delete_fact(&client, "proj", "does-not-exist")
            .await
            .unwrap();
        assert!(!missing);
    }

    // ── Test 4: list_facts filters by project ──

    #[tokio::test]
    async fn test_list_facts_filters_by_project() {
        let (_tmp, client) = setup_client();
        upsert_fact(
            &client,
            &make_fact("project-a", "key1", "val1", "general", "user"),
        )
        .await
        .unwrap();
        upsert_fact(
            &client,
            &make_fact("project-b", "key2", "val2", "general", "user"),
        )
        .await
        .unwrap();

        let all = list_facts(&client, None).await.unwrap();
        assert_eq!(all.len(), 2);

        let only_a = list_facts(&client, Some("project-a")).await.unwrap();
        assert_eq!(only_a.len(), 1);
        assert_eq!(only_a[0].fact_key, "key1");
    }
}
