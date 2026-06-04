# DADOU

## What This Is

DADOU est un assistant IA personnel et autonome qui fonctionne localement sur votre machine. Il pilote des logiciels, écrit du code, rédige des documents, exécute des compétences tierces sandboxées, et surtout — **se souvient**. Sa mémoire persistante lui donne une compréhension globale de vos projets, vos préférences, et son propre historique d'actions, lui permettant de s'améliorer session après session.

D'abord conçu comme un assistant mono-utilisateur taillé à son environnement, DADOU est open source (GPL-3.0) et vise une communauté de power users, développeurs et makers via son système modulaire de compétences.

## Core Value

**Un assistant qui apprend.** Là où les autres IA repartent de zéro à chaque session, DADOU construit et maintient un modèle mental persistant de votre monde numérique — projets, préférences, erreurs passées, succès. Il ne se contente pas de répondre, il s'améliore.

## Requirements

### Validated

- ✓ Agent IA conversationnel avec outils (shell, fichiers, navigateur, réseau) — hérité d'OpenHuman v0.56.0
- ✓ Desktop app cross-platform (Windows, macOS, Linux) via Tauri v2 + CEF — hérité
- ✓ JSON-RPC core avec controller registry, event bus, domaines modulaires — hérité
- ✓ Pipeline mémoire existant (memory graph, entities, conversations, tree summarizer) — hérité
- ✓ Intégrations LLM (Ollama, OpenAI-compatible, cloud providers) — hérité
- ✓ Système de tools extensible (filesystem, network, system, browser, wallet) — hérité
- ✓ Agents, cron, webhooks, socket.io, channels — hérité

### Active

