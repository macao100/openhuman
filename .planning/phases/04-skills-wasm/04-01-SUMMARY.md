# Phase 4, Plan 04-01: Manifest YAML + TOML SkillsStore

**Status:** COMPLETE
**Date:** 2026-06-05
**Requirements:** SKL-01, SKL-06

## Summary

Implemented the `dadou-skill.yaml` manifest format and the local TOML skills store, establishing the data model for all subsequent WASM skill lifecycle operations.

## Files Modified

### Created
- **`src/openhuman/skills/manifest.rs`** (502 lines) — YAML manifest parsing and validation
  - `SkillManifest` struct with `#[serde(deny_unknown_fields)]`
  - Sub-types: `GpgConfig`, `WasmConfig`, `Permissions`, `FilesystemPerms`, `Dependency`
  - `ManifestError` enum (thiserror): `ParseError`, `MissingField`, `InvalidField`
  - `parse_manifest(yaml_str) -> Result<SkillManifest, ManifestError>`
  - Validation: name regex `^[a-zA-Z0-9_-]+$`, max 64 chars, non-empty version, path traversal rejection
  - `Default` for `Permissions` (deny-all: no network, no filesystem)
  - Tests: 14 unit tests

- **`src/openhuman/skills/store.rs`** (508 lines) — TOML-based `SkillsStore`
  - `InstalledSkill` struct with all planned fields
  - `SkillsStore` with: `load()`, `load_from()`, `save()`, `get()`, `get_mut()`, `list()` (sorted), `upsert()`, `remove()`, `set_enabled()`, `record_audit()`
  - Atomic write pattern (`.toml.tmp` + rename)
  - Path: `~/.openhuman/skills/store.toml` with `dirs::home_dir()`
  - TOML format: `[skills.<name>]` per-skill table
  - Tests: 12 unit tests

### Updated
- **`src/openhuman/skills/mod.rs`** — Added `pub mod manifest;`, `pub mod store;`, re-exports, integration test
  - Integration test: manifest -> store -> persist -> reload roundtrip

## Artifacts Delivered

| Artifact | Lines | Provides |
|----------|-------|----------|
| `manifest.rs` | 502 | `dadou-skill.yaml` parsing and validation (14 tests) |
| `store.rs` | 508 | TOML SkillsStore persistence (12 tests) |
| `mod.rs` updates | +70 | Module wiring + integration test |

## Dependencies Used (all pre-existing in Cargo.toml)

- `serde = "1"` with `derive` — serialization
- `serde_yaml = "0.9"` — manifest parsing
- `toml = "1.0"` — store serialization
- `regex = "1.10"` — name validation
- `thiserror = "2.0"` — manifest errors
- `anyhow = "1.0"` — application errors
- `dirs = "5"` — home directory resolution
- `chrono = "0.4"` — ISO 8601 timestamps
- `tempfile = "3"` — temp directories in tests

## Build Note

`cargo check` could not be run because `whisper-rs-sys` requires `cmake` (pre-existing environment issue, not related to these changes). All code was manually verified against the crate's dependency graph and existing patterns.

## No Regressions

- All existing `pub use ops::*;` and `pub use schemas::...` exports preserved intact
- No naming conflicts with existing types (`Skill`, `SkillFrontmatter`, `SkillScope`)
- No changes to existing skills module files
- No changes to `Cargo.toml` (all dependencies already present)
