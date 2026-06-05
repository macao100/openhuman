//! Skill metadata helpers, prompt-injection support, and DADOU WASM skill
//! infrastructure (manifest parsing + TOML skills store).

pub mod bus;
pub mod inject;
pub mod manifest;
pub mod ops;
pub mod ops_create;
pub mod ops_discover;
pub mod ops_install;
pub mod ops_parse;
pub mod ops_types;
pub mod schemas;
pub mod store;
pub mod types;

pub use manifest::{
    parse_manifest, Dependency, FilesystemPerms, GpgConfig, ManifestError, Permissions,
    SkillManifest, WasmConfig,
};
pub use ops::*;
pub use schemas::{
    all_skills_controller_schemas, all_skills_registered_controllers, skills_schemas,
};
pub use store::{InstalledSkill, SkillsStore};

/// Integration test: manifest parsing -> store persistence -> reload roundtrip.
#[cfg(test)]
mod integration_tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn manifest_and_store_integration() -> Result<()> {
        let yaml = r#"
name: integration-skill
version: 0.3.0
author: "Test"
description: "Integration test skill"
wasm:
  path: build/out.wasm
  entry: run
permissions:
  filesystem:
    read:
      - "/tmp/**"
  network: false
gpg:
  fingerprint: DEADBEEF
dependencies:
  - name: dep-skill
    version: ">=0.1.0"
"#;

        // 1. Parse manifest.
        let manifest = parse_manifest(yaml).map_err(|e| anyhow::anyhow!("{e}"))?;
        assert_eq!(manifest.name, "integration-skill");
        assert_eq!(manifest.version, "0.3.0");
        assert_eq!(manifest.wasm.entry, "run");

        // 2. Create a temp store.
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("store.toml");
        let mut store = SkillsStore::load_from(&path)?;
        assert!(store.is_empty());

        // 3. Insert the parsed skill.
        let installed = InstalledSkill {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            commit_hash: "abc123def456".to_string(),
            enabled: true,
            gpg_fingerprint: manifest.gpg.as_ref().map(|g| g.fingerprint.clone()),
            installed_at: "2026-06-05T12:00:00Z".to_string(),
            last_audit_at: None,
            audit_result: None,
        };
        store.upsert(installed)?;
        assert_eq!(store.len(), 1);

        // 4. Reload from disk.
        let store2 = SkillsStore::load_from(&path)?;
        assert_eq!(store2.len(), 1);

        let reloaded = store2.get("integration-skill").unwrap();
        assert_eq!(reloaded.name, "integration-skill");
        assert_eq!(reloaded.version, "0.3.0");
        assert_eq!(reloaded.commit_hash, "abc123def456");
        assert!(reloaded.enabled);
        assert_eq!(
            reloaded.gpg_fingerprint.as_deref(),
            Some("DEADBEEF")
        );
        assert_eq!(reloaded.installed_at, "2026-06-05T12:00:00Z");

        // 5. Toggle enabled and verify persistence.
        let mut store3 = SkillsStore::load_from(&path)?;
        store3.set_enabled("integration-skill", false)?;

        let store4 = SkillsStore::load_from(&path)?;
        assert!(!store4.get("integration-skill").unwrap().enabled);

        Ok(())
    }
}
