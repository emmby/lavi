# Portable XState

A portable, low-memory statechart runtime compatible with **XState 5**, written in `#![no_std]` Rust (`portable-xstate-rust`) with idiomatic bindings for **WebAssembly / JavaScript / TypeScript** (`portable-xstate-wasm`) and **C / C++ on embedded microcontrollers** (`portable-xstate-c-cpp` / PlatformIO).

## Core Principles

1. **Strict XState 5 Behavioral Conformance**: Validated directly against upstream `statelyai/xstate` (`v5.25.0`) conformance tests with automated provenance verification (`npm run test:conformance-check`).
2. **Microcontroller-Ready Resource Envelope**:
   - `#![no_std]` + `alloc` core (`indexmap` is the sole dependency in `portable-xstate-rust`; zero `serde` overhead).
   - Bounded $O(1)$ CPU call-stack parsing and traversal via explicit heap work-stacks (`Vec`), fitting comfortably inside an 8 KB ESP32 FreeRTOS `loopTask` stack.
   - Zero-copy `const char*` JSON configuration in C/C++ and bounded runtime memory (pre-start events are safely dropped rather than queued on the heap).
3. **Idiomatic Target-Language APIs**:
   - **JS / TS (WASM)**: `createMachine`, `createActor`, `setup`, `assign`, and `SimulatedClock`.
   - **C (`portable_xstate.h`)**: Opaque handles (`MachineContext*`, `SimulatedClock*`, `const fsm_value_t*`) and function pointers with `void* user_data`.
   - **C++11 (`portable_xstate.hpp`)**: Move-safe RAII classes (`xstate::PortableXState`, `xstate::SimulatedClock`, `xstate::Value`) supporting both zero-allocation function pointers and `std::function` lambdas.
   - **Rust (`portable-xstate-rust`)**: Strongly-typed `#![no_std]` `RuntimeMachine`, `MachineDefinition`, and `SimulatedClock`.

## Project Structure

* **`portable-xstate-rust`**: Pure `#![no_std]` + `alloc` Rust statechart engine, iterative JSON parser (`src/json.rs`), and move-based machine builder (`src/parser.rs`).
* **`portable-xstate-wasm`**: WebAssembly adapter exposing XState 5's JavaScript/TypeScript API and bridging inline JS callbacks to named runtime handlers.
* **`portable-xstate-c-cpp`**: C/C++ static library and headers (`include/portable_xstate.h`, `include/portable_xstate.hpp`) for embedded (`espressif32`) and host (`native`) targets.
* **`library.json` / `build_rust.py`**: Root PlatformIO library manifest and Cargo build hook (`lib_deps = https://github.com/emmby/portable-xstate.git`).
* **`tests/` & `tests/upstream/`**: XState 5 conformance test suite and pinned upstream reference fixtures verified by `scripts/check-conformance-tests.mjs`.
* **`AGENTS.md` / `PROCESS.md` / `TECH_DEBT.md`**: Agent engineering guidelines, autonomous swarming workflow, and legacy cleanup queue.

## Building & Testing

### Build All Targets
```bash
npm run build
```

### Run All Tests
Runs the conformance provenance check, native Rust unit tests, headless Chrome WASM tests, Playwright E2E tests, C/C++ static library + PlatformIO build script tests, and Vitest XState 5 conformance tests:
```bash
npm test
```

Or run individual suites:
- `npm run test:conformance-check` (Upstream XState 5 test provenance check)
- `npm run test:native` (Rust `#![no_std]` core tests)
- `npm run test:wasm` (WASM browser tests)
- `npm run test:e2e` (HTML examples via Playwright)
- `npm run test:c-cpp` (C, C++, and PlatformIO `build_rust.py` tests)
- `npm run test:unit` (XState 5 conformance suite via Vitest)

## Usage Examples

### JavaScript / TypeScript (WASM)
```javascript
import { createMachine, createActor, assign } from './portable-xstate-wasm/pkg-npm/portable_xstate.js';

const machine = createMachine({
  initial: 'idle',
  context: { count: 0 },
  states: {
    idle: {
      on: {
        START: {
          target: 'running',
          actions: assign({ count: ({ context }) => context.count + 1 })
        }
      }
    },
    running: {
      on: { STOP: 'idle' }
    }
  }
});

const actor = createActor(machine).start();
console.log(actor.getSnapshot().value); // "idle"

actor.send({ type: 'START' });
console.log(actor.getSnapshot().value); // "running"
console.log(actor.getSnapshot().context.count); // 1
```

### C++11 (ESP32 / PlatformIO / Native)
```cpp
#include "portable_xstate.hpp"

const char* MACHINE_JSON = R"({
  "initial": "idle",
  "states": {
    "idle": { "on": { "START": { "target": "running", "actions": ["onStart"] } } },
    "running": { "after": { "1000": "idle" } }
  }
})";

xstate::PortableXState fsm(MACHINE_JSON);
fsm.registerAction("onStart", [](const xstate::Value& ctx, const xstate::Value& evt) {
  // Handle action
});
fsm.registerClock(millis); // Or bind an xstate::SimulatedClock for deterministic tests
fsm.start();

fsm.send("START");
bool is_running = fsm.matches("running"); // true
fsm.tick();
```

## Known Discrepancy: XState 5 Parity vs. Microcontroller Memory Safety

To safely target severely memory-constrained embedded environments (such as ESP32 and Arduino microcontrollers), `portable-xstate` intentionally deviates from XState 5 on pre-initialization event buffering:

- **In XState 5 (JS)**: Calling `actor.send()` before `actor.start()` appends events to a dynamically growing heap array (mailbox buffer) flushed on `start()`.
- **In `portable-xstate`**: Calling `send()` before `start()` safely **drops** the event, preventing unbound hardware interrupts or rapid sensor polling before boot from exhausting heap memory.
