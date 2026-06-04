# Codebase Structure

**Analysis Date:** 2026-06-04

## Directory Layout

```
openhuman/
├── .claude/                           # Agent rules (minimal — delegates to CLAUDE.md)
├── .github/
│   ├── workflows/                     # CI/CD: 23 workflows (build, test, e2e, release, coverage)
│   ├── ISSUE_TEMPLATE/                # feature.md, bug.md
│   └── PULL_REQUEST_TEMPLATE.md
├── app/                               # pnpm workspace openhuman-app (v0.53.45)
│   ├── package.json
│   ├── test/                          # Vitest + WDIO configs, shared mock server
│   ├── playwright/                    # Playwright E2E tests
│   └── src/                           # React/Tauri frontend source
│       ├── main.tsx                   # React entry point
│       ├── App.tsx                    # Provider chain + shell
│       ├── AppRoutes.tsx              # HashRouter route definitions
│       ├── AppRoutesIOS.tsx           # iOS-specific routes
│       ├── index.css / App.css        # Global styles
│       ├── polyfills.ts
│       ├── vite-env.d.ts
│       ├── assets/                    # Icons (SVG, TSX components)
│       ├── chat/                      # Chat helpers (promptInjectionGuard, sendError)
│       ├── components/               # React components by domain
│       ├── features/                  # Feature modules (autocomplete, daemon, human, meet, etc.)
│       ├── hooks/                     # Custom React hooks
│       ├── lib/                       # Shared libraries (i18n, bootCheck, commands, composio, MCP, tunnel, etc.)
│       ├── mascot/                    # Mascot window app
│       ├── overlay/                   # Overlay window app
│       ├── pages/                     # Top-level page components
│       ├── providers/                 # React context providers
│       ├── services/                  # Singleton services (RPC, socket, API clients, analytics)
│       ├── store/                     # Redux Toolkit slices
│       ├── styles/                    # Theme CSS
│       ├── test/                      # Test utilities (setup, test-utils)
│       ├── types/                     # TypeScript type definitions
│       └── utils/                     # Utilities (config, tauriCommands, sanitize, crypto)
│   └── src-tauri/                     # Tauri desktop shell (Rust)
│       ├── Cargo.toml
│       ├── tauri.conf.json
│       └── src/
│           ├── main.rs                # Desktop entry point
│           ├── lib.rs                 # Plugin/command registration
│           ├── core_process.rs         # In-process core lifecycle
│           ├── core_rpc.rs             # RPC bridge helpers
│           ├── cdp/                    # Chrome DevTools Protocol module
│           ├── discord_scanner/        # Discord DOM/CDP scanner
│           ├── telegram_scanner/       # Telegram DOM/CDP scanner
│           ├── slack_scanner/          # Slack DOM/CDP scanner
│           ├── whatsapp_scanner/       # WhatsApp DOM/CDP scanner
│           ├── meet_audio/             # Google Meet audio capture
│           ├── meet_video/             # Google Meet video frame bus
│           ├── meet_scanner/           # Google Meet event scanner
│           ├── screen_capture/         # Screen capture module
│           ├── webview_accounts/       # Embedded CEF webview accounts
│           ├── webview_apis/           # Webview API server
│           └── ...                     # More Tauri modules
│       └── vendor/                    # Vendored CEF-aware tauri-cli
├── docs/                              # Deep internals (memory pipeline, security, iOS, etc.)
├── gitbooks/                          # Public contributor docs (architecture, features, legal)
├── packages/                          # Packaging (arch, deb, homebrew, npm, tauri-plugin-ptt)
├── scripts/                           # Build/debug/CI scripts
│   ├── debug/                         # Bounded-output test runners
│   ├── mock-api/                      # Mock API servers for testing
│   ├── release/                       # Release automation
│   └── tests/                         # Test helper scripts
├── src/                               # Rust library + CLI binaries
│   ├── main.rs                        # Core CLI binary entry point
│   ├── core/                          # Transport layer (controllers, CLI, JSON-RPC, event bus)
│   │   ├── mod.rs                     # ControllerSchema, FieldSchema, TypeSchema
│   │   ├── all.rs                     # Global controller registry (static REGISTRY)
│   │   ├── jsonrpc.rs                 # Axum HTTP JSON-RPC server
│   │   ├── cli.rs                     # CLI dispatch
│   │   ├── dispatch.rs                # Legacy method dispatch
│   │   ├── auth.rs                    # Token auth helpers
│   │   ├── socketio.rs                # Socket.IO server
│   │   ├── event_bus/                 # Typed pub/sub event bus
│   │   │   ├── bus.rs                 # EventBus singleton (tokio broadcast)
│   │   │   ├── events.rs              # DomainEvent enum (all variants)
│   │   │   ├── native_request.rs      # Typed request/response registry
│   │   │   ├── subscriber.rs          # EventHandler trait, SubscriptionHandle
│   │   │   ├── tracing.rs             # Built-in debug subscriber
│   │   │   └── testing.rs             # Test utilities
│   │   ├── types.rs                   # RpcRequest, RpcSuccess, RpcError, AppState
│   │   ├── logging.rs                 # Logging configuration
│   │   ├── observability.rs           # Observability setup
│   │   ├── shutdown.rs                # Graceful shutdown
│   │   ├── legacy_aliases.rs          # RPC method aliases
│   │   └── rpc_log.rs                 # RPC logging
│   └── openhuman/                     # All domain logic (60+ domains)
│       ├── mod.rs                     # Module declarations
│       ├── about_app/                 # App capability catalog
│       ├── accessibility/             # Accessibility settings
│       ├── agent/                     # Multi-agent orchestration (core "brain")
│       │   ├── agents/                # Built-in agent definitions
│       │   │   ├── orchestrator/      # Top-level orchestrator agent
│       │   │   ├── researcher/        # Research agent
│       │   │   ├── planner/           # Planning agent
│       │   │   ├── code_executor/     # Code execution agent
│       │   │   ├── critic/            # Critic/review agent
│       │   │   ├── summarizer/        # Text summarization agent
│       │   │   ├── archivist/         # Memory archivist agent
│       │   │   ├── help/              # Help agent
│       │   │   ├── tools_agent/       # Tool delegation agent
│       │   │   ├── tool_maker/        # Tool creation agent
│       │   │   ├── skill_creator/     # Skill creation agent
│       │   │   ├── crypto_agent/      # Cryptocurrency agent
│       │   │   ├── markets_agent/     # Markets agent
│       │   │   ├── integrations_agent/ # Integrations agent
│       │   │   ├── mcp_setup/         # MCP setup agent
│       │   │   ├── morning_briefing/  # Morning briefing agent
│       │   │   ├── trigger_reactor/   # Trigger reaction agent
│       │   │   └── trigger_triage/    # Trigger triage agent
│       │   ├── harness/               # Agent execution harness
│       │   │   ├── session/           # Agent session management
│       │   │   ├── subagent_runner/   # Sub-agent dispatch
│       │   │   └── ... (definition, fork_context, interrupt, tool_loop, etc.)
│       │   ├── prompts/               # System prompt sections
│       │   ├── triage/                # Trigger triage pipeline
│       │   └── ... (dispatcher, cost, multimodal, pformat, profiles, progress, etc.)
│       ├── agent_experience/          # Agent experience tracking
│       ├── agent_tool_policy/         # Agent tool policy
│       ├── app_state/                 # Application state snapshot
│       ├── approval/                  # Approval gate (user prompt for actions)
│       ├── audio_toolkit/             # Audio processing tools
│       ├── autocomplete/              # Autocomplete engine
│       ├── billing/                   # Billing/invoicing
│       ├── channels/                  # Channel implementations + runtime
│       │   ├── providers/             # Channel providers (discord, telegram, slack, etc.)
│       │   ├── runtime/               # Channel runtime orchestration
│       │   └── controllers/           # Channel RPC controllers
│       ├── composio/                  # Composio integration (third-party tools)
│       ├── config/                    # Configuration management
│       │   └── schema/                # TOML config schema + env override
│       ├── connectivity/              # Connectivity monitoring
│       ├── context/                   # Agent context building
│       ├── cost/                      # Token cost tracking
│       ├── credentials/               # Credential management
│       ├── cron/                      # Scheduled task execution
│       ├── cwd_jail/                  # Working directory sandbox
│       ├── desktop_companion/         # Desktop companion mode
│       ├── devices/                   # Device management (iOS pairing)
│       ├── doctor/                    # Self-diagnostics
│       ├── embeddings/               # Embedding generation
│       ├── encryption/                # Encryption utilities
│       ├── health/                    # Health check endpoints
│       ├── heartbeat/                 # Keepalive + scheduling
│       ├── http_host/                 # HTTP hosting
│       ├── inference/                 # LLM inference
│       │   ├── http/                  # HTTP inference providers
│       │   ├── local/                 # Local inference (Ollama, etc.)
│       │   ├── provider/              # Provider abstraction
│       │   ├── voice/                 # Voice inference
│       │   └── openai_oauth/          # OpenAI OAuth
│       ├── integrations/              # External API integrations
│       ├── javascript/                # JS execution runtime
│       ├── keyring/                   # OS keyring integration
│       ├── learning/                  # Learning/ingestion pipeline
│       ├── mcp_audit/                 # MCP audit log
│       ├── mcp_client/                # MCP client
│       ├── mcp_registry/              # MCP server registry
│       ├── mcp_server/                # MCP server
│       ├── meet/                      # Google Meet integration
│       ├── meet_agent/                # Meet-specific agent
│       ├── memory/                    # Memory subsystem (core)
│       │   ├── ingestion/             # Memory ingestion pipeline
│       │   ├── ops/                   # Memory operations (CRUD)
│       │   ├── query/                 # Memory query engine
│       │   ├── schemas/               # Memory schemas/RPC
│       │   ├── tree_global/           # Global memory tree
│       │   ├── tree_source/           # Source memory tree
│       │   └── tree_topic/            # Topic memory tree
│       ├── memory_archivist/          # Memory archivist manager
│       ├── memory_conversations/      # Conversation memory store
│       ├── memory_entities/           # Entity extraction/storage
│       ├── memory_graph/              # Memory graph database
│       ├── memory_queue/              # Memory processing queue
│       ├── memory_store/              # Memory storage backend
│       ├── memory_sync/               # Memory sync between workspaces
│       ├── memory_tools/              # Memory-related tools
│       ├── memory_tree/               # Memory tree summarization
│       ├── migration/                 # Data migration framework
│       ├── migrations/                # SQL/data migrations
│       ├── notifications/             # Notification system
│       ├── overlay/                   # Overlay UI backend
│       ├── people/                    # People/contacts management
│       ├── prompt_injection/          # Prompt injection guard
│       ├── provider_surfaces/         # Provider surface abstraction
│       ├── redirect_links/            # Link redirection
│       ├── referral/                  # Referral system
│       ├── routing/                   # Agent routing/fallback
│       ├── runtime_node/              # Node.js runtime
│       ├── runtime_python/            # Python runtime
│       ├── scheduler_gate/            # Rate limiting gate
│       ├── screen_intelligence/       # Screen understanding
│       ├── security/                  # Security policy engine
│       ├── service/                   # Service management
│       ├── skills/                    # Skill metadata (runtime removed, metadata-only)
│       ├── socket/                    # Socket management
│       ├── startup/                   # Startup orchestration
│       ├── subconscious/             # Background processing
│       ├── team/                      # Team management
│       ├── test_support/              # Test helpers
│       ├── text_input/                # Text input handling
│       ├── threads/                   # Conversation thread management
│       ├── tls/                       # TLS configuration
│       ├── todos/                     # Todo/task management
│       ├── tokenjuice/                # Token compression/optimization
│       ├── tool_registry/             # Tool registry
│       ├── tool_timeout/              # Tool execution timeout
│       ├── tools/                     # Tool framework + implementations
│       │   └── impl/                  # Tool implementations by category
│       │       ├── agent/             # Sub-agent delegation tool
│       │       ├── audio/             # Audio recording/playback tools
│       │       ├── browser/           # Browser automation tools
│       │       ├── computer/          # Computer control tools
│       │       ├── cron/              # Cron schedule tools
│       │       ├── filesystem/        # Filesystem tools
│       │       ├── memory/            # Memory access tools
│       │       ├── network/           # Network tools
│       │       ├── system/            # System utilities
│       │       ├── wallet/            # Crypto wallet tools
│       │       └── whatsapp_data/     # WhatsApp data tools
│       ├── update/                    # Auto-update system
│       ├── vault/                     # Secure vault storage
│       ├── voice/                     # Voice processing
│       ├── wallet/                    # Crypto wallet integration
│       ├── webhooks/                  # Webhook handling
│       ├── webview_accounts/          # Webview account management
│       ├── webview_apis/              # Webview API definitions
│       ├── webview_notifications/     # Webview notification bridge
│       ├── whatsapp_data/             # WhatsApp data handling
│       ├── workspace/                 # Workspace management
│       ├── dev_paths.rs               # Development path helpers
│       └── util.rs                    # Shared utility functions
├── tests/                             # Rust integration/E2E tests
│   ├── json_rpc_e2e.rs                # JSON-RPC end-to-end tests
│   ├── composio_list_tools_stack_overflow_regression.rs
│   ├── learning_phase4_integration_test.rs
│   ├── memory_roundtrip_e2e.rs
│   ├── mcp_registry_e2e.rs
│   ├── screen_intelligence_vision_e2e.rs
│   └── ... (40+ test files)
├── Cargo.toml                         # Core crate manifest
├── package.json                       # Root workspace config (pnpm)
├── pnpm-workspace.yaml                # pnpm workspace config
├── pnpm-lock.yaml                     # Lockfile
├── rust-toolchain.toml                # Rust version pinning
└── docker-compose.yml                 # Docker setup
```

