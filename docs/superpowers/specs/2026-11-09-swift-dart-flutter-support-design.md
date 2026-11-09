# Swift and Dart / Flutter Support & CI/CD Design

## 1. Intent & Why

Extend `lavi`'s portable `#![no_std]` Rust statechart engine to **Swift** (iOS, macOS, and Linux) and **Dart / Flutter** (iOS, Android, macOS, Linux, and Windows via `dart:ffi`), complete with automated testing in `.github/workflows/ci.yml`, release packaging in `.github/workflows/release.yml` and `scripts/build-release-natives.mjs`, version synchronization in `scripts/sync-version.mjs`, and runnable stoplight examples in `examples/swift/` and `examples/dart/`.

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **Zero-Overhead C-ABI Reuse**: Build both `lavi-swift` and `lavi-dart` directly atop the existing C-ABI (`lavi-c-cpp/include/lavi.h`), avoiding redundant Rust FFI glue crates while keeping `#![no_std]` core statechart semantics 100% unified.
- **Full XState 5 Feature Parity Across Swift & Dart**:
  - `Machine`: `init(jsonConfig:inputJson:clock:)`, `start()`, `stop()`, `close()` / `dispose()`, `send(_:)`, `sendJson(_:)`, `matches(_:)`, `can(_:)`, `hasTag(_:)`, `isDone`, `state`, `tick()`, `nextDeadline`, `getSnapshot()`, `restoreSnapshot(_:)`, `getOutput()`, `getMeta()`, `getStateMeta(_:)`.
  - Callback registration: `registerAction`, `registerActionWithParams`, `registerGuard`, `registerGuardWithParams`, `registerDelay`, `registerClock`, and `registerSimulatedClock`.
  - `SimulatedClock`: `init(initialMs:)`, `now()`, `set(_:)`, `increment(_:)`, `close()` / `dispose()`.
  - `Value` (`LaviValue` / `Value`): `isNull`, `isObject`, `isString`, `isNumber`, `isBool`, `stringValue` / `asString()`, `numberValue` / `asNumber()`, `boolValue` / `asBool()`, `getProperty(_:)` / `operator []`.
- **Swift Package Manager (`Package.swift` + `lavi-swift/`)**:
  - Root `Package.swift` exposing the `Lavi` library product (`lavi-swift/Sources/CLavi` C system/header module + `lavi-swift/Sources/Lavi` Swift wrapper + `lavi-swift/Tests/LaviTests` XCTest suite).
  - Works out of the box with `swift test` on both macOS and Linux (`ubuntu-24.04`) against `liblavi_c_cpp.a`.
  - Release workflow builds Apple static libraries (`aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios`) and packages `LaviFFI.xcframework.zip` on `macos-15`.
- **Dart / Flutter FFI Package (`lavi-dart/`)**:
  - Standard Dart / Flutter FFI package (`lavi-dart/pubspec.yaml`, `lavi-dart/lib/lavi.dart`, `lavi-dart/test/lavi_test.dart`) using `dart:ffi` and `package:ffi`.
  - Dynamic library loader resolves `liblavi_c_cpp.{dylib,so,dll}` from `LAVI_LIB_PATH`, `lavi-dart/native/`, `target/debug`, `target/release`, or mobile framework/process symbols (`DynamicLibrary.process()` on iOS/macOS static linkage, `DynamicLibrary.open('liblavi_c_cpp.so')` on Android/Linux).
  - `scripts/stage-dart-native.mjs` stages the compiled `cdylib` (`cargo rustc --package lavi-c-cpp --lib -- --crate-type cdylib`) into `lavi-dart/native/` without leaving a shared library in `target/debug` that would interfere with static C/C++ test linking.
- **CI/CD & Release Automation**:
  - `.github/workflows/ci.yml` installs Swift and Dart on `ubuntu-24.04` and runs `npm test` (including `test:swift`, `test:dart`, and `test:examples`).
  - `.github/workflows/release.yml` adds a `build-apple-xcframework` job on `macos-15` to produce `dist/apple/LaviFFI.xcframework.zip` and uploads it to GitHub Releases.

