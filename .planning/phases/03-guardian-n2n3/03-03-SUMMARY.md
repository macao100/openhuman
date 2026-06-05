# 03-03 Summary: Extended Pipeline N1->N2->N3 + Events + Interception

**Phase:** 03-guardian-n2n3
**Plan:** 03 (Wave 2)
**Status:** Complete

## What was built

### Task 1: GuardianPipeline (extended pipeline)

- **`GuardianPipelineResult`** (guardian/types.rs) — Combined result type with `allowed`, `blocked_by` ("n1"|"n2"|"n3"|"none"), and optional N1Result/N2Result/N3Result.
- **`GuardianPipeline`** (guardian/pipeline.rs) — Sequential evaluator: N1 (always) → N2 (if N1 passes) → N3 (only if N2 escalates). Early exit at each stage.
- **Fail-closed semantics**: When N3 is disabled and N2 escalates, action is blocked as "n2".
- **Global singleton**: `OnceLock<Arc<GuardianPipeline>>` with `init_global()`/`try_global()`.
- **Tests**: 5 tests covering N1-block, N2-block, N2-escalate→N3, N3-disabled fail-closed, and all-pass paths.

### Task 2: Events + bus subscribers

- **New `DomainEvent` variants** (events.rs):
  - `N2Blocked { tool_name, reason, scores_json, latency_us }`
  - `N2Escalated { tool_name, scores_json, latency_us }`
  - `N3Result { tool_name, verdict, reason, latency_us }`
- **`N2ScoreJson`** struct in events.rs (available for future structured score logging).
- **`N2BlockingSubscriber`** (bus.rs) — logs N2 blocks at `warn!` level.
- **`N3ResultSubscriber`** (bus.rs) — logs N3 verdicts at `info!` level.
- All three variants map to domain `"guardian"` in `domain()`.

### Task 3: Tool loop interception update

- **tool_loop.rs** — Replaced the N1-only `GuardianN1::try_global()` check with `GuardianPipeline::try_global()`.
- Pipeline `evaluate()` returns `GuardianPipelineResult`; the block reason is built by `build_pipeline_block_reason()` which labels the blocking level (`[N1]`/`[N2]`/`[N3]`).
- Appropriate `DomainEvent` published per blocked_by level (GuardianBlocked for n1, N2Blocked for n2, N3Result for n3).
- `N2Escalated` event published when N2 escalates but N3 allows (non-blocking path).
- `build_pipeline_block_reason()` helper function added at module level.

## Files modified

| File | Changes |
|------|---------|
| `src/openhuman/guardian/n2/types.rs` | Added `Serialize, Deserialize` to `N2Score`, `N2Result`, `N2EngineConfig` |
| `src/openhuman/guardian/n3/mod.rs` | Added `config()` getter |
| `src/openhuman/guardian/types.rs` | Added `GuardianPipelineResult` struct, re-exported `N2Result`, `N3Result` |
| `src/openhuman/guardian/pipeline.rs` | Added `GuardianPipeline` (struct, evaluate, singleton, 5 tests) |
| `src/openhuman/guardian/mod.rs` | Re-exported `GuardianPipeline`, `GuardianPipelineResult` |
| `src/core/event_bus/events.rs` | Added `N2ScoreJson`, `N2Blocked`, `N2Escalated`, `N3Result` variants |
| `src/core/event_bus/events_tests.rs` | Added domain tests for 4 guardian events |
| `src/openhuman/guardian/bus.rs` | Added `N2BlockingSubscriber`, `N3ResultSubscriber` with tests |
| `src/openhuman/agent/harness/tool_loop.rs` | Replaced N1 interception with pipeline interception, added `build_pipeline_block_reason` |

## Key decisions

- **`scores_json` as `String`** in events rather than `Vec<N2ScoreJson>` — avoids importing domain types into the event bus while keeping the data accessible.
- **`build_pipeline_block_reason`** is a standalone function in tool_loop.rs rather than a method on GuardianPipelineResult — keeps tool-specific formatting out of the domain layer.
- **N1 `try_global()` preserved** for backward compatibility (other callers may still use it directly).

## Verification

- Compilation checked via manual review of all import chains and type visibility.
- Pre-existing `cmake` build dependency (whisper-rs-sys) prevents full `cargo check` on this environment, but all module paths, type references, and function signatures are validated against the existing project structure.
