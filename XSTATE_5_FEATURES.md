# XState 5 Supported Operations Checklist

This document tracks the feature set of `portable-xstate` against the XState 5 specification. Features are grouped by type and ordered from most important to least important.

Test links point to the `statelyai/xstate` reference implementation for verification.

## 1. Core Structure (Essential)
High usage, required for almost every state machine.

- [ ] **Machine Definition** ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/machine.test.ts))
  - [ ] `id` (Root machine identifier)
  - [ ] `initial` (Initial state key)
  - [ ] `context` (Initial extended state)
  - [ ] `states` (State nodes definition)
- [ ] **State Nodes** ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/state.test.ts))
  - [ ] Atomic States (Leaf nodes)
  - [ ] Compound States (Nested states)
  - [ ] Parallel States (Orthogonal regions) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/parallel.test.ts))
  - [ ] Final States (`type: 'final'`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/final.test.ts))
  - [ ] History States (`type: 'history'`, `history: 'shallow' | 'deep'`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/history.test.ts))
- [ ] **Transitions** ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/transition.test.ts))
  - [ ] Event Transitions (`on: { EVENT: ... }`)
  - [ ] Eventless Transitions (`always: [...]`)
  - [ ] Delayed Transitions (`after: { delay: target }`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/after.test.ts))
  - [ ] Wildcard Transitions (`*`)
  - [ ] Partial Wildcard Transitions (`models.*`)

## 2. Logic & Control Flow (High Priority)
Essential for defining machine behavior and data flow.

- [ ] **Guards** (`guard`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/guards.test.ts))
  - [ ] Named Guards (String reference)
  - [ ] Inline Guards (Function definition)
  - [ ] Higher-Order Guards (`and`, `or`, `not`)
- [ ] **Actions** (`entry`, `exit`, `actions`) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/actions.test.ts))
  - [ ] Named Actions
  - [ ] Inline Actions
  - [ ] **Built-in Actions:**
    - [ ] `assign` (Update context) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/assign.test.ts))
    - [ ] `raise` (Self-send event)
    - [ ] `log` (Console log)
    - [ ] `sendTo` (Send to specific actor) ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/actor.test.ts))
    - [ ] `stop` (Stop child actor)
- [ ] **Context & Input**
  - [ ] Immutable Context Updates ([test](https://github.com/statelyai/xstate/blob/main/packages/core/test/assign.test.ts))
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
