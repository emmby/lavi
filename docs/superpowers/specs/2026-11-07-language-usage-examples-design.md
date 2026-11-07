# Multi-Language Self-Contained Usage Examples Design

**Date**: 2026-11-07  
**Status**: Draft (Pending User Approval)

---

## 1. Intent & Why

**Lavi** supports 7 target languages (**Rust**, **JavaScript / TypeScript (WASM)**, **C**, **C++11**, **Python**, **Java**, and **Kotlin**) plus **PlatformIO / ESP32** firmware. However, the repository currently lacks a complete, consistent set of standalone, buildable usage examples across these targets:
- `README.md` is missing usage snippets for **Rust (`lavi`)** and **C (`lavi.h`)**, and the existing snippets use slightly different machines.
- `examples/` only contains browser HTML demos (`examples/*.html`) and an ESP32 PlatformIO sketch (`examples/stoplight-esp32/`), with no standalone buildable examples for Rust, Node/JS, C, host C++, Python, Java, or Kotlin.
- `examples/stoplight-esp32/src/main.cpp` uses manual `millis() - last_transition > 1000` polling with `send("TIMER")` instead of Lavi's idiomatic `after` delayed transitions + `registerClock(millis)` + `tick()`.

The goal of this design is to provide:
1. **A unified statechart example** shared across all 7 languages in both `README.md` and `examples/<lang>/`.
2. **Self-contained, idiomatic per-language directories** (`examples/rust`, `examples/js-wasm`, `examples/c`, `examples/cpp`, `examples/python`, `examples/java`, `examples/kotlin`, and `examples/esp32`) where each directory has its own local build configuration (`Cargo.toml`, `package.json`, `Makefile`, `pyproject.toml`, `build.gradle.kts`, `platformio.ini`) so users can see how to start a clean project in their language.
3. **Continuous build & test verification in `npm run build` and `npm test` (CI/CD)** so every language example is compiled and executed automatically on every change and never drifts out of date as Lavi evolves.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **One Canonical Statechart Across All 7 Languages**:
  Every language example (and `README.md` snippet) runs the exact same 3-state stoplight / pedestrian crossing statechart (`red` $\rightarrow$ `green` $\rightarrow$ `yellow` $\rightarrow$ `red`) that exercises:
  1. Declarative JSON config (`initial: "red"`, `context: { "cycles": 0 }`, `states: { "red", "green", "yellow" }`).
  2. Event-driven transition (`"PEDESTRIAN_WAIT"` in `"green"` guarded by `"canCross"` transitioning to `"yellow"` with action `"onYellow"`).
  3. Named guard callback (`"canCross"`) reading `context` (`cycles >= 1`).
  4. Named action callback (`"onRed"`, `"onGreen"`, `"onYellow"`) invoked on state transitions.
  5. Declarative `after` delayed transitions (`"1000"` ms on `"red"`, `"green"`, and `"yellow"`) driven deterministically by `SimulatedClock` (or `millis` + `tick()` on ESP32).
- **Self-Contained Per-Language Project Directories**:
  Each directory under `examples/` contains its own idiomatic project manifest and source file:
  - `examples/rust/` (`Cargo.toml` + `src/main.rs`)
  - `examples/js-wasm/` (`package.json` + `index.mjs`)
  - `examples/c/` (`Makefile` + `main.c`)
  - `examples/cpp/` (`Makefile` + `main.cpp`)
  - `examples/python/` (`pyproject.toml` + `main.py`)
  - `examples/java/` (`settings.gradle.kts` + `build.gradle.kts` + `src/main/java/Example.java`)
  - `examples/kotlin/` (`settings.gradle.kts` + `build.gradle.kts` + `src/main/kotlin/Example.kt`)
  - `examples/esp32/` (renamed from `examples/stoplight-esp32/`, containing `platformio.ini` + `src/main.cpp` using declarative `after` delays, `registerClock(millis)`, and `tick()`).
- **Complete 7-Language `README.md` Coverage**:
  Update the `## Usage Examples` section in `README.md` to showcase all 7 languages (**Rust**, **JavaScript / TypeScript (WASM)**, **C**, **C++11**, **Python**, **Java**, and **Kotlin**) using the shared statechart and linking to the corresponding `examples/<lang>/` directory.
- **Automated CI/CD & Local Build/Test Integration**:
  - `npm run build` includes `npm run build:examples`, compiling the compiled-language examples (`rust`, `c`, `cpp`, `java`, `kotlin`).
  - `npm test` includes `npm run test:examples`, executing all 7 host language examples (`rust`, `js-wasm`, `c`, `cpp`, `python`, `java`, `kotlin`) and verifying they exit `0` with passing assertions, plus syntax/header-checking `examples/esp32/src/main.cpp` against `lavi.hpp` and a minimal mock `Arduino.h` stub.

