# Codebase Concerns

**Analysis Date:** 2026-06-04

## Tech Debt

### Skills Runtime Removed (QuickJS/rquickjs)

- **Issue:** The QuickJS / `rquickjs` JavaScript runtime that previously executed skill packages has been removed. The `src/openhuman/skills/` module (~4976 lines across 13 files) is now metadata-only: `ops_create`, `ops_discover`, `ops_install`, `ops_parse`, `inject`, `schemas`, `types`. The module header comment reads "Legacy skill metadata helpers retained after QuickJS runtime removal."
- **Files:** `src/openhuman/skills/mod.rs` (line 1: `//! Skill metadata helpers and prompt-injection support.`), `src/openhuman/skills/ops*.rs`, `src/openhuman/skills/inject.rs`, `src/openhuman/skills/bus.rs`
- **Impact:** Skill execution surfaces are being rebuilt from scratch. A `.skill` package cannot run end-to-end without verifying the current code. The webhook bus was hardcoded 410 ("skill runtime removed") for all incoming webhooks until recently patched. Any code referencing QuickJS or the old runtime will not compile.
- **Fix approach:** Either restore a runtime (unlikely -- intentional removal) or commit to the replacement pattern: skills as metadata-only with execution delegated to the agent harness via composio tools. Remove dead code paths that still reference runtime concepts.

### Vendored CEF Tauri CLI

- **Issue:** The project vendors its own CEF-aware `tauri-cli` at `app/src-tauri/vendor/tauri-cef/` as a git submodule. Stock `@tauri-apps/cli` produces a broken bundle (`panic` in `cef::library_loader::LibraryLoader::new`). This adds significant build complexity -- `pnpm dev:app` must call `pnpm tauri:ensure` which runs `scripts/ensure-tauri-cli.sh`, and when the submodule is out of date (e.g. missing `tauri_runtime_cef::audio` module), the pre-push hook fails.
- **Files:** `app/src-tauri/vendor/tauri-cef/`, `scripts/ensure-tauri-cli.sh`, `app/src-tauri/vendor/tauri-plugin-notification/`
- **Impact:** Two git submodules (`tauri-cef` and `tauri-plugin-notification`) must be kept in sync with upstream. Build failures from submodule drift are cryptic (missing CEF runtime module errors). A contributor using stock Tauri CLI gets a broken bundle without clear error.
- **Fix approach:** Upstream the CEF changes into `tauri-apps/tauri` proper, or maintain a CI check that surfaces submodule status before builds.

### iOS Experimental Target

- **Issue:** The iOS client is an in-progress, non-shipping target that breaks the main development workflow. Missing iOS experimental dependencies (`@noble/ciphers/chacha`, `@noble/ciphers/webcrypto`, `qrcode.react`, `@tauri-apps/plugin-barcode-scanner`) cause 5 Vitest failures and 4 TypeScript compile errors on upstream `main`. This breaks `pnpm compile`, `pnpm build`, and `pnpm test:coverage` on a clean checkout.
- **Files:** `app/src/pages/ios/MascotScreen.tsx`, `app/src/pages/ios/PairScreen.tsx`, `app/src/components/ios/MobileTabBar.tsx`, `app/src/lib/tunnel/`, `packages/tauri-plugin-ptt/`, `src/openhuman/devices/`
- **Impact:** Any fresh clone of upstream `main` has broken CI-checks. Developers must distinguish pre-existing failures from their own. End-to-end pairing requires `tinyhumansai/backend#709` (tunnel socket.io contract) which is unmerged.
- **Fix approach:** Either gate iOS code behind a feature flag / build-time conditional (so it does not interfere with desktop builds) or remove it from the shared workspace until it ships.

### Dynamic Import Ban

