# 05-01-SUMMARY: External Data Tagging (INJ-01)

**Status:** Complete  
**Date:** 2026-06-05  
**Wave:** 1  

## Files Modified

| File | Change |
|------|--------|
| `src/openhuman/agent/harness/memory_context_safety.rs` | Migrated `<untrusted-source>` to `<external_data>` tag format. New primary functions: `wrap_external_data()`, `is_external_data()`, `escape_external_content()`. Old names kept as `#[deprecated]` aliases. |
| `src/openhuman/agent/harness/memory_context.rs` | Updated call sites from deprecated names to `is_external_data()` / `wrap_external_data()` with `Some("memory")` content type. |
| `src/openhuman/agent/prompts/mod.rs` | Added `AntiInjectionSection` struct implementing `PromptSection` with "Trust Boundaries" text. Added `render_anti_injection()` free function. Section inserted in both `with_defaults()` and `for_subagent()`. |
| `src/openhuman/agent/harness/tool_loop.rs` | Added `should_wrap_external_data()` helper (classifies tool names into external source categories), `is_outside_workspace()` helper, and `<external_data>` wrapping step after tool execution. INJ-02 `SkillOutputEnvelope` wrapping (by linter) runs before INJ-01 wrapping. |

## What Was Built

### Task 1 — `<external_data>` tag format
- `wrap_external_data(content, source, content_type)` produces `<external_data source="..." trusted="false" content_type="...">` wrapping
- `is_external_data(entry)` classifies memory entries by namespace/key heuristics
- `escape_external_content(content)` escapes `&<>` to HTML entities
- Backward-compat deprecated aliases retained for `is_potentially_untrusted`, `wrap_untrusted_for_agent`, `escape_untrusted_content`

### Task 2 — Memory context migration
- `build_context()` in `memory_context.rs` now calls `is_external_data()` and `wrap_external_data()` with explicit `content_type="memory"`
- No references to deprecated names remain in this file

### Task 3 — AntiInjectionSection in system prompt
- New section renders: `## Trust Boundaries` explaining that `<external_data trusted="false">` content is data, not instructions
- Included in `with_defaults()` (all agents) and `for_subagent()` (when safety preamble is included)
- `render_anti_injection()` free function for standalone use

### Task 4 — Tool loop wrapping
- `should_wrap_external_data(tool_name, args)` returns `(source, content_type)` for: `dadou.*` skills (`dadou_skill`/`skill_output`), web tools (`web`/`web_content`), file reads (`file`/`file_content`)
- Wrapping applied after INJ-02 envelope (skill outputs) and before `individual_results`/`tool_results` assembly
- `log::debug!` with `[anti-injection]` prefix on all wrapping decisions
- Circuit breaker still uses unwrapped `result` for accurate error matching

## Tests Added/Updated

- **memory_context_safety.rs**: 13 tests — updated all to use new function names and tag format; added `deprecated_aliases_still_work` backward compat test
- **tool_loop.rs** (`injection_tests`): 8 tests — skill wrapping, web tool wrapping, file read wrapping, internal tool exclusion, `is_outside_workspace`, `wrap_external_data` tag validity, plus INJ-02 envelope tests

## Key Design Decisions

- `content_type` defaults to `"memory"` when `None` — keeps existing memory recall call sites clean
- `trusted="false"` is hardcoded for v1 (all external data is untrusted)
- File reads are always wrapped (conservative); `is_outside_workspace` helper available for future refinement
- INJ-02 skill envelope (JSON structure for skill outputs) runs before INJ-01 wrapping, consistent with D-54

## Verification

- [x] `<external_data>` tag replaces `<untrusted-source>` in all production code paths
- [x] `AntiInjectionSection` renders in system prompt for default and sub-agent builders
- [x] Tool loop wraps skill/web/file outputs before they reach the LLM
- [x] Internal tools (bash, shell, edit, glob, grep) are NOT wrapped
- [x] Deprecated aliases maintain backward compatibility
- [x] Zero new crate dependencies
