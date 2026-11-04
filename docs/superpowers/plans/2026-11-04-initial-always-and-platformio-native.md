# Initial Eventless (`always`) Transitions, XState v5 Start Parity, and PlatformIO `native` Support Implementation Plan [flowos: cd064104-0ee6-47b0-833e-efc32dc98102]

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unify XState v5 initialization macrostep semantics (root-to-leaf initial actions, `{ "type": "xstate.init" }` payload, initial `always` transition settlement before raised internal events, reentrant external `send()` deferral, and pre-start snapshot preview) across Rust, C/C++ FFI, and WASM, and add PlatformIO `native` build/test support in `portable-xstate-platformio`.

**Architecture:** Centralize the initialization macrostep in `portable-xstate-core::RuntimeMachine::start(&mut self, guards, actions)` so Rust, FFI, and WASM execute a single canonical initialization sequence without duplicating `always` loop logic. Update `portable-xstate-ffi` (`fsm_start`, `fsm_send_event`) and `portable-xstate-wasm` (`StateNode::create_actor_method`, `Actor::start`, `Actor::get_snapshot`) to delegate initialization to `RuntimeMachine::start`, and refactor `portable-xstate-platformio` (`library.json`, `build_rust.py`, `test_build_rust.py`) to support `platform = native` alongside ESP32 targets.

**Tech Stack:** Rust (`#![no_std]` + `alloc`, `indexmap`), C11 / C++11 (`clang` / `clang++`), WebAssembly (`wasm-bindgen`, `wasm-pack`), TypeScript / Vitest (`xstate` `^5.25.0`), Python 3 (`unittest`, PlatformIO / SCons).

**Spec:** [`docs/superpowers/specs/2026-11-04-initial-always-and-platformio-native-design.md`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/platformio_native_design_plan/docs/superpowers/specs/2026-11-04-initial-always-and-platformio-native-design.md)

## Global Constraints

- **`#![no_std]` + `alloc` Compatibility**: All changes in `portable-xstate-core` and `portable-xstate-ffi` must remain strictly `#![no_std]` compatible.
- **Stable C/C++ FFI Signatures**: Keep `portable_xstate.h` and `portable_xstate.hpp` public API signatures unchanged.
- **No Duplicate Macrostep Logic Across Crates**: `portable-xstate-ffi` and `portable-xstate-wasm` must not each re-implement ad-hoc `while` loops over `machine.transition("", ...)` to settle initial `always` transitions; `RuntimeMachine` in `portable-xstate-core` must own the initialization macrostep.
- **No Premature Side-Effect Execution Before `.start()`**: Creating a machine or actor (`fsm_create`, `PortableXState` constructor, `createActor`) must never execute external action callbacks before `start()` is invoked.
- **Full Test Suite & Oracle Conformance**: All Vitest unit tests (`npm run test:unit`), Rust native tests (`npm run test:native`), C/C++ FFI tests (`npm run test:ffi`), WASM headless browser tests (`npm run test:wasm`), Playwright E2E tests (`npm run test:e2e`), and PlatformIO native/unit tests must pass.

## Review Focus

- **Repeated `start()` calls on an already-started machine or actor**: Calling `RuntimeMachine::start`, `fsm_start`, or `actor.start()` a second time must be a no-op that does not re-execute initial `entry` actions, re-run `always` transitions, or duplicate `active_timers` (tested in Task 1 and Task 3).
- **Transient initial state `after` timers exited during initial `always` transitions**: When an initial state declares an `after` timer and immediately exits via an `always` transition during `start()`, the exited state's `after` timer must be removed from `active_timers` while the settled target state's `after` timer is scheduled and fires on `tick()` (tested in Task 1 and Task 2).
- **Guarded initial `always` transition reading context assigned by initial `entry`**: When an initial state's `entry` action mutates `context` (via `Action::Assign` or a WASM dynamic `assign(...)` callback) and its `always` transition guard checks that updated property, `start()` must execute the initial `entry` assignment before evaluating the `always` guard (tested in Task 1 and Task 3).
- **`createActor(machine).getSnapshot()` called before `actor.start()`**: Inspecting `.getSnapshot()` on an unstarted actor must preview the settled state value (following initial `always` transitions and static assignments) without executing external JS side-effect callbacks or mutating `self.machine`, and calling `.start()` afterwards must still run all initial, transition, and target `entry` callbacks once in order (tested in Task 3).
- **Existing cached `portable-xstate` git checkout when `DEPENDENCY_REF` advances**: When `DEPENDENCY_DIR` already exists on disk from a prior build, `clone_dependency()` in `build_rust.py` must run `git fetch --all` before retrying `git checkout <DEPENDENCY_REF>` if the new commit SHA is missing locally (tested in Task 4).

---

## File Map

