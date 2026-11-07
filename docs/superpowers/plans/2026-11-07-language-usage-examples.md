# Multi-Language Self-Contained Usage Examples Implementation Plan [flowos: 0f1158e7-91b7-4af1-9e8e-5d4101e4e076]

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add self-contained, buildable, idiomatic usage examples across all 7 supported languages (`rust`, `js-wasm`, `c`, `cpp`, `python`, `java`, `kotlin`) plus `esp32` (renamed from `stoplight-esp32`), update `README.md` and `examples/README.md`, and wire all examples into `npm run build` and `npm test`.

**Architecture:** Each language example lives in its own subdirectory under `examples/<lang>/` with a real local build configuration (`Cargo.toml`, `package.json`, `Makefile`, `pyproject.toml`, `build.gradle.kts`, `platformio.ini`) and implements the exact same `stoplight` statechart (`red` $\rightarrow$ `green` $\rightarrow$ `yellow` $\rightarrow$ `red` with `entry` actions, `after` delayed transitions via `SimulatedClock`, and a guarded `PEDESTRIAN_WAIT` event). Root `package.json` invokes each example's local build/run tool during `npm run build` and `npm test`.

**Tech Stack:** Rust (`cargo`), Node.js / WebAssembly (`lavi-wasm`), C11 & C++11 (`clang`/`clang++`, `make`), Python 3 (`lavi-py`), Java & Kotlin (`Gradle` composite builds, `lavi-jvm`), PlatformIO (`esp32`).

**Spec:** [`docs/superpowers/specs/2026-11-07-language-usage-examples-design.md`](../specs/2026-11-07-language-usage-examples-design.md)

## Global Constraints

- All 7 host language examples and `README.md` snippets MUST use the shared `stoplight` statechart JSON (`initial: "red"`, `context: { "cycles": 1 }`, states `red`/`green`/`yellow`, `entry` actions `onRed`/`onGreen`/`onYellow`, `after: { "1000": ... }`, and `on.PEDESTRIAN_WAIT` on `green` with `target: "yellow"`, `guard: "canCross"`).
- Every host example MUST execute the exact 4-step deterministic scenario (`start` $\rightarrow$ `red`; `clock.increment(1000)` $\rightarrow$ `green`; `send("PEDESTRIAN_WAIT")` $\rightarrow$ `yellow`; `clock.increment(1000)` $\rightarrow$ `red`) and assert state and action counts before exiting `0`.
- `examples/c/main.c` MUST pass `void* user_data` (pointer to a stack struct) to `fsm_register_action` and `fsm_register_guard` with zero mutable global state.
- `examples/stoplight-esp32` MUST be renamed to `examples/esp32`, and `library.json`, `lavi-c-cpp/tests/test_build_rust.py`, `AGENTS.md`, and `examples/README.md` MUST be updated in lockstep.
- All example build outputs (`examples/rust/target/`, `examples/c/build/`, `examples/cpp/build/`, `examples/java/.gradle/`, `examples/java/build/`, `examples/kotlin/.gradle/`, `examples/kotlin/build/`) MUST be ignored in `.gitignore`.

## Review Focus

1. **Standalone `examples/rust` Cargo workspace boundary**: Running `cargo run --manifest-path examples/rust/Cargo.toml` inside a repo with a root `[workspace]` fails unless `examples/rust/Cargo.toml` declares an empty `[workspace]` table. Task 1 pins this in `examples/rust/Cargo.toml` and tests it via `cargo run --manifest-path examples/rust/Cargo.toml`.
2. **macOS vs. Linux linker flags in `examples/c/Makefile` and `examples/cpp/Makefile`**: Static linking of `liblavi_c_cpp.a` requires `-lpthread -ldl -lm` on Linux and `-lpthread` on macOS. Task 2 uses `$(CC)` / `$(CXX)` with `-lpthread -ldl -lm` (accepted on both macOS clang and Linux gcc/clang) and verifies both `make -C examples/c run` and `make -C examples/cpp run`.
3. **Host-side verification of `examples/esp32/src/main.cpp`**: Because CI does not have physical ESP32 hardware or the full PlatformIO Xtensa toolchain installed, `examples/esp32/src/main.cpp` could silently bit-rot on `lavi.hpp` changes unless syntax/type-checked on the host. Task 2 adds `examples/esp32/test_host_compile.cpp` (with a minimal mock `Arduino.h`) and compiles it with `clang++ -std=c++11 -fsyntax-only`.
4. **Gradle composite build resolution in `examples/java` and `examples/kotlin`**: `includeBuild("../../lavi-jvm")` only substitutes `io.lavi:lavi-jvm` if `lavi-jvm/build.gradle.kts` declares `group = "io.lavi"`. Task 3 sets `group = "io.lavi"` and `version = "0.1.0"` in `lavi-jvm/build.gradle.kts`.
5. **Clean git working tree after `npm run build && npm test`**: Running all example builds creates local build artifacts (`target/`, `build/`, `.gradle/`). Task 4 adds these paths to `.gitignore` and verifies `git status --porcelain` is clean after `npm test`.

