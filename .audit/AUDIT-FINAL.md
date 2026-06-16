# AUDIT FINAL — DADOU (OpenHuman)

**Date**: 2026-06-16
**Type**: Synthese des 4 rapports d'audit (Build, Securite, Qualite, Documentation)
**TEST-AUDIT.md**: Non disponible — inclus dans le score comme "En attente"

---

## 1. Dashboard Global

| Domaine | Score | Statut |
|---------|-------|--------|
| **Build** (Stage 1) | 6.0 / 10 | ⚠️ Tauri shell bloque, Prettier fail, 44 Rust + 61 ESLint warnings |
| **Securite** (Stage 2) | 7.5 / 10 | ⚠️ CSP trop permissive, 4 unwrap() critiques, 10+ points d'amelioration |
| **Qualite code** (Stage 3) | 4.5 / 10 | 🔴 135 fichiers Rust >800L, 2370L function, 28 fichiers TS morts |
| **Documentation** (Stage 4) | 5.4 / 10 | ⚠️ 12/94 domaines documentes, rebranding incomplet, pas de CODEMAPS |
| **Tests** (Stage 5) | En attente | — |

**Verdit global**: **5.4 / 10** (hors Tests) — Projet fonctionnel mais dette technique severe.
Le code compile (core Rust + TypeScript), les pratiques crypto sont solides, mais la qualite du code et la documentation sont degradees par une croissance non maitrisee.

---

## 2. Score Composite

Sans TEST-AUDIT.md, les poids sont redistribues proportionnellement :

| Domaine | Poids ajuste | Score | Contribution |
|---------|-------------|-------|-------------|
| Build | 18.75% | 6.0 | 1.13 |
| Securite | 31.25% | 7.5 | 2.34 |
| Qualite | 31.25% | 4.5 | 1.41 |
| Documentation | 18.75% | 5.4 | 1.01 |
| **Total** | **100%** | | **5.9 / 10** |

### Scores par categorie (tous audits confondus)

| Severite | Total | Repartition |
|----------|-------|-------------|
| 🔴 CRITICAL | 8 | Securite 4, Qualite 3, Doc 1 |
| ⚠️ HIGH | 14 | Qualite 7, Doc 5, Securite 2 |
| 🔶 MEDIUM | 19 | Securite 10, Qualite 5, Doc 4 |
| 💡 LOW | 13 | Securite 7, Doc 3, Qualite 3 |

---

## 3. Top 10 Actions Critiques

Priorise par impact × effort (P1 = le plus urgent). Les actions sont dedoublonnees des 4 rapports.

### P1 — Urgent (sprint en cours)

| # | Priorite | Domaine | Action | Impact | Effort |
|---|----------|---------|--------|--------|--------|
| 1 | P1 | Build | **Initialiser `git submodule update --init --recursive`** — Debloque la compilation du Tauri shell (vendor/tauri-cef absent) | Critique : Tauri shell non compilable | 1 min |
| 2 | P1 | Securite | **Remplacer `script-src 'unsafe-inline'` par nonce CSP** — La CSP actuelle annule la protection XSS primaire | Critique : XSS vector ouvert | 1-2 jours |
| 3 | P1 | Qualite | **Extraire `build_deterministic_checkpoint` de `turn.rs`** — Fonction de ~2370 lignes rendant le module non maintenable | Critique : maintenance et revue impossibles | 2-3 jours |

### P2 — Court terme (1-2 sprints)

| # | Priorite | Domaine | Action | Impact | Effort |
|---|----------|---------|--------|--------|--------|
| 4 | P2 | Securite | **Remplacer `unwrap()` par `?` dans `bubblewrap.rs` et `firejail.rs`** — Panique si l'outil de sandboxing est absent = DoS | Haut : crash processus sur echec systeme | 2h |
| 5 | P2 | Qualite | **Extraire `has_hidden_execution` de `policy.rs`** — 961 lignes dans une fonction de securite, rend l'audit impossible | Haut : fonction securite non auditable | 1-2 jours |
| 6 | P2 | Doc | **Mettre a jour CLAUDE.md avec les 94 domaines `src/openhuman/`** — 12/94 documentes = 87% de trous | Critique : agents desinformes par defaut | 1 jour |
| 7 | P2 | Qualite | **Supprimer les 28 fichiers inutilises + 6 dependances mortes (rapport knip)** | Haut : 88 exports non utilises, code mort en production | 1 jour |

