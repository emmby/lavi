# Lavi

**Lavi** is a portable, low-memory statechart and actor runtime compatible with **XState v5**, written in Rust (`lavi`) with idiomatic bindings for **WebAssembly / JavaScript / TypeScript / React / React Native** (`lavi-wasm` / `lavi-fsm`), **C / C++ on embedded microcontrollers** (`lavi-c-cpp` / PlatformIO / Arduino IDE), **Python** (`lavi-py`), **Java / Kotlin / Android** (`lavi-jvm`), **Swift / iOS / macOS** (`lavi-swift`), and **Dart / Flutter** (`lavi-dart`).

## Why Lavi?

### Design in XState. Run Everywhere.
XState v5 and Stately Studio provide an expressive, standardized JSON configuration format for Harel Statecharts and W3C SCXML semantics. **Lavi** brings that same statechart execution model beyond JavaScript so the exact same machine definition can run identically across:
- **Embedded Microcontrollers (C / C++ / PlatformIO / Arduino IDE)**: Run statecharts on 32-bit microcontrollers—including ESP32 (Xtensa & RISC-V) and ARM Cortex-M (`cortex-m0plus`, `cortex-m3`, `cortex-m4`, `cortex-m7`, `cortex-m33`: Arduino UNO R4, Nano 33 BLE, RP2040/RP2350, SAMD21/Zero/MKR, Due, STM32, Teensy)—with `#![no_std]` safety and a bounded $O(1)$ CPU call stack.
- **Native Systems (Rust)**: Embed a lightweight `#![no_std]` + `alloc` statechart engine (`indexmap` is the sole dependency; zero `serde` overhead).
- **Browsers, Node.js, React & React Native (`lavi-wasm` / `lavi-fsm`)**: Use XState v5's `createMachine`, `createActor`, `setup`, `assign`, `SimulatedClock`, and `lavi-fsm/react` hooks (`useMachine`, `useActor`, `useActorRef`, `useSelector`) backed by WebAssembly on Web/Node and a zero-config `wasm2js` target on React Native / Expo (Hermes & JSC).
- **Backend Services & Simulation (Python)**: Run statecharts in Python services or drive deterministic tests in `pytest` with `SimulatedClock` using `lavi-py`.
- **Server JVM & Android (Java & Kotlin)**: Embed statecharts in server backends or Android mobile apps with zero-friction native loading (`.jar`) and 16 KB page-aligned AAR packaging (`.aar`) using `lavi-jvm`.
- **Apple Platforms & Server (Swift)**: Run statecharts across iOS, macOS, and Linux via Swift Package Manager (`Package.swift`) and `LaviFFI.xcframework` using `lavi-swift`.
- **Cross-Platform Mobile & Desktop (React Native / Expo, Dart & Flutter)**: Run statecharts in React Native & Expo Go apps (`lavi-fsm` / `lavi-fsm/react` with zero native linking on Hermes & JSC) and in Dart CLI/server and Flutter iOS/Android/desktop apps via `dart:ffi` using `lavi-dart`.

### Why the Name "Lavi"?
**Lavi** is named in honor of systems engineering pioneer **Dr. Jonah Z. Lavi**, who partnered with computer scientist **David Harel** in 1983–1987 to invent **Harel Statecharts** while specifying the embedded avionics software for the *Lavi* aircraft—where complex flight-control logic required a deterministic, hierarchical state model. *Lavi* (לָבִיא) also means "lion" in Hebrew.

## Core Principles

1. **Strict XState 5 Behavioral Conformance**: Validated directly against upstream `statelyai/xstate` (`v5.25.0`) conformance tests with automated provenance verification (`npm run test:conformance-check`).
2. **Microcontroller-Ready Resource Envelope**:
   - `#![no_std]` + `alloc` core (`indexmap` is the sole dependency in `lavi`; zero `serde` overhead).
   - Bounded $O(1)$ CPU call-stack parsing and traversal via explicit heap work-stacks (`Vec`), fitting comfortably inside an 8 KB ESP32 FreeRTOS `loopTask` stack.
   - Zero-copy `const char*` JSON configuration in C/C++ and bounded runtime memory (pre-start events are safely dropped rather than queued on the heap).
