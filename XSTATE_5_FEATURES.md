# XState 5 Supported Operations Checklist

This document tracks the feature set of `lavi` against the XState 5 (`statelyai/xstate@5.25.0`) specification across all three targets:
- **`Rust`**: `#![no_std]` core engine (`lavi`)
- **`WASM`**: WebAssembly / JavaScript bindings (`lavi-wasm`)
- **`C/C++`**: Embedded C99 (`lavi.h`) and C++11 (`lavi.hpp`) bindings (`lavi-c-cpp`), also backing `lavi-swift`, `lavi-dart`, and `lavi-csharp`

**Automated Sync Invariant (`npm run test:conformance-check`)**:
- The **Conformance Test Suites** table below is verified automatically on every `npm test` run against [`tests/conformance-manifest.json`](./tests/conformance-manifest.json) and the actual `it.skip` / `describe.skip` counts in `tests/*.test.ts`.
- A top-level feature line linking to an upstream test suite (`packages/core/test/<suite>.test.ts`) may only be checked `- [x]` when `tests/<suite>.test.ts` is onboarded in `tests/conformance-manifest.json` with `"status": "strict"` and `0` skipped blocks.
- Every `fsm_*` C API symbol referenced in this document is verified against [`lavi-c-cpp/include/lavi.h`](./lavi-c-cpp/include/lavi.h).

---

## Conformance Test Suites

### Onboarded Suites (`tests/*.test.ts`)

| Suite | Provenance Status | Skipped Blocks (`it.skip` / `describe.skip`) | Upstream Reference |
| :--- | :--- | :---: | :--- |
| `actions.test.ts` | `strict` | 2 | [`tests/upstream/actions.test.ts`](./tests/upstream/actions.test.ts) |
| `after.test.ts` | `strict` | 3 | [`tests/upstream/after.test.ts`](./tests/upstream/after.test.ts) |
| `assign.test.ts` | `strict` | 0 | [`tests/upstream/assign.test.ts`](./tests/upstream/assign.test.ts) |
| `eventDescriptors.test.ts` | `strict` | 0 | [`tests/upstream/eventDescriptors.test.ts`](./tests/upstream/eventDescriptors.test.ts) |
| `final.test.ts` | `strict` | 0 | [`tests/upstream/final.test.ts`](./tests/upstream/final.test.ts) |
| `guards.test.ts` | `strict` | 0 | [`tests/upstream/guards.test.ts`](./tests/upstream/guards.test.ts) |
| `history.test.ts` | `strict` | 0 | [`tests/upstream/history.test.ts`](./tests/upstream/history.test.ts) |
| `input.test.ts` | `strict` | 0 | [`tests/upstream/input.test.ts`](./tests/upstream/input.test.ts) |
| `inspect.test.ts` | `strict` | 0 | [`tests/upstream/inspect.test.ts`](./tests/upstream/inspect.test.ts) |
| `json.test.ts` | `strict` | 0 | [`tests/upstream/json.test.ts`](./tests/upstream/json.test.ts) |
| `machine.test.ts` | `strict` | 1 | [`tests/upstream/machine.test.ts`](./tests/upstream/machine.test.ts) |
| `meta.test.ts` | `strict` | 0 | [`tests/upstream/meta.test.ts`](./tests/upstream/meta.test.ts) |
| `parallel.test.ts` | `strict` | 0 | [`tests/upstream/parallel.test.ts`](./tests/upstream/parallel.test.ts) |
| `state.test.ts` | `strict` | 0 | [`tests/upstream/state.test.ts`](./tests/upstream/state.test.ts) |
| `tags.test.ts` | `strict` | 0 | [`tests/upstream/tags.test.ts`](./tests/upstream/tags.test.ts) |
| `transition.test.ts` | `strict` | 0 | [`tests/upstream/transition.test.ts`](./tests/upstream/transition.test.ts) |

### Upstream Suites Not Yet Onboarded
`invoke.test.ts`, `spawn.test.ts`, `actor.test.ts`.

---

## 1. Core Structure (Essential)

- [x] **Machine Definition** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/machine.test.ts) — 14/14 active tests passing in `tests/machine.test.ts`, 1 upstream-skipped XState v4 test)
  - [x] `id` (Root machine identifier) `[Rust | WASM | C/C++]`
  - [x] `initial` (Initial state key & initial transition actions) `[Rust | WASM | C/C++]`
  - [x] `context` (Initial extended state object) `[Rust | WASM | C/C++]`
  - [x] `states` (Iterative state node hierarchy parsing) `[Rust | WASM | C/C++]`
  - [x] `version` (Machine versioning on `StateNode`) `[WASM]`
  - [x] `types` / `schemas` (TypeScript type schemas pass-through) `[Rust | WASM]`