- `portable-xstate-core/src/lib.rs`: Update `RuntimeMachine::start` signature and implementation to execute root-to-leaf initial actions with `{ "type": "xstate.init" }`, settle initial `always` transitions, drain `internal_queue` with `guards`, and drain reentrant `external_queue`.
- `portable-xstate-core/src/repro_test.rs`: Update existing `machine.start(...)` calls and add unit tests for the XState v5 initialization macrostep.
- `portable-xstate-ffi/src/lib.rs`: Pass `&ctx.guards` and `&ctx.actions` in `fsm_start`; pass `{ "type": "<event>" }` payload in `fsm_send_event`; add FFI unit tests for initial `always` settlement, event payloads, timers, and reentrant `fsm_send_event` during `fsm_start`.
- `portable-xstate-ffi/tests/c_test.c`: Add C integration assertions for initial `always` transitions and `event.type` inspection during `fsm_start` and `fsm_send_event`.
- `portable-xstate-ffi/tests/cpp_test.cpp`: Add C++ integration assertions for initial `always` transitions and `event.getProperty("type")` inspection during `machine.start()` and `machine.send()`.
- `portable-xstate-wasm/src/lib.rs`: Remove premature `runner.handle_always_transition()` in `StateNode::create_actor_method`, update `Actor::start` to delegate initialization to `self.machine.start(&self.guards, &self.actions)`, and update `Actor::get_snapshot` to preview unstarted actors via a cloned `preview_machine`.
- `tests/machine.test.ts`: Add Vitest conformance tests comparing `portable-xstate-wasm` initialization behavior against XState v5 rules (no side effects before `.start()`, pre-start `.getSnapshot()`, dynamic `assign` before `always` guards, root-to-leaf initial action ordering, `always` before raised internal events, and `{ type: 'xstate.init' }` event payloads).
- `/Users/mike/GitHub/portable-xstate-platformio/library.json`: Set `"frameworks": ["*"]` so PlatformIO's LDF resolves the library in `platform = native` environments.
- `/Users/mike/GitHub/portable-xstate-platformio/build_rust.py`: Guard SCons `Import("env")`, extract testable target/path/command helpers supporting `platform = native` and `espressif32`, add `git fetch --all` fallback in `clone_dependency()`, and update `DEPENDENCY_REF`.
- `/Users/mike/GitHub/portable-xstate-platformio/test_build_rust.py`: Import and test `build_rust.py` functions directly across ESP32 Xtensa/RISC-V and `native` environments.

---

## Phase 1: XState v5 Initialization Parity Across Rust, FFI, and WASM [flowos: a0f960a7-4468-4063-888e-95def352e299]

### Task 1: Centralize XState v5 Initialization Macrostep in `portable-xstate-core` [flowos: a9c29a2c-bf4e-4323-8ef3-fe7215dfe412]

**Files:**
- Modify: `portable-xstate-core/src/lib.rs:270-305`
- Modify: `portable-xstate-core/src/repro_test.rs:190-271`
- Modify: `portable-xstate-ffi/src/lib.rs:84-91`
- Modify: `portable-xstate-wasm/src/lib.rs:1400-1405`

**Interfaces:**
- Consumes: `RuntimeMachine::transition_internal`, `RuntimeMachine::drain_internal_queue`, `RuntimeMachine::drain_external_queue`, `RuntimeMachine::execute_action`, `RuntimeMachine::get_state_def_recursive` in `portable-xstate-core/src/lib.rs`
- Produces: `pub fn start(&mut self, guards: &IndexMap<String, GuardFunction>, actions: &IndexMap<String, ActionFunction>)` on ` portable_xstate_core::RuntimeMachine`

- [x] **Step 1: Write failing Rust unit tests in `portable-xstate-core/src/repro_test.rs`**

Update the two existing `machine.start(&IndexMap::new())` calls in `portable-xstate-core/src/repro_test.rs` (lines 191 and 263) to `machine.start(&IndexMap::new(), &IndexMap::new())`, and append the following tests to `mod tests` in `portable-xstate-core/src/repro_test.rs`:

