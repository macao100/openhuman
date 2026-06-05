# Roadmap — DADOU v1

**Generated:** 2026-06-05
**Granularity:** fine (7 phases)
**Coverage:** 25/25 v1 requirements mapped

---

## Phases

- [ ] **Phase 1: Security Foundation** — Guardian N1 deterministic rules, Windows sandbox fix, file rollback/undo
- [ ] **Phase 2: Memory & Continuity** — Provenance, confidence, contradictions, cross-session context persistence
- [ ] **Phase 3: Guardian N2+N3** — Classifier for exfiltration patterns, lightweight LLM validation for ambiguous plans
- [ ] **Phase 4: Skills WASM** — Manifest, wasmtime sandbox, GPG verification, static analysis, CLI, store
- [ ] **Phase 5: Anti-Injection** — External data tagging, structured skill output, semantic validation, plan verification
- [ ] **Phase 6: Dashboard & Semantic Router** — Local observability dashboard, embedding-based skill discovery
- [ ] **Phase 7: Python Skills** — Docker sidecar sandbox for complex Python skill execution

---

## Phase Details

### Phase 1: Security Foundation

**Goal**: DADOU validates every action deterministically, sandboxes jailed processes on Windows, and never loses file history.

**Depends on**: Nothing (builds on OpenHuman v0.56.0 base)

**Requirements**: GRD-01, GRD-04, UND-01, UND-02

**Success Criteria** (what must be TRUE):
1. Guardian N1 applies path whitelist, regex patterns, and blocklist rules to classify and accept/reject tool actions in <1ms
2. Windows sandbox executes jailed processes successfully (Restricted Tokens + Integrity Levels, AppContainer fallback, NoopBackend fail-closed)
3. Every file modification produces a timestamped diff entry before the write executes
4. User can undo the last file change with `dadou undo --last` and see the diff
5. User can rollback files to any prior state with `dadou undo --before <timestamp>`

**Plans**: 6 plans in 2 waves

Plans:
- [ ] 01-01-PLAN.md — Guardian N1 domain: types, rules engine, YAML loader, pipeline wrapper, controllers (GRD-01)
- [ ] 01-02-PLAN.md — Windows sandbox: Restricted Tokens + Integrity Levels backend, AppContainer fix, NoopBackend fail-closed (GRD-04)
- [ ] 01-03-PLAN.md — Rollback foundation: SQLite schema, diff capture, history storage, stubs controllers (UND-01)
- [ ] 01-04-PLAN.md — Guardian N1 interception: tool pipeline integration, events, controller registration (GRD-01 wire)
- [ ] 01-05-PLAN.md — Rollback interception: file_write.rs, edit_file.rs, apply_patch.rs hooks + validate_path audit (UND-01 wire)
- [ ] 01-06-PLAN.md — CLI undo: undo_last, undo_before business logic, handlers, CLI integration (UND-02)

### Phase 2: Memory & Continuity

**Goal**: DADOU builds a persistent mental model of the user's world — projects, preferences, corrections — and resumes it across restarts.

**Depends on**: Phase 1 (rollback protects memory store, N1 provides policy context for memory-related actions)

**Requirements**: MEM-01, MEM-02, MEM-03, MEM-04, CTX-01, CTX-02

**Success Criteria** (what must be TRUE):
1. DADOU references the full project context (not just the current file or directory) when planning multi-step actions
2. User corrections and preference choices are retained across restarts — DADOU doesn't relearn the same settings
3. When DADOU encounters information that contradicts a verified memory, it prompts the user for confirmation
4. Every memory item displays its source (which action produced it) and confidence level (verified > inferred > external), with decay removing low-confidence items over time
5. On restart, DADOU resumes the previous conversation context and knows which project/phase was active

**Plans**: 4 plans in 3 waves

Plans:
- [ ] 02-01-PLAN.md — Provenance & Confidence: Provenance enum, MemoryEntry extension, SQLite migration, decay scheduler (MEM-04)
- [ ] 02-02-PLAN.md — Project Context & Preferences: project_context domain, agent prompt injection, correction tool, controllers (MEM-01, MEM-02)
- [ ] 02-03-PLAN.md — Contradiction Detection: detection engine, event bus integration, resolver, preference write guard (MEM-03)
- [ ] 02-04-PLAN.md — Cross-Session Continuity: session state store, shutdown/startup hooks, periodic saves, context restoration (CTX-01, CTX-02)

