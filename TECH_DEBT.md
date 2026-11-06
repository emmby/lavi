# Technical Debt & Legacy Cleanup Queue

This file tracks pre-existing conformance discrepancies and legacy codebase anti-patterns so feature agents do **not** copy them and do **not** perform out-of-scope refactoring mid-feature (which causes merge conflicts during parallel swarming).

## Rules for Agents
1. **Do Not Copy**: Never use any pattern listed in this file as a precedent for new code. Follow the **Canonical Patterns** in [`AGENTS.md`](./AGENTS.md).
2. **Queue, Don't Sidetrack**: When you discover a legacy anti-pattern or pre-existing conformance deviation outside the direct lines of your assigned feature, append a new `TD-N` item here rather than refactoring unrelated files in your feature branch.
3. **Pulling Cleanup Tasks**: Dedicated cleanup agents can pull individual `TD-N` items as isolated tasks. When a `legacy-baseline` test file in [`tests/conformance-manifest.json`](./tests/conformance-manifest.json) is fully restored to match `tests/upstream/`, change its `"status"` to `"strict"` and remove `"legacy_baseline_sha256"`.

---

## Open Cleanup Items

### `TD-7`: Remove `def.invoke` Execution Hack from State Exit Loop in `lavi`
- **Files**: [`lavi/src/lib.rs`](./lavi/src/lib.rs) (lines 2213–2217), [`lavi-wasm/src/lib.rs`](./lavi-wasm/src/lib.rs)
- **Problem**: `RuntimeMachine::transition_internal` executes `def.invoke` entries as synchronous named actions during **state exit**, which was an early prototype workaround rather than proper XState 5 actor invocation lifecycle on state entry/exit.
- **Evaluation & Current Status (Milestone 5)**:
  - Empirically evaluated in Milestone 5: removing lines 2213–2217 was tested against the full test suite.
  - Test dependency identified: `tests/history.test.ts` test `"should invoke an actor when reentering the stored configuration through the history state"` relies on this exit behavior (`expect(spy).toHaveBeenCalledTimes(1)` fails with 0 calls if lines 2213–2217 are removed).
  - Root cause: Root-level history resolution in `RuntimeMachine` currently falls back to `refresh` (the history state node itself) without re-entering `running`. The exit loop executed `def.invoke` when `running` exited, which masked the lack of full re-entry and child actor lifecycle management.
  - Action taken: Retained lines 2213–2217 until the Actor Model (`invoke.test.ts`) is onboarded. Resolving `TD-7` cleanly requires implementing proper child actor lifecycle (`ActorRef` creation on state entry, termination on state exit) rather than simply deleting the exit loop.

### `TD-9`: Consolidate Split-Brain WASM `Actor` State, Dual Reentrant Queues, and `Rc<MachineDefinition>`
- **Files**: [`lavi-wasm/src/lib.rs`](./lavi-wasm/src/lib.rs), [`lavi/src/lib.rs`](./lavi/src/lib.rs)
- **Problem**:
  1. **Split-Brain State in WASM `Actor` and `SharedActorState`**: To allow JS action callbacks to reentrantly call `actorRef.send(...)` or `actorRef.stop()` without triggering a `wasm-bindgen` recursive `&mut self` borrow panic, `Actor` methods were changed to `&self` using `shared_state: Rc<RefCell<Option<SharedActorState>>>`. However, `Actor` still retains duplicate top-level fields (`machine`, `inspector`, `guards`, `actions`, `is_stopped`, `is_initialized`) alongside `shared_state`, syncing between them via `pull_shared()` and `push_shared()` by cloning the entire `RuntimeMachine` and `IndexMap`s back and forth.
  2. **Two Separate Reentrant Queues**: `RuntimeMachine` already has an `external_queue` (`VecDeque<(String, Value)>`), and `SharedActorState` in WASM added a second `reentrant_queue` (`VecDeque<(String, Value)>`) because WASM clones `machine` out of the `RefCell` before running a transition step (`is_in_tick`).
  3. **Deep `MachineDefinition` Clones**: `RuntimeMachine` stores `definition: MachineDefinition` by value rather than `Rc<MachineDefinition>`, causing every `RuntimeMachine::clone()` in WASM (`send()`, `tick()`, `getSnapshot()`, `StateNode` creation) to deep-clone the entire statechart AST.
- **Required Fix**:
  - Store only `Rc<RefCell<ActorInner>>` inside WASM `Actor` (eliminating duplicate top-level fields and `pull_shared()`/`push_shared()`), use fine-grained borrows so `RuntimeMachine::external_queue` serves as the single reentrant FIFO queue, and wrap `MachineDefinition` in `Rc<MachineDefinition>` inside `RuntimeMachine`.

