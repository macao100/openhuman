//! Contradiction resolution logic.
//!
//! When a contradiction is detected, the user can resolve it via one of
//! three actions defined in [`ContradictionAction`]. The
//! [`resolve_contradiction`] function applies the chosen action to the
//! memory store.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::super::provenance::{ConfidenceLevel, MemorySource, Provenance};
use super::super::{Memory, MemoryCategory};

/// Actions a user can take to resolve a contradiction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionAction {
    /// Overwrite the existing entry with the new value.
    Replace,
    /// Combine both values into a single entry.
    Merge,
    /// Keep both entries as-is; dismiss the alert.
    Dismiss,
}

impl ContradictionAction {
    /// Human-readable label for each action.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Replace => "replace",
            Self::Merge => "merge",
            Self::Dismiss => "dismiss",
        }
    }
}

impl std::str::FromStr for ContradictionAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "replace" => Ok(Self::Replace),
            "merge" => Ok(Self::Merge),
            "dismiss" => Ok(Self::Dismiss),
            other => Err(format!(
                "unknown contradiction action '{other}'; expected 'replace', 'merge', or 'dismiss'"
            )),
        }
    }
}

/// A user's resolution decision for a single contradiction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContradictionResolution {
    /// What to do with the existing entry.
    pub action: ContradictionAction,
    /// Namespace of the existing entry.
    pub namespace: String,
    /// Key of the existing entry to resolve.
    pub existing_key: String,
    /// The new value that triggered the contradiction.
    pub new_value: String,
}

/// Provenance stamped on entries created or modified by the resolver.
fn resolution_provenance(action: &ContradictionAction) -> Provenance {
    Provenance {
        source: MemorySource::UserCorrection,
        confidence: ConfidenceLevel::Verified,
        source_detail: format!("dadou_contradiction:resolve:{}", action.as_str()),
    }
}

