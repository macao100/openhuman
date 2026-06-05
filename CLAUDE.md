# OpenHuman

**AI assistant for communities — React + Tauri v2 desktop app with a Rust core (JSON-RPC / CLI).**

Narrative architecture: [`gitbooks/developing/architecture.md`](gitbooks/developing/architecture.md). Frontend: [`gitbooks/developing/architecture/frontend.md`](gitbooks/developing/architecture/frontend.md). Tauri shell: [`gitbooks/developing/architecture/tauri-shell.md`](gitbooks/developing/architecture/tauri-shell.md). Agent-harness tool surface: [`gitbooks/developing/architecture/agent-harness.md`](gitbooks/developing/architecture/agent-harness.md).

---

## Repository layout

| Path | Role |
| --- | --- |
| **`app/`** | pnpm workspace `openhuman-app` (v0.53.45): Vite + React (`app/src/`), Tauri desktop host (`app/src-tauri/`), Vitest tests |
| **`src/`** (root) | Rust lib crate `openhuman` + `openhuman-core` CLI binary (`src/main.rs`) — `src/core/` (transport: Axum/HTTP, JSON-RPC, CLI), `src/openhuman/*` domains, event bus |
| **`Cargo.toml`** (root) | Core crate; `cargo build --bin openhuman-core` produces the binary. Also defines `slack-backfill` and `gmail-backfill-3d` helper binaries in `src/bin/`. |
| **`docs/`** | Remaining deep internals (memory pipeline excalidraws, sentry, etc.). Public contributor docs live in `gitbooks/developing/`. |

Commands assume the **repo root**; `pnpm dev` delegates to the `app` workspace. The root `package.json` is `openhuman-repo` (private) and enforces pnpm via the `packageManager` field.

---

## Runtime scope

