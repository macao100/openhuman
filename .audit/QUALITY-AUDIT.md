# Audit Stage 3 - Code Quality

**Project**: DADOU (OpenHuman)
**Date**: 2026-06-16
**Scope**: Rust (src/) + TypeScript/React (app/src/)
**Audit type**: Read-only, structural analysis

---

## Executive Summary

| Dimension | Score |
|-----------|-------|
| **File size discipline** | **2/10** - 135 Rust files exceed the 800-line guideline. 318 exceed 500 lines. 29 production TS files exceed 500 lines. |
| **Dead code management** | **5/10** - Knip finds 28 unused files and 88 unused exports in TS. 46 cargo-check warnings in Rust. |
| **Function granularity** | **4/10** - Multiple functions exceed 50 lines by large margins (e.g. build_deterministic_checkpoint at ~2370 lines). |
| **Nesting discipline** | **6/10** - Deep nesting in largest files (turn.rs, observability.rs, policy.rs). Smaller modules are better. |
| **Naming conventions** | **7/10** - Mostly consistent. Minor TSX naming issues. |
| **TODO hygiene** | **4/10** - ~1500 TODO/FIXME lines in Rust, ~952 in TS. Mostly unticketed. |
| **Code duplication** | **5/10** - Structural similarity across store, i18n, test files. |
| **Console.log in production** | **3/10** - 211 console.log/warn/error calls in ~20 production files. |

**Overall Score: 4.5 / 10** - The codebase has severe scaling issues. File size discipline is the dominant problem: 135 Rust files over 800 lines makes effective review and maintenance extremely difficult.


## 1. Fichiers Oversized

### 1.1 Rust files > 800 lines

**CRITICAL** - 135 files exceed the project 800-line guideline. This is a systemic pattern.

Top 20: 3244 observability.rs, 2513 memory/read_rpc.rs, 2475 session/turn.rs,
2385 composio/ops.rs, 2337 policy_tests.rs, 2330 mcp_server/tools.rs,
2292 channels/providers/web.rs, 2240 config/schema/load.rs, 2170 core/jsonrpc.rs,
2155 inference/provider/compatible.rs, 2052 security/policy.rs, 2029 memory_store/chunks/store.rs,
1971 subagent_runner/ops.rs, 1876 telegram/channel_tests.rs, 1859 composio/ops_test.rs,
1803 tool_loop.rs, 1784 load_tests.rs, 1719 test_support_test.rs, 1696 builder.rs, 1673 config/ops.rs

### 1.2 TypeScript/React files > 500 lines (production)

**HIGH** - 29 production files exceed the 500-line guideline.

Key files: 3468 AIPanel.tsx, 2093 Conversations.tsx, 1492 webviewAccountService.ts,
1238 VoicePanel.tsx, 1197 Skills.tsx, 977 ComposioConnectModal.tsx,
953 ChatRuntimeProvider.tsx, 881 CoreStateProvider.tsx, 806 BootCheckGate.tsx,
796 memoryTree.ts, 729 chatService.ts, 728 OverlayApp.tsx, 607 coreRpcClient.ts, 544 socketService.ts

---

## 2. Dead Code

### 2.1 Knip results (TypeScript frontend)

**HIGH** - 28 unused files, 6 unused dependencies, 88 unused exports.

Unused files: ConnectionBadge.tsx, LottieAnimation.tsx, useConsciousItems.ts,
useIntelligenceApiFallback.ts, useIntelligenceStats.ts, useScreenIntelligenceItems.ts,
GoogleIcon.tsx, Card.tsx, Input.tsx, TunnelList.tsx, WebhookActivity.tsx,
skillsAgentContext.ts, 5 onboarding pages/steps, 5 billing subcomponents.

Unused dependencies: @remotion/player, @remotion/zod-types, @tauri-apps/plugin-os,
lottie-react, react-ga4, remotion.

Notable unused exports: personaSlice, mascotSlice, ApprovalRequestCard,
isIOS, isAndroid, notificationSlice, subscribeDaemonStore and 81 others.

### 2.2 Rust compiler warnings

**MEDIUM** - 46 cargo check warnings (unused imports, unused variables, unnecessary mut).
Includes guardian types (StructuredPlan, N3Config), fs2::FileExt, HANDLE, File,
ContextEligibility, MIN_CONTEXT_TOKENS, evaluate_context, Path/PathBuf imports.


## 3. Long Functions

**CRITICAL** - Several functions dramatically exceed the 50-line guideline.

- session/turn.rs: build_deterministic_checkpoint (~2370 lines) - 96% of the file
- security/policy.rs: has_hidden_execution (~961 lines) - too large to audit
- channels/providers/web.rs: spawn_progress_bridge (~480 lines)
- channels/providers/web.rs: start_chat (~303 lines)
- security/policy.rs: contains_unquoted_char (~255 lines)
- core/observability.rs: report_expected_message (~254 lines)
- core/jsonrpc.rs: bootstrap_core_runtime (~220 lines)
- memory/read_rpc.rs: delete_chunk_rpc (~170 lines)
- core/jsonrpc.rs: register_domain_subscribers (~132 lines)
- mcp_server/tools.rs: call_tool (~129 lines)
- memory/read_rpc.rs: recall_rpc (~127 lines) - core/jsonrpc.rs: rpc_handler (~133 lines)

---

## 4. Deep Nesting

