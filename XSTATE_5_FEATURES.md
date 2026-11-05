# XState 5 Supported Operations Checklist

This document tracks the feature set of `portable-xstate` against the XState 5 (`statelyai/xstate@5.25.0`) specification across all three targets: `#![no_std]` Rust (`portable-xstate-rust`), WebAssembly/JS (`portable-xstate-wasm`), and C/C++ for embedded microcontrollers (`portable-xstate-c-cpp`).

Upstream conformance test integrity is enforced mechanically via `npm run test:conformance-check` against pinned reference files in [`tests/upstream/`](./tests/upstream/) and [`tests/conformance-manifest.json`](./tests/conformance-manifest.json). Pre-existing test divergences and legacy cleanup tasks are tracked in [`TECH_DEBT.md`](./TECH_DEBT.md).

## 1. Core Structure (Essential)
High usage, required for almost every state machine.

- [x] **Machine Definition** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/machine.test.ts))
  - [x] `id` (Root machine identifier)
  - [x] `initial` (Initial state key)
  - [x] `context` (Initial extended state)
  - [x] `states` (State nodes definition)
  - [x] `version` (Machine versioning)
  - [x] `types` (TypeScript type schemas - pass-through)
- [x] **State Nodes** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/state.test.ts))
  - [x] Atomic States (Leaf nodes)
  - [x] Compound States (Nested states with initial)
  - [x] Parallel States (Orthogonal regions) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/parallel.test.ts))
  - [x] Final States (`type: 'final'`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/final.test.ts))
  - [x] **History States** (`type: 'history'`, `history: 'shallow' | 'deep'`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/history.test.ts))
    - [x] Shallow history (default)
    - [x] Deep history (explicit)
    - [x] History with default target
    - [x] History in parallel states
    - [x] Multistage history
    - [x] History state persistence/revival
- [ ] **Transitions** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/transition.test.ts))
  - [x] Event Transitions (`on: { EVENT: ... }`)
  - [x] Eventless Transitions (`always: [...]`)
  - [x] Targetless Transitions (internal transitions with actions only)
  - [ ] Self-transitions with reentry (Partial - deep reentry skipped)
  - [x] Guarded Transitions
  - [x] Root-level transitions (combinatorial machines)
  - [x] Delayed Transitions (`after: { delay: target }`) (Partial) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/after.test.ts))
    - [x] Deterministic virtual time advancement (`SimulatedClock` with `set(ms)` / `increment(ms)` and cascading `tick()` across Rust, C, C++, and WASM)
    - [x] Deep-sleep deadline introspection (`next_timer_deadline` / `fsm_get_next_deadline` / `getNextDeadline`)
    - **Documented Target-Language Constraints**:
      - Dynamic delay expressions in C/C++/Rust must use registered named delay callbacks rather than runtime string/expression evaluation (WASM can bridge inline JS delay functions to synthetic named callbacks; see `TD-1` in [`TECH_DEBT.md`](./TECH_DEBT.md)).
      - Host JS `setTimeout`/`clearTimeout` handle inspection (`should not try to clear an undefined timeout when exiting source state of a delayed transition`) and internal JS `TransitionNode` string formatting (`should format transitions properly`) are skipped by design because `portable-xstate` evaluates timers against an external clock deadline rather than spawning host JS timers.
  - [ ] Wildcard Transitions (`*`)
  - [ ] Partial Wildcard Transitions (`models.*`)

## 2. Logic & Control Flow (High Priority)
Essential for defining machine behavior and data flow.

- [x] **Guards** (`guard`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/guards.test.ts))
  - [x] Named Guards (String reference; registered via function pointer / callback in Rust/C/C++ and `setup`/`provide` in WASM)
  - [x] Inline Guards (Function definition in WASM/JS bridged to synthetic named guards)
  - [x] Guard override via `machine.provide()`
  - [ ] Higher-Order Guards (`and`, `or`, `not`)