### `TD-10`: Decouple `extract_unprocessed_fn_actions` from `__xstate_track` Marker Property
- **Files**: [`lavi-wasm/src/lib.rs`](./lavi-wasm/src/lib.rs) (lines 2786–2826), [`tests/utils.ts`](./tests/utils.ts)
- **Problem**: `extract_unprocessed_fn_actions` in `lavi-wasm` checks for a `__xstate_track` property on JS functions (set by `trackEntries` in `tests/utils.ts`). While this kept `tests/utils.ts` free of monkey-patching `machine.createActor`, coupling the WASM runtime to a test-helper marker property (`__xstate_track`) is a leaky abstraction.
- **Required Fix**: Update `sync_tracked_entry_exit` / `extract_unprocessed_fn_actions` to bridge *any* unbridged JS function found on `machine.config` `entry`/`exit` arrays (i.e. any JS `Function` that was mutated onto `config` after `createMachine` and does not already have a bridged action `type`), and remove the `__xstate_track` tag from `tests/utils.ts` and `lavi-wasm`.

### `TD-11`: Zero-Allocation Pass-1 Size Query in `fsm_get_snapshot_json`
- **Files**: [`lavi-c-cpp/src/lib.rs`](./lavi-c-cpp/src/lib.rs) (lines 601–632), [`lavi/src/json.rs`](./lavi/src/json.rs)
- **Problem**: In C FFI, `fsm_get_snapshot_json` allocates the full serialized JSON `String` on the heap during the pass-1 size query (`buffer == NULL` or `buffer_len == 0`) and serializes/allocates it a second time on pass 2 when copying into the caller's buffer (such as in `lavi::Machine::getSnapshot()`).
- **Required Fix**: Implement a streaming or counting length calculation (e.g. a zero-allocation byte-counting visitor over `RuntimeMachine` / `Value`) or write directly into the caller's buffer slice when `buffer != NULL` so pass-1 size queries perform zero heap string allocations.

### `TD-12`: Publish `lavi-wasm` to npm [flowos: e9505536-8f94-4c57-8de4-944e1257ce7c]
- **Files**: [`lavi-wasm/Cargo.toml`](./lavi-wasm/Cargo.toml), [`package.json`](./package.json), [`README.md`](./README.md)
- **Problem**: `lavi-wasm` is currently built locally into `lavi-wasm/pkg` and `lavi-wasm/pkg-npm` rather than published to the public npm registry.
- **Resolution**: Publish `lavi-wasm` (or `lavi`) to npm so JavaScript/TypeScript users can `npm install lavi-wasm` and `import { createMachine, createActor, assign } from 'lavi-wasm'` directly.

### `TD-13`: Replace Recursive `#[derive(Clone, PartialEq)]` on `StateValue` with Iterative Heap Work-Stacks [flowos: 1695ba29-a50c-4f0f-8a21-05ecb6f02ed0]
- **Files**: [`lavi/src/lib.rs`](./lavi/src/lib.rs) (lines 19–24)
- **Problem**: While `StateValue::drop`, `StateValue::to_value`, `StateValue::collect_paths`, `StateValue::matches`, `Value::clone`, and `Value::eq` use iterative heap work-stacks (`Vec`), `StateValue` still derives `Clone` and `PartialEq` (`#[derive(Debug, Clone, PartialEq)]`). Comparing or cloning deeply nested `StateValue` trees (e.g., `finalized_state != self.current_state` in `transition_internal` or `self.current_state.clone()` when recording `history_value`) recurses on the CPU call stack proportional to state nesting depth $O(D)$.
- **Required Fix**: Implement manual iterative `Clone` and `PartialEq` for `StateValue` using explicit heap `Vec` work-stacks (matching `impl Clone for Value` and `impl PartialEq for Value`), and audit remaining state-value helpers (`resolve_state_value_from_path`, `merge_state_value`) to ensure 100% bounded $O(1)$ call-stack usage.


---

## Resolved Cleanup Items

