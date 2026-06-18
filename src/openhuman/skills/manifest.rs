//! DADOU skill manifest (`dadou-skill.yaml`) parsing and validation.
//!
//! Defines the on-disk manifest format for WASM skills: name, version, author,
//! permissions, WASM entry point, dependencies, and GPG configuration.
//!
//! # Format
//!
//! ```yaml
//! name: my-skill
//! version: 0.1.0
//! author: "Alice"
//! description: "Does useful things"
//! wasm:
//!   path: build/output.wasm
//!   entry: run
//! permissions:
//!   filesystem:
//!     read:
//!       - "/tmp/**"
//!     write:
//!       - "data/**"
//!   network: false
//! gpg:
//!   fingerprint: A1B2C3D4...
//! dependencies:
//!   - name: helper-skill
//!     version: ">=0.2.0"
//! min_dadou_version: "0.5.0"
//! ```

use std::path::PathBuf;

use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Top-level representation of a `dadou-skill.yaml` manifest file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillManifest {
    /// Required skill name — must match `^[a-zA-Z0-9_-]+$`, max 64 chars.
    pub name: String,
    /// Required semver version string.
    pub version: String,
    /// Optional human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional author name.
    #[serde(default)]
    pub author: Option<String>,
    /// Optional GPG key configuration (for signed-tag verification).
    #[serde(default)]
    pub gpg: Option<GpgConfig>,
    /// WASM binary and entry-point configuration. Optional when `python` is set.
    #[serde(default)]
    pub wasm: Option<WasmConfig>,
    /// Python runtime configuration. Optional when `wasm` is set.
    /// Mutually exclusive with `wasm`.
    #[serde(default)]
    pub python: Option<PythonConfig>,
    /// Capability permissions (default: deny-all).
    #[serde(default = "default_permissions")]
    pub permissions: Permissions,
    /// Optional skill dependencies.
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    /// Minimum DADOU version required to run this skill.
    #[serde(default)]
    pub min_dadou_version: Option<String>,
}

/// GPG key configuration for signed-tag verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpgConfig {
    /// Expected GPG key fingerprint (hex string).
    pub fingerprint: String,
}

/// WASM binary and entry-point configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmConfig {
    /// Path to the WASM binary (relative to repo root). Must not contain `..`.
    pub path: PathBuf,
    /// Name of the exported entry function.
    #[serde(default = "default_entry")]
    pub entry: String,
}

fn default_entry() -> String {
    "_start".to_string()
}

/// Python runtime configuration for a DADOU skill.
///
/// Mutually exclusive with `wasm` — exactly one runtime must be specified.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PythonConfig {
    /// Path to the Python entry-point script, relative to repo root.
    /// Default: `"main.py"`.
    #[serde(default = "default_python_entry")]
    pub entry: String,
    /// List of pip requirements (e.g. `["requests>=2.28", "pydantic"]`).
    #[serde(default)]
    pub requirements: Vec<String>,
    /// Optional custom PyPI index URL.
    #[serde(default)]
    pub pip_index: Option<String>,
    /// Preferred execution mode: `"docker"` (default, sandboxed) or `"local"`.
    /// Falls back to local Python when Docker is not available.
    #[serde(default = "default_python_runtime")]
    pub runtime: String,
}

fn default_python_entry() -> String {
    "main.py".to_string()
}

fn default_python_runtime() -> String {
    "docker".to_string()
}

/// Capability permissions for a skill (deny-by-default).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub filesystem: FilesystemPerms,
    /// Whether network access is permitted. Default: `false` (no network).
    #[serde(default)]
    pub network: bool,
}

/// Filesystem access patterns (glob-based).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilesystemPerms {
    /// Glob patterns for allowed read paths.
    #[serde(default)]
    pub read: Vec<String>,
    /// Glob patterns for allowed write paths.
    #[serde(default)]
    pub write: Vec<String>,
}

