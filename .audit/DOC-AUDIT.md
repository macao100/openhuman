# Audit Documentation et Architecture -- DADOU (OpenHuman)

**Date:** 2026-06-16
**Perimetre:** Projet complet (Rust core + TypeScript/React frontend + Tauri v2)
**Methode:** Read-only. Verification fichiers, commandes, conventions (echantillon 10 fichiers).

---

## Resume Executif

| Categorie | Score | Note |
|-----------|-------|------|
| CLAUDE.md vs realite | 6/10 | Commandes OK mais 82/94 domaines non documentes |
| AGENTS.md | 8/10 | Existe, complet mais marque OpenHuman (pas DADOU) |
| Architecture documentee | 3/10 | 12/94 domaines listes dans CLAUDE.md |
| i18n | 10/10 | 65/65 chunks presents, codes-langue coherents |
| README/CONTRIBUTING | 5/10 | Existent mais marque OpenHuman |
| Gitbooks/developing | 9/10 | Tous les fichiers references existent |
| Conventions code | 6/10 | 3/10 fichiers echantillon violents les regles |
| Fraicheur docs | 2/10 | Aucune date de mise-a jour dans les docs |
| Documentation CODEMAPS | 0/10 | `docs/CODEMAPS/` inexistant |

**Score global : 5.4 / 10**

---

## 1. CLAUDE.md vs Realite (Commandes et Scripts)

### Ce qui fonctionne

| Commande documentee | Statut | Notes |
|--------------------|--------|-------|
| `pnpm dev` | OK | Delegue a `pnpm --filter dadou-app dev` |
| `pnpm dev:app` | OK | Script complet avec tauri:ensure + dotenv |
| `pnpm build` | OK | `tsc && vite build` via filtre |
| `pnpm compile` / `pnpm typecheck` | OK | `tsc --noEmit` |
| `pnpm lint` | OK | ESLint flat config |
| `pnpm format` / `pnpm format:check` | OK | Prettier + cargo fmt |
| `cargo check` / `cargo build` | OK | Core lib |
| `pnpm test` | OK | Vitest |
| `pnpm test:coverage` | OK | Vitest + coverage |
| `pnpm test:rust` | OK | Delegue a `scripts/test-rust-with-mock.sh` |
| `pnpm debug` | OK | `scripts/debug/cli.sh` |
| `pnpm mock:api` | OK | `scripts/mock-api-server.mjs` |
| `pnpm i18n:check` | OK | `scripts/i18n-coverage.ts` |
| `scripts/load-dotenv.sh` | OK | Existe |
| `scripts/ensure-tauri-cli.sh` | OK | Existe |
| `scripts/e2e-run-spec.sh` | OK | Dans `app/scripts/e2e-run-spec.sh` |
| `app/test/vitest.config.ts` | OK | Existe |
| `app/test/wdio.conf.ts` | OK | Existe |
| `app/src-tauri/src/core_process.rs` | OK | Existe |
| `app/src/utils/config.ts` | OK | Existe, 205 lignes |

### Problemes

