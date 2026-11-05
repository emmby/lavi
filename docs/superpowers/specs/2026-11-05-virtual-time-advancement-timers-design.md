# Design Specification: External Clock & `SimulatedClock` Time Advancement in `portable-xstate`

**Date:** 2026-11-05  
**Status:** Draft for Review  

---

## 1. Intent and Why

Statecharts that use delayed transitions (`after`) frequently combine:
1. **Hierarchical timeouts and polling loops**: A parent compound state with a long timeout (e.g., `ConnectToNetwork` with `after: 300000`, or `StartNetworkTimeSync` with `after: 60000`) containing a child state with a short polling self-transition (e.g., `Polling` with `after: 500` or `after: 1` and `reenter: true`).
2. **Cascading delayed states**: Entering a state via an `after` transition arms a new `after` timer on the destination state within the same time advancement window (e.g., advancing `86,400,000 ms` [24 hours] while in `Wait` [`after: 60000`], which transitions into `Sleep` at `t + 60,000 ms` and leaves `86,340,000 ms` of elapsed time to apply against `Sleep`'s newly armed `after` timer).

Previously, advancing virtual time in consumers such as `AprilClockFSM` required callers to hardcode statechart-specific sub-stepping because:
1. **Jumping an external clock variable in one leap loses intermediate `t_fire` timestamps**: If a caller sets `now_ms += 60000` before calling `tick()`, the registered clock callback already returns `t_end` (`60000`) when intermediate transitions at `t_fire = 500` execute—causing newly entered states to arm their `after` timers at `t_end + delay` (`60,500`) instead of `t_fire + delay` (`1,000`), and causing any action/guard callbacks inspecting the clock to see `t_end` instead of `t_fire`.
2. **Non-chronological timer selection in `RuntimeMachine::tick()`**: `tick()` previously selected expired timers in reverse insertion order (`.rev().find(|(_, timestamp, _)| now >= *timestamp)`) rather than earliest deadline (`min(deadline)`), evaluating timers out of chronological order when multiple timers were expired.
3. **Lack of `user_data` on C/C++ clock callbacks**: `fsm_clock_callback_t` was a bare `uint64_t (*)(void)` without `void* user_data`, forcing C++ callers to use global/static variables for virtual clocks instead of per-instance clock objects.

To stay strictly aligned with XState 5's architecture while keeping the state machine free of internal clock ownership:
- **`RuntimeMachine` never owns a clock variable**: It relies 100% on the registered external clock callback as the single source of truth for time.
- **`RuntimeMachine` exposes `next_timer_deadline()`** (`getNextDeadline` in C++/WASM, `fsm_get_next_deadline` in C) so external schedulers, embedded sleep loops, and simulated clocks can query the exact timestamp of the next scheduled `after` timer expiration.
- **`SimulatedClock` (modeled directly on XState 5's `SimulatedClock`)** owns the virtual timestamp in tests/simulations and provides `.set(target_ms)` (absolute time, drift-free) and `.increment(dt_ms)` (relative step). When advancing across `[t_start, t_end]`, `SimulatedClock` steps its own timestamp to each intermediate `next_deadline <= t_end` (`t_fire`) and invokes `tick()` while the external clock is at `t_fire`, before finally settling at `t_end`.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **Consistency with XState 5 (`Clock` & `SimulatedClock`)**: Follow XState 5's separation of concerns where the Actor/FSM delegates time entirely to an external `Clock`, real-time callers bind a system clock (`millis()`, `Date.now()`), and virtual-time callers use `SimulatedClock` (`clock.set(timestamp)` and `clock.increment(ms)`).
- **Single external source of truth for time**: `RuntimeMachine` does not maintain a shadow clock counter. At every intermediate transition `t_fire` during simulated time advancement, `SimulatedClock::now()` (and therefore the FSM's registered clock callback and any user action/guard reading `clock.now()`) returns the exact `t_fire`.
- **Exact chronological firing & cascading `t_fire` anchoring**: Fire all active `after` timers in `[t_start, t_end]` one by one in strict chronological deadline order (`deadline_a < deadline_b`), breaking ties on identical deadlines by FIFO registration order (earliest armed timer fires first).
- **Hierarchical timer isolation & precedence**: Ensure child self-transitions (`Polling -> Polling` with `reenter: true`) only exit/re-enter the child state and reset the child's `after` timer while preserving any active parent `after` timer deadline.
- **Dynamic guard & action evaluation**: Re-evaluate registered C/C++/Rust/JS guard and action callbacks (plus any resulting `always` transitions, raised internal events, and reentrant external `send` calls) at each intermediate `t_fire` step before determining the next timer deadline.
- **Safe bounding of zero-delay (`after: 0`) cycles**: Prevent `after: 0` (same-timestamp) infinite self-transition loops from hanging without prematurely truncating legitimate high-frequency positive-delay advances (such as `60,000` steps of an `after: 1` child timer inside an `after: 60000` parent timeout).

### Non-Goals
- Evaluating dynamic delay expressions via runtime JS callbacks (`after: { myDynamicDelayFn: ... }`).
- Spawning background OS threads or asynchronous platform timers inside `portable-xstate-core`.

### Anti-Goals
- Having the FSM own a separate internal source of truth for time (`fsm.now()`).
- Requiring callers to know state names, compound hierarchies, or `after` durations configured in JSON in order to advance virtual time accurately.
- Adding `std` dependencies to `#![no_std]` Rust crates (`portable-xstate-core`, `portable-xstate-ffi`) or adding heap overhead for embedded real-time callers.

---

## 3. Constraints

- `#![no_std]` compliance: Both `portable-xstate-core` and `portable-xstate-ffi` must remain `#![no_std]` (using `core` and `alloc` only).
- Zero warnings under `cargo clippy` across all workspace crates (`portable-xstate-core`, `portable-xstate-ffi`, `portable-xstate-wasm`).
- Backward compatibility: Existing `fsm_register_clock(ctx, fn_ptr)` + `fsm_tick(ctx)`, `PortableXState::registerClock(fn_ptr)` + `tick()`, and WASM `actor.tick()` remain 100% source-compatible.
- Zero regressions across the full test suite (`npm test`).

---

## 4. Technical Design

### 4.1 Cross-Language API Surface & Example Usages

| Layer | Real-Time Clock Binding | Next Timer Deadline Query | Virtual Time (`SimulatedClock`) |
| :--- | :--- | :--- | :--- |
| **Rust (`portable-xstate-core`)** | `machine.register_clock(cb)` + `machine.tick(&guards, &actions)` | `machine.next_timer_deadline() -> Option<u64>` | `SimulatedClock::new()`, `machine.register_clock(&clock)`, `clock.set(now_ms, &mut machine, &guards, &actions)`, `clock.increment(dt_ms, ...)`, `clock.now() -> u64` |
| **C (`portable_xstate.h`)** | `fsm_register_clock(ctx, cb)` or `fsm_register_clock_with_data(ctx, cb, data)` + `fsm_tick(ctx)` | `bool fsm_get_next_deadline(const MachineContext* ctx, uint64_t* out_ms)` | `SimulatedClock* fsm_simulated_clock_create(0)`, `fsm_register_simulated_clock(ctx, clock)`, `fsm_simulated_clock_set(clock, now_ms)`, `fsm_simulated_clock_increment(clock, dt_ms)`, `fsm_simulated_clock_now(clock)`, `fsm_simulated_clock_free(clock)` |
| **C++ (`portable_xstate.hpp`)** | `fsm.registerClock(millis)` (or `std::function<uint64_t()>`) + `fsm.tick()` | `bool fsm.getNextDeadline(uint64_t& out_ms) const` | `xstate::SimulatedClock clock;`, `xstate::PortableXState fsm(JSON, clock);` (or `fsm.registerClock(clock);`), `clock.set(now_ms);`, `clock.increment(dt_ms);`, `clock.now()` |
| **WASM / JS (`portable-xstate-wasm`)** | Default `Date.now()` + `actor.tick()` | `actor.getNextDeadline(): number \| undefined` | `const clock = new SimulatedClock(); createActor(machine, { clock });`, `clock.set(nowMs)`, `clock.increment(dtMs)`, `clock.now()` |

#### Example 1: C++ Real-Time Embedded (`ESP32` with `millis()`)

```cpp
#include "portable_xstate.hpp"

xstate::PortableXState fsm(APRIL_CLOCK_JSON);

void setup() {
    fsm.registerGuard("isConnected", [](const xstate::Value&, const xstate::Value&) {
        return WiFi.status() == WL_CONNECTED;
    });
    // Hardware millis() is the single external source of truth (zero heap / std::function overhead)
    fsm.registerClock(millis);
    fsm.start();
}

void loop() {
    fsm.tick();

    // Optional: query next deadline for low-power light sleep / FreeRTOS wait
    uint64_t next_deadline_ms = 0;
    if (fsm.getNextDeadline(next_deadline_ms)) {
        // MCU knows exact timestamp when the next `after` timer will expire
    }
}
```

#### Example 2: C++ Unit Testing / Virtual Time (`AprilClockFSM`)

```cpp
#include "portable_xstate.hpp"

xstate::SimulatedClock clock;
xstate::PortableXState fsm(APRIL_CLOCK_JSON, clock); // or fsm.registerClock(clock)

bool wifi_ready = false;
int poll_count = 0;

fsm.registerGuard("isConnected", [&](const xstate::Value&, const xstate::Value&) {
    return wifi_ready;
});
fsm.registerAction("pollWifi", [&](const xstate::Value&, const xstate::Value&) {
    poll_count++;
    // During the 10th poll, clock.now() is 5,000 ms
    if (poll_count == 10) {
        wifi_ready = true;
    }
});

fsm.start(); // Starts at clock.now() == 0 in ConnectToNetwork.Polling

// Advance virtual time by 60,000 ms (relative):
// - SimulatedClock steps clock.now() to 500, 1000, ..., 5000 ms, ticking `fsm` at each deadline
// - At t = 5,000 ms, `pollWifi` sets `wifi_ready = true` and transitions out of ConnectToNetwork
// - SimulatedClock continues stepping through any subsequent states' timers in [5000, 60000]
// - Settles clock.now() at 60,000 ms
clock.increment(60000);

// Or advance to an absolute timestamp (drift-free):
clock.set(86460000);
```

#### Example 3: C FFI (`portable_xstate.h`)

```c
// Real-time C caller:
MachineContext* fsm = fsm_create(APRIL_CLOCK_JSON);
fsm_register_clock(fsm, millis);
fsm_start(fsm);
fsm_tick(fsm);

// Virtual-time C test caller:
SimulatedClock* sim_clock = fsm_simulated_clock_create(0);
MachineContext* test_fsm = fsm_create(APRIL_CLOCK_JSON);
fsm_register_simulated_clock(test_fsm, sim_clock);
fsm_start(test_fsm);

fsm_simulated_clock_increment(sim_clock, 60000);
fsm_simulated_clock_set(sim_clock, 86460000);

fsm_free(test_fsm);
fsm_simulated_clock_free(sim_clock);
```

#### Example 4: Rust (`portable-xstate-core`)

```rust
let clock = SimulatedClock::new();
let mut machine = RuntimeMachine::from_json(json)?;
machine.register_clock(&clock);
machine.start(&guards, &actions);

clock.increment(60_000, &mut machine, &guards, &actions);
clock.set(86_460_000, &mut machine, &guards, &actions);
assert_eq!(clock.now(), 86_460_000);
```

#### Example 5: WASM / TypeScript (`portable-xstate-wasm`)

```ts
import { createMachine, createActor, SimulatedClock } from 'portable-xstate-wasm';

const clock = new SimulatedClock();
const actor = createActor(machine, { clock }).start();

clock.increment(60_000);
clock.set(120_000);
```

---

### 4.2 Core Engine Changes (`portable-xstate-core`)

1. **Earliest-Deadline Timer Query (`RuntimeMachine::next_timer_deadline`)**:
   ```rust
   pub fn next_timer_deadline(&self) -> Option<u64> {
       if !self.is_started {
           return None;
       }
       self.active_timers.iter().map(|(_, deadline, _)| *deadline).min()
   }
   ```

2. **Chronological Selection & Same-Timestamp Cycle Protection in `RuntimeMachine::tick()`**:
   - In `RuntimeMachine::tick(&mut self, guards, actions) -> Option<StateValue>`:
     - Queries `let now = self.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(0);`
     - Loops up to `MAX_SAME_TICK_STEPS = 100` iterations:
       1. Selects the expired timer in `self.active_timers` with `deadline <= now` that has the **minimum `deadline`**:
          ```rust
          let expired = self
              .active_timers
              .iter()
              .enumerate()
              .filter(|(_, (_, deadline, _))| *deadline <= now)
              .min_by_key(|(_, (_, deadline, _))| *deadline)
              .map(|(idx, (path, _, t_defs))| (idx, path.clone(), t_defs.clone()));
          ```
          Because `Iterator::min_by_key` returns the **first** element when multiple elements tie for the minimum key, and `self.active_timers` is appended in registration order, ties on identical `deadline` are broken by **FIFO registration order** (the timer armed earlier fires first).
       2. If `expired` is `None`, breaks out of the loop.
       3. Removes `idx` from `self.active_timers`.
       4. Constructs synthetic event `after_event` (`Value::Object` with `"type": Value::String(format!("xstate.after.{}", path))` or `"xstate.after"` when root) so action/guard callbacks inspecting `event.type` receive a valid XState 5 `after` event string.
       5. Calls `self.transition_internal("", &after_event, guards, actions, 0, Some((&path, &t_defs)))`.
       6. Drains `self.internal_queue` (`self.drain_internal_queue(guards, actions)`).
       7. Drains any reentrant external events queued by action callbacks during this transition (`self.drain_reentrant_external_queue(guards, actions)`) while keeping `self.is_processing = true`, so any state transitions triggered by reentrant `send()` calls take effect immediately at the current `now` before the next timer is evaluated.

3. **`SimulatedClock` & `IntoClockFn` in `portable-xstate-core`**:
   - Defined in `portable-xstate-core`:
     ```rust
     #[derive(Clone, Default)]
     pub struct SimulatedClock {
         now_ms: Rc< core::cell::Cell<u64> >,
     }

     pub trait IntoClockFn {
         fn into_clock_fn(self) -> Rc<dyn Fn() -> u64>;
     }
     ```
     `IntoClockFn` is implemented for `Rc<dyn Fn() -> u64>`, `&SimulatedClock`, and `SimulatedClock` so `machine.register_clock(&clock)` works alongside existing `machine.register_clock(Rc::new(|| ...))` calls.
   - Methods on `SimulatedClock`:
     - `pub fn new() -> Self`
     - `pub fn with_initial(initial_ms: u64) -> Self`
     - `pub fn now(&self) -> u64`
     - `pub fn set_now_raw(&self, now_ms: u64)`
     - `pub fn as_clock_fn(&self) -> Rc<dyn Fn() -> u64>`
     - `pub fn set(&self, target_ms: u64, machine: &mut RuntimeMachine, guards: &IndexMap<String, GuardFunction>, actions: &IndexMap<String, ActionFunction>) -> Option<StateValue>`:
       - Tracks `last_ret = None`, `last_deadline: Option<u64> = None`, `same_deadline_steps = 0usize` (`MAX_SAME_TIMESTAMP_STEPS = 100`), and `total_steps = 0usize` (`MAX_TOTAL_TIMER_STEPS = 1_000_000`).
       - While `let Some(next_deadline) = machine.next_timer_deadline()`:
         - If `next_deadline > target_ms`, break.
         - If `last_deadline == Some(next_deadline)`:
           - Increment `same_deadline_steps`; if `same_deadline_steps >= 100`, break (handles saturated `after: 0` self-transition loops).
         - Else:
           - `last_deadline = Some(next_deadline)`; `same_deadline_steps = 0`.
         - Increment `total_steps`; if `total_steps > 1_000_000`, break.
         - Set `self.now_ms.set(next_deadline)`.
         - Call `if let Some(st) = machine.tick(guards, actions) { last_ret = Some(st); }`.
       - If `target_ms > self.now_ms.get()`, set `self.now_ms.set(target_ms)`.
       - Return `last_ret`.
     - `pub fn increment(&self, dt_ms: u64, machine: &mut RuntimeMachine, guards: &IndexMap<String, GuardFunction>, actions: &IndexMap<String, ActionFunction>) -> Option<StateValue>`:
       - Calls `self.set(self.now_ms.get().saturating_add(dt_ms), machine, guards, actions)`.

---

### 4.3 Hierarchical Self-Transitions & XState 5 `reenter` Semantics

In accordance with XState 5's transition domain (LCA) rules:
- When a child state re-enters itself (e.g., `Polling` with `"after": { "500": { "target": "Polling", "reenter": true } }`, or when a compound parent state transitions to a child `.Polling`), the transition domain is inside the parent compound state (`ConnectToNetwork`).
- Only `Polling` (`i = current_parts.len()`) is exited and re-entered:
  - `Polling`'s expired timer is consumed and a new `after: 500` timer for `Polling` is armed at `clock() + 500` (`t_fire + 500`).
  - `ConnectToNetwork` is **not** exited (`exited_paths` does not contain `"ConnectToNetwork"`), so `ConnectToNetwork`'s `after: 300000` timer remains intact at its original deadline.
- When `t = 300,000` is reached, `ConnectToNetwork`'s timer (armed at `t = 0`) and `Polling`'s 600th timer (armed at `t = 299,500`) both have `deadline == 300,000`. Because `ConnectToNetwork`'s timer was registered earlier in `active_timers` (FIFO registration order), `ConnectToNetwork`'s timeout transition fires first at `t = 300,000`, exiting `ConnectToNetwork` and `Polling` and cancelling `Polling`'s timer.

---

### 4.4 C / C++ FFI Adapter (`portable-xstate-ffi`)

1. **`portable_xstate.h` & `portable-xstate-ffi/src/lib.rs`**:
   - Keep `typedef uint64_t (*fsm_clock_callback_t)(void);` and `int fsm_register_clock(MachineContext* ctx, fsm_clock_callback_t callback);` unchanged.
   - Add `user_data`-capable clock registration:
     ```c
     typedef uint64_t (*fsm_clock_with_data_callback_t)(void* user_data);
     int fsm_register_clock_with_data(
         MachineContext* ctx,
         fsm_clock_with_data_callback_t callback,
         void* user_data
     );
     ```
   - Add next-deadline inspection:
     ```c
     bool fsm_get_next_deadline(const MachineContext* ptr, uint64_t* out_deadline);
     ```
   - Add opaque C `SimulatedClock*` API (consistent with `MachineContext*`):
     ```c
     typedef struct SimulatedClock SimulatedClock;

     SimulatedClock* fsm_simulated_clock_create(uint64_t initial_ms);
     void fsm_simulated_clock_free(SimulatedClock* clock);
     int fsm_register_simulated_clock(MachineContext* ctx, SimulatedClock* clock);
     uint64_t fsm_simulated_clock_now(const SimulatedClock* clock);
     int fsm_simulated_clock_set(SimulatedClock* clock, uint64_t target_ms);
     int fsm_simulated_clock_increment(SimulatedClock* clock, uint64_t dt_ms);
     ```
     - `SimulatedClock` holds `portable_xstate_core::SimulatedClock` (`Rc<Cell<u64>>`) and a shared registry of bound `MachineContext*` pointers (`Rc<RefCell<Vec<*mut MachineContext>>>`).
     - `Drop for MachineContext` removes itself from any bound `SimulatedClock` registry so destroying `MachineContext` and `SimulatedClock` in **either** order is 100% memory-safe.
   - Fix `fsm_tick(MachineContext* ptr)` to call `ctx.machine.tick(&ctx.guards, &ctx.actions)` once (relying on `RuntimeMachine::tick`'s bounded loop rather than an unbounded `while let Some(_) = ...` loop in FFI).

2. **`portable_xstate.hpp` (`xstate::PortableXState` & `xstate::SimulatedClock`)**:
   - Define RAII `xstate::SimulatedClock` wrapping `SimulatedClock*` (`fsm_simulated_clock_create` / `fsm_simulated_clock_free`), non-copyable and movable:
     - `explicit SimulatedClock(uint64_t initial_ms = 0)`
     - `uint64_t now() const noexcept`
     - `void set(uint64_t target_ms)`
     - `void increment(uint64_t dt_ms)`
   - On `xstate::PortableXState`:
     - Add constructor overload `PortableXState(const char* json_config, SimulatedClock& clock)` (mirrors XState 5's `createActor(machine, { clock })`).
     - Keep `void registerClock(fsm_clock_callback_t callback)` (zero-overhead raw function pointer path for `millis`).
     - Add `void registerClock(SimulatedClock& clock)`.
     - Add `using ClockCallback = std::function<uint64_t()>;` and `void registerClock(ClockCallback callback)` (for custom C++ lambdas/functors via `fsm_register_clock_with_data`).
     - Add `bool getNextDeadline(uint64_t& out_deadline) const`.
     - Fix `PortableXState(PortableXState&& other)` move constructor so moving an already-started `PortableXState` re-registers `this` for actions/guards/clock callbacks.

---

### 4.5 WASM / JS Adapter (`portable-xstate-wasm`)

1. **`SimulatedClock` (`#[wasm_bindgen] pub struct SimulatedClock`)**:
   - Wraps `portable_xstate_core::SimulatedClock` and holds `Rc<RefCell<Vec<Weak<RefCell<ActorInner>>>>>` to bound `Actor` instances.
   - Exposes to JS:
     - `#[wasm_bindgen(constructor)] pub fn new() -> SimulatedClock`
     - `#[wasm_bindgen] pub fn set(&mut self, timestamp: f64)`
     - `#[wasm_bindgen] pub fn increment(&mut self, ms: f64)`
     - `#[wasm_bindgen] pub fn now(&self) -> f64`
2. **`Actor` Shared Inner State**:
   - Wrap `Actor`'s mutable state in `Rc<RefCell<ActorInner>>` so `const actor = createActor(machine, { clock }).start()` (which clones `Actor` in `.start()`) and `clock` operate on the exact same underlying `RuntimeMachine`.
   - If `options.clock` is provided to `createActor(machine, { clock })`, registers `clock` with the actor so `clock.increment(ms)` and `clock.set(timestamp)` step the actor's machine and emit snapshots on transitions.
   - Also supports Vitest fake timers (`vi.useFakeTimers()` + `vi.advanceTimersByTime(dt)` + `actor.tick()`): if `Date.now()` jumps ahead by `dt` under fake timers, `actor.tick()` steps through all intermediate deadlines in `[last_tick_ms, Date.now()]` using the same chronological deadline progression.
   - Exposes `#[wasm_bindgen(js_name = getNextDeadline)] pub fn get_next_deadline(&self) -> Option<f64>`.

---

## 5. What Is Left to the Builder

- Structuring the unit and integration tests across:
  - `portable-xstate-core/src/repro_test.rs` (Rust unit tests covering `SimulatedClock::increment` and `SimulatedClock::set`, cascading timers `Wait -> Sleep`, hierarchical parent timeout + fast child polling `ConnectToNetwork.Polling` and `StartNetworkTimeSync.Polling`, dynamic guards changing mid-window, and `after: 0` cycle termination).
  - `portable-xstate-ffi/src/lib.rs`, `portable-xstate-ffi/tests/c_test.c`, and `portable-xstate-ffi/tests/cpp_test.cpp` (verifying `fsm_simulated_clock_*`, `xstate::SimulatedClock`, `getNextDeadline`, and `registerClock`).
  - `tests/after.test.ts` (verifying `SimulatedClock` and fake timer progression in WASM).
- Internal shared-pointer wiring between `wasm::SimulatedClock` and `wasm::Actor` so `clock.increment(ms)` in JS mutates the actor's underlying `RuntimeMachine` and triggers `emit_snapshot()` when transitions occur.
