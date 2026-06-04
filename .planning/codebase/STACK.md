# Technology Stack

**Analysis Date:** 2026-06-04

## Languages

**Primary:**
- **Rust** 2021 Edition (MSRV implied by tokio 1 / rusqlite 0.37 / axum 0.8 features) — Core business logic, JSON-RPC server, CLI binary, Tauri shell, all domain modules under `src/openhuman/`. Targets Windows, macOS, Linux.
- **TypeScript** ~5.8.3 — Frontend application under `app/src/`. Strict mode enabled.

**Secondary:**
- **JavaScript** ~ES2022 — Skills runtime (legacy QuickJS metadata, Node.js runtime for tool execution).
- **CSS** — Tailwind CSS custom configuration.
- **Terraform / HCL** — Infrastructure (likely in separate infra repo, not in this repository).
- **PowerShell** — Windows install scripts (`scripts/tests/OpenHumanWindowsInstall.Tests.ps1`).
- **Swift** — iOS PTT plugin (`packages/tauri-plugin-ptt/`, Swift + Rust).

## Runtime

**Desktop:**
- **Tauri v2** (2.10) — Desktop application shell. Two runtimes available:
  - **CEF** (Chromium Embedded Framework) — Primary/default runtime, vendored fork at `app/src-tauri/vendor/tauri-cef/`. Required for production builds.
  - **Wry** (native WebView) — Alternative via `--no-default-features --features wry`.
- Targets: Windows (MSVC), macOS (x86_64 + ARM), Linux (x86_64). No Android/iOS in production.

**iOS (experimental, non-shipping):**
- Stock `@tauri-apps/cli@^2` (not vendored CEF CLI).
- Connects to desktop core remotely; no Rust core on-device.

**Server/Cloud:**
- Docker images for cloud deployments (see `ci-build-container` / `docker-ci-image.yml`).
- Core JSON-RPC server can run standalone (`openhuman-core serve`) with CORS configuration.

## Package Managers

**Rust:**
- **Cargo** (workspace at repo root, Tauri shell at `app/src-tauri/`)
- `Cargo.lock` committed

**Node:**
- **pnpm** 10.10.0 (enforced via `packageManager` field in root `package.json`)
- pnpm workspace: root (`openhuman-repo`, private) + `app/` (`openhuman-app` v0.56.0)
- `pnpm-lock.yaml` committed

## Frameworks

**Core Frameworks:**
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

**Testing:**
| Framework | Version | Purpose |
|-----------|---------|---------|
| Vitest | ^4.0.18 | Unit/component testing (app workspace) |
| @testing-library/react | ^16.3.2 | React component testing |
| Playwright | ^1.56.1 | E2E testing (web targets) |
| WDIO (WebDriverIO) | ^9.24.0 | Desktop E2E (via Appium Mac2 / tauri-driver) |
| cargo test | — | Rust unit + integration tests |
| wiremock | 0.6 | Mock HTTP server for Rust provider tests |
| rstest (via patterns) | — | Parameterized Rust tests |

**Build/Dev:**
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

**Critical:**
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

**Storage:**
| Crate | Version | Purpose |
|-------|---------|---------|
| rusqlite | 0.37 | SQLite (local storage, bundled SQLite) |
| postgres | 0.19 | PostgreSQL client (with chrono feature) |

**LLM / Inference:**
| Crate | Purpose |
|-------|---------|
| reqwest + custom provider layer | OpenAI-compatible, Anthropic, Ollama, LM Studio, custom endpoints |
| whisper-rs | 0.16 | Local speech-to-text (Whisper.cpp, metal feature on macOS) |

**Crypto:**
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

**Channel Providers (optional features):**
| Crate | Feature flag | Purpose |
|-------|-------------|---------|
| matrix-sdk | `channel-matrix` | Matrix messaging |
| whatsapp-rust | `whatsapp-web` | WhatsApp Web E2E (multi-device protocol) |
| fantoccini | `browser-native` | WebDriver-based browser automation |