- [x] **State Nodes** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/state.test.ts) — 21/21 passing in `tests/state.test.ts`)
  - [x] Atomic States (Leaf nodes) `[Rust | WASM | C/C++]`
  - [x] Compound States (Nested states with `initial`) `[Rust | WASM | C/C++]`
  - [x] Parallel States (Orthogonal regions) `[Rust | WASM | C/C++]` ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/parallel.test.ts) — 28/28 passing in `tests/parallel.test.ts`)
  - [x] Final States (`type: 'final'`) `[Rust | WASM | C/C++]` ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/final.test.ts) — 33/33 passing in `tests/final.test.ts`, 0 skips)
  - [x] **History States** (`type: 'history'`, `history: 'shallow' | 'deep'`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/history.test.ts) — 33/33 passing in `tests/history.test.ts`)
    - [x] Shallow history (default) `[Rust | WASM | C/C++]`
    - [x] Deep history (`history: 'deep'`) `[Rust | WASM | C/C++]`
    - [x] History with default `target` `[Rust | WASM | C/C++]`
    - [x] History in parallel states `[Rust | WASM | C/C++]`
    - [x] Multistage history `[Rust | WASM | C/C++]`
    - [x] History state persistence/revival (`historyValue`) `[Rust | WASM | C/C++]`
- [x] **Transitions** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/transition.test.ts) — 15/15 passing in `tests/transition.test.ts`, 0 skips)
  - [x] Event Transitions (`on: { EVENT: ... }`) `[Rust | WASM | C/C++]`
    - **Documented Memory Constraint (Pre-Start Event Dropping)**: Unlike XState 5 in JS (which buffers `actor.send()` calls before `actor.start()` in an unbounded heap array), `lavi` intentionally drops events sent before `start()` to prevent hardware interrupts or sensor polling before initialization from exhausting microcontroller RAM.
  - [x] Eventless Transitions (`always: [...]`) `[Rust | WASM | C/C++]`
  - [x] Targetless Transitions (internal transitions with actions only) `[Rust | WASM | C/C++]`
  - [x] Self-transitions with `reenter: true` `[Rust | WASM | C/C++]`
  - [x] Guarded Transitions (`guard`) `[Rust | WASM | C/C++]`
  - [x] Root-level transitions (combinatorial machines with empty `states`) `[Rust | WASM | C/C++]`
  - [ ] Delayed Transitions (`after: { delay: target }`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/after.test.ts) — 7/10 passing in `tests/after.test.ts`, strict conformance, 3 skips)
    - [x] Numeric millisecond delays (`after: { 1000: 'target' }`) `[Rust | WASM | C/C++]`
    - [x] Deterministic virtual time advancement (`SimulatedClock` with `set(ms)` / `increment(ms)` and cascading `tick()` / `fsm_tick` / `fsm_simulated_clock_set` / `fsm_simulated_clock_increment`) `[Rust | WASM | C/C++]`
    - [x] Deep-sleep deadline introspection (`next_timer_deadline` / `getNextDeadline` / `fsm_get_next_deadline`) `[Rust | WASM | C/C++]`
    - [x] Named delays and dynamic delay registration (`options.delays`, `fsm_register_delay`, `registerDelay`, `RuntimeMachine::with_delays`) `[Rust | WASM | C/C++]`
    - **Documented Target-Language Constraints**:
      - Dynamic delay expressions in C/C++/Rust use registered named delay callbacks (`fsm_register_delay`, `registerDelay`, `with_delays`) rather than runtime string/expression evaluation; WASM bridges inline JS delay functions to synthetic named callbacks.
      - Host JS `setTimeout`/`clearTimeout` handle inspection (`should not try to clear an undefined timeout when exiting source state of a delayed transition`) and internal JS `TransitionNode` string formatting (`should format transitions properly`) are skipped by design because `lavi` evaluates timers against an external clock deadline rather than spawning host JS timers.
  - [x] Wildcard Transitions (`*`) `[Rust | WASM | C/C++]` ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/eventDescriptors.test.ts) — 14/14 passing in `tests/eventDescriptors.test.ts`)
  - [x] Partial Wildcard Transitions (`models.*`) `[Rust | WASM | C/C++]` ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/eventDescriptors.test.ts) — 14/14 passing in `tests/eventDescriptors.test.ts`, including `assertEvent` partial descriptor validation in WASM)
  - [x] Pure Transition APIs (`initialTransition(logic, options)`, `transition(logic, state, event)` in WASM, `RuntimeMachine::initial_transition_pure_json` / `transition_pure_json` in Rust, `fsm_initial_transition_json` / `fsm_transition_pure_json` in `lavi.h`, `initialTransitionJson` / `transitionPureJson` in `lavi.hpp`, `initial_transition` / `transition_pure` in Python, `initialTransition` / `transitionPure` in Java/Kotlin/Swift/Dart, and `InitialTransition` / `TransitionPure` in C#) `[Rust | WASM | C/C++]`

---

## 2. Logic & Control Flow (High Priority)

- [x] **Guards** (`guard`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/guards.test.ts) — 46/46 passing in `tests/guards.test.ts`, strict conformance, 0 skips)
  - [x] Named Guards (`fsm_register_guard` in C, `registerGuard` in C++, `guards` map in Rust, `setup`/`provide` in WASM) `[Rust | WASM | C/C++]`
  - [x] Inline Guards (`js_sys::Function` in WASM/JS bridged to synthetic named guards) `[WASM]`
  - [x] Guard override via `machine.provide()` `[WASM]`
  - [x] Parameterized Guards (`params` passed to guard callbacks) `[Rust | WASM | C/C++]` (`fsm_register_guard_with_params` in C, `registerGuardWithParams` in C++)
  - [x] Higher-Order Guards (`and`, `or`, `not`, `stateIn`) `[Rust | WASM | C/C++]`