- [ ] **Actions** (`entry`, `exit`, `actions`) (Partial - see `TD-2` in [`TECH_DEBT.md`](./TECH_DEBT.md)) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/actions.test.ts))
  - [x] Named Actions (Registered via function pointer / callback in Rust/C/C++ and `setup`/`provide` in WASM)
  - [x] Inline Actions (Function definition in WASM/JS bridged to synthetic named actions)
  - [x] Action override via `machine.provide()`
  - [ ] Dynamic Action Parameters (skipped)
  - [x] Entry actions (state entry, including root initial `always` transition batching)
  - [x] Exit actions (state exit)
  - [x] Transition actions
  - [x] Initial transition actions
  - [ ] **Built-in Actions:**
    - [ ] `assign` (Update context) (Partial - assignment order skipped) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/assign.test.ts))
      - [x] Object-based assignment
      - [x] Function-based assignment (WASM/JS bridge)
      - [x] Inline assignment functions (WASM/JS bridge)
      - [x] Assignment in entry/exit/transition actions
    - [x] `raise` (Self-send internal event)
    - [ ] `log` (Console log)
    - [ ] `sendTo` (Send to specific actor) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/actor.test.ts))
    - [ ] `stop` (Stop child actor)
- [x] **Context & Input**
  - [x] Immutable Context Updates ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/assign.test.ts))
  - [x] Function-based context initialization (WASM/JS)
  - [x] Lazy context evaluation (WASM/JS)
  - [x] Context override via `machine.provide()`
  - [ ] `input` (Factory-pattern context initialization) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/input.test.ts))

## 3. State Inspection & Queries (High Priority)
Essential for runtime state inspection and decision-making.

- [x] **State API** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/state.test.ts))
  - [x] `state.value` (Current state value)
  - [x] `state.matches(stateValue)` (Query active state by dot-delimited path or partial object across Rust, C, C++, and WASM)
  - [x] `state.context` (Current context)
  - [x] `state.status` (Machine status: `'active'`, `'done'`, `'stopped'`)
  - [x] `state.can(event)` (Check if event is accepted without side effects)
  - [x] `state.hasTag(tag)` (Check if state has tag)
  - [x] State persistence and revival
- [x] **Machine API**
  - [x] `machine.states` (State node registry)
  - [x] `machine.events` (Accepted events set)
  - [x] `machine.config` (Original machine config reference)
  - [x] `machine.provide()` (Override implementations)
  - [x] `machine.resolveStateValue()` (Resolve state value)

## 4. Actor Model (Medium Priority)
For communicating with external entities or running side-effects.

- [ ] **Invoking Actors** (`invoke`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/invoke.test.ts))
  - [ ] Invoke Promise (`fromPromise`)
  - [ ] Invoke Callback (`fromCallback`)
  - [ ] Invoke Observable (`fromObservable`)
  - [ ] Invoke Machine (`fromTransition` / machine)
  - [ ] `onDone` / `onError` (Transition on completion/failure)
  - [x] Invoke function (inline function support in WASM)
- [ ] **Spawning Actors** (`spawn`) ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/spawn.test.ts))
  - [ ] Dynamic actor creation via `assign`

## 5. Advanced & Niche (Low Priority)
Features for complex use-cases or debugging.

- [x] **Deep Persistence** ([upstream test](https://github.com/statelyai/xstate/blob/xstate%405.25.0/packages/core/test/json.test.ts))
  - [x] Persisting/Restoring state snapshots
  - [x] History value persistence
  - [x] StateNode reference revival
- [ ] **Inspection & Telemetry**
  - [ ] `inspect` API integration
- [x] **Tags** (`tags: [...]`)
  - [x] Tag definition on states
  - [x] Tag checking via `state.hasTag()`
  - [x] Tag persistence across state restoration
- [ ] **Metadata** (`meta: { ... }`)
- [x] **Custom Delays/Timers** (`SimulatedClock` & custom clock providers across Rust, C, C++, and WASM)
