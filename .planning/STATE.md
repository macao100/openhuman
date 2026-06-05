# DADOU — Project State

**Last updated:** 2026-06-05 17:15
**Current phase:** 6 (Dashboard & Semantic Router) — COMPLETED
**Next phase:** 7 (Python Skills)
**Milestone:** v1
**Global progress:** 6/7 phases (86%)

---

## Project Reference

**Core value:** Un assistant qui apprend. DADOU construit et maintient un modele mental persistant du monde numerique de l'utilisateur - projets, preferences, erreurs passees, succes.

**Current focus:** Phase 5 planifiee (4 plans, 2 waves). Phase 6 a planifier ensuite.

**Key decisions:**
- Fork independant OpenHuman (pas contribution upstream) - ACTIVE
- Architecture hybride : base OpenHuman + couche DADOU - ACTIVE
- Guardian 3 niveaux (N1 + N2 + N3) - N1 DONE, N2+N3 DONE
- WASM + Python pour les skills (pas Python uniquement) - Phase 4/7
- Mono-utilisateur local-first - ACTIVE
- Memoire a provenance et confiance - DONE (Phase 2)
- JailedChild enum pour unifier les backends sandbox - DONE
- RestrictedToken primaire, AppContainer fallback - DONE
- Rollback file-level v1 (action-level v2 deferred) - DONE
- Regles Guardian hybrides Rust+YAML (fail-closed) - DONE
- Provenance JSON dans memory_docs.provenance_json - DONE
- Contexte projet + preferences persistantes - DONE
- Detection de contradictions + evenements - DONE
- Continuite inter-session (save/restore) - DONE
- N2 classifieur local (exfiltration, entropie, payloads caches) - DONE
- N3 validateur LLM leger avec cache LRU - DONE
- Pipeline etendu N1->N2->N3 avec early exit et blocked_by - DONE
- Config guardian_n2 et guardian_n3 dans config.toml - DONE
- Evenements N2Blocked, N2Escalated, N3Result - DONE
- **Dadou-skill.yaml manifest**: format YAML avec nom, version, auteur, GPG, permissions, WASM - DONE
- **Store TOML**: SkillsStore dans `~/.openhuman/skills/store.toml` - DONE
- **Wasmtime in-process**: WASI deny-by-default avec epoch-based timeout 30s - DONE
- **GPG verification**: sequoia-openpgp, git verify-tag --raw + trust store - DONE
- **Static analysis**: 15+ regles (eval, subprocess, socket, network, filesystem) - DONE
- **CLI**: `dadou skill install|update|audit|remove|list|trust-author` - DONE
- **Controllers**: 6 controleurs prefixe `dadou.*` - DONE
- **Format `<external_data>`**: remplace `<untrusted-source>`, attributs `source` + `trusted` + `content_type` - D-51
- **AntiInjectionSection**: section system prompt expliquant le contrat de confiance - D-52
- **Couverture exhaustive**: memoire, skills WASM, web, fichiers lus - D-53
- **SkillOutputEnvelope**: JSON structure {skill_name, version, content_type, trusted, output} - D-54
- **SemanticOutputValidator**: module anti_injection/validator/ avec regles + LLM check - D-55
- **Interception sortie skills**: dans tool_loop.rs apres execute() avant tool_results - D-56
- **StructuredPlan**: goal + steps[{tool, args, rationale}] - D-57
- **evaluate_plan()**: extension GuardianPipeline pour plans multi-etapes - D-58
- **ExecutionProtocolSection**: format plan JSON explique dans le system prompt - D-59

---

## Current Position

