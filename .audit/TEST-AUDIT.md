# Audit de couverture de tests — DADOU (OpenHuman)

**Date :** 2026-06-16  
**Perimetre :** Rust core (`src/` + `tests/`) + TypeScript/React (`app/src/`) + E2E (`app/test/e2e/`)  
**Methode :** Read-only. Execution des suites de test + analyse statique des fichiers.

---

## Score global : 6.5 / 10

| Critere | Note | Commentaire |
|---------|------|-------------|
| Volume de tests | 9/10 | ~13 600 tests ecrits (Rust + TS) |
| Execution Rust | 2/10 | Ne compile pas — 40 erreurs bloquantes |
| Execution TS/Vitest | 8/10 | 3 579 tests, 1 seul echec (port busy) |
| Couverture par domaine | 5/10 | 47 domaines Rust sans aucun test |
| Qualite des tests | 7/10 | Bons echantillons, assertions presentes |
| Tests E2E | 8/10 | 89 specs maintenues, helpers dedies |
| Tests ignores/ralentis | 5/10 | Tests lents marques `#[ignore]`, pas de CI les executant |

---

## 1. Tests Rust (`cargo test`)

### 1.1 Volume

| Metrique | Valeur |
|----------|--------|
| Fichiers source `.rs` (hors tests) | 1 272 |
| Fichiers de test dans `src/` | 228 (219 dans `src/openhuman/`, 9 ailleurs) |
| Fichiers de test dans `tests/` | 34 |
| Fonctions `#[test]` dans `src/openhuman/` | 7 800 |
| Fonctions `#[tokio::test]` dans `src/openhuman/` | 2 185 |
| **Total fonctions de test Rust** | **~10 173** (9 985 in-domain + 188 integration) |
| Lignes source Rust | 97 421 |
| Lignes de test Rust | 101 183 |
| Ratio test/source (lignes) | **104 %** — plus de code de test que de code source |
| Assertions `assert!`/`assert_eq!`/`assert_ne!` | 11 248 |

### 1.2 Execution : ECHEC — 40 erreurs de compilation

La suite de test Rust **ne compile pas**. Les erreurs se repartissent ainsi :

| Code erreur | Occurrences | Signification probable |
|-------------|-------------|----------------------|
| `E0063` | 17 | Fonction/item manquant dans une enum (champ missing) |
| `E0433` | 4 | Module ou type non declare |
| `E0658` | 2 | Utilisation de fonctionnalite instable |
| `E0282` | 2 | Annotation de type necessaire |
| `E0728` | 1 | `await` dans un contexte non async |
| `E0609` | 1 | Champ non trouve sur la structure |
| `E0599` | 1 | Methode non trouvee |
| `E0596` | 1 | Emprunt mutable alors que non `mut` |
| `E0428` | 1 | Module ou type duplique |
| `E0422` | 1 | Structure inexistante ou incompletter |
| `E0308` | 1 | Mismatch de type |
| `E0277` | 1 | Trait non implemente |
| `E0053` | 1 | Signature de methode incompatible |

**Impact :** La totalite des tests Rust est inexecutable. Cela signifie que :
- Aucun test Rust n'est verifie en CI (contredit CLAUDE.md section "Coverage requirement")
- Les PR peuvent etre mergees avec des tests casses
- La regression n'est pas detectee automatiquement

**Cause probable :** Changements dans les domaines (guardian/n3, rollback, inference) sans mise a jour des tests correspondants. Les erreurs E0063 (enum) suggerent un ajout de variant ou suppression de champ sans mise a jour des pattern match dans les tests.

### 1.3 Domaines sans tests (47/100 domaines)

| Domaine couvert | Domaine SANS test |
|-----------------|-------------------|
| about_app, accessibility, agent (24), app_state, autocomplete, billing, channels (23), composio (11), config (5), context (3), cost, credentials (3), cron (3), cwd_jail, desktop_companion (3), doctor, embeddings (2), http_host, inference (14), integrations (6), keyring (2), learning (5), meet, meet_agent, memory (7), memory_conversations, memory_queue, memory_store (9), memory_sync (9), memory_tools (2), memory_tree (7), migrations (4), notifications (2), people, prompt_injection, routing, runtime_python (3), screen_intelligence (3), security (2), service, skills (3), socket, subconscious (6), team, threads (4), tokenjuice (4), tools (18), update, vault, voice (3), wallet, webhooks (4), webview_accounts, whatsapp_data (2) | agent_experience, agent_tool_policy, anti_injection, approval, audio_toolkit, connectivity, dashboard, devices, encryption, guardian, health, heartbeat, javascript, mcp_audit, mcp_client, mcp_registry, mcp_server, memory_archivist, memory_entities, memory_graph, migration, overlay, provider_surfaces, redirect_links, referral, rollback, runtime_node, scheduler_gate, semantic_router, session_context, startup, test_support, text_input, tls, todos, tool_registry, tool_timeout, webview_apis, webview_notifications, workspace |

