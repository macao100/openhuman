---
description: >-
  High-level shape of the OpenHuman system (desktop shell, Rust core, Memory
  Tree, agent loop). Pointer to the deep developer architecture in the repo.
icon: code-branch
---

# Architecture

OpenHuman is open-sourced under GNU GPL3. This page is the high-level shape of the system; the deep developer architecture lives in [deep architecture reference](../architecture.md) in the repo.

## The shape

OpenHuman is a **React + Tauri v2 desktop app** with a **Rust core** that does the heavy lifting.

```
┌──────────────────────────────────────────────────┐
│ Tauri shell (app/src-tauri/) │
│ • windowing, OS integration, sidecar lifecycle │
│ • CEF child webviews for integration providers │
└──────────────────────────────────────────────────┘
 │ JSON-RPC (HTTP) ↕
┌──────────────────────────────────────────────────┐
│ Rust core (`openhuman` binary, `src/`) │
│ • Memory Tree pipeline │
│ • Integration adapters + auto-fetch scheduler │
│ • Provider router (model routing) │
│ • TokenJuice compression │
│ • Native tools (search, fetch, fs, git, …) │
│ • Voice (STT in, TTS out, Meet agent) │
└──────────────────────────────────────────────────┘
 │
┌──────────────────────────────────────────────────┐
│ React frontend (app/src/) │
│ • Screens, navigation │
│ • Talks to core over `coreRpcClient` │
│ • No business logic - presentation only │
└──────────────────────────────────────────────────┘
```

**Where logic lives:**

* **Rust core**. all business logic. Memory Tree, integrations, model routing, tools, voice. Authoritative.
* **Tauri shell**. windowing, process lifecycle, IPC. A delivery vehicle, not where features live.
* **React frontend**. UI and orchestration. Calls into core via JSON-RPC.

## Data flow

1. **Connect**. OAuth into a [integration](../../features/integrations/README.md). Backend stores the token; core never sees it in plaintext.
2. **Auto-fetch**. Every twenty minutes the [scheduler](../../features/obsidian-wiki/auto-fetch.md) walks every active connection and asks each native provider to sync.
3. **Canonicalize**. Provider output (an email page, a GitHub diff, a Slack channel dump) is normalized into provenance-tagged Markdown.
4. **Chunk**. Markdown is split into ≤3k-token deterministic chunks.
5. **Store**. Chunks land in SQLite (`<workspace>/memory_tree/chunks.db`) and as `.md` files in `<workspace>/wiki/`.
6. **Score**. Background workers run embeddings, entity extraction, hotness scoring.
7. **Summarize**. Source / topic / global summary trees are built and refreshed from the chunk pool.
8. **Retrieve**. When you ask a question, the agent queries the Memory Tree (search / drill down / topic / global / fetch).
9. **Compress**. Tool output and large source data go through [TokenJuice](../../features/token-compression.md) before entering LLM context.
10. **Route**. The [router](../../features/model-routing/) picks the right provider+model for the task hint.

## Privacy boundary

Stays on your machine:

* The Memory Tree SQLite DB.
* The Obsidian Markdown vault.
* Audio capture buffers and any local model state.

Goes through the OpenHuman backend (under one subscription):

* LLM calls (model providers).
* Web search proxy.
* Integration OAuth and tool proxying.
* TTS streaming.

See [Privacy & Security](../../features/privacy-and-security.md) for the full picture.

## DADOU — standalone fork

This fork extends OpenHuman with local-first, privacy-preserving features:

### Guardian security pipeline (N1 → N2 → N3)

Every agent action passes through a three-tier deterministic security gate:

- **N1 (Rules engine, <1ms):** Path whitelists, regex patterns, and command classification against the live `SecurityPolicy`. Blocks known-dangerous patterns deterministically. Implemented in `src/openhuman/guardian/`.
- **N2 (Heuristic classifier, <10ms):** Detects exfiltration patterns (DNS tunnels, encoded payloads, entropy anomalies) in tool outputs before they reach the LLM. Implemented in `src/openhuman/guardian/n2/`.
- **N3 (LLM validator, <500ms):** A lightweight local model validates ambiguous action plans that N1/N2 can't classify. Cached via LRU. Implemented in `src/openhuman/guardian/n3/`.

Actions pass through N1→N2→N3: ~80% resolved at N1, ~15% at N2, ~2% escalated to N3. Blocked actions show which tier rejected them.

### Standalone mode

DADOU runs fully offline with zero cloud dependencies:

- Local SQLite memory store (`OPENHUMAN_WORKSPACE`)
- Local embedding models for semantic search
- Local LLM inference via Ollama / LM Studio
- CLI binary `dadou-core` for headless operation
- No backend subscription required

### WASM skills sandbox

Third-party skills execute in a `wasmtime` sandbox with capability-gated WASI:

- Manifest: `dadou-skill.yaml` (name, version, permissions, dependencies)
- GPG signature verification via `sequoia-openpgp`
- Static analysis blocks suspicious imports (`os`, `subprocess`, `socket`, `eval`)
- 30s timeout, no network, restricted filesystem
- Implemented in `src/openhuman/skills/wasm/`

### Anti-injection

External data and skill outputs are structurally isolated from the LLM prompt:

- `<external_data source="..." trusted="false">` tagging
- Skill outputs passed as structured JSON, never raw concatenation
- Semantic validation via 15+ injection pattern rules
- JSON plan validation by Guardian before tool execution
- Implemented in `src/openhuman/anti_injection/` and `src/openhuman/prompt_injection/`

## Open source

* **Upstream:** [github.com/tinyhumansai/openhuman](https://github.com/tinyhumansai/openhuman). GNU GPL3.
* **Issues and PRs** are welcome. The project is in early beta.
* For contributors, the canonical developer guide is [deep architecture reference](../architecture.md).
