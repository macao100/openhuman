# Audit Stage 1 — Build Health Report

**Projet**: DADOU (openhuman)
**Date**: 2026-06-16
**Platform**: Windows 11, Git Bash, Node v24.13.1, pnpm 10.10.0

---

## Executive Summary

| Status | Count |
|--------|-------|
| PASS | 5 |
| PASS (warnings) | 1 |
| FAIL | 1 |

**Verdict**: Le build est fonctionnel mais avec des defauts notables. Le Tauri shell ne compile pas en raison d'un sous-module git non initialise. Prettier echoue sur 1 fichier deja modifie localement. Le code Rust produit 44 warnings, ESLint en produit 61.

---

## Tableau par Commande

| # | Commande | Status | Errors | Warnings | Time |
|---|----------|--------|--------|----------|------|
| 1 | `cargo check` (core) | PASS | 0 | 44 | 1.86s |
| 2 | `cargo check` (Tauri shell) | FAIL | 1 | 0 | <2s |
| 3 | `cargo fmt --check` (core) | PASS | 0 | 0 | <1s |
| 4 | `cargo fmt --check` (Tauri) | PASS | 0 | 0 | <1s |
| 5 | `tsc --noEmit` | PASS | 0 | 0 | 45.83s |
| 6 | ESLint | PASS (warnings) | 0 | 61 | 3.57s |
| 7 | Prettier | FAIL | 1 file | 0 | <3s |

---

## Detail des Erreurs

### 1. Cargo check (core) — 44 warnings

**Lib (42 warnings)**:
| Categorie | Nombre | Exemple |
|-----------|--------|---------|
| unused imports | ~22 | `std::io::Write as _`, `ValidatorConfig`, `LlmVerdictKind`, `InjectionRule`, `FileExt`, `File`, `DateTime`, `ExecutionStatus`, `VoiceCapability`, `Context`, `CommandExt` |
| unused variables | ~6 | `arguments`, `cipher`, `a`, `b`, `first_op_idx`, `last_op_idx`, `idx`, `installed`, `server_cancel` |
| unnecessary `mut` | ~4 | `dacl_present`, `stmt`, `stmt` |
| non_snake_case struct fields | ~6 | `Luid`, `Attributes`, `Value`, `Sid`, `Attributes`, `Label` (in `windows_restricted.rs`) |
| private_interfaces | ~1 | `PROVIDERS` constant more visible than its `Provider` type |
| dead_code | ~1 | Unread fields in slack_backfill `Outcome` enum |

**Bin slack-backfill (2 warnings)**:
- Field `0` never read in `OtherFail(String)` and `Transport(String)` variants

### 2. Cargo check (Tauri shell) — ECHEC

```
error: failed to load source for dependency `tauri`
Caused by:
  Unable to update .../vendor/tauri-cef/crates/tauri
Caused by:
  failed to read .../vendor/tauri-cef/crates/tauri/Cargo.toml
Caused by:
  Le chemin d acces specifie est introuvable (os error 3)
```

**Cause racine**: Le sous-module git `vendor/tauri-cef` est un dossier vide. Le checkout courant n'a pas ete initialise avec `git submodule update --init --recursive`.

Le Tauri shell utilise un fork vendu de Tauri (feat/cef) pour le support Chromium. Sans ce sous-module, la resolution de dependance echoue. **Ce n'est pas une erreur de code** mais un probleme de configuration du workspace.

**Resolution**:
```bash
cd <repo-root>
git submodule update --init --recursive
```

Note: `scripts/ensure-tauri-cli.sh` detecte ce cas et emet un message d'erreur explicite demandant l'initialisation du sous-module.

### 5. TypeScript tsc --noEmit — OK

Aucune erreur TypeScript. Fichier `tsconfig.json` valide, toutes les dependances de type resolues.

### 6. ESLint — 61 warnings, 0 errors

Repartition par regle:

| Regle | Occurrences |
|-------|-------------|
| `react-hooks/set-state-in-effect` | ~50 |
| `react-hooks/exhaustive-deps` | ~12 |
| `@typescript-eslint/no-explicit-any` | ~1 (estimation) |

Les 50 warnings `set-state-in-effect` sont lies a React 19 — le lint interdit desormais `setState()` synchrone dans `useEffect`. Ces appels etaient consideres normaux sous React 18 mais declenchent des re-rendus en cascade sous React 19. C'est le probleme le plus repandu dans le code frontend.

### 7. Prettier — 1 fichier non formate

- `app/src/test/setup.ts` — deja modifie localement (visible dans `git status`)

La commande `pnpm format` (Prettier --write) resoudra ce probleme.

---

## Recommandations

| Priorite | Action | Effort | Impact |
|----------|--------|--------|--------|
| HAUTE | Initialiser `git submodule update --init --recursive` | 1 min | Debloque la compilation Tauri shell |
| HAUTE | Lancer `pnpm format:fix` (Prettier --write) | <1 min | Corrige le check Prettier |
| MOYENNE | Passe en revue les ~50 appels setState-in-effect → refactoring React 19 | 1-2 jours | Elimine les 50 warnings ESLint les plus bruyants |
| MOYENNE | Ajouter `#[allow(unused)]` ou corriger les imports inutilises Rust | 30 min | Passe de 44 a ~10 warnings Rust |
| BASSE | Corriger les noms snake_case dans `windows_restricted.rs` | 15 min | Elimine 6 warnings nonstandard_style |
| BASSE | Completer les dep manquantes dans les hooks `exhaustive-deps` | 30 min | Elimine 12 warnings ESLint |

## Resume

Le build est **utilisable** mais imperfect:
- **Core Rust** compile sans erreur (44 warnings mineurs)
- **TypeScript** compile sans erreur
- **ESLint** 61 warnings mais 0 erreur bloquante
- **Tauri shell** debloque apres `git submodule update`
- **Prettier** debloque apres formatage du fichier modifie