---

## Phase 1: Core, Scripting & Embedded Examples [flowos: 6e9f3eaa-20c2-44c7-9c2f-d6b78665cc13]

### Task 1: Rust, JS/WASM, and Python Self-Contained Examples [flowos: 49a18560-2d1d-4b69-bc8d-f9e7c21473d0]

**Files:**
- Create: `examples/rust/Cargo.toml`
- Create: `examples/rust/src/main.rs`
- Create: `examples/js-wasm/package.json`
- Create: `examples/js-wasm/index.mjs`
- Create: `examples/python/pyproject.toml`
- Create: `examples/python/main.py`

**Interfaces:**
- Consumes: `lavi::{RuntimeMachine, SimulatedClock, ActionFunction, GuardFunction, Value}`, `lavi-wasm` (`createMachine`, `createActor`, `SimulatedClock`), `lavi-py` (`Machine`, `SimulatedClock`, `Value`)
- Produces: Self-contained `examples/rust`, `examples/js-wasm`, and `examples/python` projects runnable from the root build/test scripts.

- [x] **Step 1: Verify these example commands currently fail**

Run: `cargo run --manifest-path examples/rust/Cargo.toml`
Expected: FAIL (`manifest path examples/rust/Cargo.toml does not exist`)

- [x] **Step 2: Create `examples/rust/Cargo.toml` and `examples/rust/src/main.rs`**

In `examples/rust/Cargo.toml`, define package `lavi-example-rust` (`edition = "2021"`), an empty `[workspace]` table, and dependencies `lavi = { path = "../../lavi" }` and `indexmap = "2.7"`.
In `examples/rust/src/main.rs`, define `MACHINE_JSON` matching the spec's `stoplight` JSON, bind `SimulatedClock::with_initial(0)` via `machine.register_clock(&clock)`, register `"canCross"` (`ctx.as_object().and_then(|o| o.get("cycles")).and_then(|v| v.as_f64()).unwrap_or(0.0) >= 1.0`) and `"onRed"` / `"onGreen"` / `"onYellow"` in `guards` and `actions` `IndexMap`s, execute `start` $\rightarrow$ `clock.increment(1000, ...)` $\rightarrow$ `transition("PEDESTRIAN_WAIT", &Value::event("PEDESTRIAN_WAIT"), ...)` $\rightarrow$ `clock.increment(1000, ...)`, and assert each state and transition count.

- [x] **Step 3: Create `examples/js-wasm/package.json` and `examples/js-wasm/index.mjs`**

In `examples/js-wasm/package.json`, set `"name": "lavi-example-js-wasm"`, `"private": true`, `"type": "module"`, `"scripts": { "start": "node index.mjs" }`, and `"dependencies": { "lavi-wasm": "file:../../lavi-wasm/pkg-npm" }`.
In `examples/js-wasm/index.mjs`, import `{ createMachine, createActor, SimulatedClock }` from `'../../lavi-wasm/pkg-npm/lavi.js'`, configure the `stoplight` machine with `onRed` / `onGreen` / `onYellow` actions and `canCross: ({ context }) => context.cycles >= 1` guard, start the actor with `new SimulatedClock()`, run the 4-step scenario (`start` $\rightarrow$ `clock.increment(1000)` $\rightarrow$ `actor.send({ type: 'PEDESTRIAN_WAIT' })` $\rightarrow$ `clock.increment(1000)`), and assert with `node:assert/strict`.

- [x] **Step 4: Create `examples/python/pyproject.toml` and `examples/python/main.py`**

In `examples/python/pyproject.toml`, define `[project]` `name = "lavi-example-python"`, `version = "0.1.0"`, `requires-python = ">=3.8"`, `dependencies = ["lavi"]`.
In `examples/python/main.py`, import `from lavi import Machine, SimulatedClock, Value`, use `with Machine(MACHINE_JSON, clock=clock) as machine:`, register `@machine.action("onRed")`, `@machine.action("onGreen")`, `@machine.action("onYellow")`, and `@machine.guard("canCross")` (`(ctx["cycles"].as_int() or 0) >= 1`), and run the 4-step scenario with `assert` checks.

