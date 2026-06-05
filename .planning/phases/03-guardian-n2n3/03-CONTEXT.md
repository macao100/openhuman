# Phase 3: Guardian N2+N3 — Context

**Gathered:** 2026-06-05
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 3 ajoute les deux niveaux superieurs du Guardian au pipeline N1 existant (Phase 1) :

- **N2** — Classifieur local de dangerosite : detecte les patterns d'exfiltration, les payloads caches, et les anomalies d'entropie dans les sorties et arguments des outils. Latence <10ms.
- **N3** — Validateur LLM leger : escalade uniquement quand N1+N2 ne peuvent pas decider (~2% des actions). Utilise le systeme d'inference existant (`local_ai_prompt`). Latence <500ms.

**Ce qui existe deja (Phase 1 — reutilise, pas recree):**
- `src/openhuman/guardian/pipeline.rs` — Pipeline N1 (GuardianN1 struct, evaluate(), global singleton OnceLock)
- `src/openhuman/guardian/types.rs` — RuleAction, RuleResult, RuleContext, N1Result, GuardianRule trait
- `src/openhuman/guardian/rules.rs` — PathWhitelistRule, RegexPatternRule, BlocklistRule, RuleSet, YAML loader
- `src/openhuman/guardian/schemas.rs` — 3 controleurs guardians (rules_list, rules_reload, evaluate)
- `src/openhuman/guardian/ops.rs` — Operations RPC
- `src/openhuman/guardian/bus.rs` — GuardianBlockingSubscriber
- `src/openhuman/agent/harness/tool_loop.rs` — Point d'interception N1 avant tool.execute()

**Ce qui est nouveau pour Phase 3:**
- N2: moteurs de detection (exfiltration, entropie, payload caches) dans `src/openhuman/guardian/n2/`
- N3: wrapper LLM leger de validation dans `src/openhuman/guardian/n3/`
- Pipeline etendu: `GuardianPipeline` qui combine N1 -> N2 -> N3
- Nouveaux types: `N2Result`, `N3Result`, `GuardianPipelineResult`
- Nouveaux evenements: `N2Blocked`, `N3Result`, `N2Escalated`
- Configuration: sections `[guardian_n2]` et `[guardian_n3]` dans config.toml
</domain>

<decisions>
## Implementation Decisions

### N2 — Architecture des detecteurs
- **D-32:** N2 est un sous-domaine `src/openhuman/guardian/n2/` avec module `mod.rs`, trois fichiers de detection (exfiltration, entropie, hidden_payloads), et un moteur `N2Engine` qui agrege les scores.
- **D-33:** Trois detecteurs N2:
  - **Exfiltration** (`exfiltration.rs`): regex patterns pour data URLs (`data:...;base64,...`), DNS exfiltration (`nslookup ... <domain>`), tunnels SSH/ngrok, reverse shells, curl vers IPs non-routables
  - **Entropie** (`entropy.rs`): calcul d'entropie Shannon sur les arguments outil et les commandes. Seuil haut (>4.5 bits/car) = suspect, tres haut (>6.0) = bloquant
  - **Hidden payloads** (`hidden_payloads.rs`): detection de base64 decode, hex decode, payloads encodés, eval/exec de code genere, steganographie basique
- **D-34:** Chaque detecteur retourne un `N2Score { score: f64 (0.0-1.0), reason: String, triggered_by: String }`. Le moteur N2 combine les scores: si ANY score > BLOCK_THRESHOLD (0.7) -> Block. Si ANY score > ESCALATE_THRESHOLD (0.3) -> Escalate to N3. Sinon -> Allow.

### N3 — Validateur LLM
- **D-35:** N3 est un sous-domaine `src/openhuman/guardian/n3/` avec module `mod.rs`, system prompt, et wrapper d'appel LLM.
- **D-36:** N3 utilise `inference::local::ops::local_ai_prompt()` pour appeler le modele LLM local configure. Pas de nouveau provider LLM — reuse l'infrastructure existante.
- **D-37:** Le system prompt N3 est un prompt de validation securitaire avec sortie JSON structuree: `{"verdict": "allow"|"block"|"uncertain", "reason": "..."}`. Le prompt demande au LLM de valider si le plan d'action est legitime ou malveillant.
- **D-38:** N3 a un cache LRU (taille 100) pour deduplicater les validations identiques dans la meme session.

### Pipeline etendu
- **D-39:** Nouveau type `GuardianPipelineResult` qui agrege `N1Result` + `Option<N2Result>` + `Option<N3Result>` + `final_allowed: bool` + `blocked_by: String` ("n1"|"n2"|"n3"|"none"). Sequential avec early exit: si N1 bloque, on retourne immediatement. Si N2 bloque, on retourne. N3 n'est appele que si N2 est incertain.
- **D-40:** L'escalade N2->N3 est declenchee quand `N2Result.escalate` est true. L'interception dans tool_loop.rs est modifiee pour appeler `GuardianPipeline::evaluate()` au lieu de `GuardianN1::evaluate()`.

### Configuration
- **D-41:** Sections dans config.toml:
  ```toml
  [guardian_n2]
  enabled = true
  block_threshold = 0.7
  escalate_threshold = 0.3
  max_tool_output_chars = 10000  # limite d'analyse pour performance

  [guardian_n3]
  enabled = true
  max_tokens = 256
  cache_size = 100
  timeout_ms = 450  # <500ms target
  ```
- **D-42:** Si N3 est disabled mais N2 escalade, l'action est bloquee (fail-closed). Si N2 est disabled, le pipeline saute directement a N3 pour les actions que N1 passe.