### Phase 3: Guardian N2+N3

**Goal**: DADOU detects dangerous output patterns with a local classifier and escalates ambiguous plans to a lightweight LLM validator.

**Depends on**: Phase 1 (N1 rules provide the base pipeline that N2+N3 extend)

**Requirements**: GRD-02, GRD-03

**Success Criteria** (what must be TRUE):
1. Guardian N2 detects exfiltration patterns, hidden payloads, and entropy anomalies in tool outputs in <10ms
2. Guardian N3 validates ambiguous action plans using a lightweight LLM, with escalation latency <500ms
3. The full pipeline (N1 -> N2 -> N3) processes actions end-to-end: N1 passes ~80% at <1ms, N2 catches ~15% at <10ms, N3 validates the remaining ~2% at <500ms
4. Actions that pass all three levels execute without user intervention; blocked actions show which guardian level rejected them

**Plans**: 4 plans in 2 waves

Plans:
- [ ] 03-01-PLAN.md — Guardian N2: types, detection engines (exfiltration, entropy, hidden payloads) (GRD-02)
- [ ] 03-02-PLAN.md — Guardian N3: LLM validator wrapper, system prompt, LRU cache (GRD-03)
- [ ] 03-03-PLAN.md — Extended pipeline: N1->N2->N3 integration, events, bus subscribers, tool loop wiring (GRD-02, GRD-03)
- [ ] 03-04-PLAN.md — N2/N3 controllers, config schema, initialization wiring (GRD-02, GRD-03)

### Phase 4: Skills WASM

**Goal**: DADOU installs, verifies, and executes third-party skills as sandboxed WASM modules with full lifecycle management.

**Depends on**: Phase 1 (AppContainer sandbox for process isolation, rollback for skill store safety)

**Requirements**: SKL-01, SKL-02, SKL-04, SKL-05, SKL-06, SKL-07

**Success Criteria** (what must be TRUE):
1. User installs a skill from a Git repository — DADOU reads its `dadou-skill.yaml` manifest (name, version, author, permissions, dependencies)
2. DADOU verifies the skill's GPG signature against trusted authors before activation
3. Static analysis blocks skill activation if it detects suspicious imports (os, subprocess, socket, eval, requests) or writes outside allowed paths
4. WASM skills execute in a wasmtime sandbox with capability-gated WASI (no network, restricted filesystem, 30s timeout)
5. User lists installed skills, checks their enabled/disabled state, and removes them via CLI
6. The local TOML store tracks every skill (version, commit hash, activation state, signature fingerprint)

**Plans**: 5 plans in 2 waves

Plans:
- [ ] 04-01-PLAN.md — Manifest parsing (dadou-skill.yaml) + TOML skills store (SKL-01, SKL-06)
- [ ] 04-02-PLAN.md — Wasmtime in-process runtime + WASI capability-gated sandbox (SKL-02)
- [ ] 04-03-PLAN.md — GPG signature verification via sequoia-openpgp + trust store (SKL-04)
- [ ] 04-04-PLAN.md — Static analysis: suspicious imports, filesystem writes, network detection (SKL-05)
- [ ] 04-05-PLAN.md — CLI dadou skill commands + JSON-RPC controllers (SKL-07)

### Phase 5: Anti-Injection

**Goal**: External data and skill outputs cannot manipulate DADOU through prompt injection.

**Depends on**: Phase 2 (structured memory data provides the content that needs tagging), Phase 4 (skill outputs are the primary injection vector)

**Requirements**: INJ-01, INJ-02, INJ-03, INJ-04

**Success Criteria** (what must be TRUE):
1. All external data (web content, file contents, skill outputs) is wrapped in `<external_data source="..." trusted="false">` in the system prompt
2. Skill content is passed as structured JSON fields, never concatenated raw into the LLM prompt
3. Every skill response undergoes semantic validation (second model or rule-based) before being re-injected into the conversation
4. The LLM emits structured JSON plans — the Guardian validates these plans against policy before any tool execution begins