- [ ] **Actions** (`entry`, `exit`, `actions`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/actions.test.ts) — 133/136 passing in `tests/actions.test.ts`, strict conformance, 0 feature skips, 2 skips [1 upstream skip + 1 non-js-constraint], 1 todo [upstream todo])
  - [x] Named Actions (`fsm_register_action` in C, `registerAction` in C++, `actions` map in Rust, `setup`/`provide` in WASM) `[Rust | WASM | C/C++]`
  - [x] Inline Actions (`js_sys::Function` in WASM/JS bridged to synthetic named actions) `[WASM]`
  - [x] Action override via `machine.provide()` `[WASM]`
  - [x] Parameterized Actions (`params` passed to action callbacks across `fsm_register_action_with_params` in `lavi.h`, `registerActionWithParams` in `lavi.hpp`, `actions` map in Rust, and static + dynamic `params: ({ context, event }) => ...` resolver bridging in WASM) `[Rust | WASM | C/C++]`
  - [x] Entry actions (state entry, including root initial `always` transition batching) `[Rust | WASM | C/C++]`
  - [x] Exit actions (state exit in reverse document order) `[Rust | WASM | C/C++]`
  - [x] Transition actions `[Rust | WASM | C/C++]`
  - [x] Initial transition actions (`initial: { target, actions }`) `[Rust | WASM | C/C++]`
  - [x] **Built-in Actions:**
    - [x] `assign` (Update context) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/assign.test.ts) — 14/14 passing in `tests/assign.test.ts`)
      - [x] Declarative object-based static assignment (`{ type: 'xstate.assign', assignment: { ... } }`) `[Rust | WASM | C/C++]`
      - [x] Function-based, inline property, and parameterized (`params`) assignment functions (`assign(({ context, event }, params) => ...)` bridged via callbacks) `[Rust | WASM]` (C/C++ `fsm_action_callback_t` currently receives `const fsm_value_t* context` read-only)
      - [x] Assignment in entry/exit/transition actions `[Rust | WASM | C/C++]`
    - [x] `raise` (Self-send internal event queued in `internal_queue`, including delayed raise params) `[Rust | WASM | C/C++]`
    - [x] `log` (Logging action) `[Rust | WASM]`
    - [x] `cancel` (Delayed event cancellation) `[Rust | WASM]`
    - [x] `enqueueActions` (Batch action enqueueing with context assignment, guard `check`, `setup`/`provide` registration, static & dynamic `params`, and nested action delegation) `[Rust | WASM]`
    - [x] `sendParent` (Event relay from child actor to parent) `[WASM]`
    - [ ] `sendTo` (Event dispatch to named actor or ActorRef; verified in `tests/actions.test.ts`) `[WASM]` ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/actor.test.ts))
    - [x] `stopChild` (Child actor termination action creator and dynamic stop in actions/enqueueActions) `[WASM]`
    - [x] `emit` (Event emitter action capture in pure transitions) `[Rust | WASM]`
    - **Documented Target-Language Constraints (Actions)**:
      - Dev-mode warning snapshot assertions (`should warn if called in custom action`) are skipped by design because capturing and snapshotting development console warnings for illegal imperative action calls in custom action functions is host-specific JavaScript runtime behavior with no compiled or microcontroller equivalent.