- **Issue:** Production `app/src` code is banned from using dynamic imports (`import()`, `React.lazy(() => import(...))`, `await import(...)`). This prevents code splitting, lazy loading of heavy components, and conditional loading of platform-specific code. The only exceptions are Vitest harness patterns and config files.
- **Files:** `CLAUDE.md` (line 347: "No dynamic imports in production `app/src` code"), enforced via code review
- **Impact:** All ~952 frontend files are bundled eagerly. Every route component, settings panel, and modal is loaded on initial mount. This works against web performance best practices and increases baseline memory usage.
- **Fix approach:** Allow dynamic imports for heavy optional paths (settings panels, analytics, MCP transports) behind a `try/catch` guard -- the same pattern recommended for Tauri IPC calls. Update CLAUDE.md to reflect the relaxed rule.

### Pre-existing Flaky Tests (Rust)

- **Issue:** Multiple Rust tests fail intermittently when run as part of the full suite due to shared global state and timing issues, but pass in isolation.
  - `composio::action_tool::tests::factory_routes_through_direct_when_mode_is_direct` -- documented pre-existing failure
  - `composio::action_tool::tests::mode_toggle_between_calls_is_observed` -- flaky in full suite (shared composio session state)
  - `agent::harness::session::turn` -- intermittent failures (shared state or timing)
- **Files:** `src/openhuman/composio/action_tool.rs` (line 417), `src/openhuman/composio/action_tool.rs` tests, `src/openhuman/agent/harness/session/` tests
- **Impact:** `cargo test -p openhuman` cannot be trusted to give a clean signal. Engineers must run failing tests in isolation to determine root cause. CI merges with these failures known.
- **Fix approach:** Isolate shared global state per-test (reset guards, per-test session instances). For composio, avoid shared global session state in tests.

### Ignored Rust Tests

- **Issue:** At least 6 Rust tests are `#[ignore]`-gated for flakiness or external dependencies:
  - `src/core/jsonrpc_tests.rs` -- full server bootstrap, leaks process-global state
  - `src/openhuman/config/schema/load_tests.rs` -- `OPENHUMAN_WORKSPACE` env-var race
  - `src/openhuman/inference/local/service/public_infer_tests.rs` -- flaky timing under load
  - `src/openhuman/inference/provider/factory_test.rs` -- requires live LM Studio or Ollama
  - `src/openhuman/meet_agent/rpc.rs` -- flaky on CI
  - `src/openhuman/scheduler_gate/gate.rs` -- flaky timing
- **Files:** As listed above
- **Impact:** These tests never run in CI, so regressions in these areas are not caught automatically. The ignored tests represent blind spots in coverage.
- **Fix approach:** Address root causes (global state isolation, mock external services, timing resilience) and remove `#[ignore]`.

### E2E Test Gaps

- **Issue:** Out of 66 E2E specs, several are shallow or skipped:
  - `telegram-flow` and `local-model-runtime` are `describe.skip`
  - 4 skills specs are "shallow stubs"
  - `webhooks-ingress-flow` is missing a payload delivery assertion
  - `memory-roundtrip` has an async indexing race (documented RACE-1)
  - `onboarding-modes` has a config.toml write race (documented RACE-2)
  - Some specs use hardcoded pauses rather than condition waits
- **Files:** `docs/e2e-status.md`, `app/test/e2e/specs/`
- **Impact:** Critical user flows (telegram, local AI, webhooks delivery) have no E2E coverage in CI. The documented races cause non-deterministic failures.
- **Fix approach:** Replace hardcoded pauses with deterministic waits. Resolve the two documented race conditions. Implement real specs for the skipped categories.

### Dead Code and Stale Artifacts

- **Issue:** Several components and modules are retained despite being superseded:
  - `MiniSidebar.tsx` retained as backup after the redesign, `BottomTabBar.tsx` is active
  - Old `.claude/rules/15-settings-modal-system.md` doc describes a portal/modal approach that is outdated (Settings is now a full route)
  - `ReferralApplyStep.tsx` preserved but unused since onboarding step was removed
  - `selectHasIncompleteOnboarding` selector defined but unused in production code
  - `load_from_default_paths` (config loader) has zero callers
  - E2E `voice-mode.spec.ts` references legacy button labels that don't match current steps
