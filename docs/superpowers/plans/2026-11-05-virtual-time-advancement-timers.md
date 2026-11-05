# Virtual Time Advancement & `SimulatedClock` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable accurate single-call virtual and real-time advancement across multiple cascading and hierarchical `after` timer expirations in `portable-xstate` using XState 5's `SimulatedClock` and external clock model.

**Architecture:** `RuntimeMachine` remains free of internal clock ownership, querying its registered `clock_millis_fn` callback as the single source of truth for time, while exposing `next_timer_deadline() -> Option<u64>` and fixing `tick()` to evaluate expired timers in strict chronological deadline order (with FIFO tie-breaking and a 100-step same-timestamp cycle guard). `SimulatedClock` (in Rust, C, C++, and WASM) owns the virtual timestamp in tests/simulations and implements `.set(target_ms)` and `.increment(dt_ms)` by stepping its clock to each `next_timer_deadline() <= target_ms` (`t_fire`) and invoking `tick()` while `clock.now() == t_fire`, before settling at `target_ms`.

**Tech Stack:** Rust (`#![no_std]` with `core` + `alloc`), C99/C11 FFI, C++11 RAII header wrapper, WebAssembly (`wasm-bindgen` / `js-sys`), Vitest.

**Spec:** `docs/superpowers/specs/2026-11-05-virtual-time-advancement-timers-design.md`

## Global Constraints

- `#![no_std]` compliance: Both `portable-xstate-core` and `portable-xstate-ffi` must remain `#![no_std]` (using `core` and `alloc` only).
- Zero warnings under `cargo clippy` across all workspace crates (`portable-xstate-core`, `portable-xstate-ffi`, `portable-xstate-wasm`).
- Backward compatibility: Existing `fsm_register_clock(ctx, fn_ptr)` + `fsm_tick(ctx)`, `PortableXState::registerClock(fn_ptr)` + `tick()`, and WASM `actor.tick()` remain 100% source-compatible.
- Tests as Source of Truth: Never modify existing upstream XState 5 conformance test logic in `tests/after.test.ts`.
- Zero regressions across the full test suite (`npm test`).

## Review Focus

- **Guarded `after` transition where all branch guards evaluate to `false` at `t_fire`**: The expired timer must still be removed at `t_fire` so `next_timer_deadline()` advances to the next scheduled deadline (such as a parent timeout) rather than spinning infinitely at `t_fire`.
- **Calling `SimulatedClock::set(initial_ms)` before `machine.start()`**: Must set `clock.now()` to `initial_ms` without firing unstarted timers, so `machine.start()` anchors initial `after` timers at `initial_ms + delay`.
- **Reentrant `send` / `transition` inside an action callback during an intermediate `after` firing**: Must execute at `t_fire` before the next timer deadline is queried, so any new `after` timers armed by the reentrant event are anchored at `t_fire + delay` rather than `t_end + delay`.
- **Moving a C++ `xstate::PortableXState` instance after `registerClock(SimulatedClock&)`**: Move constructor and move assignment must update `SimulatedClock`'s `m_fsm` pointer and re-register the `&m_simClock->m_clock` pointer with `fsm_register_simulated_clock` so `clock.increment()` ticks the moved instance safely.
- **High-frequency `after: 1` child polling inside an `after: 60000` parent timeout (`StartNetworkTimeSync`)**: A single `clock.increment(60_000)` call must execute all 59,999 child polls at `t = 1..=59,999` and trigger the parent timeout at `t = 60,000` without being cut off by the 100-step same-timestamp (`after: 0`) cycle guard.

---

## Track & Phase Hierarchy [flowos: 9b2865f3-6614-4199-a583-d3021362289b]

### Phase 1: Core, FFI, and WASM SimulatedClock Implementation [flowos: 5d3c18b3-31ea-4150-896b-2d568afb7f0b]

---

### Task 1: Core Engine Timer Ordering, Next Deadline Query & `SimulatedClock` (`portable-xstate-core`) [flowos: 4869f53e-9d28-428f-91c6-17b06f02709e]

