# XState 5 Supported Operations Checklist

This document tracks the feature set of `portable-xstate` against the XState 5 specification. Features are grouped by type and ordered from most important to least important.

Test links point to the `statelyai/xstate` reference implementation for verification.

## 1. Core Structure (Essential)
High usage, required for almost every state machine.

- [x] **Machine Definition**([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/machine.test.ts))
  - [x] `id` (Root machine identifier)
  - [x] `initial` (Initial state key)
  - [x] `context` (Initial extended state)
  - [x] `states` (State nodes definition)
- [x] **State Nodes** ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/state.test.ts))
  - [x] Atomic States (Leaf nodes)
  - [x] Compound States (Nested states)
  - [x] Parallel States (Orthogonal regions) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/parallel.test.ts))
  - [x] Final States (`type: 'final'`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/final.test.ts))
  - [ ] History States (`type: 'history'`, `history: 'shallow' | 'deep'`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/history.test.ts))
- [ ] **Transitions** ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/transition.test.ts))
  - [x] Event Transitions (`on: { EVENT: ... }`)
  - [x] Eventless Transitions (`always: [...]`)
  - [ ] Delayed Transitions (`after: { delay: target }`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/after.test.ts))
  - [ ] Wildcard Transitions (`*`)
  - [ ] Partial Wildcard Transitions (`models.*`)

## 2. Logic & Control Flow (High Priority)
Essential for defining machine behavior and data flow.

- [ ] **Guards** (`guard`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/guards.test.ts))
  - [x] Named Guards (String reference)
  - [x] Inline Guards (Function definition)
  - [ ] Higher-Order Guards (`and`, `or`, `not`)
- [ ] **Actions** (`entry`, `exit`, `actions`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/actions.test.ts))
  - [x] Named Actions
  - [x] Inline Actions
  - [ ] **Built-in Actions:**
    - [x] `assign` (Update context) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/assign.test.ts))
    - [ ] `raise` (Self-send event)
    - [ ] `log` (Console log)
    - [ ] `sendTo` (Send to specific actor) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/actor.test.ts))
    - [ ] `stop` (Stop child actor)
- [ ] **Context & Input**
  - [x] Immutable Context Updates ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/assign.test.ts))
  - [ ] `input` (Factory-pattern context initialization) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/input.test.ts))

## 3. Actor Model (Medium Priority)
For communicating with external entities or running side-effects.

- [ ] **Invoking Actors** (`invoke`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/invoke.test.ts))
  - [ ] Invoke Promise (`fromPromise`)
  - [ ] Invoke Callback (`fromCallback`)
  - [ ] Invoke Observable (`fromObservable`)
  - [ ] Invoke Machine (`fromTransition` / machine)
  - [ ] `onDone` / `onError` (Transition on completion/failure)
- [ ] **Spawning Actors** (`spawn`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/spawn.test.ts))
  - [ ] Dynamic actor creation via `assign`

## 4. Advanced & Niche (Low Priority)
Features for complex use-cases or debugging.

- [ ] **Deep Persistence** ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/json.test.ts))
  - [ ] Persisting/Restoring nested actor states
- [ ] **Inspection & Telemetry**
  - [ ] `inspect` API integration
- [ ] **Tags** (`tags: [...]`)
- [ ] **Metadata** (`meta: { ... }`)
- [ ] **Custom Delays/Timers**
