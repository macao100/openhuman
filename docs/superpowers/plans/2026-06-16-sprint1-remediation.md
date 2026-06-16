# Sprint 1 — Remédiation Qualité + Documentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Corriger les low-hanging fruits pour remonter Qualité (4.5→6.5), Documentation (5.4→7.0), Build (6.0→8.0)

**Architecture:** 6 lots indépendants exécutables en parallèle : dead code removal, console.log→debug, Rust warnings, rebranding docs, CSP nonce, git submodule

**Tech Stack:** Rust 1.93, TypeScript 5.8, React 19, Tauri v2, pnpm

---

### Task 1: Git submodule init (B1)

**Files:** `.gitmodules`

- [ ] `git submodule update --init --recursive`
- [ ] `cargo check --manifest-path app/src-tauri/Cargo.toml 2>&1`
- [ ] Commit: `fix: init git submodules for Tauri CEF vendor`

### Task 2: Dead code removal (Q1)

**Files:** Identifiés par `pnpm knip` — 28 fichiers inutilisés + 88 exports morts dans `app/src/`

- [ ] `pnpm knip --no-progress 2>&1` → fichier de référence
- [ ] Supprimer les 28 fichiers inutilisés
- [ ] Supprimer les 88 exports morts
- [ ] `pnpm typecheck` — doit passer
- [ ] `pnpm test --run` — doit passer
- [ ] Commit: `chore: remove 28 unused files + 88 dead exports (knip)`

### Task 3: console.log → debug namespacé (Q2)

**Files:** 211 occurrences dans `app/src/`

- [ ] Remplacer `console.log(...)` par `debug(...)` (import `debug` from `app/src/lib/debug`)
- [ ] Exceptions légitimes → `logger.info(...)` : `desktopDeepLinkListener.ts`, `useDaemonLifecycle.ts`
- [ ] `grep -r "console\.log" app/src/` → 0 résultat
- [ ] `pnpm test --run` — doit passer
- [ ] `pnpm lint` — doit passer
- [ ] Commit: `refactor: replace 211 console.log with namespaced debug`

### Task 4: Rust warnings (Q3)

**Files:** ~46 warnings dans `src/` (unused imports, dead mut, etc.)

- [ ] `cargo check 2>&1 | grep warning` → liste
- [ ] Corriger chaque warning
- [ ] `cargo check 2>&1 | grep warning` → 0
- [ ] `cargo fmt`
- [ ] Commit: `chore: fix 46 Rust warnings (unused imports, dead mut)`

### Task 5: Rebranding docs (D1)

**Files:** `README.md`, `CONTRIBUTING.md`, `AGENTS.md`

- [ ] Remplacer "OpenHuman" → "DADOU" dans les 3 fichiers
- [ ] Garder "OpenHuman" dans les contextes historiques/légaux
- [ ] `grep -i "openhuman" README.md CONTRIBUTING.md AGENTS.md` → occurrences légitimes uniquement
- [ ] Commit: `docs: rebrand OpenHuman → DADOU in core docs`

### Task 6: CSP nonce-based (S1)

**Files:** `app/src-tauri/src/lib.rs`, `app/src-tauri/tauri.conf.json`, config CSP Rust

- [ ] Remplacer `'unsafe-inline'` dans `script-src` par nonce per-request
- [ ] Mettre à jour `connect-src` pour restreindre les origines
- [ ] Retirer `wasm-unsafe-eval` si non nécessaire
- [ ] Vérifier le header CSP dans la réponse HTTP
- [ ] `cargo check --manifest-path app/src-tauri/Cargo.toml` OK
- [ ] Commit: `security: replace CSP unsafe-inline with nonce-based policy`