**Files:**
- Modify: `portable-xstate-core/src/lib.rs:818-895`
- Test: `portable-xstate-core/src/repro_test.rs`

**Interfaces:**
- Consumes: `RuntimeMachine`, `GuardFunction`, `ActionFunction`, `StateValue`, `Value` in `portable-xstate-core/src/lib.rs`
- Produces:
  - `RuntimeMachine::next_timer_deadline(&self) -> Option<u64>`
  - `RuntimeMachine::tick(&mut self, guards: &IndexMap<String, GuardFunction>, actions: &IndexMap<String, ActionFunction>) -> Option<StateValue>` (chronological earliest-deadline selection, FIFO tie-breaking, `"xstate.after"` event object, reentrant external queue drain per step, 100-step same-tick bound)
  - `pub struct SimulatedClock` in `portable-xstate-core` with:
    - `pub fn new() -> Self`
    - `pub fn with_initial(initial_ms: u64) -> Self`
    - `pub fn now(&self) -> u64`
    - `pub fn set_now_raw(&self, now_ms: u64)`
    - `pub fn as_clock_fn(&self) -> Rc<dyn Fn() -> u64>`
    - `pub fn set(&self, target_ms: u64, machine: &mut RuntimeMachine, guards: &IndexMap<String, GuardFunction>, actions: &IndexMap<String, ActionFunction>) -> Option<StateValue>`
    - `pub fn increment(&self, dt_ms: u64, machine: &mut RuntimeMachine, guards: &IndexMap<String, GuardFunction>, actions: &IndexMap<String, ActionFunction>) -> Option<StateValue>`

- [ ] **Step 1: Write the failing Rust unit tests in `portable-xstate-core/src/repro_test.rs`**

