# Initial Eventless (`always`) Transitions, XState v5 Start Parity, and PlatformIO `native` Support

## 1. Intent and Why

We are migrating an ESP32 + PlatformIO project (`ClockMatter`) from itemis CREATE to `portable-xstate` and `portable-xstate-platformio`. A core design principle of `portable-xstate` is behavioral consistency with original JavaScript XState v5 (`xstate`) across all runtimes: Rust (`portable-xstate-core`), WebAssembly (`portable-xstate-wasm`), and C/C++ FFI (`portable-xstate-ffi` / `portable-xstate-platformio`).

Investigation and oracle comparison against XState v5 (`^5.25.0`) revealed specific gaps across `emmby/portable-xstate` and `emmby/portable-xstate-platformio`:

### Root Causes in Current Architecture

1. **C/C++ FFI `fsm_start()` Does Not Settle Initial Eventless (`always`) Transitions**:
   - In [`portable-xstate-ffi/src/lib.rs`](file:///Users/mike/GitHub/portable-xstate/portable-xstate-ffi/src/lib.rs#L84-L90), `fsm_start` calls `ctx.machine.start(&ctx.actions)`, which enters the initial state and runs its `entry` actions, but never evaluates eventless (`always`) transitions or passes `ctx.guards`.
   - Any machine whose initial state has an `always` transition (including guarded branches or chained transient states) stops in the transient initial state instead of settling into the target state and scheduling its `after` timers.

2. **XState v5 Initialization Macrostep Discrepancies in `portable-xstate-core` and `portable-xstate-wasm`**:
   - **Order of Initial `always` Transitions vs. Raised Internal Events**:
     - [`RuntimeMachine::start`](file:///Users/mike/GitHub/portable-xstate/portable-xstate-core/src/lib.rs#L270-L305) currently calls `self.drain_internal_queue(&IndexMap::new(), actions)` *before* any initial `always` transition is evaluated.
     - In XState v5, if an initial state has both `entry: [raise({ type: 'RAISED_EV' })]` and an `always` transition, XState v5 evaluates the initial state's `always` transition *first*, and then processes `RAISED_EV` from the internal queue in the state reached via `always`.
   - **Missing Guards During Initialization (`start`)**:
     - `RuntimeMachine::start(&mut self, actions: &IndexMap<String, ActionFunction>)` does not accept `guards: &IndexMap<String, GuardFunction>` and passes `&IndexMap::new()` to `drain_internal_queue`. Because missing guards in `transition_internal` default to `true` (`unwrap_or(true)`), guarded transitions triggered by `raise()` during `start()` erroneously bypass their guards and always succeed.
   - **Compound Initial State Action Ordering**:
     - In [`RuntimeMachine::start`](file:///Users/mike/GitHub/portable-xstate/portable-xstate-core/src/lib.rs#L296-L302), `self.initial_actions` (all `initial.actions` collected across root and nested compound states) are executed *before* `entry_actions` (`root.entry`, `parent.entry`, `child.entry`).
     - In XState v5, initialization executes actions in root-to-leaf tree entry order: `root.entry` $\rightarrow$ `root.initial.actions` $\rightarrow$ `parent.entry` $\rightarrow$ `parent.initial.actions` $\rightarrow$ `child.entry`.
   - **Initial Event Object (`{ "type": "xstate.init" }`) and FFI Event Payload**:
     - In XState v5, all initial `entry` actions, `initial.actions`, and initial `always` guards/actions/target-entries receive `{ "type": "xstate.init" }` as their event payload. Currently `RuntimeMachine::start` passes `Value::Null`.
     - Similarly, [`fsm_send_event`](file:///Users/mike/GitHub/portable-xstate/portable-xstate-ffi/src/lib.rs#L145-L154) passes `Value::Null` as the event payload instead of `{ "type": "<event_name>" }` (whereas `portable-xstate-wasm` constructs `{ "type": "<event_name>" }` for string events).
   - **Reentrant External `send()` During `start()`**:
     - `RuntimeMachine::start` does not set `self.is_processing = true` during initialization. If an initial state's `entry` action calls `fsm_send_event` / `actor.send` while the initial state also has an `always` transition, the external event executes synchronously *inside* `entry` before the initial `always` transition or remaining initial entry actions run.
     - In XState v5, external events sent during `.start()` are queued on the external mailbox until the entire initial macrostep (initial entry actions + initial `always` transitions + internal raised queue) settles, and are then drained against the settled state.
   - **WASM `StateNode::create_actor_method` Premature `handle_always_transition()`**:
     - In [`portable-xstate-wasm/src/lib.rs`](file:///Users/mike/GitHub/portable-xstate/portable-xstate-wasm/src/lib.rs#L640-L644), `StateNode::create_actor_method` calls `runner.handle_always_transition()` before `.start()` is called (while `is_started == false` and before initial state `entry` actions have executed).
     - Consequently, calling `createActor(machine).start()` on a machine with initial `always` transitions:
       1. Evaluates `always` guards before initial state `entry` assignments have run.
       2. Mutates `runner.machine.current_state` to the settled target state during `createActor()`, skipping the initial state's `entry` actions entirely while firing transition and target `entry` side-effect callbacks before `.start()`.
       3. Runs the settled state's `entry` actions a *second* time when `.start()` is called.

3. **`portable-xstate-platformio` Upstream Sync & PlatformIO `native` Environment Support**:
   - [`portable-xstate-platformio/ library.json`](file:///Users/mike/GitHub/portable-xstate-platformio/library.json) declares `"frameworks": ["arduino", "espidf"]`. When a consumer project runs `pio test -e native` (`platform = native`, which has no framework), PlatformIO's Library Dependency Finder (LDF) filters out `portable-xstate-platformio` unless `"frameworks"` includes `"*"`.
   - [`portable-xstate-platformio/build_rust.py`](file:///Users/mike/GitHub/portable-xstate-platformio/build_rust.py) is pinned to an older `portable-xstate` commit and calls `Import("env")` unconditionally at import time, preventing [`test_build_rust.py`](file:///Users/mike/GitHub/portable-xstate-platformio/test_build_rust.py) from importing and testing the actual functions in `build_rust.py`.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **XState v5 Initialization Parity Across All Runtimes**:
  - Centralize the initialization macrostep in `portable-xstate-core` (`RuntimeMachine::start(&mut self, guards, actions)`) so Rust, C/C++ FFI, and WASM share identical XState v5 semantics.
  - Execute initial actions in root-to-leaf order (`root.entry` $\rightarrow$ `root.initial.actions` $\rightarrow$ `state.entry` $\rightarrow$ `state.initial.actions`).
  - Pass `{ "type": "xstate.init" }` to initial `entry` actions, `initial.actions`, and initial `always` guards/actions/target-entries.
  - Settle initial `always` transitions (including guarded branches and chained transient states) *before* draining `internal_queue` (`raise()`), looping until both `always` transitions and `internal_queue` settle.
  - Defer any reentrant external `send()` / `transition()` calls made during initialization onto `external_queue` (`self.is_processing = true`) and drain them after the initial macrostep settles.
  - Schedule `after` timers on states entered during initial `always` transitions (and cancel `after` timers on transient initial states exited during initial `always` transitions).
  - Fix WASM `createActor(machine)` vs. `actor.start()` so side-effect actions only execute on `.start()` in exact XState v5 order, while `createActor(machine).getSnapshot()` before `.start()` still previews the settled initial state value and context without mutating the unstarted actor or firing JS side effects.
- **PlatformIO `native` & Upstream Sync in `portable-xstate-platformio`**:
  - Update `DEPENDENCY_REF` in `build_rust.py` to the updated `portable-xstate` commit and keep `include/portable_xstate.h` and `include/portable_xstate.hpp` in sync.
  - Update `library.json` (`"frameworks": ["*"]`) and `build_rust.py` so `pio test -e native` builds and links `libportable_xstate_ffi.a` on the host machine without requiring an ESP32 `BoardConfig` or `-Z build-std`, while keeping existing ESP32 (`xtensa-*` and `riscv32*`) builds unchanged.
  - Make `build_rust.py` directly importable in unit tests so `test_build_rust.py` tests the real `build_rust.py` implementation.

### Non-Goals
- Changing how `portable-xstate` parses JSON or represents state values (covered separately by the low-memory JSON parsing spec).
- Adding new FFI functions to `portable_xstate.h` beyond making `fsm_start` and `fsm_send_event` conform to XState v5 behavior.

### Anti-Goals
- **No Duplicate Macrostep Logic Across Crates**: `portable-xstate-ffi` and `portable-xstate-wasm` must not each re-implement ad-hoc `while` loops over `machine.transition("", ...)` to settle initial `always` transitions; `RuntimeMachine` in `portable-xstate-core` must own the initialization macrostep.
- **No Premature Side-Effect Execution Before `.start()`**: Creating a machine or actor (`fsm_create`, `PortableXState` constructor, `createActor`) must never execute external action callbacks before `start()` is invoked.

---

## 3. Constraints

- **`#![no_std]` + `alloc` Compatibility**: All changes in `portable-xstate-core` and `portable-xstate-ffi` must remain strictly `#![no_std]` compatible.
- **Stable C/C++ FFI Signatures**: Keep `portable_xstate.h` and `portable_xstate.hpp` public API signatures unchanged.
- **Full Test Suite & Oracle Conformance**: All Vitest unit tests (`npm run test:unit`), Rust native tests (`npm run test:native`), C/C++ FFI tests (`npm run test:ffi`), WASM headless browser tests (`npm run test:wasm`), Playwright E2E tests (`npm run test:e2e`), and PlatformIO native/unit tests must pass.

---

## 4. Architecture & Design Decisions

### 4.1 `RuntimeMachine::start` & XState v5 Initialization Macrostep (`portable-xstate-core`)

In [`portable-xstate-core/src/lib.rs`](file:///Users/mike/GitHub/portable-xstate/portable-xstate-core/src/lib.rs#L270-L305):

1. **Update `pub fn start(&mut self, guards: &IndexMap<String, GuardFunction>, actions: &IndexMap<String, ActionFunction>)`** (matching `RuntimeMachine::transition` and `RuntimeMachine::tick`):
   - Guard against repeated starts:
     ```rust
     if self.is_started { return; }
     self.is_started = true;
     self.is_processing = true;
     ```
   - Construct the XState v5 initialization event:
     ```rust
     let mut init_map = IndexMap::new();
     init_map.insert("type".to_string(), Value::String("xstate.init".to_string()));
     let init_event = Value::Object(init_map);
     ```

2. **Root-to-Leaf Action Collection & Initial Timer Scheduling**:
   - Clear `self.initial_actions` (`let _ = core::mem::take(&mut self.initial_actions);`) and build the ordered initial action list `init_actions_to_run`:
     1. Root `entry` actions: `self.definition.entry`.
     2. Root `after` timers: schedule `("", now + delay, t_defs.clone())` for each entry in `self.definition.after`.
     3. Root `initial_actions`: `self.definition.initial_actions`.
     4. If the root `initial` target is a history state in `self.definition.states`, include its `history_actions`.
     5. For each active path in `self.current_state.get_active_paths()`, iterate path prefixes `curr_path = parts[0..i].join(".")` for `i in 1..=parts.len()` (skipping empty parts and already-seen `curr_path`s):
        - Look up `def = Self::get_state_def_recursive(&self.definition.states, &curr_path)`.
        - Append `def.entry`.
        - Schedule `(curr_path.clone(), now + delay, t_defs.clone())` for each entry in `def.after`.
        - If `matches!(def.state_type, StateType::Compound)`, append `def.initial_actions` (and if `def.initial` targets a child history state, its `history_actions`).
   - Execute each action in `init_actions_to_run` with `&init_event`:
     ```rust
     for action in &init_actions_to_run {
         Self::execute_action(&mut self.context, action, &init_event, actions, &mut self.internal_queue);
     }
     ```

3. **Settle Initial `always` Transitions and `internal_queue` (`raise`), Then Drain `external_queue`**:
   - Evaluate initial eventless (`always`) transitions first:
     ```rust
     let _ = self.transition_internal("", &init_event, guards, actions, 0, None);
     ```
     (`transition_internal` automatically chains consecutive `always` transitions, exits transient states and removes their `active_timers`, executes transition/entry actions, and schedules `after` timers on entered target states.)
   - Drain any internal events raised during initial entry or initial `always` transitions:
     ```rust
     let _ = self.drain_internal_queue(guards, actions);
     ```
     (In `drain_internal_queue`, each raised event invokes `self.transition_internal(e_type, e_payload, guards, actions, 0, None)`, which itself settles any subsequent `always` transitions triggered by that raised event.)
   - Complete the initialization macrostep and drain any external events sent reentrantly during `start`:
     ```rust
     self.is_processing = false;
     let _ = self.drain_external_queue(guards, actions);
     ```

### 4.2 C/C++ FFI `fsm_start` and `fsm_send_event` (`portable-xstate-ffi`)

In [`portable-xstate-ffi/src/lib.rs`](file:///Users/mike/GitHub/portable-xstate/portable-xstate-ffi/src/lib.rs#L84-L161):

1. **`fsm_start`**:
   - Call `ctx.machine.start(&ctx.guards, &ctx.actions);` and return `0`.
2. **`fsm_send_event`**:
   - Construct an event object `{ "type": "<event>" }` instead of `Value::Null`:
     ```rust
     let mut event_map = IndexMap::new();
     event_map.insert(alloc::string::String::from("type"), Value::String(alloc::string::String::from(event)));
     let payload = Value::Object(event_map);
     ctx.machine.transition(event, &payload, &ctx.guards, &ctx.actions);
     ```
   - This makes `fsm_value_get_property(event, "type")` / `event.getProperty("type")` work consistently in C/C++ action and guard callbacks for both `xstate.init` (during `fsm_start`) and named events (during `fsm_send_event`).

### 4.3 WASM `StateNode::create_actor_method`, `Actor::start`, and `Actor::get_snapshot` (`portable-xstate-wasm`)

In [`portable-xstate-wasm/src/lib.rs`](file:///Users/mike/GitHub/portable-xstate/portable-xstate-wasm/src/lib.rs):

1. **Remove Premature `runner.handle_always_transition()` from `StateNode::create_actor_method` (lines 640–644)**:
   - Do not mutate `runner.machine` or fire JS side-effect actions inside `create_actor_method`.
2. **Use `RuntimeMachine::start` in `Actor::start` (lines 1400–1417)**:
   - When `!self.is_initialized`:
     ```rust
     self.machine.start(&self.guards, &self.actions);
     self.is_initialized = true;
     ```
   - When `self.is_initialized` is `true` (i.e. restored from a persisted snapshot):
     ```rust
     self.machine.is_started = true;
     self.machine.revive_timers();
     self.handle_always_transition();
     ```
3. **Pre-Start Snapshot Preview in `Actor::get_snapshot` (lines 1311–1341)**:
   - In XState v5, calling `createActor(machine).getSnapshot()` *before* `.start()` returns the resolved initial snapshot (after following initial `always` transitions and static assignments, without running external side-effect callbacks).
   - In `Actor::get_snapshot(&self)`, if `!self.machine.is_started && !self.is_initialized`:
     - Clone `self.machine` into a temporary `preview_machine` and call `preview_machine.start(&self.guards, &IndexMap::new())`.
     - Build the returned `Snapshot` from `preview_machine` while leaving `self.machine` unstarted so a subsequent `.start()` executes all initial `entry`, transition, and `exit` actions once in the exact XState v5 order.

### 4.4 `portable-xstate-platformio` Upstream Sync, `library.json`, and `build_rust.py`

In `/Users/mike/GitHub/portable-xstate-platformio`:

1. **`library.json`**:
   - Change `"frameworks": ["arduino", "espidf"]` to `"frameworks": ["*"]` so PlatformIO's LDF includes `portable-xstate-platformio` in both ESP32 (`arduino` / `espidf`) and host `platform = native` (frameworkless) environments.
2. **`build_rust.py`**:
   - Guard SCons `Import("env")` and top-level SCons hook registration so importing `build_rust` in standard Python (`python3 -m unittest test_build_rust.py`) succeeds without a live SCons runtime.
   - Extract clean, testable helper functions:
     - `resolve_target(platform, mcu)` returning `(rust_target, cargo_To_use, is_riscv, is_native)` (using `rustc -vV` host triple when `platform == "native"` or `mcu == ""`, and mapping `esp32`, `esp32s2`, `esp32s3`, `esp32c3`, `esp32c6`, `esp32h2` for ESP32 targets).
     - `get_lib_release_dir(rust_dir, rust_target, is_native)` returning `<rust_dir>/target/release` for native builds and `<rust_dir>/target/<rust_target>/release` for cross-compiled ESP32 builds.
     - `build_cargo_cmd(cargo_to_use, rust_target, is_riscv, is_native)` omitting `--target` and `-Z build-std=core,alloc` when `is_native` is `True`.
   - In `clone_dependency`, add a `git fetch --all` fallback before `git checkout <DEPENDENCY_REF>` if the pinned commit is not in the initial shallow fetch.
   - Update `DEPENDENCY_REF` to the new `portable-xstate` commit SHA containing the XState v5 initialization fixes.
3. **`test_build_rust.py`**:
   - Import `build_rust` directly (rather than duplicating logic in the test file) and test:
     - ESP32 Xtensa (`esp32`, `esp32s2`, `esp32s3`) and RISC-V (`esp32c3`, `esp32c6`, `esp32h2`) target resolution and Cargo command construction.
     - Host `platform = native` target resolution, release directory path, and Cargo command construction.
     - End-to-end `build_rust_lib` invocation for both `native` (no `BoardConfig()`) and `espressif32` environments.
     - `library.json` framework and platform configuration.

---

## 5. What Is Left to the Builder

- Exact helper structure inside `RuntimeMachine::start` for walking initial state paths and collecting `entry`, `initial_actions`, and `history_actions`.
- Organization of new conformance tests across `portable-xstate-ffi/src/lib.rs`, `portable-xstate-ffi/tests/c_test.c`, `portable-xstate-ffi/tests/cpp_test.cpp`, and `tests/machine.test.ts`.
