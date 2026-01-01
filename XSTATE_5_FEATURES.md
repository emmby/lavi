# XState 5 Supported Operations Checklist

This document tracks the feature set of `portable-xstate` against the XState 5 specification. Features are grouped by type and ordered from most important to least important.

Test links point to the `statelyai/xstate` reference implementation for verification.

**Test Coverage**: 73 passing tests across `machine.test.ts`, `state.test.ts`, and `history.test.ts`

## 1. Core Structure (Essential)
High usage, required for almost every state machine.

- [x] **Machine Definition** ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/machine.test.ts))
  - [x] `id` (Root machine identifier)
  - [x] `initial` (Initial state key)
  - [x] `context` (Initial extended state)
  - [x] `states` (State nodes definition)
  - [x] `version` (Machine versioning)
  - [x] `types` (TypeScript type schemas - pass-through)
- [x] **State Nodes** ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/state.test.ts))
  - [x] Atomic States (Leaf nodes)
  - [x] Compound States (Nested states with initial)
  - [x] Parallel States (Orthogonal regions) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/parallel.test.ts))
  - [x] Final States (`type: 'final'`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/final.test.ts))
  - [x] **History States** (`type: 'history'`, `history: 'shallow' | 'deep'`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/history.test.ts))
    - [x] Shallow history (default)
    - [x] Deep history (explicit)
    - [x] History with default target
    - [x] History in parallel states
    - [x] Multistage history
    - [x] History state persistence/revival
- [x] **Transitions** ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/transition.test.ts))
  - [x] Event Transitions (`on: { EVENT: ... }`)
  - [x] Eventless Transitions (`always: [...]`)
  - [x] Targetless Transitions (internal transitions with actions only)
  - [x] Self-transitions with reentry
  - [x] Guarded Transitions
  - [x] Root-level transitions (combinatorial machines)
  - [ ] Delayed Transitions (`after: { delay: target }`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/after.test.ts))
  - [ ] Wildcard Transitions (`*`)
  - [ ] Partial Wildcard Transitions (`models.*`)

## 2. Logic & Control Flow (High Priority)
Essential for defining machine behavior and data flow.

- [x] **Guards** (`guard`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/guards.test.ts))
  - [x] Named Guards (String reference)
  - [x] Inline Guards (Function definition)
  - [x] Guard override via `machine.provide()`
  - [ ] Higher-Order Guards (`and`, `or`, `not`)
- [x] **Actions** (`entry`, `exit`, `actions`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/actions.test.ts))
  - [x] Named Actions
  - [x] Inline Actions (Function definition)
  - [x] Action override via `machine.provide()`
  - [x] Entry actions (state entry)
  - [x] Exit actions (state exit)
  - [x] Transition actions
  - [x] Initial transition actions
  - [x] **Built-in Actions:**
    - [x] `assign` (Update context) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/assign.test.ts))
      - [x] Object-based assignment
      - [x] Function-based assignment
      - [x] Inline assignment functions
      - [x] Assignment in entry/exit/transition actions
    - [ ] `raise` (Self-send event)
    - [ ] `log` (Console log)
    - [ ] `sendTo` (Send to specific actor) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/actor.test.ts))
    - [ ] `stop` (Stop child actor)
- [x] **Context & Input**
  - [x] Immutable Context Updates ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/assign.test.ts))
  - [x] Function-based context initialization
  - [x] Lazy context evaluation
  - [x] Context override via `machine.provide()`
  - [ ] `input` (Factory-pattern context initialization) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/input.test.ts))

## 3. State Inspection & Queries (High Priority)
Essential for runtime state inspection and decision-making.

- [x] **State API** ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/state.test.ts))
  - [x] `state.value` (Current state value)
  - [x] `state.context` (Current context)
  - [x] `state.status` (Machine status: 'active', 'done', 'stopped')
  - [x] `state.can(event)` (Check if event is accepted)
    - [x] Simple event checks
    - [x] Event object checks
    - [x] Guard evaluation in `.can()`
    - [x] No side effects (no actor spawning, no assignments)
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

- [ ] **Invoking Actors** (`invoke`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/invoke.test.ts))
  - [ ] Invoke Promise (`fromPromise`)
  - [ ] Invoke Callback (`fromCallback`)
  - [ ] Invoke Observable (`fromObservable`)
  - [ ] Invoke Machine (`fromTransition` / machine)
  - [ ] `onDone` / `onError` (Transition on completion/failure)
  - [x] Invoke function (inline function support)
- [ ] **Spawning Actors** (`spawn`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/spawn.test.ts))
  - [ ] Dynamic actor creation via `assign`

## 5. Advanced & Niche (Low Priority)
Features for complex use-cases or debugging.

- [x] **Deep Persistence** ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/json.test.ts))
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
- [ ] **Custom Delays/Timers**

## Implementation Notes

### Combinatorial Machines
- ✅ Full support for machines without states (root-level transitions only)
- ✅ Root-level `on`, `entry`, `exit`, and `always` transitions
- ✅ Context updates via `assign` in combinatorial machines

### History States
- ✅ Comprehensive history state support including:
  - Shallow and deep history
  - Default targets
  - Parallel state history
  - History persistence and revival from snapshots
  - Proper action execution during history restoration

### WASM Integration
- ✅ Full JavaScript/TypeScript interop via `wasm-bindgen`
- ✅ Inline function support for guards and actions
- ✅ `assign()` helper function compatible with XState 5 API
- ✅ Snapshot serialization/deserialization

### Test Coverage
- **machine.test.ts**: 20 tests covering machine definition, transitions, guards, actions, and combinatorial machines
- **state.test.ts**: 20 tests covering state API, `.can()`, `.hasTag()`, status checks, and persistence
- **history.test.ts**: 33 tests covering shallow/deep history, parallel history, and history revival
