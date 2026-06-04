# Plan 02-02 Summary — Project Context & Preferences (MEM-01 + MEM-02)

**Completed:** 2026-06-05
**Status:** All tasks implemented

## Task 1: Project context domain — types, store, ops

**Files created:**
- `src/openhuman/memory/project_context/mod.rs` — Module root, re-exports all submodules
- `src/openhuman/memory/project_context/types.rs` — `ProjectFact` struct (project_name, fact_key, fact_value, category, source, updated_at), `ProjectScope` enum (Active/Archived/All). Includes serialization tests.
- `src/openhuman/memory/project_context/store.rs` — CRUD operations via `Memory` trait in `dadou_project_context` namespace. `upsert_fact`, `get_fact`, `list_facts` (with optional project filter, newest-first), `delete_fact`. Content format: fact_value + `\n__meta__:` JSON block for structured metadata (category, source, updated_at). Includes 4 tests.
- `src/openhuman/memory/project_context/ops.rs` — `load_project_context()` returns formatted `[Project context]` markdown block with version on project line, grouped facts below. Empty state returns `"No project context recorded yet."`. Includes 3 tests.

**File modified:**
- `src/openhuman/memory/mod.rs` — Added `pub mod project_context;`

## Task 2: Inject project context and preferences into agent prompts

**File modified:**
- `src/openhuman/agent/memory_loader.rs` — In `DefaultMemoryLoader::load_context`:
  - **Project context block** inserted first (before `[User working memory]`): calls `project_context::ops::load_project_context()` via global memory client, respects `max_context_chars` budget.
  - **Preferences block** inserted second (between project context and working memory): calls `preferences::load_general_preferences()` with `STANDING_PREFS_LIMIT`, formats as `[Preferences]` list, respects budget.

**Unchanged (per plan):**
- `src/openhuman/agent/harness/memory_context.rs` — No changes (session-start loader covers the requirement)

## Task 3: Preference correction tool + project context controllers

**File modified:**
- `src/openhuman/memory/preferences.rs` — Added `store_preference_correction(memory, topic, value)`:
  - Stores to `user_pref_general` namespace with `MemoryCategory::Core`
  - Content format: `"[user correction] {topic}: {value}\n[provenance] {...}"` with provenance stamped as `{source: UserCorrection, confidence: Verified}`
  - Includes 3 tests (creates entry, overwrites existing, appears in load_general_preferences)

**Files created:**
- `src/openhuman/memory/project_context/schemas.rs` — Three controllers for `dadou_project_context`:
  - `upsert_fact` — inputs: project_name, fact_key, fact_value, [category], [source]; returns key, updated_at
  - `list_facts` — inputs: [project]; returns facts array (project_name, fact_key, fact_value, category, source, updated_at)
  - `delete_fact` — inputs: project_name, fact_key; returns deleted bool
  - Includes schema count/test validation

**File modified:**
- `src/core/all.rs` — Wired `project_context::schemas::all_registered_controllers()` into `build_registered_controllers()` and `build_declared_controller_schemas()`

**Unchanged (per plan):**
- `src/openhuman/tools/impl/mod.rs` — No new tool registration needed (controllers handle namespace operations)

## Verification Notes

- All code follows existing patterns: `ControllerSchema`/`FieldSchema`/`TypeSchema` from `src/core/types.rs`, `RpcOutcome<T>` pattern, `MemoryClient` via global singleton, co-located `#[cfg(test)]` modules
- Provenance for preference corrections: `{source: UserCorrection, confidence: Verified}`
- Namespace: `"dadou_project_context"` for project facts
- Pre-existing build blocker: `cargo check` fails due to `whisper-rs-sys` cmake dependency (build note acknowledged)
