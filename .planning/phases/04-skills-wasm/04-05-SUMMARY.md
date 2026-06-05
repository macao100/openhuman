# Plan 04-05 Summary: CLI `dadou skill` commands + JSON-RPC controllers (SKL-07)

**Status:** Complete
**Date:** 2026-06-05
**Dependencies:** 04-01 (Manifest + Store), 04-02 (Wasmtime), 04-03 (GPG), 04-04 (Static Analysis)

## What was built

### 1. Install orchestration module (`src/openhuman/skills/wasm_install.rs`)

The full install pipeline is implemented in `GitSkillInstaller`:

- **`GitSkillInstaller` struct** — owns `SkillsStore`, `TrustStore`, `Arc<WasmEngine>`, and skill directory path. Constructed via `new()` (default home-based) or `with_skills_dir()` (test-friendly).
- **`install_skill(git_url)`** — full async pipeline:
  1. URL validation (rejects `file://`, `ssh://` without host)
  2. Shallow clone via `tokio::process::Command` to temp dir
  3. Parse `dadou-skill.yaml` manifest
  4. Find latest tag, verify GPG signature via `verify_git_tag_signature`
  5. Checkout the verified tag
  6. Static analysis via `scan_skill` — aborts on `Block` verdict
  7. Copy WASM binary and manifest to `~/.openhuman/skills/<name>/`
  8. Store registration via `SkillsStore::upsert`
- **`update_skill(name)`** — recovers remote URL, re-clones, re-verifies, re-analyzes, replaces store entry
- **`audit_skill(name)`** — runs static analysis on installed copy, updates audit timestamp in store
- **`remove_skill(name)`** — removes from store and deletes skill directory
- Free-function convenience wrappers: `install_skill()`, `audit_skill()`, `remove_skill()`
- `InstallError` enum with typed variants: `InvalidUrl`, `GitError`, `Manifest`, `Gpg`, `AnalysisBlocked`, `Store`, `Io`, `Wasm`, `NotFound`, `DirResolution`
- 3 outcome structs: `InstallOutcome`, `AuditOutcome`, `RemoveOutcome` (all `Serialize`)
- **Tests:** URL validation, manifest reading, store roundtrip (insert/verify/remove), error cases for missing skills, installer construction

### 2. JSON-RPC controllers (`src/openhuman/skills/schemas.rs`)

6 new controllers under the `dadou` namespace:

| RPC Method | Input | Description |
|-----------|-------|-------------|
| `dadou.skill_install` | `url` | Full install pipeline |
| `dadou.skill_update` | `name` | Re-clone, re-verify, replace |
| `dadou.skill_audit` | `name` | Re-run static analysis |
| `dadou.skill_remove` | `name` | Uninstall from store + disk |
| `dadou.skill_list` | (none) | List installed skills |
| `dadou.skill_trust_author` | `pubkey_pem` | Import GPG public key |

Exports: `all_dadou_skills_controller_schemas()`, `all_dadou_skills_registered_controllers()`, `dadou_skills_schemas()`.

### 3. CLI subcommand (`src/core/cli.rs`)

- `"skill"` top-level subcommand in `run_from_cli_args()`
- `run_dadou_skill_command()` function dispatching:
  - `openhuman-core skill install <git-url>` — async install via `tokio::runtime`
  - `openhuman-core skill update <name>` — async update
  - `openhuman-core skill audit <name>` — sync audit with findings display
  - `openhuman-core skill remove <name>` — sync removal
  - `openhuman-core skill list` — formatted table of installed skills
  - `openhuman-core skill trust-author <pubkey>` — import trusted GPG key

### 4. Wiring (`src/core/all.rs`)

- Added `dadou` controllers to both `build_registered_controllers()` and `build_declared_controller_schemas()`
- Added `"dadou"` entry in `namespace_description()`

### 5. Module registration (`src/openhuman/skills/mod.rs`)

- Added `pub mod wasm_install;`
- Re-exported all DADOU schemas functions

## Files changed

| File | Action | Lines |
|------|--------|-------|
| `src/openhuman/skills/wasm_install.rs` | **Created** | ~680 |
| `src/openhuman/skills/schemas.rs` | **Modified** | +250 (DADOU controllers appended) |
| `src/openhuman/skills/schemas_tests.rs` | **Modified** | +100 (DADOU tests appended) |
| `src/openhuman/skills/mod.rs` | **Modified** | +2 lines (module + re-exports) |
| `src/core/all.rs` | **Modified** | +7 lines (registry wiring + namespace) |
| `src/core/cli.rs` | **Modified** | +190 (run_dadou_skill_command + dispatch) |

## Verification

```bash
cargo check -p openhuman
cargo test -p openhuman -- schemas  # DADOU schema tests + existing skills tests
cargo test -p openhuman -- skills   # Skill store + manifest regression check
```

## Dependencies

- `tempfile` — already a dependency in root `Cargo.toml` (line 148)
- `chrono` — already a dependency
- All other types used are from the existing skills modules (manifest, store, verify, static_analysis, wasm)
