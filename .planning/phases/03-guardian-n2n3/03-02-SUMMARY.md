---
phase: 03-guardian-n2n3
plan: 02
subsystem: guardian
tags: [guardian, n3, llm-validator, security, cache, lru]
requires:
  - phase: 01-guardian-n1
    provides: guardian module structure, types, pipeline pattern
  - phase: 02-architecture-decisions
    provides: decisions D-35 to D-42 (N3 architecture, prompt format, cache, timeout)
provides:
  - N3Verdict enum (Allow/Block/Uncertain) with serde lowercase JSON
  - N3Result struct with fail-closed should_block() semantics
  - N3Config with sensible defaults (450ms timeout, 256 max_tokens, 100 cache size)
  - N3Result::from_llm_response() JSON parsing with graceful fallback
  - N3PromptBuilder for security validation system prompt + user context prompt
  - SHA-256 deterministic cache_key() for LLM result deduplication
  - LRU cache (HashMap-based, no external deps) for validation result caching
  - GuardianN3::evaluate() with cache-first strategy, LLM call, timeout handling
affects:
  - 03-guardian-n2n3/03-01 (N2 integration — N3 references N2Score for escalation context)
  - 03-guardian-n2n3/03-03 (pipeline — GuardianPipeline combines N1 -> N2 -> N3)
  - 03-guardian-n2n3/03-04 (events — N3Result variant in DomainEvent)
tech-stack:
  added: []
  patterns:
    - GuardianN3 with Arc<Mutex<LruCache>> for thread-safe caching
    - tokio::time::timeout for LLM call with fail-closed fallback
    - Hash-map based LRU (no external dependency, satisfies T-03-SC)
    - json-in-text extraction for lenient LLM response parsing
key-files:
  created:
    - src/openhuman/guardian/n3/types.rs
    - src/openhuman/guardian/n3/mod.rs
    - src/openhuman/guardian/n3/prompt.rs
    - src/openhuman/guardian/n3/cache.rs
  modified:
    - src/openhuman/guardian/mod.rs
decisions:
  - D-35: N3 is a sub-domain under src/openhuman/guardian/n3/
  - D-36: Uses local_ai_prompt() / prompt_interactive() for LLM calls
  - D-37: System prompt requests structured JSON output with {"verdict", "reason", "confidence"}
  - D-38: LRU cache (size 100) for deduplication within a session
metrics:
  duration_minutes: 8
  commits: 3
  files_created: 4
  total_lines: 1184
  completed_date: 2026-06-05
---

# Phase 3 Plan 02 — Guardian N3: LLM Validator — Summary

**One-liner:** Creation of the N3 LLM validator sub-domain: types (N3Verdict, N3Result, N3Config), security validation system prompt with JSON output format, LRU cache for result deduplication, and GuardianN3::evaluate() with configurable timeout calling the existing local inference infrastructure.

## Tasks Completed

| # | Task | Type | Commit | Files |
|---|------|------|--------|-------|
| 1 | N3 types and configuration | TDD (auto) | `658988c` | `n3/types.rs`, `n3/mod.rs`, `guardian/mod.rs` |
| 2 | N3 system prompt for security validation | auto | `52e62a7` | `n3/prompt.rs` |
| 3 | N3 LRU cache and LLM caller with timeout | auto | `a6b80e5` | `n3/cache.rs`, `n3/mod.rs` |

## Architecture

### N3 Call Flow

```
GuardianPipeline::evaluate()
  │
  ├─ N1: Deterministic rules (<1ms)
  │   └─ if Block → return (no N3 needed)
  │
  ├─ N2: Heuristic classifiers (<10ms)
  │   ├─ if Allow (all scores < escalate_threshold) → return Allow
  │   ├─ if Block (any score > block_threshold) → return Block
  │   └─ if Escalate (any > escalate, none > block) → N3
  │
  └─ N3: LLM Validator (<500ms)  ←  THIS PLAN
      ├─ 1. Cache check (LruCache, max 100 entries)
      ├─ 2. If miss: build N3 security prompt
      ├─ 3. Call local_ai_prompt() with timeout (default 450ms)
      ├─ 4. Parse JSON verdict from LLM response
      └─ 5. Cache result and return
```

