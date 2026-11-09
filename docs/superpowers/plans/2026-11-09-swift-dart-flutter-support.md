# Swift and Dart/Flutter Bindings & CI/CD Support Implementation Plan [flowos: 20edd44c-8fd3-4939-9eb4-a543f8c5bc22]

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add idiomatic Swift (`Package.swift` + `lavi-swift/`) and Dart/Flutter (`lavi-dart/`) bindings over `lavi.h`, runnable examples (`examples/swift/`, `examples/dart/`), and full CI/CD and release support.

**Architecture:** Reuse the zero-overhead `#![no_std]` C-ABI in `lavi-c-cpp` (`lavi.h`). Expose a `CLavi` Clang module and `Lavi` Swift wrapper via root `Package.swift` for SwiftPM/Xcode and Linux Swift, and a `dart:ffi` package in `lavi-dart/` staged via `scripts/stage-dart-native.mjs`. Integrate both into `package.json`, `scripts/sync-version.mjs`, `scripts/build-release-natives.mjs`, `.github/workflows/ci.yml`, and `.github/workflows/release.yml`.

**Tech Stack:** Rust (`lavi-c-cpp`), C (`lavi.h`), Swift 5.9+ (`Package.swift`, `XCTest`), Dart 3+ (`dart:ffi`, `package:ffi`, `package:test`), GitHub Actions (`ubuntu-24.04` & `macos-15`).

**Spec:** `docs/superpowers/specs/2026-11-09-swift-dart-flutter-support-design.md`

## Global Constraints

- Never modify conformance test files (`tests/*.test.ts` except `tests/build-release-natives.test.ts` which is in `META_TEST_FILES`).
- Keep `lavi-c-cpp/Cargo.toml` `crate-type = ["staticlib"]` so bare-metal `thumbv*-none-eabi*` targets and static C/C++ linker tests remain unaffected; build the Dart shared library via `cargo rustc --package lavi-c-cpp --lib -- --crate-type cdylib` and move it into `lavi-dart/native/`.
- All manifest versions must stay synchronized at `0.1.0` via `node scripts/sync-version.mjs --check`.

## Review Focus

- Calling `Machine` methods before `start()` or after `close()`/`dispose()` in Swift and Dart must return safe default values (`false`, `""`, `nil`/`null`) without crashing or dereferencing null pointers.
- Re-registering an action, guard, or delay with the same name (or switching between parameterized and unparameterized callbacks) must replace the previous callback cleanly without leaking or invoking stale closures.
- Restoring a snapshot (`restoreSnapshot`) on an unstarted `Machine` in Swift and Dart must lazily initialize the underlying `MachineContext*` and bind registered handlers before calling `fsm_restore_snapshot_json`, matching `lavi.hpp`.
- `SimulatedClock` bound to a `Machine` must automatically advance `after` transitions when `clock.increment()` or `clock.set()` is called, and detaching/closing the `Machine` before the `SimulatedClock` must not cause a use-after-free when the clock is later incremented.
- `Value` property lookup on non-object or null `fsm_value_t*` pointers must safely return a null `Value` (`isNull == true`) rather than crashing.

---

## Phase 1: Swift Bindings & Example [flowos: 042a3632-7f06-4f9b-ae00-a7a990c66bfd]

### - [x] Task 1: Implement CLavi C module and Lavi Swift bindings with XCTest suite [flowos: 155a007a-7c06-4ab1-bd1f-fa35f12b7cc2]

**Files:**
- Create: `Package.swift`
- Create: `lavi-swift/Sources/CLavi/include/lavi_shim.h`
- Create: `lavi-swift/Sources/CLavi/include/module.modulemap`
- Create: `lavi-swift/Sources/CLavi/shim.c`
- Create: `lavi-swift/Sources/Lavi/Lavi.swift`
- Create: `lavi-swift/Tests/LaviTests/LaviTests.swift`

**Interfaces:**
- Consumes: `lavi-c-cpp/include/lavi.h` and `target/debug/liblavi_c_cpp.a`
- Produces: Swift module `Lavi` exporting `public struct Value`, `public final class SimulatedClock`, `public final class Machine`

- [x] **Step 1: Write the failing XCTest suite (`lavi-swift/Tests/LaviTests/LaviTests.swift`) and `Package.swift` scaffold**
- [x] **Step 2: Run `swift test` to verify it fails (RED)**
- [x] **Step 3: Implement `Lavi.swift` wrapping `CLavi` (`Value`, `SimulatedClock`, `Machine`)**
- [x] **Step 4: Run `swift test` to verify all Swift unit tests pass (GREEN)**

