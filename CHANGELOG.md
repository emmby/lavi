# Changelog

All notable changes to Lavi are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-11-11

### Added
- **C# / .NET 8.0 Bindings (`lavi-csharp`)**: Added P/Invoke bindings (`Machine`, `MachineSnapshot`, `SimulatedClock`, `Value`), xUnit test suite, and a self-contained C# stoplight example (`examples/csharp`).
- **Zero Unimplemented Skips Across Onboarded XState v5 Conformance Suites**: Eliminated all 40 remaining `unimplemented-feature` skips across `actions.test.ts`, `state.test.ts`, `json.test.ts`, `transition.test.ts`, `final.test.ts`, `input.test.ts`, and `inspect.test.ts`.
  - Pure stateless transitions (`initialTransition`, `transition`) returning `[nextSnapshot, executableActions]`.
  - Child actor lifecycle and inter-actor messaging (`invoke`, `spawnChild`, `stopChild`, `sendTo`, `sendParent`, `forwardTo`, `system.get`, `snapshot.children`) and logic creators (`fromPromise`, `fromObservable`, `fromEventObservable`, `fromCallback`, `fromTransition`).
  - Dynamic action and guard `params` resolution, `setup()` strongly-typed action creators, `enqueueActions`, and `emit` / `actor.on(eventType, handler)`.
- **Full Cross-Language API Parity (`TD-21`)**: Added pure transition evaluation (`initialTransition` / `transitionPure`), final leaf substate output (`getFinalLeafOutput`), dynamic output resolvers (`registerOutputResolver`), and invoke lifecycle hooks (`registerInvokeCallbacks`) across Rust (`lavi`), C/C++ (`lavi-c-cpp`), Python (`lavi-py`), Java/Kotlin (`lavi-jvm`), Swift (`lavi-swift`), Dart/Flutter (`lavi-dart`), and C#/.NET (`lavi-csharp`).

### Changed
- **Modularized `lavi-wasm` (`TD-22`)**: Deduplicated actor logic state into `LogicActorState` and split `lavi-wasm/src/actions.rs` and `lavi-wasm/src/actor.rs` into focused `guards`, `invoke`, `system`, and `inspect` submodules.
- **Runtime & Memory Hardening (`TD-7`, `TD-9`, `TD-10`, `TD-17`, `TD-18`, `TD-19`)**: Consolidated `MachineDefinition` sharing via `Rc`, converted recursive config/invoke helpers to iterative heap `Vec` work-stacks, evicted stopped actors from `LIVE_ACTOR_REFS`, and dropped temporary synchronous WASM `Closure` instances deterministically.

## [0.3.0] - 2026-11-09

### Added
- **Inspection API (`inspect.test.ts`)**: Onboarded the XState v5 inspection conformance suite (`@xstate/inspect` observer events: `@xstate.actor`, `@xstate.event`, `@xstate.snapshot`, `@xstate.microstep`) across Rust (`lavi`), WASM/JS (`lavi-wasm`), C/C++ (`lavi-c-cpp`), Python (`lavi-py`), Java/Kotlin (`lavi-jvm`), Swift (`lavi-swift`), and Dart/Flutter (`lavi-dart`).
- **React & React Native Support (`lavi-fsm/react` & `lavi-fsm/react-native`)**:
  - Added `@xstate/react`-compatible hooks (`useMachine`, `useActor`, `useActorRef`, `useSelector`) under `lavi-fsm/react`.
  - Added a zero-config `wasm2js` pure-JavaScript fallback target (`lavi-fsm/react-native`) for React Native and Expo Go on Hermes and JavaScriptCore without native linking or `WebAssembly` globals.

### Documentation
- Added a *Reporting Issues* section to `README.md`.

## [0.2.0] - 2026-11-09