- **Files:** `app/src/components/navigation/MiniSidebar.tsx`, `app/src/pages/onboarding/components/ReferralApplyStep.tsx`, `app/src/store/selectors/authSelectors.ts`, `src/openhuman/config/schema/load.rs`
- **Impact:** Confusing to new contributors who may use the wrong component or follow stale docs. Dead code needs maintenance attention but provides no value.
- **Fix approach:** Delete unused components and docs, or mark them clearly with deprecation notices. Run a targeted dead-code elimination pass.

### i18n Maintenance Burden

- **Issue:** The i18n system uses 70 chunk files across 13 locales (5 chunks per locale), totaling ~48,192 lines. CI enforces parity via `pnpm i18n:check` which blocks PRs with missing keys in any locale. Adding a single new key requires touching 14 files (the English chunk + the same chunk in all 13 locales).
- **Files:** `app/src/lib/i18n/en.ts`, `app/src/lib/i18n/chunks/{en,ar,bn,de,es,fr,hi,id,it,ko,pt,ru,zh-CN}-{1..5}.ts`
- **Impact:** High translation maintenance cost. The 4 TypeScript compile errors on main are caused by missing iOS i18n deps, not translation gaps, but the chunk system exacerbates merge conflicts during refactoring.
- **Fix approach:** Consider a translation management tool (e.g. Lokalise, Crowdin) with automated PR generation. Alternatively, collapse to English-only for missing locales rather than blocking CI on parity.

---

## Known Bugs

### macOS Tahoe whisper-rs / llama.cpp Build Blocker

- **Symptoms:** `cargo build` fails on macOS Tahoe (Apple Silicon) with clang errors about `-mcpu=native + --target=arm64-apple-macosx` incompatibility. Affects both `whisper-rs` (voice feature) and `llama.cpp` (local AI inference).
- **Files:** `Cargo.toml` dependencies, `~/.cargo/registry/src/.../whisper-rs-sys-0.15.0/build.rs`, `llama.cpp` cmake
- **Trigger:** Apple clang 21+ rejects `-mcpu=native` when `--target` is set. ggml cmake sets `GGML_NATIVE=ON` by default. `cargo check` and `cargo build` both fail.
- **Workaround:** Patch whisper-rs-sys build.rs to add `config.define("GGML_NATIVE", "OFF")`. For `cargo check`, set env `GGML_NATIVE=OFF`. The patch is fragile -- resets on `cargo clean`, version bumps, or registry re-download.
- **Fix approach:** Needs upstream patch in `whisper-rs-sys` or a Cargo feature to opt out of `GGML_NATIVE` on Apple Silicon cross-builds.

### Upstream main CI Broken (5 Vitest + 4 TS Errors)

- **Symptoms:** A clean checkout of `tinyhumansai/openhuman` main fails with 5 Vitest test failures and 4 TypeScript compile errors. Caused by missing iOS experimental dependencies.
- **Files:** `pnpm compile` output, `pnpm test:coverage` output
- **Trigger:** Missing npm packages: `@noble/ciphers/chacha`, `@noble/ciphers/webcrypto`, `qrcode.react`, `@tauri-apps/plugin-barcode-scanner`
- **Workaround:** `--no-verify` for unrelated changes. Always stash and run checks on base branch before blaming your PR.
- **Fix approach:** Gate iOS imports behind conditional compilation or make iOS deps optional in package.json.

### Port Conflict Recovery Platform Dependencies

- **Symptoms:** Port conflict recovery via `reap_stale_openhuman_processes` was macOS-only until recently. Linux and Windows implementations are newer and may have edge cases.
- **Files:** `app/src-tauri/src/process_recovery.rs` (line 30: `#[cfg(target_os = "macos")]`), `src/openhuman/connectivity/rpc.rs`
- **Trigger:** Core port 7788 is in use by a stale process at startup. On macOS, the reap logic works; on Linux/Win, newer implementations may have gaps.
- **Workaround:** Manually `lsof -i :7788 && kill <PID>`.
- **Fix approach:** Add tests for Linux and Windows `/proc/<pid>/cmdline` and `wmic` parsing in `process_recovery.rs`.