- **Shipped product**: desktop — Windows, macOS, Linux.
- **Tauri host** (`app/src-tauri`): desktop-only. No Android/iOS branches.
- **Core runs in-process** inside the Tauri host as a tokio task — there is **no sidecar binary anymore** (removed in PR #1061). The lifecycle is owned by `core_process::CoreProcessHandle` in `app/src-tauri/src/core_process.rs`; on Cmd+Q the core dies with the GUI. Frontend RPC still goes over HTTP (`core_rpc_relay` + `core_rpc` client) to `http://127.0.0.1:<port>/rpc`, authenticated with a per-launch bearer in `OPENHUMAN_CORE_TOKEN`. Set `OPENHUMAN_CORE_REUSE_EXISTING=1` to attach to an externally-started `openhuman-core` process (e.g. a debug harness).

**Where logic lives**
- **Rust core**: business logic, execution, domains, RPC, persistence, CLI. Authoritative.
- **Tauri + React (`app/`)**: UX, screens, navigation, bridging to the core. Presents and orchestrates only.

---

## iOS client (experimental)

The iOS client is an **in-progress, non-shipping** target in this repo. It does not ship a Rust core on-device; instead it connects to the desktop core via one of three transports selected by a `ConnectionProfile`.

**Transport strategies** (see `app/src/services/transport/`):
- `LanHttpTransport` — direct HTTP to the desktop core on the same LAN.
- `TunnelTransport` — socket.io relay through the backend; E2E encrypted with XChaCha20-Poly1305 over X25519 key agreement.
- `CloudHttpTransport` — fallback via the cloud backend API.

**Key paths:**
- PTT plugin: `packages/tauri-plugin-ptt/` (Swift + Rust, iOS-only).
- iOS screens: `app/src/pages/ios/` and `app/src/components/ios/`.
- Devices domain (Rust): `src/openhuman/devices/`.
- Tunnel crypto (TS): `app/src/lib/tunnel/`.
- iOS build entry: `pnpm tauri:ios:dev` — uses stock `@tauri-apps/cli@^2` via `npx`, **not** the vendored CEF CLI.
- Setup guide: `docs/ios/SETUP.md`.

**Backend dependency:** `tinyhumansai/backend#709` (tunnel socket.io contract) must be merged and deployed for end-to-end pairing to work.

---

## Commands (from repo root)

```bash
pnpm dev                  # Vite dev server only (app workspace)
pnpm dev:app              # Full Tauri desktop dev (CEF runtime, loads env via scripts/load-dotenv.sh)
pnpm build                # Production UI build
pnpm typecheck            # tsc --noEmit (app workspace, aliased to `compile`)
pnpm compile              # Same as typecheck
pnpm lint                 # ESLint --cache
pnpm format               # Prettier write + cargo fmt
pnpm format:check         # Prettier check + cargo fmt --check

# Rust — core library + CLI
cargo check --manifest-path Cargo.toml
cargo build --manifest-path Cargo.toml --bin openhuman-core

# Rust — Tauri shell
cargo check --manifest-path app/src-tauri/Cargo.toml
pnpm rust:check           # Tauri shell check
```

Note: `pnpm core:stage` is a no-op (echoes a message). The sidecar was removed in PR #1061; core is linked in-process.

**Tests**: `pnpm test` (Vitest, app workspace) · `pnpm test:coverage` · `pnpm test:rust` (cargo test via `scripts/test-rust-with-mock.sh`).
**Quality**: ESLint + Prettier + Husky in `app`. Pre-push hook runs `pnpm rust:check` — pass `--no-verify` only for unrelated pre-existing breakage.

### Agent debug runners (`scripts/debug/`)

Bounded-output wrappers around the project test runners. Stdout stays summary-sized (so it fits in agent context); full output is teed to `target/debug-logs/<kind>-<suffix>-<timestamp>.log`. Add `--verbose` to also stream raw output. Prefer these over invoking Vitest / WDIO / cargo directly when iterating.

```bash
# Vitest
pnpm debug unit                                    # full suite
pnpm debug unit src/components/Foo.test.tsx        # one file (positional pattern)
pnpm debug unit -t "renders empty state"           # filter by test name
pnpm debug unit Foo -t "renders empty" --verbose

# WDIO E2E (one spec at a time)
pnpm debug e2e test/e2e/specs/smoke.spec.ts
pnpm debug e2e test/e2e/specs/cron-jobs-flow.spec.ts cron-jobs --verbose

# cargo tests (delegates to scripts/test-rust-with-mock.sh)
pnpm debug rust
pnpm debug rust json_rpc_e2e

# Inspect saved logs
pnpm debug logs                  # list 50 most recent
pnpm debug logs last             # print most recent (last 400 lines)
pnpm debug logs unit             # most recent matching prefix "unit"
pnpm debug logs last --tail 100
```

Files: `scripts/debug/{cli,unit,e2e,rust,logs,lib}.sh` plus `README.md`. Entry point is `pnpm debug` (`scripts/debug/cli.sh`).

### Coverage requirement (merge gate)

PRs must meet **≥ 80% coverage on changed lines**. Enforced by [`.github/workflows/coverage.yml`](.github/workflows/coverage.yml) using `diff-cover` over merged Vitest (`app/coverage/lcov.info`) and `cargo-llvm-cov` (core + Tauri shell) lcov outputs. Below the threshold the PR will not merge — add tests for new/changed lines, not just the happy path.

---

## Configuration

- **[`.env.example`](.env.example)** — Rust core, Tauri shell, backend URL, logging, proxy, storage, AI binary overrides. Load via `source scripts/load-dotenv.sh`.
- **[`app/.env.example`](app/.env.example)** — `VITE_*` (core RPC URL, backend URL, Sentry DSN, dev helpers). Copy to `app/.env.local`.

**Frontend config** is centralized in [`app/src/utils/config.ts`](app/src/utils/config.ts). Read `VITE_*` there and re-export — **never** `import.meta.env` directly elsewhere.

**Rust config** uses a TOML `Config` struct (`src/openhuman/config/schema/types.rs`) with env overrides (`src/openhuman/config/schema/load.rs`).

**Agent access mode** — the `[autonomy]` block (`src/openhuman/config/schema/autonomy.rs`) drives the agent's filesystem/shell reach via `SecurityPolicy` (`src/openhuman/security/policy.rs`). Tiers: `level` (`readonly` = read-only / `supervised` = "ask before edit" / `full` = full access) × `workspace_only` × `trusted_roots` (per-folder `read`/`readwrite` grants outside the workspace, overriding `forbidden_paths` for their subtree) × `allow_tool_install` (gates `install_tool`). Edit live via the `config.update_autonomy_settings` RPC or **Settings → Agent access** (`AgentAccessPanel.tsx`); changes swap the process-global policy in `security::live_policy` and apply to new sessions. The default projects home is `~/OpenHuman/projects` (`config::default_projects_dir`, env `OPENHUMAN_PROJECTS_DIR`), auto-created at startup and injected as a ReadWrite trusted root — distinct from the hidden internal `~/.openhuman/workspace`.

**Command permission model (deterministic, fail-closed):** `classify_command` buckets a command into `CommandClass` (`Read` / `Write` / `Network` / `Install` / `Destructive`); an unrecognized command is **`Write`**, never `Read`. `gate_decision(class, tier)` → `Allow` / `Prompt` / `Block`: read-only allows only reads; ask-before-edit prompts every act (file *create* is free, *edit-existing* prompts); full runs read+write but **always-asks** Network/Install/Destructive. Acting tools (`shell`/`node_exec`/`npm_exec`/`file_write`/`edit_file`/`apply_patch`/`git_operations`/`curl`) return `external_effect_with_args() == true` for `Prompt` classes so the harness routes them through the `ApprovalGate` *before* `execute()`; read-only `Block` + structural guards (`check_gated_command`) are enforced in-tool. The LLM may pass a `category` (escalate-only: `max(rust_floor, declared)`). System/credential dirs are an **unconditional** cross-platform block (`is_always_forbidden`, trusted-root-proof). Enforcement is in Rust (`classify_command`/`gate_decision`/`check_gated_command`/`is_path_string_allowed`/`validate_path`), never the system prompt.

> ⚠️ **The approval prompt is ON by default** (opt out with `OPENHUMAN_APPROVAL_GATE=0`/`false`, `jsonrpc.rs`). `ApprovalGate::init_global` installs unless disabled, so `try_global()` is `Some` and the prompt is wired end-to-end; with `OPENHUMAN_APPROVAL_GATE=0` the harness skips the intercept and `Prompt`-class calls **run unprompted**. The gate parks only for **interactive chat turns** (a `tokio` task-local chat context is set in `channels/providers/web.rs`; background triage/cron turns carry no context and are allowed through, not gated). It publishes `DomainEvent::ApprovalRequested`, which `ApprovalSurfaceSubscriber` bridges to the `approval_request` web-channel socket event; the frontend (`ChatApprovalRequestEvent` → `chatRuntime.pendingApprovalByThread` → `ApprovalRequestCard` above the composer) surfaces Approve/Deny, routing to the `openhuman.approval_decide` RPC. A typed `yes`/`no` chat reply is also honoured server-side (web.rs ingress router runs before the "newer request aborts the in-flight turn" path); any other text cancels the parked turn and is taken as a fresh message. Unanswered prompts still park to the 10-min TTL → Deny. Read-only blocking, path hardening, structural guards, and classification **are** live regardless of the flag. Full access ships as documented full-trust (not sandboxed).

---

## Testing

### Unit (Vitest)

- Co-locate as `*.test.ts` / `*.test.tsx` under `app/src/**`.
- Config: `app/test/vitest.config.ts`; setup: `app/src/test/setup.ts`.
- Run from repo root: `pnpm test` or `pnpm test:coverage`. (Inside `app/`, `pnpm test:unit` is also defined.)
- Prefer behavior over implementation. Use helpers in `app/src/test/`. No real network, no time flakes.

### Shared mock backend

Used by both unit and Rust tests.
- Core: `scripts/mock-api-core.mjs` · server: `scripts/mock-api-server.mjs` · E2E wrapper: `app/test/e2e/mock-server.ts`.
- Admin: `GET /__admin/health`, `POST /__admin/reset`, `POST /__admin/behavior`, `GET /__admin/requests`.
- Run manually: `pnpm mock:api`.

### E2E (WDIO — dual platform)

Full guide: [`gitbooks/developing/e2e-testing.md`](gitbooks/developing/e2e-testing.md).
- **Linux (CI)**: `tauri-driver` (WebDriver :4444).
- **macOS (local)**: Appium Mac2 (XCUITest :4723) on the `.app` bundle.
- Specs: `app/test/e2e/specs/*.spec.ts`. Helpers in `app/test/e2e/helpers/`. Config: `app/test/wdio.conf.ts`.

```bash
pnpm test:e2e:build
bash app/scripts/e2e-run-spec.sh test/e2e/specs/smoke.spec.ts smoke
pnpm test:e2e:all:flows
docker compose -f e2e/docker-compose.yml run --rm e2e   # Linux E2E on macOS
```

Use `element-helpers.ts` (`clickNativeButton`, `waitForWebView`, `clickToggle`) — never raw `XCUIElementType*`. Assert UI outcomes and mock effects.

### Deterministic core reset (E2E)

`app/scripts/e2e-run-spec.sh` creates and cleans a temp `OPENHUMAN_WORKSPACE` by default. `OPENHUMAN_WORKSPACE` redirects core config + storage away from `~/.openhuman`. Each spec gets a fresh in-process core inside the freshly-built Tauri bundle.

### Rust tests with mock

```bash
pnpm test:rust
bash scripts/test-rust-with-mock.sh --test json_rpc_e2e
```

---

## Frontend (`app/src/`)

**Provider chain** (`App.tsx`):
`Sentry.ErrorBoundary` → `Redux Provider` → `PersistGate` (with `PersistRehydrationScreen`) → `BootCheckGate` → `CoreStateProvider` → `SocketProvider` → `ChatRuntimeProvider` → `HashRouter` → `CommandProvider` → `ServiceBlockingGate` → `AppShell` (`AppRoutes` + `BottomTabBar` + walkthrough/mascot/snackbars).

No `UserProvider` / `AIProvider` / `SkillProvider` — auth and core snapshot live in `CoreStateProvider`, fetched via `fetchCoreAppSnapshot()` RPC (auth tokens are NOT in redux-persist; they live in the in-process core).

**State** (`store/`): Redux Toolkit slices — `accounts`, `channelConnections`, `chatRuntime`, `coreMode`, `deepLinkAuth`, `mascot`, `notification`, `providerSurface`, `socket`, `thread`. Persisted slices via redux-persist. Prefer Redux over ad-hoc `localStorage` (exception: ephemeral UI state like upsell dismiss flags).

**Services** (`services/`): singletons — `apiClient`, `socketService`, `coreRpcClient` + `coreCommandClient` (HTTP bridge to in-process core via Tauri IPC), `chatService`, `analytics`, `notificationService`, `webviewAccountService`, `daemonHealthService`, plus domain `api/*` clients.

**MCP** (`lib/mcp/`): JSON-RPC transport, validation, types over Socket.io.

**Routing** (`AppRoutes.tsx`, HashRouter): `/` (Welcome), `/onboarding/*`, `/home`, `/human`, `/intelligence`, `/skills`, `/chat` (unified agent + connected web apps, replaces old `/conversations` + `/accounts`), `/channels`, `/invites`, `/notifications`, `/rewards`, `/webhooks` (redirects to `/settings/webhooks-triggers`), `/settings/*`. Default catch-all is `DefaultRedirect`. There is no `/login`, no `/mnemonic` (recovery phrase moved to Settings), no `/agents`, no `/conversations`.

**AI config**: bundled prompts in `src/openhuman/agent/prompts/` (also bundled via `app/src-tauri/tauri.conf.json` `resources`). Loaders in `app/src/lib/ai/` use `?raw` imports, optional remote fetch, and `ai_get_config` / `ai_refresh_config` in Tauri.

---

## Tauri shell (`app/src-tauri/`)

Thin desktop host. Top-level modules: `core_process`, `core_rpc`, `cdp`, `cef_preflight`, `cef_profile`, `dictation_hotkeys`, `file_logging`, `mascot_native_window`, `native_notifications`, `notification_settings`, `process_kill`, `process_recovery`, `screen_capture`, `window_state`, plus the per-provider scanner modules (`discord_scanner`, `gmessages_scanner`, `imessage_scanner`, `meet_scanner`, `slack_scanner`, `telegram_scanner`, `whatsapp_scanner`), `meet_audio` / `meet_call` / `meet_video`, `fake_camera`, `webview_accounts`, `webview_apis`.

**Core lifecycle**: `core_process::CoreProcessHandle` spawns the JSON-RPC server as an in-process tokio task and authenticates inbound RPC with a per-launch hex bearer (`OPENHUMAN_CORE_TOKEN`). On stale-listener detection (#1130) the handle revalidates the PID before force-killing so PID reuse can't kill an unrelated process. `restart_core_process` / `start_core_process` Tauri commands let the frontend cycle it for updates.

Registered IPC (see [`gitbooks/developing/architecture/tauri-shell.md`](gitbooks/developing/architecture/tauri-shell.md)) includes `greet`, `write_ai_config_file`, `ai_get_config`, `ai_refresh_config`, `core_rpc_relay`, `core_rpc_token`, `start_core_process`, `restart_core_process`, window commands, and `openhuman_*` daemon helpers. Always use `invoke('core_rpc_relay', ...)` for in-process RPC (avoids CORS preflight that `fetch()` would trigger).

### CEF child webviews — no new JS injection

Embedded provider webviews (`acct_*`, loading third-party origins like `web.telegram.org`, `linkedin.com`, `slack.com`, …) **must not** grow any new JavaScript injection. Do not add new `.js` files under `app/src-tauri/src/webview_accounts/`, do not append new blocks to `build_init_script` / `RUNTIME_JS`, and do not dispatch scripts via CDP `Page.addScriptToEvaluateOnNewDocument` / `Runtime.evaluate` for these webviews. The migrated providers (whatsapp, telegram, slack, discord, browserscan) load with **zero** injected JS under CEF by design — all scraping and observability runs natively via CDP in the per-provider scanner modules, and anything host-controlled that runs inside a third-party origin is a scraping/attack-surface liability.

New behavior for these webviews lives in:

- **CEF handlers** — `on_navigation`, `on_new_window`, `LoadHandler::OnLoadStart`, `CefRequestHandler::*` (wired in `webview_accounts/mod.rs`).
- **CDP from the scanner side** — `Network.*`, `Emulation.*`, `Input.*`, `Page.*` driven by the per-provider `*_scanner/` modules.
- **Rust-side notification/IPC hooks** — never cross into the renderer.

If a feature truly cannot be built this way (e.g. intercepting a click the page's JS preventDefaults), the correct answer is to **surface the limitation**, not to ship an init script. Legacy injection that already exists for non-migrated providers (`gmail`, `linkedin`, `google-meet` recipe files plus the `runtime.js` bridge) is grandfathered but should shrink, not grow.

Watch out for Tauri plugins that inject JS by default. `tauri-plugin-opener` ships `init-iife.js` (a global click listener that calls `plugin:opener|open_url` via HTTP-IPC) unless you build it with `.open_js_links_on_click(false)`. Any new plugin added to `app/src-tauri/src/lib.rs` must be audited for a `js_init_script` call — if found, opt out or configure around it.

---

## Rust core (`src/`)

- **`src/openhuman/`** — Domain logic. Current domains: `about_app`, `accessibility`, `agent`, `app_state`, `approval`, `autocomplete`, `billing`, `channels`, `composio`, `config`, `context`, `cost`, `credentials`, `cron`, `doctor`, `embeddings`, `encryption`, `health`, `heartbeat`, `integrations`, `learning`, `local_ai`, `meet`, `meet_agent`, `memory`, `migration`, `node_runtime`, `notifications`, `overlay`, `people`, `prompt_injection`, `provider_surfaces`, `providers`, `redirect_links`, `referral`, `routing`, `scheduler_gate`, `screen_intelligence`, `security`, `service`, `skills`, `socket`, `subconscious`, `team`, `text_input`, `threads`, `tokenjuice`, `tool_timeout`, `tools`, `tree_summarizer`, `update`, `voice`, `wallet`, `webhooks`, `webview_accounts`, `webview_apis`, `webview_notifications`. RPC controllers in per-domain `rpc.rs` / `schemas.rs`; use `RpcOutcome<T>` per [`AGENTS.md`](AGENTS.md).
- **Skills runtime removed**: the QuickJS / `rquickjs` runtime that previously executed skill packages is gone. `src/openhuman/skills/` is now a metadata-only domain (`ops_create`, `ops_discover`, `ops_install`, `ops_parse`, `inject`, `schemas`, `types`) — see the module header comment "Legacy skill metadata helpers retained after QuickJS runtime removal."
- **Module layout rule**: new functionality goes in a **dedicated subdirectory** (`openhuman/<domain>/mod.rs` + siblings). **Do not** add new standalone `*.rs` files at `src/openhuman/` root (`dev_paths.rs` and `util.rs` are grandfathered, not a template).
- **Controller schema contract**: shared types in `src/core/types.rs` / `src/core/mod.rs` (`ControllerSchema`, `FieldSchema`, `TypeSchema`).
- **Domain schema files**: per-domain `schemas.rs` (e.g. `src/openhuman/cron/schemas.rs`), exported from domain `mod.rs`.
- **Controller-only exposure**: expose features to CLI and JSON-RPC via the controller registry. **Do not** add domain branches in `src/core/cli.rs` / `src/core/jsonrpc.rs`.
- **Light `mod.rs`**: keep domain `mod.rs` export-focused. Operational code in `ops.rs`, `store.rs`, `types.rs`, etc.
- **`src/core/`** — Transport only. Modules: `all`, `all_tests`, `auth`, `autocomplete_cli_adapter`, `cli`, `cli_tests`, `dispatch`, `event_bus/`, `jsonrpc`, `jsonrpc_tests`, `legacy_aliases`, `logging`, `memory_cli`, `observability`, `rpc_log`, `shutdown`, `socketio`, `types`, plus `agent_cli`. No heavy domain logic here. (There is no `src/core_server/` — older docs that reference `core_server` mean `src/core/`.)

### Controller migration checklist

- `src/openhuman/<domain>/mod.rs`: add `mod schemas;`, re-export `all_controller_schemas as all_<domain>_controller_schemas` and `all_registered_controllers as all_<domain>_registered_controllers`.
- `src/openhuman/<domain>/schemas.rs` defines `schemas`, `all_controller_schemas`, `all_registered_controllers`, and `handle_*` fns delegating to domain `rpc.rs`.
- Wire exports into `src/core/all.rs`. Remove migrated branches from `src/core/dispatch.rs`.

### Event bus (`src/core/event_bus/`)

Typed pub/sub + in-process typed request/response. Both singletons — use module-level functions; never construct `EventBus` / `NativeRegistry` directly.

- **Broadcast** (`publish_global` / `subscribe_global`) — fire-and-forget. Many subscribers, no return.
- **Native request/response** (`register_native_global` / `request_native_global`) — one-to-one typed dispatch keyed by method string. Zero serialization — trait objects, `mpsc::Sender`, `oneshot::Sender` pass through unchanged. Internal-only; JSON-RPC-facing work goes through `src/core/all.rs`.

Core types (all in `src/core/event_bus/`):

| Type | File | Purpose |
| --- | --- | --- |
| `DomainEvent` | `events.rs` | `#[non_exhaustive]` enum of all cross-module events |
| `EventBus` | `bus.rs` | Singleton over `tokio::sync::broadcast`; ctor is `pub(crate)` |
| `NativeRegistry` / `NativeRequestError` | `native_request.rs` | Typed request/response registry by method name |
| `EventHandler` | `subscriber.rs` | Async trait with optional `domains()` filter |
| `SubscriptionHandle` | `subscriber.rs` | RAII — drops cancel the subscriber |
| `TracingSubscriber` | `tracing.rs` | Built-in debug logger |

Singleton API: `init_global(capacity)`, `publish_global(event)`, `subscribe_global(handler)`, `register_native_global(method, handler)`, `request_native_global(method, req)`, `global()` / `native_registry()`.

Domains: `agent`, `memory`, `channel`, `cron`, `skill`, `tool`, `webhook`, `system`.

Each domain owns a `bus.rs` with its `EventHandler` impls — e.g. `cron/bus.rs` (`CronDeliverySubscriber`), `webhooks/bus.rs` (`WebhookRequestSubscriber`), `channels/bus.rs` (`ChannelInboundSubscriber`). Convention: `<Purpose>Subscriber` + `name()` returning `"<domain>::<purpose>"`.

**Adding events**: add variants to `DomainEvent`, extend the `domain()` match, create `<domain>/bus.rs`, register subscribers at startup, publish via `publish_global`.

**Adding a native handler**: define request/response types in the domain (owned fields, `Arc`s, channels — not borrows; `Send + 'static`, not `Serialize`). Register at startup keyed by `"<domain>.<verb>"`. Callers dispatch via `request_native_global`.

**Tests**: re-register the same method to override; or construct a fresh `NativeRegistry::new()` for isolation.

---

## Design

Premium, calm visual language — ocean primary `#4A83DD`, sage / amber / coral semantics, Inter + Cabinet Grotesk + JetBrains Mono, Tailwind with custom radii/spacing/shadows. Implementation tokens live in [`app/tailwind.config.js`](app/tailwind.config.js).

## Shell vs app code

Tauri/Rust in the shell is a **delivery vehicle** (windowing, process lifecycle, IPC). Keep UI behavior and product logic in TypeScript/React (`app/`). Only grow Rust in the shell for hard platform/security reasons.

## Git workflow

This file is loaded into every contributor's Claude Code session, so the instructions below are written generically: `<your-username>` means **your** GitHub username (the owner of your fork), not any specific maintainer. Adapt the literal commands accordingly.

**One-time remote setup.** Contribute via your own fork of `tinyhumansai/openhuman`. Recommended remote layout:

```
origin    git@github.com:<your-username>/openhuman.git  (your fork — push here)
upstream  git@github.com:tinyhumansai/openhuman.git     (fetch-only; never push)
```

If you cloned the upstream directly, fix it once:

```bash
git remote rename origin upstream
git remote add origin git@github.com:<your-username>/openhuman.git
git fetch upstream
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the full new-contributor walkthrough.

- **Never write code on `main`.** Before making any code changes, branch off the latest upstream `main` (`git fetch upstream && git checkout -b <branch> upstream/main`). All work happens on that feature branch; `main` stays clean and only advances via merged PRs.
- Issues and PRs on upstream **[tinyhumansai/openhuman](https://github.com/tinyhumansai/openhuman)** — not a fork — unless explicitly told otherwise.
- Issue templates: [`.github/ISSUE_TEMPLATE/feature.md`](.github/ISSUE_TEMPLATE/feature.md), [`.github/ISSUE_TEMPLATE/bug.md`](.github/ISSUE_TEMPLATE/bug.md). PR template: [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). AI-authored text should follow them verbatim.
- PRs target **`main`** of `tinyhumansai/openhuman`.
- **Push branches to `origin` (your fork), never to `upstream` (`tinyhumansai/openhuman`).** PRs are opened against `tinyhumansai/openhuman:main` with `--head <your-username>:<branch>` so the source is the fork. Direct pushes to upstream pollute its branch list and skip code-review boundaries. Treat the `upstream` remote as fetch-only.
- **When the user asks you to push or open a PR, resolve blockers and push — don't prompt for permission.** If a pre-push hook fails on something unrelated to your changes (e.g. pre-existing breakage on `main` in code you didn't touch), push with `--no-verify` and call it out in the PR body. If the hook fails on your own changes, fix them and push again. Don't ask the user whether to bypass — just do the right thing and tell them what you did.

---

## Coding philosophy

- **Unix-style modules**: small, sharp-responsibility units composed through clear boundaries.
- **Tests before the next layer**: ship unit tests for new/changed behavior before stacking features. Untested code is incomplete.
- **Docs with code**: new/changed behavior ships with matching rustdoc / code comments; update `AGENTS.md` or architecture docs when rules or user-visible behavior change.

---

## Debug logging (must follow)

- Default to **verbose diagnostics** on new/changed flows so issues are easy to trace end-to-end.
- Log entry/exit, branches, external calls, retries/timeouts, state transitions, errors.
- Use stable grep-friendly prefixes (`[domain]`, `[rpc]`, `[ui-flow]`) and correlation fields (request IDs, method names, entity IDs).
- Rust: `log` / `tracing` at `debug` / `trace`. `app/`: namespaced `debug` + dev-only detail.
- **Never** log secrets or full PII — redact.
- Changes lacking diagnosis logging are incomplete.

---

## Feature design workflow

Specify → prove in Rust → prove over RPC → surface in the UI → test.

1. **Specify against the current codebase** — ground in existing domains, controller/registry patterns, JSON-RPC naming (`openhuman.<namespace>_<function>`). No parallel architectures.
2. **Implement in Rust** — domain logic under `src/openhuman/<domain>/`, schemas + handlers in the registry, unit tests until correct in isolation.
3. **JSON-RPC E2E** — extend [`tests/json_rpc_e2e.rs`](tests/json_rpc_e2e.rs) / [`scripts/test-rust-with-mock.sh`](scripts/test-rust-with-mock.sh) so RPC methods match what the UI will call.
4. **UI in Tauri app** — React screens/state using `core_rpc_relay` / `coreRpcClient`. Keep rules in the core.
5. **App unit tests** — Vitest.
6. **App E2E** — desktop specs for user-visible flows.

**Capability catalog**: when a change adds/removes/renames a user-facing feature, update `src/openhuman/about_app/` in the same work.

**Planning rule**: up front, define the **E2E scenarios (core RPC + app)** that cover the full intended scope — happy paths, failure modes, auth gates, regressions. Not testable end-to-end ⇒ incomplete spec or too-large cut.

---

## Key patterns

- **File size**: prefer ≤ ~500 lines; split growing modules.
- **Pre-merge** (code changes): Prettier, ESLint, `tsc --noEmit` in `app/`; `cargo fmt` + `cargo check` for changed Rust.
- **No dynamic imports** in production `app/src` code — static `import` / `import type` only. No `import()`, `React.lazy(() => import(...))`, `await import(...)`. For heavy optional paths, use a static import and guard the call site with `try/catch` or a runtime check. *Exceptions*: Vitest harness patterns in `*.test.ts` / `__tests__` / `test/setup.ts`; ambient `typeof import('…')` in `.d.ts`; config files (e.g. `tailwind.config.js` JSDoc).
- **Dual socket sync**: when changing the realtime protocol, keep `socketService` / MCP transport aligned with core socket behavior (see `gitbooks/developing/architecture.md` dual-socket section).
- **i18n for all UI text**: every user-visible string in `app/src/**` (headings, labels, button text, placeholders, status chips, toasts, error messages, dialog copy) must go through `useT()` from `app/src/lib/i18n/I18nContext`. Hard-coded literals in JSX or `label=`/`placeholder=`/`aria-label=` props are not allowed. Add the key to [`app/src/lib/i18n/en.ts`](app/src/lib/i18n/en.ts) in the same PR — other locales fall back to English. Exceptions: developer-only debug logs, code identifiers, and non-display data (URLs, slugs, technical sentinel values).
- **i18n chunk files — update ALL locales**: the source-of-truth translation files are the **chunk files** under `app/src/lib/i18n/chunks/` (`en-{1..5}.ts` plus `<locale>-{1..5}.ts` for each locale). When adding or changing keys in `en.ts`, you **must also** add them to the corresponding English chunk file (`en-N.ts`) **and** to the same chunk number for every non-English locale (use the English value as a placeholder — translators fill in later). CI enforces parity via `pnpm i18n:check`; missing keys in any locale chunk will fail the i18n coverage gate. Locales: `ar`, `bn`, `de`, `es`, `fr`, `hi`, `id`, `it`, `ko`, `pt`, `ru`, `zh-CN`.

---

## Platform notes

- **Vendored CEF-aware `tauri-cli`**: runtime is CEF; only the vendored CLI at `app/src-tauri/vendor/tauri-cef/crates/tauri-cli` bundles Chromium into `Contents/Frameworks/`. Stock `@tauri-apps/cli` produces a broken bundle (panic in `cef::library_loader::LibraryLoader::new`). `pnpm dev:app` and all `cargo tauri` scripts call `pnpm tauri:ensure` which runs [`scripts/ensure-tauri-cli.sh`](scripts/ensure-tauri-cli.sh). If overwritten, reinstall with `cargo install --locked --path app/src-tauri/vendor/tauri-cef/crates/tauri-cli`.
- **macOS deep links**: often require a built `.app` bundle, not just `tauri dev`.
- **Tauri environment guard**: use `isTauri()` (from `app/src/services/webviewAccountService.ts`) or wrap `invoke(...)` in `try/catch`; do not check `window.__TAURI__` directly — it is not present at module load and bypasses the established wrapper contract.
- **Core is in-process** (no sidecar): `core_rpc` reaches the embedded server at `http://127.0.0.1:<port>/rpc` with bearer auth via `OPENHUMAN_CORE_TOKEN`. `scripts/stage-core-sidecar.mjs` no longer exists; `pnpm core:stage` is a no-op echo. To run the core standalone for debugging, use `./target/debug/openhuman-core serve` (token at `{workspace}/core.token`, default `~/.openhuman-staging/core.token` under `OPENHUMAN_APP_ENV=staging`).

<!-- GSD:project-start source:PROJECT.md -->
## Project

**DADOU**

DADOU est un assistant IA personnel et autonome qui fonctionne localement sur votre machine. Il pilote des logiciels, écrit du code, rédige des documents, exécute des compétences tierces sandboxées, et surtout — **se souvient**. Sa mémoire persistante lui donne une compréhension globale de vos projets, vos préférences, et son propre historique d'actions, lui permettant de s'améliorer session après session.

D'abord conçu comme un assistant mono-utilisateur taillé à son environnement, DADOU est open source (GPL-3.0) et vise une communauté de power users, développeurs et makers via son système modulaire de compétences.

**Core Value:** **Un assistant qui apprend.** Là où les autres IA repartent de zéro à chaque session, DADOU construit et maintient un modèle mental persistant de votre monde numérique — projets, préférences, erreurs passées, succès. Il ne se contente pas de répondre, il s'améliore.

### Constraints

- **Langage**: Rust 1.93 (core), TypeScript 5.8 + React 19 (frontend), Tauri v2 (desktop shell)
- **Package manager**: pnpm 10.10.0, Node ≥24
- **Desktop runtime**: Tauri v2 avec CEF Chromium (fork vendu `feat/cef`)
- **Licence**: GPL-3.0 (contrainte forte — tout fork doit rester sous GPL-3.0)
- **Build**: whisper-rs/llama.cpp bloquent sur macOS Tahoe (GGML_NATIVE=OFF requis), CI upstream cassée (5 tests Vitest + 4 erreurs TS)
- **Sécurité**: Pas d'exécution directe par le LLM — tout passe par le Guardian. Skills sandboxées. Injection IA traitée comme menace de premier ordre.
- **Performance**: Le Guardian N3 (LLM) ne doit pas ajouter > 500ms de latence aux actions courantes. N1+N2 visent < 10ms.
- **Windows-first**: Le développement et les tests se font d'abord sur Windows 11. macOS et Linux suivent.
<!-- GSD:project-end -->

<!-- GSD:stack-start source:codebase/STACK.md -->
## Technology Stack

## Languages
- **Rust** 2021 Edition (MSRV implied by tokio 1 / rusqlite 0.37 / axum 0.8 features) — Core business logic, JSON-RPC server, CLI binary, Tauri shell, all domain modules under `src/openhuman/`. Targets Windows, macOS, Linux.
- **TypeScript** ~5.8.3 — Frontend application under `app/src/`. Strict mode enabled.
- **JavaScript** ~ES2022 — Skills runtime (legacy QuickJS metadata, Node.js runtime for tool execution).
- **CSS** — Tailwind CSS custom configuration.
- **Terraform / HCL** — Infrastructure (likely in separate infra repo, not in this repository).
- **PowerShell** — Windows install scripts (`scripts/tests/OpenHumanWindowsInstall.Tests.ps1`).
- **Swift** — iOS PTT plugin (`packages/tauri-plugin-ptt/`, Swift + Rust).
## Runtime
- **Tauri v2** (2.10) — Desktop application shell. Two runtimes available:
- Targets: Windows (MSVC), macOS (x86_64 + ARM), Linux (x86_64). No Android/iOS in production.
- Stock `@tauri-apps/cli@^2` (not vendored CEF CLI).
- Connects to desktop core remotely; no Rust core on-device.
- Docker images for cloud deployments (see `ci-build-container` / `docker-ci-image.yml`).
- Core JSON-RPC server can run standalone (`openhuman-core serve`) with CORS configuration.
## Package Managers
- **Cargo** (workspace at repo root, Tauri shell at `app/src-tauri/`)
- `Cargo.lock` committed
- **pnpm** 10.10.0 (enforced via `packageManager` field in root `package.json`)
- pnpm workspace: root (`openhuman-repo`, private) + `app/` (`openhuman-app` v0.56.0)
- `pnpm-lock.yaml` committed
## Frameworks
| Framework | Version | Purpose | Location |
|-----------|---------|---------|----------|
| Axum | 0.8 | HTTP/JSON-RPC server, WebSocket upgrade | `src/core/`, `src/openhuman/inference/http/` |
| tokio | 1 (full) | Async runtime, process management | Everywhere in Rust |
| Tauri v2 | 2.10 | Desktop host (CEF-based windowing, IPC, system tray) | `app/src-tauri/` |
| React | 19.1.0 | UI framework | `app/src/` |
| Vite | 8.0.0 | Frontend bundler and dev server | `app/` |
| Redux Toolkit | ^2.11.2 | Client state management | `app/src/store/` |
| react-router-dom | ^7.13.0 | Client-side routing (HashRouter) | `app/src/AppRoutes.tsx` |
| Tailwind CSS | ^3.4.19 | Utility-first CSS | `app/tailwind.config.js` |
| Socket.io | server 0.15 (socketioxide), client ^4.8.3 | Real-time bidirectional communication | `src/openhuman/socket/`, `app/src/services/socketService.ts` |
| Framework | Version | Purpose |
|-----------|---------|---------|
| Vitest | ^4.0.18 | Unit/component testing (app workspace) |
| @testing-library/react | ^16.3.2 | React component testing |
| Playwright | ^1.56.1 | E2E testing (web targets) |
| WDIO (WebDriverIO) | ^9.24.0 | Desktop E2E (via Appium Mac2 / tauri-driver) |
| cargo test | — | Rust unit + integration tests |
| wiremock | 0.6 | Mock HTTP server for Rust provider tests |
| rstest (via patterns) | — | Parameterized Rust tests |
| Tool | Version | Purpose |
|------|---------|---------|
| ESLint | ^9.39.2 | TypeScript/JSX linting |
| Prettier | ^3.8.1 | Code formatting |
| Husky | ^9.1.7 | Git hooks |
| knip | ^6.3.1 | Dead code detection |
| cargo fmt | nightly | Rust formatting |
| cargo clippy | — | Rust linting |
| cross-env | ^10.1.0 | Cross-platform env vars |
## Key Dependencies
### Rust Crates (Core — `Cargo.toml` at root)
| Crate | Version | Why it matters |
|-------|---------|---------------|
| serde / serde_json | 1 | Serialization for all RPC, config, persistence |
| reqwest | 0.12 | HTTP client for LLM providers, APIs, web scraping. Features: json, rustls-tls, native-tls, stream, http2, multipart, socks |
| axum | 0.8 | JSON-RPC HTTP server, inference HTTP endpoints |
| tokio | 1 | Full async runtime, tokio tasks for core lifecycle |
| rusqlite | 0.37 | SQLite persistence (bundled). Used for channel state, config, credential stores |
| tracing / tracing-subscriber | 0.1 / 0.3 | Structured logging, spans, OpenTelemetry integration |
| uuid | 1 | Session IDs, entity IDs |
| chrono | 0.4 | Timestamps, timezone-aware datetime |
| socketioxide | 0.15 | Socket.io server for real-time frontend ↔ core communication |
| sentry | 0.47.0 | Error reporting (Rust core + Tauri shell) |
| opentelemetry / opentelemetry-otlp | 0.32 | OpenTelemetry trace/metric export |
| prometheus | 0.14 | Metrics exposition |
| anyhow | 1.0 | Flexible error handling in application code |
| thiserror | 2.0 | Typed error definitions in library code |
| Crate | Version | Purpose |
|-------|---------|---------|
| rusqlite | 0.37 | SQLite (local storage, bundled SQLite) |
| postgres | 0.19 | PostgreSQL client (with chrono feature) |
| Crate | Purpose |
|-------|---------|
| reqwest + custom provider layer | OpenAI-compatible, Anthropic, Ollama, LM Studio, custom endpoints |
| whisper-rs | 0.16 | Local speech-to-text (Whisper.cpp, metal feature on macOS) |
| Crate | Version | Purpose |
|-------|---------|---------|
| aes-gcm | 0.10 | AES-256-GCM encryption |
| chacha20poly1305 | 0.10 | ChaCha20-Poly1305 (tunnel E2E encryption) |
| x25519-dalek | 2 | X25519 key agreement (tunnel key exchange) |
| argon2 | 0.5 | Password hashing / key derivation |
| ring | 0.17 | TLS cryptography provider |
| rustls | 0.23 | TLS (ring-backed) |
| sha2 / sha1 / hmac | 0.10/0.10/0.12 | Hashing and HMAC |
| base64 | 0.22 | Base64 encoding/decoding |
| Crate | Feature flag | Purpose |
|-------|-------------|---------|
| matrix-sdk | `channel-matrix` | Matrix messaging |
| whatsapp-rust | `whatsapp-web` | WhatsApp Web E2E (multi-device protocol) |
| fantoccini | `browser-native` | WebDriver-based browser automation |
| Crate | Version | Purpose |
|-------|---------|---------|
| bitcoin | 0.32 | BTC P2WPKH PSBT build/sign/broadcast |
| ethers-core / ethers-signers | 2.0.14 | EVM chain wallet signing |
| ed25519-dalek | 2 | Solana transaction signing |
| bs58 | 0.5 | Base58 (Solana/Tron addresses) |
| coins-bip39 | 0.8 | BIP-39 mnemonic to seed |
| Crate | Version | Purpose |
|-------|---------|---------|
| lettre | 0.11.22 | SMTP email sending (rustls-tls) |
| mail-parser | 0.11.2 | Email parsing |
| async-imap | 0.11 | IMAP email fetching |
| Crate | Version | Purpose |
|-------|---------|---------|
| whisper-rs | 0.16 | Speech-to-text |
| cpal | 0.15 | Audio input capture |
| hound | 3.5 | WAV audio encoding |
| enigo | 0.3 | Keyboard/mouse simulation |
| rdev | 0.5 | Global keyboard/mouse listener |
| Crate | Purpose |
|-------|---------|
| prost | 0.14 | Protobuf (Yuanbao/WeChat channel protocol) |
| clap | 4.5 | CLI argument parsing |
| schemars | 1.2 | JSON Schema generation for RPC |
| shellexpand | 3.1 | Shell path expansion |
| dialoguer | 0.12 | Interactive CLI prompts |
| sysinfo | 0.33 | System information (process/scheduler gate) |
| starship-battery | 0.10 | Battery monitoring (scheduler gate) |
| resvg + tiny-skia | 0.45 / 0.11 | SVG rasterization (mascot fake camera) |
| image | 0.25 | Image decoding (PNG, JPEG) |
| tempfile | 3 | Temporary files |
| keyring | 3 | OS keychain integration |
| tar / xz2 / zip / flate2 | — | Archive extraction (Node/Python runtime bootstrap) |
| urlencoding | 2.1 | URL encoding |
| cron | 0.12 | Cron expression parsing |
### NPM Packages (App — `app/package.json`)
| Package | Version | Purpose |
|---------|---------|---------|
| react / react-dom | ^19.1.0 | UI framework |
| @reduxjs/toolkit | ^2.11.2 | State management |
| react-redux | ^9.2.0 | React-Redux bindings |
| redux-persist | ^6.0.0 | Redux state persistence |
| react-router-dom | ^7.13.0 | Client routing (HashRouter) |
| @tauri-apps/api | ^2.10.0 | Tauri IPC bridge |
| socket.io-client | ^4.8.3 | Real-time communication (socket.io client) |
| @sentry/react | ^10.38.0 | Frontend error tracking |
| zod | 4.3.6 | Schema validation |
| Package | Version | Purpose |
|---------|---------|---------|
| @radix-ui/react-dialog | ^1.1.15 | Accessible dialog primitives |
| cmdk | ^1.1.1 | Command palette (⌘K) |
| lottie-react | ^2.4.1 | Lottie animations |
| @rive-app/react-webgl2 | ^4.28.6 | Rive interactive animations |
| @remotion/player | 4.0.454 | Video/motion generation player |
| three / @types/three | ^0.183.2 | 3D rendering |
| react-icons | ^5.6.0 | Icon library |
| react-markdown | ^10.1.0 | Markdown rendering |
| qrcode.react | ^4.2.0 | QR code display |
| react-joyride | ^3.1.0 | Product walkthroughs |
| Package | Version | Purpose |
|---------|---------|---------|
| @noble/ciphers | ^1.2.1 | Web Crypto primitives |
| @noble/curves | ^2.2.0 | Elliptic curve operations |
| @noble/hashes | ^2.0.1 | Hash functions |
| @scure/bip32 / @scure/bip39 | ^2.0.1 | Wallet key derivation |
| @scure/base | ^2.2.0 | Base encoding |
| Package | Version | Purpose |
|---------|---------|---------|
| @tauri-apps/plugin-deep-link | ^2 | Deep link handling (OAuth callbacks) |
| @tauri-apps/plugin-opener | ^2 | Open URLs externally |
| @tauri-apps/plugin-os | ^2.3.2 | OS info |
| @tauri-apps/plugin-barcode-scanner | ^2.4.4 | Barcode scanning |
| Plugin | Version | Purpose |
|--------|---------|---------|
| tauri-plugin-deep-link | 2.0.0 | OAuth callback handling |
| tauri-plugin-global-shortcut | 2 | Dictation hotkeys |
| tauri-plugin-notification | vendored | Native notifications |
| tauri-plugin-opener | 2 | External URL opening |
| tauri-plugin-single-instance | 2 | Single-instance lock (CEF cache race prevention) |
| tauri-plugin-updater | 2 | Auto-update for desktop shell |
## Configuration
- `.env` (repo root, gitignored) — Rust core + Tauri shell env overrides. Template at `.env.example`.
- `app/.env.local` (gitignored) — Frontend VITE_* vars. Template at `app/.env.example`.
- Env loaded via `source scripts/load-dotenv.sh` (bash) or `scripts/run-dev-win.sh` (PowerShell on Windows).
- Rust config also read from TOML files via `src/openhuman/config/schema/load.rs` with env override.
| Variable | Default | Purpose |
|----------|---------|---------|
| `OPENHUMAN_CORE_PORT` | 7788 | JSON-RPC server port |
| `OPENHUMAN_CORE_TOKEN` | auto-generated | Bearer auth for JSON-RPC |
| `OPENHUMAN_MODEL` | — | Default LLM model |
| `BACKEND_URL` | https://api.tinyhumans.ai | Backend API URL |
| `OPENHUMAN_CORE_SENTRY_DSN` | — | Sentry DSN (Rust core) |
| `VITE_SENTRY_DSN` | — | Sentry DSN (frontend) |
| `OPENHUMAN_TELEGRAM_BOT_USERNAME` | openhuman_bot | Telegram bot for DM linking |
| `SELTZ_API_KEY` | — | Seltz search API key |
| `OPENHUMAN_WEB_SEARCH_MAX_RESULTS` | 5 | Web search result budget |
| `RUST_LOG` | info | Logging level (tracing) |
| File | Purpose |
|------|---------|
| `Cargo.toml` (root) | Rust core manifest |
| `app/src-tauri/Cargo.toml` | Tauri shell manifest |
| `app/package.json` | Frontend + scripts |
| `package.json` (root) | Workspace root, pnpm version pin |
| `app/tailwind.config.js` | Tailwind CSS config (custom tokens) |
| `app/test/vitest.config.ts` | Vitest configuration |
| `app/vite.config.ts` | Vite bundler configuration |
| `app/test/wdio.conf.ts` | WDIO E2E configuration |
| `app/tsconfig.json` | TypeScript configuration |
| `.prettierrc` | Prettier configuration |
| `app/knip.json` | Dead code analysis config |
| `app/eslint.config.*` | ESLint flat config |
| `app/src-tauri/tauri.conf.json` | Tauri window/bundle/resources config |
## Platform Requirements
- Rust toolchain (rustup recommended, stable channel)
- Node.js >= 24.0.0
- pnpm >= 10.10.0
- CEF development dependencies (Chromium runtime auto-downloads on first `cargo tauri build`)
- Platform-specific: macOS Xcode (for Mac2 E2E / iOS targets), Linux WebKit/Gtk, Windows MSVC build tools
- Optional: Docker (for Linux E2E tests in `docker-compose`)
- **Desktop:** Windows (x86_64, MSVC), macOS (x86_64 + ARM, .app + .dmg), Linux (x86_64, .AppImage)
- **Cloud/Server:** Docker containers (multi-arch Linux), standalone `openhuman-core` binary
- No mobile app in production (iOS is experimental only)
- Backend dependency: `tinyhumansai/backend` for cloud sync, tunnel relay (socket.io), billing, skills registry
## Build Profiles
| Profile | Inherits | Key Settings | Use |
|---------|----------|-------------|-----|
| `release` | — | debug = "line-tables-only", split-debuginfo = "packed" | Production builds |
| `ci` | release | opt-level = 1, codegen-units = 16, lto = false, strip = true | CI test builds (fast) |
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

## Naming Patterns
- `snake_case.rs` — e.g. `cron/ops.rs`, `cron/schemas.rs`, `cron/store.rs`
- Module files always named `mod.rs` within directories
- Binary entry points at `src/bin/<snake_name>.rs`
- Component files: `PascalCase.tsx` — e.g. `BottomTabBar.tsx`, `ApprovalRequestCard.tsx`
- Utility/hook files: `camelCase.ts` — e.g. `test-utils.tsx`, `commandTestUtils.ts`
- Config files: `kebab-case.*` — e.g. `vitest.config.ts`, `wdio.conf.ts`
- Functions, methods, variables, modules: `snake_case`
- Types, traits, enums, type parameters: `PascalCase`
- Constants and statics: `SCREAMING_SNAKE_CASE`
- Lifetimes: short lowercase (`'a`, `'de`), descriptive for complex cases (`'input`)
- Variables and functions: `camelCase` with descriptive names
- Booleans: prefer `is`, `has`, `should`, or `can` prefixes
- Component types/interfaces: `PascalCase`
- Custom hooks: `camelCase` with `use` prefix (e.g. `useCoreState`, `useT`)
- Redux slices: `camelCase` — e.g. `channelConnectionsSlice`, `socketSlice`
- Constants (module-level): `UPPER_SNAKE_CASE` — e.g. `CORE_RPC_URL`, `DEFAULT_TEST_MOCK_API_PORT`
## Code Style
- Tool: `cargo fmt` — enforced via `pnpm format` (calls `cargo fmt` + Prettier)
- 4-space indent (rustfmt default)
- Max line width: 100 characters (rustfmt default)
- Linting: `cargo clippy` — installed via `rust-toolchain.toml` (`rust-toolchain.toml`, line 5)
- Toolchain: Rust 1.93.0 (`rust-toolchain.toml`, line 4)
- Tool: Prettier with `@trivago/prettier-plugin-sort-imports` (`app/.prettierrc`)
- Semicolons: always (`semi: true`)
- Quotes: single (`singleQuote: true`)
- Trailing commas: ES5 (`trailingComma: "es5"`)
- Print width: 100 (`printWidth: 100`)
- Tab width: 2, no tabs (`tabWidth: 2`, `useTabs: false`)
- Arrow parens: avoid when possible (`arrowParens: "avoid"`)
- End of line: LF (`endOfLine: "lf"`)
- JSX single quotes: disabled (`jsxSingleQuote: false`)
- Bracket same line: true (`bracketSameLine: true`)
- Config: `app/eslint.config.js` (ESLint flat config, ESLint 9+)
- Plugins: `@typescript-eslint`, `eslint-plugin-react`, `eslint-plugin-react-hooks`, `eslint-plugin-import`
- Prettier integration via `eslint-config-prettier` (applied last to disable conflicting rules)
- Key rules:
- Test files get relaxed rules: `@typescript-eslint/no-explicit-any` off, `no-undef` off
## Import Organization
- Production `app/src` code uses static `import` / `import type` only
- No `import()`, `React.lazy(() => import(...))`, `await import(...)`
- Exceptions: Vitest harness patterns in test files, ambient `typeof import('…')` in `.d.ts`, config files
## Error Handling
- **Libraries/domain errors**: typed errors with `thiserror` (`Cargo.toml` line 107 specifies `thiserror = "2.0"`)
- **Application-level errors**: `anyhow` for flexible error context (`Cargo.toml` line 81 specifies `anyhow = "1.0"`)
- Propagation via `Result<T, E>` and `?` operator
- Context added with `.context()` / `.with_context()`
-  Production code never uses `unwrap()` — only in tests and truly unreachable states
- Example pattern from `src/openhuman/cron/ops.rs`:
- Defined at `src/rpc/mod.rs` (line 24):
- Controllers wrap results in `RpcOutcome<T>` for consistent response format
- `RpcOutcome::new(value, logs)` for results with execution logs
- `RpcOutcome::into_cli_compatible_json()` converts to CLI-compatible JSON shape
- Defined at `src/rpc/structured_error.rs` (line 36):
- Sent via sentinel-prefixed string `"__OPENHUMAN_STRUCTURED_RPC_ERROR_V1__:"` through the existing `Result<Value, String>` channel
- JSON-RPC transport decodes transparently without branch on method name
- `expected_user_state: true` skips Sentry reporting (expected user-visible states like stale threads)
- Defined at `src/core/types.rs` (line 15):
- `try/catch` with `unknown` error narrowing
- Example pattern (from `app/src/test/setup.ts`):
- Schema validation with Zod at system boundaries
- `getErrorMessage()` pattern for safe `Error` extraction from `unknown`
## Logging
- Framework: `log` crate + `tracing` / `tracing-subscriber` / `tracing-appender`
- Verbose diagnostics on new/changed flows — log entry/exit, branches, external calls, state transitions
- Stable grep-friendly prefixes: `[domain]`, `[rpc]`, `[ui-flow]`
- Correlation fields: request IDs, method names, entity IDs
- Never log secrets or full PII — redact
- `console.log` is allowed (no ESLint restriction), but tests silence it via `vi.spyOn(console, 'log').mockImplementation(() => {})`
- Debug namespace conventions: namespaced `debug` + dev-only detail
- Sentry for error tracking (`@sentry/react`)
## Comments
- `//!` for module-level doc comments
- `///` for function/item doc comments
- `// SAFETY:` comments required for every `unsafe` block
- Inline `//` for implementation notes, `TODO` / `FIXME` for known issues
- JSDoc for public APIs and exported functions
- File header doc comments with `@file` purpose (in some modules)
- `// ── Section separator ──` style used for test file organization
- `// ── Module-level mocks ──` etc. (see `BottomTabBar.test.tsx` pattern)
## Module Design
- New functionality goes in a dedicated `src/openhuman/<domain>/` subdirectory
- Light `mod.rs`: export-focused, operational code in `ops.rs`, `store.rs`, `types.rs`, `schemas.rs`, `bus.rs`
- Controller schema contract: shared types in `src/core/types.rs`
- Controller-only exposure: expose to CLI/JSON-RPC via the controller registry, not by adding domain branches to `src/core/cli.rs` / `src/core/jsonrpc.rs`
- Event bus per domain: each domain owns a `bus.rs` with `EventHandler` impls (e.g. `cron/bus.rs`, `webhooks/bus.rs`)
- Module layout rule in `CLAUDE.md`: "Do not add new standalone `*.rs` files at `src/openhuman/` root"
- Organized by feature/surface area, not by file type
- Example: `components/intelligence/`, `components/channels/`, `components/settings/panels/`
- Redux slices in `store/` (`accountsSlice`, `socketSlice`, etc.)
- Services as singletons in `services/`
- Shared utilities in `lib/`
## File Size Limits
- Prefer ≤500 lines per file
- Split growing modules — example: `cron/store.rs` at 623 lines is at the upper bound
- Prefer ≤800 lines per file
- Test setup (`setup.ts`) at ~265 lines, `I18nContext.tsx` at ~104 lines
## i18n
- `useT()` hook from `I18nContext` (`app/src/lib/i18n/I18nContext.tsx`, line 99)
- Every user-visible string in `app/src/**` must go through `useT()`
- Fallback chain: active locale → English → raw key → optional `fallback` param
- Source of truth: `app/src/lib/i18n/en.ts`
- 13 locales: `ar`, `bn`, `de`, `en`, `es`, `fr`, `hi`, `id`, `it`, `ko`, `pl`, `pt`, `ru`, `zh-CN`
- RTL support: Arabic only (set via `dir` attribute on `<html>`)
- `app/src/lib/i18n/chunks/{locale}-{1..5}.ts` for each locale (5 chunk files per locale × 14 locales = 70 files)
- Keys must be added to English chunk files AND all non-English chunk files (English value as placeholder)
- CI enforces parity via `pnpm i18n:check` — missing keys in any locale chunk fail the gate
- Dot-separated namespaced keys: `'nav.home'`, `'common.cancel'`, `'settings.panels.ai'`
## Commit Format
- Conventional commits: `<type>: <description>`
- Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`
- See `.github/PULL_REQUEST_TEMPLATE.md` for full PR format
## Function Design
- Functions are small and focused: `pause_job` (1 line), `resume_job` (1 line), `add_once` (4 lines)
- Public API functions return `Result<T, E>` or `anyhow::Result<T>`
- RPC handler signature: takes `Value` params, returns `Result<Value, String>`
- Functions under 50 lines
- React components focused on single responsibility
- Custom hooks for reusable stateful logic
## Key Patterns
- Each domain exposes `all_registered_controllers()` returning `Vec<RegisteredController>`
- Each registration pairs a `ControllerSchema` with a `handler` function
- Wiring via `src/core/all.rs` — never add domain branches to `src/core/cli.rs`/`src/core/jsonrpc.rs`
- Schema defined via `FieldSchema`, `TypeSchema`, `ControllerSchema` from `src/core/types.rs`
- Singletons: `publish_global` / `subscribe_global` for fire-and-forget broadcast
- `request_native_global` / `register_native_global` for typed one-to-one dispatch
- `DomainEvent` enum at `src/core/event_bus/events.rs` — `#[non_exhaustive]`, new variants added freely
- Each domain owns a `bus.rs` with `EventHandler` impls
- Rust variables immutable by default (`let`, not `let mut`)
- TypeScript: prefer spread operator for immutable updates, avoid mutation
- Redux Toolkit reducers use Immer internally for immutable state updates
- TypeScript production code uses static `import` / `import type` only
- Exceptions limited to test files, `.d.ts`, and config files
- `Sentry.ErrorBoundary` → `Redux Provider` → `PersistGate` → `BootCheckGate` → `CoreStateProvider` → `SocketProvider` → `ChatRuntimeProvider` → `HashRouter` → `CommandProvider` → `ServiceBlockingGate` → `AppShell`
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

## System Overview
```text
```
## Component Responsibilities
| Component | Responsibility | File |
|-----------|----------------|------|
| Rust Core Library | All business logic, domains, RPC, persistence, CLI | `src/` (lib crate `openhuman`) |
| Tauri Shell | Desktop host, IPC bridge, CDP scanning, window lifecycle | `app/src-tauri/src/lib.rs` |
| React UI | UX, screens, navigation, state management | `app/src/` |
| iOS Client | Non-shipping experimental mobile target | `app/src/pages/ios/`, `app/src/components/ios/` |
## Pattern Overview
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
### Real-time Event Flow (Dual Socket)
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
### Sidecar removal migration
### Static REGISTRY with OnceLock
## Error Handling
- Domain controllers return `Result<Value, String>` — errors are string messages returned as JSON-RPC error responses
- `ControllerFuture` type alias: `Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'static>>`
- Frontend: `try/catch` around `coreRpcClient.callMethod()` with `sanitizeError()` for safe display
## Cross-Cutting Concerns
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->
## Project Skills

| Skill | Description | Path |
|-------|-------------|------|
| ship-and-babysit | "End-to-end PR shipping workflow for tinyhumansai/openhuman: commit local changes, push to the user's fork, open or reuse a PR against main, then babysit CI and CodeRabbit feedback until the PR is green and clean. Use when the user asks to ship, open a PR, monitor CI, address review comments, or 'babysit' a branch." | `.codex/skills/ship-and-babysit/SKILL.md` |
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->

<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