### Non-Goals
- **Modifying Core `lavi` Statechart Semantics**: Unless a minor ergonomic helper on `RuntimeMachine` is helpful, core statechart execution semantics remain unchanged.
- **Removing Existing Interactive Browser Demos**: The interactive HTML demos (`examples/*.html`, `examples/robot_vacuum.html`, etc.) served on GitHub Pages remain intact in `examples/`.

### Anti-Goals
- **Hidden Build Commands Only in Root `package.json`**: Users inspecting `examples/<lang>/` must see a standard local build configuration file (`Cargo.toml`, `Makefile`, `build.gradle.kts`, `package.json`, `pyproject.toml`) inside that directory rather than a bare source file that only compiles via a long one-liner hidden in the root `package.json`.
- **Network Downloads or External Hardware Required for `npm test`**: All 7 host examples must build and run offline against the local repo build artifacts (`lavi`, `lavi-wasm`, `lavi-c-cpp`, `lavi-py`, `lavi-jvm`).

---

## 3. Shared Statechart Specification

All 7 host language examples and `README.md` snippets use the following statechart definition:

```json
{
  "id": "stoplight",
  "initial": "red",
  "context": { "cycles": 1 },
  "states": {
    "red": {
      "entry": ["onRed"],
      "after": { "1000": "green" }
    },
    "green": {
      "entry": ["onGreen"],
      "on": {
        "PEDESTRIAN_WAIT": {
          "target": "yellow",
          "guard": "canCross"
        }
      },
      "after": { "1000": "yellow" }
    },
    "yellow": {
      "entry": ["onYellow"],
      "after": { "1000": "red" }
    }
  }
}
```

### Shared Execution Scenario (Deterministic Verification)
Every host language example performs the exact same steps and assertions:
1. Create a `SimulatedClock` starting at `0` ms.
2. Create the `stoplight` machine bound to the clock.
3. Register:
   - Action `"onRed"`, `"onGreen"`, `"onYellow"` (recording transitions or printing state entry).
   - Guard `"canCross"` checking that `context.cycles >= 1`.
4. Start the machine and assert `matches("red")` is `true` (and `"onRed"` ran).
5. Advance the clock by `1000` ms (`clock.increment(1000)`) and assert `matches("green")` is `true` (`red` $\rightarrow$ `green` via `after: 1000`).
6. Send `"PEDESTRIAN_WAIT"` (which passes guard `"canCross"`) and assert `matches("yellow")` is `true` (`green` $\rightarrow$ `yellow`).
7. Advance the clock by `1000` ms (`clock.increment(1000)`) and assert `matches("red")` is `true` (`yellow` $\rightarrow$ `red` via `after: 1000`).

---

## 4. Per-Language Directory Layout & Build Mechanics

### 4.1 `examples/rust/`
- **Files**:
  - `examples/rust/Cargo.toml`: Standalone package `lavi-example-rust` with `[workspace]` table (so it is self-contained when copied out, while referencing `lavi = { path = "../../lavi" }` and `indexmap = "2.7"`).
  - `examples/rust/src/main.rs`: Uses `lavi::{RuntimeMachine, SimulatedClock, ActionFunction, GuardFunction}`.
- **Build / Run**:
  - `cargo build --manifest-path examples/rust/Cargo.toml`
  - `cargo run --manifest-path examples/rust/Cargo.toml`

### 4.2 `examples/js-wasm/`
- **Files**:
  - `examples/js-wasm/package.json`: `"type": "module"`, `"dependencies": { "lavi-wasm": "file:../../lavi-wasm/pkg-npm" }`, `"scripts": { "start": "node index.mjs" }`.
  - `examples/js-wasm/index.mjs`: Imports `{ createMachine, createActor, SimulatedClock }` from `'../../lavi-wasm/pkg-npm/lavi.js'` (with comment showing `'lavi-wasm'` for published usage) and runs the scenario.
- **Build / Run**:
  - `npm --prefix examples/js-wasm start` (or `node examples/js-wasm/index.mjs`).

### 4.3 `examples/c/`
- **Files**:
  - `examples/c/Makefile`: Compiles `main.c` using `$(CC)` (`clang`/`gcc`) with `-I../../lavi-c-cpp/include -L../../target/debug -llavi_c_cpp` (plus platform link flags `-lpthread -ldl -lm` on Unix), with `all`, `run`, and `clean` targets.
  - `examples/c/main.c`: Uses `MachineContext*`, `SimulatedClock*`, `fsm_register_action`, `fsm_register_guard` with `void* user_data` (passing a stack struct pointer, zero globals), `fsm_value_get_property`, `fsm_value_get_number`, and `assert(...)`.
- **Build / Run**:
  - `make -C examples/c`
  - `make -C examples/c run`

