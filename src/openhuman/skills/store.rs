//! TOML-based SkillsStore for tracking installed DADOU skills.
//!
//! Persists skill installation state to `~/.openhuman/skills/store.toml`.
//! Each skill entry records version, commit hash, activation state, GPG
//! fingerprint, and audit metadata.
//!
//! # File format (`store.toml`)
//!
//! ```toml
//! [skills.my-skill]
//! name = "my-skill"
//! version = "0.1.0"
//! commit_hash = "abc123..."
//! enabled = true
//! gpg_fingerprint = "A1B2C3D4..."
//! installed_at = "2026-06-05T10:00:00Z"
//! last_audit_at = "2026-06-05T10:05:00Z"
//! audit_result = "pass"
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Skill runtime type (WASM sandbox or Python subprocess).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SkillRuntime {
    Wasm,
    Python,
}

impl Default for SkillRuntime {
    fn default() -> Self {
        Self::Wasm
    }
}

fn default_runtime() -> SkillRuntime {
    SkillRuntime::Wasm
}

/// A single installed skill entry in the TOML store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkill {
    /// Skill name (matches manifest `name`).
    pub name: String,
    /// Installed version string (from manifest).
    pub version: String,
    /// Git commit hash at install time.
    pub commit_hash: String,
    /// Whether the skill is currently activated/enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Optional GPG fingerprint for signed-tag verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpg_fingerprint: Option<String>,
    /// ISO 8601 install timestamp.
    pub installed_at: String,
    /// ISO 8601 timestamp of the most recent audit, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_audit_at: Option<String>,
    /// Audit result: "pass", "fail", or absent if not yet audited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_result: Option<String>,
    /// Skill runtime: WASM or Python (default: Wasm for backward compat).
    #[serde(default = "default_runtime")]
    pub runtime: SkillRuntime,
    /// Optional Python skill configuration (PythonConfig serialized as JSON).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python_config: Option<serde_json::Value>,
}

fn default_enabled() -> bool {
    true
}

/// The on-disk TOML store structure (wraps a `[skills]` table).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoreFile {
    #[serde(default)]
    skills: HashMap<String, InstalledSkill>,
}

// ---------------------------------------------------------------------------
// SkillsStore
// ---------------------------------------------------------------------------

/// Persistent store for installed skills, backed by a TOML file.
///
/// All mutations (`upsert`, `remove`, `set_enabled`) write through to disk
/// immediately using an atomic write pattern (.tmp + rename).
///
/// Thread-safety: `SkillsStore` is **not** `Send + Sync` by design — it
/// holds an exclusive mutable reference to the in-memory map. Callers
/// should wrap it in `Arc<Mutex<SkillsStore>>` or similar when sharing
/// across tokio tasks.
#[derive(Debug, Clone)]
pub struct SkillsStore {
    /// Path to the `store.toml` file.
    path: PathBuf,
    /// In-memory skill map keyed by skill name.
    skills: HashMap<String, InstalledSkill>,
}

impl SkillsStore {
    /// Default relative path under the user's home directory.
    const STORE_RELATIVE_PATH: &'static str = ".openhuman/skills/store.toml";

    /// Default skills data root under the user's home directory.
    pub const SKILLS_DIR_RELATIVE: &'static str = ".openhuman/skills";

