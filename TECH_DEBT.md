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

### `TD-3`: Graduate `tests/machine.test.ts`, `tests/state.test.ts`, and `tests/history.test.ts` to Strict Provenance
- **Files**: [`tests/machine.test.ts`](./tests/machine.test.ts), [`tests/state.test.ts`](./tests/state.test.ts), [`tests/history.test.ts`](./tests/history.test.ts), [`tests/conformance-manifest.json`](./tests/conformance-manifest.json)
- **Problem**: These three suites pass or skip individual tests but contain historical edits relative to `tests/upstream/{machine,state,history}.test.ts`.
- **Resolution**: Diff each file against `tests/upstream/`, remove any non-whitelisted edits, register legitimate skips in `tests/conformance-manifest.json`, and set `"status": "strict"`.

### `TD-4`: Remove Debug File Logging and `createActor` Monkey-Patching in `tests/utils.ts`
- **Files**: [`tests/utils.ts`](./tests/utils.ts)
- **Problem**: `trackEntries` in `tests/utils.ts` calls `fs.appendFileSync('trace.log', ...)`, `fs.appendFileSync('debug.log', ...)`, logs `console.log("HELLO XSTATE V5 createActor invoked!")`, and monkey-patches `machine.createActor`.
- **Resolution**: Restore clean, in-memory `trackEntries` matching upstream `packages/core/test/utils.ts` without filesystem side effects or console noise.

### `TD-5`: Replace Remaining Recursive Traversal Helpers with Iterative Heap Work-Stacks
- **Files**: [`portable-xstate-rust/src/lib.rs`](./portable-xstate-rust/src/lib.rs), [`portable-xstate-wasm/src/lib.rs`](./portable-xstate-wasm/src/lib.rs)
- **Problem**: While `json.rs`, `parser.rs`, `Value::drop`, and `StateDefinition::drop` use bounded $O(1)$ call-stack traversal via explicit heap `Vec` work-stacks, older helpers (`StateValue::collect_paths`, `StateValue::matches`, and `StateNode::events` in WASM) still use CPU call-stack recursion.
- **Resolution**: Convert all remaining recursive state/value traversals in `portable-xstate-rust` and `portable-xstate-wasm` to iterative `Vec` work-stacks to guarantee microcontroller stack safety.

### `TD-6`: Remove Redundant `StateDefinition::type_` String Field and XState 4 `cond` Naming
- **Files**: [`portable-xstate-rust/src/lib.rs`](./portable-xstate-rust/src/lib.rs), [`portable-xstate-rust/src/parser.rs`](./portable-xstate-rust/src/parser.rs)
- **Problem**: `StateDefinition` stores both `pub type_: Option<String>` and `pub state_type: StateType`, and `TransitionDefinition` uses XState 4's `cond` field name instead of XState 5's `guard`.
- **Resolution**: Consolidate on `StateType` and rename `cond` to `guard` across `portable-xstate-rust` and its adapters.
