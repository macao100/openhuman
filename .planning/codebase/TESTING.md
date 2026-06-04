# Testing Patterns

**Analysis Date:** 2026-06-04

## Test Framework

**Frontend Unit Tests (Vitest):**
- Runner: Vitest (via `pnpm test`, `pnpm test:coverage`)
- Config: `app/test/vitest.config.ts`
- Setup: `app/src/test/setup.ts`
- Globals: `true` (no explicit imports needed for `describe`, `it`, `expect`, `vi`)
- Environment: `jsdom`
- Workers: single-threaded (`maxWorkers: 1, minWorkers: 1`)
- Mock settings: `clearMocks: true, mockReset: false, restoreMocks: false`
- Test matchers: `src/**/*.test.{ts,tsx}` and `test/*.test.{ts,tsx}`
- Timeout: 30s per test, 30s per hook

**Rust Tests (cargo test):**
- Runner: `cargo test -p openhuman` (core), `cargo test --manifest-path app/src-tauri/Cargo.toml` (Tauri shell)
- Commands: `pnpm test:rust` (via `scripts/test-rust-with-mock.sh`), `pnpm debug rust`
- `#[test]` for synchronous tests, `#[tokio::test]` for async tests
- Unit tests in `#[cfg(test)] mod tests { ... }` blocks within source files
- Integration tests as standalone files in `tests/` directory (34 integration test files)

**E2E (WDIO — dual platform):**
- Runner: WDIO with `tauri-driver` (WebDriver :4444) on Linux CI
- macOS local dev: Appium Mac2 (XCUITest :4723)
- Specs: `app/test/e2e/specs/*.spec.ts`
- Config: `app/test/wdio.conf.ts`
- Helpers: `app/test/e2e/helpers/{platform,element-helpers,deep-link-helpers,app-helpers}.ts`
- Playwright also available: `app/test/playwright/` (used by `e2e-playwright.yml` workflow)

## Run Commands

```bash
# Vitest (from repo root)
pnpm test                   # Full unit suite
pnpm test:coverage          # Unit tests with coverage report
pnpm test:unit              # Same as pnpm test

# Watchers
pnpm test:watch             # Vitest watch mode
pnpm test:unit:watch        # Same

# Rust
pnpm test:rust              # Core crate tests (via scripts/test-rust-with-mock.sh)
pnpm test:rust:e2e          # Rust E2E tests (scripts/test-rust-e2e.sh)
cargo test -p openhuman     # Direct cargo invocation
cargo test --test json_rpc_e2e  # Single integration test file

# E2E
pnpm test:e2e:build         # Build app + stage core
pnpm test:e2e               # All E2E flows
pnpm test:e2e:flows         # All E2E flows (gitbooks alias)

# Debug wrappers (bounded output, logs to target/debug-logs/)
pnpm debug unit                            # Full Vitest suite
pnpm debug unit src/components/Foo.test.tsx # Single file
pnpm debug unit -t "renders empty"         # Filter by test name
pnpm debug e2e test/e2e/specs/smoke.spec.ts # Single E2E spec
pnpm debug rust                             # All cargo tests
pnpm debug rust json_rpc_e2e               # Single Rust test
pnpm debug logs                             # List recent logs
pnpm debug logs last                        # View most recent log
```

## Test File Organization

**Location — Frontend:**
- Co-located with source: `app/src/**/*.test.ts` / `*.test.tsx`
- Also in `__tests__` subdirectory: `app/src/**/__tests__/*.test.ts` / `*.test.tsx`
- Example: `app/src/components/BottomTabBar.test.tsx` vs `app/src/components/intelligence/__tests__/MemoryWorkspace.test.tsx`
- Shared test utilities: `app/src/test/` directory

**Location — Rust:**
- Unit tests: `#[cfg(test)]` modules within the same source file
- Integration tests: `tests/*.rs` (34 files including `json_rpc_e2e.rs`, `observability_smoke.rs`, `inference_provider_e2e.rs`, etc.)

**Naming — TypeScript:**
- `*.test.ts` for utilities and hooks
- `*.test.tsx` for React components
- Also `*.spec.ts` for E2E specs (in `test/e2e/specs/`)

