# Coding Conventions

**Analysis Date:** 2026-06-04

## Naming Patterns

**Rust Files:**
- `snake_case.rs` — e.g. `cron/ops.rs`, `cron/schemas.rs`, `cron/store.rs`
- Module files always named `mod.rs` within directories
- Binary entry points at `src/bin/<snake_name>.rs`

**TypeScript/React Files:**
- Component files: `PascalCase.tsx` — e.g. `BottomTabBar.tsx`, `ApprovalRequestCard.tsx`
- Utility/hook files: `camelCase.ts` — e.g. `test-utils.tsx`, `commandTestUtils.ts`
- Config files: `kebab-case.*` — e.g. `vitest.config.ts`, `wdio.conf.ts`

**Rust Identifiers:**
- Functions, methods, variables, modules: `snake_case`
- Types, traits, enums, type parameters: `PascalCase`
- Constants and statics: `SCREAMING_SNAKE_CASE`
- Lifetimes: short lowercase (`'a`, `'de`), descriptive for complex cases (`'input`)

**TypeScript/React Identifiers:**
- Variables and functions: `camelCase` with descriptive names
- Booleans: prefer `is`, `has`, `should`, or `can` prefixes
- Component types/interfaces: `PascalCase`
- Custom hooks: `camelCase` with `use` prefix (e.g. `useCoreState`, `useT`)
- Redux slices: `camelCase` — e.g. `channelConnectionsSlice`, `socketSlice`
- Constants (module-level): `UPPER_SNAKE_CASE` — e.g. `CORE_RPC_URL`, `DEFAULT_TEST_MOCK_API_PORT`

## Code Style

**Formatting — Rust (rustfmt):**
- Tool: `cargo fmt` — enforced via `pnpm format` (calls `cargo fmt` + Prettier)
- 4-space indent (rustfmt default)
- Max line width: 100 characters (rustfmt default)
- Linting: `cargo clippy` — installed via `rust-toolchain.toml` (`rust-toolchain.toml`, line 5)
- Toolchain: Rust 1.93.0 (`rust-toolchain.toml`, line 4)

**Formatting — TypeScript (Prettier):**
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

**Linting — TypeScript (ESLint):**
- Config: `app/eslint.config.js` (ESLint flat config, ESLint 9+)
- Plugins: `@typescript-eslint`, `eslint-plugin-react`, `eslint-plugin-react-hooks`, `eslint-plugin-import`
- Prettier integration via `eslint-config-prettier` (applied last to disable conflicting rules)
- Key rules:
  - `@typescript-eslint/no-unused-vars`: error (ignoring `_`-prefixed and `ALL_CAPS`)
  - `@typescript-eslint/no-explicit-any`: warn
  - `no-console`: off (allowed in frontend code)
  - `no-debugger`: error
  - `import/no-duplicates`: error
  - `prefer-const`: error
  - `no-var`: error
  - `curly`: `["error", "multi", "consistent"]` (allow single-line without braces)
  - `nonblock-statement-body-position`: `["error", "beside"]`
  - `react-hooks/rules-of-hooks`: error
  - `react-hooks/exhaustive-deps`: warn
- Test files get relaxed rules: `@typescript-eslint/no-explicit-any` off, `no-undef` off

## Import Organization

**TypeScript — managed by `@trivago/prettier-plugin-sort-imports`:**
1. Third-party modules (`<THIRD_PARTY_MODULES>`)
2. `src/` aliased imports (`^src/`)
3. Parent relative imports (`^[../]`)
4. Sibling relative imports (`^[./]`)

With separation between groups (`importOrderSeparation: true`), sorted specifiers (`importOrderSortSpecifiers: true`), case-insensitive ordering (`importOrderCaseInsensitive: true`), and namespace specifiers grouped (`importOrderGroupNamespaceSpecifiers: true`).

**No dynamic imports in production code:**
- Production `app/src` code uses static `import` / `import type` only
- No `import()`, `React.lazy(() => import(...))`, `await import(...)`
- Exceptions: Vitest harness patterns in test files, ambient `typeof import('…')` in `.d.ts`, config files

## Error Handling

