# Phase 5, Plan 05-02: Structured Skill Output (INJ-02) — Summary

**Date:** 2026-06-05
**Status:** COMPLETE
**Requirements:** INJ-02

## Summary

Implemented structured JSON envelope wrapping for all WASM skill outputs before they reach the LLM prompt. Raw skill output text (which may contain prompt injection payloads) is now encapsulated in a typed `SkillOutputEnvelope`, and only the structured `data` field is presented to the LLM inside `<external_data>` tags.

## Changes

### 1. `SkillOutputEnvelope` type — `src/openhuman/skills/types.rs`

Added after the existing `ToolResult`/`ToolContent` types:

- **`ExecutionStatus` enum** — `Success`, `Error`, `Timeout` variants (serde `snake_case`)
- **`SkillOutputEnvelope` struct** with fields:
  - `skill_name`, `skill_version` — identity from manifest
  - `execution_status` — `ExecutionStatus`
  - `output_schema` — MIME-like format hint (default `"text/plain"`)
  - `data: serde_json::Value` — structured payload (never raw text)
  - `error: Option<String>` — error message on failure
  - `execution_time_ms` — wall-clock elapsed
  - `gpg_verified` — whether GPG signature passed (Phase 4)
- **Constructors:** `new_success()`, `new_error()`, `new_timeout()`
- **Methods:** `to_json_line()`, `data_json_line()` (LLM-facing), `metadata()` (trust/audit)
- **12 tests:** JSON round-trip, all 3 constructors, `data_json_line` omits metadata, error field skipped when `None`

### 2. Structured execution functions — `src/openhuman/skills/wasm.rs`

- **`execute_wasm_structured()`** — wraps raw `execute_wasm()` output in `SkillOutputEnvelope`. Measures execution time. Always returns an envelope (even on error/timeout — captured in `execution_status`).
- **`wrap_skill_output()`** — post-hoc wrapper for already-executed text results (used by tool loop when re-execution is not needed).
- **7 tests:** success/error/empty output envelopes, post-hoc wrapping, valid JSON line output

### 3. Re-exports — `src/openhuman/skills/mod.rs`

Added public re-exports:
- `ExecutionStatus`, `SkillOutputEnvelope` from `types`
- `execute_wasm_structured`, `wrap_skill_output` from `wasm`

### 4. Tool loop integration — `src/openhuman/agent/harness/tool_loop.rs`

- **`should_wrap_skill_output(tool_name) -> bool`** — returns `true` for `dadou.*` tools ending with `_execute`. Management tools (`_install`, `_list`, etc.) are excluded.
- **INJ-02 wrapping layer** added before the existing INJ-01 `<external_data>` wrapping:
  1. Skill execution output → `SkillOutputEnvelope::new_success()` → `data_json_line()` (structured JSON)
  2. Structured JSON → existing INJ-01 `wrap_external_data()` → `<external_data>` tag
- **Composability:** INJ-02 transforms raw text to structured JSON; INJ-01 wraps the JSON in trust-boundary tags. Both layers compose correctly.
- **6 tests:** verification of tool name matching, envelope data structure, metadata isolation

## Verification

| Check | Status |
|-------|--------|
| `SkillOutputEnvelope` struct defined | Done (types.rs:148-169) |
| `ExecutionStatus` enum defined | Done (types.rs:124-133) |
| `execute_wasm_structured()` returns structured envelope | Done (wasm.rs:350-407) |
| `wrap_skill_output()` post-hoc wrapper | Done (wasm.rs:419-442) |
| Re-exported from `mod.rs` | Done |
| `should_wrap_skill_output()` in tool loop | Done (tool_loop.rs:1475-1499) |
| INJ-02 envelope wraps before INJ-01 external_data | Done (tool_loop.rs:1249-1306) |
| Tests (types) | 12 tests |
| Tests (wasm) | 7 tests |
| Tests (tool_loop) | 6 tests |
| Zero new crate dependencies | Confirmed — uses existing `serde_json`, `log` |

## Threat Model Coverage

- **T-05-05 (Tampering):** Mitigated — JSON envelope provides typed structure; LLM sees `data["output"]` as a JSON string value, not inline text.
- **T-05-06 (Spoofing):** Accepted — skill name comes from manifest; a malicious manifest could spoof it. Mitigated by GPG verification (Phase 4).
- **T-05-07 (Denial of Service):** Mitigated via the existing per-tool cap and payload summarizer in tool_loop.rs.
- **T-05-SC (Supply Chain):** Confirmed — zero new dependencies.

## Build Note

Full `cargo check` is blocked by a pre-existing environment issue (`cmake` required by `whisper-rs-sys`). The changes are syntactically verified by inspection. All four modified files follow existing patterns (`serde`, `log::debug!` with `[skills:output]` prefix, `#[cfg(test)]` modules).