```rust
    #[test]
    fn test_start_root_to_leaf_action_order_and_init_event() {
        use alloc::rc::Rc;
        use core::cell::RefCell;

        let log: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(Vec::new()));
        let make_logger = |name: &str| {
            let log_ref = log.clone();
            let label = name.to_string();
            Rc::new(move |_ctx: &mut Value, evt: &Value, _params: &Option<Value>| {
                let evt_type = evt
                    .as_object()
                    .and_then(|m| m.get("type"))
                    .and_then(|v| v.as_string())
                    .cloned()
                    .unwrap_or_else(|| "null".to_string());
                log_ref.borrow_mut().push((label.clone(), evt_type));
            }) as ActionFunction
        };

        let mut parent_def = StateDefinition::default_atomic();
        parent_def.state_type = StateType::Compound;
        parent_def.initial = Some("child".to_string());
        parent_def.entry = vec![Action::Named("parent_entry".to_string())];
        parent_def.initial_actions = vec![Action::Named("parent_init_action".to_string())];

        let mut child_def = StateDefinition::default_atomic();
        child_def.entry = vec![Action::Named("child_entry".to_string())];
        parent_def.states.insert("child".to_string(), child_def);

        let definition = MachineDefinition {
            id: "order".to_string(),
            type_: None,
            state_type: StateType::Compound,
            initial: "parent".to_string(),
            initial_actions: vec![Action::Named("root_init_action".to_string())],
            context: None,
            types: None,
            invoke: vec![],
            states: indexmap::indexmap! { "parent".to_string() => parent_def },
            after: IndexMap::new(),
            on: IndexMap::new(),
            entry: vec![Action::Named("root_entry".to_string())],
            exit: vec![],
            id_map: IndexMap::new(),
        };

        let actions = indexmap::indexmap! {
            "root_entry".to_string() => make_logger("root_entry"),
            "root_init_action".to_string() => make_logger("root_init_action"),
            "parent_entry".to_string() => make_logger("parent_entry"),
            "parent_init_action".to_string() => make_logger("parent_init_action"),
            "child_entry".to_string() => make_logger("child_entry"),
        };

        let mut machine = RuntimeMachine::new(&definition).unwrap();
        machine.start(&IndexMap::new(), &actions);
        // Repeated start() must be a no-op
        machine.start(&IndexMap::new(), &actions);

        assert_eq!(
            *log.borrow(),
            vec![
                ("root_entry".to_string(), "xstate.init".to_string()),
                ("root_init_action".to_string(), "xstate.init".to_string()),
                ("parent_entry".to_string(), "xstate.init".to_string()),
                ("parent_init_action".to_string(), "xstate.init".to_string()),
                ("child_entry".to_string(), "xstate.init".to_string()),
            ]
        );
    }

    #[test]
    fn test_start_always_before_raised_and_guards_applied() {
        use alloc::rc::Rc;

        let mut boot = StateDefinition::default_atomic();
        boot.entry = vec![
            Action::Assign(AssignAction {
                action_type: "xstate.assign".to_string(),
                assignment: Value::Object(indexmap::indexmap! {
                    "ready".to_string() => Value::Bool(true),
                }),
            }),
            Action::Raise(Value::Object(indexmap::indexmap! {
                "type".to_string() => Value::String("RAISED_EV".to_string()),
            })),
        ];
        boot.on.insert(
            "".to_string(),
            vec![TransitionDefinition {
                target: Some(vec!["settled".to_string()]),
                cond: Some(TransitionGuard::Name("isReady".to_string())),
                actions: vec![],
                reenter: false,
            }],
        );
        boot.on.insert(
            "RAISED_EV".to_string(),
            vec![TransitionDefinition {
                target: Some(vec!["wrong_from_boot".to_string()]),
                cond: None,
                actions: vec![],
                reenter: false,
            }],
        );

        let mut settled = StateDefinition::default_atomic();
        settled.on.insert(
            "RAISED_EV".to_string(),
            vec![
                TransitionDefinition {
                    target: Some(vec!["blocked_by_guard".to_string()]),
                    cond: Some(TransitionGuard::Name("alwaysFalse".to_string())),
                    actions: vec![],
                    reenter: false,
                },
                TransitionDefinition {
                    target: Some(vec!["final_state".to_string()]),
                    cond: Some(TransitionGuard::Name("isReady".to_string())),
                    actions: vec![],
                    reenter: false,
                },
            ],
        );

        let definition = MachineDefinition {
            id: "always_raise".to_string(),
            type_: None,
            state_type: StateType::Compound,
            initial: "boot".to_string(),
            initial_actions: vec![],
            context: Some(Value::Object(indexmap::indexmap! {
                "ready".to_string() => Value::Bool(false),
            })),
            types: None,
            invoke: vec![],
            states: indexmap::indexmap! {
                "boot".to_string() => boot,
                "settled".to_string() => settled,
                "wrong_from_boot".to_string() => StateDefinition::default_atomic(),
                "blocked_by_guard".to_string() => StateDefinition::default_atomic(),
                "final_state".to_string() => StateDefinition::default_atomic(),
            },
            after: IndexMap::new(),
            on: IndexMap::new(),
            entry: vec![],
            exit: vec![],
            id_map: IndexMap::new(),
        };

        let guards: IndexMap<String, GuardFunction> = indexmap::indexmap! {
            "isReady".to_string() => Rc::new(|ctx: &Value, _evt: &Value, _params: &Option<Value>| {
                ctx.as_object()
                    .and_then(|m| m.get("ready"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            }) as GuardFunction,
            "alwaysFalse".to_string() => Rc::new(|_ctx: &Value, _evt: &Value, _params: &Option<Value>| false) as GuardFunction,
        };

        let mut machine = RuntimeMachine::new(&definition).unwrap();
        machine.start(&guards, &IndexMap::new());

        assert_eq!(machine.state(), "final_state");
    }

    #[test]
    fn test_start_cancels_transient_timer_and_schedules_settled_timer() {
        use alloc::rc::Rc;
        use core::cell::Cell;

        let now_ms = Rc::new(Cell::new(100u64));
        let clock_cell = now_ms.clone();

        let mut transient = StateDefinition::default_atomic();
        transient.after.insert(
            50,
            vec![TransitionDefinition {
                target: Some(vec!["wrong_timeout".to_string()]),
                cond: None,
                actions: vec![],
                reenter: false,
            }],
        );
        transient.on.insert(
            "".to_string(),
            vec![TransitionDefinition {
                target: Some(vec!["active".to_string()]),
                cond: None,
                actions: vec![],
                reenter: false,
            }],
        );

        let mut active = StateDefinition::default_atomic();
        active.after.insert(
            100,
            vec![TransitionDefinition {
                target: Some(vec!["timed_out".to_string()]),
                cond: None,
                actions: vec![],
                reenter: false,
            }],
        );

        let definition = MachineDefinition {
            id: "timers".to_string(),
            type_: None,
            state_type: StateType::Compound,
            initial: "transient".to_string(),
            initial_actions: vec![],
            context: None,
            types: None,
            invoke: vec![],
            states: indexmap::indexmap! {
                "transient".to_string() => transient,
                "active".to_string() => active,
                "wrong_timeout".to_string() => StateDefinition::default_atomic(),
                "timed_out".to_string() => StateDefinition::default_atomic(),
            },
            after: IndexMap::new(),
            on: IndexMap::new(),
            entry: vec![],
            exit: vec![],
            id_map: IndexMap::new(),
        };

        let mut machine = RuntimeMachine::new(&definition).unwrap();
        machine.register_clock(Rc::new(move || clock_cell.get()));
        machine.start(&IndexMap::new(), &IndexMap::new());
        assert_eq!(machine.state(), "active");

        // At t=160ms, transient's 50ms timer would have expired if not cancelled
        now_ms.set(160);
        machine.tick(&IndexMap::new(), &IndexMap::new());
        assert_eq!(machine.state(), "active");

        // At t=200ms, active's 100ms timer expires
        now_ms.set(200);
        machine.tick(&IndexMap::new(), &IndexMap::new());
        assert_eq!(machine.state(), "timed_out");
    }
```