### Non-Goals
- **Flutter Web (`dart:js_interop`)**: Deferred per decision during design; `lavi-dart` focuses on native mobile/desktop (`dart:ffi`) first.

### Anti-Goals
- **Breaking Embedded / Static C-C++ Builds**: Do not add `"cdylib"` permanently to `lavi-c-cpp/Cargo.toml`'s `crate-type` list, as bare-metal `thumbv*-none-eabi*` targets reject `cdylib` and static C/C++ test invocations (`-L target/debug -llavi_c_cpp`) expect static linkage in `target/debug`. Instead, invoke `cargo rustc --package lavi-c-cpp --lib -- --crate-type cdylib` for Dart shared library generation and move the resulting shared library into `lavi-dart/native/`.

## 3. Architecture & Component Details

### 3.1 Swift Bindings (`Package.swift`, `lavi-swift/`, `examples/swift/`)
- `lavi-swift/Sources/CLavi/include/lavi_shim.h` includes `../../../../lavi-c-cpp/include/lavi.h` with `module.modulemap` exporting module `CLavi`.
- `lavi-swift/Sources/Lavi/Lavi.swift` implements:
  - `public struct Value`: wraps `OpaquePointer?` (`const fsm_value_t*`), providing `isNull`, `isObject`, `isString`, `isNumber`, `isBool`, `stringValue: String?`, `numberValue: Double?`, `boolValue: Bool?`, `property(_ key: String) -> Value`, and `subscript(_ key: String) -> Value`.
  - `public final class SimulatedClock`: wraps `OpaquePointer?` (`SimulatedClock*`) with `deinit { fsm_simulated_clock_free(ptr) }`, `now() -> UInt64`, `set(_ targetMs: UInt64)`, `increment(_ dtMs: UInt64)`.
  - `public final class Machine`: wraps `OpaquePointer?` (`MachineContext*`) and bridges Swift closures via an internal `CallbackRegistry` passed as `Unmanaged.passUnretained(self).toOpaque()` to C `@convention(c)` thunks. Supports both pre-`start()` and post-`start()` registration, `restoreSnapshot(_:)`, `getSnapshot()`, `getOutput()`, `getMeta()`, `getStateMeta(_:)`, `matches(_:)`, `can(_:)`, `hasTag(_:)`, `isDone`, `state`, `send(_:)`, `sendJson(_:)`, `tick()`, `nextDeadline`, and `stop()`.
- `Package.swift` links `-L target/debug` and `-L target/release` with `-llavi_c_cpp` so `swift test` and `swift run` work seamlessly after `cargo build --package lavi-c-cpp`.

### 3.2 Dart / Flutter Bindings (`lavi-dart/`, `scripts/stage-dart-native.mjs`, `examples/dart/`)
- `scripts/stage-dart-native.mjs` moves the compiled `liblavi_c_cpp.{dylib,so,dll}` from `target/<profile>/` into `lavi-dart/native/` so `dart test` and `dart run` load it deterministically while keeping `target/<profile>/` clean for static C/C++ linking.
- `lavi-dart/lib/src/bindings.dart` binds every function and callback typedef in `lavi.h` using `dart:ffi`.
- `lavi-dart/lib/lavi.dart` exports `Machine`, `SimulatedClock`, and `Value`:
  - Uses `NativeCallable.isolateLocal` to create synchronous C function pointers for actions, parameterized actions, guards, parameterized guards, delays, and custom clocks, passing the callback name lookup cleanly and closing `NativeCallable` instances in `Machine.close()` / `dispose()`.

### 3.3 Release & Version Tooling (`scripts/sync-version.mjs`, `scripts/build-release-natives.mjs`)
- `scripts/sync-version.mjs` adds `pubspec` and `pubspec-example` manifest handlers for `lavi-dart/pubspec.yaml` and `examples/dart/pubspec.yaml`.
- `scripts/build-release-natives.mjs` exports `APPLE_XCFRAMEWORK_TARGETS` (`aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios`) and `packageAppleXcframeworkZip` so Apple XCFramework packaging is deterministically planned and tested alongside Android AAR and Embedded ZIP archives.