    /// Resolve the default store path under the user's home directory.
    ///
    /// Returns `None` when `dirs::home_dir()` cannot be resolved (unusual
    /// on desktop, possible in containerised environments).
    pub fn default_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(Self::STORE_RELATIVE_PATH))
    }

    /// Resolve the default skills data directory.
    pub fn default_skills_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(Self::SKILLS_DIR_RELATIVE))
    }

    /// Load the store from the default path (`~/.openhuman/skills/store.toml`).
    ///
    /// Returns an empty store if the file does not exist. Creates parent
    /// directories if they are missing.
    pub fn load() -> Result<Self> {
        let path =
            Self::default_path().context("cannot resolve home directory for skills store")?;
        Self::load_from(&path)
    }

    /// Load the store from a specific path.
    ///
    /// Returns an empty store if the file does not exist. Creates parent
    /// directories if they are missing.
    pub fn load_from(path: &Path) -> Result<Self> {
        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create skills store dir: {}", parent.display())
            })?;
        }

        let skills = if path.exists() {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read skills store: {}", path.display()))?;
            let store_file: StoreFile = toml::from_str(&content)
                .with_context(|| format!("failed to parse skills store: {}", path.display()))?;
            tracing::debug!(
                "[skill-store] loaded {} skills from {}",
                store_file.skills.len(),
                path.display()
            );
            store_file.skills
        } else {
            tracing::debug!(
                "[skill-store] store file not found at {}; starting empty",
                path.display()
            );
            HashMap::new()
        };

        Ok(Self {
            path: path.to_path_buf(),
            skills,
        })
    }

    /// Save the current store to disk atomically (.tmp + rename).
    pub fn save(&self) -> Result<()> {
        let store_file = StoreFile {
            skills: self.skills.clone(),
        };
        let toml_str =
            toml::to_string_pretty(&store_file).context("failed to serialize skills store")?;

        atomic_write(&self.path, &toml_str)
            .with_context(|| format!("failed to save skills store: {}", self.path.display()))?;

        tracing::debug!(
            "[skill-store] saved {} skills to {}",
            self.skills.len(),
            self.path.display()
        );
        Ok(())
    }

    /// Get a reference to an installed skill by name.
    pub fn get(&self, name: &str) -> Option<&InstalledSkill> {
        self.skills.get(name)
    }

    /// Get a mutable reference to an installed skill by name.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut InstalledSkill> {
        self.skills.get_mut(name)
    }

    /// Return all installed skills as owned values (for consumers that need ownership).
    pub fn installed(&self) -> Vec<InstalledSkill> {
        self.skills.values().cloned().collect()
    }

    /// List all installed skills, sorted alphabetically by name.
    pub fn list(&self) -> Vec<&InstalledSkill> {
        let mut skills: Vec<&InstalledSkill> = self.skills.values().collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    /// Insert or update a skill entry, then persist to disk.
    ///
    /// If a skill with the same name already exists, its fields are replaced.
    pub fn upsert(&mut self, skill: InstalledSkill) -> Result<()> {
        let name = skill.name.clone();
        self.skills.insert(name, skill);
        self.save()
    }

    /// Remove a skill by name from the store and persist to disk.
    ///
    /// Returns `true` if the skill existed and was removed, `false` otherwise.
    pub fn remove(&mut self, name: &str) -> Result<bool> {
        let existed = self.skills.remove(name).is_some();
        if existed {
            self.save()?;
        }
        Ok(existed)
    }

    /// Toggle the enabled/disabled state of a skill, then persist to disk.
    ///
    /// Returns an error if the skill is not in the store.
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        let skill = self
            .skills
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("skill {name:?} not found in store"))?;
        skill.enabled = enabled;
        self.save()
    }

    /// Record an audit result for a skill, then persist to disk.
    ///
    /// Sets both `last_audit_at` (to the current UTC ISO 8601 timestamp)
    /// and `audit_result`. Returns an error if the skill is not in the store.
    pub fn record_audit(&mut self, name: &str, result: &str) -> Result<()> {
        let skill = self
            .skills
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("skill {name:?} not found in store"))?;
        skill.last_audit_at = Some(chrono_now_iso8601());
        skill.audit_result = Some(result.to_string());
        self.save()
    }

    /// Number of skills in the store.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Path to the underlying store file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write content to a file atomically: write to `.toml.tmp` then rename.