### Cargo Incremental Stale UI

- **Symptoms:** After a Rust rebuild, the app shows old frontend because Cargo's incremental compilation cache is stale.
- **Files:** `app/src-tauri/Cargo.toml`
- **Trigger:** Rust changes that don't trigger full recompilation of Tauri resources.
- **Fix approach:** Run `cargo clean --manifest-path app/src-tauri/Cargo.toml` and rebuild. Investigate missing `rerun-if-changed` directives in `build.rs`.

### composio Identity Collision Risk

- **Symptoms:** Two separate `TeamUsage` types exist -- one in `creditsApi.ts:24` (billing: cycle budget, limits) and one in `types/team.ts:11` (team model: daily token limit). Different import paths, no collision at runtime, but confusing and could silently cause the wrong type to be used.
- **Files:** `app/src/services/api/creditsApi.ts` (line 24), `app/src/types/team.ts` (line 11)
- **Trigger:** Importing from the wrong path.
- **Fix approach:** Rename one of the types to disambiguate (e.g. `BillingTeamUsage` vs `ModelTeamUsage`).

### "5-hour" Label Stragglers

- **Symptoms:** `LimitPill` label and its hover tooltip in `Conversations.tsx` still say "5h" / "5-hour" after a terminology refactor to "10-hour" (commit 8c52236).
- **Files:** `app/src/pages/Conversations.tsx`
- **Fix approach:** Update the straggler labels to match the current terminology.

---

## Security Considerations

### Path Validation Entry Points

- **Risk:** All file I/O path validation must go through `validate_path` / `validate_parent_path` in `src/openhuman/security/policy.rs`. The string-only `is_path_string_allowed()` is not sufficient on its own -- it is only a first pass. Tool callers under `src/openhuman/tools/impl/filesystem/` must use the async validation functions, not the legacy `is_path_allowed` / `is_resolved_path_allowed`.
- **Files:** `src/openhuman/security/policy.rs` (lines 1544-1603 for `validate_path`, line 1704 for `validate_parent_path`), `src/openhuman/security/policy_tests.rs` (1875+ for tests)
- **Current mitigation:** 100+ security policy tests covering command injection, path traversal, symlink escapes, null bytes, forbidden paths, and credential store protection.
- **Recommendations:** Audit all filesystem tool implementations to ensure they use the correct validation functions. Add a static assertion or lint that flags `is_path_allowed` usage in tool modules.

### validate_parent_path Ordering

