# Phase 05-04 Summary: JSON Plan Validation (INJ-04)

## Objective
Require the LLM to emit structured JSON action plans, then validate the complete plan through the Guardian pipeline before any step executes.

## Files Modified

### Task 1: StructuredPlan types
- **`src/openhuman/guardian/types.rs`** — Added `StructuredPlan` (goal + steps), `PlanStep` (tool + args + rationale), `PlanValidationResult` (allowed, blocked_by, reasoning, rejected_steps, step_results) with serde derives and unit tests (roundtrip, empty plan, max steps, rejection states).
- **`src/openhuman/guardian/mod.rs`** — Exported `StructuredPlan`, `PlanStep`, `PlanValidationResult`.

### Task 2: GuardianPipeline::evaluate_plan()
- **`src/openhuman/guardian/pipeline.rs`** — Added `evaluate_plan()` to `GuardianPipeline`:
  - Stage 1: Structure check (empty goal, max 20 steps, empty tool name)
  - Stage 2: Step-by-step N1→N2→N3 validation for each step
  - Stage 3: Final decision with `blocked_by` and `rejected_steps`
  - Publishes `PlanValidated` event after validation
  - Tests: empty goal, empty steps, exceeds max steps, empty tool name, safe plan, blocked step, rejected step indices

### Task 3: ExecutionProtocolSection
- **`src/openhuman/agent/prompts/mod.rs`** — Added `ExecutionProtocolSection`:
  - Instructs LLM to emit structured JSON plans before multi-step actions
  - Explains plan format (`{"plan": {"goal": ..., "steps": [...]}}`)
  - States constraints: 20 steps max, clear rationale required, single-step exceptions allowed
  - Added to `with_defaults()` between ToolsSection and SafetySection
  - Added to `for_subagent()` when safety preamble is included
  - Added `render_execution_protocol()` free function

### Task 4: Events, N3 prompt, and controller
- **`src/core/event_bus/events.rs`** — Added `PlanValidated` DomainEvent variant (goal, allowed, blocked_by, step_count, rejected_step_indices); registered in `guardian` domain match.
- **`src/openhuman/guardian/n3/prompt.rs`** — Added `plan_intent_system_prompt()` and `plan_intent_user_prompt()` for plan-level LLM validation.
- **`src/openhuman/guardian/schemas.rs`** — Added `guardian.plan_validate` controller (input: plan JSON, output: PlanValidationResult).
- **`src/openhuman/guardian/ops.rs`** — Added `validate_plan()` operation function.

### Task 5: Plan interception in tool loop
- **`src/openhuman/agent/harness/tool_loop.rs`**:
  - Added `extract_structured_plan()` helper (JSON extraction via serde + regex for code blocks)
  - Added plan validation interception before tool execution loop: if plan found, validate through GuardianPipeline, reject → push rejection to history and skip tool execution, accept → proceed with per-tool validation
  - Uses `[guardian:plan]` / `[agent_loop]` logging prefixes
- **`src/openhuman/agent/harness/tool_loop_tests.rs`** — Added 7 unit tests for plan extraction (direct JSON, wrapped JSON, code block, code block without lang tag, non-plan text, malformed JSON, empty string).

## Architecture
```
LLM emits JSON plan → extract_structured_plan() → GuardianPipeline::evaluate_plan()
  → Structure check (goal, max 20 steps, tool names)
  → Per-step N1→N2→N3 pipeline validation
  → PlanValidated event published
  → Approved: proceed with per-tool Guardian validation (defense in depth)
  → Rejected: rejection message pushed to history, LLM revises
```

## Key Design Decisions
1. **Defense in depth**: Plan validation is a pre-filter — per-tool Guardian N1→N2→N3 still runs even after plan approval (T-05-14).
2. **Fail-closed**: Rejected plans cancel ALL tool execution, not just blocked steps.
3. **V1 scope**: Plan-level N3 intent validation (plan_intent_user_prompt) is wired for future use but not auto-triggered — per-step pipeline is sufficient for V1.
4. **Zero new crate dependencies**: Uses existing `serde_json`, `regex`, and `uuid`.
5. **Simplified types**: Uses `goal: String` + `Vec<PlanStep>` (not the richer `plan_id`, `resources`, `risk_level` from the spec) — these can be added in V2 when the LLM reliably emits them.

## Verification
- `StructuredPlan` serializes/deserializes from JSON roundtrip
- `evaluate_plan()` validates structure, caps steps at 20, runs each step through pipeline
- `ExecutionProtocolSection` renders in system prompt with correct format
- `extract_structured_plan()` extracts plans from direct JSON, wrapped `{"plan": ...}`, and code blocks
- Rejected plans cancel tool execution and ask LLM to revise
- `PlanValidated` events are published on validation
- `guardian.plan_validate` controller available for external testing