**Plans**: 4 plans in 2 waves

Plans:
- [ ] 05-01-PLAN.md — External data tagging `<external_data>` + AntiInjectionSection system prompt (INJ-01)
- [ ] 05-02-PLAN.md — Structured JSON output envelope for skill results, never raw concat (INJ-02)
- [ ] 05-03-PLAN.md — Semantic output validation: 15+ injection pattern rules + optional LLM check (INJ-03)
- [ ] 05-04-PLAN.md — JSON plan validation by Guardian pipeline: StructuredPlan, evaluate_plan, ExecutionProtocolSection (INJ-04)

### Phase 6: Dashboard & Semantic Router

**Goal**: DADOU provides real-time observability and routes user queries to the right skills.

**Depends on**: Phase 4 (skills must exist to be discovered and displayed)

**Requirements**: OBS-01, RTR-01

**Success Criteria** (what must be TRUE):
1. User opens `localhost:7790` in a browser and sees: active skills, Guardian decisions, memory graph, and action history
2. The dashboard updates in real-time via SSE without page refresh
3. DADOU selects the top-3 most relevant skills for a user query using local embedding similarity
4. Semantic routing runs entirely on-device with zero external API calls (<5ms inference)

**Plans**: TBD
**UI hint**: yes

### Phase 7: Python Skills

**Goal**: DADOU executes complex Python skills that require full language support in a sandboxed Docker sidecar.

**Depends on**: Phase 4 (skill infrastructure: manifest, store, CLI), Phase 1 (Windows sandbox base)

**Requirements**: SKL-03

**Success Criteria** (what must be TRUE):
1. DADOU launches a Python skill in a Docker container via WSL2 backend on Windows
2. Communication between DADOU and the Python skill uses JSON-RPC over stdin/stdout
3. The Docker sandbox enforces CPU quota, memory limit, and network isolation (no outbound by default)
4. Graceful fallback is documented and communicated to the user when Docker is not installed

**Plans**: TBD

---

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Security Foundation | 6/6 | ✅ Complete | 2026-06-04 |
| 2. Memory & Continuity | 4/4 | ✅ Complete | 2026-06-05 |
| 3. Guardian N2+N3 | 4/4 | ✅ Complete | 2026-06-05 |
| 4. Skills WASM | 5/5 | ✅ Complete | 2026-06-05 |
| 5. Anti-Injection | 4/4 | ✅ Complete | 2026-06-05 |
| 6. Dashboard & Semantic Router | 4/4 | ✅ Complete | 2026-06-05 |
| 7. Python Skills | 0/TBD | ⏳ Pending | - |

---

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| GRD-01 | Phase 1 | ✅ |
| GRD-02 | Phase 3 | ✅ |
| GRD-03 | Phase 3 | ✅ |
| GRD-04 | Phase 1 | ✅ |
| MEM-01 | Phase 2 | ✅ |
| MEM-02 | Phase 2 | ✅ |
| MEM-03 | Phase 2 | ✅ |
| MEM-04 | Phase 2 | ✅ |
| SKL-01 | Phase 4 | ✅ |
| SKL-02 | Phase 4 | ✅ |
| SKL-03 | Phase 7 | ⏳ |
| SKL-04 | Phase 4 | ✅ |
| SKL-05 | Phase 4 | ✅ |
| SKL-06 | Phase 4 | ✅ |
| SKL-07 | Phase 4 | ✅ |
| INJ-01 | Phase 5 | ✅ |
| INJ-02 | Phase 5 | ✅ |
| INJ-03 | Phase 5 | ✅ |
| INJ-04 | Phase 5 | ✅ |
| UND-01 | Phase 1 | ✅ |
| UND-02 | Phase 1 | ✅ |
| RTR-01 | Phase 6 | ✅ |
| OBS-01 | Phase 6 | ✅ |
| CTX-01 | Phase 2 | ✅ |
| CTX-02 | Phase 2 | ✅ |

---
*Last updated: 2026-06-05*