**Wallet:**
| Crate | Version | Purpose |
|-------|---------|---------|
| bitcoin | 0.32 | BTC P2WPKH PSBT build/sign/broadcast |
| ethers-core / ethers-signers | 2.0.14 | EVM chain wallet signing |
| ed25519-dalek | 2 | Solana transaction signing |
| bs58 | 0.5 | Base58 (Solana/Tron addresses) |
| coins-bip39 | 0.8 | BIP-39 mnemonic to seed |

**Communication / Email:**
| Crate | Version | Purpose |
|-------|---------|---------|
| lettre | 0.11.22 | SMTP email sending (rustls-tls) |
| mail-parser | 0.11.2 | Email parsing |
| async-imap | 0.11 | IMAP email fetching |

**Audio / Speech:**
| Crate | Version | Purpose |
|-------|---------|---------|
| whisper-rs | 0.16 | Speech-to-text |
| cpal | 0.15 | Audio input capture |
| hound | 3.5 | WAV audio encoding |
| enigo | 0.3 | Keyboard/mouse simulation |
| rdev | 0.5 | Global keyboard/mouse listener |

**Other:**
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

**Critical:**
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

**UI / Visual:**
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

**Crypto (frontend):**
| Package | Version | Purpose |
|---------|---------|---------|
| @noble/ciphers | ^1.2.1 | Web Crypto primitives |
| @noble/curves | ^2.2.0 | Elliptic curve operations |
| @noble/hashes | ^2.0.1 | Hash functions |
| @scure/bip32 / @scure/bip39 | ^2.0.1 | Wallet key derivation |
| @scure/base | ^2.2.0 | Base encoding |

**Tauri Plugins:**
| Package | Version | Purpose |
|---------|---------|---------|
| @tauri-apps/plugin-deep-link | ^2 | Deep link handling (OAuth callbacks) |
| @tauri-apps/plugin-opener | ^2 | Open URLs externally |
| @tauri-apps/plugin-os | ^2.3.2 | OS info |
| @tauri-apps/plugin-barcode-scanner | ^2.4.4 | Barcode scanning |

**Tauri Plugins (Rust side — `app/src-tauri/Cargo.toml`):**
| Plugin | Version | Purpose |
|--------|---------|---------|
| tauri-plugin-deep-link | 2.0.0 | OAuth callback handling |
| tauri-plugin-global-shortcut | 2 | Dictation hotkeys |
| tauri-plugin-notification | vendored | Native notifications |
| tauri-plugin-opener | 2 | External URL opening |
| tauri-plugin-single-instance | 2 | Single-instance lock (CEF cache race prevention) |
| tauri-plugin-updater | 2 | Auto-update for desktop shell |

## Configuration

**Environment:**
- `.env` (repo root, gitignored) — Rust core + Tauri shell env overrides. Template at `.env.example`.
- `app/.env.local` (gitignored) — Frontend VITE_* vars. Template at `app/.env.example`.
- Env loaded via `source scripts/load-dotenv.sh` (bash) or `scripts/run-dev-win.sh` (PowerShell on Windows).
- Rust config also read from TOML files via `src/openhuman/config/schema/load.rs` with env override.

**Key config env vars:**
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

**Build:**
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

**Development:**
- Rust toolchain (rustup recommended, stable channel)
- Node.js >= 24.0.0
- pnpm >= 10.10.0
- CEF development dependencies (Chromium runtime auto-downloads on first `cargo tauri build`)
- Platform-specific: macOS Xcode (for Mac2 E2E / iOS targets), Linux WebKit/Gtk, Windows MSVC build tools
- Optional: Docker (for Linux E2E tests in `docker-compose`)

**Production:**
- **Desktop:** Windows (x86_64, MSVC), macOS (x86_64 + ARM, .app + .dmg), Linux (x86_64, .AppImage)
- **Cloud/Server:** Docker containers (multi-arch Linux), standalone `openhuman-core` binary
- No mobile app in production (iOS is experimental only)
- Backend dependency: `tinyhumansai/backend` for cloud sync, tunnel relay (socket.io), billing, skills registry

## Build Profiles

| Profile | Inherits | Key Settings | Use |
|---------|----------|-------------|-----|
| `release` | — | debug = "line-tables-only", split-debuginfo = "packed" | Production builds |
| `ci` | release | opt-level = 1, codegen-units = 16, lto = false, strip = true | CI test builds (fast) |

---

*Stack analysis: 2026-06-04*
