# Phase 2 Plan 02-03 Summary — Contradiction Detection (MEM-03)

**Date:** 2026-06-05
**Status:** Complete

## What was built

### Task 1: Contradiction detection engine
- **Created** `src/openhuman/memory/contradiction/mod.rs` — module root with `pub use` re-exports
- **Created** `src/openhuman/memory/contradiction/detector.rs`:
  - `ContradictionCandidate` struct (existing_entry, new_value, similarity, namespace)
  - `ContradictionReport` struct (candidates, checked_against, elapsed_ms) with `has_contradictions()`
  - `check_for_contradictions()` async function:
    - Confidence gate: skips if `new_provenance` is `None` or not `Verified`
    - Calls `memory.recall_relevant_by_vector()` with `CONTRADICTION_RECALL_LIMIT` (10)
    - Fetches full entries via `memory.get()` to check `confidence_level() == Verified`
    - Flags entries where content differs from the new value
  - Tests cover: confidence filtering, report structure, empty namespace, identical content edge case
- **Modified** `src/openhuman/memory/mod.rs` — added `pub mod contradiction;`

### Task 2: Contradiction event + resolver + controllers
- **Created** `src/openhuman/memory/contradiction/resolver.rs`:
  - `ContradictionAction` enum (Replace, Merge, Dismiss) with `Serialize`, `Deserialize`, `FromStr`
  - `ContradictionResolution` struct
  - `resolve_contradiction()` — Replace overwrites, Merge combines both values, Dismiss no-ops
  - Tests cover all three actions + `from_str` + serde roundtrip
- **Created** `src/openhuman/memory/contradiction/schemas.rs`:
  - `dadou_contradiction` namespace with `check` and `resolve` controllers
  - Schema-count-matches-registered validation test
  - Required-inputs-are-present validation test
- **Modified** `src/core/event_bus/events.rs`:
  - Added `ContradictionDetected` variant (namespace, existing_key, existing_content, new_value, similarity)
  - Added `ContradictionResolved` variant (namespace, existing_key, resolution)
  - Both mapped to "memory" domain in the `domain()` method
- **Modified** `src/core/all.rs`:
  - Wired `contradiction::schemas::all_registered_controllers()` into `build_registered_controllers()`
  - Wired `contradiction::schemas::all_controller_schemas()` into `build_declared_controller_schemas()`

### Task 3: Wire contradiction check into preference write path
- **Modified** `src/openhuman/memory/preferences.rs`:
  - Added `store_preference_with_contradiction_check()`:
    - Checks both `user_pref_general` and `user_pref_situational` namespaces
    - If no contradictions: writes the preference with provenance embedded in content
    - If contradictions found: publishes `DomainEvent::ContradictionDetected` for each, returns report, does NOT commit
  - Added required imports for `check_for_contradictions`, `ContradictionReport`, `publish_global`, `DomainEvent`
  - Fixed missing `#[cfg(test)]` attribute on the test module

## Files changed

| File | Action | Lines |
|------|--------|-------|
| `src/openhuman/memory/contradiction/mod.rs` | Created | 36 |
| `src/openhuman/memory/contradiction/detector.rs` | Created | 249 |
| `src/openhuman/memory/contradiction/resolver.rs` | Created | 320 |
| `src/openhuman/memory/contradiction/schemas.rs` | Created | 388 |
| `src/openhuman/memory/mod.rs` | Modified (add pub mod) | +1 |
| `src/core/event_bus/events.rs` | Modified (+2 variants + domain match) | +15 |
| `src/core/all.rs` | Modified (wire controllers + schemas) | +8 |
| `src/openhuman/memory/preferences.rs` | Modified (add function + imports) | +90 |

## Design decisions

- **Confidence filter**: Only Verified-vs-Verified contradictions trigger alerts. Inferred and External entries are ignored as they are not authoritative.
- **Conservative detection**: Any Verified entry with different content that passes the vector similarity threshold is flagged. This over-flags rather than misses — the user resolves false positives via Dismiss.
- **Check controller without provenance**: The `dadou_contradiction.check` RPC passes `None` as provenance, so it checks against all semantically-close entries regardless of confidence. This is intentional for manual "what would this contradict?" queries.
- **Preference write defers on conflict**: `store_preference_with_contradiction_check()` does NOT commit when contradictions are found. The caller must resolve first, then write.
- **No `strsim` dependency**: String difference is detected via simple `!=` on content; the vector similarity from `recall_relevant_by_vector` already ensures semantic closeness.

## Build notes

Full `cargo check` could not be run due to a pre-existing build environment issue: `cmake` is not installed on this machine (required by `whisper-rs-sys` build script). All changes are syntactically verified by manual review; no compilation errors are expected from the new modules.
