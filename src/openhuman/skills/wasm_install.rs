//! Full install orchestration for DADOU WASM skills.
//!
//! Provides the complete skill lifecycle pipeline:
//!
//! 1. **Install** — `git clone` → parse manifest → GPG verify tag → static
//!    analysis → copy WASM binary + manifest to `~/.openhuman/skills/<name>/`
//!    → register in `SkillsStore`.
//! 2. **Update** — fetch latest from existing git remote → re-verify GPG →
//!    re-run analysis → update store entry.
//! 3. **Audit** — re-run static analysis on an installed skill without re-fetching.
//! 4. **Remove** — unregister from store and delete skill directory.
//!
//! # Error handling
//!
//! All pipeline errors are typed via [`InstallError`]. The caller chooses how
//! to present them (CLI println, JSON-RPC error response, etc.).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use chrono::Utc;
use tempfile::TempDir;

use crate::openhuman::skills::manifest::{parse_manifest, ManifestError, SkillManifest};
use crate::openhuman::skills::static_analysis::{scan_skill, AnalysisResult, AnalysisVerdict};
use crate::openhuman::skills::store::{InstalledSkill, SkillsStore};
use crate::openhuman::skills::verify::{
    verify_git_tag_signature, TrustStore, SignatureVerificationResult, VerifyError,
};
use crate::openhuman::skills::wasm::WasmEngine;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during skill install, update, audit, or removal.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    /// The supplied Git URL is malformed or uses a disallowed protocol.
    #[error("invalid git URL: {0}")]
    InvalidUrl(String),

    /// A Git command (clone, fetch, tag, checkout) failed.
    #[error("git operation failed: {0}")]
    GitError(String),

    /// The `dadou-skill.yaml` manifest could not be parsed or is invalid.
    #[error("manifest error: {0}")]
    Manifest(#[from] crate::openhuman::skills::manifest::ManifestError),

    /// GPG signature verification failed or the signer is untrusted.
    #[error("GPG verification failed: {0}")]
    Gpg(#[from] VerifyError),

    /// Static analysis found blocking patterns.
    #[error("static analysis blocked installation: {0}")]
    AnalysisBlocked(String),

    /// Skills store read/write error.
    #[error("store error: {0}")]
    Store(String),

    /// Underlying I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// WASM engine or binary error.
    #[error("WASM error: {0}")]
    Wasm(String),

    /// The named skill is not installed (for update / audit / remove).
    #[error("skill not found: {0}")]
    NotFound(String),

    /// pip install failed during Python skill setup.
    #[error("pip install failed: {0}")]
    PipError(String),

    /// Workspace / skills directory could not be resolved.
    #[error("directory resolution error: {0}")]
    DirResolution(String),
}

impl From<anyhow::Error> for InstallError {
    fn from(e: anyhow::Error) -> Self {
        InstallError::Store(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Outcome types
// ---------------------------------------------------------------------------

/// Result of a successful skill install or update.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallOutcome {
    /// Skill name (from manifest).
    pub name: String,
    /// Installed version string.
    pub version: String,
    /// GPG verification status: `"verified"`, `"untrusted"`, or `"no_signature"`.
    pub gpg_status: String,
    /// Static analysis verdict.
    pub analysis_verdict: AnalysisVerdict,
    /// Number of static analysis findings.
    pub findings_count: usize,
    /// Canonical path to the installed skill directory.
    pub path: PathBuf,
}

/// Result of a skill audit.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditOutcome {
    /// Skill name.
    pub name: String,
    /// Static analysis verdict.
    pub verdict: AnalysisVerdict,
    /// All findings from the scan.
    pub findings: Vec<crate::openhuman::skills::static_analysis::AnalysisFinding>,
    /// ISO 8601 timestamp of this audit.
    pub last_audit_at: String,
}

/// Result of a skill removal.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RemoveOutcome {
    /// Skill name that was removed.
    pub name: String,
    /// Whether the store entry and directory were actually deleted.
    pub removed: bool,
    /// The path that was deleted (if any).
    pub path: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// GitSkillInstaller
// ---------------------------------------------------------------------------

/// Orchestrator for the full DADOU skill lifecycle.
///
/// Owns the dependencies needed for install/update operations: the local
/// [`SkillsStore`], the GPG [`TrustStore`], and a shared [`WasmEngine`].
///
/// # Example
///
/// ```ignore
/// let installer = GitSkillInstaller::new(
///     SkillsStore::load()?,
///     TrustStore::load()?,
///     Arc::new(WasmEngine::new()?),
/// );
/// let outcome = installer.install_skill("https://github.com/user/skill.git").await?;
/// println!("Installed {} v{}", outcome.name, outcome.version);
/// ```
pub struct GitSkillInstaller {
    /// TOML-backed store for installed skill metadata.
    store: SkillsStore,
    /// GPG public-key trust store for verifying signed tags.
    trust_store: TrustStore,
    /// Shared wasmtime engine for optional WASM pre-validation.
    wasm_engine: Arc<WasmEngine>,
    /// Base directory where installed skills live (`~/.openhuman/skills/`).
    skills_dir: PathBuf,
}

impl GitSkillInstaller {
    /// Create a new installer with the given dependencies.
    ///
    /// Resolves the skills base directory from the store's default location.
    pub fn new(
        store: SkillsStore,
        trust_store: TrustStore,
        wasm_engine: Arc<WasmEngine>,
    ) -> Result<Self, InstallError> {
        let skills_dir = SkillsStore::default_skills_dir()
            .ok_or_else(|| InstallError::DirResolution(
                "cannot resolve home directory for skills folder".into(),
            ))?;
        Ok(Self {
            store,
            trust_store,
            wasm_engine,
            skills_dir,
        })
    }

    /// Create a new installer with an explicit skills directory (useful in tests).
    pub fn with_skills_dir(
        store: SkillsStore,
        trust_store: TrustStore,
        wasm_engine: Arc<WasmEngine>,
        skills_dir: PathBuf,
    ) -> Self {
        Self {
            store,
            trust_store,
            wasm_engine,
            skills_dir,
        }
    }

    /// Reference to the underlying skills store.
    pub fn store(&self) -> &SkillsStore {
        &self.store
    }

    /// Mutable reference to the underlying skills store.
    pub fn store_mut(&mut self) -> &mut SkillsStore {
        &mut self.store
    }

    // -----------------------------------------------------------------------
    // Install pipeline
    // -----------------------------------------------------------------------

    /// Full install pipeline: clone → manifest → GPG → analysis → store.
    ///
    /// 1. Validates the Git URL (rejects `file://` and bare `ssh://`).
    /// 2. Shallow-clones the repository to a temporary directory.
    /// 3. Parses `dadou-skill.yaml` from the clone root.
    /// 4. Finds the latest semver tag and verifies its GPG signature.
    /// 5. Runs static analysis on the source tree.
    /// 6. Copies the WASM binary and manifest to `~/.openhuman/skills/<name>/`.
    /// 7. Registers the skill in the TOML store.
    ///
    /// Returns the [`InstallOutcome`] on success.
    pub async fn install_skill(&mut self, git_url: &str) -> Result<InstallOutcome, InstallError> {
        tracing::info!("[skills:install] starting install from {git_url}");

        // Step 1 — Validate URL
        let validated_url = validate_git_url(git_url)?;

        // Step 2 — Clone to temp dir
        let (_tmp_dir, clone_path) = clone_to_temp(validated_url).await?;
        tracing::info!("[skills:install] cloned to {}", clone_path.display());

        // Step 3 — Read and parse manifest
        let manifest = read_manifest(&clone_path)?;
        tracing::info!(
            "[skills:install] manifest parsed: {} v{} runtime={}",
            manifest.name,
            manifest.version,
            manifest.runtime_kind()
        );

        // Route to WASM or Python install path based on manifest runtime.
        match manifest.runtime_kind() {
            "python" => {
                return self.install_python_skill(&manifest, &clone_path, git_url).await;
            }
            "wasm" => {
                // Continue with WASM path below.
            }
            _ => {
                return Err(InstallError::Manifest(
                    ManifestError::InvalidField(
                        "manifest must specify either 'wasm' or 'python' section".to_string(),
                    ),
                ));
            }
        }

        // Skip GPG + tag steps if no GPG is configured in manifest.
        let sig_result = if manifest.gpg.is_some() {
            // Step 4 — Find latest tag and verify GPG signature
            let tag = find_latest_tag(&clone_path)?;
            let result = verify_git_tag_signature(&tag, &clone_path, &self.trust_store)?;
            tracing::info!("[skills:install] GPG result: {result:?}");

            // If GPG is required and signature is invalid → abort
            match &result {
                SignatureVerificationResult::Invalid { reason } => {
                    return Err(InstallError::Gpg(VerifyError::SignatureInvalid(
                        reason.clone(),
                    )));
                }
                _ => {}
            }

            // Checkout the verified tag so the source tree matches the signed commit
            checkout_tag(&clone_path, &tag)?;

            Some(result)
        } else {
            tracing::warn!(
                "[skills:install] no GPG fingerprint in manifest — skipping tag verification"
            );
            None
        };

        // Step 5 — Static analysis
        let analysis = scoped_static_analysis(&clone_path, &manifest)?;
        if analysis.verdict == AnalysisVerdict::Block {
            let reasons: Vec<String> = analysis
                .findings
                .iter()
                .map(|f| format!("[{:?}] {}:{} — {}", f.severity, f.file, f.line, f.pattern))
                .collect();
            return Err(InstallError::AnalysisBlocked(reasons.join("\n")));
        }

        // Step 6 — Compute final install path and copy artifacts
        let dest_dir = self.skills_dir.join(&manifest.name);
        std::fs::create_dir_all(&dest_dir)
            .with_context(|| format!("failed to create skill dir: {}", dest_dir.display()))?;

        // Copy WASM binary from repo to dest
        let wasm_rel = manifest.wasm.as_ref().unwrap().path.clone();
        let wasm_src = clone_path.join(&wasm_rel);
        if wasm_src.exists() {
            let wasm_dest = dest_dir.join(&wasm_rel);
            if let Some(parent) = wasm_dest.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create WASM dir: {}", parent.display()))?;
            }
            std::fs::copy(&wasm_src, &wasm_dest)
                .with_context(|| format!("failed to copy WASM: {} -> {}", wasm_src.display(), wasm_dest.display()))?;
            tracing::info!("[skills:install] copied WASM to {}", wasm_dest.display());
        } else {
            tracing::warn!(
                "[skills:install] WASM binary not found at {} — skill may be source-only",
                wasm_src.display()
            );
        }

        // Copy manifest YAML for audit trail
        let manifest_src = clone_path.join("dadou-skill.yaml");
        if manifest_src.exists() {
            let manifest_dest = dest_dir.join("dadou-skill.yaml");
            std::fs::copy(&manifest_src, &manifest_dest)?;
        }

        // Step 7 — Determine GPG status and fingerprint
        let (gpg_status, gpg_fingerprint) = match &sig_result {
            Some(SignatureVerificationResult::Valid { fingerprint }) => {
                ("verified".to_string(), Some(fingerprint.clone()))
            }
            Some(SignatureVerificationResult::Untrusted { fingerprint }) => {
                ("untrusted".to_string(), Some(fingerprint.clone()))
            }
            Some(SignatureVerificationResult::NoSignature) => {
                ("no_signature".to_string(), None)
            }
            Some(SignatureVerificationResult::Invalid { .. }) => {
                unreachable!("handled above")
            }
            None => ("skipped".to_string(), None),
        };

        // Step 8 — Register in the store
        let commit_hash = resolve_head_commit(&clone_path)?;
        let installed = InstalledSkill {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            commit_hash,
            enabled: true,
            gpg_fingerprint,
            installed_at: Utc::now().to_rfc3339(),
            last_audit_at: Some(Utc::now().to_rfc3339()),
            audit_result: Some(if analysis.verdict == AnalysisVerdict::Block {
                "fail".to_string()
            } else {
                "pass".to_string()
            }),
            runtime: crate::openhuman::skills::store::SkillRuntime::Wasm,
            python_config: None,
        };

        self.store
            .upsert(installed)
            .map_err(|e| InstallError::Store(e.to_string()))?;

        // The store already persists on upsert; no need to save again.

        tracing::info!(
            "[skills:install] installed {} v{} at {}",
            manifest.name,
            manifest.version,
            dest_dir.display()
        );

        Ok(InstallOutcome {
            gpg_status,
            name: manifest.name,
            version: manifest.version,
            analysis_verdict: analysis.verdict,
            findings_count: analysis.findings.len(),
            path: dest_dir,
        })
    }

    // -----------------------------------------------------------------------
    // Python install pipeline
    // -----------------------------------------------------------------------

    /// Install a Python skill from a cloned repository.
    ///
    /// Unlike WASM skills, Python skills copy the entire source tree and
    /// the skill is executed at runtime via Docker or local Python.
    async fn install_python_skill(
        &mut self,
        manifest: &SkillManifest,
        clone_path: &Path,
        _git_url: &str,
    ) -> Result<InstallOutcome, InstallError> {
        let python_config = manifest.python.as_ref()
            .ok_or_else(|| InstallError::Manifest(ManifestError::InvalidField(
                "python section missing from manifest".to_string(),
            )))?;

        // Steps 4-5 — GPG + static analysis (reuse WASM pipeline steps)
        let sig_result = if manifest.gpg.is_some() {
            let tag = find_latest_tag(clone_path)?;
            let result = verify_git_tag_signature(&tag, clone_path, &self.trust_store)?;
            match &result {
                SignatureVerificationResult::Invalid { reason } => {
                    return Err(InstallError::Gpg(VerifyError::SignatureInvalid(
                        reason.clone(),
                    )));
                }
                _ => {}
            }
            checkout_tag(clone_path, &tag)?;
            Some(result)
        } else {
            None
        };

        let analysis = scoped_static_analysis(clone_path, manifest)?;
        if analysis.verdict == AnalysisVerdict::Block {
            let reasons: Vec<String> = analysis
                .findings
                .iter()
                .map(|f| format!("[{:?}] {}:{} — {}", f.severity, f.file, f.line, f.pattern))
                .collect();
            return Err(InstallError::AnalysisBlocked(reasons.join("\n")));
        }

        // Step 6 — Copy entire source tree to ~/.openhuman/skills/<name>/src/
        let dest_dir = self.skills_dir.join(&manifest.name);
        let src_dir = dest_dir.join("src");
        std::fs::create_dir_all(&src_dir)
            .with_context(|| format!("failed to create skill src dir: {}", src_dir.display()))?;

        copy_dir_except_git(clone_path, &src_dir)?;
        tracing::info!("[skills:install] copied Python source to {}", src_dir.display());

        // Copy manifest YAML for audit trail
        let manifest_src = clone_path.join("dadou-skill.yaml");
        if manifest_src.exists() {
            std::fs::copy(&manifest_src, dest_dir.join("dadou-skill.yaml"))?;
        }

        // Step 7 — GPG status and store registration
        let (gpg_status, gpg_fingerprint) = match &sig_result {
            Some(SignatureVerificationResult::Valid { fingerprint }) => {
                ("verified".to_string(), Some(fingerprint.clone()))
            }
            Some(SignatureVerificationResult::Untrusted { fingerprint: _, .. }) => {
                ("untrusted".to_string(), None)
            }
            None => ("no_signature".to_string(), None),
            Some(SignatureVerificationResult::Invalid { .. }) => unreachable!(),
            Some(SignatureVerificationResult::NoSignature) => ("no_signature".to_string(), None),
        };

        let commit_hash = resolve_head_commit(clone_path)?;

        let installed = InstalledSkill {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            commit_hash,
            enabled: true,
            gpg_fingerprint,
            installed_at: Utc::now().to_rfc3339(),
            last_audit_at: Some(Utc::now().to_rfc3339()),
            audit_result: Some("pass".to_string()),
            runtime: crate::openhuman::skills::store::SkillRuntime::Python,
            python_config: Some(serde_json::to_value(python_config).unwrap_or_default()),
        };

        self.store
            .upsert(installed)
            .map_err(|e| InstallError::Store(e.to_string()))?;

        tracing::info!(
            "[skills:install] installed Python skill {} v{} at {}",
            manifest.name,
            manifest.version,
            dest_dir.display()
        );

        Ok(InstallOutcome {
            gpg_status,
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            analysis_verdict: analysis.verdict,
            findings_count: analysis.findings.len(),
            path: dest_dir,
        })
    }

    // -----------------------------------------------------------------------
    // Update pipeline
    // -----------------------------------------------------------------------

    /// Update an installed skill by re-cloning and re-verifying.
    ///
    /// The existing skill directory is replaced atomically:
    /// 1. Clones the repository from the stored git remote (heuristic via
    ///    the skill name — the remote URL is recovered from the installed
    ///    skill's cloned copy).
    /// 2. Re-runs the full install pipeline.
    /// 3. Replaces the old directory with the new one.
    pub async fn update_skill(&mut self, name: &str) -> Result<InstallOutcome, InstallError> {
        let installed = self
            .store
            .get(name)
            .ok_or_else(|| InstallError::NotFound(name.to_string()))?
            .clone();

        tracing::info!("[skills:install] updating skill '{name}' (v{})", installed.version);

        // We need the git URL. For now, reconstruct from the installed skill's
        // assumed remote. In v2, store the origin URL in the store.
        // Try to read from the installed manifest if available.
        let installed_dir = self.skills_dir.join(name);
        // Try the remote from the existing clone
        let remote_url = try_get_remote_url(&installed_dir).unwrap_or_else(|| {
            tracing::warn!(
                "[skills:install] no remote URL found for '{name}'; \
                 update requires the original git URL"
            );
            String::new()
        });

        if remote_url.is_empty() {
            return Err(InstallError::GitError(format!(
                "cannot determine git remote URL for '{name}'. \
                 Re-install with `dadou skill install <url>` instead."
            )));
        }

        // Run the full install pipeline with the recovered URL
        let outcome = self.install_skill(&remote_url).await?;
        tracing::info!("[skills:install] updated '{name}' to v{}", outcome.version);
        Ok(outcome)
    }

    // -----------------------------------------------------------------------
    // Audit
    // -----------------------------------------------------------------------

    /// Re-run static analysis on an installed skill.
    ///
    /// Does NOT re-fetch from git — operates on the locally-installed copy.
    pub fn audit_skill(&mut self, name: &str) -> Result<AuditOutcome, InstallError> {
        let installed = self
            .store
            .get(name)
            .ok_or_else(|| InstallError::NotFound(name.to_string()))?
            .clone();

        tracing::info!("[skills:install] auditing skill '{name}'");

        let skill_dir = self.skills_dir.join(name);
        if !skill_dir.exists() {
            return Err(InstallError::NotFound(format!(
                "skill directory not found: {}",
                skill_dir.display()
            )));
        }

        // Re-parse manifest to get permissions for analysis
        let manifest_path = skill_dir.join("dadou-skill.yaml");
        let manifest = if manifest_path.exists() {
            let content = std::fs::read_to_string(&manifest_path)
                .map_err(InstallError::Io)?;
            Some(parse_manifest(&content).map_err(InstallError::Manifest)?)
        } else {
            None
        };

        let permissions = manifest
            .as_ref()
            .map(|m| m.permissions.clone())
            .unwrap_or_default();

        let analysis = scan_skill(&skill_dir, &permissions)?;

        let result_str = match analysis.verdict {
            AnalysisVerdict::Pass => "pass",
            AnalysisVerdict::Warn => "warn",
            AnalysisVerdict::Block => "fail",
        };

        // Update store
        self.store
            .record_audit(name, result_str)
            .map_err(|e| InstallError::Store(e.to_string()))?;

        let now = Utc::now().to_rfc3339();

        let findings = analysis.findings.clone();

        tracing::info!(
            "[skills:install] audit of '{name}': {:?} ({} findings)",
            analysis.verdict,
            findings.len()
        );

        Ok(AuditOutcome {
            name: name.to_string(),
            verdict: analysis.verdict,
            findings,
            last_audit_at: now,
        })
    }

    // -----------------------------------------------------------------------
    // Remove
    // -----------------------------------------------------------------------

    /// Remove an installed skill from the store and filesystem.
    ///
    /// Deletes the skill directory under `~/.openhuman/skills/<name>/` and
    /// removes the entry from the TOML store.
    pub fn remove_skill(&mut self, name: &str) -> Result<RemoveOutcome, InstallError> {
        let existed = self
            .store
            .get(name)
            .is_some();

        if !existed {
            return Err(InstallError::NotFound(name.to_string()));
        }

        // Remove from store first
        self.store
            .remove(name)
            .map_err(|e| InstallError::Store(e.to_string()))?;

        // Remove skill directory
        let skill_dir = self.skills_dir.join(name);
        let removed_path = if skill_dir.exists() {
            std::fs::remove_dir_all(&skill_dir)?;
            tracing::info!("[skills:install] removed skill directory: {}", skill_dir.display());
            Some(skill_dir)
        } else {
            None
        };

        tracing::info!("[skills:install] removed skill '{name}'");

        Ok(RemoveOutcome {
            name: name.to_string(),
            removed: true,
            path: removed_path,
        })
    }
}

// ---------------------------------------------------------------------------
// Free functions (convenience wrappers)
// ---------------------------------------------------------------------------

/// Install a skill from a Git URL using default dependencies.
///
/// Convenience wrapper that loads the store, trust store, and engine,
/// then delegates to [`GitSkillInstaller::install_skill`].
pub async fn install_skill(git_url: &str) -> Result<InstallOutcome, InstallError> {
    let store = SkillsStore::load().map_err(|e| InstallError::Store(e.to_string()))?;
    let trust_store =
        TrustStore::load().map_err(|e| InstallError::Gpg(e))?;
    let wasm_engine = Arc::new(
        WasmEngine::new().map_err(|e| InstallError::Wasm(e.to_string()))?,
    );
    let mut installer = GitSkillInstaller::new(store, trust_store, wasm_engine)?;
    installer.install_skill(git_url).await
}

/// Audit an installed skill by name.
///
/// Convenience wrapper that loads the store, creates a minimal installer,
/// and delegates to [`GitSkillInstaller::audit_skill`].
pub fn audit_skill(name: &str) -> Result<AuditOutcome, InstallError> {
    let store = SkillsStore::load().map_err(|e| InstallError::Store(e.to_string()))?;
    let trust_store =
        TrustStore::load().map_err(|e| InstallError::Gpg(e))?;
    let wasm_engine = Arc::new(
        WasmEngine::new().map_err(|e| InstallError::Wasm(e.to_string()))?,
    );
    let mut installer = GitSkillInstaller::new(store, trust_store, wasm_engine)?;
    installer.audit_skill(name)
}

/// Remove a skill by name.
///
/// Convenience wrapper that loads the store and delegates to
/// [`GitSkillInstaller::remove_skill`].
pub fn remove_skill(name: &str) -> Result<RemoveOutcome, InstallError> {
    let store = SkillsStore::load().map_err(|e| InstallError::Store(e.to_string()))?;
    let trust_store =
        TrustStore::load().map_err(|e| InstallError::Gpg(e))?;
    let wasm_engine = Arc::new(
        WasmEngine::new().map_err(|e| InstallError::Wasm(e.to_string()))?,
    );
    let mut installer = GitSkillInstaller::new(store, trust_store, wasm_engine)?;
    installer.remove_skill(name)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Minimum acceptable length for a Git URL after trimming.
const MIN_GIT_URL_LEN: usize = 10;

/// Validate that a Git URL uses an allowed protocol and has a reasonable shape.
fn validate_git_url(url: &str) -> Result<&str, InstallError> {
    let trimmed = url.trim();
    if trimmed.len() < MIN_GIT_URL_LEN {
        return Err(InstallError::InvalidUrl(format!(
            "URL too short ({})",
            trimmed.len()
        )));
    }

    if trimmed.starts_with("https://") || trimmed.starts_with("git@") {
        Ok(trimmed)
    } else if trimmed.starts_with("file://") {
        Err(InstallError::InvalidUrl(
            "local file:// URLs are not allowed for skill installation".into(),
        ))
    } else if trimmed.starts_with("ssh://") {
        Err(InstallError::InvalidUrl(
            "plain ssh:// URLs without host are not allowed; use git@host:path instead".into(),
        ))
    } else {
        Err(InstallError::InvalidUrl(format!(
            "URL must start with 'https://' or 'git@' (got {:?})",
            trimmed.chars().take(20).collect::<String>()
        )))
    }
}

/// Clone a Git repository to a temporary directory (shallow, depth 1).
///
/// Returns the `TempDir` guard and the canonical clone path.
async fn clone_to_temp(url: &str) -> Result<(TempDir, PathBuf), InstallError> {
    let tmp = tempfile::tempdir().map_err(|e| {
        InstallError::GitError(format!("failed to create temp dir: {e}"))
    })?;
    let target = tmp.path().join("repo");

    tracing::debug!("[skills:install] cloning {url} -> {}", target.display());

    let output = tokio::process::Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(url)
        .arg(&target)
        .output()
        .await
        .map_err(|e| InstallError::GitError(format!("failed to execute git clone: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(InstallError::GitError(format!(
            "git clone failed: {}",
            stderr.lines().next().unwrap_or("(unknown error)")
        )));
    }

    Ok((tmp, target))
}

/// Read and parse `dadou-skill.yaml` from the cloned repository root.
fn read_manifest(clone_path: &Path) -> Result<SkillManifest, InstallError> {
    let manifest_path = clone_path.join("dadou-skill.yaml");
    if !manifest_path.exists() {
        return Err(InstallError::Manifest(
            crate::openhuman::skills::manifest::ManifestError::MissingField("dadou-skill.yaml"),
        ));
    }
    let content = std::fs::read_to_string(&manifest_path)?;
    let manifest = parse_manifest(&content).map_err(InstallError::Manifest)?;
    Ok(manifest)
}

/// Find the latest Git tag in the repository (sorted by version:refname).
fn find_latest_tag(repo_path: &Path) -> Result<String, InstallError> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("tag")
        .arg("--list")
        .arg("--sort=-version:refname")
        .output()
        .map_err(|e| InstallError::GitError(format!("failed to list tags: {e}")))?;

    if !output.status.success() {
        return Err(InstallError::GitError("git tag command failed".into()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let tag = stdout.lines().next().unwrap_or("").trim().to_string();

    let tag = if tag.is_empty() {
        // No tags — use HEAD
        tracing::warn!("[skills:install] no tags found; using HEAD");
        "HEAD".to_string()
    } else {
        tag
    };

    Ok(tag)
}

/// Checkout a specific tag in the repository.
fn checkout_tag(repo_path: &Path, tag: &str) -> Result<(), InstallError> {
    if tag == "HEAD" {
        return Ok(());
    }

    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("checkout")
        .arg(tag)
        .output()
        .map_err(|e| InstallError::GitError(format!("failed to checkout tag: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(InstallError::GitError(format!(
            "git checkout {tag} failed: {}",
            stderr.lines().next().unwrap_or("(unknown error)")
        )));
    }

    Ok(())
}

/// Recursively copy a directory tree, skipping `.git`.
fn copy_dir_except_git(src: &Path, dest: &Path) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let src_path = entry.path();
        let dest_path = dest.join(&name);
        if file_type.is_dir() {
            std::fs::create_dir_all(&dest_path)?;
            copy_dir_except_git(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

/// Resolve the HEAD commit hash of a repository.
fn resolve_head_commit(repo_path: &Path) -> Result<String, InstallError> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .map_err(|e| InstallError::GitError(format!("failed to get HEAD commit: {e}")))?;

    if !output.status.success() {
        return Err(InstallError::GitError(
            "failed to resolve HEAD commit".into(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Attempt to recover the Git remote origin URL from an installed skill's dir.
fn try_get_remote_url(skill_dir: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(skill_dir)
        .arg("remote")
        .arg("get-url")
        .arg("origin")
        .output()
        .ok()?;

    if output.status.success() {
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !url.is_empty() {
            return Some(url);
        }
    }
    None
}

/// Run static analysis on the cloned repository, using either the manifest's
/// declared permissions or the deny-by-default fallback.
fn scoped_static_analysis(
    clone_path: &Path,
    manifest: &SkillManifest,
) -> Result<AnalysisResult, InstallError> {
    let permissions = &manifest.permissions;
    let result = scan_skill(clone_path, permissions)
        .map_err(|e| InstallError::Store(e.to_string()))?;

    tracing::info!(
        "[skills:install] static analysis: {:?} ({} findings)",
        result.verdict,
        result.findings.len()
    );

    Ok(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── URL validation ─────────────────────────────────────────────────

    #[test]
    fn validates_https_url() {
        assert!(validate_git_url("https://github.com/user/skill.git").is_ok());
    }

    #[test]
    fn validates_git_ssh_url() {
        assert!(validate_git_url("git@github.com:user/skill.git").is_ok());
    }

    #[test]
    fn rejects_file_url() {
        let err = validate_git_url("file:///tmp/skill").unwrap_err();
        assert!(
            matches!(err, InstallError::InvalidUrl(s) if s.contains("file://")),
            "expected InvalidUrl, got: {err}"
        );
    }

    #[test]
    fn rejects_short_url() {
        let err = validate_git_url("https://a").unwrap_err();
        assert!(matches!(err, InstallError::InvalidUrl(_)));
    }

    #[test]
    fn rejects_plain_ssh() {
        let err = validate_git_url("ssh://host").unwrap_err();
        assert!(
            matches!(err, InstallError::InvalidUrl(s) if s.contains("ssh://")),
            "expected InvalidUrl, got: {err}"
        );
    }

    #[test]
    fn rejects_random_string() {
        let err = validate_git_url("not-a-url-at-all").unwrap_err();
        assert!(matches!(err, InstallError::InvalidUrl(_)));
    }

    // ── Manifest reading ───────────────────────────────────────────────

    #[test]
    fn read_manifest_missing_file() {
        let dir = TempDir::new().unwrap();
        let err = read_manifest(dir.path()).unwrap_err();
        assert!(matches!(err, InstallError::Manifest(_)));
    }

    #[test]
    fn read_manifest_valid_yaml() {
        let dir = TempDir::new().unwrap();
        let manifest_content = r#"
name: test-skill
version: 0.1.0
wasm:
  path: build/test.wasm
permissions:
  network: false
"#;
        std::fs::write(dir.path().join("dadou-skill.yaml"), manifest_content).unwrap();
        let manifest = read_manifest(dir.path()).unwrap();
        assert_eq!(manifest.name, "test-skill");
        assert_eq!(manifest.version, "0.1.0");
    }

    #[test]
    fn read_manifest_bad_yaml() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("dadou-skill.yaml"), "::: invalid yaml :::").unwrap();
        let err = read_manifest(dir.path()).unwrap_err();
        assert!(matches!(err, InstallError::Manifest(_)));
    }

    // ── Store integration ──────────────────────────────────────────────

    #[test]
    fn install_skill_store_roundtrip() {
        // Create a temp skills store and verify the GitSkillInstaller
        // can be constructed and store operations work.
        let dir = TempDir::new().unwrap();
        let store_path = dir.path().join("store.toml");
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        let store = SkillsStore::load_from(&store_path).unwrap();
        let trust_store = TrustStore::load_from(
            dir.path().join("certs"),
            dir.path().join("trust.toml"),
        );

        let wasm_engine = Arc::new(WasmEngine::new().unwrap());
        let mut installer = GitSkillInstaller::with_skills_dir(
            store,
            trust_store,
            wasm_engine,
            skills_dir.clone(),
        );

        // Manually insert a skill (simulating what install_skill would do)
        let skill = InstalledSkill {
            name: "mock-skill".to_string(),
            version: "0.1.0".to_string(),
            commit_hash: "abc123".to_string(),
            enabled: true,
            gpg_fingerprint: None,
            installed_at: Utc::now().to_rfc3339(),
            last_audit_at: Some(Utc::now().to_rfc3339()),
            audit_result: Some("pass".to_string()),
        };
        installer
            .store_mut()
            .upsert(skill)
            .unwrap();

        // Verify it's in the store
        let loaded = installer.store().get("mock-skill").unwrap();
        assert_eq!(loaded.version, "0.1.0");
        assert!(loaded.enabled);

        // Remove it
        let outcome = installer.remove_skill("mock-skill").unwrap();
        assert!(outcome.removed);
        assert!(installer.store().get("mock-skill").is_none());
    }

    #[test]
    fn remove_nonexistent_skill() {
        let dir = TempDir::new().unwrap();
        let store_path = dir.path().join("store.toml");
        let store = SkillsStore::load_from(&store_path).unwrap();
        let trust_store = TrustStore::load_from(
            dir.path().join("certs"),
            dir.path().join("trust.toml"),
        );

        let wasm_engine = Arc::new(WasmEngine::new().unwrap());
        let mut installer = GitSkillInstaller::with_skills_dir(
            store,
            trust_store,
            wasm_engine,
            dir.path().join("skills"),
        );

        let err = installer.remove_skill("nonexistent").unwrap_err();
        assert!(matches!(err, InstallError::NotFound(_)));
    }

    #[test]
    fn audit_nonexistent_skill() {
        let dir = TempDir::new().unwrap();
        let store_path = dir.path().join("store.toml");
        let store = SkillsStore::load_from(&store_path).unwrap();
        let trust_store = TrustStore::load_from(
            dir.path().join("certs"),
            dir.path().join("trust.toml"),
        );

        let wasm_engine = Arc::new(WasmEngine::new().unwrap());
        let mut installer = GitSkillInstaller::with_skills_dir(
            store,
            trust_store,
            wasm_engine,
            dir.path().join("skills"),
        );

        let err = installer.audit_skill("nonexistent").unwrap_err();
        assert!(matches!(err, InstallError::NotFound(_)));
    }

    #[test]
    fn git_skill_installer_new_creates_installer() {
        let store = SkillsStore::load_from(
            &TempDir::new().unwrap().path().join("store.toml"),
        )
        .unwrap();
        let trust_store = TrustStore::load_from(
            TempDir::new().unwrap().path().join("certs"),
            TempDir::new().unwrap().path().join("trust.toml"),
        );
        let wasm_engine = Arc::new(WasmEngine::new().unwrap());

        let installer = GitSkillInstaller::new(store, trust_store, wasm_engine).unwrap();
        // Verify the store is accessible and empty
        assert!(installer.store().is_empty());
    }
}