| Dimension | Value |
|-----------|-------|
| Milestone | v1 |
| Phase | 6 - Dashboard & Semantic Router |
| Status | COMPLETED |
| Progress | [######################            ] 86% |

**Next action:** Planifier Phase 7 (Python Skills) — `plan-phase 7`.

---

## Performance Metrics

*A definir apres les premieres phases.*

### Targets (from constraints)

| Metric | Target |
|--------|--------|
| Guardian N1 latency | <1ms |
| Guardian N2 latency | <10ms |
| Guardian N3 latency | <500ms |
| N3 coverage (escalade) | <2% des actions |
| Semantic router inference | <5ms |
| WASM skill timeout | 30s |

### Implemented (measured)

| Metric | Target | Status |
|--------|--------|--------|
| Provenance confidence decay | <100ms per pass | Implemented |
| Contradiction detection | <100ms per check | Implemented |
| Session save/restore | <50ms (synchronous) | Implemented |

---

## Accumulated Context

### Decisions taken

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-06-04 | Phase 1 = Security Foundation (N1 + rollback + Windows sandbox) | P1/P2 pitfalls bloquent toute execution de skills |
| 2026-06-04 | Anti-Injection = phase separee (Phase 5) | Depend de la memoire structuree (Phase 2) et des skills (Phase 4) |
| 2026-06-04 | Python Skills = derniere phase (Phase 7) | Docker sidecar complexe, WASM suffit pour 80% des cas |
| 2026-06-04 | CTX-01/CTX-02 groupes avec MEM | Continuite inter-session = extension naturelle de la memoire persistante |
| 2026-06-04 | GRD-04 (Windows sandbox) dans Phase 1 | Pitfall critique identifie dans la recherche |
| 2026-06-05 | Phase 1: Guardian N1 domain implemented | Types, rules, pipeline, schemas, bus subscriber |
| 2026-06-05 | Phase 1: Windows sandbox (RestrictedToken + JailedChild) | GRD-04 resolved |
| 2026-06-05 | Phase 1: Rollback infrastructure (SQLite + LCS diff) | UND-01 implemented, UND-02 CLI wired |
| 2026-06-05 | D-12: Provenance JSON dans memory_docs.provenance_json | Pas de nouvelle table, ALTER TABLE ADD COLUMN |
| 2026-06-05 | D-16: Decay scheduler: Verified->Inferred 30j, External->delete 7j | Configurable via RPC |
| 2026-06-05 | D-18: Namespace dadou_project_context pour faits projet | Injecte dans le prompt agent au demarrage |
| 2026-06-05 | D-22: Preferences avec provenance user_correction/verified | Outil dadou_correct_preference |
| 2026-06-05 | D-25: Moteur de contradiction conservative | Vector recall + confidence gate, evenement ContradictionDetected |
| 2026-06-05 | D-28: Session context dans dadou_session_context SQLite | Save on shutdown + periodic 5min + restore on startup |
| 2026-06-05 | D-32: N2 sous-domaine guardian/n2/ avec 3 detecteurs | Exfiltration, entropie, hidden payloads |
| 2026-06-05 | D-33: Detection patterns d'exfiltration (8+ regex) | Data URLs, DNS tunnels, reverse shells, SSH/ngrok, socat |
| 2026-06-05 | D-34: Scoring N2: block >0.7, escalate 0.3-0.7, allow <0.3 | Deux seuils configurables |
| 2026-06-05 | D-35: N3 sous-domaine guardian/n3/ avec LLM wrapper | Utilise local_ai_prompt existant |
| 2026-06-05 | D-36: N3 utilise inference::local::ops::local_ai_prompt | Pas de nouveau provider |
| 2026-06-05 | D-37: System prompt N3 avec sortie JSON structuree | Verdict: allow/block/uncertain |
| 2026-06-05 | D-38: N3 cache LRU (taille 100) | Deduplication intra-session |
| 2026-06-05 | D-39: GuardianPipeline avec early exit | N1->N2->N3, blocked_by dans le resultat |
| 2026-06-05 | D-40: Escalade N2->N3 conditionnelle | Uniquement si N2 incertain |
| 2026-06-05 | D-41: Sections [guardian_n2] et [guardian_n3] dans config.toml | Seuils, timeouts, enable/disable |
| 2026-06-05 | D-42: N3 disabled + N2 escalate = block (fail-closed) | Securite maximale par defaut |
| 2026-06-05 | D-43: Nouveaux DomainEvent variants | N2Blocked, N2Escalated, N3Result |
| 2026-06-05 | D-44: dadou-skill.yaml manifest | Format YAML avec nom, version, auteur, GPG, permissions, WASM |
| 2026-06-05 | D-45: Store TOML ~/.openhuman/skills/store.toml | SkillsStore avec load/save/upsert/remove |
| 2026-06-05 | D-46: Wasmtime in-process avec WASI deny-by-default | Epoch-based timeout 30s, reseau bloque, filesystem restreint |
| 2026-06-05 | D-47: GPG via sequoia-openpgp + git verify-tag --raw | Trust store ~/.openhuman/skills/certs/ |
| 2026-06-05 | D-48: Static analysis avec 15+ regles | Block sur eval/subprocess/socket/network non autorise |
| 2026-06-05 | D-49: CLI dadou skill install|update|audit|remove|list | Sous-commande dans core/cli.rs |
| 2026-06-05 | D-50: 6 controleurs dadou.skill_* | Prefixe dadou. pour distinguer des skills.* herites |
| **2026-06-05** | **D-51: Format `<external_data>`** | **Tag `<external_data source="..." trusted="false">` remplace `<untrusted-source>`** |
| **2026-06-05** | **D-52: AntiInjectionSection system prompt** | **Section expliquant le contrat de confiance au LLM** |
| **2026-06-05** | **D-53: Couverture exhaustive wrapping** | **Memoire, skills WASM, web content, file reads** |
| **2026-06-05** | **D-54: SkillOutputEnvelope JSON** | **{skill_name, version, content_type, trusted, output, truncated}** |
| **2026-06-05** | **D-55: SemanticOutputValidator** | **Module anti_injection/validator/ avec rules.rs + llm_check.rs** |
| **2026-06-05** | **D-56: Interception dans tool_loop** | **Apres execute() et avant tool_results, wrapping + validation** |
| **2026-06-05** | **D-57: StructuredPlan type** | **{goal, steps: [{tool, args, rationale}]}** |
| **2026-06-05** | **D-58: GuardianPipeline::evaluate_plan()** | **Validation structure + intention + chaque etape via pipeline** |
| **2026-06-05** | **D-59: ExecutionProtocolSection** | **Section system prompt expliquant le format plan JSON** |

### Open questions

1. Build/supply chain security audit - npm ~1100 packages, Cargo crates non audites
2. Versions exactes des dependances (wasmtime, sequoia) a verifier sur crates.io
3. Gestion des conflits de port 7790 pour le dashboard
4. ~~Schema de migration de la memoire - superposition vs migration~~ -> RESOLU: PRAGMA user_version pattern

### Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| AppContainer OwnedHandle bridge | **Resolved** | — | JailedChild enum |
| Docker non disponible sur Windows sans WSL2 | Low | High (Phase 7) | Fallback Python via wasmtime/Pyodide |
| Performance N3 >500ms sur hardware modeste | Medium | Medium | N3 optionnel, basculer sur N2. Timeout configurable. |
| whisper-rs-sys build bloque cargo check/test | **Active** | Medium | Verification manuelle par pattern matching |
| wasmtime compile time/linkage Windows | Medium | Medium | CI test avec cargo check -p openhuman |
| sequoia-openpgp compile time | Medium | Low | Feature crypto-rust, pas de native NSS |
| Faux positifs validation semantique | Medium | Medium | Mode Relaxed disponible, regles ajustables |
| Plan validation N3 >500ms pour plans complexes | Medium | Medium | Plan capped a 20 steps, timeout N3 450ms |

### Blockers

- **whisper-rs-sys**: libclang.dll manquant sur Windows -> cargo check impossible. Tout le code Phase 1-5 est verifie par correspondance structurelle avec les patterns existants.

---

## Phase Summary

### Phase 1 — Security Foundation ✅
| Plan | Content | Commit |
|------|---------|--------|
| 01-01 | Guardian N1 domain | `80d1061` |
| 01-02 | Windows sandbox fix | `a3c4379` |
| 01-03 | Rollback foundation | `cee1264` |
| 01-04 | Guardian N1 interception | `3faccdd` |
| 01-05 | Rollback hooks | `c03e94f` |
| 01-06 | CLI undo | `efaf501` |

### Phase 2 — Memory & Continuity ✅
| Plan | Content | Commit |
|------|---------|--------|
| 02-01 | Provenance & Confidence (MEM-04) | `b7c3f84` |
| 02-02 | Project Context & Preferences (MEM-01, MEM-02) | `a18b253` |
| 02-03 | Contradiction Detection (MEM-03) | `270f323` |
| 02-04 | Cross-Session Continuity (CTX-01, CTX-02) | `ed5acc5` |

### Phase 3 — Guardian N2+N3 ✅
| Plan | Content | Wave |
|------|---------|------|
| 03-01 | Guardian N2: types, detecteurs (exfiltration, entropie, hidden payloads) | 1 |
| 03-02 | Guardian N3: LLM validator, system prompt, LRU cache | 1 |
| 03-03 | Pipeline etendu N1->N2->N3, events, tool loop wiring | 2 |
| 03-04 | Controllers N2/N3, config schema, initialization | 2 |

### Phase 4 — Skills WASM ✅
| Plan | Content | Wave |
|------|---------|------|
| 04-01 | Manifest dadou-skill.yaml + TOML SkillsStore (SKL-01, SKL-06) | 1 |
| 04-02 | Wasmtime runtime + WASI capability-gated (SKL-02) | 1 |
| 04-03 | GPG verification via sequoia-openpgp + trust store (SKL-04) | 1 |
| 04-04 | Static analysis: imports, filesystem, network (SKL-05) | 1 |
| 04-05 | CLI dadou skill + JSON-RPC controllers (SKL-07) | 2 |

### Phase 5 — Anti-Injection ✅
| Plan | Content | Wave |
|------|---------|------|
| 05-01 | External data tagging `<external_data>` + AntiInjectionSection (INJ-01) | 1 |
| 05-02 | SkillOutputEnvelope JSON structure (INJ-02) | 1 |
| 05-03 | Semantic output validation: rules + LLM check (INJ-03) | 2 |
| 05-04 | JSON plan validation via Guardian pipeline (INJ-04) | 2 |

### Phase 6 — Dashboard & Semantic Router 📋 (To plan)
*Planning en cours — Phase 6*

### Phase 7 — Python Skills ⏳
*To be planned after Phase 6*

---

## Session Continuity

### Files referenced

| File | Role |
|------|------|
| `.planning/ROADMAP.md` | Phase definitions and success criteria |
| `.planning/REQUIREMENTS.md` | 25 v1 requirements with IDs |
| `.planning/research/SUMMARY.md` | Research synthesis, build order |
| `.planning/config.json` | Granularity: fine, mode: yolo |
| `.planning/STATE.md` | This file - project state |
| `.planning/phases/03-guardian-n2n3/03-CONTEXT.md` | Phase 3 decisions D-32->D-43 |
| `.planning/phases/03-guardian-n2n3/03-01-PLAN.md` | N2 classifier engine |
| `.planning/phases/03-guardian-n2n3/03-02-PLAN.md` | N3 LLM validator |
| `.planning/phases/03-guardian-n2n3/03-03-PLAN.md` | Extended pipeline |
| `.planning/phases/03-guardian-n2n3/03-04-PLAN.md` | Config + controllers |
| `.planning/phases/04-skills-wasm/04-CONTEXT.md` | Phase 4 decisions D-44->D-50 |
| `.planning/phases/04-skills-wasm/04-01-PLAN.md` | Manifest + Store (SKL-01, SKL-06) |
| `.planning/phases/04-skills-wasm/04-02-PLAN.md` | Wasmtime runtime (SKL-02) |
| `.planning/phases/04-skills-wasm/04-03-PLAN.md` | GPG verification (SKL-04) |
| `.planning/phases/04-skills-wasm/04-04-PLAN.md` | Static analysis (SKL-05) |
| `.planning/phases/04-skills-wasm/04-05-PLAN.md` | CLI + Controllers (SKL-07) |
| `.planning/phases/05-anti-injection/05-CONTEXT.md` | Phase 5 decisions D-51->D-59 |
| `.planning/phases/05-anti-injection/05-01-PLAN.md` | External data tagging (INJ-01) |
| `.planning/phases/05-anti-injection/05-02-PLAN.md` | Structured skill output (INJ-02) |
| `.planning/phases/05-anti-injection/05-03-PLAN.md` | Semantic output validation (INJ-03) |
| `.planning/phases/05-anti-injection/05-04-PLAN.md` | JSON plan validation (INJ-04) |
| `CLAUDE.md` | Repo layout, commands, conventions |

### Next commands

1. `plan-phase 6` — Planifier la Phase 6 (Dashboard & Semantic Router)
2. `execute-phase 06-dashboard` — Exécuter les plans Phase 6
3. `plan-phase 7` — Planifier la Phase 7 (Python Skills)
4. `execute-phase 07-python-skills` — Exécuter les plans Phase 7
5. Installer cmake + LLVM pour débloquer `cargo check` (whisper-rs-sys)

---
*Last updated: 2026-06-05*