/// Apply a contradiction resolution to the memory store.
///
/// - `Replace`: overwrite `existing_key` with `new_value` and stamp Verified
///   provenance from user correction.
/// - `Merge`: combine existing content and new value into a single entry.
/// - `Dismiss`: no-op — log the dismissal and return.
///
/// Returns a human-readable status string.
pub async fn resolve_contradiction(
    memory: &Arc<dyn Memory>,
    resolution: &ContradictionResolution,
) -> anyhow::Result<String> {
    match resolution.action {
        ContradictionAction::Replace => {
            let prov = resolution_provenance(&resolution.action);
            let content = format!(
                "{}\n[provenance] {}",
                resolution.new_value,
                serde_json::to_string(&prov).unwrap_or_else(|_| "{}".into())
            );

            memory
                .store(
                    &resolution.namespace,
                    &resolution.existing_key,
                    &content,
                    MemoryCategory::Core,
                    None,
                )
                .await?;

            log::info!(
                "[dadou_contradiction] replaced '{}' in namespace '{}' with new value",
                resolution.existing_key,
                resolution.namespace,
            );

            Ok(format!(
                "replaced '{}' with new value",
                resolution.existing_key
            ))
        }

        ContradictionAction::Merge => {
            // Fetch the existing content so we can combine with the new value.
            let existing_content = memory
                .get(&resolution.namespace, &resolution.existing_key)
                .await?
                .map(|e| e.content)
                .unwrap_or_default();

            let prov = resolution_provenance(&resolution.action);
            let combined = format!(
                "{}\n[merged with] {}\n[provenance] {}",
                existing_content,
                resolution.new_value,
                serde_json::to_string(&prov).unwrap_or_else(|_| "{}".into()),
            );

            memory
                .store(
                    &resolution.namespace,
                    &resolution.existing_key,
                    &combined,
                    MemoryCategory::Core,
                    None,
                )
                .await?;

            log::info!(
                "[dadou_contradiction] merged '{}' in namespace '{}'",
                resolution.existing_key,
                resolution.namespace,
            );

            Ok(format!(
                "merged new value into '{}'",
                resolution.existing_key
            ))
        }

        ContradictionAction::Dismiss => {
            log::info!(
                "[dadou_contradiction] dismissed contradiction for '{}' in namespace '{}'",
                resolution.existing_key,
                resolution.namespace,
            );
            Ok(format!(
                "dismissed contradiction for '{}'",
                resolution.existing_key
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhuman::embeddings::NoopEmbedding;
    use crate::openhuman::memory_store::UnifiedMemory;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<dyn Memory>) {
        let tmp = TempDir::new().unwrap();
        let mem: Arc<dyn Memory> =
            Arc::new(UnifiedMemory::new(tmp.path(), Arc::new(NoopEmbedding), None).unwrap());
        (tmp, mem)
    }

    // ── Test 1: Replace overwrites the existing entry ──

    #[tokio::test]
    async fn replace_overwrites_existing_entry() {
        let (_tmp, mem) = setup();
        let ns = "test_prefs";

        mem.store(ns, "theme", "dark mode", MemoryCategory::Core, None)
            .await
            .unwrap();

        let resolution = ContradictionResolution {
            action: ContradictionAction::Replace,
            namespace: ns.to_string(),
            existing_key: "theme".to_string(),
            new_value: "light mode".to_string(),
        };

        let status = resolve_contradiction(&mem, &resolution).await.unwrap();
        assert!(status.contains("replaced"), "status: {status}");

        let entry = mem.get(ns, "theme").await.unwrap().expect("should exist");
        assert!(entry.content.contains("light mode"));
    }

    // ── Test 2: Dismiss leaves both entries unchanged ──

    #[tokio::test]
    async fn dismiss_leaves_entry_unchanged() {
        let (_tmp, mem) = setup();
        let ns = "test_prefs";

        mem.store(ns, "lang", "French", MemoryCategory::Core, None)
            .await
            .unwrap();

        let resolution = ContradictionResolution {
            action: ContradictionAction::Dismiss,
            namespace: ns.to_string(),
            existing_key: "lang".to_string(),
            new_value: "English".to_string(),
        };

        let status = resolve_contradiction(&mem, &resolution).await.unwrap();
        assert!(status.contains("dismissed"), "status: {status}");

        let entry = mem.get(ns, "lang").await.unwrap().expect("should exist");
        assert!(entry.content.contains("French"));
        assert!(!entry.content.contains("English"));
    }

    // ── Test 3: Merge creates a combined entry ──

    #[tokio::test]
    async fn merge_combines_both_values() {
        let (_tmp, mem) = setup();
        let ns = "test_prefs";

        mem.store(ns, "style", "terse replies", MemoryCategory::Core, None)
            .await
            .unwrap();

        let resolution = ContradictionResolution {
            action: ContradictionAction::Merge,
            namespace: ns.to_string(),
            existing_key: "style".to_string(),
            new_value: "detailed explanations".to_string(),
        };

        let status = resolve_contradiction(&mem, &resolution).await.unwrap();
        assert!(status.contains("merged"), "status: {status}");

        let entry = mem.get(ns, "style").await.unwrap().expect("should exist");
        assert!(entry.content.contains("terse replies"));
        assert!(entry.content.contains("detailed explanations"));
        assert!(entry.content.contains("[merged with]"));
    }

    // ── Test 4: Replace on non-existent key still works (creates a new entry) ──

    #[tokio::test]
    async fn replace_on_nonexistent_key_creates_entry() {
        let (_tmp, mem) = setup();
        let ns = "test_prefs";

        let resolution = ContradictionResolution {
            action: ContradictionAction::Replace,
            namespace: ns.to_string(),
            existing_key: "brand_new".to_string(),
            new_value: "fresh value".to_string(),
        };

        resolve_contradiction(&mem, &resolution).await.unwrap();

        let entry = mem
            .get(ns, "brand_new")
            .await
            .unwrap()
            .expect("should exist");
        assert!(entry.content.contains("fresh value"));
    }

    // ── Test 5: ContradictionAction from_str roundtrip ──

    #[test]
    fn contradiction_action_from_str() {
        use std::str::FromStr;
        assert_eq!(
            ContradictionAction::from_str("replace").unwrap(),
            ContradictionAction::Replace
        );
        assert_eq!(
            ContradictionAction::from_str("MERGE").unwrap(),
            ContradictionAction::Merge
        );
        assert_eq!(
            ContradictionAction::from_str("Dismiss").unwrap(),
            ContradictionAction::Dismiss
        );
        assert!(ContradictionAction::from_str("unknown").is_err());
    }

    // ── Test 6: ContradictionAction serde roundtrip ──

    #[test]
    fn contradiction_action_serde() {
        for action in &[
            ContradictionAction::Replace,
            ContradictionAction::Merge,
            ContradictionAction::Dismiss,
        ] {
            let json = serde_json::to_string(action).unwrap();
            let back: ContradictionAction = serde_json::from_str(&json).unwrap();
            assert_eq!(back, *action);
        }
    }
}
