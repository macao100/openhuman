# External Integrations

**Analysis Date:** 2026-06-04

## APIs & External Services

### LLM / Inference Providers

The core uses a flexible provider system. Supports any OpenAI-compatible chat completions API, plus Anthropic's native API format.

| Provider | Integration | Auth | Implementation |
|----------|------------|------|----------------|
| OpenAI-compatible (generic) | Rust `OpenAiCompatibleProvider` | `Authorization: Bearer <key>` | `src/openhuman/inference/provider/compatible.rs` |
| Anthropic | Rust `OpenAiCompatibleProvider` with `AuthStyle::Anthropic` | `x-api-key: <key>` + `anthropic-version: 2023-06-01` | `src/openhuman/inference/provider/compatible.rs` |
| OpenHuman Backend | `openhuman_backend.rs` | JWT session token | `src/openhuman/inference/provider/openhuman_backend.rs` |
| Ollama | Local HTTP inference client | None (localhost) | `src/openhuman/inference/local/ollama/` |
| LM Studio | Local inference via OpenAI-compatible endpoint | None (localhost) | `src/openhuman/inference/local/service/lm_studio.rs` |
| Custom providers (Groq, Mistral, xAI, Together, Perplexity, GLM, MiniMax, Bedrock, Qianfan, Venice, Moonshot, etc.) | All via `OpenAiCompatibleProvider` | Per-endpoint auth | `src/openhuman/inference/provider/compatible.rs` |

### Channel / Messaging Providers

| Service | Integration | Transport | Key Files |
|---------|------------|-----------|-----------|
| **Discord** | Rust API client + WebSocket gateway | HTTP + WebSocket (gateway bot API) | `src/openhuman/channels/providers/discord/` |
| **Telegram** | Rust bot API client (MTProto-like via Bot API) | HTTPS long-polling (getUpdates) | `src/openhuman/channels/providers/telegram/` |
| **WhatsApp** | Cloud API / Business Platform | HTTPS | `src/openhuman/channels/providers/whatsapp/` |
| **WhatsApp Web** | multi-device protocol via `whatsapp-rust` 0.5 (optional feature) | WebSocket (E2E encrypted) | `src/openhuman/channels/providers/whatsapp_web/` |
| **Slack** | Slack RTM API + Web API | WebSocket (RTM) + HTTPS | `src/openhuman/channels/providers/slack/` |
| **Matrix** | Matrix SDK (optional feature `channel-matrix`) | HTTPS + WebSocket (sync) | `src/openhuman/channels/providers/matrix/` |
| **Signal** | Signal protocol | | `src/openhuman/channels/providers/signal/` |
| **IRC** | IRC protocol | TCP plaintext | `src/openhuman/channels/providers/irc/` |
| **DingTalk** | DingTalk bot API | HTTPS | `src/openhuman/channels/providers/dingtalk/` |
| **Lark (Feishu)** | Lark bot API | HTTPS | `src/openhuman/channels/providers/lark/` |
| **QQ** | QQ bot protocol | | `src/openhuman/channels/providers/qq/` |
| **Mattermost** | Mattermost WebSocket + REST API | WebSocket + HTTPS | `src/openhuman/channels/providers/mattermost/` |
| **Email (IMAP/SMTP)** | IMAP fetch via `async-imap`, SMTP send via `lettre` | IMAP (TLS) + SMTP (TLS) | `src/openhuman/channels/providers/email_channel/` |
| **iMessage** | macOS-only: reads `~/Library/Messages/chat.db` via `rusqlite` | Local SQLite (read-only) | `src/openhuman/channels/providers/imessage/`, `app/src-tauri/src/imessage_scanner/` |
| **Yuanbao (Tencent)** | Custom protobuf-based protocol with COS media upload | HTTPS + Tencent COS (S3-compatible) | `src/openhuman/channels/providers/yuanbao/` |
| **Linq** | | | `src/openhuman/channels/providers/linq/` |
| **Web** | Built-in web channel for browser-based communication | | `src/openhuman/channels/providers/web/` |

### Webview Accounts (Desktop CEF Webviews)

Third-party websites loaded as embedded webviews via CEF, automated via CDP (Chrome DevTools Protocol):

