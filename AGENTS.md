# Portable XState Agent Guidelines

## 1. Core Goal & XState 5 Compatibility Invariant

- **Strict XState 5 Compatibility**: The primary goal of `portable-xstate` is strict compatibility with XState 5 state machine definitions and full compliance with the upstream XState 5 conformance test suite (`statelyai/xstate`), except where fundamentally constrained by the absence of a JavaScript runtime (e.g., pure JSON/Rust/FFI boundaries where arbitrary JS callbacks cannot execute).
- **Tests as Source of Truth**: Upstream XState 5 conformance tests are the source of truth. **NEVER** modify test logic, assertions, or state machine definitions to make a test pass.
- **Documenting Non-JS Constraints**: If a test or sub-feature is genuinely constrained by the lack of a JavaScript runtime, do not silently alter or delete it—explicitly confirm with the user and document the constraint in `XSTATE_5_FEATURES.md` (and `README.md` if architectural).

## 2. Mandatory Feature & Test Implementation Process (`PROCESS.md`)

Whenever adding functionality or enabling conformance tests in `portable-xstate`, you **MUST** strictly follow [PROCESS.md](./PROCESS.md):

1. **Select Target**:
   - Verify all existing tests pass (`npm test`).
   - Identify the next feature set from [XSTATE_5_FEATURES.md](./XSTATE_5_FEATURES.md).
   - Copy the unmodified upstream test content from the linked `statelyai/xstate` repository URL into `tests/`.
   - **STOP GATE 1**: Stop immediately and wait for the user to confirm the copied test content and check it into Git before making any edits.

2. **Setup Test**:
   - Make **minimal compilation fixes only** in `tests/<filename>.test.ts` (e.g., updating imports or adding explicit TypeScript type annotations like `ctx: any`). Do **NOT** change test logic.
   - Run the test (`npx vitest tests/<filename>.test.ts`) to verify it compiles and fails as expected.
   - **STOP GATE 2**: Stop immediately and wait for the user to confirm the test compiles and fails as expected, and check it into Git.

3. **Analyze & Implement**:
   - Run the targeted test via `npx vitest tests/<filename>.test.ts`.
   - Implement the required functionality in Rust without modifying the test logic:
     - `portable-xstate-rust`: Core statechart semantics, runtime logic, and JSON deserialization (`src/json.rs`, `src/parser.rs`).
     - `portable-xstate-wasm` / `portable-xstate-c-cpp`: WASM and C/C++ adapter surfaces.

4. **Clean Up, Verify & Document**:
   - Run `cargo clippy` across the Rust workspace and fix all warnings.
   - Remove temporary debug statements (`println!`, `console.log`).
   - Check for console errors in browser/WASM tests where applicable.
   - Run the full test suite (`npm test`) to verify zero regressions across native, WASM, and E2E suites.
   - Update [XSTATE_5_FEATURES.md](./XSTATE_5_FEATURES.md) (and [README.md](./README.md) if significant capabilities were added).

## 3. PlatformIO Integration Maintenance (`library.json` & `build_rust.py`)

PlatformIO support lives directly in this repository (`library.json`, `build_rust.py`, `portable-xstate-c-cpp/tests/test_build_rust.py`, and `examples/stoplight-esp32`):

- **Build Script & Manifest**: If crate names, directory paths, Cargo flags, or static library names in `portable-xstate-c-cpp` change, update `library.json`, `build_rust.py`, and `portable-xstate-c-cpp/tests/test_build_rust.py` in lockstep.
- **Headers & Examples**: Keep `portable-xstate-c-cpp/include/portable_xstate.h`, `portable_xstate.hpp`, and `examples/stoplight-esp32` aligned with any C/C++ API changes.
- **Verification**: Always run `npm run test:c-cpp` (which executes C, C++, and `test_build_rust.py` unit tests) before claiming completion.