Append tests to `portable-xstate-core/src/repro_test.rs` covering:
1. `test_simulated_clock_cascading_timers_wait_to_sleep`: Machine starts in `Wait` (`after: { 60000: "Sleep" }`), `Sleep` (`after: { 3600000: "Wake" }`). Calling `clock.increment(86_400_000, &mut machine, ...)` fires `Wait -> Sleep` at `t = 60,000` (anchoring `Sleep`'s timer at `60,000 + 3,600,000 = 3,660,000`), fires `Sleep -> Wake` at `t = 3,660,000`, and settles `clock.now() == 86_400_000` in state `"Wake"`.
2. `test_simulated_clock_hierarchical_polling_and_parent_timeout_tie_break`: Compound state `ConnectToNetwork` (`after: { 300000: "Timeout" }`, `initial: "Polling"`) with child `Polling` (`entry: ["poll"]`, `after: { 500: [{ target: "#Connected", guard: "isConnected" }, { target: "Polling", reenter: true }] }`).
   - Case A (guard stays `false`): `clock.increment(300_000, ...)` invokes `poll` at `t = 0, 500, 1000, ..., 299_500` (600 times total) and at `t = 300_000` fires the parent timeout (`ConnectToNetwork -> Timeout`) before a 601st poll, ending in `"Timeout"` with `clock.now() == 300_000`.
   - Case B (guard becomes `true` on 10th poll at `t = 4,500`): `clock.increment(300_000, ...)` transitions to `"Connected"` at `t = 5,000`, cancels `ConnectToNetwork`'s `300,000` timer, and settles at `clock.now() == 300_000` in `"Connected"` with `poll_count == 10`.
3. `test_simulated_clock_high_frequency_polling_with_parent_timeout`: `StartNetworkTimeSync` (`after: { 60000: "SyncTimeout" }`) with `Polling` (`after: { 1: { target: "Polling", reenter: true } }`, `entry: ["tickPoll"]`). `clock.increment(60_000, ...)` runs all 60,000 polls (`t = 0..=59,999`) and transitions to `"SyncTimeout"` at `t = 60,000`.
4. `test_simulated_clock_zero_delay_loop_is_bounded`: State `Loop` (`after: { 0: { target: "Loop", reenter: true } }`, `entry: ["inc"]`). `clock.increment(1000, ...)` terminates without hanging and settles `clock.now() == 1000`.
5. `test_simulated_clock_set_before_start_anchors_initial_timers` & `test_simulated_clock_reentrant_transition_during_after_anchors_at_t_fire`: `clock.set(2500, ...)` before `machine.start(...)` anchors a `500 ms` initial timer at `3000 ms`; and an action during an `after` firing at `t = 3000` that calls `machine.transition("REENTRANT", ...)` enters a state whose `after: 1000` timer fires at `t = 4000` during the same `clock.set(5000, ...)` call.

- [ ] **Step 2: Run `cargo test --package portable-xstate-core` to verify compilation fails**

Run: `cargo test --package portable-xstate-core`
Expected: FAIL with unresolved `SimulatedClock` and `next_timer_deadline`.

- [ ] **Step 3: Implement `next_timer_deadline`, updated `tick`, and `SimulatedClock` in `portable-xstate-core/src/lib.rs`**

In `portable-xstate-core/src/lib.rs`:
1. Factor a private helper `drain_reentrant_external_queue(&mut self, guards: &IndexMap<String, GuardFunction>, actions: &IndexMap<String, ActionFunction>) -> Option<StateValue>` that drains `self.external_queue` (with a 100-iteration loop guard) while `self.is_processing` is already `true`, and call it from `drain_external_queue` and inside each step of `tick`.
2. Add `pub fn next_timer_deadline(&self) -> Option<u64>` to `RuntimeMachine`.
3. Update `RuntimeMachine::tick` to:
   - Construct `after_event` with `"type": Value::String("xstate.after".to_string())`.
   - Loop with `steps < 100`, selecting the expired timer via `.filter(|(_, (_, deadline, _))| *deadline <= now).min_by_key(|(_, (_, deadline, _))| *deadline)`.
   - After `transition_internal` and `drain_internal_queue`, call `self.drain_reentrant_external_queue(guards, actions)` inside the loop so reentrant `transition()` calls take effect at `now` (`t_fire`).
4. Implement `pub struct SimulatedClock` in `portable-xstate-core/src/lib.rs` with `new`, `with_initial`, `now`, `set_now_raw`, `as_clock_fn`, `set`, and `increment` per Section 4.2 of the spec.

- [ ] **Step 4: Run `cargo test --package portable-xstate-core` and `cargo clippy --package portable-xstate-core`**

Run: `cargo test --package portable-xstate-core && cargo clippy --package portable-xstate-core -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 5: Commit Task 1**

```bash
git add portable-xstate-core/src/lib.rs portable-xstate-core/src/repro_test.rs
git commit -m "feat(core): add SimulatedClock, next_timer_deadline, and chronological timer firing"
```

---

### Task 2: C & C++ FFI `SimulatedClock`, `user_data` Clock Registration & `getNextDeadline` (`portable-xstate-ffi`) [flowos: a44e477f-fa1b-43dc-93ff-a55bcde9b0ce]

**Files:**
- Modify: `portable-xstate-ffi/src/lib.rs:25-195`
- Modify: `portable-xstate-ffi/include/portable_xstate.h:49-80`
- Modify: `portable-xstate-ffi/include/portable_xstate.hpp:53-245`
- Test: `portable-xstate-ffi/src/lib.rs`, `portable-xstate-ffi/tests/c_test.c`, `portable-xstate-ffi/tests/cpp_test.cpp`

**Interfaces:**
- Consumes: `RuntimeMachine::next_timer_deadline`, `RuntimeMachine::tick` from Task 1 (`portable-xstate-core`)
- Produces:
  - C (`portable_xstate.h` & `portable-xstate-ffi/src/lib.rs`):
    - `typedef uint64_t (*fsm_clock_with_data_callback_t)(void* user_data);`
    - `int fsm_register_clock_with_data(MachineContext* ctx, fsm_clock_with_data_callback_t callback, void* user_data);`
    - `bool fsm_get_next_deadline(const MachineContext* ptr, uint64_t* out_deadline);`
    - `typedef struct { uint64_t now_ms; } fsm_simulated_clock_t;`
    - `void fsm_simulated_clock_init(fsm_simulated_clock_t* clock, uint64_t initial_ms);`
    - `int fsm_register_simulated_clock(MachineContext* ctx, fsm_simulated_clock_t* clock);`
    - `uint64_t fsm_simulated_clock_now(const fsm_simulated_clock_t* clock);`
    - `int fsm_simulated_clock_set(MachineContext* ctx, fsm_simulated_clock_t* clock, uint64_t target_ms);`
    - `int fsm_simulated_clock_increment(MachineContext* ctx, fsm_simulated_clock_t* clock, uint64_t dt_ms);`
  - C++ (`portable_xstate.hpp`):
    - `using ClockCallback = std::function<uint64_t()>;`
    - `class SimulatedClock` with `SimulatedClock(uint64_t initial_ms = 0)`, `uint64_t now() const noexcept`, `void set(uint64_t target_ms)`, `void increment(uint64_t dt_ms)`
    - `PortableXState::registerClock(fsm_clock_callback_t callback)`
    - `PortableXState::registerClock(ClockCallback callback)`
    - `PortableXState::registerClock(SimulatedClock& clock)`
    - `bool PortableXState::getNextDeadline(uint64_t& out_deadline) const`

- [ ] **Step 1: Write the failing FFI tests in `portable-xstate-ffi/src/lib.rs`, `c_test.c`, and `cpp_test.cpp`**

1. In `portable-xstate-ffi/src/lib.rs` (`mod tests`), add `test_simulated_clock_hierarchical_and_reentrant_ffi` testing `fsm_simulated_clock_init`, `fsm_register_simulated_clock`, `fsm_simulated_clock_increment`, `fsm_simulated_clock_set`, `fsm_get_next_deadline`, and reentrant `fsm_send_event` inside an `after` action callback.
2. In `portable-xstate-ffi/tests/c_test.c`, add a test section verifying `fsm_simulated_clock_t` advancing across a compound state with `Polling` (`after: 500`, `reenter: true`) and parent timeout (`after: 300000`), plus cascading `Wait` (`after: 60000`) -> `Sleep` (`after: 3600000`) across an `86,400,000 ms` increment.
3. In `portable-xstate-ffi/tests/cpp_test.cpp`, add tests verifying:
   - `xstate::SimulatedClock` with `PortableXState::registerClock(clock)`, `clock.increment(60000)`, `clock.set(86460000)`, and `clock.now()` inspected inside C++ guard/action lambdas at intermediate `t_fire` steps.
   - Moving `xstate::PortableXState` (`xstate::PortableXState moved = std::move(fsm);`) after `fsm.registerClock(clock)` and verifying `clock.increment(...)` advances `moved`.

- [ ] **Step 2: Run `npm run test:ffi` to verify failure**

Run: `npm run test:ffi`
Expected: FAIL with missing C/C++ symbols and `SimulatedClock` definitions.

- [ ] **Step 3: Implement C FFI functions and C++ `SimulatedClock` wrapper**

1. In `portable-xstate-ffi/src/lib.rs`:
   - Update `fsm_tick` to call `let _ = ctx.machine.tick(&ctx.guards, &ctx.actions);` once (removing the outer `while let Some(_) = ...` loop).
   - Implement `fsm_register_clock_with_data`, `fsm_get_next_deadline`, `fsm_simulated_clock_init`, `fsm_register_simulated_clock`, `fsm_simulated_clock_now`, `fsm_simulated_clock_set`, and `fsm_simulated_clock_increment`.
2. In `portable-xstate-ffi/include/portable_xstate.h`:
   - Declare all new C types and functions.
3. In `portable-xstate-ffi/include/portable_xstate.hpp`:
   - Define `xstate::SimulatedClock` and `PortableXState` methods (`registerClock` overloads, `getNextDeadline`, and move/destructor updating `m_simClock`).

- [ ] **Step 4: Run `cargo test --package portable-xstate-ffi`, `npm run test:ffi`, and `cargo clippy --package portable-xstate-ffi`**

Run: `cargo test --package portable-xstate-ffi && npm run test:ffi && cargo clippy --package portable-xstate-ffi -- -D warnings`
Expected: PASS (`C Test Passed!`, `C++ Test Passed!`, 0 warnings).

- [ ] **Step 5: Commit Task 2**

```bash
git add portable-xstate-ffi/src/lib.rs portable-xstate-ffi/include/portable_xstate.h portable-xstate-ffi/include/portable_xstate.hpp portable-xstate-ffi/tests/c_test.c portable-xstate-ffi/tests/cpp_test.cpp
git commit -m "feat(ffi): add C/C++ SimulatedClock, clock user_data, and getNextDeadline"
```

---

### Task 3: WASM Adapter `SimulatedClock` & `getNextDeadline` (`portable-xstate-wasm`) [flowos: c9d00007-a2b8-4001-bb02-acc79549dd1c]

**Files:**
- Modify: `portable-xstate-wasm/src/lib.rs:480-525, 630-645, 1070-1085, 1445-1462, 1566-1605`
- Modify: `XSTATE_5_FEATURES.md:38-41, 126`

**Interfaces:**
- Consumes: `portable_xstate_core::SimulatedClock`, `RuntimeMachine::next_timer_deadline`, `RuntimeMachine::tick` from Task 1
- Produces:
  - `#[wasm_bindgen] pub struct SimulatedClock` in `portable-xstate-wasm` with `new()`, `set(timestamp: f64)`, `increment(ms: f64)`, `now() -> f64`
  - `Actor::get_next_deadline(&self) -> Option<f64>` (`js_name = getNextDeadline`)
  - Support for `{ clock }` option in `createActor(machine, { clock })` and `StateNode::create_actor_method`
  - Chronological multi-step advancement in `Actor::tick()` when `Date::now()` jumps forward under fake timers (`vi.advanceTimersByTime(dt)`)

- [ ] **Step 1: Write a WASM test in `portable-xstate-wasm/tests/verification.rs` (or `portable-xstate-wasm/src/lib.rs`)**

Check existing WASM tests in `portable-xstate-wasm/tests/` and add a test verifying:
1. `SimulatedClock` with `createActor(machine, { clock })`: `clock.increment(60_000)` and `clock.set(120_000)` across a hierarchical polling + timeout machine.
2. `actor.tick()` after `Date.now()` advances across multiple timer expirations.

- [ ] **Step 2: Run WASM build & tests to verify failure**

Run: `npm run build:npm && npx vitest run tests/after.test.ts`
Expected: New `SimulatedClock` test fails until implemented.

- [ ] **Step 3: Implement `SimulatedClock` and `getNextDeadline` in `portable-xstate-wasm/src/lib.rs` and update `XSTATE_5_FEATURES.md`**

In `portable-xstate-wasm/src/lib.rs`:
1. Export `#[wasm_bindgen] pub struct SimulatedClock` backed by a shared inner state so `clock.set(timestamp)` and `clock.increment(ms)` step any bound `Actor` through intermediate deadlines and emit snapshots on transitions.
2. Update `Actor::tick(&mut self)` so if the registered clock (`Date::now()`) has jumped ahead past multiple timer deadlines, it steps chronologically through each `next_timer_deadline() <= target_now` without the old `count > 100` truncation bug.
3. Add `#[wasm_bindgen(js_name = getNextDeadline)] pub fn get_next_deadline(&self) -> Option<f64>` on `Actor`.
4. Update `XSTATE_5_FEATURES.md` to document `SimulatedClock` / custom clock support.

- [ ] **Step 4: Run full verification suite (`cargo clippy` and `npm test`)**

Run: `cargo clippy --all-targets -- -D warnings && npm run build && npm test`
Expected: All Rust native, WASM, Playwright E2E, C/C++ FFI, and Vitest unit tests pass with 0 warnings.

- [ ] **Step 5: Commit Task 3**

```bash
git add portable-xstate-wasm/ XSTATE_5_FEATURES.md
git commit -m "feat(wasm): expose SimulatedClock, getNextDeadline, and multi-step tick"
```