- [x] **Step 5: Run Rust, JS/WASM, and Python examples to verify they pass**

Run: `npm run build:npm && npm run build:py && cargo run --manifest-path examples/rust/Cargo.toml && npm --prefix examples/js-wasm start && PYTHONPATH=lavi-py/python python3 examples/python/main.py`
Expected: PASS (all three exit `0` and print completion output)

- [x] **Step 6: Commit**

```bash
git add examples/rust examples/js-wasm examples/python
git commit -m "feat(examples): add self-contained Rust, JS/WASM, and Python examples"
```

---

### Task 2: C, C++11, and ESP32 Self-Contained Examples & Rename [flowos: 33a26651-17c9-4f1f-8acb-1b07884e848f]

**Files:**
- Create: `examples/c/Makefile`
- Create: `examples/c/main.c`
- Create: `examples/cpp/Makefile`
- Create: `examples/cpp/main.cpp`
- Move/Modify: `examples/stoplight-esp32/platformio.ini` $\rightarrow$ `examples/esp32/platformio.ini`
- Move/Modify: `examples/stoplight-esp32/src/main.cpp` $\rightarrow$ `examples/esp32/src/main.cpp`
- Create: `examples/esp32/test_host_compile.cpp`
- Modify: `library.json:36`
- Modify: `lavi-c-cpp/tests/test_build_rust.py:256-265`
- Modify: `AGENTS.md:83-88`

**Interfaces:**
- Consumes: `lavi-c-cpp/include/lavi.h`, `lavi-c-cpp/include/lavi.hpp`, `target/debug/liblavi_c_cpp.a`
- Produces: `examples/c` (`make -C examples/c run`), `examples/cpp` (`make -C examples/cpp run`), and `examples/esp32` (with host syntax-check `examples/esp32/test_host_compile.cpp`).

- [x] **Step 1: Update `lavi-c-cpp/tests/test_build_rust.py` for `examples/esp32` and run to verify failure**

In `lavi-c-cpp/tests/test_build_rust.py`, update `test_stoplight_esp32_example_and_workflow_cleanup` to check `examples/esp32/platformio.ini` and `examples/esp32/src/main.cpp` (and assert `examples/stoplight-esp32` no longer exists).
Run: `python3 -m unittest lavi-c-cpp/tests/test_build_rust.py -v`
Expected: FAIL (`AssertionError: False is not true` for `examples/esp32/platformio.ini`)

- [x] **Step 2: Rename `examples/stoplight-esp32` to `examples/esp32` and update `src/main.cpp`, `library.json`, and `AGENTS.md`**

Run `git mv examples/stoplight-esp32 examples/esp32`.
Update `examples/esp32/src/main.cpp` to use the shared `stoplight` JSON with `"after": { "1000": ... }`, `machine->registerClock([]() -> uint64_t { return millis(); });`, and `machine->tick();` inside `loop()`.
Create `examples/esp32/test_host_compile.cpp` which defines minimal `Arduino.h` stubs (`HIGH`, `LOW`, `OUTPUT`, `pinMode`, `digitalWrite`, `millis`, `delay`, `Serial`) and `#include "src/main.cpp"` so `clang++ -std=c++11 -fsyntax-only -I lavi-c-cpp/include examples/esp32/test_host_compile.cpp` verifies `examples/esp32/src/main.cpp` on the host.
Update `library.json` line 36 and `AGENTS.md` lines 83–88 from `examples/stoplight-esp32` to `examples/esp32`.

- [x] **Step 3: Create `examples/c/Makefile` and `examples/c/main.c`**

In `examples/c/Makefile`, define targets `all` (`build/example_c`), `run` (`./build/example_c`), and `clean`, compiling `main.c` with `$(CC) -std=c11 -Wall -Wextra -I../../lavi-c-cpp/include main.c -L../../target/debug -llavi_c_cpp -lpthread -ldl -lm -o build/example_c`.
In `examples/c/main.c`, define a stack struct `typedef struct { int red_entries; int green_entries; int yellow_entries; } StoplightLog;`, pass `&log` as `void* user_data` to `fsm_register_action` (`on_light_entry`) and `NULL` to `fsm_register_guard` (`can_cross`, reading `fsm_value_get_property(ctx, "cycles")` via `fsm_value_get_number`), bind `SimulatedClock*` with `fsm_register_simulated_clock`, run the 4-step scenario, assert all states and entry counts, and free `machine` and `clock`.

- [x] **Step 4: Create `examples/cpp/Makefile` and `examples/cpp/main.cpp`**

