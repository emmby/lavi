# XState 5 Supported Operations Checklist

This document tracks the feature set of `portable-xstate` against the XState 5 (`statelyai/xstate@5.25.0`) specification across all three targets:
- **`Rust`**: `#![no_std]` core engine (`portable-xstate-rust`)
- **`WASM`**: WebAssembly / JavaScript bindings (`portable-xstate-wasm`)
- **`C/C++`**: Embedded C99 (`portable_xstate.h`) and C++11 (`portable_xstate.hpp`) bindings (`portable-xstate-c-cpp`)

**Automated Sync Invariant (`npm run test:conformance-check`)**:
- The **Conformance Test Suites** table below is verified automatically on every `npm test` run against [`tests/conformance-manifest.json`](./tests/conformance-manifest.json) and the actual `it.skip` / `describe.skip` counts in `tests/*.test.ts`.
- A top-level feature line linking to an upstream test suite (`packages/core/test/<suite>.test.ts`) may only be checked `- [x]` when `tests/<suite>.test.ts` is onboarded in `tests/conformance-manifest.json` with `"status": "strict"` and `0` skipped blocks.
- Every `fsm_*` C API symbol referenced in this document is verified against [`portable-xstate-c-cpp/include/portable_xstate.h`](./portable-xstate-c-cpp/include/portable_xstate.h).

---

## Conformance Test Suites

### Onboarded Suites (`tests/*.test.ts`)

| Suite | Provenance Status | Skipped Blocks (`it.skip` / `describe.skip`) | Upstream Reference |
| :--- | :--- | :---: | :--- |
| `actions.test.ts` | `strict` | 55 | [`tests/upstream/actions.test.ts`](./tests/upstream/actions.test.ts) |
| `after.test.ts` | `strict` | 5 | [`tests/upstream/after.test.ts`](./tests/upstream/after.test.ts) |
| `assign.test.ts` | `strict` | 0 | [`tests/upstream/assign.test.ts`](./tests/upstream/assign.test.ts) |
| `eventDescriptors.test.ts` | `strict` | 0 | [`tests/upstream/eventDescriptors.test.ts`](./tests/upstream/eventDescriptors.test.ts) |
| `history.test.ts` | `strict` | 0 | [`tests/upstream/history.test.ts`](./tests/upstream/history.test.ts) |
| `machine.test.ts` | `strict` | 1 | [`tests/upstream/machine.test.ts`](./tests/upstream/machine.test.ts) |
| `state.test.ts` | `strict` | 1 | [`tests/upstream/state.test.ts`](./tests/upstream/state.test.ts) |

### Upstream Suites Not Yet Onboarded
`transition.test.ts`, `parallel.test.ts`, `final.test.ts`, `guards.test.ts`, `input.test.ts`, `invoke.test.ts`, `spawn.test.ts`, `actor.test.ts`, `json.test.ts`, `inspect.test.ts`, `meta.test.ts`, `tags.test.ts`.

---

## 1. Core Structure (Essential)

- [x] **Machine Definition** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/machine.test.ts) — 14/14 active tests passing in `tests/machine.test.ts`, 1 upstream-skipped XState v4 test)
  - [x] `id` (Root machine identifier) `[Rust | WASM | C/C++]`
  - [x] `initial` (Initial state key & initial transition actions) `[Rust | WASM | C/C++]`
  - [x] `context` (Initial extended state object) `[Rust | WASM | C/C++]`
  - [x] `states` (Iterative state node hierarchy parsing) `[Rust | WASM | C/C++]`
  - [x] `version` (Machine versioning on `StateNode`) `[WASM]`
  - [x] `types` / `schemas` (TypeScript type schemas pass-through) `[Rust | WASM]`
