# Lavi Agent Guidelines

**Violating the letter of these rules is violating the spirit of these rules.**

`lavi` reproduces XState 5 state machine behavior and API semantics across `#![no_std]` Rust (`lavi`), WebAssembly/JS (`lavi-wasm`), C/C++ for embedded microcontrollers (`lavi-c-cpp` / PlatformIO / Arduino IDE), Python (`lavi-py`), Java/Kotlin (`lavi-jvm`), Swift (`lavi-swift`), and Dart/Flutter (`lavi-dart`).

---

## 1. The 4-Level Decision Hierarchy (Resolving Competing Goals)

When XState 5 parity, target-language idioms, and microcontroller constraints pull against each other, resolve them in this exact priority order:

### Level 1 — Hard Invariants (Never Negotiable, Zero Exceptions)
1. **Conformance Test Integrity (`tests/*.test.ts` & `tests/upstream/`)**:
   - Upstream `statelyai/xstate` conformance tests are the immutable source of truth, verified mechanically by `npm run test:conformance-check`.
   - **NEVER** modify test assertions, machine definitions, or test bodies (e.g., never inject `actor.tick()` into tests, never delete/gut code inside `it.skip(...)`, and never edit files in `tests/upstream/`).
   - **NEVER** add an unregistered `it.skip`, `describe.skip`, or `.only`, **NEVER** register a new `it.skip` in `tests/conformance-manifest.json` for an in-scope failing test just to make a test suite pass, and **NEVER** embed test-runner hooks, mock/spy resets, or global JS polyfills (e.g., calling `mockClear` on `console.warn` inside `createMachine`, or mutating `globalThis.Promise` / calling `js_sys::Function::new_no_args` inside `#[wasm_bindgen(start)]`) inside `lavi` or `lavi-wasm` runtime code. Test-environment settings and Node polyfills belong in `vitest.config.mts` and `tests/vitest.setup.ts`.
   - **When a conformance test uses dynamic JS features**:
     - *Inline JS callbacks in machine config* (`entry: ({ context }) => ...`, `guard: ...`, `assign(...)`, dynamic `delays`): **Do not skip the test.** Bridge the `js_sys::Function` to a synthetic callback ID in `lavi-wasm` so the JS test passes 100% unmodified, and expose named callback registration (`register_action` / `register_guard` / `register_delay`) in Rust, C, and C++ (see **Level 3**).
     - *Unbridgeable JS runtime artifacts* (e.g., `Symbol.observable`, JS `Proxy` / `FinalizationRegistry` internals, or XState internal private fields with no compiled-language equivalent): Follow the **Level 4** escalation protocol (`it.skip(...)` with 100% unmodified test body + `conformance-manifest.json` entry with `"category": "non-js-constraint"`).