### - [x] Task 2: Add Swift stoplight example (`examples/swift`) [flowos: 8c4e6e62-65a0-4142-b57d-2c08a1dd4c1f]

**Files:**
- Create: `examples/swift/Package.swift`
- Create: `examples/swift/Sources/main.swift`

**Interfaces:**
- Consumes: Root `Lavi` Swift package (`path: "../.."`)
- Produces: Runnable `examples/swift` executable verifying stoplight transitions

- [x] **Step 1: Create `examples/swift/Package.swift` and `examples/swift/Sources/main.swift`**
- [x] **Step 2: Run `swift run --package-path examples/swift` and verify output `"Swift example passed (transitions: 4)"`**

---

## Phase 2: Dart / Flutter FFI Bindings & Example [flowos: 298ec60f-7e3b-448b-835a-92acd2790896]

### - [x] Task 3: Add `stage-dart-native.mjs` and implement `lavi-dart` FFI bindings & tests [flowos: 836fd0b8-2340-4809-a8ac-76bb45c7abea]

**Files:**
- Create: `scripts/stage-dart-native.mjs`
- Create: `lavi-dart/pubspec.yaml`
- Create: `lavi-dart/lib/lavi.dart`
- Create: `lavi-dart/lib/src/bindings.dart`
- Create: `lavi-dart/lib/src/loader.dart`
- Create: `lavi-dart/test/lavi_test.dart`

**Interfaces:**
- Consumes: `lavi-c-cpp` (`cargo rustc --package lavi-c-cpp --lib -- --crate-type cdylib`)
- Produces: `package:lavi/lavi.dart` exporting `Value`, `SimulatedClock`, `Machine`

- [x] **Step 1: Create `scripts/stage-dart-native.mjs`, `lavi-dart/pubspec.yaml`, and failing `lavi-dart/test/lavi_test.dart`**
- [x] **Step 2: Run `dart test` in `lavi-dart` to verify it fails (RED)**
- [x] **Step 3: Implement `lavi-dart/lib/src/loader.dart`, `lavi-dart/lib/src/bindings.dart`, and `lavi-dart/lib/lavi.dart`**
- [x] **Step 4: Run `dart test` in `lavi-dart` to verify all Dart FFI tests pass (GREEN)**

### - [x] Task 4: Add Dart stoplight example (`examples/dart`) [flowos: 3c278700-27bb-44e3-9352-f6a1ba590ddb]

**Files:**
- Create: `examples/dart/pubspec.yaml`
- Create: `examples/dart/bin/main.dart`

**Interfaces:**
- Consumes: `lavi-dart` (`path: ../../lavi-dart`)
- Produces: Runnable `examples/dart/bin/main.dart` verifying stoplight transitions

- [x] **Step 1: Create `examples/dart/pubspec.yaml` and `examples/dart/bin/main.dart`**
- [x] **Step 2: Run `dart run bin/main.dart` in `examples/dart` and verify output `"Dart example passed (transitions: 4)"`**

---

## Phase 3: CI/CD, Release Packaging & Version Synchronization [flowos: 28860ea7-aa82-4685-a671-8853ec67ada9]

### - [x] Task 5: Update `sync-version.mjs`, `build-release-natives.mjs`, `ci.yml`, `release.yml`, and `package.json` scripts [flowos: 797752bf-5f6e-4699-bb99-218d6589a174]

**Files:**
- Modify: `scripts/sync-version.mjs`
- Modify: `scripts/build-release-natives.mjs`
- Modify: `tests/build-release-natives.test.ts`
- Modify: `package.json`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Modify: `.gitignore`

**Interfaces:**
- Consumes: `lavi-swift`, `lavi-dart`, `examples/swift`, `examples/dart`
- Produces: Unified `npm run build`, `npm test`, `sync-version.mjs`, and `build-release-natives.mjs` support for Swift and Dart/Flutter

- [x] **Step 1: Add failing tests in `tests/build-release-natives.test.ts` for Apple XCFramework targets/packaging, Dart FFI desktop/Android targets, and `sync-version.mjs` Dart pubspec support**
- [x] **Step 2: Run `npx vitest run tests/build-release-natives.test.ts` and watch it fail (RED)**
- [x] **Step 3: Implement updates in `scripts/sync-version.mjs`, `scripts/build-release-natives.mjs`, `package.json`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, and `.gitignore`**
- [x] **Step 4: Run `npx vitest run tests/build-release-natives.test.ts` and `npm test` to verify all suites pass (GREEN)**
