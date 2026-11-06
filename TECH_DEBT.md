# Technical Debt & Legacy Cleanup Queue

This file tracks pre-existing conformance discrepancies and legacy codebase anti-patterns so feature agents do **not** copy them and do **not** perform out-of-scope refactoring mid-feature (which causes merge conflicts during parallel swarming).

## Rules for Agents
1. **Do Not Copy**: Never use any pattern listed in this file as a precedent for new code. Follow the **Canonical Patterns** in [`AGENTS.md`](./AGENTS.md).
2. **Queue, Don't Sidetrack**: When you discover a legacy anti-pattern or pre-existing conformance deviation outside the direct lines of your assigned feature, append a new `TD-N` item here rather than refactoring unrelated files in your feature branch.
3. **Pulling Cleanup Tasks**: Dedicated cleanup agents can pull individual `TD-N` items as isolated tasks. When a `legacy-baseline` test file in [`tests/conformance-manifest.json`](./tests/conformance-manifest.json) is fully restored to match `tests/upstream/`, change its `"status"` to `"strict"` and remove `"legacy_baseline_sha256"`.

---

## Open Cleanup Items

### `TD-1`: Restore Unmodified Upstream `tests/after.test.ts` and Fake-Timer / `SimulatedClock` Bridging
- **Files**: [`tests/after.test.ts`](./tests/after.test.ts), [`tests/upstream/after.test.ts`](./tests/upstream/after.test.ts), [`portable-xstate-wasm/src/lib.rs`](./portable-xstate-wasm/src/lib.rs)
- **Problem**: `tests/after.test.ts` was historically truncated (218 lines vs. upstream's full suite), manually injects `actorRef.tick()` into test bodies, and guts the bodies of `it.skip(...)` blocks.
- **Resolution**: Replace `tests/after.test.ts` with `tests/upstream/after.test.ts` (using only whitelisted import and `: any` adaptations), bridge `SimulatedClock` / Vitest fake timers in `portable-xstate-wasm` so manual `.tick()` calls are unnecessary in JS tests, keep full bodies inside any registered `it.skip` entries, and switch `after.test.ts` from `"legacy-baseline"` to `"strict"` in `tests/conformance-manifest.json`.

### `TD-2`: Reconcile `tests/actions.test.ts` with Upstream `statelyai/xstate@5.25.0`
- **Files**: [`tests/actions.test.ts`](./tests/actions.test.ts), [`tests/upstream/actions.test.ts`](./tests/upstream/actions.test.ts), [`tests/conformance-manifest.json`](./tests/conformance-manifest.json)
- **Problem**: `tests/actions.test.ts` contains historical `it.skip` additions and minor inline edits that predate `npm run test:conformance-check`.
- **Resolution**: Restore `tests/actions.test.ts` to match `tests/upstream/actions.test.ts` under strict normalization, register any legitimately unimplemented sub-features in `tests/conformance-manifest.json` `allowed_skips`, and graduate `actions.test.ts` from `"legacy-baseline"` to `"strict"`.

### `TD-3`: Graduate `tests/state.test.ts` and `tests/history.test.ts` to Strict Provenance
- **Files**: [`tests/state.test.ts`](./tests/state.test.ts), [`tests/history.test.ts`](./tests/history.test.ts), [`tests/conformance-manifest.json`](./tests/conformance-manifest.json)
- **Problem**: These suites pass or skip individual tests but contain historical edits relative to `tests/upstream/{state,history}.test.ts` (`tests/machine.test.ts` has already been graduated to `"strict"`).
- **Resolution**: Diff each file against `tests/upstream/`, remove any non-whitelisted edits, register legitimate skips in `tests/conformance-manifest.json`, and set `"status": "strict"`.

### `TD-6`: Remove Redundant `StateDefinition::type_` String Field and XState 4 `cond` Naming
- **Files**: [`portable-xstate-rust/src/lib.rs`](./portable-xstate-rust/src/lib.rs), [`portable-xstate-rust/src/parser.rs`](./portable-xstate-rust/src/parser.rs)
- **Problem**: `StateDefinition` stores both `pub type_: Option<String>` and `pub state_type: StateType`, and `TransitionDefinition` uses XState 4's `cond` field name instead of XState 5's `guard`.
- **Resolution**: Consolidate on `StateType` and rename `cond` to `guard` across `portable-xstate-rust` and its adapters.

### `TD-7`: Remove `def.invoke` Execution Hack from State Exit Loop in `portable-xstate-rust`
- **Files**: [`portable-xstate-rust/src/lib.rs`](./portable-xstate-rust/src/lib.rs), [`portable-xstate-wasm/src/lib.rs`](./portable-xstate-wasm/src/lib.rs)
- **Problem**: `RuntimeMachine::transition_internal` (`portable-xstate-rust/src/lib.rs:1257-1261`) executes `def.invoke` entries as synchronous named actions during **state exit**, which was a one-off hack rather than proper XState 5 actor invocation lifecycle on state entry/exit.
- **Resolution**: Replace with proper `invoke` lifecycle semantics when implementing the Actor Model (`invoke.test.ts`).

### `TD-8`: Promote Snapshot Persistence to `portable-xstate-rust` and `portable-xstate-c-cpp`
- **Files**: [`portable-xstate-rust/src/lib.rs`](./portable-xstate-rust/src/lib.rs), [`portable-xstate-wasm/src/lib.rs`](./portable-xstate-wasm/src/lib.rs), [`portable-xstate-c-cpp/include/portable_xstate.h`](./portable-xstate-c-cpp/include/portable_xstate.h), [`portable-xstate-c-cpp/include/portable_xstate.hpp`](./portable-xstate-c-cpp/include/portable_xstate.hpp)
- **Problem**: While state queries (`can`, `has_tag`, `status`/`is_done`, and `stop`) are now implemented in `portable-xstate-rust` and exposed in `portable-xstate-c-cpp` (`fsm_can`, `fsm_has_tag`, `fsm_is_done`, `fsm_stop`), snapshot serialization and restoration remain implemented inside `portable-xstate-wasm/src/lib.rs` rather than on `RuntimeMachine` in `portable-xstate-rust`, and are not yet exposed in C/C++.
- **Resolution**: Move snapshot serialize/restore into `portable-xstate-rust::RuntimeMachine`, delegate to it from `portable-xstate-wasm`, and expose C/C++ bindings in `portable-xstate-c-cpp`.

---

## Resolved Cleanup Items

- **`TD-4`** (Resolved): Removed `fs.appendFileSync('trace.log', ...)`, `fs.appendFileSync('debug.log', ...)`, `console.log` spam, and `machine.createActor` / `machine.provide` monkey-patching from [`tests/utils.ts`](./tests/utils.ts), bridging tracked entry/exit callbacks iteratively in `portable-xstate-wasm`.
- **`TD-3` (`tests/machine.test.ts`)** (Resolved): Reconciled [`tests/machine.test.ts`](./tests/machine.test.ts) with [`tests/upstream/machine.test.ts`](./tests/upstream/machine.test.ts) and graduated `machine.test.ts` from `"legacy-baseline"` to `"strict"` in [`tests/conformance-manifest.json`](./tests/conformance-manifest.json).
- **`TD-5`** (Resolved): Replaced remaining recursive traversal helpers (`StateValue::collect_paths`, `StateValue::matches`, `StateValue::drop` in [`portable-xstate-rust/src/lib.rs`](./portable-xstate-rust/src/lib.rs) and `StateNode::events` in [`portable-xstate-wasm/src/lib.rs`](./portable-xstate-wasm/src/lib.rs)) with bounded $O(1)$ call-stack iterative traversals using explicit heap `Vec` work-stacks, verified by a 64 KB stack thread test at depth >= 256.
- **`TD-8` (State Queries)** (Resolved): Promoted `can` / `can_event`, `has_tag`, `status` (`MachineStatus::Active` / `Done` / `Stopped`), `is_done`, and `stop` to `RuntimeMachine` in [`portable-xstate-rust/src/lib.rs`](./portable-xstate-rust/src/lib.rs), delegated to them in [`portable-xstate-wasm/src/lib.rs`](./portable-xstate-wasm/src/lib.rs), and exposed C/C++ bindings (`fsm_can`, `fsm_has_tag`, `fsm_is_done`, `fsm_stop`, and `PortableXState::{can, hasTag, isDone, stop}`) in `portable-xstate-c-cpp`.