## Directory Purposes

**app/ (pnpm workspace openhuman-app):**
- Purpose: React + Tauri v2 frontend application
- Contains: React source, Tauri shell, test configs
- Key files: `app/src/main.tsx` (React entry), `app/src/App.tsx` (provider chain), `app/src/AppRoutes.tsx` (routing), `app/src-tauri/src/lib.rs` (Tauri plugin registration)

**src/ (Rust core library + CLI):**
- Purpose: All business logic, transport layer, CLI binary
- Contains: `src/openhuman/` (60+ domains), `src/core/` (transport, registry, event bus), `src/main.rs` (CLI)
- Key files: `src/core/all.rs` (controller registry), `src/core/jsonrpc.rs` (RPC server), `src/core/event_bus/events.rs` (DomainEvent)

**src/core/ (Transport Layer):**
- Purpose: Controller registry, JSON-RPC, CLI dispatch, event bus, auth
- Contains: Transport infrastructure only — no domain logic
- Key files: `src/core/mod.rs` (ControllerSchema), `src/core/all.rs` (global REGISTRY), `src/core/jsonrpc.rs` (Axum server), `src/core/cli.rs` (CLI dispatch), `src/core/event_bus/` (typed pub/sub)

**src/openhuman/ (Domain Logic):**
- Purpose: All business logic organized by domain
- Contains: Every feature as a self-contained module with `mod.rs`, `schemas.rs`, `rpc.rs`, `ops.rs`, `types.rs`, `store.rs` pattern
- Key files: `src/openhuman/mod.rs` (module declarations), `src/openhuman/agent/` (agent runtime), `src/openhuman/memory/` (memory subsystem), `src/openhuman/tools/` (tool framework)

