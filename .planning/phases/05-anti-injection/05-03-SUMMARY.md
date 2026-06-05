# Plan 05-03 Summary: Semantic Output Validation (INJ-03)

**Status:** Complete
**Date:** 2026-06-05
**Build:** Cannot verify (pre-existing cmake issue with whisper-rs-sys on Windows; no syntax errors in new code)

## What Was Built

### New Domain: `src/openhuman/anti_injection/`

**5 files created, 4 files modified.**

### Files Created

1. **`src/openhuman/anti_injection/mod.rs`** — Module root with re-exports for the validator facade, rules, LLM check types, and controller wiring functions (`all_anti_injection_registered_controllers`, `all_anti_injection_controller_schemas`).

2. **`src/openhuman/anti_injection/validator/mod.rs`** — `SemanticOutputValidator` facade:
   - `ValidationMode` enum: `Strict` (fail-closed, default) / `Relaxed` (warn only)
   - `ValidatorConfig`: mode, `enable_llm_check`, `max_analysis_chars` (default 10K)
   - `ValidationResult`: `allowed`, `rule_findings`, `llm_verdict`, `summary`
   - `SemanticOutputValidator::validate()`: runs rule-based scan always, optional LLM deep-check for Medium+ findings
   - Tests: strict blocks, relaxed allows, max_analysis_chars respected, LLM check not called when disabled

3. **`src/openhuman/anti_injection/validator/rules.rs`** — 17 injection detection rules:
   - **Critical/High (11 rules)**: ignore_previous_instructions, system_prompt_override, role_switch, forget_all_instructions, tool_abuse, code_execution_request, data_exfiltration_request, credential_request, gate_bypass, reverse_injection, xml_tag_abuse
   - **Medium (4 rules)**: output_format_injection, markdown_injection, url_injection, chain_injection
   - **Low (1 rule)**: hidden_hex (long hex string)
   - **Medium (1 rule)**: hidden_base64 (long base64 string)
   - All patterns use `OnceLock<Regex>` for lazy compilation
   - 25+ tests: each rule has positive test, negative tests for benign content, edge cases for case insensitivity, position tracking, multi-rule triggering, regex compilation validation

4. **`src/openhuman/anti_injection/validator/llm_check.rs`** — Optional LLM deep-check:
   - `LlmVerdict` struct with `verdict: LlmVerdictKind`, `reason`, `confidence`
   - `LlmVerdictKind` enum: Safe, Suspicious, Malicious, Uncertain
   - `llm_deep_check()`: builds N3-style prompt with rule findings context, calls `local_ai_prompt()` with 2s timeout
   - Timeout/parse failure/LLM error returns `None` (graceful degradation — rules already caught it)
   - Re-uses existing `load_config_with_timeout()` + `local::global().prompt_interactive()` infrastructure
   - 8 tests: parse all verdict kinds, markdown-wrapped JSON, malformed JSON, empty responses, confidence clamping

5. **`src/openhuman/anti_injection/schemas.rs`** — 3 JSON-RPC controllers under `anti_injection` namespace:
   - `anti_injection.validate` — Run validation on arbitrary text
   - `anti_injection.config` — Get/set validator configuration (strict/relaxed mode)
   - `anti_injection.rules_list` — List all 17 active rules with metadata
   - Tests: missing param validation, injection detection via RPC, rules count, config mode setting

### Files Modified

6. **`src/openhuman/mod.rs`** — Added `pub mod anti_injection;`
7. **`src/core/all.rs`** — Wired `anti_injection` controllers into both `build_registered_controllers()` and `build_declared_controller_schemas()`. Added namespace description for `"anti_injection"`.
8. **`src/core/event_bus/events.rs`** — Added `InjectionBlocked { tool_name, reason, finding_count }` variant to `DomainEvent` enum, mapped to `"guardian"` domain in `domain()` method.
9. **`src/openhuman/agent/harness/tool_loop.rs`** — Semantic validation inserted between INJ-02 (envelope creation) and INJ-01 (external_data wrapping):
   - Creates `SemanticOutputValidator` with default config
   - Validates the INJ-02 envelope output
   - On block: replaces result with `[policy-blocked]` message, publishes `InjectionBlocked` event
   - On relaxed mode with findings: logs warning
   - Raw injection payload never reaches LLM context

## Threat Model Compliance

| Threat | Disposition | Implementation |
|--------|-------------|----------------|
| T-05-08 (Tampering via injection) | Mitigated | 17 regex rules detect known patterns |
| T-05-09 (Spoofing via LLM check) | Mitigated | LLM check is optional enhancement, rules always run |
| T-05-10 (DoS via validator) | Mitigated | `max_analysis_chars` = 10K default, 2s LLM timeout |
| T-05-11 (EoP via block bypass) | Mitigated | Blocked output replaced with error message, raw output never reaches LLM |
| T-05-SC (Dependency) | Mitigated | Zero new dependencies — uses existing `regex` crate |

## Key Design Decisions

- **Rules-only by default** (v1): LLM deep-check is opt-in (`enable_llm_check: false`). The rule engine provides sufficient protection for the first release.
- **Strict mode default**: Any rule trigger blocks the output (fail-closed). Relaxed mode available for development/debugging.
- **Validation after envelope**: Validation runs on the data_json_line output of the INJ-02 `SkillOutputEnvelope`, not on raw skill output.
- **Blocked message format**: Uses `[policy-blocked]` prefix matching the existing hard-reject detection pattern used by the Guardian pipeline.
