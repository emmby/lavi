# Testing & Implementation Process

This document outlines the standard operating procedure for enabling and passing the next set of tests in the `portable-xstate` project.

## 1. Select Target
- Verify all tests pass.
- Identify the next feature set from `XSTATE_5_FEATURES.md`.
- **Copy Tests**: Copy the test content from the linked XState repository URL.
- **STOP HERE**. Do not proceed until the user has confirmed the test content has been copied correctly. The user will check it in.

## 2. Setup Test
- **Minimal Compilation Fixes**: Create a new test file in `tests/` and make **minimal changes** to enable compilation (e.g. imports), but do NOT change logic.
- **Verify Failure**: Run the test to ensure it fails as expected.
- **Git Check-in**: Commit the failing test file by itself to establish a baseline.
  ```bash
  git add tests/<filename>.test.ts
  git commit -m "chore: add <filename> test suite"
  ```
  *This ensures we can later verify that no test logic was altered.*

## 3. Run Tests
- Run the specific test file using Vitest:
  ```bash
  npx vitest tests/<filename>.test.ts
  ```
- **Constraint**: Do NOT modify the test files to make them pass. The tests are the source of truth.

## 4. Analyze & Implement
- Read the failure output carefully.
- Modify the implementation code to satisfy the test requirements.
  - **Core Logic**: Modify `portable-xstate-core` (Rust).
  - **JSON/DTO**: Modify `portable-xstate-json` (Rust) for deserialization.
  - **Wasm Adapter**: Modify `portable-xstate` (Rust/Wasm) for the API surface.
- Re-run the specific test frequently.

## 5. Specific Issues & Fixes
- **Implicit Any**: Fix TypeScript errors in the test files if they are just type annotations (like `ctx: any`), but do not change the logic.
- **NotImplemented**: Implement missing features in Rust.

## 6. Clean Up
- Once functionality works:
  - Run `cargo clippy` in the Rust crates and fix warnings.
  - Remove debug `println!` or `console.log` statements.
  - Check for console errors in the browser tests (if applicable).

## 7. Verify & Document
- Run the full test suite to ensure no regressions:
  ```bash
  npm test
  ```
- Update `XSTATE_5_FEATURES.md` to check off newly supported features.
- Update `README.md` if significant new capabilities are added.