In `examples/cpp/Makefile`, define targets `all` (`build/example_cpp`), `run` (`./build/example_cpp`), and `clean`, compiling `main.cpp` with `$(CXX) -std=c++11 -Wall -Wextra -I../../lavi-c-cpp/include main.cpp -L../../target/debug -llavi_c_cpp -lpthread -ldl -lm -o build/example_cpp`.
In `examples/cpp/main.cpp`, use `lavi::SimulatedClock clock(0);`, `lavi::Machine machine(MACHINE_JSON, clock);`, `machine.registerAction(...)`, `machine.registerGuard("canCross", ...)`, and run the 4-step scenario with `assert(...)`.

- [x] **Step 5: Verify C, C++, ESP32 host compile check, and `test_build_rust.py` pass**

Run: `npm run test:c-cpp && make -C examples/c run && make -C examples/cpp run && clang++ -std=c++11 -fsyntax-only -I lavi-c-cpp/include examples/esp32/test_host_compile.cpp`
Expected: PASS

- [x] **Step 6: Commit**

```bash
git add examples/c examples/cpp examples/esp32 examples/stoplight-esp32 library.json lavi-c-cpp/tests/test_build_rust.py AGENTS.md
git commit -m "feat(examples): add C/C++ examples and rename stoplight-esp32 to esp32"
```

---

## Phase 2: JVM Examples, CI Wiring & Documentation [flowos: e200b89b-37e2-4146-83ec-983f827cba82]

### Task 3: Java and Kotlin Self-Contained Gradle Examples [flowos: 87b0ed28-c463-4b85-9d6c-39b56343aa98]

**Files:**
- Modify: `lavi-jvm/build.gradle.kts:1-10`
- Create: `examples/java/settings.gradle.kts`
- Create: `examples/java/build.gradle.kts`
- Create: `examples/java/src/main/java/Example.java`
- Create: `examples/kotlin/settings.gradle.kts`
- Create: `examples/kotlin/build.gradle.kts`
- Create: `examples/kotlin/src/main/kotlin/Example.kt`

**Interfaces:**
- Consumes: `io.lavi.Machine`, `io.lavi.SimulatedClock`, `io.lavi.Value`, `io.lavi.*` Kotlin extensions from `lavi-jvm`
- Produces: Self-contained Gradle projects in `examples/java` and `examples/kotlin` runnable via `./lavi-jvm/gradlew -p examples/java run` and `./lavi-jvm/gradlew -p examples/kotlin run`.

- [x] **Step 1: Verify running the Java example project currently fails**

Run: `./lavi-jvm/gradlew -p examples/java run`
Expected: FAIL (`Directory '.../examples/java' does not contain a Gradle build`)

- [x] **Step 2: Set `group` and `version` in `lavi-jvm/build.gradle.kts`**

Add `group = "io.lavi"` and `version = "0.1.0"` to `lavi-jvm/build.gradle.kts` so composite builds (`includeBuild("../../lavi-jvm")`) substitute `io.lavi:lavi-jvm:0.1.0`.

- [x] **Step 3: Create `examples/java/settings.gradle.kts`, `examples/java/build.gradle.kts`, and `examples/java/src/main/java/Example.java`**

In `examples/java/settings.gradle.kts`, set `rootProject.name = "lavi-example-java"` and `includeBuild("../../lavi-jvm")`.
In `examples/java/build.gradle.kts`, apply `application`, set Java 1.8 compatibility, depend on `implementation("io.lavi:lavi-jvm:0.1.0")`, set `application { mainClass.set("Example") }`, and configure `tasks.named<JavaExec>("run") { systemProperty("lavi.library.path", "${projectDir}/../../target/debug") }`.
In `examples/java/src/main/java/Example.java`, implement `public final class Example` with `main(String[] args)` using `try (SimulatedClock clock = new SimulatedClock(0); Machine machine = new Machine(MACHINE_JSON, clock)...)`, running the 4-step scenario and throwing `AssertionError` if any state or action count check fails.

- [x] **Step 4: Create `examples/kotlin/settings.gradle.kts`, `examples/kotlin/build.gradle.kts`, and `examples/kotlin/src/main/kotlin/Example.kt`**

In `examples/kotlin/settings.gradle.kts`, set `rootProject.name = "lavi-example-kotlin"` and `includeBuild("../../lavi-jvm")`.
In `examples/kotlin/build.gradle.kts`, apply `kotlin("jvm") version "2.1.10"` and `application`, depend on `implementation("io.lavi:lavi-jvm:0.1.0")`, set `application { mainClass.set("ExampleKt") }`, and configure `tasks.named<JavaExec>("run") { systemProperty("lavi.library.path", "${projectDir}/../../target/debug") }`.
In `examples/kotlin/src/main/kotlin/Example.kt`, implement `fun main()` using `SimulatedClock(0).use { clock -> Machine(MACHINE_JSON, clock).use { machine -> ... } }` with `onAction`, `onGuard`, and `check(...)` assertions for the 4-step scenario.