- [ ] **State Nodes** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/state.test.ts) — 20/21 passing in `tests/state.test.ts`, 1 `spawn` test skipped)
  - [x] Atomic States (Leaf nodes) `[Rust | WASM | C/C++]`
  - [x] Compound States (Nested states with `initial`) `[Rust | WASM | C/C++]`
  - [ ] Parallel States (Orthogonal regions) `[Rust | WASM | C/C++]` ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/parallel.test.ts) — core support implemented; upstream `parallel.test.ts` not yet onboarded)
  - [ ] Final States (`type: 'final'`) `[Rust | WASM | C/C++]` ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/final.test.ts) — parsed in Rust and sets `MachineStatus::Done` / `is_done()` / `status: 'done'` / `fsm_is_done`; `onDone` transitions and upstream `final.test.ts` not yet implemented)
  - [x] **History States** (`type: 'history'`, `history: 'shallow' | 'deep'`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/history.test.ts) — 33/33 passing in `tests/history.test.ts`)
    - [x] Shallow history (default) `[Rust | WASM | C/C++]`
    - [x] Deep history (`history: 'deep'`) `[Rust | WASM | C/C++]`
    - [x] History with default `target` `[Rust | WASM | C/C++]`
    - [x] History in parallel states `[Rust | WASM | C/C++]`
    - [x] Multistage history `[Rust | WASM | C/C++]`
    - [x] History state persistence/revival (`historyValue`) `[Rust | WASM | C/C++]`
- [ ] **Transitions** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/transition.test.ts) — upstream `transition.test.ts` not yet onboarded)
  - [x] Event Transitions (`on: { EVENT: ... }`) `[Rust | WASM | C/C++]`
  - [x] Eventless Transitions (`always: [...]`) `[Rust | WASM | C/C++]`
  - [x] Targetless Transitions (internal transitions with actions only) `[Rust | WASM | C/C++]`
  - [x] Self-transitions with `reenter: true` `[Rust | WASM | C/C++]` (Partial — deep descendant reentry edge cases in `actions.test.ts` skipped)
  - [x] Guarded Transitions (`guard`) `[Rust | WASM | C/C++]`
  - [x] Root-level transitions (combinatorial machines with empty `states`) `[Rust | WASM | C/C++]`
  - [ ] Delayed Transitions (`after: { delay: target }`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/after.test.ts) — 5/10 passing in `tests/after.test.ts`, strict conformance, pending dynamic delay callbacks)
    - [x] Numeric millisecond delays (`after: { 1000: 'target' }`) `[Rust | WASM | C/C++]`
    - [x] Deterministic virtual time advancement (`SimulatedClock` with `set(ms)` / `increment(ms)` and cascading `tick()` / `fsm_tick` / `fsm_simulated_clock_set` / `fsm_simulated_clock_increment`) `[Rust | WASM | C/C++]`
    - [x] Deep-sleep deadline introspection (`next_timer_deadline` / `getNextDeadline` / `fsm_get_next_deadline`) `[Rust | WASM | C/C++]`
    - [ ] Named delays (`delays: { myDelay: ... }`) and dynamic delay callbacks `[ ]`
    - **Documented Target-Language Constraints**:
      - Dynamic delay expressions in C/C++/Rust must use registered named delay callbacks rather than runtime string/expression evaluation (WASM can bridge inline JS delay functions to synthetic named callbacks; resolved in `TD-1` in [`TECH_DEBT.md`](./TECH_DEBT.md)).
      - Host JS `setTimeout`/`clearTimeout` handle inspection (`should not try to clear an undefined timeout when exiting source state of a delayed transition`) and internal JS `TransitionNode` string formatting (`should format transitions properly`) are skipped by design because `portable-xstate` evaluates timers against an external clock deadline rather than spawning host JS timers.
  - [x] Wildcard Transitions (`*`) `[Rust | WASM | C/C++]` ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/eventDescriptors.test.ts) — 14/14 passing in `tests/eventDescriptors.test.ts`)
  - [x] Partial Wildcard Transitions (`models.*`) `[Rust | WASM | C/C++]` ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/eventDescriptors.test.ts) — 14/14 passing in `tests/eventDescriptors.test.ts`, including `assertEvent` partial descriptor validation in WASM)

---

## 2. Logic & Control Flow (High Priority)