- [x] **Context & Input** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/input.test.ts) — 14/14 passing in `tests/input.test.ts`, 0 skips)
  - [x] Context initialization from JSON object `[Rust | WASM | C/C++]`
  - [x] Function-based lazy context initialization (`context: () => ({ ... })`) `[WASM]`
  - [x] Context property inspection in C/C++ callbacks (`fsm_value_is_object`, `fsm_value_is_string`, `fsm_value_is_number`, `fsm_value_is_bool`, `fsm_value_get_string`, `fsm_value_get_number`, `fsm_value_get_bool`, `fsm_value_get_property`) `[C/C++]`
  - [x] `input` (Factory-pattern context initialization `context: ({ input }) => ...`, initial event `event.input`, missing input error snapshot status, `fsm_create_with_input` in `lavi.h`, and `lavi::Machine(json, input)` in `lavi.hpp`) `[Rust | WASM | C/C++]`


---

## 3. State Inspection & Queries (High Priority)

- [ ] **State / Snapshot API** (Snapshot persistence core and C/C++ FFI resolved in `TD-8` in [`TECH_DEBT.md`](./TECH_DEBT.md))
  - [x] `state.matches(stateValue)` (Dot-delimited path matching across `RuntimeMachine::matches`, `Snapshot::matches`, `fsm_matches`, and `lavi::Machine::matches`; partial object matching in Rust & WASM) `[Rust | WASM | C/C++]`
  - [x] `state.value` (`RuntimeMachine::current_state` in Rust, `snapshot.value` in WASM) `[Rust | WASM]`
  - [x] `state.context` (`RuntimeMachine::context` in Rust, `snapshot.context` in WASM) `[Rust | WASM]`
  - [x] `state.status` (`'active' | 'done' | 'stopped'` in WASM, `RuntimeMachine::status()` / `is_done()` / `stop()` in Rust, `fsm_is_done` / `fsm_stop` in C, and `lavi::Machine::isDone()` / `stop()` in C++) `[Rust | WASM | C/C++]`
  - [x] `state.can(event)` (Pure transition check with guards across `RuntimeMachine::can` / `can_event`, `Snapshot::can`, `fsm_can`, and `lavi::Machine::can`) `[Rust | WASM | C/C++]`
  - [x] `state.hasTag(tag)` (Active state hierarchy tag check across `RuntimeMachine::has_tag`, `Snapshot::hasTag`, `fsm_has_tag`, and `lavi::Machine::hasTag`) `[Rust | WASM | C/C++]`
- [x] **Machine / StateNode API**
  - [x] `machine.states` (StateNode tree registry) `[WASM]`
  - [x] `machine.getStateNodeById()` (Resolve child StateNode by string ID) `[WASM]`
  - [x] `machine.events` (Accepted events list in definition order) `[WASM]`
  - [x] `machine.config` (Original machine config reference) `[WASM]`
  - [x] `machine.provide()` (Override guards/actions/context) `[WASM]`
  - [x] `machine.resolveState()` (Resolve state value and `done` status) `[WASM]`

---

## 4. Actor Model (Medium Priority)

- [x] **Invoking Actors** (`invoke`) (Lifecycle in `lavi-wasm` & host lifecycle callbacks across all targets; TD-7 & TD-21 resolved)
  - [x] Invoke Promise (`fromPromise`) `[WASM]`
  - [x] Invoke Callback (`fromCallback`) `[WASM]`
  - [x] Invoke Observable (`fromObservable`, `fromEventObservable`) `[WASM]`
  - [x] Invoke Machine (`fromTransition` / child machine) `[WASM]`
  - [x] `onDone` / `onError` (Transition on actor completion/failure) `[WASM]`
  - [x] Host Invoke Start/Stop Lifecycle Callbacks (`RuntimeMachine::register_invoke_callbacks` in Rust, `fsm_register_invoke_callbacks` in `lavi.h`, `registerInvokeCallbacks` in `lavi.hpp`, `register_invoke_callbacks` in Python, `registerInvokeCallbacks` / `onInvoke` in Java/Kotlin, `registerInvokeCallbacks` in Swift/Dart, and `RegisterInvokeCallbacks` in C#) `[Rust | WASM | C/C++]`
- [x] **Spawning Actors** (`spawn`)
  - [x] Dynamic child actor creation via `assign` / `spawnChild` `[WASM]`

---

## 5. Advanced & Niche (Low Priority)

- [x] **Deep Persistence** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/json.test.ts) — 3/3 passing in `tests/json.test.ts`)
  - [x] Persisting/Restoring state snapshots (`getPersistedSnapshot()`, `createActor(machine, { snapshot })`, `RuntimeMachine::get_snapshot_json`/`restore_from_snapshot_json`, `fsm_get_snapshot_json`/`fsm_restore_snapshot_json`, `lavi::Machine::getSnapshot`/`restoreSnapshot`) `[Rust | WASM | C/C++]`
  - [x] History value persistence and `StateNode` reference revival `[Rust | WASM | C/C++]`
  - [x] Machine definition schema conformity and serialization (`machine.definition`, `JSON.stringify(machine.definition)`) `[Rust | WASM]`
