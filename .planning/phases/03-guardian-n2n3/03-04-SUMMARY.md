# Plan 03-04 Summary: Config Schema + N2/N3 Controllers + Initialization

**Status**: Complete
**Wave**: 2 of Phase 3

## What was built

### Task 1: Config schema — Guardian N2 + N3 sections
- **`src/openhuman/config/schema/types.rs`**:
  - Added `GuardianN2Config` struct with `enabled`, `block_threshold`, `escalate_threshold`, `max_input_chars`
  - Added `GuardianN3Config` struct with `enabled`, `max_tokens`, `timeout_ms`, `cache_size`, `model_override`
  - Both with `#[serde(default)]` and `impl Default` matching D-41 defaults
  - Both fields added to the `Config` struct and `Config::default()` method

- **`src/openhuman/guardian/n2/types.rs`**:
  - Added `impl From<GuardianN2Config> for N2EngineConfig` — maps all 3 fields

- **`src/openhuman/guardian/n3/types.rs`**:
  - Added `impl From<GuardianN3Config> for N3Config` — maps all 5 fields

### Task 2: Controllers for N2 and N3
- **`src/openhuman/guardian/schemas.rs`**:
  - Extended `all_controller_schemas()` with 3 new schemas: `n2_evaluate`, `n3_status`, `pipeline_status`
  - Extended `all_registered_controllers()` with 3 new handlers
  - All under existing `guardian` namespace (consistent with N1 controllers)
  - 6 total controllers now (3 N1 + 3 N2/N3)

- **`src/openhuman/guardian/ops.rs`**:
  - Added `n2_evaluate()` — creates GuardianN2 with defaults, runs evaluation, returns N2Result
  - Added `n3_status()` — creates GuardianN3 with defaults, returns config/cache stats
  - Added `pipeline_status()` — aggregates N1 rules + N2/N3 config snapshot

- **`src/openhuman/guardian/n3/mod.rs`**:
  - Added `config()` accessor method to GuardianN3

### Task 3: Initialization wiring + documentation
- No changes to `src/core/all.rs` needed — new controllers are automatically wired via the existing `all_guardian_registered_controllers()` call
- **`src/openhuman/guardian/mod.rs`**:
  - Added initialization order documentation with code example for N1+N2+N3 pipeline startup
  - Documents fail-closed behavior when N2 or N3 is disabled

## RPC methods added
| Method | Description |
|--------|-------------|
| `openhuman.guardian_n2_evaluate` | Run N2 classifier on tool invocation (debugging) |
| `openhuman.guardian_n3_status` | Get N3 validator status and config |
| `openhuman.guardian_pipeline_status` | Get full pipeline status (N1+N2+N3) |

## Files modified
- `src/openhuman/config/schema/types.rs` — GuardianN2Config, GuardianN3Config structs + Config fields
- `src/openhuman/guardian/n2/types.rs` — From<GuardianN2Config> conversion
- `src/openhuman/guardian/n3/types.rs` — From<GuardianN3Config> conversion
- `src/openhuman/guardian/n3/mod.rs` — config() accessor method
- `src/openhuman/guardian/schemas.rs` — 3 new controller schemas + handlers
- `src/openhuman/guardian/ops.rs` — 3 new operation functions
- `src/openhuman/guardian/mod.rs` — initialization order documentation

## Verification needed
- `cargo check -p openhuman` — compilation check (whisper-rs pre-existing errors expected)
- 6 guardian controllers should be accessible via RPC
- Config deserializes with `guardian_n2` and `guardian_n3` sections