**MEDIUM** - Deep nesting exists primarily in the largest files.
- session/turn.rs: 6-7 levels in main agent loop
- core/observability.rs: 8+ levels in error classification chains
- security/policy.rs: deeply nested conditionals across 961-line function
- config/schema/load.rs: complex migration chains
Smaller modules maintain good nesting discipline with early returns.

---

## 5. Naming Conventions

**Rust: PASS** - All modules follow snake_case. Only grandfathered files at src/openhuman/ root.
4 subdirectories lack mod.rs: dashboard/web, people/migrations, tokenjuice/tests, tokenjuice/vendor.

**TypeScript/React: LOW** - 6 files use camelCase instead of PascalCase:
- providerIcons.tsx, toolkitMeta.tsx, toolkitMeta.test.tsx
- providerConfigs.tsx, skillIcons.tsx

---

## 6. Standalone .rs Files at src/openhuman/ Root

**PASS** - Only dev_paths.rs and util.rs (grandfathered) plus mod.rs exist. Rule respected.


## 7. TODO/FIXME Comments

**HIGH** - High volume of unactioned technical debt.
- Rust: 14 files, ~1500 TODO/FIXME lines
- TypeScript: 12 files, ~952 TODO/FIXME lines

Notable unticketed TODOs:
- devices/rpc.rs:219 - backend revoke endpoint pending (PR #709 follow-up)
- memory/preferences.rs:213 - use provenance when Memory::store accepts it
- memory/provenance/decay.rs:141 - TODO(MEM-04) - wire into cron scheduler
- memory_queue/store.rs:346 - TODO(multi-process)
- composio/auth_retry_tests.rs:204 - TODO(composio-retry-dedup)
- PairPhoneModal.tsx:9 - replace poll with socket event bridge
- transport/profileStore.ts:6 - iOS TODO(Layer 5)
- transport/profileStore.ts:97 - SECURITY TODO(post-Layer-7)
- 10 i18n chunk files: translate custom GIF mascot strings

Only provenance/decay.rs references an issue number (MEM-04). The rest are dangling.

---

## 8. Code Duplication

**MEDIUM** - Several structural duplication patterns.

1. Store CRUD boilerplate across cron, notifications, memory_store (rusqlite patterns)
2. i18n chunk files: 14 locales x 5 chunks = 70 nearly identical files (~1350-1390 lines each)
3. Test files with repetitive AAA blocks (~1880 lines in composio tests)
4. Config types.rs vs load.rs: field co-evolution risk
5. Provider scanner modules share CDP-based architecture (intentional pattern)

---

## Top 10 Most Problematic Files

| Rank | File | Lines | Key Issues |
|------|------|-------|------------|
| 1 | session/turn.rs | 2475 | 2370-line function; deepest nesting |
| 2 | security/policy.rs | 2052 | 961-line function; security-critical |
| 3 | core/observability.rs | 3244 | Largest file; nested match chains |
| 4 | channels/providers/web.rs | 2292 | 480+303 line functions |
| 5 | AIPanel.tsx | 3468 | Largest TS file; single panel |
| 6 | memory/read_rpc.rs | 2513 | Multiple 100+ line RPC handlers |
| 7 | composio/ops.rs | 2385 | Long functions; large test file |
| 8 | core/jsonrpc.rs | 2170 | Bootstrap+routing+errors in one file |
| 9 | Conversations.tsx | 2093 | Large page component |
| 10 | webviewAccountService.ts | 1492 | Mega-service |


---

## Recommendations (Prioritized)

### Immediate (next sprint)
1. **[CRITICAL] Split turn.rs** - Extract build_deterministic_checkpoint (~2370 lines) into dedicated module.
2. **[CRITICAL] Split policy.rs::has_hidden_execution** - The 961-line function makes security audit impossible.
3. **[HIGH] Remove dead code from knip report** - 28 unused files, 6 unused deps.
4. **[HIGH] Split AIPanel.tsx (3468 lines)** - Extract sub-panels by domain.

### Short-term (next 2-3 sprints)
5. **[HIGH] Tackle top 10 oversized files** - Create refactoring issues for each.
6. **[HIGH] Clean up 211 console.log calls** - Focus on desktopDeepLinkListener.ts (20) and useDaemonLifecycle.ts (14).
7. **[MEDIUM] Ticket all unticketed TODOs** - Add issue references.
8. **[MEDIUM] Fix 46 Rust compiler warnings** - Remove unused imports, fix mut.
9. **[MEDIUM] Fix 6 non-PascalCase filenames** - Rename to PascalCase.

### Long-term
10. **[LOW] Audit unused Redux slices** - personaSlice, mascotSlice, notificationSlice.
11. **[LOW] Extract store CRUD helpers** - Generic SQLite CRUD trait.
12. **[LOW] Investigate dashboard/web** - Missing mod.rs.

---

## Review Summary

| Severity | Count | Status |
|----------|-------|--------|
| CRITICAL | 3 | fail - 135 Rust files >800 lines; 2370-line function; 961-line security function |
| HIGH     | 7 | fail - 29 TS files >500 lines; 28 unused files; 211 console.log calls; 1500+ TODOs |
| MEDIUM   | 5 | warn - 46 Rust warnings; 88 unused exports; structural duplication |
| LOW      | 3 | note - 6 non-PascalCase TSX files; naming generally consistent |

**Verdict: BLOCK** - The file size crisis alone warrants a block. The codebase has significant systemic quality issues that compound each other: oversized files make review harder, which lets more dead code and TODOs accumulate.

**Recommended action**: Create a dedicated Quality Sprint to split the top 10 files by line count and eliminate the knip-reported dead code before further feature work.
