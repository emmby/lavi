# Testing & Implementation Process

This document defines the autonomous workflow for implementing features and passing conformance tests in `lavi`—whether working as a single agent or as part of a parallel agent swarm.

**Required Reading First**: Always read [`AGENTS.md`](./AGENTS.md) for the 4-Level Decision Hierarchy, Microcontroller Resource Envelope, and Canonical vs. Legacy Anti-Patterns table.

---

## 1. Select Target & Isolate Workspace
- When swarming in parallel with other agents, run in an isolated git worktree (`Workspace: "branch"`) scoped to a single feature from [`XSTATE_5_FEATURES.md`](./XSTATE_5_FEATURES.md) or a single cleanup item from [`TECH_DEBT.md`](./TECH_DEBT.md).
- Verify the baseline suite is green before touching code:
  ```bash
  npm test
  ```
  *(Note: `npm test` automatically runs `npm run test:conformance-check` first).*

## 2. Automated Conformance Test Setup
- **Fetch & Pin Upstream Test (if adding a new suite)**:
  ```bash
  node scripts/fetch-upstream-test.mjs <filename>.test.ts
  ```
  This saves the unmodified upstream `statelyai/xstate@5.25.0` file to `tests/upstream/<filename>.test.ts` and records its SHA-256 hash in `tests/upstream/UPSTREAM_VERSION.json`.
- **Copy & Apply Whitelisted Adaptations Only**:
  Copy `tests/upstream/<filename>.test.ts` to `tests/<filename>.test.ts` and register `<filename>.test.ts` in `tests/conformance-manifest.json` with `"status": "strict"`.
  You may make **only** these three mechanical adaptations in `tests/<filename>.test.ts`:
  1. Rewrite module imports from `'../src/...'` to `'../lavi-wasm/pkg-npm/lavi'` (and `'./stubs.ts'` / `'./utils.ts'`).
  2. Add explicit `: any` TypeScript parameter annotations where required for compilation without XState's full TS generic inference.
  3. Change `it(` / `describe(` to `it.skip(` / `describe.skip(` for sub-features outside your current task scope, **keeping the inner test body 100% unmodified** and registering each skipped title in `tests/conformance-manifest.json` under `allowed_skips`.
- **Verify Provenance & Watch the Target Test Fail (RED)**:
  ```bash
  npm run test:conformance-check
  npx vitest run tests/<filename>.test.ts
  ```
  Once `npm run test:conformance-check` passes, **do not edit `tests/<filename>.test.ts` again** except to remove `it.skip` as tests pass (never add or register new `it.skip` entries for in-scope tests during implementation).
- **Working on Suites Currently Marked `"legacy-baseline"` (`TD-1`..`TD-3`)**:
  If your assigned feature lives inside a test file still marked `"status": "legacy-baseline"` in `tests/conformance-manifest.json` (such as `tests/after.test.ts` or `tests/actions.test.ts`), its `legacy_baseline_sha256` lock will reject any ad-hoc edits. You must either complete that file's graduation task (`TD-1`, `TD-2`, or `TD-3` in [`TECH_DEBT.md`](./TECH_DEBT.md)) first so it matches `tests/upstream/<filename>.test.ts` under `"status": "strict"`, or add your feature coverage in a new strict test / native Rust + C/C++ suite while keeping the frozen legacy file untouched.

## 3. Cross-Target Implementation (Rust $\rightarrow$ WASM $\rightarrow$ C/C++ $\rightarrow$ Python $\rightarrow$ Java/Kotlin $\rightarrow$ Swift $\rightarrow$ Dart $\rightarrow$ C#)
Statechart features apply across the core engine and its target language bindings:

1. **Core Portable Engine (`lavi`)**:
   - Implement the statechart semantics in `lavi/src/runtime.rs` (alongside `src/types.rs`, `src/snapshot.rs`, `src/clock.rs`, and re-exports in `src/lib.rs`) and deserialization in `lavi/src/json.rs` / `lavi/src/parser.rs`.
   - Enforce embedded invariants: `#![no_std]` + `alloc` only, microcontroller dependency policy (`indexmap` by default; ask for user approval before adding any new `#![no_std]` crate rather than reinventing non-trivial data structures), iterative heap `Vec` work-stacks (zero CPU call-stack recursion), and move semantics.
   - Add native Rust unit tests (`cargo test`).