- [x] **Step 2: Run `cargo test --package portable-xstate-core` to verify the new tests fail to compile**

Run: `cargo test --package portable-xstate-core`
Expected: FAIL with `this method takes 1 argument but 2 arguments were supplied` on `machine.start(...)`.

- [x] **Step 3: Implement `RuntimeMachine::start(&mut self, guards, actions)` in `portable-xstate-core/src/lib.rs` and update the two workspace call sites**

In [`portable-xstate-core/src/lib.rs`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/platformio_native_design_plan/portable-xstate-core/src/lib.rs#L270-L305), replace `pub fn start(&mut self, actions: &IndexMap<String, ActionFunction>)` with:

```rust
    pub fn start(
        &mut self,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) {
        if self.is_started {
            return;
        }
        self.is_started = true;
        self.is_processing = true;

        let mut init_map = IndexMap::new();
        init_map.insert("type".to_string(), Value::String("xstate.init".to_string()));
        let init_event = Value::Object(init_map);

        let _ = core::mem::take(&mut self.initial_actions);
        let mut init_actions_to_run = self.definition.entry.clone();
        let now = self.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(0);
        for (delay, t_defs) in &self.definition.after {
            self.active_timers
                .push(("".to_string(), now + delay, t_defs.clone()));
        }
        init_actions_to_run.extend(self.definition.initial_actions.clone());
        let root_init_key = if self.definition.initial.starts_with('#') {
            &self.definition.initial[1..]
        } else {
            &self.definition.initial
        };
        let root_init_resolved = self
            .definition
            .id_map
            .get(root_init_key)
            .map(|s| s.as_str())
            .unwrap_or(&self.definition.initial);
        if let Some(root_init_def) =
            Self::get_state_def_recursive(&self.definition.states, root_init_resolved)
        {
            if matches!(root_init_def.state_type, StateType::History { .. }) {
                init_actions_to_run.extend(root_init_def.history_actions.clone());
            }
        }

        let mut seen_paths = alloc::vec::Vec::new();
        for path in self.current_state.get_active_paths() {
            let parts: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
            for i in 1..=parts.len() {
                let curr_path = parts[0..i].join(".");
                if !seen_paths.contains(&curr_path) {
                    if let Some(def) =
                        Self::get_state_def_recursive(&self.definition.states, &curr_path)
                    {
                        init_actions_to_run.extend(def.entry.clone());
                        for (delay, t_defs) in &def.after {
                            self.active_timers.push((
                                curr_path.clone(),
                                now + delay,
                                t_defs.clone(),
                            ));
                        }
                        if matches!(def.state_type, StateType::Compound) {
                            init_actions_to_run.extend(def.initial_actions.clone());
                            if let Some(child_init) = &def.initial {
                                let child_path = format!("{}.{}", curr_path, child_init);
                                if let Some(child_def) = Self::get_state_def_recursive(
                                    &self.definition.states,
                                    &child_path,
                                ) {
                                    if matches!(child_def.state_type, StateType::History { .. }) {
                                        init_actions_to_run
                                            .extend(child_def.history_actions.clone());
                                    }
                                }
                            }
                        }
                    }
                    seen_paths.push(curr_path);
                }
            }
        }

        for action in &init_actions_to_run {
            Self::execute_action(
                &mut self.context,
                action,
                &init_event,
                actions,
                &mut self.internal_queue,
            );
        }

        // 1. Settle initial eventless (`always`) transitions first
        let _ = self.transition_internal("", &init_event, guards, actions, 0, None);

        // 2. Drain internal events raised during initial entry or initial `always` transitions
        let _ = self.drain_internal_queue(guards, actions);

        // 3. Complete initialization macrostep and drain any reentrant external events
        self.is_processing = false;
        let _ = self.drain_external_queue(guards, actions);
    }
```

Also update the call site in [`portable-xstate-ffi/src/lib.rs`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/platformio_native_design_plan/portable-xstate-ffi/src/lib.rs#L89) to `ctx.machine.start(&ctx.guards, &ctx.actions);` and in [`portable-xstate-wasm/src/lib.rs`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/platformio_native_design_plan/portable-xstate-wasm/src/lib.rs#L1403) to `self.machine.start(&self.guards, &self.actions);`.

- [x] **Step 4: Run `cargo test --package portable-xstate-core` to verify all core tests pass**

Run: `cargo test --package portable-xstate-core`
Expected: PASS (5 passed; 0 failed).

- [x] **Step 5: Commit Task 1**

```bash
git add portable-xstate-core/src/lib.rs portable-xstate-core/src/repro_test.rs portable-xstate-ffi/src/lib.rs portable-xstate-wasm/src/lib.rs
git commit -m "feat(core): unify XState v5 initialization macrostep in RuntimeMachine::start"
```

---

### Task 2: C/C++ FFI Initial `always` Settlement and Event Object Payloads (`portable-xstate-ffi`) [flowos: 1e4565f9-cdbd-4eca-b770-120926dee563]

**Files:**
- Modify: `portable-xstate-ffi/src/lib.rs:134-150, 353-455`
- Modify: `portable-xstate-ffi/tests/c_test.c:1-66`
- Modify: `portable-xstate-ffi/tests/cpp_test.cpp:1-55`

**Interfaces:**
- Consumes: `RuntimeMachine::start(&mut self, guards, actions)` and `RuntimeMachine::transition(&mut self, event_type, event_payload, guards, actions)` from `portable-xstate-core`
- Produces: C ABI `fsm_start(ptr)` and `fsm_send_event(ptr, event_ptr)` passing `{ "type": "xstate.init" }` and `{ "type": "<event>" }` `Value::Object` payloads to registered C/C++ guards and actions

- [x] **Step 1: Write failing FFI unit and C/C++ integration tests**

1. In [`portable-xstate-ffi/src/lib.rs`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/platformio_native_design_plan/portable-xstate-ffi/src/lib.rs#L454), add tests inside `mod tests` verifying:
   - `test_initial_always_and_reentrant_send_on_start_ffi`: A machine with initial state `"boot"` that has `entry: ["onBootEntry"]` (which calls `fsm_send_event(machine, "EXTERNAL_FROM_BOOT")`) and `always: [{ "target": "ready", "guard": "isAllowed" }]`, where `"ready"` transitions on `"EXTERNAL_FROM_BOOT"` to `"running"`. Verify that `fsm_create` does not invoke `onBootEntry`, `fsm_start` passes `"xstate.init"` as `event.type` to `onBootEntry` and `isAllowed`, settles `"boot" -> "ready"` first, and then drains `"EXTERNAL_FROM_BOOT"` to reach `"running"` with `event.type == "EXTERNAL_FROM_BOOT"`.
2. In [`portable-xstate-ffi/tests/c_test.c`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/platformio_native_design_plan/portable-xstate-ffi/tests/c_test.c#L30-L35), add assertions inside `my_c_callback`:

```c
    const fsm_value_t* ev_type = fsm_value_get_property(event, "type");
    assert(ev_type != NULL);
    assert(fsm_value_is_string(ev_type));
    char ev_buf[32];
    fsm_value_get_string(ev_type, ev_buf, sizeof(ev_buf));
    assert(strcmp(ev_buf, "TIMER") == 0);
```

3. In [`portable-xstate-ffi/tests/cpp_test.cpp`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/platformio_native_design_plan/portable-xstate-ffi/tests/cpp_test.cpp#L30-L35), add assertions inside the `"doTimerAction"` callback and add an initial `always` C++ test block in `main()`:

```cpp
            auto ev_type = event.getProperty("type");
            assert(!ev_type.isNull());
            std::string ev_name;
            assert(ev_type.getString(ev_name) && ev_name == "TIMER");
```

- [x] **Step 2: Run `npm run test:ffi` and `cargo test --package portable-xstate-ffi` to verify failure**

Run: `cargo test --package portable-xstate-ffi && npm run test:ffi`
Expected: FAIL on `ev_type != NULL` / `!ev_type.isNull()` because `fsm_send_event` currently passes `Value::Null`.

- [x] **Step 3: Construct `{ "type": "<event>" }` payload in `fsm_send_event` in `portable-xstate-ffi/src/lib.rs`**

In [`portable-xstate-ffi/src/lib.rs`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/platformio_native_design_plan/portable-xstate-ffi/src/lib.rs#L146-L150), replace `let payload = Value::Null;` with:

```rust
    if !ctx.machine.is_started { return 0; }
    let mut event_map = IndexMap::new();
    event_map.insert(
        alloc::string::String::from("type"),
        Value::String(alloc::string::String::from(event)),
    );
    let payload = Value::Object(event_map);
    ctx.machine.transition(event, &payload, &ctx.guards, &ctx.actions);
    0
```

- [x] **Step 4: Run Rust FFI tests and C/C++ integration tests to verify they pass**

Run: `cargo test --package portable-xstate-ffi && npm run test:ffi`
Expected: PASS (`test result: ok` for `portable-xstate-ffi`, `C Test Passed!`, and `C++ Test Passed!`).

- [x] **Step 5: Commit Task 2**

```bash
git add portable-xstate-ffi/src/lib.rs portable-xstate-ffi/tests/c_test.c portable-xstate-ffi/tests/cpp_test.cpp
git commit -m "fix(ffi): settle initial always transitions in fsm_start and pass event type payloads"
```

---

### Task 3: WASM `createActor`, `Actor::start`, and Pre-Start `getSnapshot` Parity (`portable-xstate-wasm`) [flowos: 28f525de-5e32-4cd9-b7cf-9b078b455396]

**Files:**
- Modify: `portable-xstate-wasm/src/lib.rs:635-646, 1310-1348, 1400-1417, 1663-1673`
- Modify: `tests/machine.test.ts:301-315`

**Interfaces:**
- Consumes: `RuntimeMachine::start(&mut self, guards, actions)` from `portable-xstate-core`
- Produces: `StateNode::create_actor_method`, `Actor::start`, and `Actor::get_snapshot` in `portable-xstate-wasm` matching XState v5 `createActor(machine)` and `actor.start()` lifecycle semantics

- [x] **Step 1: Write failing Vitest conformance tests in `tests/machine.test.ts`**

In [`tests/machine.test.ts`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/platformio_native_design_plan/tests/machine.test.ts#L301-L315), expand `describe('initial state', ...)` with the following tests:

```ts
  describe('initial state', () => {
    it('should follow always transition', () => {
      const machine = createMachine({
        initial: 'a',
        states: {
          a: {
            always: [{ target: 'b' }]
          },
          b: {}
        }
      });

      expect(createActor(machine).getSnapshot().value).toBe('b');
    });

    it('should not execute side-effect actions on createActor or pre-start getSnapshot, and should execute them once on start() with xstate.init event', () => {
      const calls: Array<{ step: string; eventType: string }> = [];
      const machine = createMachine({
        initial: 'a',
        states: {
          a: {
            entry: [
              ({ event }) => calls.push({ step: 'entry:a', eventType: event.type }),
              assign({ ready: true })
            ],
            exit: [
              ({ event }) => calls.push({ step: 'exit:a', eventType: event.type })
            ],
            always: [
              {
                guard: ({ context }) => Boolean((context as any).ready),
                target: 'b',
                actions: [
                  ({ event }) => calls.push({ step: 'always:a->b', eventType: event.type })
                ]
              }
            ]
          },
          b: {
            entry: [
              ({ event }) => calls.push({ step: 'entry:b', eventType: event.type })
            ]
          }
        },
        context: { ready: false }
      });

      const actor = createActor(machine);
      expect(calls).toEqual([]);

      // Pre-start getSnapshot() should not fire JS side-effect callbacks
      actor.getSnapshot();
      expect(calls).toEqual([]);

      actor.start();
      // Calling start() a second time must be a no-op
      actor.start();

      expect(actor.getSnapshot().value).toBe('b');
      expect(actor.getSnapshot().context).toEqual({ ready: true });
      expect(calls).toEqual([
        { step: 'entry:a', eventType: 'xstate.init' },
        { step: 'exit:a', eventType: 'xstate.init' },
        { step: 'always:a->b', eventType: 'xstate.init' },
        { step: 'entry:b', eventType: 'xstate.init' }
      ]);
    });

    it('should execute compound initial actions in root-to-leaf order', () => {
      const order: string[] = [];
      const machine = createMachine({
        entry: () => order.push('root.entry'),
        initial: {
          target: 'parent',
          actions: () => order.push('root.initial.actions')
        },
        states: {
          parent: {
            entry: () => order.push('parent.entry'),
            initial: {
              target: 'child',
              actions: () => order.push('parent.initial.actions')
            },
            states: {
              child: {
                entry: () => order.push('child.entry')
              }
            }
          }
        }
      });

      createActor(machine).start();
      expect(order).toEqual([
        'root.entry',
        'root.initial.actions',
        'parent.entry',
        'parent.initial.actions',
        'child.entry'
      ]);
    });
  });
```

- [x] **Step 2: Build the npm WASM package and run Vitest to verify the new tests fail**

Run: `npm run build:npm && npx vitest run tests/machine.test.ts`
Expected: FAIL because `createActor(machine)` prematurely executes `handle_always_transition()` before `a`'s `entry` assignment runs, leaving the actor in `'a'` and firing callbacks during `createActor`.

- [x] **Step 3: Update `StateNode::create_actor_method`, `Actor::get_snapshot`, `Actor::start`, and `create_actor` in `portable-xstate-wasm/src/lib.rs`**

In [`portable-xstate-wasm/src/lib.rs`](file:///Users/mike/.gemini/antigravity/worktrees/portable-xstate/platformio_native_design_plan/portable-xstate-wasm/src/lib.rs):

1. Remove lines 635–644 in `StateNode::create_actor_method` (the premature `if !has_snapshot { runner.handle_always_transition(); }` block).
2. In `create_actor` (around line 1666), set `runner.is_initialized = true;` when restoring `runner.machine.current_state` from `options.snapshot.value`.
3. In `Actor::get_snapshot` (lines 1310–1348), preview the initial state on a cloned machine when `!self.machine.is_started && !self.is_initialized`:

```rust
    #[wasm_bindgen(js_name = getSnapshot)]
    pub fn get_snapshot(&self) -> Snapshot {
        let preview;
        let machine_ref = if !self.machine.is_started && !self.is_initialized {
            let mut m = self.machine.clone();
            m.start(&self.guards, &IndexMap::new());
            preview = m;
            &preview
        } else {
            &self.machine
        };

        let active_paths = machine_ref.current_state.get_active_paths();
        let value_val = if active_paths.is_empty() && machine_ref.definition.states.is_empty() {
            js_sys::Object::new().into()
        } else if active_paths.len() == 1 {
            self.path_to_value_object(&active_paths[0])
        } else {
            let root_obj = js_sys::Object::new();
            for path in active_paths {
                self.merge_path_into_obj(&root_obj, &path);
            }
            root_obj.into()
        };
        let ctx_json = value_to_json(&machine_ref.context);
        let ctx_str = serde_json::to_string(&ctx_json).unwrap_or("{}".to_string());
        let ctx_str_clean = if ctx_str == "null" { "{}" } else { &ctx_str };
        let ctx_val = js_sys::JSON::parse(ctx_str_clean).unwrap_or(js_sys::Object::new().into());
        let history_obj = self.get_history_value_obj();
        let is_done = machine_ref
            .get_state_definition(&machine_ref.state())
            .map(|def| def.type_.as_deref() == Some("final"))
            .unwrap_or(false);
        let status = if self.is_stopped {
            "stopped".to_string()
        } else if is_done {
            "done".to_string()
        } else {
            "active".to_string()
        };

        Snapshot {
            value: value_val,
            context: ctx_val,
            status,
            machine_ref: machine_ref.clone(),
            guards: self.guards.clone(),
            history_value: history_obj.into(),
            config: self.config.clone(),
        }
    }
```

4. In `Actor::start` (lines 1400–1417), guard against repeated starts and only call `self.handle_always_transition()` when starting from a restored snapshot:

```rust
    pub fn start(&mut self) -> Actor {
        if self.machine.is_started {
            return self.clone();
        }

        if !self.is_initialized {
            self.machine.start(&self.guards, &self.actions);
            self.is_initialized = true;
        } else {
            self.machine.is_started = true;
            self.machine.revive_timers();
            self.handle_always_transition();
        }

        self.emit_snapshot();

        self.clone()
    }
```

- [x] **Step 4: Build WASM artifacts and run the full `portable-xstate` test suite**

Run: `npm run build && npm test`
Expected: PASS across `test:native`, `test:wasm`, `test:e2e`, `test:ffi`, and `test:unit`.

- [x] **Step 5: Commit Task 3**

```bash
git add portable-xstate-wasm/src/lib.rs tests/machine.test.ts
git commit -m "fix(wasm): defer initial always transitions and actions until Actor::start"
```

---

## Phase 2: PlatformIO `native` Environment Support and Upstream Sync [flowos: 7ed98601-a73b-46b3-8257-d9af9b81f660]

### Task 4: PlatformIO `native` Support, Testable `build_rust.py`, and Upstream Sync (`portable-xstate-platformio`) [flowos: ae3e3282-ac15-4f3f-ad7f-9ce39f5d1b5c]

**Files:**
- Modify: `/Users/mike/GitHub/portable-xstate-platformio/library.json:1-32`
- Modify: `/Users/mike/GitHub/portable-xstate-platformio/build_rust.py:1-176`
- Modify: `/Users/mike/GitHub/portable-xstate-platformio/test_build_rust.py:1-103`

**Interfaces:**
- Consumes: Pinned `portable-xstate` commit SHA from Phase 1 (`git rev-parse HEAD` in `portable-xstate`)
- Produces:
  - `resolve_target(platform, mcu) -> tuple[str | None, str, bool, bool]` in `build_rust.py`
  - `get_lib_release_dir(rust_dir, rust_target, is_native) -> str` in `build_rust.py`
  - `build_cargo_cmd(cargo_to_use, rust_target, is_riscv, is_native, manifest_path=CARGO_TOML) -> list[str]` in `build_rust.py`
  - `clone_dependency(active_env=None)` and `build_rust_lib(mcu, rust_target, active_env=None, *args, **kwargs)` in `build_rust.py`

- [x] **Step 1: Write failing unit tests in `/Users/mike/GitHub/portable-xstate-platformio/test_build_rust.py` that import `build_rust` directly**

Replace `/Users/mike/GitHub/portable-xstate-platformio/test_build_rust.py` with tests that `import build_rust` directly and assert:
1. `build_rust.resolve_target("espressif32", mcu)` maps `esp32`, `esp32s2`, `esp32s3` to `xtensa-*-espidf` (`is_riscv == False, is_native == False`), `esp32c3`, `esp32c6`, `esp32h2` to `riscv32*-esp-espidf` (`is_riscv == True, is_native == False`), and returns `rust_target is None, is_native == False` for unsupported `"stm32f4"`.
2. `build_rust.resolve_target("native", "")` detects the host triple via `rustc -vV` and returns `(host_triple, "cargo", False, True)`.
3. `build_rust.get_lib_release_dir(rust_dir, rust_target, is_native)` returns `<rust_dir>/target/release` when `is_native=True` and `<rust_dir>/target/<rust_target>/release` when `is_native=False`.
4. `build_rust.build_cargo_cmd(cargo_to_use, rust_target, is_riscv, is_native)` omits `-Z build-std` and `--target` when `is_native=True`, and includes `["-Z", "build-std=std,panic_abort", "--target", rust_target]` when `is_native=False`.
5. `build_rust.clone_dependency(active_env)` runs `git fetch --all` and retries `git checkout <DEPENDENCY_REF>` when the first `git checkout` raises `subprocess.CalledProcessError`.
6. `library.json` declares `"frameworks": ["*"]` and `"platforms": ["*"]`.

- [x] **Step 2: Run `python3 -m unittest test_build_rust.py` in `/Users/mike/GitHub/portable-xstate-platformio` to verify it fails**

Run: `python3 -m unittest test_build_rust.py` (with `Cwd` set to `/Users/mike/GitHub/portable-xstate-platformio`)
Expected: FAIL with `ModuleNotFoundError: No module named 'SCons'` when importing `build_rust`.

- [x] **Step 3: Update `library.json` and refactor `build_rust.py` in `/Users/mike/GitHub/portable-xstate-platformio`**

1. In [`/Users/mike/GitHub/portable-xstate-platformio/library.json`](file:///Users/mike/GitHub/portable-xstate-platformio/library.json#L25-L28), set:

```json
  "frameworks": [
    "*"
  ],
```

2. In [`/Users/mike/GitHub/portable-xstate-platformio/build_rust.py`](file:///Users/mike/GitHub/portable-xstate-platformio/build_rust.py):
   - Update `DEPENDENCY_REF` to the latest `portable-xstate` commit SHA containing Tasks 1–3.
   - Guard the SCons import so `import build_rust` works outside SCons:

```python
try:
    from SCons.Script import Import
    Import("env")
except Exception:
    env = None
```

   - Add helper functions:

```python
def _detect_host_rust_target():
    try:
        out = subprocess.check_output(["rustc", "-vV"], text=True)
        for line in out.splitlines():
            if line.startswith("host:"):
                return line.split(":", 1)[1].strip()
    except Exception:
        pass
    return "native"


def resolve_target(platform, mcu):
    platform_norm = (platform or "").lower()
    mcu_norm = (mcu or "").lower()
    if platform_norm == "native" or mcu_norm in ("", "native"):
        return _detect_host_rust_target(), "cargo", False, True

    rust_target = MCU_TO_RUST_TARGET.get(mcu_norm)
    if not rust_target:
        return None, "cargo", False, False

    is_riscv = rust_target.startswith("riscv32")
    return rust_target, "cargo", is_riscv, False


def get_lib_release_dir(rust_dir, rust_target, is_native):
    if is_native or not rust_target:
        return os.path.join(rust_dir, "target", "release")
    return os.path.join(rust_dir, "target", rust_target, "release")


def build_cargo_cmd(cargo_to_use, rust_target, is_riscv, is_native, manifest_path=CARGO_TOML):
    cmd = [
        cargo_to_use,
        "build",
        "--manifest-path",
        manifest_path,
        "--release",
    ]
    if not is_native and rust_target:
        cmd.extend(["-Z", "build-std=std,panic_abort", "--target", rust_target])
    return cmd
```

   - In `clone_dependency(active_env=None)`: if `git checkout <DEPENDENCY_REF>` raises `subprocess.CalledProcessError`, run `subprocess.check_call(["git", "-C", DEPENDENCY_DIR, "fetch", "--all"])` and retry `subprocess.check_call(["git", "-C", DEPENDENCY_DIR, "checkout", DEPENDENCY_REF])` before calling `active_env.Exit(1)`.
   - Wrap the top-level SCons environment detection and `build_rust_lib` invocation in `if env is not None:`.

- [x] **Step 4: Run `python3 -m unittest test_build_rust.py -v` in `/Users/mike/GitHub/portable-xstate-platformio` to verify all unit tests pass**

Run: `python3 -m unittest test_build_rust.py -v` (with `Cwd` set to `/Users/mike/GitHub/portable-xstate-platformio`)
Expected: PASS (`OK`).

- [x] **Step 5: Commit Task 4 in `/Users/mike/GitHub/portable-xstate-platformio`**

```bash
git add library.json build_rust.py test_build_rust.py
git commit -m "feat(native): support PlatformIO native builds and update portable-xstate ref"
```