| Service | Purpose | Implementation |
|---------|---------|----------------|
| Gmail | Email reading via CDP DOM snapshot | `app/src-tauri/src/gmail_scanner/` |
| Google Meet | Meeting awareness, captions, fake camera | `app/src-tauri/src/meet_scanner/`, `app/src-tauri/src/meet_audio/`, `app/src-tauri/src/meet_call/`, `app/src-tauri/src/meet_video/` |
| LinkedIn | Profile enrichment | `app/src-tauri/src/linkedin_scanner/` |
| Slack (CDP) | Message sync via webapp automation | `app/src-tauri/src/slack_scanner/` |
| Telegram (CDP) | Message sync via webapp automation | `app/src-tauri/src/telegram_scanner/` |
| Discord (CDP) | Message sync via webapp automation | `app/src-tauri/src/discord_scanner/` |
| WhatsApp (CDP) | Message sync via webapp automation | `app/src-tauri/src/whatsapp_scanner/` |

### Web Search

| Service | Integration | Auth | Implementation |
|---------|------------|------|----------------|
| **Seltz** | Direct REST API (`seltz.ai`) | API key (`SELTZ_API_KEY`) | `src/openhuman/integrations/seltz/` |
| **SearXNG** | Self-hosted instance REST API | None (local) | `src/openhuman/integrations/searxng/` |
| **Brave Search** | Brave Search API | | `src/openhuman/integrations/brave/` |
| **Google Places** | Google Places API | API key | `src/openhuman/integrations/google_places/` |
| **Apify** | Apify platform API | | `src/openhuman/integrations/apify/` |
| **TinyFish** | TinyFish search API | | `src/openhuman/integrations/tinyfish/` |

### Other Tool-Level Integrations

| Service | Integration | Implementation |
|---------|------------|----------------|
| **Twilio** | SMS/Voice API | `src/openhuman/integrations/twilio/` |
| **Stock Prices** | Financial data API | `src/openhuman/integrations/stock_prices/` |
| **Composio** | Third-party app integration platform (Google Calendar, GitHub, etc.) | `src/openhuman/composio/`, `app/src/lib/composio/` |

## Data Storage

**Databases:**
- **SQLite** (local) — Primary local storage. All channel state, config, credentials, message caches. Via `rusqlite` 0.37 with `bundled` feature (embedded SQLite). Used in both Rust core and Tauri shell (macOS iMessage scanner).
- **PostgreSQL** (server/cloud) — Via `postgres` 0.19 crate with `with-chrono-0_4`. Used by backend API service (external to this repo at `tinyhumansai/backend`).

**File Storage:**
- **Local filesystem** — All local storage (`~/.openhuman/` workspace by default, configurable via `OPENHUMAN_WORKSPACE`). Includes SQLite DBs, config files, runtime downloads (Node.js, Python), cache.
- **Tencent COS** (Object Storage) — Used by Yuanbao channel for media uploads. S3-compatible API.
- Cloud storage via backend (external).

**Caching:**
- In-memory caches within Rust core (e.g., credential cache via `parking_lot` RwLock).
- No dedicated caching service (Redis/Memcached) detected.

## Authentication & Identity

**Auth Provider:**
- **OpenHuman Backend JWT** — Primary auth. Session JWT obtained from login flow (`api.tinyhumans.ai` or custom `BACKEND_URL`). Used for API calls, skills OAuth proxy, and core RPC auth.
- **OAuth 2.0** — Via `motosan-ai-oauth` crate (v0.2, `codex` feature) for OpenAI OAuth and credential-connected services (Composio OAuth handoff).
- **Core RPC Bearer Token** — Per-launch hex bearer token (`OPENHUMAN_CORE_TOKEN`), auto-generated, authenticated via `Authorization: Bearer` header on `/rpc` endpoint.
- **OS Keychain** — Via `keyring` crate (v3, `apple-native`/`windows-native`/`linux-native` features). Used for stored credentials.

**Implementation:**
- Auth flow: Login screen -> backend JWT -> stored in OS keychain (not redux-persist).
- Core-side: `src/openhuman/config/schema/load.rs` + `src/core/auth/`.
- Frontend: `app/src/services/api/authApi.ts`, `app/src/services/apiClient.ts`.

## MCP (Model Context Protocol)

