//! Two-lane explicit user preferences — namespaces + read helpers.
//!
//! Preferences written by the `save_preference` tool live in one of two
//! namespaces depending on their relevance scope:
//!
//! - [`USER_PREF_GENERAL_NAMESPACE`] — always-on; injected into the system
//!   prompt at thread start (Lane A).
//! - [`USER_PREF_SITUATIONAL_NAMESPACE`] — topic-scoped; recalled per-turn by
//!   semantic similarity to the user's message (Lane B).
//!
//! Keeping the namespace constants and read helpers here (rather than in the
//! tool module) lets the write path, the system-prompt builder, and the
//! per-turn recall path all share one definition.

use std::sync::Arc;

use super::contradiction::{check_for_contradictions, ContradictionReport};
use super::provenance::{ConfidenceLevel, MemorySource, Provenance};
use super::{Memory, MemoryCategory};

use crate::core::event_bus::{publish_global, DomainEvent};

/// Always-on preferences — injected into the system prompt every thread.
pub const USER_PREF_GENERAL_NAMESPACE: &str = "user_pref_general";

/// Topic-scoped preferences — recalled per query against the user's message.
pub const USER_PREF_SITUATIONAL_NAMESPACE: &str = "user_pref_situational";

/// Default cap on general preferences injected into the system prompt. Keeps
/// the always-on block bounded so it can't blow a small model's context window
/// (see the legacy `gpt-4` 8K overflow).
pub const STANDING_PREFS_LIMIT: usize = 10;

/// Load the latest-`limit` general preferences as plain-language strings,
/// newest-first (by `updated_at`). This is the Lane-A system-prompt block.
///
/// `list()` returns entries ordered newest-first but with `content` set to the
/// title (= topic key), so the body value is fetched via `get()`.
pub async fn load_general_preferences(memory: &Arc<dyn Memory>, limit: usize) -> Vec<String> {
    let entries = memory
        .list(Some(USER_PREF_GENERAL_NAMESPACE), None, None)
        .await
        .unwrap_or_default();

    let mut out = Vec::new();
    for entry in entries.into_iter().take(limit) {
        if let Ok(Some(full)) = memory.get(USER_PREF_GENERAL_NAMESPACE, &entry.key).await {
            let value = full.content.trim();
            if !value.is_empty() {
                out.push(value.to_string());
            }
        }
    }
    out
}

/// Top-K situational preferences to recall per turn (Lane B).
pub const SITUATIONAL_RECALL_LIMIT: usize = 5;

/// Minimum query↔preference vector similarity for a situational preference to be
/// injected. Below this the current message isn't considered relevant to the
/// preference, so nothing is injected (the "unrelated query → no block"
/// behaviour). Tunable against live data.
pub const SITUATIONAL_MIN_SIMILARITY: f64 = 0.35;

/// Recall situational preferences semantically relevant to `query` (Lane B).
///
/// Returns only preferences whose vector similarity to the message clears
/// [`SITUATIONAL_MIN_SIMILARITY`], so an unrelated message yields an empty list
/// (and no injected block). Uses the model-aware embedding recall, so a stale
/// embedding-model signature is excluded rather than mis-scored.
pub async fn recall_situational_preferences(memory: &Arc<dyn Memory>, query: &str) -> Vec<String> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    memory
        .recall_relevant_by_vector(
            USER_PREF_SITUATIONAL_NAMESPACE,
            query,
            SITUATIONAL_RECALL_LIMIT,
            SITUATIONAL_MIN_SIMILARITY,
        )
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(_topic, value)| value)
        .collect()
}

/// Minimum similarity for an existing preference to be flagged as a possible
/// contradiction of a newly-saved one. Higher than the Lane-B recall floor — we
/// only surface genuinely-close matches as contradiction candidates. Tunable.
pub const CONTRADICTION_SIMILARITY: f64 = 0.6;