3. **Idiomatic Target-Language APIs**:
   - **JS / TS / React / React Native (`lavi-fsm`)**: `createMachine`, `createActor`, `setup`, `assign`, `isMachineSnapshot`, `inspect` / `actor.system.inspect`, `SimulatedClock`, and `lavi-fsm/react` (`useMachine`, `useActor`, `useActorRef`, `useSelector`).
   - **C (`lavi.h`)**: Opaque handles (`MachineContext*`, `SimulatedClock*`, `const fsm_value_t*`) and function pointers (`fsm_register_action`, `fsm_register_guard`, `fsm_register_inspector`) with `void* user_data`.
   - **C++11 (`lavi.hpp`)**: Move-safe RAII classes (`lavi::Machine`, `lavi::SimulatedClock`, `lavi::Value`) supporting both zero-allocation function pointers and `std::function` lambdas.
   - **Rust (`lavi`)**: Strongly-typed `#![no_std]` `RuntimeMachine`, `MachineDefinition`, `InspectEvent`, and `SimulatedClock`.
   - **Python (`lavi-py`)**: Idiomatic `Machine`, `SimulatedClock`, `Value`, decorators (`@machine.action`, `@machine.guard`, `@machine.inspector`), context managers (`with Machine(...)`), and dict/list payload interop.
   - **Java (`lavi-jvm`)**: `AutoCloseable` `Machine` and `SimulatedClock`, fluent registration (`registerAction`, `registerGuard`, `registerInspector`), and zero-GC-leak callback mechanics.
   - **Kotlin (`lavi-jvm`)**: Trailing lambdas (`onAction`, `onGuard`, `onClock`, `onInspect`), operator indexing (`ctx["key"]`), and `Map<String, Any?>` event payload support.
   - **Swift (`lavi-swift`)**: ARC-managed `Machine` and `SimulatedClock`, subscript value access (`ctx["key"]`), trailing closures (`registerAction`, `registerGuard`, `registerDelay`, `registerInspector`), and `[String: Any]` payload support.
   - **Dart / Flutter (`lavi-dart`)**: `Finalizer`-backed `Machine` and `SimulatedClock` with deterministic `.dispose()`, `operator []` value inspection, `registerInspector`, and `Map<String, Object?>` payload support.

## Interactive WASM Demos

Hierarchical statecharts replace fragile `if/else` boolean-flag logic with declarative guarantees—such as parent-state `exit` safety cutoffs, eventless `always` guards, `after` timers, and `history` recovery—that run identically in the browser (WASM) and on microcontrollers (Rust / C / C++).