**app/src/components/:**
- Purpose: Reusable React UI components organized by domain
- Contains: `accounts/`, `channels/`, `chat/`, `commands/`, `composio/`, `home/`, `intelligence/`, `notifications/`, `rewards/`, `settings/`, `skills/`, `ui/`, `webhooks/`, `mcp-setup/`, `walkthrough/`, `upsell/`
- Key files: `BootCheckGate/BootCheckGate.tsx`, `chat/ApprovalRequestCard.tsx`, `ui/Button.tsx`, `commands/CommandProvider.tsx`

**app/src/pages/:**
- Purpose: Top-level route page components
- Contains: `Accounts.tsx`, `Home.tsx`, `Intelligence.tsx`, `Skills.tsx`, `Channels.tsx`, `Settings.tsx`, `Welcome.tsx`, `Notifications.tsx`, `Rewards.tsx`, `Invites.tsx`, `Conversations.tsx`, plus `onboarding/` (multi-step wizard) and `ios/` (mobile screens)
- Key files: `AppRoutes.tsx` (route definitions), `onboarding/Onboarding.tsx` (wizard)

**app/src/store/ (Redux Toolkit):**
- Purpose: Client-side state management with redux-persist
- Contains: Slices for `accounts`, `channelConnections`, `chatRuntime`, `companion`, `connectivity`, `coreMode`, `locale`, `mascot`, `notification`, `persona`, `providerSurface`, `socket`, `theme`, `thread`, `agentProfile`
- Key files: `index.ts` (configureStore), `hooks.ts` (typed hooks), `resetActions.ts`