2. **WASM / JS Adapter (`lavi-wasm`)**:
   - Bridge JS APIs and inline JS functions (`js_sys::Function` $\rightarrow$ synthetic named callbacks) in `lavi-wasm/src/` (`actor.rs`, `actions.rs`, `state_node.rs`, `value_bridge.rs`, `lib.rs`).
   - Rebuild WASM (`npm run build:npm`) and run `npx vitest run tests/<filename>.test.ts` until the target conformance tests pass unmodified.
3. **C & C++ Embedded Adapter (`lavi-c-cpp`)**:
   - Expose any new runtime capabilities idiomatically in C (`lavi-c-cpp/include/lavi.h` and `src/lib.rs`) using opaque handles and function pointers with `void* user_data`, and in C++11 (`lavi-c-cpp/include/lavi.hpp`) using RAII and move-safe methods.
   - Add unit tests in `lavi-c-cpp/tests/c_test.c` and `lavi-c-cpp/tests/cpp_test.cpp` and run `npm run test:c-cpp`.
4. **Python Adapter (`lavi-py`)**:
   - Expose new runtime capabilities idiomatically in Python via PyO3 (`lavi-py/src/lib.rs`) and update type stubs in `lavi-py/python/lavi/__init__.pyi`.
   - Run `npm run test:py` to verify with pytest.
5. **Java & Kotlin Adapter (`lavi-jvm`)**:
   - Expose new runtime capabilities in JNI (`lavi-jvm/src/lib.rs`), Java (`lavi-jvm/src/main/java/io/lavi/Machine.java`), and Kotlin (`lavi-jvm/src/main/kotlin/io/lavi/MachineExtensions.kt`).
   - Run `npm run test:jvm` to verify both Java and Kotlin JUnit test suites.
6. **Swift Adapter (`lavi-swift`), Dart / Flutter Adapter (`lavi-dart`) & C# / .NET Adapter (`lavi-csharp`)**:
   - When new C ABI capabilities are added in `lavi.h`, expose them in Swift (`lavi-swift/Sources/Lavi/Lavi.swift`), Dart (`lavi-dart/lib/src/bindings.dart` & `lavi-dart/lib/lavi.dart`), and C# (`lavi-csharp/Lavi/NativeMethods.cs` & `lavi-csharp/Lavi/Machine.cs`).
   - Run `npm run test:swift`, `npm run test:dart`, and `npm run test:csharp` to verify all three test suites.

## 4. Handling Legacy Anti-Patterns & Host-Language Boundaries
- **Spotted Legacy Debt**: If you see existing code using recursion, XState 4 naming (`cond`, `type_`), manual `actorRef.tick()` in tests, or debug hacks in `tests/utils.ts`, **do not copy it** and **do not refactor unrelated files mid-feature**. Append a scoped entry to [`TECH_DEBT.md`](./TECH_DEBT.md).
- **Host-Language Boundaries**: Never build dynamic expression parsers or JS interpreters in Rust, C, or C++. If an upstream test requires pure interpreted JS dynamism that cannot be bridged via WASM callbacks, document the constraint in [`XSTATE_5_FEATURES.md`](./XSTATE_5_FEATURES.md) and register the skip in `tests/conformance-manifest.json`.

## 5. Clean Up, Verify & Document
Before committing or claiming completion, run the full verification checklist:
1. **Lint Rust Workspace**:
   ```bash
   cargo clippy --workspace --all-targets -- -D warnings
   ```
2. **Remove Debug Artifacts**: Ensure zero temporary `println!`, `console.log`, or file writes remain.
3. **Rebuild & Run Full Test Suite**:
   ```bash
   npm run build
   npm test
   ```
   Verify that all stages (`test:conformance-check`, `test:native`, `test:wasm`, `test:e2e`, `test:c-cpp`, `test:py`, `test:jvm`, `test:swift`, `test:dart`, `test:csharp`, `test:examples`, and `test:unit`) pass.
4. **Single-Host Release Packaging Verification (Optional / Pre-Release)**:
   ```bash
   node scripts/build-release-natives.mjs
   ```
   Generates the release plan and verifies 16 KB page-size alignment for Android and desktop target distributions.
5. **Update Documentation (Verified by `npm run test:conformance-check`)**:
   - Update the Conformance Test Suites table and feature checkboxes in [`XSTATE_5_FEATURES.md`](./XSTATE_5_FEATURES.md) (and [`TECH_DEBT.md`](./TECH_DEBT.md)). Note that `npm run test:conformance-check` mechanically verifies that `XSTATE_5_FEATURES.md` stays in sync with `tests/conformance-manifest.json`, active `it.skip`/`describe.skip` counts, and `lavi.h`.
   - Update [`README.md`](./README.md) if public APIs or capabilities changed.