///
/// Prevents partial writes: a crash during the write leaves the original
/// file intact (the `.tmp` orphan is harmless and will be overwritten on
/// the next write).
fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, content)
        .with_context(|| format!("failed to write temp file: {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("failed to rename temp file to: {}", path.display()))?;
    Ok(())
}

/// Generate an ISO 8601 UTC timestamp string.
fn chrono_now_iso8601() -> String {
    // Use chrono if available, otherwise fall back to a simple UTC timestamp.
    // chrono is already a dependency in Cargo.toml.
    chrono::Utc::now().to_rfc3339()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a test `InstalledSkill` with the given name.
    fn test_skill(name: &str) -> InstalledSkill {
        InstalledSkill {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            commit_hash: "abc123".to_string(),
            enabled: true,
            gpg_fingerprint: None,
            installed_at: "2026-06-05T10:00:00Z".to_string(),
            last_audit_at: None,
            audit_result: None,
            runtime: SkillRuntime::Wasm,
            python_config: None,
        }
    }

    /// Helper: create a `SkillsStore` backed by a temp directory.
    fn temp_store() -> (SkillsStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("store.toml");
        let store = SkillsStore::load_from(&path).expect("failed to load empty store");
        (store, dir)
    }

    #[test]
    fn load_returns_empty_when_store_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.toml");
        assert!(!path.exists());
        let store = SkillsStore::load_from(&path).unwrap();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn upsert_adds_new_skill() {
        let (mut store, _dir) = temp_store();
        store.upsert(test_skill("my-skill")).unwrap();
        assert_eq!(store.len(), 1);

        let s = store.get("my-skill").unwrap();
        assert_eq!(s.name, "my-skill");
        assert_eq!(s.version, "0.1.0");
    }

    #[test]
    fn upsert_updates_existing_skill() {
        let (mut store, _dir) = temp_store();
        store.upsert(test_skill("my-skill")).unwrap();

        let mut updated = test_skill("my-skill");
        updated.version = "0.2.0".to_string();
        updated.commit_hash = "def456".to_string();
        store.upsert(updated).unwrap();

        assert_eq!(store.len(), 1);
        let s = store.get("my-skill").unwrap();
        assert_eq!(s.version, "0.2.0");
        assert_eq!(s.commit_hash, "def456");
    }

    #[test]
    fn remove_deletes_skill() {
        let (mut store, _dir) = temp_store();
        store.upsert(test_skill("my-skill")).unwrap();
        assert_eq!(store.len(), 1);

        let removed = store.remove("my-skill").unwrap();
        assert!(removed);
        assert!(store.is_empty());
        assert!(store.get("my-skill").is_none());
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let (mut store, _dir) = temp_store();
        let removed = store.remove("nonexistent").unwrap();
        assert!(!removed);
    }

    #[test]
    fn set_enabled_toggles_state() {
        let (mut store, _dir) = temp_store();
        store.upsert(test_skill("my-skill")).unwrap();

        // Initially enabled (default).
        assert!(store.get("my-skill").unwrap().enabled);

        // Disable.
        store.set_enabled("my-skill", false).unwrap();
        assert!(!store.get("my-skill").unwrap().enabled);

        // Re-enable.
        store.set_enabled("my-skill", true).unwrap();
        assert!(store.get("my-skill").unwrap().enabled);
    }

    #[test]
    fn set_enabled_errors_for_missing_skill() {
        let (mut store, _dir) = temp_store();
        let result = store.set_enabled("nonexistent", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.toml");

        // Create store, add skills, save.
        {
            let mut store = SkillsStore::load_from(&path).unwrap();
            let mut s1 = test_skill("alpha");
            s1.enabled = true;
            s1.gpg_fingerprint = Some("A1B2".to_string());
            store.upsert(s1).unwrap();

            let mut s2 = test_skill("beta");
            s2.enabled = false;
            s2.last_audit_at = Some("2026-06-05T11:00:00Z".to_string());
            s2.audit_result = Some("pass".to_string());
            store.upsert(s2).unwrap();
        }

        // Load from same path, verify all fields survived.
        let store = SkillsStore::load_from(&path).unwrap();
        assert_eq!(store.len(), 2);

        let alpha = store.get("alpha").unwrap();
        assert_eq!(alpha.version, "0.1.0");
        assert!(alpha.enabled);
        assert_eq!(alpha.gpg_fingerprint.as_deref(), Some("A1B2"));

        let beta = store.get("beta").unwrap();
        assert!(!beta.enabled);
        assert_eq!(beta.last_audit_at.as_deref(), Some("2026-06-05T11:00:00Z"));
        assert_eq!(beta.audit_result.as_deref(), Some("pass"));
    }

    #[test]
    fn empty_store_sorted_list() {
        let (store, _dir) = temp_store();
        let list = store.list();
        assert!(list.is_empty());
    }

    #[test]
    fn list_sorted_by_name() {
        let (mut store, _dir) = temp_store();
        store.upsert(test_skill("zulu")).unwrap();
        store.upsert(test_skill("alpha")).unwrap();
        store.upsert(test_skill("beta")).unwrap();

        let list = store.list();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "alpha");
        assert_eq!(list[1].name, "beta");
        assert_eq!(list[2].name, "zulu");
    }

    #[test]
    fn atomic_write_creates_and_renames() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.toml");

        // First write.
        atomic_write(&path, "key = 'value'").unwrap();
        assert!(path.exists());
        assert!(!path.with_extension("toml.tmp").exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("value"));

        // Second write — overwrites cleanly.
        atomic_write(&path, "key = 'updated'").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("updated"));
    }

    #[test]
    fn record_audit_updates_fields() {
        let (mut store, _dir) = temp_store();
        store.upsert(test_skill("my-skill")).unwrap();

        store.record_audit("my-skill", "pass").unwrap();
        let s = store.get("my-skill").unwrap();
        assert!(s.last_audit_at.is_some());
        assert_eq!(s.audit_result.as_deref(), Some("pass"));

        store.record_audit("my-skill", "fail").unwrap();
        let s = store.get("my-skill").unwrap();
        assert_eq!(s.audit_result.as_deref(), Some("fail"));
    }

    #[test]
    fn record_audit_errors_for_missing_skill() {
        let (mut store, _dir) = temp_store();
        let result = store.record_audit("nonexistent", "pass");
        assert!(result.is_err());
    }

    #[test]
    fn default_path_resolves() {
        let path = SkillsStore::default_path();
        assert!(path.is_some());
        let p = path.unwrap();
        assert!(p.to_string_lossy().contains(".openhuman/skills/store.toml"));
    }

    #[test]
    fn get_mut_allows_in_place_update() {
        let (mut store, _dir) = temp_store();
        store.upsert(test_skill("my-skill")).unwrap();

        {
            let s = store.get_mut("my-skill").unwrap();
            s.version = "9.9.9".to_string();
            s.enabled = false;
        }

        let s = store.get("my-skill").unwrap();
        assert_eq!(s.version, "9.9.9");
        assert!(!s.enabled);
    }
}