### Evenements
- **D-43:** Nouveaux variants `DomainEvent`:
  - `N2Blocked { tool_name, reason, scores, latency_us }` — quand N2 bloque une action
  - `N2Escalated { tool_name, scores, latency_us }` — quand N2 escalade vers N3
  - `N3Result { tool_name, verdict, reason, latency_us }` — resultat du LLM N3

### Claude's Discretion
- Seuils exacts d'entropie (4.5/6.0 recommande) — ajustables via config
- Liste exacte des patterns regex d'exfiltration
- Format exact du prompt systeme N3
- Taille du cache LRU (100 recommande)
- Timeout N3 (450ms recommande pour marge <500ms)
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Guardian N1 existant (base a etendre)
- `src/openhuman/guardian/mod.rs` — Structure du module, re-exports
- `src/openhuman/guardian/types.rs` — Types de base (RuleAction, N1Result, GuardianRule trait)
- `src/openhuman/guardian/pipeline.rs` — Pipeline N1, pattern OnceLock singleton
- `src/openhuman/guardian/schemas.rs` — Pattern controleur
- `src/openhuman/guardian/ops.rs` — Pattern operations RPC
- `src/openhuman/guardian/bus.rs` — Pattern event handler subscriber
- `src/openhuman/guardian/rules.rs` — Moteur de regles compilees

### Point d'interception
- `src/openhuman/agent/harness/tool_loop.rs:914-984` — Point N1 existant, la ou N2+N3 seront inseres

### Infrastructure LLM (pour N3)
- `src/openhuman/inference/ops.rs:52-70` — `inference_prompt()` — appel LLM local
- `src/openhuman/inference/local/ops.rs:186-201` — `local_ai_prompt()` — implementation locale
- `src/openhuman/inference/provider/traits.rs` — ChatMessage, Provider trait

### Event bus
- `src/core/event_bus/events.rs` — DomainEvent enum (pattern pour nouveaux variants)
- `src/core/event_bus/bus.rs` — publish_global, subscribe_global

### Controller registry
- `src/core/all.rs` — build_registered_controllers
- `src/core/mod.rs` — ControllerSchema, FieldSchema, TypeSchema

### Configuration (pattern)
- `src/openhuman/config/schema/types.rs` — Config struct (pattern pour ajouter sections guardian_n2/n3)
- `src/openhuman/config/schema/load.rs` — Chargement config avec env overrides

### Security (pattern existant)
- `src/openhuman/security/policy.rs` — SecurityPolicy, classify_command, gate_decision
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **GuardianN1 pipeline** (`guardian/pipeline.rs`): `evaluate()`, `init_global()`, `try_global()` — pattern a reproduire pour GuardianPipeline global
- **Inference system** (`inference/local/ops.rs`): `local_ai_prompt(config, prompt, max_tokens, no_think)` — fonction existante pour appeler le LLM local
- **Event bus** (`core/event_bus/`): Pattern pour ajouter N2Blocked, N3Result variants
- **Controller registry** (`core/all.rs`): Pattern pour enregistrer les controleurs N2/N3
- **OnceLock singleton** (`guardian/pipeline.rs`): Pattern pour le singleton global GuardianPipeline
- **Regex** (deja dans le workspace): Pour les patterns d'exfiltration N2
- **log/tracing** (deja dans le workspace): Logging structure

### Integration Points
- **tool_loop.rs:914-984**: Remplacer l'appel `GuardianN1::evaluate()` par `GuardianPipeline::evaluate()` qui execute N1 -> (si passe) -> N2 -> (si incertain) -> N3
- **events.rs**: Ajouter variants N2Blocked, N2Escalated, N3Result
- **guardian/bus.rs**: Ajouter N2BlockingSubscriber, N3ResultSubscriber
- **core/all.rs**: Ajouter controleurs guardian_n2/guardian_n3
- **config types.rs**: Ajouter sections `[guardian_n2]` et `[guardian_n3]`
- **guardian/mod.rs**: Ajouter sous-modules n2 et n3

### Established Patterns (from Phase 1 + 2)
- Domain pattern: `mod.rs` + types.rs + ops.rs + schemas.rs
- Controller schema: `ControllerSchema` dans schemas.rs, enregistre dans `all_registered_controllers`
- RpcOutcome<T>: Pattern de retour standard pour toutes les RPC
- OnceLock<Arc<T>>: Singleton global pour pipeline
- EventHandler trait: Subscriber avec domain filter
</code_context>

<specifics>
## Specific Ideas

- N2 detection via regex + entropie + heuristiques — pas de ML (trop lourd, dependance GPU)
- Score 0.0-1.0 avec deux seuils: block (0.7) et escalate (0.3)
- Escalade N3 = appel LLM local avec prompt specialise < 500ms
- Cache LRU N3 pour les patterns de validation frequents
- Les seuils sont configurables via config.toml
- Si N3 est desactive mais N2 incertain -> block (fail-closed)
- Structure de donnees pour N2Result: { allowed, escalate, scores: Vec<N2Score>, latency_us }
- Structure pour N3Result: { verdict, reason, latency_us }
- PipelineResult: { N1Result, Option<N2Result>, Option<N3Result>, allowed, blocked_by }
</specifics>

<deferred>
## Deferred Ideas

- N2 avec modele ML entraine (candle/ort) — trop couteux pour v1, les heuristiques suffisent
- Dashboard UI pour visualiser les decisions N2/N3 -> Phase 6 (Dashboard)
- Apprentissage automatique des patterns N2 -> v2
- N3 avec modele specialise fine-tune -> v2
- Analyse comportementale inter-session -> v2
- Mode "N3 toujours actif" (pour environnements haute securite) -> v2 (REQ-xx deferred)
</deferred>

---

*Phase: 03-guardian-n2n3*
*Context gathered: 2026-06-05*
