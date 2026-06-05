# Phase 4: Skills WASM — Context

**Gathered:** 2026-06-05
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 4 ajoute un système complet de skills WASM exécutables à DADOU, en s'appuyant sur le domaine `src/openhuman/skills/` existant (devenu metadata-only après la suppression du runtime QuickJS).

**Ce qui existe déjà (hérité d'OpenHuman — réutilisé, pas recréé):**
- `src/openhuman/skills/mod.rs` — Module structure, exports
- `src/openhuman/skills/types.rs` — `SkillFrontmatter`, `Skill`, `SkillScope` (format SKILL.md agentskills.io)
- `src/openhuman/skills/ops_types.rs` — Core type helpers, constants
- `src/openhuman/skills/ops_parse.rs` — SKILL.md parsing (YAML frontmatter + body)
- `src/openhuman/skills/ops_discover.rs` — Skill discovery (user + workspace scopes)
- `src/openhuman/skills/ops_install.rs` — URL-based SKILL.md install via HTTPS
- `src/openhuman/skills/ops_create.rs` — SKILL.md scaffolding
- `src/openhuman/skills/inject.rs` — SKILL.md body injection into agent loop (match heuristic, render)
- `src/openhuman/skills/schemas.rs` — 5 controllers: list, read_resource, create, install_from_url, uninstall
- `src/openhuman/skills/bus.rs` — Legacy no-op bus hook
- `src/openhuman/skills/ops_tests.rs`, `schemas_tests.rs` — Tests existants
- `src/core/all.rs` — Wiring: `all_skills_registered_controllers()` et `all_skills_controller_schemas()`

**Ce qui est NOUVEAU pour Phase 4 (système DADOU — format `dadou-skill.yaml` au lieu de SKILL.md):**
- Format de manifeste `dadou-skill.yaml` avec nom, version, auteur, signature GPG, dépendances, permissions
- Store local TOML (`~/.openhuman/skills/store.toml`) pour état des skills installées
- Runtime WASM in-process via Wasmtime avec WASI capability-gated
- Vérification GPG des tags signés via sequoia-openpgp
- Analyse statique avant activation (imports suspects, écriture sandbox)
- CLI `dadou skill install|update|audit|remove`
- Nouveaux contrôleurs JSON-RPC pour la gestion du cycle de vie

**Relation entre l'ancien et le nouveau système :**
- L'ancien système SKILL.md (prompts textuels) reste intact — découverte, catalogage, injection dans le prompt LLM
- Le nouveau système DADOU (skills WASM) est un sous-domaine parallèle qui ajoute l'exécution sandboxée
- Un skill DADOU peut aussi avoir un SKILL.md pour la documentation/description, mais le point d'entrée est le WASM
</domain>

<decisions>
## Implementation Decisions

### D-44: Structure du manifeste `dadou-skill.yaml`
- Format YAML structuré avec sections obligatoires et optionnelles
- Obligatoire: `name`, `version`, `wasm.path`, `wasm.entry`
- Optionnel: `author`, `description`, `permissions.filesystem.read`, `permissions.filesystem.write`, `permissions.network`, `gpg.fingerprint`
- Pas de permission `network` par défaut (fail-closed : pas de réseau sans déclaration explicite)
- Validation stricte: rejet immédiat si champ obligatoire manquant
- Stocké à la racine du dépôt Git du skill, à côté du binaire WASM

### D-45: Store local TOML
- Fichier: `~/.openhuman/skills/store.toml`
- Table `[skills]` avec clé = nom du skill, valeur = table d'état
- Champs par skill: `version`, `commit_hash`, `enabled` (bool), `gpg_fingerprint` (optionnel), `installed_at` (timestamp ISO 8601), `last_audit_at` (timestamp optionnel), `audit_result` (optionnel: "pass"|"fail"|"not_audited")
- Opérations atomiques: lecture → modification → écriture (pas de lock pour v1, fichier < 100 skills typique)
- Helper `SkillsStore` struct avec `load()`, `save()`, `get()`, `set()`, `remove()`

### D-46: Runtime Wasmtime in-process
- Wasmtime embarqué directement dans le processus DADOU (pas de sidecar)
- WASI context avec capabilities réduites:
  - `wasi:filesystem` : restreint à `~/.openhuman/skills/<name>/data/` uniquement
  - `wasi:env` : vide (pas de variables d'environnement exposées)
  - `wasi:clock` : limité à la clock monotonic (pas de wall clock)
  - `wasi:random` : désactivé (pas de random pour éviter l'entropie de probing)
  - `wasi:tcp`, `wasi:udp`, `wasi:http` : TOUS désactivés (pas de réseau)
- Timeout 30s par exécution, implémenté via `wasmtime::Store::set_epoch_deadline`
- Pool de `wasmtime::Engine` réutilisé (créé une fois au démarrage)
- API: `execute_wasm_skill(name: &str, input: &[u8]) -> Result<Vec<u8>, ExecutionError>`

### D-47: Vérification GPG via sequoia-openpgp
- Clés publiques des auteurs de confiance stockées dans `~/.openhuman/skisters/openpgp/certs/` (format OpenPGP cert)
- `sequoia-openpgp` en mode crypto-rust (pas de dépendance au GPG binaire système)
- Vérification effectuée sur le tag Git signé du dépôt distant
- Fonctionnement: `git fetch` l'objet tag → extrait la signature → vérifie avec sequoia
- Si la signature est invalide ou l'auteur pas dans la keyring de confiance → installation bloquée avec message d'erreur
- Gestion du premier ajout: `dadou skill trust-author <fingerprint>` pour ajouter une clé

### D-48: Analyse statique
- Analyse du code source (Rust, TypeScript, Python, etc.) avant activation
- Règles de détection:
  - Imports suspects: `os`, `subprocess`, `socket`, `eval`, `exec`, `requests` (HTTP)
  - Appels système: `std::process::Command`, `std::fs::write` hors répertoires autorisés
  - Patterns réseau: `TcpStream`, `connect()`, `http::`, `curl`, `wget`
- Mode opératoire: scan des fichiers sources dans `~/.openhuman/skills/<name>/src/`
- Résultat: `pass` (aucun pattern suspect), `warn` (patterns à risque mais non-bloquants), `block` (pattern dangereux)
- Un résultat `block` empêche l'activation du skill
- L'analyse est stockée dans le TOML store (`audit_result`)

### D-49: CLI `dadou skill` — architecture
- Pas de nouveau binaire — s'intègre dans le CLI existant `openhuman-core`
- Sous-commandes:
  - `dadou skill install <git-url>` — Clone le dépôt, lit le manifeste, vérifie GPG, analyse statique, compile WASM si nécessaire, installe dans le store
  - `dadou skill update <name>` — Git pull, reverrouille GPG, ré-analyse, met à jour le store
  - `dadou skill audit <name>` — Ré-analyse statique, met à jour le store
  - `dadou skill remove <name>` — Désinstalle, supprime les fichiers
  - `dadou skill list` — Liste les skills installées avec leur état
  - `dadou skill trust-author <fingerprint>` — Ajoute une clé GPG aux auteurs de confiance
- Implémenté comme un sous-domaine `src/openhuman/skills/cli.rs` dispatché via `run_namespace_command` + `"skill"` subcommand registré dans `src/core/cli.rs`, ou via un sous-commande dédiée

### D-50: Nouveaux contrôleurs JSON-RPC
- `dadou.skill_install` — Installer un skill depuis un dépôt Git
- `dadou.skill_update` — Mettre à jour un skill existant
- `dadou.skill_audit` — Auditer un skill installé
- `dadou.skill_remove` — Désinstaller un skill
- `dadou.skill_list` — Lister les skills avec leur état depuis le store
- Préfixe `dadou.` pour distinguer des contrôleurs `skills.*` hérités (SKILL.md metadata)

### Claude's Discretion
- Seuils exacts de détection pour l'analyse statique (liste des patterns suspects)
- Version exacte de wasmtime et sequoia-openpgp à ajouter dans Cargo.toml (vérifier crates.io)
- Structure exacte du répertoire de données par skill (`~/.openhuman/skills/<name>/data/`)
- Règles précises de permission filesystem pour WASI (chemins autorisés supplémentaires)
- Mécanisme de compilation Rust → WASM (via cargo-wasi ou bundle pré-compilé)
- Choix entre sous-commande dédiée et namespace pour la CLI
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Domaine skills existant (base à étendre)
- `src/openhuman/skills/mod.rs` — Structure du module, re-exports
- `src/openhuman/skills/types.rs` — `ToolResult`, `ToolContent` (à ne pas confondre avec les nouveaux types)
- `src/openhuman/skills/ops_types.rs` — `SkillFrontmatter`, `Skill`, `SkillScope` (format existant)
- `src/openhuman/skills/ops_parse.rs` — Parsing SKILL.md (pattern de lecture fichier → struct)
- `src/openhuman/skills/ops_install.rs` — Pattern d'installation: validation → fetch → write → rediscover
- `src/openhuman/skills/schemas.rs` — Pattern controleurs: schemas + handlers + registre
- `src/openhuman/skills/ops_tests.rs` — Pattern tests unitaires skills

### Controller registry
- `src/core/all.rs` — `build_registered_controllers()` + `build_controller_schemas()`
- `src/core/mod.rs` — `ControllerSchema`, `FieldSchema`, `TypeSchema`

### CLI dispatch
- `src/core/cli.rs` — `run_from_cli_args()`, pattern sous-commandes (`undo`, `agent`)
- `src/core/agent_cli.rs` — Exemple de CLI sub-module

### Event bus (pour futurs événements de cycle de vie)
- `src/core/event_bus/events.rs` — DomainEvent enum
- `src/core/event_bus/bus.rs` — publish_global, subscribe_global

### Configuration
- `src/openhuman/config/schema/types.rs` — Config struct (pattern pour ajouter section skills si nécessaire)
- `Cargo.toml` (root) — Dépendances Rust, regénérée si wasmtime/sequoia ajoutés

### Phase 1 — SecurityFoundation (dépendance)
- `src/openhuman/guardian/n1/` — Guardian N1 (base de validation)
- `.planning/phases/01-security-foundation/01-CONTEXT.md` — Décisions D-01 à D-15
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **Domaine skills existant** (`src/openhuman/skills/`): Types `Skill`, `SkillScope`, fonctions `discover_skills()`, `is_workspace_trusted()`
- **Controller registry** (`src/core/all.rs`): `all_skills_registered_controllers()` déjà câblé
- **CLI dispatch** (`src/core/cli.rs`): Pattern `run_from_cli_args()`, namespace dispatch
- **`toml` crate** (déjà dans Cargo.toml): "1.0" — utilisable pour le store TOML sans nouvelle dépendance
- **`dirs` crate** (déjà dans Cargo.toml): Résolution de `~/.openhuman/` existante
- **`reqwest` + `tokio`** (déjà dans Cargo.toml): Fetch Git repos, HTTP requests
- **`regex`** (déjà dans Cargo.toml): Patterns d'analyse statique
- **`log`/`tracing`** (déjà dans Cargo.toml): Logging structuré

### Nouvelles Dépendances Requises
- **wasmtime**: Runtime WASM — vérifier version sur crates.io, ajouter à Cargo.toml (features: wasi, default)
- **sequoia-openpgp**: Vérification GPG — version stable, feature `crypto-rust`
- **git2** (ou commande git via `std::process::Command`): Fetch tags Git, clone repositories
- Potentiellement: `walkdir` (déjà présent) pour l'analyse statique récursive

### Integration Points
- **`src/openhuman/skills/mod.rs`**: Ajouter les nouveaux sous-modules (manifest, store, wasm, verify, static_analysis, cli)
- **`src/openhuman/skills/schemas.rs`**: Ajouter 5 nouveaux contrôleurs préfixés `dadou.*`
- **`src/core/all.rs`**: Ajouter les nouveaux contrôleurs dans les deux registres
- **`src/core/cli.rs`**: Ajouter sous-commande `"skill"` (ou namespace)
- **`~/.openhuman/skills/store.toml`**: Nouveau fichier store (pas dans la codebase, créé au runtime)
- **`~/.openhuman/skills/certs/`**: Nouveau répertoire pour clés GPG (créé au premier trust-author)
- **`Cargo.toml`**: Ajouter wasmtime et sequoia-openpgp

### Established Patterns (from Phase 1-3)
- Domain pattern: `mod.rs` + ops.rs + types.rs + schemas.rs + bus.rs (optionnel)
- Controller schema: `ControllerSchema` dans schemas.rs, enregistré dans `all_registered_controllers`
- RpcOutcome<T>: Pattern de retour standard pour toutes les RPC
- Fichiers dans `src/openhuman/`: Nouveaux sous-domaines dans répertoires dédiés
- Logging: `tracing::debug!` / `tracing::info!` avec préfixes `[skills-wasm]` / `[skill-store]` etc.
</code_context>

<specifics>
## Specific Ideas

- **Manifest minimal**: 10-15 lignes YAML, validation stricte, rejet si champs obligatoires manquants
- **Store TOML**: Fichier unique `~/.openhuman/skills/store.toml`, lu/écrit par `SkillsStore` struct
- **WASI gating**: Approche "deny-by-default": tout ce qui n'est pas explicitement autorisé est interdit
- **GPG vérification**: `sequoia-openpgp` en pur Rust, keyring dans `~/.openhuman/skills/certs/`
- **Analyse statique**: Scan récursif des fichiers source, pattern matching sur le texte (pas d'AST requirement pour v1)
- **CLI**: Structure sous-commande `install`, `update`, `audit`, `remove`, `list`, `trust-author`
- **Git fetch**: Utiliser la commande `git` système (pas libgit2 pour éviter dépendance complexe) via `std::process::Command`
- **Sandbox filesystem**: Chaque skill écrit dans `~/.openhuman/skills/<name>/data/` uniquement
- **Epoch-based timeout**: `wasmtime::Store::set_epoch_deadline(30)` + Engine config `epoch_interruption: true`
</specifics>

<deferred>
## Deferred Ideas

- Compilation Rust → WASM automatisée (cargo-wasi ou wasm-pack) — v2 : le skill est distribué pré-compilé en WASM
- Interface skills → LLM (skill retourne des données structurées consommées par le LLM) — Phase 5 (Anti-Injection)
- Dashboard UI pour voir les skills actives — Phase 6 (Dashboard)
- Semantic router pour découverte de skills par embedding — Phase 6
- Skills Python via Docker sidecar — Phase 7
- Skill registry communautaire avec review automatique CI — v2
- Marketplace / boutique de compétences — v2
- Délégation de confiance par auteur/organisation — v2
- Skills hybrides WASM → Python bridge — v2
- Mise à jour automatique des skills (cron) — v2
- Sandbox multi-process (isolation renforcée) — v2
</deferred>

---

*Phase: 04-skills-wasm*
*Context gathered: 2026-06-05*
