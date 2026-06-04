<!-- refreshed: 2026-06-04 -->
# Architecture

**Analysis Date:** 2026-06-04

## System Overview

```text
┌──────────────────────────────────────────────────────────────────────┐
│                        React UI (app/src/)                           │
│  Pages  │  Components  │  Store (Redux)  │  Hooks  │  lib / utils   │
└──────────────────────┬───────────────────────────────────────────────┘
                       │
               Tauri IPC bridge
         invoke('core_rpc_relay', ...)   +   Socket.IO
                       │
┌──────────────────────┴───────────────────────────────────────────────┐
│                  Tauri Shell (app/src-tauri/src/)                     │
│  CoreProcessHandle  │  CDP  │  Provider Scanners  │  Window/CEF     │
└──────────────────────┬───────────────────────────────────────────────┘
                       │
               In-process tokio task
          HTTP/JSON-RPC 2.0  →  http://127.0.0.1:<port>/rpc
          Auth: Bearer (OPENHUMAN_CORE_TOKEN)
                       │
┌──────────────────────┴───────────────────────────────────────────────┐
│                   Rust Core Library (src/)                            │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────┐     │
│  │                    Domains (src/openhuman/)                  │     │
│  │  agent │ memory │ channels │ tools │ inference │ config    │     │
│  │  cron │ skills │ security │ ... 60+ domains                 │     │
│  └────────────────────────────┬────────────────────────────────┘     │
│                               │                                      │
│  ┌────────────────────────────┴────────────────────────────────┐     │
│  │              Transport Layer (src/core/)                      │     │
│  │  Controllers  │  Registry  │  CLI  │  JSON-RPC  │  Event Bus │     │
│  └──────────────────────────────────────────────────────────────┘     │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

## Component Responsibilities

| Component | Responsibility | File |
|-----------|----------------|------|
| Rust Core Library | All business logic, domains, RPC, persistence, CLI | `src/` (lib crate `openhuman`) |
| Tauri Shell | Desktop host, IPC bridge, CDP scanning, window lifecycle | `app/src-tauri/src/lib.rs` |
| React UI | UX, screens, navigation, state management | `app/src/` |
| iOS Client | Non-shipping experimental mobile target | `app/src/pages/ios/`, `app/src/components/ios/` |

## Pattern Overview

**Overall:** Three-layer architecture — Rust core (authoritative business logic) -> Tauri desktop host (thin IPC bridge) -> React frontend (UX rendering).

**Key Characteristics:**
- **Controller registry pattern**: Domain logic is exposed via a centralized `ControllerSchema` / `RegisteredController` registry (`src/core/all.rs`), consumed by both JSON-RPC and CLI transports — no domain branches in transport code.
- **Event bus pattern**: Typed pub/sub via `DomainEvent` enum (`src/core/event_bus/events.rs`) with broadcast channel for cross-domain communication; native typed request/response (`NativeRegistry`) for one-to-one dispatch.
- **Provider chain pattern**: React app uses a deeply nested provider hierarchy (`App.tsx` L90-L123) — Sentry.ErrorBoundary -> Redux Provider -> PersistGate -> ThemeProvider -> I18nProvider -> BootCheckGate -> CoreStateProvider -> SocketProvider -> ChatRuntimeProvider -> Router -> CommandProvider -> ServiceBlockingGate.
- **Transport-agnostic controller contract**: `ControllerSchema` (`src/core/mod.rs`) defines namespace, function, description, inputs, outputs — same schema drives CLI help and RPC method discovery.
- **In-process core lifecycle**: Core runs as a tokio task inside the Tauri host, not as a sidecar. Lifecycle owned by `CoreProcessHandle` (`app/src-tauri/src/core_process.rs`).

## Layers

### Rust Core Domain Layer (`src/openhuman/`):
- Purpose: All business logic, organized by domain
- Location: `src/openhuman/`
- Contains: 60+ domain modules (agent, memory, channels, tools, inference, config, security, cron, skills, etc.)
- Depends on: External crates (reqwest, tokio, serde, axum, etc.), OS primitives
- Used by: Transport layer (`src/core/`), CLI binary (`src/main.rs`)

### Rust Transport Layer (`src/core/`):
- Purpose: Controller registry, JSON-RPC server, CLI dispatch, event bus
- Location: `src/core/`
- Contains: `mod.rs` (ControllerSchema), `all.rs` (registry), `jsonrpc.rs` (Axum HTTP server), `cli.rs` (CLI dispatch), `event_bus/` (pub/sub)
- Depends on: Domain modules (`src/openhuman/`)
- Used by: Tauri shell (`app/src-tauri/src/core_rpc.rs`), CLI binary

### Tauri Shell Layer (`app/src-tauri/src/`):
- Purpose: Desktop host, IPC bridge, CDP scanners, window management
- Location: `app/src-tauri/src/`
- Contains: `lib.rs` (Tauri plugin registration, commands), `core_process.rs` (core lifecycle), `core_rpc.rs` (HTTP bridge helpers), CDP scanners (discord, telegram, slack, whatsapp, etc.)
- Depends on: Rust core library via in-process tokio task
- Used by: Desktop application binary

### React Frontend Layer (`app/src/`):
- Purpose: UI rendering, user interaction, state management
- Location: `app/src/`
- Contains: `App.tsx` (provider chain), `AppRoutes.tsx` (routing), `pages/`, `components/`, `store/`, `services/`, `hooks/`, `lib/`, `providers/`
- Depends on: Tauri IPC for core communication, external APIs (backend services)
- Used by: User via desktop application

## Data Flow

### Primary Request Path (RPC Relay)

1. React component calls `coreRpcClient.callMethod()` (`app/src/services/coreRpcClient.ts`)
2. `coreRpcClient` invokes Tauri `invoke('core_rpc_relay', { method, params })` (`app/src-tauri/src/lib.rs`)
3. Tauri shell forwards as HTTP POST to `http://127.0.0.1:<port>/rpc` with Bearer auth (`app/src-tauri/src/core_rpc.rs`)
4. Axum `rpc_handler` in `src/core/jsonrpc.rs` parses JSON-RPC 2.0 request
5. `invoke_method` resolves method name via controller registry (`src/core/all.rs`)
6. Handler function executes domain logic in `src/openhuman/<domain>/`
7. Response flows back: JSON-RPC response -> Tauri IPC -> Promise resolution in React

