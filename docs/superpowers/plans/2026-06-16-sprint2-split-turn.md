# Split turn.rs — Implementation Plan

> **Goal:** Découper turn.rs (2475L) en 6 modules <500L sans changer la logique

**Architecture:** Extraction de `impl Agent` blocks vers des fichiers séparés dans `session/`

---
### Task 1: Extraire `turn_system_prompt.rs`

- Extraire `build_system_prompt()` (L2026-2131, ~105L)
- Ajouter `mod turn_system_prompt;` dans `session/mod.rs`
- Commit: `refactor: extract build_system_prompt to turn_system_prompt.rs`

### Task 2: Extraire `turn_integrations.rs`

- Extraire `fetch_connected_integrations()` + `refresh_delegation_tools()` (L1842-2026, ~184L)
- Commit: `refactor: extract integrations methods to turn_integrations.rs`

### Task 3: Extraire `turn_progress.rs`

- Extraire `emit_progress()` + `summarize_iteration_checkpoint()` (L1585-1842 + L2131-2476)
- Commit: `refactor: extract progress methods to turn_progress.rs`

### Task 4: Extraire `turn_context.rs`

- Extraire `inject_agent_experience_context()` (L1185-1585, ~400L)
- Commit: `refactor: extract experience context to turn_context.rs`

### Task 5: Nettoyer `turn.rs`

- Garder uniquement `turn()` + imports nécessaires (~150L)
- Commit: `refactor: trim turn.rs to core turn() method`

### Vérification: `cargo check` après chaque tâche