### 4.4 `examples/cpp/`
- **Files**:
  - `examples/cpp/Makefile`: Compiles `main.cpp` using `$(CXX) -std=c++11` with `-I../../lavi-c-cpp/include -L../../target/debug -llavi_c_cpp`, with `all`, `run`, and `clean` targets.
  - `examples/cpp/main.cpp`: Uses `lavi::SimulatedClock`, `lavi::Machine`, `registerAction`, `registerGuard`, `matches`, `send`, and `clock.increment(1000)`.
- **Build / Run**:
  - `make -C examples/cpp`
  - `make -C examples/cpp run`

### 4.5 `examples/python/`
- **Files**:
  - `examples/python/pyproject.toml`: Standard PEP 621 `pyproject.toml` declaring `dependencies = ["lavi"]` and `[project.scripts] example = "main:main"`.
  - `examples/python/main.py`: Uses `from lavi import Machine, SimulatedClock, Value`, context manager `with Machine(MACHINE_JSON, clock=clock) as machine:`, `@machine.action(...)`, `@machine.guard(...)`, `machine.start()`, `clock.increment(1000)`, `machine.send("PEDESTRIAN_WAIT")`.
- **Build / Run**:
  - `PYTHONPATH=lavi-py/python python3 examples/python/main.py`

### 4.6 `examples/java/`
- **Files**:
  - `examples/java/settings.gradle.kts`: `rootProject.name = "lavi-example-java"` + `includeBuild("../../lavi-jvm")`.
  - `examples/java/build.gradle.kts`: `application` plugin, `dependencies { implementation("io.lavi:lavi-jvm") }`, `application { mainClass.set("Example") }`, and `tasks.named<JavaExec>("run") { systemProperty("lavi.library.path", "${rootDir}/../../target/debug") }`.
  - `examples/java/src/main/java/Example.java`: Uses `try (SimulatedClock clock = new SimulatedClock(0); Machine machine = new Machine(JSON, clock)...)` and verifies each transition.
- **Build / Run**:
  - `./lavi-jvm/gradlew -p examples/java assemble`
  - `./lavi-jvm/gradlew -p examples/java run`

### 4.7 `examples/kotlin/`
- **Files**:
  - `examples/kotlin/settings.gradle.kts`: `rootProject.name = "lavi-example-kotlin"` + `includeBuild("../../lavi-jvm")`.
  - `examples/kotlin/build.gradle.kts`: `kotlin("jvm") version "2.1.10"` + `application` plugin, `dependencies { implementation("io.lavi:lavi-jvm") }`, `application { mainClass.set("ExampleKt") }`, and `tasks.named<JavaExec>("run") { systemProperty("lavi.library.path", "${rootDir}/../../target/debug") }`.
  - `examples/kotlin/src/main/kotlin/Example.kt`: Uses `SimulatedClock(0).use { clock -> Machine(JSON, clock).use { machine -> ... } }` with `onAction`, `onGuard`, `ctx["cycles"]`, etc.
- **Build / Run**:
  - `./lavi-jvm/gradlew -p examples/kotlin assemble`
  - `./lavi-jvm/gradlew -p examples/kotlin run`

### 4.8 `examples/esp32/` (Renamed from `examples/stoplight-esp32/`)
- **Rename & Lockstep Updates**:
  - Move `examples/stoplight-esp32/` $\rightarrow$ `examples/esp32/`.
  - Update `examples/esp32/src/main.cpp` to use declarative `"after": { "1000": "..." }` transitions, `machine->registerClock([]() -> uint64_t { return millis(); });`, and `machine->tick();` in `loop()`.
  - Update all references in `library.json`, `lavi-c-cpp/tests/test_build_rust.py`, `AGENTS.md`, and `examples/README.md` to `examples/esp32`.
  - Add a host-side syntax/compile check of `examples/esp32/src/main.cpp` using a tiny mock `Arduino.h` header during `npm run test:examples` so `examples/esp32/src/main.cpp` is also verified on every CI run without needing a physical ESP32 board.

---

## 5. Builder's Calls

- **Group ID in `lavi-jvm/build.gradle.kts`**: Set `group = "io.lavi"` and `version = "0.1.0"` in `lavi-jvm/build.gradle.kts` so Gradle composite builds (`includeBuild("../../lavi-jvm")`) in `examples/java` and `examples/kotlin` resolve `implementation("io.lavi:lavi-jvm:0.1.0")` cleanly just like a published Maven coordinate.
- **Gitignore Cleanup**: Add build output directories for the examples (`examples/rust/target/`, `examples/c/build/`, `examples/cpp/build/`, `examples/java/.gradle/`, `examples/java/build/`, `examples/kotlin/.gradle/`, `examples/kotlin/build/`) to `.gitignore` so building and running examples leaves a clean git worktree.