**app/src/services/:**
- Purpose: Singleton services for external communication
- Contains: `coreRpcClient.ts` (RPC bridge), `socketService.ts` (Socket.IO), `apiClient.ts` (backend HTTP), `coreStateApi.ts` (snapshot fetching), `chatService.ts`, `analytics.ts`, `webviewAccountService.ts`, `coreHealthMonitor.ts`, plus `transport/` (iOS transport strategies)
- Key files: `coreRpcClient.ts` (JSON-RPC client over Tauri IPC), `socketService.ts` (real-time events), `api/` (backend API clients)

**app/src/hooks/:**
- Purpose: Custom React hooks for data fetching and lifecycle
- Contains: `useAppUpdate.ts`, `useBackendUrl.ts`, `useChannelDefinitions.ts`, `useComposeioTriggerHistory.ts`, `useDaemonHealth.ts`, `useIntelligenceSocket.ts`, `useMemoryIngestionStatus.ts`, `useThreadQueries.ts`, `useUsageState.ts`, `useUser.ts`, `useWebhooks.ts`, etc.
- Key files: `useThreadQueries.ts`, `useUsageState.ts`, `useIntelligenceSocket.ts`

**app/src/lib/:**
- Purpose: Shared libraries and utilities
- Contains: `i18n/` (internationalization), `mcp/` (MCP transport/client), `bootCheck/` (app boot validation), `commands/` (keyboard/command system), `composio/` (Composio integration), `tunnel/` (E2E encryption), `ai/` (AI config loading), `nativeNotifications/`, `webviewNotifications/`, `channels/` (channel definitions), `coreState/` (snapshot store)
- Key files: `i18n/I18nContext.tsx`, `mcp/index.ts` (MCP transport), `commands/registry.ts`