| Element | Severite | Details |
|---------|----------|---------|
| `app/.env.example` reference | LOW | CLAUDE.md dit `app/.env.example`, fichier existe |
| `app/scripts/run-dev-win.sh` | LOW | Existe sous `scripts/run-dev-win.sh` (pas `app/scripts/`) mais le path `../scripts/run-dev-win.sh` depuis `app/` est correct |
| `pnpm core:stage` qualifie de no-op | OK | Correct (sidecar supprime PR #1061) |
| GSD workflow en commentaires HTML | MEDIUM | GSD marqueurs dans CLAUDE.md peuvent embrouiller les agents |
| CLAUDE.md 880 lignes | MEDIUM | Depasse la limite de 500 lignes recommandee |

---

## 2. AGENTS.md

**Statut:** EXISTE (656 lignes, 47 KB)
**Chemin:** `/AGENTS.md`

**Forces:**
- Description de l'architecture a jour (core in-process, pas sidecar)
- References vers gitbooks correctes
- Contenu substantiel et coherent

**Faiblesses:**
- Toujours intitule "OpenHuman" (projet rebrande DADOU en v1.0.0)
- Pas de date de derniere mise a jour
- Ne mentionne pas les nouveaux domaines (memory_*, mcp_*, guardian, etc.)

---

## 3. Architecture Documentee vs Code Reel

### Domaines documentes dans CLAUDE.md (12/94)

```
about_app, agent, config, cron, devices, inference, memory, security, skills, socket, tools, webhooks
```

### Domaines NON documentes dans CLAUDE.md (82)

```
accessibility, agent_experience, agent_tool_policy, anti_injection,
app_state, approval, audio_toolkit, autocomplete, billing, channels,
composio, connectivity, context, cost, credentials, cwd_jail, dashboard,
desktop_companion, doctor, embeddings, encryption, guardian, health,
heartbeat, http_host, integrations, javascript, keyring, learning,
mcp_audit, mcp_client, mcp_registry, mcp_server, meet, meet_agent,
memory_archivist, memory_conversations, memory_entities, memory_graph,
memory_queue, memory_store, memory_sync, memory_tools, memory_tree,
migration, migrations, notifications, overlay, people, prompt_injection,
provider_surfaces, redirect_links, referral, rollback, routing,
runtime_node, runtime_python, scheduler_gate, screen_intelligence,
semantic_router, service, session_context, startup, subconscious,
team, test_support, text_input, threads, tls, todos, tokenjuice,
tool_registry, tool_timeout, update, vault, voice, wallet,
webview_accounts, webview_apis, webview_notifications, whatsapp_data,
workspace
```

**Impact:** 87% des domaines ne sont pas documentes. Documentation gravement obsolete.
**Severite: CRITICAL**

### Domaines documentes mais inexistants

- `src/openhuman/providers` -- Reference dans CLAUDE.md mais n'existe pas comme domaine
- `src/openhuman/context` -- EXISTE mais PAS documente
- `src/openhuman/inference` -- EXISTE et documente

### Verification fichiers references dans CLAUDE.md

| Fichier reference | Statut |
|-------------------|--------|
| `src/openhuman/config/schema/types.rs` | OK |
| `src/openhuman/config/schema/load.rs` | OK |
| `src/openhuman/config/schema/autonomy.rs` | OK |
| `src/openhuman/security/policy.rs` | OK |
| `src/openhuman/security/live_policy.rs` | OK |
| `src/openhuman/agent/prompts/` | OK |
| `app/src/utils/config.ts` | OK (205 lignes) |
| `app/src-tauri/src/core_process.rs` | OK |
| `app/tailwind.config.js` | OK |
| `app/test/vitest.config.ts` | OK |
| `app/test/wdio.conf.ts` | OK (146 lignes) |
| `src/main.rs` | OK (301 lignes) |
| `src/core/event_bus/events.rs` | OK |
| `docs/ios/SETUP.md` | OK |

---

## 4. i18n -- Verification des Locales

### 13 locales x 5 chunks = 65 fichiers

| Locale | Chunks | Total cles |
|--------|--------|-----------|
| `ar` | 1-5 | 3,352 |
| `bn` | 1-5 | 3,394 |
| `de` | 1-5 | 3,471 |
| `en` | 1-5 | 3,378 |
| `es` | 1-5 | 3,443 |
| `fr` | 1-5 | 3,456 |
| `hi` | 1-5 | 3,396 |
| `id` | 1-5 | 3,402 |
| `it` | 1-5 | 3,430 |
| `ko` | 1-5 | 3,378 |
| `pl` | 1-5 | 3,227 |
| `pt` | 1-5 | 3,436 |
| `ru` | 1-5 | 3,413 |
| `zh-CN` | 1-5 | 3,285 |

**Source of truth:** `app/src/lib/i18n/en.ts` (3,384 lignes d'export, 3,604 lignes total)

**Observations:**
- Tous les 65 chunks existent. CI `i18n:check` devrait passer.
- `scripts/i18n-coverage.ts` existe.
- `scripts/verify-i18n-bundle.mjs` existe.
- `useT()` defini a `I18nContext.tsx:99` -- conforme a la doc.

**Note:** `pl` a le moins de cles (3,227) et `de` le plus (3,471). La difference de 244 cles entre le max et le min est moderee mais pourrait indiquer un leger decalage de traduction.

**Severite: LOW** (ecart <= 250 cles)

---

## 5. README / CONTRIBUTING

| Fichier | Statut | Taille | Notes |
|---------|--------|--------|-------|
| `README.md` | EXISTE | 193 lignes / 17.5 KB | Marque "OpenHuman" (pas DADOU) |
| `README.de.md` | EXISTE | - | Traduction allemande |
| `README.ja-JP.md` | EXISTE | - | Traduction japonaise |
| `README.ko.md` | EXISTE | - | Traduction coreenne |
| `README.zh-CN.md` | EXISTE | - | Traduction chinoise |
| `app/README.md` | EXISTE | 7 lignes | Template Tauri generique |
| `CONTRIBUTING.md` | EXISTE | ~15 KB | Marque "OpenHuman" |
| `CONTRIBUTING-BEGINNERS.md` | EXISTE | ~14 KB | Marque "OpenHuman" |
| `CODE_OF_CONDUCT.md` | EXISTE | - | Standard |
| `SECURITY.md` | EXISTE | 53 lignes | Politique de securite |
| `PR_DESCRIPTION.md` | EXISTE | - | Template PR |

**Probleme majeur:** Tous les READMEs et le CONTRIBUTING sont marques "OpenHuman" alors que le projet s'appelle DADOU v1.0.0 (visible dans `app/package.json`: `dadou-app 1.0.0`, `package.json` root: `dadou-repo`).

**Severite: HIGH**

---

## 6. Gitbooks/developing/ -- Fichiers d'Architecture

### Tous les fichiers references dans CLAUDE.md existent

| Fichier | Lignes | Statut |
|---------|--------|--------|
| `gitbooks/developing/architecture.md` | 403 | OK |
| `gitbooks/developing/architecture/frontend.md` | 2,302 | OK |
| `gitbooks/developing/architecture/tauri-shell.md` | 216 | OK |
| `gitbooks/developing/architecture/agent-harness.md` | 315 | OK |
| `gitbooks/developing/e2e-testing.md` | 262 | OK |

### Fichiers supplementaires presents

| Fichier | Utile |
|---------|-------|
| `gitbooks/developing/testing-strategy.md` | OK |
| `gitbooks/developing/cef.md` | OK |
| `gitbooks/developing/getting-set-up.md` | OK |
| `gitbooks/developing/building-rust-core.md` | OK |
| `gitbooks/developing/release-policy.md` | OK |
| `gitbooks/developing/mcp-server.md` | OK |
| `gitbooks/developing/agent-observability.md` | OK |
| `gitbooks/developing/architecture/desktop-companion.md` | OK |

### Problemes

| Probleme | Severite |
|----------|----------|
| Aucune date de mise a jour dans aucun fichier | MEDIUM |
| `frontend.md` fait 2,302 lignes (tres long) | MEDIUM |
| Docs references `src/openhuman/providers/` qui n'existe plus | MEDIUM |

---

## 7. Conventions de Code -- Echantillon 10 Fichiers

### Fichiers conformes (7/10)

| Fichier | useT() | console.log | Dynamic imports | Taille |
|---------|--------|-------------|-----------------|--------|
| `SkillDetailDrawer.tsx` | OK (1) | 0 | 0 | - |
| `CoreJobList.tsx` | OK (1) | 0 | - | - |
| `WhatsAppMemorySection.tsx` | OK (1) | 0 | - | - |
| `Conversations.welcomeLock.test.tsx` | N/A (test) | 0 | 0 | - |
| `framing.test.ts` | N/A (test) | 0 | 0 | - |
| `MemoryHeatmap.tsx` | OK (1) | 0 | 0 | - |
| `localAiBootstrap.ts` | N/A (util) | 3 | 0 | 104 lignes |

### Fichiers non conformes (3/10)

| Fichier | Violation | Severite |
|---------|-----------|----------|
| **IntelligenceSubconsciousTab.tsx** | 20 appels `console.log` | HIGH |
| **NotificationRoutingPanel.tsx** | 2 appels `console` | MEDIUM |
| **localAiBootstrap.ts** | 3 appels `console.log` | MEDIUM |

**Taux de conformite constate:** 70% (7/10 fichiers propres)
**Taux de violation console.log:** 30% (3/10 ont des appels non supprimes)

### Points d'attention supplementaires

- `SkillDetailDrawer.tsx` importe `debug` de `'debug'` -- contourne le systeme de logging du projet
- Tous les fichiers utilisent bien des imports statiques (pas de `import()` dynamique)
- Les fichiers UI utilisent `useT()` conformement

---

## 8. Problemes Transversaux

### Docs manquantes

| Doc | Chemin attendu | Status |
|-----|----------------|--------|
| CODEMAPS | `docs/CODEMAPS/INDEX.md` | **MANQUANT** -- repertoire inexistant |
| .claude/memory.md | `.claude/memory.md` | PRESENT (291 lignes) |

### Rebranding OpenHuman -> DADOU

| Element | Valeur actuelle | Valeur attendue | Severite |
|---------|----------------|-----------------|----------|
| CLAUDE.md titre | "OpenHuman" + "DADOU" (mixte) | DADOU | HIGH |
| AGENTS.md titre | "OpenHuman" | DADOU | HIGH |
| README.md | "OpenHuman" | DADOU | HIGH |
| CONTRIBUTING.md | "OpenHuman" | DADOU | HIGH |
| app/package.json | `dadou-app 1.0.0` | Correct | OK |
| Root package.json | `dadou-repo` | Correct | OK |

### Fraicheur des documents

- Aucun fichier de documentation ne contient de date de derniere mise a jour
- CLAUDE.md contient des donnees de commit references qui peuvent dater
- Les gitbooks n'ont pas de metadonnees de version

---

## Recommandations

### CRITICAL (corriger immediatement)

1. **Mettre a jour CLAUDE.md** pour lister les 94 domaines `src/openhuman/` (ou au moins les plus importants : guardian, mcp_*, memory_*, channels, tools, etc.). La section Rust core est a 12/94 domaines documentes.
2. **Ajouter une section CODEMAPS** dans `docs/CODEMAPS/` avec les architectures mises a jour.

### HIGH (corriger avant le prochain release)

3. **Rebranding complet:** Remplacer "OpenHuman" par "DADOU" dans CLAUDE.md, AGENTS.md, README.md, CONTRIBUTING.md, et tous les READMEs traduits.
4. **Supprimer les `console.log`** dans `IntelligenceSubconsciousTab.tsx` (20 occurrences) et les autres fichiers offenders.
5. **Uniformiser les comptes de cles i18n** : `pl` a 3227 cles vs `de` a 3471 (ecart de 244).

### MEDIUM (corriger dans le trimestre)

6. **Ajouter des dates de fraicheur** (`Last Updated: YYYY-MM-DD`) en haut de CLAUDE.md, AGENTS.md, et chaque fichier gitbooks.
7. **Reduire CLAUDE.md** de 880 a < 500 lignes en deplacant les sections GSD (GSD comments) et la stack technique detaillee vers des fichiers dedies.
8. **Mettre a jour `AGENTS.md`** avec les nouveaux patterns (Guardian, MCP, memory_*).
9. **Supprimer les references a `src/openhuman/providers/`** dans les docs (ce domaine n'existe plus).
10. **Reducer `gitbooks/developing/architecture/frontend.md`** (2,302 lignes) ou la decomposer.

### LOW (bonnes pratiques)

11. **Ajouter un hook Prettier PostToolUse** dans `.claude/settings.json` pour formater automatiquement.
12. **Verifier que tous les fichiers `app/src/`** utilisent `useT()` pour le texte UI (scan automatique).
13. **Ajouter un badge de couverture de documentation** dans le README.
14. **Creer un script `pnpm docs:audit`** pour verifier la coherence docs/code automatiquement.

---

## Fichiers Impactes (Read-Only, Non Modifies)

| Categorie | Fichiers |
|-----------|----------|
| Core doc | `CLAUDE.md` (880L), `AGENTS.md` (656L) |
| READMEs | `README.md`, `README.{de,ja-JP,ko,zh-CN}.md`, `app/README.md` |
| Contrib | `CONTRIBUTING.md`, `CONTRIBUTING-BEGINNERS.md` |
| Gitbooks | `gitbooks/developing/*.md` (30 fichiers) |
| Conventions | `IntelligenceSubconsciousTab.tsx`, `NotificationRoutingPanel.tsx`, `localAiBootstrap.ts` |
| i18n | 65 chunks + `en.ts` (13 locales) |
| Config | `.env.example`, `app/.env.example` |

---

*Fin du rapport d'audit. Aucun fichier n'a ete modifie.*

*Ce rapport a ete genere par analyse read-only du code source, des fichiers de documentation et des conventions enumces dans les regles du projet.*