👉 **[Explore the Live Interactive WASM Examples →](https://emmby.github.io/lavi/examples/)**

<a href="https://emmby.github.io/lavi/examples/">
  <img src="./docs/images/robot-vacuum-demo.png" alt="Lavi Interactive WASM Demo" width="540">
</a>

## Project Structure

* **`lavi`**: Pure `#![no_std]` + `alloc` Rust statechart engine, iterative JSON parser (`src/json.rs`), and move-based machine builder (`src/parser.rs`).
* **`lavi-wasm`**: WebAssembly adapter exposing XState 5's JavaScript/TypeScript API and bridging inline JS callbacks to named runtime handlers.
* **`lavi-c-cpp`**: C/C++ static and shared library, headers (`include/lavi.h`, `include/lavi.hpp`), and PlatformIO/Arduino IDE manifests (`library.json`, `library.properties`, `build_rust.py`) for embedded (`espressif32`) and host (`native`) targets.
* **`lavi-py`**: Python native extension via PyO3 (`abi3-py38`) with full `.pyi` type stubs and pytest integration.
* **`lavi-jvm`**: Java and Kotlin JNI bridge via `jni-rs` with Gradle `java-library`, desktop resource extraction, and Android AAR packaging.
* **`lavi-swift` & `Package.swift`**: Swift Package Manager targets (`CLavi`, `Lavi`, `LaviTests`) wrapping the C ABI for iOS, macOS, and Linux.
* **`lavi-dart`**: Dart / Flutter `dart:ffi` package (`package:lavi/lavi.dart`) wrapping `liblavi_c_cpp.{dylib,so,dll}` for mobile, desktop, and server.
* **`scripts/build-release-natives.mjs`**: Single-host cross-compilation plan generator, Android AAR packager (16 KB page-alignment check), Apple `LaviFFI.xcframework.zip` packager, and unified Arduino IDE / PlatformIO `.zip` packager (`dist/embedded/lavi-embedded.zip`).
* **`tests/` & `tests/upstream/`**: XState 5 conformance test suite and pinned upstream reference fixtures verified by `scripts/check-conformance-tests.mjs`.
* **`AGENTS.md` / `PROCESS.md` / `TECH_DEBT.md`**: Agent engineering guidelines, autonomous swarming workflow, and legacy cleanup queue.

## Building & Testing

### Build All Targets
```bash
npm run build
```

### Run All Tests
Runs the conformance provenance check, native Rust unit tests, headless Chrome WASM tests, Playwright E2E tests, C/C++ static library + PlatformIO build script tests, Python pytest tests, Java & Kotlin JUnit tests, Swift XCTests, Dart tests, self-contained language examples, and Vitest XState 5 conformance tests:
```bash
npm test
```

Or run individual suites:
- `npm run test:conformance-check` (Upstream XState 5 test provenance check)
- `npm run test:native` (Rust `#![no_std]` core tests)
- `npm run test:wasm` (WASM browser tests)
- `npm run test:e2e` (HTML examples via Playwright)
- `npm run test:c-cpp` (C, C++, and PlatformIO `build_rust.py` tests)
- `npm run test:py` (Python unit tests via pytest)
- `npm run test:jvm` (Java and Kotlin unit tests via Gradle JUnit 5)
- `npm run test:swift` (Swift Package Manager unit tests via `swift test`)
- `npm run test:dart` (Dart FFI unit tests via `dart test`)
- `npm run test:examples` (Self-contained Rust, JS/WASM, C, C++, Python, Java, Kotlin, Swift, Dart, and ESP32 examples)
- `npm run test:unit` (XState 5 conformance suite via Vitest)

## Installation

| Ecosystem | Registry | Package / Artifact | Install Command |
| :--- | :--- | :--- | :--- |
| **JavaScript / TypeScript / React / React Native** | [npm](https://www.npmjs.com/package/lavi-fsm) | `lavi-fsm` | `npm install lavi-fsm` |
| **Rust (`#![no_std]`)** | [crates.io](https://crates.io/crates/lavi) | `lavi` | `cargo add lavi` |
| **Python (`abi3` wheels)** | [PyPI](https://pypi.org/project/lavi/) | `lavi` | `pip install lavi` / `uv add lavi` |
| **Java / Kotlin (Desktop Fat JAR)** | [Maven Central](https://central.sonatype.com/artifact/io.github.emmby/lavi) | `io.github.emmby:lavi` | `implementation("io.github.emmby:lavi:0.2.0")` |
| **Android (16 KB-aligned AAR)** | [GitHub Releases](https://github.com/emmby/lavi/releases) | `lavi-release.aar` | `implementation(files("libs/lavi-release.aar"))` |
| **Swift (iOS / macOS / Linux)** | [Swift Package Manager & GitHub Releases](https://github.com/emmby/lavi/releases) | `Lavi` / `LaviFFI.xcframework.zip` | `.package(url: "https://github.com/emmby/lavi.git", from: "0.2.0")` |
| **Dart / Flutter (`dart:ffi`)** | Git / Workspace (`lavi-dart`) | `lavi` | `dart pub add lavi --path lavi-dart` |
| **C / C++ (Arduino IDE & PlatformIO)** | [GitHub Releases](https://github.com/emmby/lavi/releases) | `lavi-embedded.zip` | Arduino IDE *Add .ZIP Library...* or PlatformIO `lib_deps` |

## Usage Examples

Every supported language has a self-contained, buildable example project under [`examples/`](./examples/) running the same `stoplight` statechart (`red` $\rightarrow$ `green` $\rightarrow$ `yellow` $\rightarrow$ `red` with `entry` actions, `after` delayed transitions via `SimulatedClock`, and a guarded `PEDESTRIAN_WAIT` event):

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

### Rust (`lavi` — [`examples/rust/`](./examples/rust/))

```bash
cargo add lavi
```

```rust
use indexmap::IndexMap;
use lavi::{ActionFunction, GuardFunction, RuntimeMachine, SimulatedClock, Value};
use std::rc::Rc;

let clock = SimulatedClock::with_initial(0);
let mut machine = RuntimeMachine::from_json(MACHINE_JSON).unwrap();
machine.register_clock(&clock);

let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();
guards.insert(
    "canCross".to_string(),
    Rc::new(|ctx, _evt, _params| {
        ctx.as_object()
            .and_then(|o| o.get("cycles"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) >= 1.0
    }),
);
let actions: IndexMap<String, ActionFunction> = IndexMap::new();

machine.start(&guards, &actions);
assert!(machine.matches("red"));

clock.increment(1000, &mut machine, &guards, &actions);
assert!(machine.matches("green"));

machine.transition("PEDESTRIAN_WAIT", &Value::event("PEDESTRIAN_WAIT"), &guards, &actions);
assert!(machine.matches("yellow"));
```

### JavaScript / TypeScript (`lavi-fsm` — [`examples/js-wasm/`](./examples/js-wasm/))

```bash
npm install lavi-fsm
```

For the supported statechart subset (see [`XSTATE_5_FEATURES.md`](./XSTATE_5_FEATURES.md)), `lavi-fsm` is a **1-line import replacement** for `xstate` in Node.js, browser bundlers, and ES modules:

```diff
- import { createMachine, createActor, setup, assign, raise, SimulatedClock } from 'xstate';
+ import { createMachine, createActor, setup, assign, raise, SimulatedClock } from 'lavi-fsm';
```

```javascript
import { createMachine, createActor, SimulatedClock } from 'lavi-fsm';

const machine = createMachine(
  {
    id: 'stoplight',
    initial: 'red',
    context: { cycles: 1 },
    states: {
      red: { entry: ['onRed'], after: { 1000: 'green' } },
      green: {
        entry: ['onGreen'],
        on: { PEDESTRIAN_WAIT: { target: 'yellow', guard: 'canCross' } },
        after: { 1000: 'yellow' }
      },
      yellow: { entry: ['onYellow'], after: { 1000: 'red' } }
    }
  },
  {
    guards: { canCross: ({ context }) => context.cycles >= 1 }
  }
);

const clock = new SimulatedClock();
const actor = createActor(machine, { clock }).start();
console.log(actor.getSnapshot().matches('red')); // true

clock.increment(1000);
console.log(actor.getSnapshot().matches('green')); // true

actor.send({ type: 'PEDESTRIAN_WAIT' });
console.log(actor.getSnapshot().matches('yellow')); // true
```

In **React** (Web) and **React Native / Expo** (Hermes & JSC, resolved automatically via the `"react-native"` export condition with zero native linking or `WebAssembly` global required), use `lavi-fsm/react`:

```tsx
import { useMachine, useSelector } from 'lavi-fsm/react';

const [snapshot, send, actorRef] = useMachine(machine);
const cycles = useSelector(actorRef, (s) => s.context.cycles);
```

### C (`lavi.h` — [`examples/c/`](./examples/c/))
```c
#include "lavi.h"
#include <assert.h>

static bool can_cross(const char* name, const fsm_value_t* ctx, const fsm_value_t* evt, void* user_data) {
  const fsm_value_t* cycles = fsm_value_get_property(ctx, "cycles");
  return cycles != NULL && fsm_value_get_number(cycles) >= 1.0;
}

SimulatedClock* clock = fsm_simulated_clock_create(0);
MachineContext* machine = fsm_create(MACHINE_JSON);
fsm_register_simulated_clock(machine, clock);
fsm_register_guard(machine, "canCross", can_cross, NULL);

fsm_start(machine);
assert(fsm_matches(machine, "red"));

fsm_simulated_clock_increment(clock, 1000);
assert(fsm_matches(machine, "green"));

fsm_send_event(machine, "PEDESTRIAN_WAIT");
assert(fsm_matches(machine, "yellow"));

fsm_free(machine);
fsm_simulated_clock_free(clock);
```

### C++11 (`lavi.hpp` — [`examples/cpp/`](./examples/cpp/) & [`examples/esp32/`](./examples/esp32/))

For 32-bit embedded projects, precompiled static libraries (`liblavi_c_cpp.a`) are provided for all 6 ESP32 architectures (`esp32`, `esp32s2`, `esp32s3`, `esp32c3`, `esp32c6`, `esp32h2`) and 5 ARM Cortex-M architectures (`cortex-m0plus`, `cortex-m3`, `cortex-m4`, `cortex-m7`, `cortex-m33` — covering Arduino UNO R4, Nano 33 BLE, RP2040/RP2350, SAMD21/Zero/MKR, Due, STM32, and Teensy; no Rust toolchain required):
- **Arduino IDE**: Search for **`lavi`** in the **Arduino Library Manager**, or download `lavi-embedded.zip` from [GitHub Releases](https://github.com/emmby/lavi/releases) and install via **Sketch → Include Library → Add .ZIP Library...** (see [`examples/esp32/Stoplight/Stoplight.ino`](./examples/esp32/Stoplight/Stoplight.ino)).
- **PlatformIO**: Add `lib_deps = emmby/lavi@^0.2.0` (or `https://github.com/emmby/lavi/releases/download/v0.2.0/lavi-embedded.zip`) to [`platformio.ini`](./examples/esp32/platformio.ini).

```cpp
#include "lavi.hpp"
#include <cassert>

lavi::SimulatedClock clock(0);
lavi::Machine machine(MACHINE_JSON, clock); // Or pass `registerClock(millis)` on ESP32

machine.registerGuard("canCross", [](const lavi::Value& ctx, const lavi::Value&) {
  double cycles = 0.0;
  ctx.getProperty("cycles").getNumber(cycles);
  return cycles >= 1.0;
});

machine.start();
assert(machine.matches("red"));

clock.increment(1000);
assert(machine.matches("green"));

machine.send("PEDESTRIAN_WAIT");
assert(machine.matches("yellow"));
```

### Python (`lavi` — [`examples/python/`](./examples/python/))

```bash
pip install lavi
```

```python
from lavi import Machine, SimulatedClock, Value

clock = SimulatedClock(initial_ms=0)
with Machine(MACHINE_JSON, clock=clock) as machine:
    @machine.guard("canCross")
    def can_cross(ctx: Value, _evt: Value) -> bool:
        return (ctx["cycles"].as_int() or 0) >= 1

    machine.start()
    assert machine.matches("red")

    clock.increment(1000)
    assert machine.matches("green")

    machine.send("PEDESTRIAN_WAIT")
    assert machine.matches("yellow")
```

### Java (`io.github.emmby:lavi` — [`examples/java/`](./examples/java/))

```kotlin
// build.gradle.kts (Maven Central desktop fat JAR; or use `lavi-release.aar` on Android)
dependencies {
    implementation("io.github.emmby:lavi:0.2.0")
}
```

```java
import io.lavi.Machine;
import io.lavi.SimulatedClock;

try (SimulatedClock clock = new SimulatedClock(0);
     Machine machine = new Machine(MACHINE_JSON, clock)
         .registerGuard("canCross", (ctx, evt) ->
             ctx.getProperty("cycles").asLong().orElse(0L) >= 1L)) {
    machine.start();
    assert machine.matches("red");

    clock.increment(1000);
    assert machine.matches("green");

    machine.send("PEDESTRIAN_WAIT");
    assert machine.matches("yellow");
}
```

### Kotlin (`io.github.emmby:lavi` — [`examples/kotlin/`](./examples/kotlin/))

```kotlin
// build.gradle.kts (Maven Central desktop fat JAR; or use `lavi-release.aar` on Android)
dependencies {
    implementation("io.github.emmby:lavi:0.2.0")
}
```

```kotlin
import io.lavi.*

SimulatedClock(0).use { clock ->
    Machine(MACHINE_JSON, clock).use { machine ->
        machine.onGuard("canCross") { ctx, _ ->
            (ctx["cycles"].asLongOrNull() ?: 0L) >= 1L
        }

        machine.start()
        check(machine.matches("red"))

        clock.increment(1000)
        check(machine.matches("green"))

        machine.send("PEDESTRIAN_WAIT")
        check(machine.matches("yellow"))
    }
}
```

### Swift (`Lavi` — [`examples/swift/`](./examples/swift/))

```swift
// Package.swift
dependencies: [
    .package(url: "https://github.com/emmby/lavi.git", from: "0.2.0")
]
```

```swift
import Lavi

let clock = SimulatedClock(initialMs: 0)
let machine = try Machine(json: MACHINE_JSON, clock: clock)

machine.registerGuard("canCross") { ctx, _ in
    (ctx["cycles"].int ?? 0) >= 1
}

try machine.start()
assert(machine.matches("red"))

clock.increment(1000)
assert(machine.matches("green"))

machine.send("PEDESTRIAN_WAIT")
assert(machine.matches("yellow"))
```

### Dart / Flutter (`lavi` — [`examples/dart/`](./examples/dart/))

```yaml
# pubspec.yaml
dependencies:
  lavi:
    path: ../lavi-dart
```

```dart
import 'package:lavi/lavi.dart';

final clock = SimulatedClock(0);
final machine = Machine(machineJson, clock: clock);

machine.registerGuard('canCross', (ctx, _) => (ctx['cycles'].asInt ?? 0) >= 1);

machine.start();
assert(machine.matches('red'));

clock.increment(1000);
assert(machine.matches('green'));

machine.send('PEDESTRIAN_WAIT');
assert(machine.matches('yellow'));

machine.dispose();
clock.dispose();
```

## How Lavi Compares

- **[XState v5](https://stately.ai/docs/xstate)** is the reference standard when your entire stack runs on JavaScript/TypeScript; **Lavi** brings those exact statechart semantics and Stately Studio JSON definitions to `#![no_std]` Rust, C, C++, WebAssembly, Python, Java, Kotlin, Swift, and Dart/Flutter (with C# planned) via a single shared native engine.
- **[StateSmith](https://github.com/StateSmith/StateSmith)**, **[`statig`](https://github.com/mdeloof/statig)**, and **[Boost.SML](https://github.com/boost-ext/sml)** are ideal when you need compile-time code generators or macros with **zero heap (`no-alloc`)** on tiny 8-bit or Cortex-M0 chips; **Lavi** uses `#![no_std]` + `alloc` (targeting 32-bit MCUs like ESP32, desktop, and web) to unlock runtime/OTA JSON loading, snapshot persistence, parallel states, and declarative `after` timers across languages.
- **[QP/C & QP/C++](https://www.state-machine.com/products/qp)** provide a full Active Object RTOS framework (dual GPLv3 / commercial); **Lavi** is an MIT-licensed, standalone statechart engine that drops into any existing RTOS (FreeRTOS, Zephyr), async runtime, or UI loop.
- **[itemis CREATE](https://www.itemis.com/en/products/itemis-create/)** and **MathWorks Stateflow** serve enterprise workflows requiring proprietary IDEs and safety-certification kits; **Lavi** is open-source, uses standard XState v5 JSON, and verifies correctness directly against the upstream XState test suite.

| Feature | **Lavi** | **XState v5** | **StateSmith** | **itemis CREATE** | **QP/C & C++** | **`statig`** | **Boost.SML** |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **License** | MIT | MIT | MIT / Apache-2.0 | Commercial | GPLv3 / Comm. | MIT / Apache-2.0 | Boost |
| **Execution Model** | Runtime Engine | Runtime Engine | Code Generator | Code Generator | Framework + Codegen | Rust Macro | C++ Template |
| **Visual Editor** | Stately Studio | Stately Studio | draw.io / PlantUML | itemis IDE | QM Modeler | — | — |
| **Languages** | Rust, JS/TS, C, C++, Py, Java, Kotlin, Swift, Dart *(+C# planned)* | JS, TS | C, C++, C#, Py, JS/TS | C, C++, Java, Py, TS, C# | C, C++ | Rust | C++ |
| **Embedded Support** | `#![no_std]` + `alloc` ($O(1)$ stack) | — | Zero-heap | Zero-heap | Zero-heap | Zero-heap | Zero-heap |
| **Runtime / OTA Config** | ✅ | ✅ | — *(Compile-time)* | — *(Compile-time)* | — *(Compile-time)* | — *(Compile-time)* | — *(Compile-time)* |
| **Parallel States** | ✅ | ✅ | — | ✅ | Manual¹ | — | ✅ |
| **Shallow & Deep History** | ✅ | ✅ | ✅ *(Plugin/map)* | ✅ | ✅ | — | Shallow only |
| **Declarative `after` & `SimulatedClock`** | ✅ | ✅ | Manual² | ✅ | Manual² | — | — |
| **Snapshot Save / Restore** | ✅ | ✅ | — | — | — | — | — |
| **XState v5 Conformance** | [◐ Core HSM (16/19 suites)](./XSTATE_5_FEATURES.md) | Reference | — | — | — | — | — |

*¹ Via QP's manual "Orthogonal Component" pattern. ² Via user-managed timer events rather than declarative `after` delays.*

## For Contributors: Agentic Development & Parallel Swarming

In any new conversation on this repository, [`AGENTS.md`](./AGENTS.md) is automatically loaded into the agent's system prompt, enforcing the [`PROCESS.md`](./PROCESS.md) cross-target workflow (`Rust` $\rightarrow$ `WASM` $\rightarrow$ `C/C++` $\rightarrow$ `Python` $\rightarrow$ `Java/Kotlin` $\rightarrow$ `Swift` $\rightarrow$ `Dart`) and automated conformance provenance checks (`npm run test:conformance-check`).

You can launch agents in three ways:

### 1. Use `/teamwork` (or ask for a parallel subagent swarm in one conversation)
Start a new conversation and either type **`/teamwork`** or prompt:
> *"Pick 3 orthogonal tasks from `XSTATE_5_FEATURES.md` / `TECH_DEBT.md` and spawn 3 parallel subagents in isolated branches (`Workspace: "branch"`) following `PROCESS.md`, then audit and merge their branches."*

The main agent acts as the coordinator, spawns `self` subagents with `Workspace: "branch"` (each in its own git worktree), waits for them to finish, audits their diffs against `AGENTS.md`, and merges them cleanly.

### 2. Ask for a specific set of features or tech-debt items
If you already know which conformance suites or `TD-N` cleanup items you want implemented:
> *"Spawn 2 parallel branch subagents following `PROCESS.md`:*
> *1. Resolve `TD-1` (`after.test.ts` full upstream restoration) from `TECH_DEBT.md`.*
> *2. Onboard `guards.test.ts` across Rust, WASM, and C/C++ under strict provenance."*

### 3. Separate top-level conversations in Antigravity (Worktree mode)
If you prefer separate top-level chat threads instead of subagents:
- Open a new conversation in the Agent Manager with **Worktree isolation** enabled.
- Give it a single one-liner:
  > *"Follow `PROCESS.md` to onboard `final.test.ts` across Rust, WASM, and C/C++."* or *"Resolve `TD-6` from `TECH_DEBT.md`."*

## Reporting Issues

Found a bug, a discrepancy with XState v5 behavior, or a language-binding issue? Please [open an issue on GitHub](https://github.com/emmby/lavi/issues/new)!

To help us reproduce and fix it quickly, please include:
- The **machine JSON definition** (or XState config)
- The **relevant host code** (registered actions/guards/delays, events sent, and target language/platform)
- The **expected vs. actual state or snapshot**

## License & Attribution

- **Lavi Runtime (`lavi`, `lavi-wasm`, `lavi-c-cpp`, `lavi-py`, `lavi-jvm`, `lavi-swift`, `lavi-dart`)**: Licensed under the [MIT License](./LICENSE) (`Copyright (c) 2025-2026 Mike Burton`).
- **Conformance Test Suite (`tests/` & `tests/upstream/`)**: Adapted from the [statelyai/xstate](https://github.com/statelyai/xstate) repository (`xstate@5.25.0`), `Copyright (c) 2015 David Khourshid`, used under the MIT License. See [`tests/LICENSE-THIRD-PARTY`](./tests/LICENSE-THIRD-PARTY) for the full notice.
- **Standards & Prior Art**: Statechart execution semantics follow David Harel's Statecharts formalism (1987) and the [W3C SCXML Specification](https://www.w3.org/TR/scxml/).
- **Acknowledgments & Non-Affiliation**: With thanks to David Khourshid and the Stately.ai team for their work on XState and Stately Studio. Lavi is an independent community project and is not affiliated with, officially maintained by, or endorsed by Stately.ai or David Khourshid.