- [ ] **Guards** (`guard`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/guards.test.ts) — upstream `guards.test.ts` not yet onboarded)
  - [x] Named Guards (`fsm_register_guard` in C, `registerGuard` in C++, `guards` map in Rust, `setup`/`provide` in WASM) `[Rust | WASM | C/C++]`
  - [x] Inline Guards (`js_sys::Function` in WASM/JS bridged to synthetic named guards) `[WASM]`
  - [x] Guard override via `machine.provide()` `[WASM]`
  - [x] Parameterized Guards (`params` passed to guard callbacks) `[Rust | WASM]` (not yet exposed in `fsm_guard_callback_t` in C/C++)
  - [ ] Higher-Order Guards (`and`, `or`, `not`, `stateIn`)
- [ ] **Actions** (`entry`, `exit`, `actions`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/actions.test.ts) — 80/136 passing in `tests/actions.test.ts`, strict conformance, resolved in `TD-2`)
  - [x] Named Actions (`fsm_register_action` in C, `registerAction` in C++, `actions` map in Rust, `setup`/`provide` in WASM) `[Rust | WASM | C/C++]`
  - [x] Inline Actions (`js_sys::Function` in WASM/JS bridged to synthetic named actions) `[WASM]`
  - [x] Action override via `machine.provide()` `[WASM]`
  - [x] Static Action Parameters (`params` object passed to action callbacks) `[Rust | WASM]` (dynamic parameter resolver functions skipped; `params` not yet exposed in `fsm_action_callback_t` in C/C++)
  - [x] Entry actions (state entry, including root initial `always` transition batching) `[Rust | WASM | C/C++]`
  - [x] Exit actions (state exit in reverse document order) `[Rust | WASM | C/C++]`
  - [x] Transition actions `[Rust | WASM | C/C++]`
  - [x] Initial transition actions (`initial: { target, actions }`) `[Rust | WASM | C/C++]`
  - [ ] **Built-in Actions:**
    - [x] `assign` (Update context) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/assign.test.ts) — 14/14 passing in `tests/assign.test.ts`)
      - [x] Declarative object-based static assignment (`{ type: 'xstate.assign', assignment: { ... } }`) `[Rust | WASM | C/C++]`
      - [x] Function-based, inline property, and parameterized (`params`) assignment functions (`assign(({ context, event }, params) => ...)` bridged via callbacks) `[Rust | WASM]` (C/C++ `fsm_action_callback_t` currently receives `const fsm_value_t* context` read-only)
      - [x] Assignment in entry/exit/transition actions `[Rust | WASM | C/C++]`
    - [x] `raise` (Self-send internal event queued in `internal_queue`) `[Rust | WASM | C/C++]`
    - [ ] `log` (Logging action)
    - [ ] `sendTo` / `sendParent` ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/actor.test.ts))
    - [ ] `stopChild` / `cancel`
    - [ ] `emit` (Event emitter action)
- [ ] **Context & Input** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/input.test.ts) — upstream `input.test.ts` not yet onboarded)
  - [x] Context initialization from JSON object `[Rust | WASM | C/C++]`
  - [x] Function-based lazy context initialization (`context: () => ({ ... })`) `[WASM]`
  - [x] Context property inspection in C/C++ callbacks (`fsm_value_is_object`, `fsm_value_is_string`, `fsm_value_is_number`, `fsm_value_is_bool`, `fsm_value_get_string`, `fsm_value_get_number`, `fsm_value_get_bool`, `fsm_value_get_property`) `[C/C++]`
  - [ ] `input` (Factory-pattern context initialization `context: ({ input }) => ...`)

---

## 3. State Inspection & Queries (High Priority)