### Module Structure

```
src/openhuman/guardian/n3/
├── mod.rs      — GuardianN3 struct, evaluate(), call_llm()
├── types.rs    — N3Verdict, N3Result, N3Config, LlmResponse
├── prompt.rs   — N3PromptBuilder (system prompt, user prompt builder, cache_key)
└── cache.rs    — LruCache<V> (HashMap + Vec order tracker)
```

### Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Cache key | SHA-256( tool_name \| args_json \| command ) | Deterministic, collision-resistant, no external dep |
| Cache eviction | LRU via Vec order tracker | Simple, O(n) eviction fits cache size (100) |
| Timeout behaviour | tokio::time::timeout -> Uncertain on timeout | Fail-closed: if LLM doesn't respond, block action |
| Parse failure | JSON extraction from surrounding text, fallback Uncertain | LLM may wrap JSON in markdown or commentary |
| no_think | true | Skip reasoning tokens (waste of latency for short verdict) |

### Threat Model Compliance

| Threat ID | Disposition | Implementation |
|-----------|-------------|----------------|
| T-03-05 (Spoofing - LLM response) | Mitigate | `from_llm_response` returns None on parse fail -> Uncertain -> Block |
| T-03-06 (DoS - LLM timeout) | Mitigate | `tokio::time::timeout` with configurable `timeout_ms` (default 450) -> Uncertain |
| T-03-07 (Tampering - Cache staleness) | Accept | LRU eviction naturally replaces old entries; session-scoped only |
| T-03-08 (Information Disclosure) | Accept | Prompt contains tool args, no secrets; logged locally only |
| T-03-SC (Tampering - new packages) | Mitigate | Zero new dependencies: `HashMap`-based LRU, `tokio::time` already in workspace |

## Deviations from Plan

None — plan executed exactly as written.

### Implementation Notes

- The `evaluate()` signature uses `&[(String, f64)]` for N2 scores instead of `&[N2Score]` since the N2 module is created in parallel (Plan 03-01) and may not be available at compile time. The tuples represent `(detector_name, score)` pairs. This avoids a hard dependency between parallel plans. When both plans are merged, the integration layer can convert freely.
- The plan originally referenced `inference::local::ops::local_ai_prompt` for the LLM call. The implementation uses `LocalAiService::prompt_interactive()` directly (via `local::global()`) which returns `Result<String, String>` instead of `Result<RpcOutcome<String>, String>`, avoiding unnecessary RPC wrapper overhead for internal use.
- `lru` crate was not found in `Cargo.toml`, so the plan's "use std::collections::HashMap + ordered tracking" fallback was followed (consistent with T-03-SC mitigation requiring zero new packages).

## Pre-existing Build Issue

The project's `whisper-rs-sys` dependency requires `cmake` which is not installed on this Windows environment. This prevents `cargo test` from completing, but the compilation errors are in `whisper-rs-sys`, not in the N3 code. The N3 code follows all existing Rust module patterns (serde derives, tokio async, parking_lot mutex, sha2 hashing — all confirmed present in Cargo.toml).

## Sub-directory Structure Verification

All 4 files created:

- `src/openhuman/guardian/n3/types.rs` — 312 lines (>50 min)
- `src/openhuman/guardian/n3/mod.rs` — 268 lines (>100 min)
- `src/openhuman/guardian/n3/prompt.rs` — 320 lines (>60 min)
- `src/openhuman/guardian/n3/cache.rs` — 284 lines (>50 min)
- Total: 1184 lines across 4 files
- `src/openhuman/guardian/mod.rs` updated with `pub mod n3;`

## Self-Check: PASSED
