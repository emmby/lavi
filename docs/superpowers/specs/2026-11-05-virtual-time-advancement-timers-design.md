# Design Specification: Advancing Time Across Multiple Timer Expirations (`tick_at` & `advance`)

**Date:** 2026-11-05  
**Status:** Draft for Review  

---

## 1. Intent and Why

Statecharts that use delayed transitions (`after`) frequently combine:
1. **Hierarchical timeouts and polling loops**: A parent compound state with a long timeout (e.g., `ConnectToNetwork` with `after: 300000`, or `StartNetworkTimeSync` with `after: 60000`) containing a child state with a short polling self-transition (e.g., `Polling` with `after: 500` or `after: 1`).
2. **Cascading delayed states**: Entering a state via an `after` transition arms a new `after` timer on the destination state within the same time advancement window (e.g., advancing `86,400,000 ms` [24 hours] while in `Wait` [`after: 60000`], which transitions into `Sleep` at `t + 60,000 ms` and leaves `86,340,000 ms` of elapsed time to apply against `Sleep`'s newly armed `after` timer).

Previously, `portable-xstate` suffered from two core engine limitations in `RuntimeMachine::tick()`:
- **Mis-anchored cascading timers**: When a transition fired inside `tick()`, newly entered states queried `clock_millis_fn()` (`t_end`, the end of the jump) instead of the firing timer's expiration deadline (`t_fire`), scheduling subsequent timers at `t_end + delay` instead of `t_fire + delay`.
- **Non-chronological timer selection**: `tick()` selected expired timers in reverse insertion order (`.rev().find(|(_, timestamp, _)| now >= *timestamp)`) rather than earliest deadline (`min(deadline)`), causing out-of-order evaluation when multiple timers expired within the same time jump.

Additionally, callers had to maintain a separate external clock variable and manually sub-step `tick()` based on out-of-band knowledge of the statechart's internal `after` delays.

At the same time, an API based *solely* on relative time steps (`advance(dt_ms)`) introduces a risk of cumulative clock drift in real-time/embedded loops (such as an ESP32 `loop()` using `millis()`). Therefore, following the spirit of XState 5's `SimulatedClock` (`clock.set(timestamp)` and `clock.increment(ms)`), `portable-xstate` will track the machine's current monotonic timestamp (`now_ms`) internally and provide **both**:
1. **Absolute timestamp advancement (`tick_at` / `tickAt`)** as the primary, drift-free primitive.
2. **Relative delta advancement (`advance`)** as an ergonomic convenience built directly on `tick_at(self.now_ms + dt_ms)`.
3. **Callback-driven `tick()`** upgraded to delegate to `tick_at(clock())` so existing callers using `register_clock` / `registerClock` / `vi.advanceTimersByTime` automatically gain exact multi-step chronological timer execution.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **Drift-free absolute time primitive (`tick_at` / `tickAt`)**: Allow callers across Rust, C, C++, and WASM/JS to pass the current monotonic timestamp (`now_ms`) and advance the state machine across any time window `[t_start, t_end]` in a single call.
- **Ergonomic relative time wrapper (`advance`)**: Allow virtual-time and unit-test callers to advance by an elapsed duration (`dt_ms`) in a single call without maintaining an external timestamp variable.
- **Exact chronological firing & `t_fire` anchoring**: Fire all active `after` timers with `deadline <= target_now_ms` one by one in strict chronological deadline order (`deadline_a < deadline_b`), advancing the machine's internal clock to `t_fire` at each step so newly armed `after` timers are anchored at `t_fire + delay`.
- **Hierarchical timer isolation & precedence**: Ensure child self-transitions (`Polling -> Polling`) only exit/re-enter the child state and reset the child's `after` timer while preserving any active parent `after` timer deadline. When a parent timeout and a re-armed child timer share the exact same deadline (`deadline_a == deadline_b`), FIFO registration order fires the earlier-registered parent timeout first.
- **Dynamic guard & action evaluation**: Re-evaluate registered C/C++/Rust/JS guard and action callbacks (plus any resulting `always` transitions, raised internal events, and reentrant external `send` calls) at each intermediate `t_fire` step before determining the next timer deadline.
- **Safe bounding of zero-delay cycles**: Prevent `after: 0` (same-timestamp) infinite self-transition loops from hanging without prematurely cutting off legitimate high-frequency positive-delay advances (such as `60,000` steps of an `after: 1` child timer inside an `after: 60000` parent timeout).

### Non-Goals
- Evaluating dynamic delay expressions via runtime JS callbacks (`after: { myDynamicDelayFn: ... }`).
- Spawning background OS threads or asynchronous platform timers inside `portable-xstate-core`.

### Anti-Goals
- Requiring callers to know state names, compound hierarchies, or `after` durations configured in JSON in order to advance time accurately.
- Forcing real-time embedded callers to pass relative `dt_ms` steps that could accumulate drift against hardware `millis()`.
- Breaking backward compatibility with existing `fsm_register_clock` / `fsm_tick`, `PortableXState::registerClock` / `tick()`, or WASM `actor.tick()` callers.

---

## 3. Constraints

- `#![no_std]` compliance: Both `portable-xstate-core` and `portable-xstate-ffi` must remain `#![no_std]` (using `core` and `alloc` only).
- Zero warnings under `cargo clippy` across all workspace crates (`portable-xstate-core`, `portable-xstate-ffi`, `portable-xstate-wasm`).
- Zero regressions across the full test suite (`npm test`: Rust native tests, WASM headless browser tests, Playwright E2E tests, C and C++ FFI integration tests, and Vitest XState 5 conformance tests).

---

## 4. Technical Design

### 4.1 Cross-Language API Surface

Consistent naming is used across all language bindings (`snake_case` in Rust and C; `camelCase` in C++ and JS/WASM):

| Target | Absolute Time (Primary, Drift-Free) | Relative Delta (Convenience) | Registered Clock Callback (Existing) |
| :--- | :--- | :--- | :--- |
| **Rust (`RuntimeMachine`)** | `machine.tick_at(now_ms, &guards, &actions)` | `machine.advance(dt_ms, &guards, &actions)` | `machine.register_clock(cb)` + `machine.tick(&guards, &actions)` |
| **C (`portable_xstate.h`)** | `int fsm_tick_at(MachineContext* ptr, uint64_t now_ms)` | `int fsm_advance(MachineContext* ptr, uint64_t dt_ms)` | `fsm_register_clock(ptr, cb)` + `int fsm_tick(MachineContext* ptr)` |
| **C++ (`xstate::PortableXState`)** | `void tickAt(uint64_t now_ms)` | `void advance(uint64_t dt_ms)` | `registerClock(cb)` + `void tick()` |
| **WASM / JS (`Actor`)** | `actor.tickAt(nowMs: number)` | `actor.advance(dtMs: number)` | `actor.tick()` |

Additionally, `RuntimeMachine` exposes `pub fn now_ms(&self) -> u64` (and `pub now_ms: u64` field) for inspecting the machine's current timestamp.

### 4.2 Internal Clock Ownership & Lifecycle (`RuntimeMachine`)

`RuntimeMachine` in `portable-xstate-core/src/lib.rs` will store:
- `pub now_ms: u64`: The machine's current monotonic timestamp in milliseconds, initialized to `0` in `RuntimeMachine::from_definition`.

**Clock synchronization rules across lifecycle methods:**
1. **Pre-start seeding (`tick_at` before `start`)**:
   - If `tick_at(now_ms, ...)` is called before `start()`, `self.now_ms` is updated to `now_ms` and the call returns `None`. This allows embedded callers using `tick_at(millis())` without `register_clock` to seed the boot timestamp before calling `start()`, or they can simply pass `millis()` via `register_clock`.
2. **`start(&mut self, guards, actions)`**:
   - If `self.clock_millis_fn` is `Some(ref cb)`, update `self.now_ms = cb()`. Otherwise keep the current `self.now_ms` (default `0`, or pre-seeded value).
   - All initial `after` timers (on the root machine and initial active state hierarchy) are scheduled with `deadline = self.now_ms.saturating_add(*delay)`.
3. **`revive_timers(&mut self)`**:
   - If `self.clock_millis_fn` is `Some(ref cb)`, update `self.now_ms = cb()`.
   - Re-arms active state timers at `self.now_ms.saturating_add(*delay)`.
4. **`transition(&mut self, event_type, event_payload, guards, actions)` (External `send`)**:
   - If `!self.is_processing` (i.e. a top-level external event, not a reentrant `send` emitted inside an action callback during `tick_at` or `start`), and `self.clock_millis_fn` is `Some(ref cb)`, update `self.now_ms = cb()`.
   - Inside `transition_internal`, whenever states are entered, their `after` timers are armed at `self.now_ms.saturating_add(*delay)` (**never** querying `clock_millis_fn` directly inside `transition_internal`).
5. **`tick(&mut self, guards, actions)`**:
   - Queries `let target_now = self.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(self.now_ms);`
   - Delegates directly to `self.tick_at(target_now, guards, actions)`.
6. **`advance(&mut self, dt_ms, guards, actions)`**:
   - Computes `let target_now = self.now_ms.saturating_add(dt_ms);`
   - Delegates directly to `self.tick_at(target_now, guards, actions)`.

### 4.3 Chronological Multi-Step Timer Execution (`tick_at`)

When `RuntimeMachine::tick_at(&mut self, target_now_ms: u64, guards, actions) -> Option<StateValue>` is called:

1. **Pre-checks**:
   - If `!self.is_started`: set `self.now_ms = target_now_ms` and return `None`.
   - If `self.is_processing`: return `None` (guards against reentrant `tick` calls from inside callbacks).
   - Set `self.is_processing = true`.

2. **Chronological Discrete-Event Loop**:
   - Maintain:
     - `last_ret: Option<StateValue> = None`
     - `last_fire_ms: Option<u64> = None`
     - `same_timestamp_steps: usize = 0` (`MAX_SAME_TIMESTAMP_STEPS = 100`)
     - `total_steps: usize = 0` (`MAX_TOTAL_TIMER_STEPS = 1_000_000`)
   - In each iteration:
     1. **Select earliest expired timer**:
        Find the timer in `self.active_timers` with `deadline <= target_now_ms` that has the **minimum `deadline`**:
        ```rust
        let expired = self
            .active_timers
            .iter()
            .enumerate()
            .filter(|(_, (_, deadline, _))| *deadline <= target_now_ms)
            .min_by_key(|(_, (_, deadline, _))| *deadline)
            .map(|(idx, (path, deadline, t_defs))| (idx, path.clone(), *deadline, t_defs.clone()));
        ```
        Because `Iterator::min_by_key` in Rust returns the **first** element when multiple elements tie for the minimum key, and `self.active_timers` is appended in registration order, ties on identical `deadline` are broken by **FIFO registration order** (the timer armed earlier fires first).
     2. If `expired` is `None`, break out of the loop.
     3. **Loop safety checks**:
        - If `last_fire_ms == Some(deadline)`:
          - Increment `same_timestamp_steps`.
          - If `same_timestamp_steps >= 100`, break (prevents infinite `after: 0` self-transition hangs).
        - Else:
          - `last_fire_ms = Some(deadline)`
          - `same_timestamp_steps = 0`
        - Increment `total_steps`; if `total_steps > 1_000_000`, break.
     4. **Advance internal clock to `t_fire`**:
        - Set `self.now_ms = deadline`.
     5. **Consume timer & execute transition**:
        - Remove `idx` from `self.active_timers` (`self.active_timers.remove(idx)`).
        - Construct synthetic event `after_event`: `Value::Object` with `"type": Value::String("xstate.after".to_string())` (so C/C++ callbacks inspecting the `type` property of `event` receive a valid string).
        - Call `self.transition_internal("", &after_event, guards, actions, 0, Some((&path, &t_defs)))`.
          - Note: During `transition_internal`, any exited states have their active timers removed (`self.active_timers.retain(|(p, _, _)| !exited_paths.contains(p))`), any entered states have their `after` timers armed at `self.now_ms + delay` (`t_fire + delay`), and any `always` (eventless) transitions are settled.
        - Drain `self.internal_queue` via `self.drain_internal_queue(guards, actions)`.
        - Drain any reentrant events in `self.external_queue` that were enqueued by action callbacks during this step (while keeping `self.is_processing = true`), so their state changes and newly armed timers are also anchored at `t_fire`.
        - Update `last_ret` if any transition occurred.

3. **Settle Clock at `target_now_ms`**:
   - After the loop exits, if `target_now_ms > self.now_ms`, set `self.now_ms = target_now_ms`.
   - Set `self.is_processing = false`.
   - Drain any remaining `external_queue` events and return `last_ret`.

### 4.4 Self-Transitions on `after` Timers (`Polling -> Polling`)

In statecharts authored in JSON (including XState v4-style JSON or hand-authored statecharts like `AprilClockFSM`), polling child states are commonly written either with or without an explicit `"reenter": true` property:
```json
"Polling": {
  "after": {
    "500": "Polling"
  }
}
```
or with guarded branches:
```json
"Polling": {
  "after": {
    "500": [
      { "target": "#Connected", "guard": "isConnected" },
      { "target": "Polling" }
    ]
  }
}
```
Wait: In `portable-xstate-core/src/parser.rs`, `parse_transition_object` and `parse_single_transition` currently default `reenter` to `false`. For `on` event transitions, XState 5 defaults `reenter` to `false` (internal transition unless `reenter: true` is passed). However, for an `after` transition that specifies a `target` resolving to its own source state (`target_full == current_path`), an internal transition (`reenter: false`) would neither exit nor re-enter the state, which means the `after` timer that just expired would **never be re-armed** unless `reenter` is true.
- Therefore, in `parse_after` (in `portable-xstate-core/src/parser.rs`), transitions parsed under `after` that specify a `target` (and do not explicitly set `"reenter": false`) will default `reenter` to `true` when `target_full == current_path` (or `force_transition.is_some()` in `transition_internal` when `dom_lca == current_parts.len() && dom_lca == target_parts.len()`).
- Because `domain_depth` for `Polling -> Polling` with re-entry is `current_parts.len() - 1` (the parent state `ConnectToNetwork`), only `Polling` (`i = current_parts.len()`) is exited and re-entered. `ConnectToNetwork` is **not** exited, so `ConnectToNetwork`'s `after: 300000` timer remains intact at its original deadline while `Polling`'s `after: 500` timer is re-armed at `t_fire + 500`.

### 4.5 Adapter Updates (`portable-xstate-ffi` and `portable-xstate-wasm`)

1. **`portable-xstate-ffi/src/lib.rs`**:
   - Update `fsm_tick` to call `ctx.machine.tick(&ctx.guards, &ctx.actions);` once (since `RuntimeMachine::tick` now loops internally until all timers `<= now` have settled, removing the redundant unbounded `while let Some(_) = ctx.machine.tick(...) {}` loop in `fsm_tick`).
   - Add `#[no_mangle] pub extern "C" fn fsm_tick_at(ptr: *mut MachineContext, now_ms: u64) -> c_int`.
   - Add `#[no_mangle] pub extern "C" fn fsm_advance(ptr: *mut MachineContext, dt_ms: u64) -> c_int`.
2. **`portable-xstate-ffi/include/portable_xstate.h` & `portable_xstate.hpp`**:
   - Declare `int fsm_tick_at(MachineContext* ptr, uint64_t now_ms);` and `int fsm_advance(MachineContext* ptr, uint64_t dt_ms);` in `portable_xstate.h`.
   - Add `void tickAt(uint64_t now_ms)` and `void advance(uint64_t dt_ms)` to `xstate::PortableXState` in `portable_xstate.hpp`.
3. **`portable-xstate-wasm/src/lib.rs`**:
   - Update `Actor::tick(&mut self)` to call `self.machine.tick(&self.guards, &self.actions)` once (removing the outer `while` loop with the `count > 100` cap that previously truncated multi-step timer runs at 100 steps) and emit a snapshot if a transition occurred.
   - Add `#[wasm_bindgen(js_name = tickAt)] pub fn tick_at(&mut self, now_ms: f64) -> Result<(), JsValue>` and `#[wasm_bindgen] pub fn advance(&mut self, dt_ms: f64) -> Result<(), JsValue>` on `Actor`.

---

## 5. What Is Left to the Builder

- Structuring the unit and integration tests across:
  - `portable-xstate-core/src/repro_test.rs` (Rust unit tests covering cascading timers, hierarchical parent timeout + fast child polling, dynamic guards changing mid-window, `after: 0` cycle termination, and `tick_at` vs `advance`).
  - `portable-xstate-ffi/src/lib.rs`, `portable-xstate-ffi/tests/c_test.c`, and `portable-xstate-ffi/tests/cpp_test.cpp` (verifying `fsm_tick_at`, `fsm_advance`, `PortableXState::tickAt`, and `PortableXState::advance`).
  - `tests/after.test.ts` (or WASM tests).
- Minor implementation details in helper functions within `RuntimeMachine` (such as factoring the external queue draining helper so both `drain_external_queue` and `tick_at` share the same step-drain logic).