/// Find existing preferences (across both lanes) semantically close to `value`,
/// excluding `exclude_topic` (the just-saved one). Returns `(topic, value)`
/// pairs so the chat agent — which captured the preference in the first place —
/// can resolve a contradiction itself: overwrite the conflicting topic or remove
/// it. No separate model call; the conversation affirms it.
pub async fn recall_related_preferences(
    memory: &Arc<dyn Memory>,
    value: &str,
    exclude_topic: &str,
    limit: usize,
) -> Vec<(String, String)> {
    if value.trim().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // `limit` is a global cap across *both* lanes, not per-namespace — spend a
    // shared budget so the total surfaced for one contradiction check can never
    // exceed what the caller asked for.
    let mut remaining = limit;
    for ns in [USER_PREF_GENERAL_NAMESPACE, USER_PREF_SITUATIONAL_NAMESPACE] {
        if remaining == 0 {
            break;
        }
        if let Ok(hits) = memory
            .recall_relevant_by_vector(ns, value, remaining, CONTRADICTION_SIMILARITY)
            .await
        {
            for (topic, val) in hits {
                if topic != exclude_topic {
                    out.push((topic, val));
                    remaining = remaining.saturating_sub(1);
                    if remaining == 0 {
                        break;
                    }
                }
            }
        }
    }
    out
}

/// Store a user preference correction with explicit provenance.
///
/// Writes to `user_pref_general` namespace with `MemoryCategory::Core`.
/// The content is structured as `"[user correction] {topic}: {value}"` so
/// the agent can parse the correction intent from the stored text.
///
/// Provenance is stamped as `{source: UserCorrection, confidence: Verified}`
/// and embedded in the content as a `[provenance]` metadata line, consistent
/// with the memory_loader `[Prior conversations]` block pattern.
pub async fn store_preference_correction(
    memory: &Arc<dyn Memory>,
    topic: &str,
    value: &str,
) -> anyhow::Result<()> {
    let content = format!(
        "[user correction] {topic}: {value}\n[provenance] {}",
        serde_json::to_string(&Provenance {
            source: MemorySource::UserCorrection,
            confidence: ConfidenceLevel::Verified,
            source_detail: format!("store_preference_correction: '{topic}'"),
        })
        .unwrap_or_else(|_| "{}".to_string())
    );

    memory
        .store(
            USER_PREF_GENERAL_NAMESPACE,
            topic,
            &content,
            MemoryCategory::Core,
            None,
        )
        .await
}