| Component | Role | Implementation |
|-----------|------|----------------|
| **MCP Client** | Connects to external MCP servers (stdio-based) | `src/openhuman/mcp_client/` — stdio transport + client + registry |
| **MCP Server** | Hosts MCP servers for external clients | `src/openhuman/mcp_server/` |
| **MCP Registry** | Manages installed/available MCP servers | `src/openhuman/mcp_registry/` |
| **MCP Audit** | Logs MCP server interactions | `src/openhuman/mcp_audit/` |
| **Frontend MCP** | Socket.io-based MCP transport + validation | `app/src/lib/mcp/` — transport, types, validation, error handling, rate limiting |

## Monitoring & Observability

**Error Tracking:**
- **Sentry** — Three separate projects:
  1. Rust core (`OPENHUMAN_CORE_SENTRY_DSN`) — `src/` + `Cargo.toml` sentry crate
  2. Tauri shell (`OPENHUMAN_TAURI_SENTRY_DSN`) — `app/src-tauri/Cargo.toml`
  3. React frontend (`VITE_SENTRY_DSN`) — `@sentry/react` + `@sentry/vite-plugin` for source maps
- Self-hosted Sentry at `sentry.tinyhumans.ai` (configurable via `SENTRY_URL`).

**Logs:**
- Rust: `tracing` framework with `tracing-subscriber` (fmt, env-filter, ansi), `tracing-appender` for file output. `RUST_LOG` env var for level control.
- Frontend: `debug` npm package, namespaced per domain.
- Debug log output directory: `target/debug-logs/` (via `scripts/debug/` wrappers).

**Metrics:**
- **Prometheus** (`prometheus` 0.14 crate) — In-core metrics exposition.
- **OpenTelemetry** (`opentelemetry` + `opentelemetry-otlp` 0.32) — Trace and metric export to OTLP-compatible backends.

**Analytics:**
- **Google Analytics 4** — `react-ga4` on frontend, `VITE_GA_MEASUREMENT_ID`. Anonymous page views and feature-engagement only.

## CI/CD & Deployment

**Hosting:**
- **Desktop** — Direct downloads: macOS (.app/.dmg), Windows (.exe/.msi), Linux (.AppImage). GitHub Releases.
- **Cloud/Server** — Docker images (multi-arch Linux). Self-hosted or VPS deployment.

**CI Pipeline:**
- **GitHub Actions** — 23 workflow files in `.github/workflows/`:
  - `build.yml`, `build-desktop.yml`, `build-windows.yml` — Build matrices (macOS, Windows, Linux)
  - `test.yml`, `test-reusable.yml`, `typecheck.yml`, `coverage.yml` — Testing
  - `e2e.yml`, `e2e-reusable.yml`, `e2e-playwright.yml`, `e2e-agent-review.yml` — E2E testing
  - `release-production.yml`, `release-staging.yml` — Release pipelines
  - `release-packages.yml` — Package publishing
  - `coverage.yml` — Coverage gate (>= 80% on changed lines via diff-cover)
  - `installer-smoke.yml`, `deploy-smoke.yml` — Post-deploy verification
  - `pr-quality.yml` — PR quality checks
  - `android-compile.yml`, `ios-compile.yml` — Mobile compile checks
  - `docker-ci-image.yml` — Docker CI image build
  - `uptime-monitor.yml`, `weekly-code-review.yml` — Ongoing
  - `contributor-rewards.yml`, `tauri-cef-pin-guard.yml` — Specialized

**Release Pipeline:**
1. Staging release (`release-staging.yml`) -> QA
2. Production promotion (`release-production.yml`, promotes from staging tag or hotfix branch)
3. Builds for: macOS (x86_64 + ARM), Windows (x86_64), Linux (x86_64)
4. Sentry debug info / source maps uploaded
5. Docker images for cloud deployments

## Environment Configuration

**Required env vars (for production operation):**
| Variable | Required by | Description |
|----------|------------|-------------|
| `OPENHUMAN_CORE_TOKEN` | Core | RPC bearer token (auto-generated in desktop, required for cloud) |
| `BACKEND_URL` | Core + Frontend | Backend API URL (default: `https://api.tinyhumans.ai`) |
| `OPENHUMAN_CORE_SENTRY_DSN` | Core | Error reporting DSN (recommended) |
| `VITE_SENTRY_DSN` | Frontend | Frontend error reporting DSN |
| `JWT_TOKEN` | Various | Session JWT for API calls |