2. **Microcontroller Safety & Resource Envelope (`lavi` & `lavi-c-cpp`)**:
   - **`#![no_std]` + `alloc` only**: Never depend on `std` in `lavi` or `lavi-c-cpp`.
   - **Microcontroller Dependency Policy (Don't Reinvent the Wheel, Gate New Crates)**:
     - Currently `lavi` depends only on `indexmap` (`default-features = false`). Never add `std`-only crates, serialization/reflection frameworks (`serde`, `serde_json`), regex engines, or expression parsers.
     - If implementing a feature cleanly would otherwise require reinventing a non-trivial data structure or algorithm from scratch (the way `indexmap` avoided hand-rolling an insertion-ordered map), **do not reinvent the wheel** and **do not silently add a crate in a swarm branch**. Pause and ask the user for approval to add the crate, providing: (1) what wheel it avoids reinventing, (2) confirmation that `default-features = false` is `#![no_std]` + `alloc` only and compiles on `wasm32-unknown-unknown` and `xtensa-esp32-espidf`, and (3) its transitive dependency count and footprint.
   - **Bounded $O(1)$ CPU Call-Stack**: Microcontrollers (such as ESP32 FreeRTOS `loopTask` with an 8 KB stack) overflow on recursive functions. All JSON parsing, state tree traversal, value inspection, and `Drop` implementations **must** use iterative heap work-stacks (`Vec`), never CPU call-stack recursion.
   - **Bounded Heap Memory & Zero Pre-Start Buffering**: Never introduce unbounded queues or buffers, and **never** buffer events sent before `machine.start()` (even in a bounded ring buffer/queue)—unstarted machines strictly drop events by design.

### Level 2 — XState 5 Statechart Semantics (Portable Core)
All statechart execution semantics—state hierarchy resolution, LCCA transition domain calculation, SCXML exit/transition/entry action ordering, eventless (`always`) loops, `assign` context updates, `raise` internal events, `after` delayed transitions, `SimulatedClock` chronological stepping, history states, `.matches()`, `.can()`, `.hasTag()`, and snapshot persistence—live in `lavi` and **must** behave identically across Rust, WASM, C, and C++.

### Level 3 — Idiomatic Target-Language Mechanics (No Greenspun's Tenth Rule)
**NEVER** invent dynamic execution, string-based expression evaluators, math parsers, or untyped runtime reflection in Rust, C, or C++ to mimic JavaScript's interpreted runtime. Instead, map XState concepts to each target language's native idiom:
- **WASM / JS (`lavi-wasm`)**: Exposes XState 5's JS API (`createMachine`, `createActor`, `setup`, `assign`, `SimulatedClock`). When a JS test passes inline JS functions (`entry: () => ...`, `guard: ({ context }) => ...`), the WASM adapter intercepts the `js_sys::Function`, assigns a synthetic name (e.g., `__inline_action_N`), stores the JS closure in the WASM instance's callback map, and passes pure declarative configuration to `lavi`.
- **Rust (`lavi`)**: Uses strongly-typed enums (`StateValue`, `StateType`, `Action`, `TransitionGuard`, `Value`), move semantics (`from_definition`, `swap_remove`), and registered `ActionFunction` / `GuardFunction` / `Clock` callbacks.
- **C (`lavi.h`)**: Uses opaque pointers (`MachineContext*`, `SimulatedClock*`, `const fsm_value_t*`), integer status codes (`0` on success, negative on error), zero-copy `const char*` JSON input, and named C function pointers (`fsm_register_action`, `fsm_register_guard`, `fsm_register_clock_with_data`) with `void* user_data`.
- **C++ (`lavi.hpp`)**: Provides header-only C++11 RAII wrappers (`lavi::Machine`, `lavi::SimulatedClock`, `lavi::Value`) with move-only semantics (`rebind_this_pointers()` on move), zero-copy `const char*` JSON storage before `start()`, and overloads for both zero-allocation C function pointers and `std::function` lambdas.

### Level 4 — Host-Language & Memory Boundary Escalation Protocol
If an upstream XState 5 test relies on a pure JavaScript runtime artifact that cannot be bridged in `lavi-wasm` and has no idiomatic compiled equivalent (e.g., JS garbage-collector `FinalizationRegistry` hooks, internal XState JS private fields, or runtime evaluation of arbitrary JS code strings), **or** conflicts with an intentional microcontroller memory invariant (such as dropping pre-start events instead of mailbox buffering):
1. Do **not** alter the test body, do **not** weaken the microcontroller memory invariant, and do **not** build an interpreter in Rust/C.
2. Document the exact constraint in [`XSTATE_5_FEATURES.md`](./XSTATE_5_FEATURES.md) (and [`README.md`](./README.md) if architectural) and register the specific test title in [`tests/conformance-manifest.json`](./tests/conformance-manifest.json) with `"category": "non-js-constraint"` or `"memory-constraint"`, keeping the test body inside `it.skip(...)` 100% unmodified.

---

## 2. Canonical Patterns vs. Legacy Anti-Patterns (Do Not Copy)

**Written rules in this document always override existing code in the repository.** Some older files contain legacy patterns from early prototypes; never copy them. When you spot a legacy anti-pattern outside your task scope, **do not refactor it mid-feature** (which causes swarm merge conflicts)—log it in [`TECH_DEBT.md`](./TECH_DEBT.md) instead.

| Area | Canonical Pattern (DO THIS) | Legacy Anti-Pattern in Repo (DO NOT COPY) |
| :--- | :--- | :--- |
| **Conformance Tests (`tests/*.test.ts`)** | Keep upstream test bodies 100% intact; bridge timers/callbacks inside `lavi-wasm`. | Injecting manual `actorRef.tick()` calls into test bodies or deleting code inside `it.skip(...)` (legacy debt in `tests/after.test.ts`, tracked as `TD-1`). |
| **Test Helpers (`tests/utils.ts`)** | Pure in-memory inspection via XState 5 public APIs (`getSnapshot()`, `provide()`, `setup()`). | Writing debug files (`fs.appendFileSync('trace.log')`), `console.log` spam, or monkey-patching `machine.createActor` (tracked as `TD-4`). |
| **JS / WASM Public API** | XState 5 `createMachine(...)` + `createActor(machine, options).start()`. | Legacy `new Actor(jsonString).start()` constructor. |
| **Rust Tree & Value Traversal (`lavi`)** | Iterative traversal using an explicit heap `Vec` work-stack (see `parser.rs`, `json.rs`, `Value::drop`, `StateDefinition::drop`). | Recursive functions on nested states or values (such as legacy `StateValue::collect_paths` or `StateNode::events`, tracked as `TD-5`). |
| **Rust Struct Fields (`lavi`)** | Strongly-typed enums (`StateType`, `HistoryType`) and XState 5 terminology (`guard`). | Legacy stringly-typed `type_: Option<String>` or XState 4 `cond` naming (tracked as `TD-6`). |
| **C/C++ Embedded Callbacks** | Pass `void* user_data` in C (`fsm_register_*`) and support zero-allocation function pointers alongside `std::function` in C++. | Global/static mutable state in C/C++ or forcing heap-allocated closures when a bare function pointer suffices. |

---

## 3. Mandatory Feature & Swarm Workflow (`PROCESS.md`)

Every statechart feature applies across **all three targets** (`lavi`, `lavi-wasm`, and `lavi-c-cpp`) and must follow [`PROCESS.md`](./PROCESS.md):

1. **Pre-Flight & Isolation**:
   - Work in an isolated branch/worktree when swarming in parallel.
   - Verify `npm test` (which runs `npm run test:conformance-check` first) passes before making changes.
2. **Automated Conformance Test Setup**:
   - Fetch unmodified upstream tests via `node scripts/fetch-upstream-test.mjs <name>.test.ts` and register the file in `tests/conformance-manifest.json`.
   - Apply **only** whitelisted compilation adaptations in `tests/<name>.test.ts`: rewriting module imports to `../lavi-wasm/pkg-npm/lavi` (and `./stubs.ts` / `./utils.ts`), adding `: any` parameter type annotations, and marking out-of-scope tests `it.skip` (with full test bodies intact and registered in `tests/conformance-manifest.json`).
   - Run `npm run test:conformance-check` (must pass) and `npx vitest run tests/<name>.test.ts` (must fail on the target feature before implementation).
3. **Cross-Target Implementation (Rust $\rightarrow$ WASM $\rightarrow$ C/C++)**:
   - Implement core statechart semantics in `lavi` (`#![no_std]`, iterative work-stacks, native Rust unit tests).
   - Update `lavi-wasm` bindings so the target tests in `tests/<name>.test.ts` pass **without editing `tests/<name>.test.ts`**.
   - Update `lavi-c-cpp` (`lavi.h`, `lavi.hpp`, `src/lib.rs`) and add C and C++ tests in `lavi-c-cpp/tests/c_test.c` and `cpp_test.cpp`.
4. **Clean Up, Verify & Document**:
   - Run `cargo clippy --workspace --all-targets -- -D warnings` and fix all warnings.
   - Run the full suite (`npm test`) and confirm `test:conformance-check`, `test:native`, `test:wasm`, `test:e2e`, `test:c-cpp`, `test:py`, `test:jvm`, `test:swift`, `test:dart`, `test:examples`, and `test:unit` all pass.
   - Always update all affected documentation ([`README.md`](./README.md), [`RELEASING.md`](./RELEASING.md), [`PROCESS.md`](./PROCESS.md), [`XSTATE_5_FEATURES.md`](./XSTATE_5_FEATURES.md), [`AGENTS.md`](./AGENTS.md), and [`TECH_DEBT.md`](./TECH_DEBT.md)) before claiming completion.

---

## 4. PlatformIO & Arduino IDE Integration Maintenance (`lavi-c-cpp/`)

PlatformIO and Arduino IDE support lives in `lavi-c-cpp/` (`lavi-c-cpp/library.json`, `lavi-c-cpp/library.properties`, `lavi-c-cpp/build_rust.py`, `lavi-c-cpp/tests/test_build_rust.py`, `scripts/build-release-natives.mjs`, and `examples/esp32`):
- **Build Script & Manifests**: If crate names, directory paths, Cargo flags, or static library names in `lavi-c-cpp` change, update `lavi-c-cpp/library.json`, `lavi-c-cpp/library.properties`, `lavi-c-cpp/build_rust.py`, `scripts/build-release-natives.mjs`, and `lavi-c-cpp/tests/test_build_rust.py` in lockstep.
- **Headers & Examples**: Keep `lavi-c-cpp/include/lavi.h`, `lavi.hpp`, `examples/esp32/src/main.cpp`, and `examples/esp32/Stoplight/Stoplight.ino` aligned with any C/C++ API changes.
- **Verification**: Always run `npm run test:c-cpp` (which executes C, C++, and `test_build_rust.py` unit tests) before claiming completion.

---

## 5. Common Rationalizations & Red Flags (STOP Immediately)

| Excuse / Rationalization | Reality |
| :--- | :--- |
| *"I've spent 2 hours on this edge case and 35/36 tests pass—I'll just tweak or `.skip` this one test line (or register it in `conformance-manifest.json`)."* | **Forbidden.** Modifying, skipping, or registering an in-scope failing conformance test in `conformance-manifest.json` hides a runtime bug. Fix the runtime or escalate to the user. |
| *"I'll adjust `tests/utils.ts` or inject `actor.tick()` into the test body like `after.test.ts` did."* | **Forbidden.** `after.test.ts` and `utils.ts` are logged in `TECH_DEBT.md` (`TD-1`, `TD-4`) as legacy violations. Never copy them or alter test bodies. |
| *"The stakeholder asked for dynamic delay/action expressions in JSON for C and Rust, so I'll write a small expression parser."* | **Forbidden (Greenspun's Tenth Rule).** Never build expression evaluators or script interpreters in Rust/C/C++. Use named callbacks (`registerAction`, `registerGuard`, `registerDelay` / function pointers) in C/C++/Rust and JS function bridging in WASM. |
| *"Recursion is simpler here and `StateValue::collect_paths` already uses recursion."* | **Forbidden.** `collect_paths` is legacy debt (`TD-5`). Recursion overflows the 8 KB ESP32 FreeRTOS stack. Always use an explicit `Vec` work-stack on the heap. |
| *"I'll buffer these pre-start events in an unbounded `Vec` (or a small capped ring buffer) so the pre-start test passes."* | **Forbidden.** Unstarted machines must drop events immediately (zero pre-start buffering, even bounded). Use the Level 4 Memory Boundary protocol. |
| *"While I'm here, I'll refactor the legacy code in adjacent files too."* | **Forbidden during feature work.** Unscoped refactoring causes merge conflicts for parallel swarm agents. Log it in `TECH_DEBT.md` and stay in scope. |
| *"A test spy isn't cleared between `it(...)` blocks and I can't edit the test file, so I'll call `mockClear()` inside `lavi-wasm`."* | **Forbidden.** Never pollute runtime/WASM code with test-runner spy hooks; standard runner settings like `clearMocks: true` belong in `vitest.config.mts`. |

### Red Flags — If You Think Any of These, STOP:
- "Just a tiny change to the test file to match our runtime..."
- "I'll parse/evaluate the expression string in Rust so C gets dynamic expressions..."
- "The state tree won't be deep enough to overflow the stack..."
- "Another test in this file called `.tick()` manually, so I will too..."