/// Store a preference value, checking for contradictions against existing
/// verified entries before committing.
///
/// Returns `Ok(None)` when no contradiction was found and the write completed
/// normally. Returns `Ok(Some(ContradictionReport))` when contradictions were
/// detected — the caller (agent/tool) should surface this to the user and
/// await a resolution action before proceeding.
///
/// When contradictions are found, the write is **not committed**. The caller
/// must call `resolve_contradiction` (replace / merge / dismiss) first.
pub async fn store_preference_with_contradiction_check(
    memory: &Arc<dyn Memory>,
    topic: &str,
    value: &str,
    provenance: Option<&Provenance>,
) -> anyhow::Result<Option<ContradictionReport>> {
    // Step 1: check for contradictions against both preference namespaces.
    // We check the general namespace first (it's smaller and more likely to
    // have verified entries), then the situational namespace.
    let mut combined_candidates = Vec::new();
    let mut total_checked = 0usize;
    let mut total_elapsed = 0u64;

    for ns in &[
        USER_PREF_GENERAL_NAMESPACE,
        USER_PREF_SITUATIONAL_NAMESPACE,
    ] {
        let report = check_for_contradictions(memory, ns, value, provenance, CONTRADICTION_SIMILARITY)
            .await?;
        if !report.candidates.is_empty() {
            combined_candidates.extend(report.candidates);
        }
        total_checked += report.checked_against;
        total_elapsed += report.elapsed_ms;
    }

    let report = ContradictionReport {
        candidates: combined_candidates,
        checked_against: total_checked,
        elapsed_ms: total_elapsed,
    };

    // Step 2: if no contradictions found, commit the write.
    if !report.has_contradictions() {
        // TODO: use the provenance when the Memory::store signature accepts it.
        // For now, embed it in content like store_preference_correction does.
        let content = format!(
            "{topic}: {value}\n[provenance] {}",
            serde_json::to_string(
                &provenance.unwrap_or(&Provenance {
                    source: MemorySource::ChatHistory,
                    confidence: ConfidenceLevel::Inferred,
                    source_detail: String::new(),
                })
            )
            .unwrap_or_else(|_| "{}".into())
        );

        memory
            .store(
                USER_PREF_GENERAL_NAMESPACE,
                topic,
                &content,
                MemoryCategory::Core,
                None,
            )
            .await?;

        return Ok(None);
    }

    // Step 3: contradictions found — publish event, do NOT commit.
    for candidate in &report.candidates {
        publish_global(DomainEvent::ContradictionDetected {
            namespace: candidate.namespace.clone(),
            existing_key: candidate.existing_entry.key.clone(),
            existing_content: candidate.existing_entry.content.clone(),
            new_value: candidate.new_value.clone(),
            similarity: candidate.similarity,
        });
    }

    log::info!(
        "[dadou_contradiction] detected {} contradiction(s) for topic '{}', write deferred",
        report.candidates.len(),
        topic,
    );

    Ok(Some(report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhuman::embeddings::NoopEmbedding;
    use crate::openhuman::memory::MemoryCategory;
    use crate::openhuman::memory_store::UnifiedMemory;
    use tempfile::TempDir;

    #[tokio::test]
    async fn load_general_preferences_returns_values_newest_first_capped() {
        let tmp = TempDir::new().unwrap();
        let mem: Arc<dyn Memory> =
            Arc::new(UnifiedMemory::new(tmp.path(), Arc::new(NoopEmbedding), None).unwrap());

        mem.store(
            USER_PREF_GENERAL_NAMESPACE,
            "reply_language",
            "Reply in British English.",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();
        mem.store(
            USER_PREF_GENERAL_NAMESPACE,
            "tone",
            "Be terse.",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();

        let general = load_general_preferences(&mem, 10).await;
        // Returns the values (bodies), not the topic keys.
        assert!(general.iter().any(|v| v.contains("British English")));
        assert!(general.iter().any(|v| v.contains("Be terse")));
        assert!(!general.iter().any(|v| v == "reply_language"));

        // The limit caps the block.
        assert_eq!(load_general_preferences(&mem, 1).await.len(), 1);
    }

    // ── store_preference_correction ────────────────────────────────────

    #[tokio::test]
    async fn store_preference_correction_creates_entry_in_general_namespace() {
        let tmp = TempDir::new().unwrap();
        let mem: Arc<dyn Memory> =
            Arc::new(UnifiedMemory::new(tmp.path(), Arc::new(NoopEmbedding), None).unwrap());

        store_preference_correction(&mem, "theme", "dark mode").await.unwrap();

        let retrieved = mem
            .get(USER_PREF_GENERAL_NAMESPACE, "theme")
            .await
            .unwrap()
            .expect("entry should exist");
        assert!(retrieved.content.contains("[user correction]"));
        assert!(retrieved.content.contains("theme: dark mode"));
        assert!(retrieved.content.contains("[provenance]"));
    }

    #[tokio::test]
    async fn store_preference_correction_overwrites_existing_key() {
        let tmp = TempDir::new().unwrap();
        let mem: Arc<dyn Memory> =
            Arc::new(UnifiedMemory::new(tmp.path(), Arc::new(NoopEmbedding), None).unwrap());

        store_preference_correction(&mem, "language", "English").await.unwrap();
        store_preference_correction(&mem, "language", "French").await.unwrap();

        let retrieved = mem
            .get(USER_PREF_GENERAL_NAMESPACE, "language")
            .await
            .unwrap()
            .expect("entry should exist");
        assert!(retrieved.content.contains("French"));
        assert!(!retrieved.content.contains("English"));
    }

    #[tokio::test]
    async fn store_preference_correction_appears_in_load_general_preferences() {
        let tmp = TempDir::new().unwrap();
        let mem: Arc<dyn Memory> =
            Arc::new(UnifiedMemory::new(tmp.path(), Arc::new(NoopEmbedding), None).unwrap());

        store_preference_correction(&mem, "reply_style", "always cite sources")
            .await
            .unwrap();

        let prefs = load_general_preferences(&mem, 10).await;
        assert!(prefs.iter().any(|v| v.contains("always cite sources")));
    }
}