- [ ] **MEM-01**: DADOU maintient un contexte global de projet au-delà du fichier courant (anti-vue-tunnel)
- [ ] **MEM-02**: DADOU se souvient des corrections et préférences utilisateur entre sessions
- [ ] **MEM-03**: DADOU détecte les contradictions entre nouveaux souvenirs et souvenirs vérifiés, et demande confirmation
- [ ] **MEM-04**: Mémoire avec provenance (source, niveau de confiance : verified > inferred > external) et decay automatique
- [ ] **GRD-01**: Guardian N1 — règles déterministes de validation des actions (path whitelist, regex, blocklist)
- [ ] **GRD-02**: Guardian N2 — classifieur local de dangerosité (détection patterns d'exfiltration, prompts cachés)
- [ ] **GRD-03**: Guardian N3 — LLM léger de validation pour les plans ambigus (escalade uniquement)
- [ ] **SKL-01**: Système de compétences : manifeste dadou-skill.yaml, dépôt Git autonome par compétence
- [ ] **SKL-02**: Runtime sandboxé WASM pour compétences légères (pas de conteneur requis)
- [ ] **SKL-03**: Runtime Python sandboxé (Docker rootless / Podman / nsjail) pour compétences complexes
- [ ] **SKL-04**: Signature GPG des compétences, permissions déclarées et vérifiées dans le manifeste
- [ ] **SKL-05**: Analyse statique avant activation (détection imports suspects, eval, subprocess)
- [ ] **SKL-06**: Store local TOML listant compétences installées (version, hash, état activé/désactivé)
- [ ] **SKL-07**: Commandes CLI : install, update, audit, remove
- [ ] **INJ-01**: Balisage strict des données externes dans le prompt système (`<external_data trusted="false">`)
- [ ] **INJ-02**: Jamais de concaténation directe du contenu d'une compétence dans le prompt — passage par JSON structuré
- [ ] **INJ-03**: Validation sémantique de sortie : toute réponse d'une compétence est scrutée avant réinjection
- [ ] **INJ-04**: Le LLM planificateur n'a pas d'accès direct à l'exécution — plans JSON validés par le Guardian
- [ ] **UND-01**: Rollback/undo des modifications fichiers (historique horodaté avec diffs)
- [ ] **UND-02**: Commandes dadou undo --last, dadou undo --before <timestamp>
- [ ] **RTR-01**: Routeur sémantique local pour la découverte de compétences (embedding → top-3 skills)
- [ ] **OBS-01**: Dashboard locale (localhost:7790) : skills actives, actions Guardian, graphe mémoire, historique
- [ ] **CTX-01**: Continuité inter-session : contexte courant et actions en attente sauvegardés au restart
- [ ] **CTX-02**: Au redémarrage, DADOU sait quel projet était en cours et à quelle phase

### Out of Scope

- Multi-utilisateur sur une même instance — DADOU est mono-utilisateur par conception (chaque utilisateur a sa propre instance)
- Android/iOS natif — le client iOS expérimental d'OpenHuman n'est pas une priorité DADOU
- Mode SaaS / cloud hébergé — DADOU est local-first
- Marketplace de compétences avec paiement — v1 gratuite et communautaire
- Chiffrement homomorphe des souvenirs — trop coûteux pour le cas d'usage actuel

## Context

DADOU est un fork indépendant d'OpenHuman v0.56.0, un assistant IA desktop open source (React + Tauri v2 + Rust core, licence GPL-3.0). Le codebase existant (~80 domaines Rust, ~1100 packages npm, 60+ composants React) fournit une base solide : agents, outils, mémoire, intégrations LLM, canaux de communication.

L'utilisateur cible est un développeur multipotentiel (tech, stratégie, droit, économie) qui travaille sur Windows et a besoin d'un assistant capable de comprendre l'ensemble de son contexte de travail, pas juste le fichier ouvert.

La force d'OpenHuman est son architecture modulaire (controller registry, event bus, transport JSON-RPC). La faiblesse pour le cas DADOU est l'absence de persistance du contexte global entre sessions et l'absence de sandboxing pour l'exécution de code tiers.

Le `.claude/memory.md` (2912 lignes) documente en détail les bugs connus, les patterns, et les zones fragiles du codebase. La cartographie dans `.planning/codebase/` (7 documents, 2198 lignes) fournit une référence technique complète.

## Constraints

- **Langage**: Rust 1.93 (core), TypeScript 5.8 + React 19 (frontend), Tauri v2 (desktop shell)
- **Package manager**: pnpm 10.10.0, Node ≥24
- **Desktop runtime**: Tauri v2 avec CEF Chromium (fork vendu `feat/cef`)
- **Licence**: GPL-3.0 (contrainte forte — tout fork doit rester sous GPL-3.0)
- **Build**: whisper-rs/llama.cpp bloquent sur macOS Tahoe (GGML_NATIVE=OFF requis), CI upstream cassée (5 tests Vitest + 4 erreurs TS)
- **Sécurité**: Pas d'exécution directe par le LLM — tout passe par le Guardian. Skills sandboxées. Injection IA traitée comme menace de premier ordre.
- **Performance**: Le Guardian N3 (LLM) ne doit pas ajouter > 500ms de latence aux actions courantes. N1+N2 visent < 10ms.
- **Windows-first**: Le développement et les tests se font d'abord sur Windows 11. macOS et Linux suivent.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Fork indépendant, pas contribution upstream | Objectifs divergents (mémoire, skills, sécurité) vs upstream (communautés) | — Pending |
| Architecture hybride : base OpenHuman + couche DADOU | Évite de réécrire 80 domaines, permet itération rapide sur les couches nouvelles | — Pending |
| Guardian 3 niveaux (N1 règles + N2 classifieur + N3 LLM) | 95% des actions validées en < 10ms, escalade LLM uniquement pour cas ambigus | — Pending |
| WASM + Python pour les skills (pas Python uniquement) | WASM = sandbox natif sans conteneur, démarrage instantané, multi-langage | — Pending |
| Mono-utilisateur local-first | Complexité multi-utilisateur non justifiée en v1, aligné avec usage personnel | — Pending |
| Mémoire à provenance et confiance | Permet de distinguer souvenirs vérifiés vs inférés, évite corruption par données externes | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd:complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-06-04 after initialization*