**Parmi les 47 domaines non couverts, les plus critiques :**
- **`guardian`** — module de securite critique (N1/N2/N3)
- **`approval`** — flux d'approbation utilisateur
- **`encryption`** — chiffrement (tunnel, credentials)
- **`mcp_registry`**, **`mcp_client`**, **`mcp_server`** — protocole MCP
- **`devices`** — iOS device linking
- **`scheduler_gate`** — ordonnancement (critique metier)
- **`tool_registry`**, **`tool_timeout`** — systeme de tools
- **`audio_toolkit`** — fonctionnalite audio core

### 1.4 Tests ignores (`#[ignore]`)

Quelques tests d'integration sont marques `#[ignore]` :
- `cwd_jail_e2e.rs` — Windows AppContainer
- `live_routing_e2e.rs` — necessite infra externe
- `memory_graph_sync_e2e.rs` — lent (SQLite + ingestion)
- `subconscious_e2e.rs` — necessite Ollama en cours

### 1.5 Echantillons qualite Rust

**`security/policy_tests.rs`** (145 tests) :
- Helper functions (fixtures) bien factorisees (`default_policy()`, `readonly_policy()`)
- Tests nommes de facon descriptive (`autonomy_default_is_supervised`)
- Assertions presentes (`assert!`, `assert_eq!`)
- Couvre tous les niveaux d'autonomie et cas limites
- **Qualite : Bonne**

**`inference/provider/factory_test.rs`** :
- Utilise `tempfile::TempDir` pour isolation
- Pattern arrange/factories bien defini
- Tests parametriques via helpers
- **Qualite : Bonne**

**`rollback/ops.rs`** (tests inline) :
- Variables inutilisees (`entry1`, `ts`, `link`) suggerent des tests incomplets ou du code mort
- **Qualite : Passable**

---

## 2. Tests TypeScript/React (Vitest)

### 2.1 Volume

| Metrique | Valeur |
|----------|--------|
| Fichiers source `.ts`/`.tsx` | 596 |
| Fichiers de test `.test.ts`/`.test.tsx` | 356 |
| **Ratio test/source** | **60 %** |
| Fonctions `it()` | 2 993 |
| Fonctions `test()` | 324 |
| **Total tests (approx.)** | **~3 300** (plus `describe()` pour organisation) |
| Assertions `expect()` | 6 610 |
| Duree execution | 1 778s (~30 min) |

### 2.2 Execution : 3 579 tests — 1 echec

| Statut | Nombre |
|--------|--------|
| Test files passed | 362 |
| Test files failed | 1 |
| Test files skipped | 1 |
| **Total test files** | **364** |
| Tests passed | 3 575 |
| Tests failed | 1 |
| Tests skipped | 3 |
| **Total tests** | **3 579** |

**Echec unique :** `src/test/mockApiCore.portSelection.test.ts` — `EADDRINUSE: address already in use 127.0.0.1:5005`

Environnement : port 5005 deja occupe par un processus anterieur. Non bloquant, erreur de concurrence sur le port de mock.

### 2.3 Repartition par repertoire

| Repertoire | Tests | Commentaire |
|------------|-------|-------------|
| `src/components/` | 137 | Couverture bonne, composants isoles |
| `src/pages/` | 34 | Pages principales couvertes |
| `src/store/` | 24 | Slices Redux testes |
| `src/services/` | 54 | API + services couverts |
| `src/hooks/` | 13 | Hooks personnalises |
| `src/lib/` | 29 | Utilitaires et lib |
| `src/providers/` | 4 | Providers React |
| `src/utils/` | 34 | Fonctions utilitaires |

**Sous-repertoires sans test :**
- `src/components/webhooks/` — zero test
- `src/lib/ai/` — zero test

### 2.4 Echantillons qualite TypeScript