**Secrets location:**
- Development: `.env` (gitignored) at repo root + `app/.env.local` (gitignored)
- Production: GitHub Actions secrets, env vars on deployment host
- OS keychain: For user credentials (via `keyring` crate)
- Core token: `{workspace}/core.token` file (auto-generated, `0o600`)

## Webhooks & Callbacks

**Incoming:**
- **Webhook receiver** — Generic incoming webhook router with typed event bus dispatch. Supports channel/provider-specific webhook endpoints.
  - Location: `src/openhuman/webhooks/` — `bus.rs`, `ops.rs`, `router.rs`, `types.rs`
  - Frontend: Settings UI at `/settings/webhooks-triggers`

**Outgoing:**
- **Webhook sender** — Webhook triggers emitted via event bus for external system integration.
  - Event types: channel events, cron deliveries, skill executions, tool calls
  - Delivered via HTTP POST to configured webhook URLs

**OAuth Callbacks:**
- Deep link handling via `tauri-plugin-deep-link` — Captures OAuth redirect URIs (e.g., `openhuman://oauth/callback`).
- Composio OAuth handoff: `src/openhuman/composio/oauth_handoff/`.

## Network Configuration

**Proxy Support:**
- HTTP/HTTPS/SOCKS proxy via configurable env vars (`OPENHUMAN_HTTP_PROXY`, `OPENHUMAN_HTTPS_PROXY`, `OPENHUMAN_ALL_PROXY`, `OPENHUMAN_NO_PROXY`).
- reqwest configured with `socks` feature for SOCKS proxy support.
- Proxy scope filtering (`OPENHUMAN_PROXY_SCOPE`, `OPENHUMAN_PROXY_SERVICES`).

**Transport Layer:**
- TLS via `rustls` (ring-backed) on macOS/Linux, `native-tls` (schannel) on Windows for WebSocket connections.
- CORS configuration for JSON-RPC server (`OPENHUMAN_CORE_ALLOWED_ORIGINS`).
- Dual Socket.io channels: core <-> frontend (in-process for desktop, network for cloud).

## Wallet / Blockchain

| Chain | Integration | RPC Endpoint (default) |
|-------|------------|----------------------|
| EVM (Ethereum, Polygon, etc.) | `ethers-core` + `ethers-signers` | `https://ethereum-rpc.publicnode.com` |
| Bitcoin | `bitcoin` 0.32 (P2WPKH PSBT) | `https://blockstream.info/api` |
| Solana | `ed25519-dalek` | `https://api.mainnet-beta.solana.com` |
| Tron | `bs58` + `ripemd` | `https://api.trongrid.io` |

All chain RPC URLs overridable via `OPENHUMAN_WALLET_RPC_EVM`, `OPENHUMAN_WALLET_RPC_BTC`, `OPENHUMAN_WALLET_RPC_SOLANA`, `OPENHUMAN_WALLET_RPC_TRON`.

## iOS Client Transport (Experimental)

The iOS client connects to the desktop core via one of three transport strategies:

| Transport | Encryption | Implementation |
|-----------|-----------|----------------|
| **LanHttpTransport** | Plain HTTP (LAN-local) | `app/src/services/transport/LanHttpTransport.ts` |
| **TunnelTransport** | E2E: XChaCha20-Poly1305 over X25519 key agreement via Socket.io relay | `app/src/services/transport/TunnelTransport.ts`, `app/src/lib/tunnel/crypto.ts`, `app/src/services/transport/LanHttpTransport.ts` |
| **CloudHttpTransport** | HTTPS via cloud backend | `app/src/services/transport/CloudHttpTransport.ts` |

## Environment Configuration

**Env files:**
- `.env.example` — Core + Tauri shell vars (Rust backend config)
- `app/.env.example` — Frontend VITE_* vars
- Templates for both are committed; actual `.env` / `.env.local` are gitignored

**Key frontend config:**
- All `VITE_*` vars centralized in `app/src/utils/config.ts` — never use `import.meta.env` directly elsewhere.

---

*Integration audit: 2026-06-04*