### P3 — Moyen terme (3-4 sprints)

| # | Priorite | Domaine | Action | Impact | Effort |
|---|----------|---------|--------|--------|--------|
| 8 | P3 | Build | **Corriger les 50 warnings `setState-in-effect` (React 19)** — Re-rendus en cascade, le plus gros contributeur ESLint | Moyen : 50/61 warnings ESLint | 2 jours |
| 9 | P3 | Doc | **Rebranding complet OpenHuman -> DADOU** — CLAUDE.md, AGENTS.md, README.md, CONTRIBUTING.md, READMEs traduits | Haut : coherence marque | 1 jour |
| 10 | P3 | Securite | **Ajouter `cargo audit` dans la CI** — Detection automatique de CVE sur les dependances Rust | Moyen : 0 scan CVE actuellement | 4h |

---

## 4. Matrice de Risque

### 🔴 Risque eleve + Probable — Actions immediates requises

| Risque | Description | Mitigation |
|--------|-------------|------------|
| CSP `'unsafe-inline'` | Tous les scripts inline executables — XSS possible si une injection reussit | Remplacer par nonce/hash CSP (P1 #2) |
| 135 fichiers Rust >800 lignes | Impossibilite de revue efficace, bugs caches, dette qui croit | Plan de refactoring systematique (P2 #5, #7) |
| 1500+ TODOs/FIXMEs non ticketes | Dette invisible, decisions techniques perdues, bugs non suivis | Campaigne de ticketing + deduplication |
| 28 fichiers TS morts en production | Surface d'attaque inutile, maintenance gaspillée | Suppression immediate (P2 #8) |

### 🟠 Risque eleve + Improbable — Surveiller

| Risque | Description | Mitigation |
|--------|-------------|------------|
| Crash bubblewrap/firejail | Panique si outil sandboxing absent — feature-gated, improbable en prod | Propagation d'erreur (P2 #4) |
| Fuite de secrets via logs | Scrutation `scrub_secrets()` en place, regex couvrantes | Maintenir la couverture regex |
| Panique Regex au demarrage | Regex statiques `unwrap()` dans `main.rs` — patterns valides manuellement | Ajouter `expect()` (P1 #4 secondaire) |

### 🟡 Risque faible + Probable — Planifier

| Risque | Description | Mitigation |
|--------|-------------|------------|
| 61 warnings ESLint | 50 setState-in-effect (React 19), 11 autres — n'empechent pas le build | Refactoring React 19 (P3 #9) |
| Prettier non deterministe | 1 fichier non formaté, risque de divergence | Hook Prettier post-commit |
| Composants oversized (AIPanel 3468L) | Bugs UI plus probables, difficulté de test | Split en sous-panneaux |

### 🟢 Risque faible + Improbable — Acceptable / Surveillance passive

| Risque | Description | Mitigation |
|--------|-------------|------------|
| SHA-1 pour HMAC-SHA1 Tencent COS | Usage non-securitaire (HMAC uniquement, pas de signature) | Documenter et verifier dans maj |
| Clé maitresse en memoire sans zeroing | Attaque par dump memoire — improbable sans accès machine | Ajouter zeroize (MEDIUM) |
| `EncryptedPayload.salt` toujours vide | Confusion possible, pas de faille | Documenter le champ |

---

## 5. Dette Technique Estimee

Estimation en jours/homme par domaine, basee sur les correctifs identifies dans chaque rapport.

### Build : ~3 jours

| Tache | Effort |
|-------|--------|
| Initialiser submodule + verifier Tauri shell | 0.5h |
| Corriger les 44 warnings Rust (unused imports/vars) | 1 jour |
| Corriger les 50 warnings setState-in-effect (React 19) | 2 jours (estimation haute) |
| Corriger 12 warnings exhaustive-deps | 0.5 jour |
| **Total Build** | **~4 jours** |

### Securite : ~7 jours

| Tache | Effort |
|-------|--------|
| CSP nonce/hash pour script-src | 2 jours |
| Resserrer connect-src et frame-src CSP | 1 jour |
| bubblewrap/firejail unwrap -> ? | 1 jour |
| Ajouter `expect()` aux Regex main.rs | 0.5 jour |
| Mise a jour ring 0.17 -> 0.18 | 0.5 jour |
| Ajouter cargo audit dans CI | 0.5 jour |
| Zeroing memoire cle (zeroize) | 1 jour |
| Revue unsafe blocks + // SAFETY: | 1 jour |
| **Total Securite** | **~7.5 jours** |

### Qualite : ~25 jours

| Tache | Effort |
|-------|--------|
| Split turn.rs (2370L) | 3 jours |
| Split policy.rs::has_hidden_execution (961L) | 2 jours |
| Top 10 fichiers oversized restants | 10 jours (1 jour/fichier) |
| Supprimer 28 fichiers inutilises (knip) | 1 jour |
| Nettoyer 211 console.log calls | 1 jour |
| Ticketter 1500+ TODO/FIXME | 3 jours |
| Renommer 6 fichiers non-PascalCase | 0.5 jour |
| Refactoring CRUD store duplication | 2 jours |
| Normalisation i18n (ecart pl/de) | 1 jour |
| Audit unused Redux slices | 1.5 jour |
| **Total Qualite** | **~25 jours** |

### Documentation : ~6 jours

| Tache | Effort |
|-------|--------|
| Mettre a jour CLAUDE.md (82 domaines manquants) | 2 jours |
| Rebranding OpenHuman -> DADOU (tous fichiers) | 1 jour |
| Creer docs/CODEMAPS/ | 2 jours |
| Ajouter dates de fraicheur dans les docs | 0.5 jour |
| Reduire CLAUDE.md de 880 a <500 lignes | 1 jour |
| Reduire frontend.md (2302L) | 0.5 jour |
| **Total Documentation** | **~6 jours** |

### Tests : En attente

Pas de TEST-AUDIT.md. Estimation baseline : ~5-8 jours pour couvrir les lacunes identifiables.

### Total dette estimee : ~42 jours / homme

> Note : Cette estimation suppose un developpeur familier avec le codebase. Les taches de refactoring (split de fichiers) sont les plus couteuses car elles necessitent de comprendre la logique avant de la decomposer.

---

## 6. Roadmap Recommandee

Sprints de 2 semaines, bases sur priorite P1 > P2 > P3.

### Sprint 1 — Stabilisation urgente

| Priorite | Action |
|----------|--------|
| P1 | `git submodule update --init --recursive` (debloque Tauri shell) |
| P1 | Lancer `pnpm format:fix` (Prettier) |
| P1 | Remplacer CSP `script-src 'unsafe-inline'` par nonce |
| P2 | Remplacer `unwrap()` par `?` dans bubblewrap.rs + firejail.rs |
| P1 | Extraire `build_deterministic_checkpoint` de turn.rs (fonction 2370L) |
| — | Ajouter `expect()` aux Regex `main.rs` |
| — | Ajouter `cargo audit` dans la CI |

**Objectif Sprint 1**: Core Rust + Tauri shell compilent sans warning bloquants. CSP resserree. 3 fonctions critiques refactorees.

### Sprint 2 — Refactoring securite + qualite

| Priorite | Action |
|----------|--------|
| P2 | Extraire `has_hidden_execution` de policy.rs (961L) |
| P2 | Mettre a jour CLAUDE.md — lister les 94 domaines |
| P2 | Supprimer 28 fichiers inutilises (knip) |
| P3 | Mise a jour ring 0.17 -> 0.18 |
| — | Revue des unsafe blocks + ajout // SAFETY: |
| — | Zeroing memoire cle (zeroize) |

**Objectif Sprint 2**: 7/10 en securite. CLAUDE.md a jour. Code mort elimine.

### Sprint 3 — Dette documentation + qualite

| Priorite | Action |
|--------|--------|
| P3 | Rebranding OpenHuman -> DADOU (README, CONTRIBUTING, AGENTS.md) |
| P3 | Creer docs/CODEMAPS/ |
| — | Normalisation i18n (ecart pl:3227 / de:3471) |
| — | Reduire CLAUDE.md de 880 a <500 lignes |
| — | Ticketter 100+ TODO/FIXME prioritaires |
| — | Nettoyer les 211 console.log en production |

**Objectif Sprint 3**: Documentation a 7/10. Dette TODO visible et traquee. Marque unifiee.

### Sprint 4+ — Amelioration continue

| Priorite | Action |
|--------|--------|
| P3 | Corriger 50 warnings setState-in-effect (React 19) |
| — | Refactoring top 10 fichiers oversized (Rust + TS) |
| — | Audit Redux slices inutilises (personaSlice, mascotSlice, notificationSlice) |
| — | Extraction CRUD store generique (SQLite) |
| — | Ajouter dates de fraicheur dans tous les docs |
| — | Ajouter hook Prettier PostToolUse |

**Objectif Sprint 4+**: Score qualite >7/10. React 19 compliant. CI complete avec scans.

---

## 7. Ce Qui Va Bien

Malgre un score composite de 5.9/10, plusieurs piliers sont solides et doivent etre preserves.

### Securite — Les fondations sont bonnes

- **Aucun secret hardcode** dans le code source. Scrutation systematique (7 patterns regex) en Rust, redaction RPC, et sanitization frontend.
- **Chiffrement exemplaire** : AES-256-GCM, ChaCha20-Poly1305, X25519, Argon2id, nonces aleatoires, pas d'ECB, pas de cles statiques, pas d'IV fixes.
- **Protection anti-injection** : 4 regles de detection frontend, detection leetspeak, zero-width unicode, base64, scoring block/review/allow.
- **Gestion d'erreurs** : Typage `thiserror`/`anyhow`, `StructuredRpcError` avec sentinel prefix, redaction logs, scrubbing Sentry.

### Build — Le pipeline fonctionne

- `cargo check` (core Rust) : 0 erreurs
- `tsc --noEmit` : 0 erreurs TypeScript
- ESLint : 0 erreurs (61 warnings non bloquants)
- Les commands documentees (`pnpm dev`, `pnpm test`, `pnpm lint`, `pnpm format`) sont toutes fonctionnelles
- Les scripts de debug (`pnpm debug`) et de mock API (`pnpm mock:api`) existent et sont operationnels

### Architecture — Decisions solides

- Separation transport/domaine via Controller Registry : pas de logique domaine dans le code transport
- Event bus type : communication inter-domaine propre et decouplee
- Core in-process : architecture simplifiee (plus de sidecar)
- i18n : 13 locales, 65 chunks, CI coverage — exemplaire pour un projet desktop
- Documentation gitbooks : 8 fichiers d'architecture existent et sont coherents

### Ce qui merite d'etre cite

- **0 erreur TypeScript** sur un frontend de cette taille (3468L AIPanel.tsx inclus) — le typage est rigoureux
- **La securite crypto est professionnelle** — aucun algorithme obsoletes ou modes dangereux
- **Les conventions i18n sont respectees** — `useT()` systematique, 65/65 chunks presents
- **Le module layout Rust** est discipline — pas de fichiers sauvages a la racine `src/openhuman/` (seulement 2 grandfathered)

---

## Annexe : Scores Bruts par Rapport

| Rapport | Score | CRITICAL | HIGH | MEDIUM | LOW | Verdict |
|---------|-------|----------|------|--------|-----|---------|
| BUILD | 6.0/10 | 0 | 2 | 4 | 0 | PASS (with caveats) |
| SECURITY | 7.5/10 | 4 | 2 | 10 | 7 | WARN |
| QUALITY | 4.5/10 | 3 | 7 | 5 | 3 | BLOCK |
| DOC | 5.4/10 | 1 | 5 | 4 | 3 | WARN |

---

*Synthese produite a partir des 4 rapports d'audit Stage 1-4. Le Stage 5 (Tests) est en attente. Le score composite sera recalcule une fois TEST-AUDIT.md disponible.*
