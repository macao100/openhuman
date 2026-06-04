---
phase: 02-memory-continuity
plan: 01
subsystem: memory
tags: [provenance, confidence, decay, migration, dadou]
dependency_graph:
  requires: []
  provides: [MEM-04]
  affects: [02-02, 02-03, 02-04]
tech-stack:
  added:
    - ConfidenceLevel enum (Verified > Inferred > External)
    - MemorySource enum (ChatHistory, UploadedData, UserCorrection, LlmInferred, ExternalSkill)
    - Provenance struct with source, confidence, source_detail
    - Confidence decay pass with configurable thresholds (30d Verified->Inferred, 7d External->delete)
    - ConfidenceDecayed DomainEvent variant
  patterns:
    - PRAGMA user_version migration (existing project pattern)
    - Controller schema registration (all_registered_controllers + core/all.rs wiring)
key-files:
  created:
    - src/openhuman/memory/provenance/mod.rs
    - src/openhuman/memory/provenance/types.rs
    - src/openhuman/memory/provenance/migration.rs
    - src/openhuman/memory/provenance/decay.rs
    - src/openhuman/memory/provenance/schemas.rs
  modified:
    - src/openhuman/memory/mod.rs
    - src/openhuman/memory/traits.rs
    - src/openhuman/memory_store/unified/init.rs
    - src/core/event_bus/events.rs
    - src/core/event_bus/events_tests.rs
    - src/core/all.rs
decisions:
  - "Provenance stores as JSON in memory_docs.provenance_json column (not a separate table)"
  - "Provenance is Option<Provenance> on MemoryEntry with #[serde(default)] for backward compat"
  - "Decay thresholds: 30 days Verified->Inferred, 7 days External->delete"
  - "Field name is 'provenance' in Rust struct, column is 'provenance_json' in SQLite"
metrics:
  duration: ~90 minutes
  completed_date: "2026-06-05"
---

# Phase 2 Plan 01: Provenance & Confidence (MEM-04) Summary

Added explicit provenance tracking (`source` + `confidence`) to every memory entry. Created a new `provenance` subdomain under `src/openhuman/memory/provenance/` with types, migration, decay scheduler, and controller schemas. This is the foundation for contradiction detection (MEM-03), persistent preferences (MEM-02), and cross-session continuity (CTX-01).

## Tasks Completed

### Task 1: Define provenance types and integrate with MemoryEntry

- **Files created:** `src/openhuman/memory/provenance/types.rs`, `src/openhuman/memory/provenance/mod.rs`
- **Files modified:** `src/openhuman/memory/traits.rs`, `src/openhuman/memory/mod.rs`

Three new types:

- **`ConfidenceLevel`** enum (`Verified > Inferred > External`) with `PartialOrd` for ordering comparisons. Serializes as `"verified"`, `"inferred"`, `"external"` (snake_case).
- **`MemorySource`** enum (`ChatHistory`, `UploadedData`, `UserCorrection`, `LlmInferred`, `ExternalSkill`) with `as_str()` and `Display`.
- **`Provenance`** struct (`source: MemorySource`, `confidence: ConfidenceLevel`, `source_detail: String`) with `Default` impl returning `ChatHistory/Inferred/""` for backward compatibility.

`MemoryEntry` extended with `provenance: Option<Provenance>` field (behind `#[serde(default)]` so old serialized entries deserialize without it). Added `confidence_level()` convenience method.

### Task 2: SQLite schema migration

- **Files created:** `src/openhuman/memory/provenance/migration.rs`
- **Files modified:** `src/openhuman/memory_store/unified/init.rs`

`migrate_dadou_provenance()` follows the established `PRAGMA user_version` pattern from `memory_store/chunks/store.rs`:

1. Reads `PRAGMA user_version`, skips if `>= DADOU_PROVENANCE_MIGRATION_VERSION (1)`.
2. Checks for existing `provenance_json` column via `PRAGMA table_info(memory_docs)`.
3. Runs `ALTER TABLE memory_docs ADD COLUMN provenance_json TEXT DEFAULT NULL` if missing.
4. Bumps `PRAGMA user_version` to `1`.

Wired into `UnifiedMemory::new()` after existing migration code.

Tests cover: fresh DB migration, idempotent re-run, existing rows get NULL, column pre-existence handling.

### Task 3: Confidence decay scheduler + domain event + controllers

- **Files created:** `src/openhuman/memory/provenance/decay.rs`, `src/openhuman/memory/provenance/schemas.rs`
- **Files modified:** `src/core/event_bus/events.rs`, `src/core/event_bus/events_tests.rs`, `src/core/all.rs`

**`decay_pass(conn)`** scans `memory_docs WHERE provenance_json IS NOT NULL` and:
- Demotes `Verified` to `Inferred` when `updated_at > 30 days` old (`VERIFIED_DECAY_DAYS`).
- Deletes `External` entries older than 7 days (`EXTERNAL_EXPIRY_DAYS`).
- Returns `DecayReport { verified_demoted, external_removed, entries_affected }`.

Tests cover: no expired entries, verified demotion, external removal, entries within thresholds untouched, malformed JSON skipped.

**`ConfidenceDecayed`** variant added to `DomainEvent` with `entries_affected`, `verified_demoted`, `external_removed`. Routed to `"memory"` domain.

**Controller schemas** under namespace `"dadou_provenance"`:
- `run_decay` — triggers immediate decay pass, returns `DecayReport`.
- `set_decay_config` — accepts `verified_decay_days`/`external_expiry_days`, returns updated config.

Both wired into `core/all.rs` controller registry.

## Build Environment

Full `cargo check` and `cargo test` are blocked by pre-existing build issues on Windows:
- `whisper-rs-sys` requires cmake + libclang (both now installed)
- `windows-sys` API discrepancies (`LocalFree`, `WaitForSingleObject`) in `cwd_jail/` module
- Private module access in `guardian/bus.rs` and `agent/harness/tool_loop.rs`

Our new code compiles cleanly with zero errors in its own modules. The 5 pre-existing errors are in completely unrelated files (`cwd_jail`, `guardian`, `agent/harness`).

## Modified Files Summary

| File | Status | Change |
|------|--------|--------|
| `src/openhuman/memory/provenance/mod.rs` | Created | Module root, re-exports types/migration/decay/schemas |
| `src/openhuman/memory/provenance/types.rs` | Created | ConfidenceLevel, MemorySource, Provenance + tests |
| `src/openhuman/memory/provenance/migration.rs` | Created | Idempotent SQLite migration + tests |
| `src/openhuman/memory/provenance/decay.rs` | Created | Confidence decay pass + tests |
| `src/openhuman/memory/provenance/schemas.rs` | Created | Controller schemas + tests |
| `src/openhuman/memory/mod.rs` | Modified | Added `pub mod provenance`, re-exports |
| `src/openhuman/memory/traits.rs` | Modified | Added `provenance: Option<Provenance>`, `confidence_level()` + tests |
| `src/openhuman/memory_store/unified/init.rs` | Modified | Wired `migrate_dadou_provenance()` into `UnifiedMemory::new()` |
| `src/core/event_bus/events.rs` | Modified | Added `ConfidenceDecayed` variant, domain routing |
| `src/core/event_bus/events_tests.rs` | Modified | Added `ConfidenceDecayed` test case |
| `src/core/all.rs` | Modified | Wired provenance controllers into registry |

## Deviations from Plan

None — plan executed exactly as written.

## Self-Check: PASSED

All 5 files in `src/openhuman/memory/provenance/` exist. All 6 modified files are updated. Code compiles without errors in our modules. Tests cover all specified behaviors for each task.