### Real-time Event Flow (Dual Socket)

1. **Socket.IO connection** from React (`app/src/services/socketService.ts` via `app/src/services/coreSocket.ts`)
2. Socket.IO server in Rust core (`src/core/socketio.rs`) events pushed to connected clients
3. Events dispatched from domains via `publish_global(DomainEvent::...)` (`src/core/event_bus/bus.rs`)
4. `socketio.rs` subscribes to event bus and forwards to WebSocket clients
5. React socket service dispatches Redux actions (`app/src/store/socketSlice.ts`, `chatRuntimeSlice.ts`)

**State Management:**
- Server state (from core) flows through `CoreStateProvider` (`app/src/providers/CoreStateProvider.tsx`) which calls `fetchCoreAppSnapshot()` RPC
- Client state uses Redux Toolkit (`app/src/store/`) with redux-persist for persistence
- Real-time state arrives via Socket.IO events and updates Redux slices
- Auth tokens live in the in-process core, NOT in redux-persist

## Key Abstractions

### ControllerSchema / RegisteredController:
- Purpose: Transport-agnostic function contract for domain logic
- Examples: `src/core/mod.rs` (schema definition), `src/core/all.rs` (registry), `src/openhuman/memory/schemas.rs` (memory controllers)
- Pattern: Each domain exports `all_controller_schemas` and `all_registered_controllers` from its `schemas.rs`; wired into `src/core/all.rs` which builds the global `REGISTRY` static

### DomainEvent:
- Purpose: Typed cross-module communication via broadcast channel
- Examples: `src/core/event_bus/events.rs` (events enum), `src/openhuman/cron/bus.rs` (CronDeliverySubscriber), `src/openhuman/webhooks/bus.rs` (WebhookRequestSubscriber)
- Pattern: Variants added to `DomainEvent` enum, domain creates `*Subscriber` struct implementing `EventHandler`, registers at startup via `subscribe_global`

### SecurityPolicy:
- Purpose: Filesystem/shell access control for agent actions
- Examples: `src/openhuman/security/policy.rs` (SecurityPolicy, AutonomyLevel, TrustedRoot), `src/openhuman/security/live_policy.rs` (global live policy)
- Pattern: `classify_command` buckets commands into `CommandClass` (Read/Write/Network/Install/Destructive); `gate_decision(class, tier)` -> Allow/Prompt/Block

### Agent Harness:
- Purpose: Multi-agent orchestration, tool loop, sub-agent dispatch
- Examples: `src/openhuman/agent/harness/mod.rs` (harness root), `src/openhuman/agent/harness/session/` (session management), `src/openhuman/agent/harness/subagent_runner/` (sub-agent lifecycle)
- Pattern: Session-based agent loop with tool call/response cycle, sub-agents dispatched via `spawn_subagent` tool

### Tool System:
- Purpose: Extensible tool framework for agents
- Examples: `src/openhuman/tools/mod.rs` (Tool trait, ToolSpec), `src/openhuman/tools/impl/` (tool implementations: agent, audio, browser, computer, cron, filesystem, memory, network, system, wallet, whatsapp_data)
- Pattern: `Tool` trait with `PermissionLevel`, implementations in `tools/impl/`, policy system for access control

