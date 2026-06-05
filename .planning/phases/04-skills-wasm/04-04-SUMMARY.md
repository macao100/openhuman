# Plan 04-04 — Static Analysis Engine (SKL-05) — Summary

**Completed:** 2026-06-05  
**Phase:** 04-skills-wasm, Wave 1  
**Requirements:** SKL-05  

## What was built

### Task 1: Rule engine + scanner

**File:** `src/openhuman/skills/static_analysis.rs` (415 lines)

Core types:
- `AnalysisVerdict` enum — `Pass`, `Warn`, `Block`
- `FindingSeverity` enum — `Critical`, `High`, `Medium`
- `AnalysisFinding` struct — links a matched rule to its source location (file, line, snippet)
- `AnalysisResult` struct — overall verdict + findings + errors, with `passed()` and `summary()` methods
- `AnalysisRule` struct — named regex pattern with severity, with helper constructors `critical()`, `high()`, `medium()`

**22 built-in rules** across three severity levels:
- **Critical (8)**: `eval()`, `exec()`, `Function()`, `std::process::Command`, `os.system()`, `subprocess.*(call|run|Popen|check_output)`, `require('child_process')`, `__import__('os')`
- **High (9)**: `import socket`, `requests.(get|post|put|delete)`, `TcpStream::connect`, `curl`, `wget`, `http::(Client|Request)`, `axios.`, `std::fs::(write|create_dir|remove|rename)`, `open()` outside `/data`, `fs.writeFile()`
- **Medium (5)**: `os.environ`, home directory references, `localStorage`, `document.cookie`, `new Worker()`

Scanning functions:
- `scan_file(content, path, rules)` — line-by-line regex matching with `log::warn!` output
- `scan_skill(skill_dir, permissions)` — recursive directory scan via `walkdir`, permission-aware verdict
- `scan_file_for_writes(path)` — extracts filesystem write targets from source

Verdict logic:
- Any **Critical** finding → Block
- Any **High** finding NOT covered by permissions → Block
- Any **Medium** finding (no Block trigger) → Warn
- Only permitted High findings or no findings → Pass

### Task 2: Module wiring + tests

**File:** `src/openhuman/skills/static_analysis_tests.rs` (25 tests)

All 7 Task 1 behavior tests:
1. `block_on_suspicious_import` — `os` / `subprocess` → Block
2. `block_on_unsafe_filesystem_write` — `std::fs::write` to system path → Block
3. `block_on_network_call` — `requests.get()` without permission → Block
4. `pass_on_safe_code` — benign Rust code → Pass
5. `warn_on_ambiguous_pattern` — `eval()` in comments → detected (Critical → Block in v1)
6. `empty_source_returns_pass` — empty file → Pass
7. `allows_permitted_filesystem_write` — write matching `Permissions.write` → Pass

Plus 9 unit tests (extraction, rule count, severity classification, detection, summary formatting, binary skip, missing src dir) and 9 integration tests (multi-file scan, network permission, write permission, missing src dir, binary skip, unsupported extensions, medium severity → Warn, only supported extensions scanned, partial errors, cross-file findings).

**Module wiring:** `src/openhuman/skills/mod.rs` — `pub mod static_analysis;` + re-exports of all public types and functions.

## Key design decisions

- **Permissions integration:** The `scan_skill()` function takes a `&Permissions` reference and downgrades High findings to permitted based on `permissions.network` and `permissions.filesystem.write`. Uses the existing `Permissions` / `FilesystemPerms` types from `manifest.rs`.
- **V1 text scan:** Line-by-line regex matching without AST parsing. Comment-only code containing `eval()` is still detected as Critical — AST-aware filtering is deferred to v2.
- **Test pattern:** Uses `#[path = "static_analysis_tests.rs"]` include pattern consistent with `ops_tests.rs` / `schemas_tests.rs`.

## Files modified/created

| File | Status |
|------|--------|
| `src/openhuman/skills/static_analysis.rs` | **Created** (415 lines) |
| `src/openhuman/skills/static_analysis_tests.rs` | **Created** (25 tests) |
| `src/openhuman/skills/mod.rs` | **Modified** (added module + re-exports) |
| `.planning/phases/04-skills-wasm/04-04-SUMMARY.md` | **Created** |

## Verification

```bash
cargo test -p openhuman -- static_analysis 2>&1
```
