# Sprint 2 — Split turn.rs

**Date:** 2026-06-16
**Objectif:** Découper `turn.rs` (2475L) en 6 modules <500L

## Architecture cible

```
session/
├── mod.rs                  (ré-exports)
├── types.rs                (existant)
├── transcript.rs           (existant)
├── turn.rs                 (~150L) — Agent::turn()
├── turn_context.rs         (~400L) — inject_agent_experience_context
├── turn_progress.rs        (~260L) — emit_progress + summarize_checkpoint
├── turn_integrations.rs    (~190L) — fetch_integrations + refresh_delegation
└── turn_system_prompt.rs   (~110L) — build_system_prompt
```

## Contrainte

Chaque fonction reste `impl Agent { fn ... }` dans son module.
Zéro changement de logique. Zéro changement d'interface publique.
C'est du déplacement de code uniquement.

## Vérification

- Chaque fichier ≤500L
- `cargo check` OK
- `cargo test --lib` pour le domaine agent OK
- Pas de régression sur les tests existants