**Test Utilities — Frontend:**
- `app/src/test/setup.ts` — global setup: mock API server start, Tauri mocks, storage polyfills, console silencing
- `app/src/test/test-utils.tsx` — `renderWithProviders()` helper (Redux Provider + MemoryRouter wrapper)
- `app/src/test/commandTestUtils.ts` — command palette testing helpers
- `app/src/test/mockDefaultSkillStatusHooks.ts` — skill status mock

## Test Structure

**Suite Organization — TypeScript (from `app/src/components/__tests__/BottomTabBar.test.tsx`):**
```typescript
import { configureStore } from '@reduxjs/toolkit';
import { render, screen } from '@testing-library/react';
import { Provider } from 'react-redux';
import { MemoryRouter } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';

// ── Module-level mocks ──
vi.mock('../../providers/CoreStateProvider', () => ({ useCoreState: vi.fn() }));
vi.mock('../../utils/config', async importOriginal => {
  const actual = await importOriginal<typeof import('../../utils/config')>();
  return { ...actual, APP_ENVIRONMENT: 'development' };
});

// ── Helpers ──
interface BuildStoreOpts { ... }
function buildStore(opts: BuildStoreOpts = {}) { ... }

// ── Tests ──
describe('BottomTabBar', () => {
  it('renders navigation tabs when session is active', async () => {
    // Arrange
    // Act
    // Assert
  });
});
```

**Patterns:**
- **Setup**: `buildStore()` helper creates a fresh Redux store with selected reducers
- **Render**: `render()` via `@testing-library/react` (or `renderWithProviders()` for full provider chains)
- **Mocking**: Module-level `vi.mock()` before tests, `vi.spyOn()` for runtime spies
- **Cleanup**: `cleanup()` from `@testing-library/react` called in `afterEach` (setup.ts)

**Suite Organization — Rust (from `tests/` integration tests):**
```rust
#[test]
fn test_name() { ... }

#[tokio::test]
async fn test_async_name() { ... }
```

## Mocking

**Shared Mock Backend (API):**
- Core implementation: `scripts/mock-api-core.mjs`
- Standalone server: `scripts/mock-api-server.mjs`
- E2E wrapper: `app/test/e2e/mock-server.ts`
- Vitest setup (`setup.ts`) starts the server automatically on port 5005 by default
- Admin endpoints: `GET /__admin/health`, `POST /__admin/reset`, `POST /__admin/behavior`, `GET /__admin/requests`
- Sensitive headers (Authorization, Proxy-Authorization) are auto-redacted to `[REDACTED]` in request logs
- Run manually: `pnpm mock:api`

**Vitest Mocking — Tauri modules:**
From `app/src/test/setup.ts`:
```typescript
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn(() => false) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(), emit: vi.fn() }));
vi.mock('@tauri-apps/plugin-deep-link', () => ({ onOpenUrl: vi.fn(), getCurrent: vi.fn() }));
vi.mock('@tauri-apps/plugin-opener', () => ({ open: vi.fn() }));
vi.mock('@tauri-apps/plugin-os', () => ({ platform: vi.fn().mockResolvedValue('macos') }));
```

**Vitest Mocking — Application modules:**
- `tauriCommands` fully mocked in setup.ts
- `config` module mocked with test values
- `backendUrl` mocked to return the mock API URL
- `redux-persist` and `redux-logger` mocked to avoid CJS/ESM issues
- `@sentry/react` mocked with no-op ErrorBoundary

**Rust Mocking:**
- `wiremock` for HTTP mocking (`Cargo.toml` dev-dependencies, line 233)
- Real SQLite (rusqlite bundled) for store tests
- `filetime` for backdating mtime in lock file tests (`Cargo.toml` dev-dependencies, line 235)
- `sentry` with `test` feature for observability smoke tests (`Cargo.toml` dev-dependencies, lines 225-231)

**What to Mock (frontend):**
- All Tauri IPC calls (not available in test env)
- External network requests (redirect to mock API)
- Redux persist and logging middleware
- Sentry

**What NOT to Mock:**
- Redux store (use real reducers)
- React Router (use MemoryRouter)
- React components (test real rendering)