### Transport Manager (iOS):
- Purpose: iOS client transport abstraction to reach desktop core
- Examples: `app/src/services/transport/TransportManager.ts`, `LanHttpTransport.ts`, `TunnelTransport.ts` (crypto in `app/src/lib/tunnel/`), `CloudHttpTransport.ts`
- Pattern: `CoreTransport` interface with multiple strategies; `TransportManager` selects based on `ConnectionProfile`

## Entry Points

### Rust CLI Binary:
- Location: `src/main.rs`
- Triggers: `openhuman-core` command line invocation
- Responsibilities: Sentry init, dotenv load, CLI dispatch via `src/core/cli.rs`

### Tauri Desktop Application:
- Location: `app/src-tauri/src/main.rs` (Rust entry), `app/src-tauri/src/lib.rs` (plugin/command registration)
- Triggers: Desktop application launch
- Responsibilities: Initialize CEF, register IPC commands, manage core process lifecycle, window management

### React Application:
- Location: `app/src/main.tsx`
- Triggers: Tauri webview load (desktop) or browser (dev mode)
- Responsibilities: Mount React tree, initialize boot services (webview accounts, notifications, internet status, core health monitor), render App

## Architectural Constraints

- **Threading:** Single-threaded tokio async runtime for core; Tauri runs on its own event loop; JavaScript is single-threaded. Core runs as an in-process tokio task inside the Tauri host — no sidecar.
- **Global state:** `static REGISTRY: OnceLock<Vec<RegisteredController>>` in `src/core/all.rs` — the global controller registry initialized once. `CURRENT_RPC_TOKEN` in `app/src-tauri/src/core_process.rs`. `live_policy` for SecurityPolicy.
- **Circular imports:** The Rust crate is structured so that `src/core/` (transport) depends on `src/openhuman/` (domains), not vice versa. Event bus has `DomainEvent` enum which domains publish — domains depend on `core::event_bus` but core does not depend on specific domain logic.
- **No dynamic imports in production React code:** Static `import`/`import type` only. No `import()`, `React.lazy()`, `await import()`. Exceptions: Vitest harness patterns, `.d.ts`, config files.
- **No new JS injection in CEF webviews:** Provider webviews (Telegram, Discord, Slack, etc.) load with zero injected JS. All scraping via CDP in scanner modules.

## Anti-Patterns

### Stale-listener PID reuse

**What happens:** `CoreProcessHandle` probes an existing listener, determines it is stale, and force-kills it. Between the probe and the kill, the original PID may exit and a new process may inherit the same PID.
**Why it's wrong:** A force-kill after PID reuse could terminate an unrelated process.
**Do this instead:** `core_process.rs` revalidates the PID after the grace window before force-killing — see stale-listener detection in `src/core_process.rs`.

### Sidecar removal migration

**What happens:** The core used to run as a sidecar binary (separate process). After PR #1061 it runs in-process, but some docs and `pnpm core:stage` still reference the old sidecar pattern.
**Why it's wrong:** Confuses developers about the runtime architecture.
**Do this instead:** The sidecar was removed. `pnpm core:stage` is a no-op. Core lifecycle is `CoreProcessHandle` in `app/src-tauri/src/core_process.rs`.

### Static REGISTRY with OnceLock

**What happens:** The global `REGISTRY: OnceLock<Vec<RegisteredController>>` in `src/core/all.rs` means all controllers must be registered at startup — no runtime registration.
**Why it's wrong:** Cannot add new controllers without recompiling the core binary.
**Do this instead:** Accepted constraint for a desktop app. For plugins, the MCP framework provides runtime tool registration outside the controller system.

## Error Handling

**Strategy:** Rust: `Result<T, E>` with `?` propagation, `anyhow` for application-level errors, typed errors via `thiserror` in domain crates. JSON-RPC: standard JSON-RPC 2.0 error objects (`RpcError`, `RpcFailure` in `src/core/types.rs`).

**Patterns:**
- Domain controllers return `Result<Value, String>` — errors are string messages returned as JSON-RPC error responses
- `ControllerFuture` type alias: `Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'static>>`
- Frontend: `try/catch` around `coreRpcClient.callMethod()` with `sanitizeError()` for safe display

## Cross-Cutting Concerns

**Logging:** Rust: `log`/`tracing` at `debug`/`trace` with structured fields. `app/`: `debug` package with namespaced loggers. Sentry for error tracking in both core and frontend.
**Validation:** Zod schemas in frontend (`app/src/lib/mcp/validation.ts`). Rust: serde deserialization with schema validation at controller boundaries.
**Authentication:** Per-launch bearer token (`OPENHUMAN_CORE_TOKEN`) for core RPC. Auth tokens live in the in-process core, not redux-persist. Backend auth via session tokens.
**i18n:** All user-visible text through `useT()` from `app/src/lib/i18n/I18nContext`. Translation chunk files in `app/src/lib/i18n/chunks/` for 14 locales.

---

*Architecture analysis: 2026-06-04*
