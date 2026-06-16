# Sprint 1 — Remédiation Qualité + Documentation

**Date:** 2026-06-16
**Source:** Audit DADOU (.audit/AUDIT-FINAL.md, score 5.9/10)

## Objectif

Corriger les problèmes rapides (low-hanging fruits) pour remonter :
- Qualité code de 4.5 → 6.5
- Documentation de 5.4 → 7.0
- Build de 6.0 → 8.0

## Lots

### Q1 — Dead code removal (1j)
Supprimer 28 fichiers inutilisés + 88 exports morts détectés par knip.
Vérification : `pnpm knip` clean + `pnpm typecheck` OK.

### Q2 — console.log → debug (2j)
Remplacer 211 `console.log` en production par `debug` namespacé.
Exceptions : `desktopDeepLinkListener.ts`, `useDaemonLifecycle.ts` (log opérationnel légitime → `logger.info`).
Vérification : `grep -r "console\.log" app/src/` = 0.

### Q3 — Rust warnings (0.5j)
Corriger 46 warnings (unused imports, dead code, mut inutile).
Vérification : `cargo check 2>&1 | grep warning` = 0.

### D1 — Rebranding docs (1j)
Remplacer OpenHuman → DADOU dans README.md, CONTRIBUTING.md, AGENTS.md.
Garder "OpenHuman" dans les contextes historiques/légaux.
Vérification : grep négatif sur les docs principales.

### S1 — CSP nonce-based (1.5j)
Remplacer `'unsafe-inline'` dans script-src par nonce per-request.
Configurer le Tauri shell pour injecter le nonce.
Vérification : CSP header dans la réponse HTTP.

### B1 — Git submodule init (0.1j)
`git submodule update --init --recursive`
Vérification : `cargo check --manifest-path app/src-tauri/Cargo.toml` OK.

## Règle

Un commit par lot. Atomique, vérifiable.