- [x] **Step 5: Run Java and Kotlin examples to verify they pass**

Run: `cargo build --package lavi-jvm && ./lavi-jvm/gradlew -p examples/java run && ./lavi-jvm/gradlew -p examples/kotlin run`
Expected: PASS (`BUILD SUCCESSFUL` for both Java and Kotlin examples)

- [x] **Step 6: Commit**

```bash
git add lavi-jvm/build.gradle.kts examples/java examples/kotlin
git commit -m "feat(examples): add self-contained Java and Kotlin Gradle examples"
```

---

### Task 4: CI/Build Integration & 7-Language Documentation [flowos: 057b46fe-ca2f-4b64-aaed-15088f20b6b6]

**Files:**
- Modify: `package.json:4-23`
- Modify: `.gitignore`
- Modify: `README.md:80-210`
- Modify: `examples/README.md`

**Interfaces:**
- Consumes: All 8 example directories (`examples/rust`, `examples/js-wasm`, `examples/c`, `examples/cpp`, `examples/python`, `examples/java`, `examples/kotlin`, `examples/esp32`)
- Produces: `npm run build:examples` (wired into `npm run build`), `npm run test:examples` (wired into `npm test`), updated `README.md` and `examples/README.md`, and clean `.gitignore`.

- [x] **Step 1: Verify `npm run test:examples` is not yet defined**

Run: `npm run test:examples`
Expected: FAIL (`Missing script: "test:examples"`)

- [x] **Step 2: Wire `build:examples` and `test:examples` into `package.json` and update `.gitignore`**

In `package.json`:
- Add `"build:examples": "cargo build --manifest-path examples/rust/Cargo.toml && make -C examples/c && make -C examples/cpp && ./lavi-jvm/gradlew -p examples/java assemble && ./lavi-jvm/gradlew -p examples/kotlin assemble"` and append `&& npm run build:examples` to `"build"`.
- Add `"test:examples": "cargo run --manifest-path examples/rust/Cargo.toml && npm --prefix examples/js-wasm start && make -C examples/c run && make -C examples/cpp run && PYTHONPATH=lavi-py/python python3 examples/python/main.py && ./lavi-jvm/gradlew -p examples/java run && ./lavi-jvm/gradlew -p examples/kotlin run && clang++ -std=c++11 -fsyntax-only -I lavi-c-cpp/include examples/esp32/test_host_compile.cpp"` and include `npm run test:examples` inside `"test"`.
In `.gitignore`, add `examples/rust/target/`, `examples/c/build/`, `examples/cpp/build/`, `examples/java/.gradle/`, `examples/java/build/`, `examples/kotlin/.gradle/`, and `examples/kotlin/build/`.

- [x] **Step 3: Update `README.md` and `examples/README.md` with all 7 languages and `examples/<lang>/` links**

In `README.md` (`## Usage Examples`), include idiomatic snippets for all 7 languages (**Rust (`lavi`)**, **JavaScript / TypeScript (`lavi-wasm`)**, **C (`lavi.h`)**, **C++11 (`lavi.hpp`)**, **Python (`lavi-py`)**, **Java (`lavi-jvm`)**, and **Kotlin (`lavi-jvm`)**) using the shared `stoplight` machine and linking each heading to its `examples/<lang>/` directory.
In `examples/README.md`, document both the self-contained per-language project directories (`examples/rust`, `examples/js-wasm`, `examples/c`, `examples/cpp`, `examples/python`, `examples/java`, `examples/kotlin`, `examples/esp32`) with their build/run commands and the interactive browser HTML demos.

- [x] **Step 4: Run full linter, build, and test suite and verify clean git status**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo clippy --manifest-path examples/rust/Cargo.toml --all-targets -- -D warnings && npm run build && npm test`
Expected: PASS (all suites including `build:examples` and `test:examples` pass with zero warnings and zero untracked build artifacts in `git status`)

- [x] **Step 5: Commit**

```bash
git add package.json .gitignore README.md examples/README.md docs/superpowers/specs/2026-11-07-language-usage-examples-design.md docs/superpowers/plans/2026-11-07-language-usage-examples.md
git commit -m "docs(examples): wire all language examples into CI and update READMEs"
```