## Fixtures and Factories

**TypeScript Test Utilities (from `app/src/test/test-utils.tsx`):**
```typescript
export function renderWithProviders(
  ui: ReactElement,
  {
    preloadedState,
    store = createTestStore(preloadedState),
    initialEntries = ['/'],
    ...renderOptions
  }: ExtendedRenderOptions = {}
) {
  // Wraps in Provider + CoreStateContext.Provider + MemoryRouter
  return { store, ...render(ui, { wrapper: Wrapper, ...renderOptions }) };
}
```

**Store creation pattern:**
```typescript
const testRootReducer = combineReducers({
  channelConnections: channelConnectionsReducer,
  companion: companionReducer,
  // ...other slices needed for test
});

export function createTestStore(preloadedState?: Record<string, unknown>) {
  return configureStore({ reducer: testRootReducer, preloadedState: preloadedState as never });
}
```

**Location:**
- Test utilities: `app/src/test/`
- Mock data typically constructed inline in test files via helper functions like `buildStore()`, `renderBottomTabBar()`
- No centralized factory/fixture directory — data is scoped to test needs

## Coverage

**Requirement: ≥ 80% on changed lines (merge gate).**

Enforced by `.github/workflows/coverage.yml`:
- `diff-cover` with `--fail-under=80` threshold
- Merges coverage from three sources: Vitest (frontend), cargo-llvm-cov (Rust core), cargo-llvm-cov (Tauri shell)
- Scoped to **changed lines only** — test lines are excluded from lcov reports and don't skew the ratio
- Runs only on PRs (not pushes to main)

**Frontend Coverage (Vitest):**
- Provider: `v8` (built into Node.js)
- Reporters: `text`, `text-summary`, `html`, `lcov`
- Include: `src/**/*.{ts,tsx}`
- Exclude: `main.tsx`, `*.d.ts`, test files, `types.ts`, `types/` directories
- Output: `app/coverage/lcov.info`
- Coverage thresholds are **commented out** in vitest.config.ts — enforcement is via diff-cover only

**Rust Coverage (cargo-llvm-cov):**
- Core: `cargo llvm-cov -p openhuman --lcov --output-path lcov-core.info`
- Tauri shell: `cargo llvm-cov --manifest-path app/src-tauri/Cargo.toml --lcov --output-path lcov-tauri.info`
- CI uses `CARGO_BUILD_JOBS: '1'` for coverage to avoid linker bus errors
- Core coverage runs in 45-minute timeout window

**View Coverage Locally:**
```bash
pnpm test:coverage                              # Vitest coverage (HTML in app/coverage/)
cargo llvm-cov -p openhuman --html              # Rust coverage (HTML in target/llvm-cov/)
```

## Debug Runners

Located at `scripts/debug/{cli,unit,e2e,rust,logs,lib}.sh`:

- **`scripts/debug/cli.sh`** — dispatcher for `pnpm debug <command> [args]`
- **`scripts/debug/unit.sh`** — wraps Vitest, tees output to `target/debug-logs/unit-<timestamp>.log`
- **`scripts/debug/e2e.sh`** — wraps `app/scripts/e2e-run-spec.sh`, logs to `target/debug-logs/e2e-<suffix>-<timestamp>.log`
- **`scripts/debug/rust.sh`** — wraps `scripts/test-rust-with-mock.sh`, logs to `target/debug-logs/rust-<timestamp>.log`
- **`scripts/debug/logs.sh`** — inspect saved logs: `list`, `last`, prefix matching, `--head N` / `--tail N`
- **`scripts/debug/lib.sh`** — shared functions: `debug_run`, `debug_log_dir`, `debug_summarize_vitest`, `debug_summarize_cargo`

Design: stdout stays summary-sized (agent-friendly); full output teed to log files. `--verbose` flag streams raw output.

## CI/CD

**24+ GitHub Workflows in `.github/workflows/`:**