**`BootCheckGate.test.tsx`** :
- Mocking exhaustif des dependances Tauri (isTauri, runBootCheck, etc.)
- Tests organises en `describe()` avec noms clairs
- Couvre les etats : picker, core mode set, port conflict, echec
- Utilise `waitFor`, `fireEvent` pour simuler les interactions
- **Qualite : Excellente**

**`agentProfilesApi.test.ts`** :
- Mock du RPC client via `vi.fn()`
- Reset d'etat entre les tests (`beforeEach`)
- Tests de list/select/upsert/delete
- Verifie les appels RPC exacts (`toHaveBeenCalledWith`)
- **Qualite : Bonne**

**`semver.test.ts`** :
- Tests de fonctions pures (parse, compare)
- Couvre les cas limites (malformed, suffixes, v-prefix)
- Structure simple et lisible
- **Qualite : Bonne**

---

## 3. Tests E2E

| Metrique | Valeur |
|----------|--------|
| Specs E2E | 89 fichiers `.spec.ts` |
| Fichiers helpers | 16 |
| Config | `app/test/wdio.conf.ts` |
| Mock server | `app/test/e2e/mock-server.ts` |

**Thematiques couvertes :**
- Authentication (login, logout, onboarding)
- Chat harness (send/stream, cancel, sub-agent, wallet flow, scroll/render)
- Connecteurs (GitHub, Gmail, Google Calendar, Slack, Discord, Notion, Jira, Airtable, Asana, ClickUp, Confluence, Todoist, YouTube, Google Sheets, Google Drive)
- Channels (Telegram, WhatsApp, web channel)
- Cron jobs, webhooks
- Skills (execution, lifecycle, multi-round, OAuth, registry)
- Voice mode, screen intelligence
- Settings (account, advanced, channels, data, dev options, features)
- Navigation, notifications, rewards
- Memoire (roundtrip), browser tool, filesystem tool

**Couverture E2E :** Tres large. Les 89 specs couvrent la majorite des parcours utilisateur critiques.

---

## 4. Synthese et recommandations

### Forces
- Volume de tests impressionnant (>13 600 tests ecrits)
- Ratio test/source excellent (plus de code de test que de code source en Rust)
- Tests TypeScript bien ecrits avec assertions veritables
- Couverture E2E exhaustive (89 specs, 16 helpers)
- Infrastructure de test presente (mock server, CI scripts, configs)

### Faiblesses
1. **CRITIQUE : Tests Rust ne compilent pas** — 40 erreurs. La totalite du bedrock de tests est inactive.
2. **47 domaines Rust sans aucun test** — modules entiers non couverts, dont des modules critiques (guardian, approval, encryption, mcp*)
3. **Un seul test Vitest echoue** mais signale un probleme d'isolation (port reuse)
4. **Tests Rust lents marques `#[ignore]`** — jamais executes en CI standard
5. **Pas de coverage exécute** — les seuils dans vitest.config.ts sont commentes

### Recommandations priorisees

**Priorite 1 (immédiat) :**
- [ ] Corriger les 40 erreurs de compilation des tests Rust. Commencer par les `E0063` (17 erreurs, probablement des pattern match non exhaustifs suite a ajout de variants d'enum)
- [ ] Debloquer la CI Rust (`pnpm test:rust`) — elle est probablement cassee depuis l'ajout du module guardian/n3

**Priorite 2 (haute) :**
- [ ] Ajouter des tests unitaires pour les 5 domaines critiques sans couverture : `guardian`, `approval`, `encryption`, `mcp_registry`, `tool_registry`
- [ ] Activer les seuils de coverage dans `vitest.config.ts` (decommenter le bloc `thresholds`)
- [ ] Ajouter `webhooks/` et `lib/ai/` a la couverture Vitest

**Priorite 3 (moyenne) :**
- [ ] Executer les tests `#[ignore]` periodiquement (CI hebdomadaire ou manuelle)
- [ ] Resoudre le conflit de port 5005 dans `mockApiCore.portSelection.test.ts`
- [ ] Nettoyer les variables inutilisees dans les tests Rust (rollback, security, inference)
- [ ] Reduire le temps d'execution Vitest (1 778s) — parallelisation ou splitting

**Priorite 4 (basse) :**
- [ ] Couvrir les 47 domaines restants avec au moins un test de smoke par domaine
- [ ] Ajouter des tests d'integration pour les nouveaux modules (memory_archivist, memory_entities, memory_graph, semantic_router, session_context)
- [ ] Verifier la couverture des tests E2E sur Windows (environnement de dev principal)