**Rust — layered approach:**
- **Libraries/domain errors**: typed errors with `thiserror` (`Cargo.toml` line 107 specifies `thiserror = "2.0"`)
- **Application-level errors**: `anyhow` for flexible error context (`Cargo.toml` line 81 specifies `anyhow = "1.0"`)
- Propagation via `Result<T, E>` and `?` operator
- Context added with `.context()` / `.with_context()`
-  Production code never uses `unwrap()` — only in tests and truly unreachable states
- Example pattern from `src/openhuman/cron/ops.rs`:
  ```rust
  use anyhow::Result;
  // Domains return anyhow::Result<T>, propagate with ?
  pub fn pause_job(config: &Config, id: &str) -> Result<CronJob> { ... }
  ```

**Rust — RPC boundary (`RpcOutcome<T>`):**
- Defined at `src/rpc/mod.rs` (line 24):
  ```rust
  pub struct RpcOutcome<T> {
      pub value: T,
      pub logs: Vec<String>,
  }
  ```
- Controllers wrap results in `RpcOutcome<T>` for consistent response format
- `RpcOutcome::new(value, logs)` for results with execution logs
- `RpcOutcome::into_cli_compatible_json()` converts to CLI-compatible JSON shape

**Rust — Structured RPC errors (`StructuredRpcError`):**
- Defined at `src/rpc/structured_error.rs` (line 36):
  ```rust
  pub struct StructuredRpcError {
      pub message: String,
      pub data: Option<Value>,
      pub expected_user_state: bool,
  }
  ```
- Sent via sentinel-prefixed string `"__OPENHUMAN_STRUCTURED_RPC_ERROR_V1__:"` through the existing `Result<Value, String>` channel
- JSON-RPC transport decodes transparently without branch on method name
- `expected_user_state: true` skips Sentry reporting (expected user-visible states like stale threads)

**Rust — CLI responses (`CommandResponse<T>`):**
- Defined at `src/core/types.rs` (line 15):
  ```rust
  pub struct CommandResponse<T> {
      pub result: T,
      pub logs: Vec<String>,
  }
  ```

**TypeScript — async/await with typed errors:**
- `try/catch` with `unknown` error narrowing
- Example pattern (from `app/src/test/setup.ts`):
  ```typescript
  try {
    const { resetRequestCallCount } = await import('../lib/mcp/rateLimiter');
    // ...
  } catch {
    // Module may be fully mocked — safe to skip
  }
  ```
- Schema validation with Zod at system boundaries
- `getErrorMessage()` pattern for safe `Error` extraction from `unknown`

## Logging

**Rust:**
- Framework: `log` crate + `tracing` / `tracing-subscriber` / `tracing-appender`
- Verbose diagnostics on new/changed flows — log entry/exit, branches, external calls, state transitions
- Stable grep-friendly prefixes: `[domain]`, `[rpc]`, `[ui-flow]`
- Correlation fields: request IDs, method names, entity IDs
- Never log secrets or full PII — redact

**TypeScript:**
- `console.log` is allowed (no ESLint restriction), but tests silence it via `vi.spyOn(console, 'log').mockImplementation(() => {})`
- Debug namespace conventions: namespaced `debug` + dev-only detail
- Sentry for error tracking (`@sentry/react`)

## Comments

**Rust:**
- `//!` for module-level doc comments
- `///` for function/item doc comments
- `// SAFETY:` comments required for every `unsafe` block
- Inline `//` for implementation notes, `TODO` / `FIXME` for known issues

**TypeScript:**
- JSDoc for public APIs and exported functions
- File header doc comments with `@file` purpose (in some modules)
- `// ── Section separator ──` style used for test file organization
- `// ── Module-level mocks ──` etc. (see `BottomTabBar.test.tsx` pattern)

## Module Design

**Rust — Domain-first organization:**
- New functionality goes in a dedicated `src/openhuman/<domain>/` subdirectory
- Light `mod.rs`: export-focused, operational code in `ops.rs`, `store.rs`, `types.rs`, `schemas.rs`, `bus.rs`
- Controller schema contract: shared types in `src/core/types.rs`
- Controller-only exposure: expose to CLI/JSON-RPC via the controller registry, not by adding domain branches to `src/core/cli.rs` / `src/core/jsonrpc.rs`
- Event bus per domain: each domain owns a `bus.rs` with `EventHandler` impls (e.g. `cron/bus.rs`, `webhooks/bus.rs`)
- Module layout rule in `CLAUDE.md`: "Do not add new standalone `*.rs` files at `src/openhuman/` root"