### `TD-1`: Restore Unmodified Upstream `tests/after.test.ts` and Fake-Timer / `SimulatedClock` Bridging (Resolved in Milestone 3)
- **Files Modified**: [`tests/after.test.ts`](./tests/after.test.ts), [`tests/conformance-manifest.json`](./tests/conformance-manifest.json), [`lavi-wasm/src/lib.rs`](./lavi-wasm/src/lib.rs), [`tests/integration/timer_execution.test.ts`](./tests/integration/timer_execution.test.ts)
- **Problem**: `tests/after.test.ts` was historically truncated (218 lines vs. upstream's 362 lines), manually injected `actorRef.tick()` into test bodies, and gutted code inside `it.skip(...)` blocks.
- **Architectural Changes**:
  - Restored verbatim upstream `tests/after.test.ts` (362 lines) matching `statelyai/xstate@5.25.0` with 0 manual `actorRef.tick()` calls and full `it.skip(...)` test bodies intact.
  - Graduated `tests/after.test.ts` to `"status": "strict"` in `tests/conformance-manifest.json`, eliminating `"legacy_baseline_sha256"` and `"tech_debt_id": "TD-1"`.
  - Implemented generic earliest-deadline host timer bridging in `lavi-wasm` (`Actor::reschedule_host_timer`, `Actor::on_host_timeout`, `resolve_host_clock`) supporting fake timers (`vi.useFakeTimers()`), native `setTimeout`, custom host clocks, and pre-clearing `active_timeout = None` to solve Issue #5001.
  - Implemented Observer protocol in `Actor::subscribe` supporting both function listeners and `{ next, complete, error }` Observer objects, with automatic invocation of `complete()` on final states.
  - Authored 16 empirical challenger tests in `tests/integration/timer_execution.test.ts`.
- **Verification**: Verified via `npm run test:conformance-check` (0 errors), `tests/after.test.ts` (5 passed, 5 skipped), `cargo test --workspace` (69 passed), `cargo clippy` (0 warnings), `npm run test:c-cpp` (passed), and `npm test` (all 6 phases passed).

### `TD-2`: Reconcile `tests/actions.test.ts` with Upstream `statelyai/xstate@5.25.0` (Resolved in Milestone 4)
- **Files Modified**: [`tests/actions.test.ts`](./tests/actions.test.ts), [`tests/conformance-manifest.json`](./tests/conformance-manifest.json), [`lavi-wasm/src/lib.rs`](./lavi-wasm/src/lib.rs), [`tests/integration/actions_challenge.test.ts`](./tests/integration/actions_challenge.test.ts)
- **Problem**: `tests/actions.test.ts` contained historical modifications, test body alterations, and missing upstream tests, running under `"status": "legacy-baseline"`.
- **Architectural Changes**:
  - Reconciled `tests/actions.test.ts` with upstream `tests/upstream/actions.test.ts` with 0 byte normalized diff (99,489 chars, SHA-256 `250731fbde2bd4d82522d15a5cce39651c5b690efa68297e9a9332e9b9eddec6`), 0 manual tick injections, and 0 gutted test bodies. Reverted Test #14 debug statements and restored Test #48 upstream skip.
  - Graduated `tests/actions.test.ts` to `"status": "strict"` in `tests/conformance-manifest.json`, eliminating `"legacy_baseline_sha256"` and `"tech_debt_id": "TD-2"`.
  - Implemented interior mutability in `lavi-wasm/src/lib.rs` (`Actor::start`, `Actor::send`, `Actor::stop`, `Actor::subscribe_js` converted to `&self` backed by `Rc<RefCell<Option<SharedActorState>>>`) to eliminate wasm-bindgen recursive borrow panics when action callbacks send events.
  - Implemented deterministic synchronous reentrant event queuing (`shared.reentrant_queue` drained iteratively in FIFO order).
  - Exported genuine `raise(event, options)` action creator in WASM with immediate microstep drainage and delayed raise tagging.
  - Migrated host timer closures to `Closure::once_into_js`, eliminating closure drop panics during transition timer cleanup.
  - Preserved actor status `"done"` on completion, dropping subsequent events on final actors.
  - Authored 23 empirical challenger tests in `tests/integration/actions_challenge.test.ts`.
- **Verification**: Verified via `npm run test:conformance-check` (0 errors), `tests/actions.test.ts` (80 passed, 55 skipped, 1 todo), `cargo test --workspace` (69 passed), `cargo clippy` (0 warnings), `npm run test:c-cpp` (passed), and `npm test` (all 6 phases passed, 235 passed, 62 skipped, 1 todo).

### `TD-3`: Reconcile `machine.test.ts`, `state.test.ts`, and `history.test.ts` (Resolved in Milestone Survey/Foundation)
- **Files Modified**: [`tests/machine.test.ts`](./tests/machine.test.ts), [`tests/state.test.ts`](./tests/state.test.ts), [`tests/history.test.ts`](./tests/history.test.ts), [`tests/conformance-manifest.json`](./tests/conformance-manifest.json)
- **Resolution**: Reconciled all three test suites with upstream references in `tests/upstream/` and graduated them to `"status": "strict"` in `tests/conformance-manifest.json`.

### `TD-4`: Purge Debug Logging and Test Monkey-Patching (Resolved in Milestone Survey/Foundation)
- **Files Modified**: [`tests/utils.ts`](./tests/utils.ts), [`lavi-wasm/src/lib.rs`](./lavi-wasm/src/lib.rs)
- **Resolution**: Removed `fs.appendFileSync('trace.log', ...)`, `fs.appendFileSync('debug.log', ...)`, `console.log` spam, and `machine.createActor` / `machine.provide` monkey-patching from `tests/utils.ts`, bridging tracked entry/exit callbacks iteratively in `lavi-wasm`.

### `TD-5`: Eliminate Recursive State & Value Traversals (Resolved in Milestone Survey/Foundation)
- **Files Modified**: [`lavi/src/lib.rs`](./lavi/src/lib.rs), [`lavi-wasm/src/lib.rs`](./lavi-wasm/src/lib.rs)
- **Resolution**: Replaced remaining recursive traversal helpers (`StateValue::collect_paths`, `StateValue::matches`, `StateValue::drop` in `lavi` and `StateNode::events` in `lavi-wasm`) with bounded $O(1)$ call-stack iterative traversals using explicit heap `Vec` work-stacks, verified by a 64 KB stack thread test at depth >= 256.

### `TD-6`: Rust Struct Field Hygiene (`state_type` & `guard` Naming) (Resolved in Milestone 1)
- **Files Modified**: [`lavi/src/lib.rs`](./lavi/src/lib.rs), [`lavi/src/parser.rs`](./lavi/src/parser.rs), [`lavi-wasm/src/lib.rs`](./lavi-wasm/src/lib.rs)
- **Problem**: `StateDefinition` stored redundant `pub type_: Option<String>` alongside `pub state_type: StateType`, and `TransitionDefinition` used legacy XState 4 naming `cond` instead of XState 5 `guard`.
- **Architectural Changes**:
  - Removed `type_: Option<String>` from `StateDefinition` and `MachineDefinition` in `lavi`.
  - Consolidated all state classification on the strongly-typed `StateType` enum across Rust and WASM.
  - Renamed `cond` to `guard` on `TransitionDefinition`, preserving JSON input backwards compatibility in `parser.rs` via `obj.swap_remove("guard").or_else(|| obj.swap_remove("cond"))`.
  - Updated all match patterns, builders, and adapters in `lavi-wasm`.
- **Verification**: Verified via `cargo test --workspace` (pass), `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings), `npm run test:conformance-check` (pass), and `npm test` (all passed).

### `TD-8`: Promote Snapshot Persistence to `lavi` and `lavi-c-cpp` (Resolved in Milestone 2)
- **Files Modified**: [`lavi/src/lib.rs`](./lavi/src/lib.rs), [`lavi/src/json.rs`](./lavi/src/json.rs), [`lavi-c-cpp/src/lib.rs`](./lavi-c-cpp/src/lib.rs), [`lavi-c-cpp/include/lavi.h`](./lavi-c-cpp/include/lavi.h), [`lavi-c-cpp/include/lavi.hpp`](./lavi-c-cpp/include/lavi.hpp), [`lavi-wasm/src/lib.rs`](./lavi-wasm/src/lib.rs), [`lavi/tests/snapshot_tests.rs`](./lavi/tests/snapshot_tests.rs), [`lavi/tests/adversarial_stack_tests.rs`](./lavi/tests/adversarial_stack_tests.rs), [`tests/integration/snapshot_status.test.ts`](./tests/integration/snapshot_status.test.ts), `lavi-c-cpp/tests/c_test.c`, `lavi-c-cpp/tests/cpp_test.cpp`
- **Problem**: Snapshot serialization, restoration, and state queries were fragmented or trapped in JS/WASM code rather than available to bare-metal Rust and C/C++ microcontrollers.
- **Architectural Changes**:
  - Implemented core snapshot persistence in `lavi`: `StateValue::to_value()`, `get_persisted_snapshot_value()`, `get_persisted_snapshot_json()`, `restore_from_snapshot_value_owned()`, and `restore_from_snapshot_json()`.
  - Replaced recursive `#[derive(Clone, PartialEq)]` on `Value` with explicit iterative `impl Clone for Value` and `impl PartialEq for Value` using heap work-stacks, achieving strictly bounded $O(1)$ CPU call-stack footprint tested to depth 500+ in 64 KB and 32 KB stacks.
  - Exposed buffer-safe C99 FFI (`fsm_get_snapshot_json`, `fsm_restore_snapshot_json`) with two-pass sizing and C++11 RAII methods (`lavi::Machine::getSnapshot`, `restoreSnapshot`).
  - Replaced ad-hoc WASM hydration with direct delegation to Rust core, purged all 10 raw `console.log` statements from WASM restore logic, and ensured persisted status preserves `"status": "done"`.
  - Promoted state query methods (`can`, `has_tag`, `status`, `is_done`, `stop`) across Rust, C, and C++.
- **Verification**: Verified via `lavi/tests/snapshot_tests.rs` (8 passed), `lavi/tests/adversarial_stack_tests.rs` (9 passed), `tests/integration/snapshot_status.test.ts` (4 passed), C unit tests in `c_test.c`, C++ tests in `cpp_test.cpp`, `cargo test --workspace` (pass), `cargo clippy` (0 warnings), and `npm test` (all passed).