- [ ] **State / Snapshot API** (Snapshot persistence core and C/C++ FFI resolved in `TD-8` in [`TECH_DEBT.md`](./TECH_DEBT.md))
  - [x] `state.matches(stateValue)` (Dot-delimited path matching across `RuntimeMachine::matches`, `Snapshot::matches`, `fsm_matches`, and `PortableXState::matches`; partial object matching in Rust & WASM) `[Rust | WASM | C/C++]`
  - [x] `state.value` (`RuntimeMachine::current_state` in Rust, `snapshot.value` in WASM) `[Rust | WASM]`
  - [x] `state.context` (`RuntimeMachine::context` in Rust, `snapshot.context` in WASM) `[Rust | WASM]`
  - [x] `state.status` (`'active' | 'done' | 'stopped'` in WASM, `RuntimeMachine::status()` / `is_done()` / `stop()` in Rust, `fsm_is_done` / `fsm_stop` in C, and `PortableXState::isDone()` / `stop()` in C++) `[Rust | WASM | C/C++]`
  - [x] `state.can(event)` (Pure transition check with guards across `RuntimeMachine::can` / `can_event`, `Snapshot::can`, `fsm_can`, and `PortableXState::can`) `[Rust | WASM | C/C++]`
  - [x] `state.hasTag(tag)` (Active state hierarchy tag check across `RuntimeMachine::has_tag`, `Snapshot::hasTag`, `fsm_has_tag`, and `PortableXState::hasTag`) `[Rust | WASM | C/C++]`
- [ ] **Machine / StateNode API**
  - [x] `machine.states` (StateNode tree registry) `[WASM]`
  - [x] `machine.events` (Accepted events list in definition order) `[WASM]`
  - [x] `machine.config` (Original machine config reference) `[WASM]`
  - [x] `machine.provide()` (Override guards/actions/context) `[WASM]`
  - [x] `machine.resolveState()` (Resolve state value and `done` status) `[WASM]`

---

## 4. Actor Model (Medium Priority)

- [ ] **Invoking Actors** (`invoke`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/invoke.test.ts) — not yet implemented; see `TD-7` in [`TECH_DEBT.md`](./TECH_DEBT.md))
  - [ ] Invoke Promise (`fromPromise`)
  - [ ] Invoke Callback (`fromCallback`)
  - [ ] Invoke Observable (`fromObservable`)
  - [ ] Invoke Machine (`fromTransition` / child machine)
  - [ ] `onDone` / `onError` (Transition on actor completion/failure)
- [ ] **Spawning Actors** (`spawn`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/spawn.test.ts) — not yet implemented)
  - [ ] Dynamic child actor creation via `assign` / `spawnChild`

---

## 5. Advanced & Niche (Low Priority)

- [ ] **Deep Persistence** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/json.test.ts) — upstream `json.test.ts` not yet onboarded)
  - [x] Persisting/Restoring state snapshots (`getPersistedSnapshot()`, `createActor(machine, { snapshot })`, `RuntimeMachine::get_snapshot_json`/`restore_from_snapshot_json`, `fsm_get_snapshot_json`/`fsm_restore_snapshot_json`, `PortableXState::getSnapshot`/`restoreSnapshot`) `[Rust | WASM | C/C++]`
  - [x] History value persistence and `StateNode` reference revival `[Rust | WASM | C/C++]`
- [ ] **Inspection & Telemetry**
  - [x] Basic `inspect` callback (`@xstate.snapshot` and `@xstate.event` emission in `createActor(machine, { inspect })`) `[WASM]`
  - [ ] Full `@xstate.actor` / `@xstate.microstep` inspection protocol
- [x] **Tags** (`tags: [...]`)
  - [x] Tag definition on states `[Rust | WASM | C/C++]`
  - [x] Tag checking via `snapshot.hasTag(tag)` / `RuntimeMachine::has_tag` / `fsm_has_tag` / `PortableXState::hasTag` `[Rust | WASM | C/C++]`
  - [x] Tag persistence across state restoration `[Rust | WASM | C/C++]`
- [ ] **Metadata & Descriptions** (`meta: { ... }`, `description`, `output`)
- [x] **Custom Clocks & Deterministic Virtual Time** (`SimulatedClock`, `register_clock`, `fsm_register_clock`, `fsm_register_clock_with_data`, `fsm_register_simulated_clock`, `fsm_simulated_clock_create`, `fsm_simulated_clock_free`, `fsm_simulated_clock_now`, `fsm_simulated_clock_set`, `fsm_simulated_clock_increment`) `[Rust | WASM | C/C++]`