**TypeScript — Feature-first organization:**
- Organized by feature/surface area, not by file type
- Example: `components/intelligence/`, `components/channels/`, `components/settings/panels/`
- Redux slices in `store/` (`accountsSlice`, `socketSlice`, etc.)
- Services as singletons in `services/`
- Shared utilities in `lib/`

## File Size Limits

**Rust:**
- Prefer ≤500 lines per file
- Split growing modules — example: `cron/store.rs` at 623 lines is at the upper bound

**TypeScript:**
- Prefer ≤800 lines per file
- Test setup (`setup.ts`) at ~265 lines, `I18nContext.tsx` at ~104 lines

## i18n

**Framework:**
- `useT()` hook from `I18nContext` (`app/src/lib/i18n/I18nContext.tsx`, line 99)
- Every user-visible string in `app/src/**` must go through `useT()`
- Fallback chain: active locale → English → raw key → optional `fallback` param

**Translation Map:**
- Source of truth: `app/src/lib/i18n/en.ts`
- 13 locales: `ar`, `bn`, `de`, `en`, `es`, `fr`, `hi`, `id`, `it`, `ko`, `pl`, `pt`, `ru`, `zh-CN`
- RTL support: Arabic only (set via `dir` attribute on `<html>`)

**Chunk Files:**
- `app/src/lib/i18n/chunks/{locale}-{1..5}.ts` for each locale (5 chunk files per locale × 14 locales = 70 files)
- Keys must be added to English chunk files AND all non-English chunk files (English value as placeholder)
- CI enforces parity via `pnpm i18n:check` — missing keys in any locale chunk fail the gate

**Key Format:**
- Dot-separated namespaced keys: `'nav.home'`, `'common.cancel'`, `'settings.panels.ai'`

## Commit Format

- Conventional commits: `<type>: <description>`
- Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`
- See `.github/PULL_REQUEST_TEMPLATE.md` for full PR format

## Function Design

**Rust:**
- Functions are small and focused: `pause_job` (1 line), `resume_job` (1 line), `add_once` (4 lines)
- Public API functions return `Result<T, E>` or `anyhow::Result<T>`
- RPC handler signature: takes `Value` params, returns `Result<Value, String>`

**TypeScript:**
- Functions under 50 lines
- React components focused on single responsibility
- Custom hooks for reusable stateful logic

## Key Patterns

**Controller Registry (Rust):**
- Each domain exposes `all_registered_controllers()` returning `Vec<RegisteredController>`
- Each registration pairs a `ControllerSchema` with a `handler` function
- Wiring via `src/core/all.rs` — never add domain branches to `src/core/cli.rs`/`src/core/jsonrpc.rs`
- Schema defined via `FieldSchema`, `TypeSchema`, `ControllerSchema` from `src/core/types.rs`

**Event Bus (Rust):**
- Singletons: `publish_global` / `subscribe_global` for fire-and-forget broadcast
- `request_native_global` / `register_native_global` for typed one-to-one dispatch
- `DomainEvent` enum at `src/core/event_bus/events.rs` — `#[non_exhaustive]`, new variants added freely
- Each domain owns a `bus.rs` with `EventHandler` impls

**Immutability:**
- Rust variables immutable by default (`let`, not `let mut`)
- TypeScript: prefer spread operator for immutable updates, avoid mutation
- Redux Toolkit reducers use Immer internally for immutable state updates

**No Dynamic Imports in Production:**
- TypeScript production code uses static `import` / `import type` only
- Exceptions limited to test files, `.d.ts`, and config files

**Provider Chain (React):**
- `Sentry.ErrorBoundary` → `Redux Provider` → `PersistGate` → `BootCheckGate` → `CoreStateProvider` → `SocketProvider` → `ChatRuntimeProvider` → `HashRouter` → `CommandProvider` → `ServiceBlockingGate` → `AppShell`

---

*Convention analysis: 2026-06-04*