/// A dependency on another skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    #[serde(default)]
    pub version: String,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            filesystem: FilesystemPerms {
                read: vec![],
                write: vec![],
            },
            network: false,
        }
    }
}

fn default_permissions() -> Permissions {
    Permissions::default()
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during manifest parsing and validation.
#[derive(Debug, Error)]
pub enum ManifestError {
    /// The YAML string could not be parsed.
    #[error("failed to parse YAML: {0}")]
    ParseError(String),

    /// A required field is missing or empty.
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    /// A field failed validation.
    #[error("invalid field: {0}")]
    InvalidField(String),
}

impl SkillManifest {
    /// Returns the runtime kind: `"wasm"` or `"python"`.
    ///
    /// Panics if neither or both runtimes are set (should be caught by validation).
    pub fn runtime_kind(&self) -> &str {
        if self.wasm.is_some() {
            "wasm"
        } else if self.python.is_some() {
            "python"
        } else {
            "unknown"
        }
    }
}

impl From<serde_yaml::Error> for ManifestError {
    fn from(e: serde_yaml::Error) -> Self {
        ManifestError::ParseError(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum length for skill names.
pub const MAX_NAME_LEN: usize = 64;

/// Allowed character pattern for skill names (alphanumeric, underscore, hyphen).
/// The same constraint is used in `ops_types.rs` for SKILL.md slug derivation.
const NAME_PATTERN: &str = r"^[a-zA-Z0-9_-]+$";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse and validate a `dadou-skill.yaml` manifest string.
///
/// Returns a [`SkillManifest`] on success, or a [`ManifestError`] describing
/// the first validation failure.
pub fn parse_manifest(yaml_str: &str) -> Result<SkillManifest, ManifestError> {
    let manifest: SkillManifest = serde_yaml::from_str(yaml_str)?;

    // Validate `name` — required, within length, safe characters.
    let trimmed_name = manifest.name.trim();
    if trimmed_name.is_empty() {
        return Err(ManifestError::MissingField("name"));
    }
    if trimmed_name.len() > MAX_NAME_LEN {
        return Err(ManifestError::InvalidField(format!(
            "name exceeds max length of {MAX_NAME_LEN} (got {} chars)",
            trimmed_name.len()
        )));
    }
    let name_re = Regex::new(NAME_PATTERN).expect("NAME_PATTERN is valid regex");
    if !name_re.is_match(trimmed_name) {
        return Err(ManifestError::InvalidField(format!(
            "name {trimmed_name:?} must match {NAME_PATTERN}"
        )));
    }

    // Validate `version` — required, non-empty.
    let trimmed_version = manifest.version.trim();
    if trimmed_version.is_empty() {
        return Err(ManifestError::MissingField("version"));
    }

    // Validate exactly one runtime is specified.
    let has_wasm = manifest.wasm.is_some();
    let has_python = manifest.python.is_some();
    if has_wasm == has_python {
        if has_wasm {
            return Err(ManifestError::InvalidField(
                "both wasm and python are set — exactly one runtime must be specified".to_string(),
            ));
        } else {
            return Err(ManifestError::MissingField(
                "either 'wasm' or 'python' section is required",
            ));
        }
    }

    // Validate `wasm.path` when present — must be non-empty and free of path traversal.
    if let Some(ref wasm) = manifest.wasm {
        let wasm_path_str = wasm.path.to_string_lossy();
        if wasm_path_str.trim().is_empty() {
            return Err(ManifestError::MissingField("wasm.path"));
        }
        if wasm_path_str.contains("..") {
            return Err(ManifestError::InvalidField(format!(
                "wasm.path must not contain '..' (got {wasm_path_str:?})"
            )));
        }
    }

    // Validate `python.entry` when present.
    if let Some(ref python) = manifest.python {
        let entry = python.entry.trim();
        if entry.is_empty() {
            return Err(ManifestError::MissingField("python.entry"));
        }
        if entry.contains("..") {
            return Err(ManifestError::InvalidField(format!(
                "python.entry must not contain '..' (got {entry:?})"
            )));
        }
    }

    Ok(SkillManifest {
        name: trimmed_name.to_string(),
        version: trimmed_version.to_string(),
        ..manifest
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_yaml() -> &'static str {
        r#"
name: my-test-skill
version: 0.1.0
author: "Alice"
description: "A test skill"
wasm:
  path: build/output.wasm
  entry: run
permissions:
  filesystem:
    read:
      - "/tmp/**"
    write:
      - "data/**"
  network: false
gpg:
  fingerprint: A1B2C3D4E5F6
dependencies:
  - name: helper-skill
    version: ">=0.2.0"
min_dadou_version: "0.5.0"
"#
    }

    #[test]
    fn parses_valid_manifest() {
        let m = parse_manifest(valid_yaml()).unwrap();
        assert_eq!(m.name, "my-test-skill");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.author, Some("Alice".to_string()));
        assert_eq!(m.description.as_deref(), Some("A test skill"));
        assert_eq!(
            m.wasm.as_ref().unwrap().path.to_string_lossy(),
            "build/output.wasm"
        );
        assert_eq!(m.wasm.as_ref().unwrap().entry, "run");
        assert!(!m.permissions.network);
        assert_eq!(m.permissions.filesystem.read.len(), 1);
        assert_eq!(m.permissions.filesystem.read[0], "/tmp/**");
        assert_eq!(m.permissions.filesystem.write.len(), 1);
        assert_eq!(m.permissions.filesystem.write[0], "data/**");
        assert_eq!(m.gpg.as_ref().unwrap().fingerprint, "A1B2C3D4E5F6");
        assert_eq!(m.dependencies.len(), 1);
        assert_eq!(m.dependencies[0].name, "helper-skill");
        assert_eq!(m.dependencies[0].version, ">=0.2.0");
        assert_eq!(m.min_dadou_version.as_deref(), Some("0.5.0"));
    }

    #[test]
    fn rejects_missing_name() {
        let yaml = r#"
version: 0.1.0
wasm:
  path: test.wasm
"#;
        let err = parse_manifest(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("name"),
            "expected error mentioning 'name', got {err}"
        );
    }

    #[test]
    fn rejects_empty_name() {
        let yaml = r#"
name: ""
version: 0.1.0
wasm:
  path: test.wasm
"#;
        let err = parse_manifest(yaml).unwrap_err();
        assert!(
            matches!(&err, ManifestError::MissingField("name")),
            "expected MissingField(name), got {err}"
        );
    }

    #[test]
    fn rejects_missing_version() {
        let yaml = r#"
name: valid-name
wasm:
  path: test.wasm
"#;
        let err = parse_manifest(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("version"),
            "expected error mentioning 'version', got {err}"
        );
    }

    #[test]
    fn rejects_empty_version() {
        let yaml = r#"
name: valid-name
version: ""
wasm:
  path: test.wasm
"#;
        let err = parse_manifest(yaml).unwrap_err();
        assert!(
            matches!(&err, ManifestError::MissingField("version")),
            "expected MissingField(version), got {err}"
        );
    }

    #[test]
    fn rejects_path_traversal() {
        let yaml = r#"
name: valid-name
version: 0.1.0
wasm:
  path: "../../../etc/passwd"
"#;
        let err = parse_manifest(yaml).unwrap_err();
        assert!(
            matches!(&err, ManifestError::InvalidField(s) if s.contains("..")),
            "expected InvalidField with '..', got {err}"
        );
    }

    #[test]
    fn rejects_unknown_yaml_keys() {
        let yaml = r#"
name: valid-name
version: 0.1.0
wasm:
  path: test.wasm
unknown_key: value
"#;
        let err = parse_manifest(yaml);
        assert!(
            err.is_err(),
            "expected error for unknown key, got Ok: {:?}",
            err
        );
    }

    #[test]
    fn defaults_permissions_deny_network() {
        let yaml = r#"
name: valid-name
version: 0.1.0
wasm:
  path: test.wasm
"#;
        let m = parse_manifest(yaml).unwrap();
        assert!(!m.permissions.network, "network must default to false");
    }

    #[test]
    fn defaults_permissions_empty_filesystem() {
        let yaml = r#"
name: valid-name
version: 0.1.0
wasm:
  path: test.wasm
"#;
        let m = parse_manifest(yaml).unwrap();
        assert!(
            m.permissions.filesystem.read.is_empty(),
            "filesystem read must default to empty"
        );
        assert!(
            m.permissions.filesystem.write.is_empty(),
            "filesystem write must default to empty"
        );
    }

    #[test]
    fn rejects_invalid_name_chars() {
        let yaml = r#"
name: invalid name with spaces
version: 0.1.0
wasm:
  path: test.wasm
"#;
        let err = parse_manifest(yaml).unwrap_err();
        assert!(
            matches!(&err, ManifestError::InvalidField(s) if s.contains("name")),
            "expected InvalidField about name, got {err}"
        );
    }

    #[test]
    fn rejects_name_too_long() {
        let long_name = "a".repeat(MAX_NAME_LEN + 1);
        let yaml = format!(
            r#"
name: {long_name}
version: 0.1.0
wasm:
  path: test.wasm
"#
        );
        let err = parse_manifest(&yaml).unwrap_err();
        assert!(
            matches!(&err, ManifestError::InvalidField(s) if s.contains("max length")),
            "expected InvalidField about max length, got {err}"
        );
    }

    #[test]
    fn serde_roundtrip() {
        let m = parse_manifest(valid_yaml()).unwrap();
        let yaml_out = serde_yaml::to_string(&m).unwrap();
        let m2 = parse_manifest(&yaml_out).unwrap();
        assert_eq!(m.name, m2.name);
        assert_eq!(m.version, m2.version);
        assert_eq!(m.author, m2.author);
        assert_eq!(
            m.wasm.as_ref().unwrap().path,
            m2.wasm.as_ref().unwrap().path
        );
        assert_eq!(
            m.wasm.as_ref().unwrap().entry,
            m2.wasm.as_ref().unwrap().entry
        );
        assert_eq!(m.permissions.network, m2.permissions.network);
        assert_eq!(
            m.gpg.as_ref().map(|g| &g.fingerprint),
            m2.gpg.as_ref().map(|g| &g.fingerprint)
        );
        assert_eq!(m.dependencies.len(), m2.dependencies.len());
    }

    #[test]
    fn defaults_entry_is_start() {
        let yaml = r#"
name: valid-name
version: 0.1.0
wasm:
  path: test.wasm
"#;
        let m = parse_manifest(yaml).unwrap();
        assert_eq!(m.wasm.as_ref().unwrap().entry, "_start");
    }

    #[test]
    fn accepts_minimal_manifest() {
        let yaml = r#"
name: minimal
version: 1.0.0
wasm:
  path: output.wasm
"#;
        let m = parse_manifest(yaml).unwrap();
        assert_eq!(m.name, "minimal");
        assert_eq!(m.version, "1.0.0");
        assert_eq!(
            m.wasm.as_ref().unwrap().path.to_string_lossy(),
            "output.wasm"
        );
        assert!(m.author.is_none());
        assert!(m.description.is_none());
        assert!(m.dependencies.is_empty());
        assert!(m.min_dadou_version.is_none());
    }

    #[test]
    fn default_permissions_struct() {
        let p = Permissions::default();
        assert!(!p.network);
        assert!(p.filesystem.read.is_empty());
        assert!(p.filesystem.write.is_empty());
    }

    #[test]
    fn manifest_error_display() {
        let err = ManifestError::MissingField("version");
        assert_eq!(err.to_string(), "missing required field: version");

        let err = ManifestError::InvalidField("bad value".to_string());
        assert_eq!(err.to_string(), "invalid field: bad value");

        let err = ManifestError::ParseError("expected a value".to_string());
        assert_eq!(err.to_string(), "failed to parse YAML: expected a value");
    }
}