- [x] **Inspection & Telemetry** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/inspect.test.ts) — 12/12 passing in `tests/inspect.test.ts`, strict conformance, 0 skips)
  - [x] `inspect` callback & observer (`createActor(machine, { inspect })`, `actor.system.inspect(fnOrObserver)`, `RuntimeMachine::register_inspector`, `fsm_register_inspector` in `lavi.h`, `registerInspector` in `lavi.hpp`, `register_inspector` / `@machine.inspector()` in Python, `registerInspector` / `onInspect` in Java/Kotlin, `registerInspector` in Swift, and `registerInspector` in Dart) `[Rust | WASM | C/C++]`
  - [x] Full `@xstate.actor`, `@xstate.event`, `@xstate.action`, `@xstate.microstep`, and `@xstate.snapshot` inspection protocol (including `isMachineSnapshot` type guard) `[Rust | WASM | C/C++]`
- [x] **Tags** (`tags: [...]`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/tags.test.ts) — 6/6 passing in `tests/tags.test.ts`, 0 skips)
  - [x] Tag definition on atomic, compound, parallel, and root machine states `[Rust | WASM | C/C++]`
  - [x] Tag checking via `snapshot.hasTag(tag)` / `RuntimeMachine::has_tag` / `fsm_has_tag` / `lavi::Machine::hasTag` `[Rust | WASM | C/C++]`
  - [x] Active tag set inspection via `snapshot.tags` getter returning `Set<string>` in WASM `[WASM]`
  - [x] Tag persistence across state restoration `[Rust | WASM | C/C++]`
- [x] **Metadata & Descriptions** (`meta: { ... }`, `description`, `output`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/meta.test.ts) — 16/16 passing in `tests/meta.test.ts`, 0 skips)
  - [x] State and machine metadata definition and aggregation via `snapshot.getMeta()` in WASM, `RuntimeMachine::get_meta()` / `get_state_meta()` in Rust, `fsm_get_meta_json` / `fsm_get_state_meta_json` in `lavi.h`, and `get_meta_json()` / `get_state_meta_json()` in `lavi.hpp` `[Rust | WASM | C/C++]`
  - [x] State and transition descriptions (`stateNode.description`, `stateNode.root.on.<event>[0].description`) `[Rust | WASM]`
  - [x] State and machine output definition and dynamic output resolvers (`StateDefinition.output`, `MachineDefinition.output`, `snapshot.output` in WASM, `RuntimeMachine::get_output()` / `get_final_leaf_output()` / `register_output_resolver()` in Rust, `fsm_get_output_json` / `fsm_get_final_leaf_output_json` / `fsm_register_output_resolver` in `lavi.h`, `get_output()` / `getFinalLeafOutput()` / `registerOutputResolver()` in `lavi.hpp`, `get_output` / `get_final_leaf_output` / `register_output_resolver` / `@machine.output_resolver` in Python, `getOutput` / `getFinalLeafOutput` / `registerOutputResolver` / `onOutputResolver` in Java/Kotlin, `getOutput` / `getFinalLeafOutput` / `registerOutputResolver` in Swift/Dart, and `GetOutput` / `GetFinalLeafOutput` / `RegisterOutputResolver` in C#) `[Rust | WASM | C/C++]`
- [x] **Custom Clocks & Deterministic Virtual Time** (`SimulatedClock`, `register_clock`, `fsm_register_clock`, `fsm_register_clock_with_data`, `fsm_register_simulated_clock`, `fsm_simulated_clock_create`, `fsm_simulated_clock_free`, `fsm_simulated_clock_now`, `fsm_simulated_clock_set`, `fsm_simulated_clock_increment`) `[Rust | WASM | C/C++]`