- **Risk:** For write operations, `validate_parent_path` MUST be called BEFORE `create_dir_all`. Calling it after allows a symlink attack to create directories outside the workspace before the security check fires (Issue #1927).
- **Files:** `src/openhuman/security/policy.rs`, `.claude/memory.md` (line 257-258)
- **Current mitigation:** Documented rule in `.claude/memory.md`. Some call sites may still be vulnerable.
- **Recommendations:** Audit all `create_dir_all` calls for correct ordering. Add a test that verifies the call order invariant.

### Approval Gate Can Be Disabled

- **Risk:** The approval gate is on by default but can be disabled with `OPENHUMAN_APPROVAL_GATE=0`/`false`. When disabled, `Prompt`-class commands (Network, Install, Destructive) run without user confirmation. While read-only blocking, path hardening, structural guards, and classification remain live, the user-facing approval UX is entirely bypassed.
- **Files:** `src/openhuman/approval/gate.rs`, `src/core/jsonrpc.rs`, `CLAUDE.md` (line 128)
- **Current mitigation:** The gate parks only for interactive chat turns. Background triage/cron turns carry no context and are always allowed through. The 10-min TTL auto-denies unanswered prompts.
- **Recommendations:** Surface the gate status in the settings UI so users know when approval prompting is disabled. Consider making the disabled state require explicit opt-in at startup.

### CEF JS Injection Ban

- **Risk:** Embedded provider webviews (`acct_*` loading third-party origins like `web.telegram.org`, `linkedin.com`, `slack.com`) must not receive new JavaScript injection. Any new Tauri plugin added to `app/src-tauri/src/lib.rs` must be audited for a `js_init_script` call. `tauri-plugin-opener` ships `init-iife.js` by default unless configured with `.open_js_links_on_click(false)`.
- **Files:** `app/src-tauri/src/webview_accounts/`, `app/src-tauri/src/lib.rs`, `CLAUDE.md` (lines 204-216)
- **Current mitigation:** Explicit policy documented in CLAUDE.md and enforced via code review. Legacy injection exists for non-migrated providers (gmail, linkedin, google-meet recipe files + `runtime.js`) but should shrink.
- **Recommendations:** Add a CI check that greps for `js_init_script` or `addScriptToEvaluateOnNewDocument` in Tauri plugin configurations. Audit the existing legacy injection for removal.

### CSP Configuration

- **Risk:** The Content Security Policy in `tauri.conf.json` currently has `https:` in `default-src` and `connect-src` to allow GA4 and other external services. This is permissive by CEF standards. If tightened, it would break analytics and some integrations.
- **Files:** `app/src-tauri/tauri.conf.json`
- **Current mitigation:** Analytics uses `react-ga4` which injects a `<script>` tag at runtime via `gtag.js`. GA4 Measurement Protocol (pure HTTP POST, no script injection) is documented as the fallback if CEF ever tightens `script-src`.
- **Recommendations:** Proactively migrate to GA4 Measurement Protocol to remove the need for `https:` in `script-src`. Audit external script loads.

### Bubblewrap Sandbox (Linux)

- **Risk:** Linux sandboxing via `bubblewrap` (`bwrap`) must NOT include `--share-net` for network-isolated command execution. The `src/openhuman/security/bubblewrap.rs` file has an explicit constraint about this.
- **Files:** `src/openhuman/security/bubblewrap.rs` (line 128)
- **Current mitigation:** Documented constraint in the bubblewrap module.
- **Recommendations:** Verify the constraint is enforced at runtime, not just documented.

### std::process::exit in Production Code

- **Risk:** `std::process::exit()` is called in several production paths: `src/main.rs:150` (exit code 1), `src/core/agent_cli.rs:99/215` (exit 0), `src/openhuman/agent/harness/interrupt.rs:62` (exit 130). And `src/openhuman/agent/agents/loader.rs` has 13 `panic!` calls for unrecoverable configuration errors (e.g., wrong `ToolScope`). These provide no cleanup opportunity.
- **Files:** `src/main.rs`, `src/core/agent_cli.rs`, `src/openhuman/agent/harness/interrupt.rs`, `src/openhuman/agent/agents/loader.rs`
- **Current mitigation:** The `panic!` calls guard against programmer errors (wrong agent tool scope configuration). The `exit()` calls are in CLI-mode execution paths.
- **Recommendations:** Audit whether CLI `exit(0)` should instead drop gracefully through `main()`. Consider replacing `panic!` in agent loader with `anyhow::bail!` for recoverable configuration errors.

---

## Performance Bottlenecks

### Large Codebase Size

- **Problem:** The Rust core has ~88 domain directories, each with multiple files. The frontend has ~952 files. The Tauri shell `lib.rs` is 4353 lines. Several Rust domains are large: `subconscious` (~5729 lines), `tokenjuice` (~3998 lines), `memory` (~14336 lines across sub-modules).
- **Files:** `app/src-tauri/src/lib.rs`, `src/openhuman/subconscious/`, `src/openhuman/tokenjuice/`, `src/openhuman/memory/`
- **Cause:** Organic growth across many features and contributors, no systematic refactoring/splitting.
- **Improvement path:** Split `app/src-tauri/src/lib.rs` into separate Tauri command modules by concern. Refactor `subconscious` and `tokenjuice` into smaller, focused files. Apply the existing guideline of prefer <=500 lines per file.

### CoreStateProvider Blast Radius

- **Problem:** `CoreStateProvider` (875 lines) is consumed by ~25 components. Changes to its state shape or bootstrap behavior affect routes, socket, onboarding, navigation, settings, and hooks.
- **Files:** `app/src/providers/CoreStateProvider.tsx`
- **Cause:** Centralized auth and app state bootstrap pattern.
- **Improvement path:** Extract independent concerns (e.g., runtime status, core config) into separate providers. Use selectors to minimize re-render blast radius.

### Module-Level Cache in useUsageState

- **Problem:** `useUsageState` uses a module-level `_cache` variable with 60s TTL to prevent duplicate API calls when multiple components mount simultaneously. This is a new pattern but not consistently applied across the codebase, leading to inconsistent caching behavior.
- **Files:** `app/src/hooks/useUsageState.ts`
- **Cause:** Pattern introduced for billing phase 1, not generalized.
- **Improvement path:** Extract the module-level cache into a reusable hook or utility (e.g., `useCachedApiCall`). Apply to other high-frequency API calls.

### Build Times

- **Problem:** Rust compilation times are likely long due to the ~88 domain crate size and heavy dependencies (tokio, axum, serde, composio, etc.). The `tauri-cef` submodule adds CEF compilation to the shell.
- **Files:** `Cargo.toml`, `app/src-tauri/Cargo.toml`
- **Improvement path:** Profile crate compilation. Consider splitting the openhuman crate into smaller crates for faster incremental builds. Use `cargo check` instead of `cargo build` during iteration.

---

## Fragile Areas

### CoreStateProvider (High Blast Radius)

- **Files:** `app/src/providers/CoreStateProvider.tsx` (875 lines)
- **Why fragile:** Consumed by ~25 components across routes, socket, onboarding, nav, settings, and hooks. The premature `isBootstrapping: false` bug (issue #413) caused blank Settings screens. The `bootstrapFailCountRef` retry counter bug (issue #2158) produced impossible `attempt 11/5` log messages.
- **Safe modification:** Add new state fields carefully -- ensure all consumers handle the new state correctly. Never change the bootstrap sequence without testing all downstream components. Write tests for the bootstrap logic.
- **Test coverage:** Bootstrap logic is tested implicitly through integration tests, but unit tests for state transitions are limited.

### Onboarding System (Complex State Machine)

- **Files:** `app/src/pages/onboarding/Onboarding.tsx`, `app/src/pages/onboarding/components/`, `app/src/providers/CoreStateProvider.tsx`
- **Why fragile:** 3-step wizard with deferred onboarding, portal-based overlay (z-[9999]), Redux persist for state, workspace flag file (`.skip_onboarding`), and RPC calls for completion status. Multiple race conditions fixed (RPC/Redux race #197, logout state sync). RPC can fail at startup, requiring fallback to Redux state.
- **Safe modification:** Always test both fresh-onboarding and deferred-onboarding paths. Never change the completion flow without updating both Redux and workspace flag. Keep the z-index stacking in mind for portal overlays.
- **Test coverage:** Some E2E tests exist but some reference legacy button labels.

### Config Corruption Recovery

- **Files:** `src/openhuman/config/schema/load.rs` (`parse_config_with_recovery`)
- **Why fragile:** The recovery chain tries primary config -> `.bak` -> archives corrupt file -> `Config::default()`. The `.bak` is now permanent (not deleted on successful save). New config fields must use `#[serde(default = "fn_name")]` not bare `#[serde(default)]` or they silently get `0`/`false` instead of meaningful defaults.
- **Safe modification:** Always define a named default function for new config fields. Never trust bare `#[serde(default)]`.
- **Test coverage:** Config load tests exist but one is `#[ignore]`-gated for env-var race.

### Submodule Dependencies (Vendored Crates)

- **Files:** `app/src-tauri/vendor/tauri-cef/`, `app/src-tauri/vendor/tauri-plugin-notification/`
- **Why fragile:** Two git submodules can drift from upstream. When `tauri-cef` is out of date, the Tauri shell fails to compile with cryptic missing-module errors (e.g., missing `tauri_runtime_cef::audio`). Updates require `git submodule update --remote --checkout`.
- **Safe modification:** Before modifying Tauri shell code, verify the submodule is up-to-date. If a new Tauri version changes the CEF runtime API, the vendor crate must be updated first.
- **Test coverage:** `tauri-cef-pin-guard.yml` workflow checks submodule pin status.

### Tauri Shell lib.rs (Monolithic)

- **Files:** `app/src-tauri/src/lib.rs` (4353 lines)
- **Why fragile:** Single file handling all Tauri commands, setup, window management, deep links, and event wiring. Any change to a single command handler affects the entire module.
- **Safe modification:** Follow the existing pattern of extracting command handlers into the relevant domain module (e.g., `core_process.rs`, `cdp/mod.rs`). Do not add new commands directly to `lib.rs` -- add them in a focused module and register in `lib.rs`.
- **Test coverage:** Limited due to Tauri's AppHandle dependency. Some unit tests for extracted pure functions (see `classify_request` extraction pattern).

### Approval Gate State

- **Files:** `src/openhuman/approval/gate.rs`, `src/openhuman/agent/harness/tool_loop.rs`
- **Why fragile:** The approval gate is a `tokio` task-local that parks interactive chat turns. Background turns bypass it entirely. The `OPENHUMAN_APPROVAL_GATE=0` flag disables it completely (Prompt-class commands run unprompted). The gate publishes `DomainEvent::ApprovalRequested` which the frontend bridges to socket events -- any break in this chain silently runs commands without approval.
- **Safe modification:** Never change the gate enable/disable logic without testing both interactive and background paths. The 10-min TTL auto-deny is a safety net; changing it changes the security posture.
- **Test coverage:** Unit tests for gate decisions exist. Integration tests for the full approve/deny UX flow are limited.

---

## Scaling Limits

### Rust Core Domain Count

- **Current:** ~88 domain directories under `src/openhuman/`, each with multiple files. The `src/core/all.rs` registry must wire every domain's controllers and schemas.
- **Limit:** Compilation time grows with each domain. The controller registry pattern means every domain must be registered in `all.rs` -- forgetting a domain silently excludes its RPC methods.
- **Scaling path:** Consider grouping domains into sub-crates (e.g., `openhuman-memory`, `openhuman-channels`) with their own controller registries. Add a compile-time check that verifies all expected domains are registered.

### i18n Chunk Files

- **Current:** 13 locales x 5 chunks + 5 English source chunks = 70 files, ~48,192 lines total.
- **Limit:** Each new locale adds 5 files that must maintain parity. Each new key touches 14 files. Merge conflicts are frequent.
- **Scaling path:** Migrate to a translation management platform with automated PR generation. Investigate lazy-loaded locale files to reduce bundle size.

### E2E Test Suite

- **Current:** 66 specs across 11 categories. Some are shallow stubs or `describe.skip`.
- **Limit:** Running the full suite is slow (some specs have 30s waits). CI parallelism is required. Adding new specs without addressing existing race conditions reduces confidence.
- **Scaling path:** Resolve the two documented race conditions (RACE-1, RACE-2). Replace hardcoded waits with condition-based waits across all specs. Implement proper specs for skipped categories before adding new ones.

---

## Dependencies at Risk

### CEF Vendored Tauri CLI

- **Risk:** Vendored `tauri-cef` is a git submodule maintained separately from the main Tauri project. If Tauri 3.x ships breaking changes to the CEF runtime bridge, the vendor crate must be updated to match. There is no automated sync.
- **Impact:** Build failure on macOS and Linux. Stock `@tauri-apps/cli` produces a broken bundle with no viable fallback.
- **Migration plan:** Upstream the CEF changes to the official Tauri repository. Monitor the `#tauri-cef` submodule for upstream compatibility.

### whisper-rs / ggml (macOS)

- **Risk:** `whisper-rs` 0.16 and `llama.cpp` both fail on Apple clang 21+ with `GGML_NATIVE=ON`. The fragile workaround (patching the registry crate) resets on every `cargo clean` or crate version bump.
- **Impact:** Voice features and local AI inference cannot be built on the latest macOS toolchain without manual patching.
- **Migration plan:** Either upstream the `GGML_NATIVE=OFF` workaround or switch to a different ASR/inference backend that doesn't depend on ggml cmake.

### tinyhumansai/backend (External API)

- **Risk:** Several features depend on the `tinyhumansai/backend` service: tunnel socket.io for iOS pairing (PR #709), billing API, composio backend. If the backend API changes, the client must be updated in lockstep.
- **Impact:** iOS pairing is blocked on PR #709 being merged and deployed. Config/env variable normalization (`normalize_backend_api_base_url`) was recently fixed to handle scheme-less URLs -- a sign of integration fragility.
- **Migration plan:** Add integration tests that mock the backend API (already partially done via `scripts/mock-api-core.mjs`). Document backend API version expectations in the capability catalog.

---

## Missing Critical Features

### Graceful max_iterations Truncation

- **Problem:** When an agent hits `max_iterations`, `tool_loop.rs:705` calls `anyhow::bail!`. There is no graceful truncation -- the agent's work is lost without summary or partial results.
- **Files:** `src/openhuman/agent/harness/tool_loop.rs` (line 705)
- **Blocks:** Long-running agents cannot produce partial results when hitting iteration limits. Users see a hard error instead of "I ran out of steps, here's what I did."
- **Priority:** Medium

### Cron Loop Auto-Start

- **Problem:** The cron scheduler loop was never spawned at startup until recently fixed. Without it, scheduled jobs never auto-fire. The fix was gating on `config.cron.enabled`.
- **Files:** `src/core/jsonrpc.rs`, `src/openhuman/cron/scheduler.rs`
- **Blocks:** Scheduled jobs (daily summaries, periodic actions) don't run after app restart unless the cron flag is explicitly enabled.
- **Priority:** Fixed (issue #830), but any change to the startup sequence could re-break this.

### E2E Local Model Testing

- **Problem:** `local-model-runtime` E2E spec is `describe.skip`. Local AI model download and inference cannot be verified in CI.
- **Files:** `app/test/e2e/specs/`
- **Blocks:** Confidence in local AI feature regressions.
- **Priority:** Medium

---

## Test Coverage Gaps

### CoreStateProvider Bootstrap

- **What's not tested:** The full bootstrap sequence (RPC fetch, retry logic, timeout handling, fallback to Redux state) has limited unit test coverage. The `bootstrapFailCountRef` bug was caught by log inspection, not tests.
- **Files:** `app/src/providers/CoreStateProvider.tsx`
- **Risk:** A change to the bootstrap sequence could cause blank Settings, incorrect route gating, or onboarding loops -- discovered only in manual testing.
- **Priority:** High

### Security Policy Integration Tests

- **What's not tested:** The security policy unit tests (100+ tests) are thorough for the policy module itself. However, end-to-end tests that verify the policy is actually enforced through the full tool execution chain (RPC -> harness -> tool -> policy -> result) are limited.
- **Files:** `src/openhuman/security/policy.rs`, `src/openhuman/security/policy_tests.rs`
- **Risk:** A tool implementation could bypass the policy by not calling `validate_path`/`validate_parent_path`, and this would not be caught by policy unit tests.
- **Priority:** Medium

### Port Conflict Recovery (Cross-Platform)

- **What's not tested:** `process_recovery.rs` has tests for macOS but the Linux and Windows implementations (`/proc/<pid>/cmdline` parsing, `wmic` output parsing) have gaps.
- **Files:** `app/src-tauri/src/process_recovery.rs`
- **Risk:** Port conflict recovery silently fails on Linux or Windows, leaving stale processes and preventing the app from starting on the expected port.
- **Priority:** Medium

### Tauri Shell Commands

- **What's not tested:** The 4353-line `lib.rs` has minimal unit test coverage. Most Tauri commands are only tested through E2E tests, which are slow and some are `describe.skip`.
- **Files:** `app/src-tauri/src/lib.rs`
- **Risk:** Changes to Tauri command handlers can introduce regressions that are only caught during manual testing or slow E2E runs.
- **Priority:** Low

---

*Concerns audit: 2026-06-04*
