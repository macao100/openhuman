//! Business logic for project context: loading and formatting facts
//! for injection into the agent's system prompt.
//!
//! The primary entry point is [`load_project_context`], which queries all
//! stored project facts and formats them as a `[Project context]` markdown
//! block suitable for prompt injection.

use crate::openhuman::memory_store::MemoryClient;

use super::store;
use super::types::ProjectFact;

/// Empty-string constant returned when no project facts exist.
const EMPTY_CONTEXT: &str = "No project context recorded yet.";

/// Load all project facts and format them as a markdown block for prompt
/// injection.
///
/// The output format:
/// ```text
/// [Project context]
/// - Project: openhuman-backend (v0.56.0)
///   Goal: Build AI assistant
///   Architecture: Three-layer Rust core + Tauri + React
/// - Project: dadou (v0.1.0)
///   Goal: Personal AI assistant with persistent memory
/// ```
///
/// When no facts exist, returns [`EMPTY_CONTEXT`].
pub async fn load_project_context(client: &MemoryClient) -> String {
    let facts = match store::list_facts(client, None).await {
        Ok(facts) => facts,
        Err(e) => {
            tracing::warn!("[project_context] failed to list facts: {e}");
            return String::new();
        }
    };

    if facts.is_empty() {
        return EMPTY_CONTEXT.to_string();
    }

    // Group facts by project, preserving the "newest first" order within each project.
    let mut by_project: std::collections::BTreeMap<String, Vec<ProjectFact>> =
        std::collections::BTreeMap::new();
    for fact in facts {
        by_project
            .entry(fact.project_name.clone())
            .or_default()
            .push(fact);
    }

    let mut out = String::from("[Project context]\n");

    for (_project_name, project_facts) in &by_project {
        // Find a version fact if present, else use the project name bare.
        let version = project_facts
            .iter()
            .find(|f| f.category == "version" || f.fact_key == "version")
            .map(|f| f.fact_value.as_str());

        if let Some(ver) = version {
            out.push_str(&format!(
                "- Project: {} (v{ver})\n",
                project_facts[0].project_name
            ));
        } else {
            out.push_str(&format!("- Project: {}\n", project_facts[0].project_name));
        }

        for fact in project_facts.iter().filter(|f| f.category != "version") {
            let line = format!("  {}: {}\n", capitalize(&fact.fact_key), fact.fact_value);
            out.push_str(&line);
        }
    }

    out
}

/// Capitalize the first character of a string for display in the context block.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhuman::embeddings::NoopEmbedding;
    use crate::openhuman::memory_store::{MemoryClient, UnifiedMemory};
    use chrono::Utc;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn setup_client() -> (TempDir, MemoryClient) {
        let tmp = TempDir::new().unwrap();
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

    // ── Test 1: load_project_context formats facts as a markdown block ──

    #[tokio::test]
    #[cfg_attr(windows, ignore = "Windows path format differs")]
    async fn test_load_project_context_formats_facts() {
        let (_tmp, client) = setup_client();
        store::upsert_fact(
            &client,
            &make_fact("openhuman-backend", "version", "0.56.0", "version", "user"),
        )
        .await
        .unwrap();
        store::upsert_fact(
            &client,
            &make_fact(
                "openhuman-backend",
                "goal",
                "Build AI assistant",
                "goal",
                "user",
            ),
        )
        .await
        .unwrap();
        store::upsert_fact(
            &client,
            &make_fact("dadou", "version", "0.1.0", "version", "user"),
        )
        .await
        .unwrap();

        let ctx = load_project_context(&client).await;

        assert!(ctx.contains("[Project context]"));
        assert!(ctx.contains("openhuman-backend (v0.56.0)"));
        assert!(ctx.contains("dadou (v0.1.0)"));
        assert!(ctx.contains("Build AI assistant"));
    }

    // ── Test 2: Empty project context returns a short string ──

    #[tokio::test]
    async fn test_load_project_context_empty() {
        let (_tmp, client) = setup_client();
        let ctx = load_project_context(&client).await;
        assert_eq!(ctx, EMPTY_CONTEXT);
    }

    // ── Test 3: Multiple facts per project are grouped correctly ──

    #[tokio::test]
    #[cfg_attr(windows, ignore = "Windows path format differs")]
    async fn test_load_project_context_groups_by_project() {
        let (_tmp, client) = setup_client();
        store::upsert_fact(
            &client,
            &make_fact("proj", "version", "1.0", "version", "user"),
        )
        .await
        .unwrap();
        store::upsert_fact(
            &client,
            &make_fact("proj", "goal", "Be great", "goal", "user"),
        )
        .await
        .unwrap();
        store::upsert_fact(
            &client,
            &make_fact("proj", "architecture", "Rust+Tauri", "architecture", "user"),
        )
        .await
        .unwrap();

        let ctx = load_project_context(&client).await;
        assert!(ctx.contains("Goal: Be great"));
        assert!(ctx.contains("Architecture: Rust+Tauri"));
        // Version is on the project line.
        assert!(ctx.contains("proj (v1.0)"));
    }
}