| Workflow | Purpose |
|----------|---------|
| `test.yml` | PR/push test gate — delegates to `test-reusable.yml` |
| `test-reusable.yml` | Reusable: Vitest, Rust core tests, Rust Tauri tests, i18n coverage check |
| `coverage.yml` | Coverage gate — diff-cover ≥ 80% on changed lines |
| `e2e.yml` | E2E test suite |
| `e2e-reusable.yml` | Reusable E2E workflow |
| `e2e-agent-review.yml` | Agent review E2E |
| `e2e-playwright.yml` | Playwright E2E |
| `pr-quality.yml` | PR quality checks |
| `typecheck.yml` | TypeScript type checking |
| `build-desktop.yml` | Desktop build matrix |
| `build-windows.yml` | Windows-specific build |
| `build.yml` | General build |
| `release-staging.yml` | Staging release (includes test gate) |
| `release-production.yml` | Production release (includes test gate) |

**CI Test Architecture:**
- Frontend: Vitest with v8 coverage, generates `lcov.info`
- Rust: `cargo test -p openhuman` on Linux + Windows (keyring ACL tests), `cargo test` for Tauri shell
- i18n: `pnpm i18n:check` verifies translation parity across all 13 locales + chunk files
- Uses custom CI container: `ghcr.io/tinyhumansai/openhuman_ci:rust-1.93.0`
- Caching: pnpm store (via cache action), Swatinem/rust-cache, sccache
- `scripts/ci-cancel-aware.sh` wrapper handles cancellations gracefully

## Test Types

**Unit Tests — Frontend:**
- Test behavior over implementation details
- Deterministic: no real network, no time flakes, no hidden global state
- Use `renderWithProviders()` for component tests needing Redux + Router
- Use shared mocks from `setup.ts` before adding custom mocks
- Co-located: `*.test.ts` / `*.test.tsx` next to source

**Unit Tests — Rust:**
- `#[cfg(test)] mod tests { ... }` blocks in source files
- Test public API through function calls
- Use wiremock for HTTP-dependent tests
- Real SQLite for store tests (bundled via rusqlite)

**Integration Tests — Rust (`tests/` directory):**
- 34 integration test files covering domains:
  - `json_rpc_e2e.rs` — JSON-RPC protocol E2E
  - `inference_provider_e2e.rs` — inference provider with wiremock
  - `memory_tree_walk_e2e.rs`, `memory_roundtrip_e2e.rs` — memory pipeline
  - `embeddings_rpc_e2e.rs` — embedding service RPC
  - `mcp_registry_e2e.rs`, `mcp_setup_e2e.rs` — MCP protocol
  - `vault_sync_e2e.rs`, `subconscious_e2e.rs` — domain integrations
  - `keyring_secretstore_e2e.rs` — cross-platform secrets

**E2E Tests:**
- Full desktop app via WDIO on app bundle
- Uses `scripts/test-rust-with-mock.sh` for Rust mock backend
- Uses `pnpm test:e2e:build` to build the app
- Each spec run via `app/scripts/e2e-run-spec.sh` with isolated temp `OPENHUMAN_WORKSPACE`
- Element helpers: `clickNativeButton`, `waitForWebView`, `clickToggle`
- Assert UI outcomes and mock effects
- macOS deep links require built `.app` bundle

## Common Patterns

**Async Testing — Vitest:**
```typescript
it('loads data asynchronously', async () => {
  const result = await asyncOperation();
  expect(result).toEqual(expected);
});
```

**Error Testing — Vitest:**
```typescript
it('throws error for invalid input', () => {
  expect(() => validateInput(badData)).toThrow('Invalid input');
});
```

**Mocking lifecycle — setup.ts:**
```typescript
// Before all tests: start mock server
const mockApiServer = await startMockServer(port, { retryIfInUse: true });

// After each test: clear request log, cleanup DOM, re-seed IPC handle
afterEach(() => {
  clearRequestLog();
  cleanup();
});

// After all tests: stop mock server
afterAll(async () => {
  await stopMockServer();
});

// Before each test: reset mock behavior, reset rate limiter
beforeEach(async () => {
  resetMockBehavior();
});
```

**Test-specific features (Rust):**
- `e2e-test-support` Cargo feature exposes `openhuman.test_reset` RPC for E2E — off by default in shipped binaries
- Enabled via `app/scripts/e2e-build.sh` for test builds

---

*Testing analysis: 2026-06-04*