**tests/ (Rust integration tests):**
- Purpose: JSON-RPC E2E tests and integration tests
- Contains: 40+ test files covering RPC, memory, inference, MCP, keyring, cron, etc.
- Key files: `json_rpc_e2e.rs`, `memory_roundtrip_e2e.rs`, `mcp_registry_e2e.rs`

**gitbooks/:**
- Purpose: Public contributor and user documentation
- Contains: architecture docs, feature guides, legal docs in English + Chinese
- Key files: `developing/architecture.md`, `developing/e2e-testing.md`, `features/*`

**scripts/:**
- Purpose: Automation scripts for build, debug, CI
- Contains: `debug/` (bounded-output test runners), `mock-api/` (mock servers), `release/` (packaging)
- Key files: `scripts/debug/cli.sh` (debug entry point), `scripts/test-rust-with-mock.sh`

## Key File Locations

**Entry Points:**
- `src/main.rs`: Rust CLI binary entry (Sentry init, dotenv, CLI dispatch)
- `app/src/main.tsx`: React application entry (polyfills, boot services, render tree)
- `app/src-tauri/src/main.rs`: Desktop binary entry (Tauri builder)
- `app/src-tauri/src/lib.rs`: Tauri plugin registration, IPC commands

**Configuration:**
- `Cargo.toml`: Rust core crate manifest
- `app/package.json`: Frontend package manifest (pnpm workspace)
- `rust-toolchain.toml`: Rust version pinning
- `pnpm-workspace.yaml`: Workspace member definitions
- `.env.example`: Core environment variables
- `app/.env.example`: Frontend environment variables
- `app/src/utils/config.ts`: Frontend config hub (reads VITE_*, re-exports)
- `src/openhuman/config/schema/`: Rust TOML config schema + env override