### Added
- **Swift / Apple Platform Bindings (`lavi-swift` & `Package.swift`)**: Added idiomatic ARC-managed Swift bindings (`Machine`, `SimulatedClock`, `Value`), XCTest suite, Swift stoplight example (`examples/swift`), and automated `LaviFFI.xcframework.zip` packaging for macOS (`arm64`, `x86_64`) and iOS (`arm64` device + `arm64`/`x86_64` simulator).
- **Dart / Flutter Bindings (`lavi-dart`)**: Added `dart:ffi` bindings (`Machine`, `SimulatedClock`, `Value`), `Finalizer`-backed lifecycle management with deterministic `.dispose()`, `dart test` suite, and Dart stoplight example (`examples/dart`).
- **32-bit ARM Cortex-M Embedded Targets**: Expanded `lavi-embedded.zip` precompiled static libraries to include 5 ARM Cortex-M architectures (`cortex-m0plus`, `cortex-m3`, `cortex-m4`, `cortex-m7`, `cortex-m33`, including hard-float `eabihf` and softfp variants) alongside all 6 ESP32 architectures (`esp32`, `esp32s2`, `esp32s3`, `esp32c3`, `esp32c6`, `esp32h2`).
- **Automated Embedded & npm Registry Publishing**:
  - Automated publishing to the **PlatformIO Registry** (`emmby/lavi`) and **Arduino Library Manager** (`emmby/lavi-arduino`) in `.github/workflows/release.yml`.
  - Switched npm (`lavi-fsm`) publishing to zero-secret GitHub OIDC Trusted Publishing with provenance.

### Changed
- Gated both the GitHub Pages deployment workflow and the Release workflow on successful completion of the reusable `CI` workflow.

## [0.1.0] - 2026-11-08

### Added
- **Core `#![no_std]` Rust Statechart Engine (`lavi`)**:
  - Hierarchical (compound), parallel, final, and history (shallow & deep) states with full LCCA transition domain resolution and W3C SCXML exit/transition/entry action ordering.
  - Iterative $O(1)$ CPU call-stack JSON parser and state tree traversal designed for 8 KB microcontroller stacks.
  - Eventless (`always`) transitions, delayed (`after`) transitions, `SimulatedClock` chronological stepping (`advance` / `set` / `get_next_deadline`), `assign` context updates, `raise` internal event queues, `.matches()`, `.can()`, `.hasTag()`, `output`, `meta`, and snapshot persistence (`get_persisted_snapshot` / `restore_snapshot`).
- **Multi-Language Bindings & Packaging**:
  - **WebAssembly / JS / TS (`lavi-wasm` / `lavi-fsm`)**: Drop-in XState v5 API (`createMachine`, `createActor`, `setup`, `assign`, `raise`, `SimulatedClock`) for Node.js, bundlers, and browsers.
  - **C & C++11 (`lavi-c-cpp`)**: Zero-copy C ABI (`lavi.h`), header-only RAII C++11 wrapper (`lavi.hpp`), PlatformIO (`library.json`, `build_rust.py`) and Arduino IDE (`library.properties`, `lavi-embedded.zip`) integration.
  - **Python (`lavi-py`)**: PyO3 `abi3-py38` native extension with decorator ergonomics (`@machine.action`, `@machine.guard`, `@machine.delay`), context managers, and `.pyi` type stubs.
  - **Java & Kotlin (`lavi-jvm` / `io.github.emmby:lavi`)**: JNI bindings with `AutoCloseable` lifecycle, Kotlin DSL extensions, desktop fat JARs, and 16 KB page-aligned Android AARs (`lavi-release.aar`).
- **Automated Conformance & Release Pipeline**:
  - Upstream `statelyai/xstate` (`v5.25.0`) conformance test suite verified by `scripts/check-conformance-tests.mjs`.
  - Single-tag release pipeline (`.github/workflows/release.yml`), cross-ecosystem version synchronization (`scripts/sync-version.mjs`), and interactive WASM browser demos.

[0.4.0]: https://github.com/emmby/lavi/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/emmby/lavi/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/emmby/lavi/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/emmby/lavi/releases/tag/v0.1.0