**Core Logic:**
- `src/core/all.rs`: Global controller registry (static REGISTRY)
- `src/core/jsonrpc.rs`: Axum HTTP JSON-RPC 2.0 server
- `src/core/event_bus/events.rs`: DomainEvent enum
- `src/openhuman/agent/harness/`: Agent harness, session, tool loop
- `src/openhuman/security/policy.rs`: SecurityPolicy, AutonomyLevel
- `src/openhuman/tools/mod.rs`: Tool trait, ToolSpec
- `src/openhuman/memory/mod.rs`: Memory subsystem
- `app/src/store/index.ts`: Redux store configuration
- `app/src/services/coreRpcClient.ts`: RPC bridge client
- `app/src/services/socketService.ts`: Socket.IO real-time client

**Testing:**
- `app/test/vitest.config.ts`: Vitest configuration
- `app/test/wdio.conf.ts`: WDIO configuration
- `app/test/e2e/`: E2E test helpers + mock server
- `app/src/test/setup.ts`: Vitest test setup
- `tests/json_rpc_e2e.rs`: Rust JSON-RPC E2E tests
- `scripts/test-rust-with-mock.sh`: Rust test runner

## Naming Conventions

**Files:**
- Rust: `snake_case.rs` (modules), `snake_case.rs` for domain files (mod.rs, ops.rs, schemas.rs, types.rs, store.rs)
- React: `PascalCase.tsx` for components/pages, `camelCase.ts` for services/hooks/utils
- Tests: `*.test.ts` / `*.test.tsx` co-located with source

**Directories:**
- Rust domains: `snake_case/` (e.g., `src/openhuman/memory_graph/`)
- React feature dirs: `camelCase/` (e.g., `app/src/components/webhooks/`)
- React pages: `camelCase/` for multi-file pages (e.g., `pages/onboarding/`)

## Where to Add New Code

**New Rust Domain:**
- Create `src/openhuman/<domain>/` with `mod.rs`, `schemas.rs`, `ops.rs`, `types.rs`, `store.rs` as needed
- Export controllers via `schemas.rs` -> `all_<domain>_controller_schemas` and `all_<domain>_registered_controllers`
- Wire into `src/core/all.rs`
- Keep `mod.rs` export-focused; logic in separate files
- Do NOT add standalone `*.rs` at `src/openhuman/` root

**New React Page:**
- Page component: `app/src/pages/<Name>.tsx` (simple) or `app/src/pages/<feature>/` (complex with sub-components)
- Route definition: `app/src/AppRoutes.tsx`
- Tests: `app/src/pages/__tests__/<Name>.test.tsx`

**New React Component:**
- Implementation: `app/src/components/<domain>/<ComponentName>.tsx`
- Tests: `app/src/components/<domain>/__tests__/<ComponentName>.test.tsx`
- For reusable UI primitives: `app/src/components/ui/<ComponentName>.tsx`

**New Service:**
- Core API clients: `app/src/services/api/<name>Api.ts`
- Core services: `app/src/services/<name>Service.ts`

**New Redux Slice:**
- Slice: `app/src/store/<name>Slice.ts`
- Wire into: `app/src/store/index.ts` (reducer map)
- Wire persist: add to persist whitelist in `index.ts`

**New Rust Integration Test:**
- File: `tests/<name>_e2e.rs`

**New Script:**
- Shared scripts: `scripts/<name>.sh` or `scripts/<name>.mjs`
- Debug runners: `scripts/debug/<name>.sh`

## Special Directories

**`.claude/`:**
- Purpose: Claude Code agent configuration
- Generated: No
- Committed: Yes

**`app/src-tauri/vendor/`:**
- Purpose: Vendored CEF-aware `tauri-cli` (required for production builds)
- Generated: No (vendored dependency)
- Committed: Yes

**`gitbooks/`:**
- Purpose: Public documentation (architecture, features, legal)
- Generated: No
- Committed: Yes

**`docs/`:**
- Purpose: Deep internal documentation (memory pipeline, security, iOS)
- Generated: No
- Committed: Yes

**`packages/`:**
- Purpose: Platform packaging configs (arch, deb, homebrew, npm)
- Generated: No
- Committed: Yes

---

*Structure analysis: 2026-06-04*
