
#[cfg(test)]
mod tests {
    extern crate std;
    use crate::*;
    use indexmap::IndexMap;
    use alloc::vec;
    use alloc::string::ToString;

    #[test]
    fn test_parallel_history_repro_rust() {
        let definition = MachineDefinition {
            id: "machine".to_string(),
            type_: None,
            state_type: StateType::Compound,
            initial: "off".to_string(), initial_actions: vec![],
            context: None,
            types: None,
            invoke: vec![],
            states: indexmap::indexmap! {
                "off".to_string() => StateDefinition {
                    id: Some("off".to_string()),
                    state_type: StateType::Atomic,
                    type_: None,
                    initial: None, initial_actions: vec![],
                    entry: vec![],
                    exit: vec![],
                    after: indexmap::IndexMap::new(),
                    on: indexmap::indexmap! {
                        "SWITCH".to_string() => vec![TransitionDefinition {
                            target: Some(vec!["on".to_string()]),
                            cond: None,
                            actions: vec![],
                            reenter: false,
                        }],
                        "DEEP_POWER".to_string() => vec![TransitionDefinition {
                            target: Some(vec!["on.deepHistory".to_string()]),
                            cond: None,
                            actions: vec![],
                            reenter: false,
                        }],
                    },
                    states: indexmap::indexmap!{},
                    history: None,
                    history_target: None,
                    tags: vec![],
                    history_actions: vec![],
                    invoke: vec![],
                },
                "on".to_string() => StateDefinition {
                    id: Some("on".to_string()),
                    state_type: StateType::Parallel,
                    type_: Some("parallel".to_string()),
                    initial: None, initial_actions: vec![], // Parallel has no initial -> enters all children
                    entry: vec![],
                    exit: vec![],
                    after: indexmap::IndexMap::new(),
                    on: indexmap::indexmap! {
                        "POWER".to_string() => vec![TransitionDefinition {
                            target: Some(vec!["off".to_string()]),
                            cond: None,
                            actions: vec![],
                            reenter: false,
                        }],
                    },
                    states: indexmap::indexmap! {
                        "A".to_string() => StateDefinition {
                            id: Some("A".to_string()),
                            state_type: StateType::Compound,
                            type_: Some("compound".to_string()),
                            initial: Some("B".to_string()), initial_actions: vec![],
// Default B
                            entry: vec![],
                            exit: vec![],
                            after: indexmap::IndexMap::new(),
                            on: indexmap::indexmap!{},
                            states: indexmap::indexmap! {
                                "B".to_string() => StateDefinition {
                                    id: Some("B".to_string()),
                                    state_type: StateType::Atomic,
                                    type_: None,
                                    initial: None, initial_actions: vec![],
                                    entry: vec![],
                                    exit: vec![],
                                    after: indexmap::IndexMap::new(),
                                    on: indexmap::indexmap! {
                                        "INNER_A".to_string() => vec![TransitionDefinition {
                                            target: Some(vec!["C".to_string()]), // Relative C
                                            cond: None,
                                            actions: vec![],
                                            reenter: false,
                                        }],
                                    },
                                    states: indexmap::indexmap!{},
                                    history: None,
                                    history_target: None,
                                    tags: vec![],
                                    history_actions: vec![],
                                    invoke: vec![],
                                },
                                 "C".to_string() => StateDefinition {
                                    id: Some("C".to_string()),
                                    state_type: StateType::Compound,
                                    type_: Some("compound".to_string()),
                                    initial: Some("D".to_string()), initial_actions: vec![],
entry: vec![],
                                    exit: vec![],
                                    after: indexmap::IndexMap::new(),
                                    on: indexmap::indexmap!{},
                                    states: indexmap::indexmap! {
                                         "D".to_string() => StateDefinition {
                                            id: Some("D".to_string()),
                                            state_type: StateType::Atomic,
                                            type_: None,
                                            initial: None, initial_actions: vec![],
                                            entry: vec![],
                                            exit: vec![],
                                            after: indexmap::IndexMap::new(),
                                            on: indexmap::indexmap!{},
                                            states: indexmap::indexmap!{},
                                            history: None,
                                            history_target: None,
                                            tags: vec![],
                                            history_actions: vec![],
                                            invoke: vec![],
                                        },
                                    },
                                    history: None,
                                    history_target: None,
                                    tags: vec![],
                                    history_actions: vec![],
                                    invoke: vec![],
                                },
                            },
                            history: None,
                            history_target: None,
                            tags: vec![],
                            history_actions: vec![],
                            invoke: vec![],
                        },
                        "K".to_string() => StateDefinition {
                            id: Some("K".to_string()),
                            state_type: StateType::Compound,
                            type_: Some("compound".to_string()),
                            initial: Some("L".to_string()), initial_actions: vec![],
entry: vec![],
                            exit: vec![],
                            after: indexmap::IndexMap::new(),
                            on: indexmap::indexmap!{},
                            states: indexmap::indexmap! {
                                "L".to_string() => StateDefinition::default_atomic(),
                            },
                            history: None,
                            history_target: None,
                            tags: vec![],
                            history_actions: vec![],
                            invoke: vec![],
                        },
                        "deepHistory".to_string() => StateDefinition {
                            id: Some("deepHistory".to_string()),
                            state_type: StateType::History { deep: true },
                            type_: Some("history".to_string()),
                            initial: None, initial_actions: vec![],
                            entry: vec![],
                            exit: vec![],
                            after: indexmap::IndexMap::new(),
                            on: indexmap::indexmap!{},
                            states: indexmap::indexmap!{},
                            history: Some(HistoryType::Deep),
                            history_target: None,
                            tags: vec![],
                            history_actions: vec![],
                            invoke: vec![],
                        },
                    },
                    history: None,
                    history_target: None,
                    tags: vec![],
                    history_actions: vec![],
                    invoke: vec![],
                },
            },
            after: indexmap::IndexMap::new(),
            on: indexmap::indexmap!{},
            entry: vec![],
            exit: vec![],
            id_map: IndexMap::new(), // Populate if needed
        };

        let mut machine = RuntimeMachine::new(&definition).unwrap();
        machine.start(&IndexMap::new(), &IndexMap::new());
        
        // 1. Initial State: atomic "off"
        assert_eq!(machine.current_state, StateValue::Atomic("off".to_string()));

        // 2. SWITCH -> on
        machine.transition("SWITCH", &Value::Null, &IndexMap::new(), &IndexMap::new());
        // Expect: on.A.B, on.K.L
        std::println!("Step 2 State: {:?}", machine.current_state);

        // 3. INNER_A -> C
        machine.transition("INNER_A", &Value::Null, &IndexMap::new(), &IndexMap::new());
        // Expect: on.A.C.D
        std::println!("Step 3 State (Should be C.D): {:?}", machine.current_state);
        
        // VERIFY INNER_A WORKED
        let active = machine.current_state.get_active_paths();
        assert!(active.contains(&"on.A.C.D".to_string()), "INNER_A failed to reach C.D!");

        // 4. POWER -> off
        machine.transition("POWER", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert_eq!(machine.current_state, StateValue::Atomic("off".to_string()));
        
        // Check history storage
        if let Some(hist) = machine.history_value.get("on") {
             std::println!("History at 'on': {:?}", hist);
        } else {
             std::println!("NO HISTORY at 'on'!");
        }

        // 5. DEEP_POWER -> on.deepHistory
        machine.transition("DEEP_POWER", &Value::Null, &IndexMap::new(), &IndexMap::new());
        std::println!("Step 5 State (Restored): {:?}", machine.current_state);
        
        let active_final = machine.current_state.get_active_paths();
        assert!(active_final.contains(&"on.A.C.D".to_string()), "History failed to restore C.D!");
    }

    #[test]
    fn test_combinatorial_machine() {
        let definition = MachineDefinition {
            id: "toggle".to_string(),
            type_: None,
            state_type: StateType::Parallel, // Combinatorial is implicit parallel of empty states
            initial: "".to_string(),
            initial_actions: vec![],
            context: Some(Value::Object(indexmap::indexmap! {
                "value".to_string() => Value::Number(42.0)
            })),
            types: None,
            invoke: vec![],
            states: IndexMap::new(),
            after: indexmap::IndexMap::new(),
            on: indexmap::indexmap! {
                "INC".to_string() => vec![TransitionDefinition {
                    target: None,
                    cond: None,
                    actions: vec![Action::Assign(AssignAction {
                        action_type: "xstate.assign".to_string(),
                        assignment: Value::Object(indexmap::indexmap! {
                            "value".to_string() => Value::Number(43.0) // simplified for now
                        }),
                    })],
                    reenter: false,
                }],
            },
            entry: vec![],
            exit: vec![],
            id_map: IndexMap::new(),
        };

        let mut machine = RuntimeMachine::new(&definition).unwrap();
        machine.start(&IndexMap::new(), &IndexMap::new());
        // Combinatorial machines (no states) should have an empty object state or something that represents root
        // In our current implementation, empty states -> Parallel(IndexMap::new())
        machine.transition("INC", &Value::Null, &IndexMap::new(), &IndexMap::new());
        
        let ctx = machine.context.as_object().unwrap();
        assert_eq!(ctx.get("value").unwrap(), &Value::Number(43.0));
    }

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

        assert!(machine.matches("final_state"));
        assert!(!machine.matches("boot"));
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
        assert!(machine.matches("active"));

        // At t=160ms, transient's 50ms timer would have expired if not cancelled
        now_ms.set(160);
        machine.tick(&IndexMap::new(), &IndexMap::new());
        assert!(machine.matches("active"));

        // At t=200ms, active's 100ms timer expires
        now_ms.set(200);
        machine.tick(&IndexMap::new(), &IndexMap::new());
        assert!(machine.matches("timed_out"));
    }

    #[test]
    fn test_matches_atomic_compound_and_parallel() {
        let atomic = StateValue::Atomic("green".to_string());
        assert!(atomic.matches("green"));
        assert!(!atomic.matches("yellow"));
        assert!(!atomic.matches("gree"));
        assert!(!atomic.matches("green.sub"));
        assert!(!atomic.matches(""));

        let compound = StateValue::Compound(
            "Display".to_string(),
            Box::new(StateValue::Compound(
                "Theme".to_string(),
                Box::new(StateValue::Atomic("Light".to_string())),
            )),
        );
        assert!(compound.matches("Display"));
        assert!(compound.matches("Display.Theme"));
        assert!(compound.matches("Display.Theme.Light"));
        assert!(!compound.matches("Display.Theme.Dark"));
        assert!(!compound.matches("Display.Theme.Light.Extra"));
        assert!(!compound.matches("Display.Them"));
        assert!(!compound.matches("Disp"));

        let parallel = StateValue::Compound(
            "Display".to_string(),
            Box::new(StateValue::Parallel(indexmap::indexmap! {
                "Theme".to_string() => StateValue::Compound(
                    "Theme".to_string(),
                    Box::new(StateValue::Atomic("Light".to_string())),
                ),
                "Power".to_string() => StateValue::Compound(
                    "Power".to_string(),
                    Box::new(StateValue::Atomic("On".to_string())),
                ),
            })),
        );
        assert!(parallel.matches("Display"));
        assert!(parallel.matches("Display.Theme"));
        assert!(parallel.matches("Display.Theme.Light"));
        assert!(parallel.matches("Display.Power"));
        assert!(parallel.matches("Display.Power.On"));
        assert!(!parallel.matches("Display.Power.Off"));
        assert!(!parallel.matches("Display.Theme.Dark"));

        // Partial StateValue matching
        let partial_one_region = StateValue::Compound(
            "Display".to_string(),
            Box::new(StateValue::Compound(
                "Power".to_string(),
                Box::new(StateValue::Atomic("On".to_string())),
            )),
        );
        assert!(parallel.matches_state_value(&partial_one_region));

        let partial_both_regions = StateValue::Compound(
            "Display".to_string(),
            Box::new(StateValue::Parallel(indexmap::indexmap! {
                "Theme".to_string() => StateValue::Compound(
                    "Theme".to_string(),
                    Box::new(StateValue::Atomic("Light".to_string())),
                ),
                "Power".to_string() => StateValue::Compound(
                    "Power".to_string(),
                    Box::new(StateValue::Atomic("On".to_string())),
                ),
            })),
        );
        assert!(parallel.matches_state_value(&partial_both_regions));

        let partial_mismatch = StateValue::Compound(
            "Display".to_string(),
            Box::new(StateValue::Parallel(indexmap::indexmap! {
                "Theme".to_string() => StateValue::Compound(
                    "Theme".to_string(),
                    Box::new(StateValue::Atomic("Light".to_string())),
                ),
                "Power".to_string() => StateValue::Compound(
                    "Power".to_string(),
                    Box::new(StateValue::Atomic("Off".to_string())),
                ),
            })),
        );
        assert!(!parallel.matches_state_value(&partial_mismatch));

        // Empty sentinel Atomic("") should yield no active paths
        assert!(StateValue::Atomic("".to_string()).get_active_paths().is_empty());

        // Atomic region inside a parallel state ("a" => Atomic("a"))
        let parallel_with_atomic_region = StateValue::Compound(
            "P".to_string(),
            Box::new(StateValue::Parallel(indexmap::indexmap! {
                "a".to_string() => StateValue::Atomic("a".to_string()),
                "b".to_string() => StateValue::Compound(
                    "b".to_string(),
                    Box::new(StateValue::Atomic("b1".to_string())),
                ),
            })),
        );
        assert_eq!(
            parallel_with_atomic_region.get_active_paths(),
            vec!["P.a".to_string(), "P.b.b1".to_string()]
        );
        assert!(parallel_with_atomic_region.matches("P.a"));
        assert!(!parallel_with_atomic_region.matches("P.a.a"));

        // Same-named child state inside a parallel region must not false-match shallower state
        let partial_deeper_same_name = StateValue::Compound(
            "Display".to_string(),
            Box::new(StateValue::Parallel(indexmap::indexmap! {
                "Theme".to_string() => StateValue::Compound(
                    "Theme".to_string(),
                    Box::new(StateValue::Compound(
                        "Theme".to_string(),
                        Box::new(StateValue::Atomic("Light".to_string())),
                    )),
                ),
                "Power".to_string() => StateValue::Compound(
                    "Power".to_string(),
                    Box::new(StateValue::Atomic("On".to_string())),
                ),
            })),
        );
        assert!(!parallel.matches_state_value(&partial_deeper_same_name));

        // Empty parallel sub-branch ({ Display: {} } vs { Display: { Theme: 'Light', Nonexistent: {} } })
        let partial_empty_compound = StateValue::Compound(
            "Display".to_string(),
            Box::new(StateValue::Parallel(IndexMap::new())),
        );
        assert!(parallel.matches_state_value(&partial_empty_compound));

        let partial_nonexistent_empty_branch = StateValue::Compound(
            "Display".to_string(),
            Box::new(StateValue::Parallel(indexmap::indexmap! {
                "Theme".to_string() => StateValue::Compound(
                    "Theme".to_string(),
                    Box::new(StateValue::Atomic("Light".to_string())),
                ),
                "Nonexistent".to_string() => StateValue::Compound(
                    "Nonexistent".to_string(),
                    Box::new(StateValue::Parallel(IndexMap::new())),
                ),
            })),
        );
        assert!(!parallel.matches_state_value(&partial_nonexistent_empty_branch));
    }

    #[test]
    fn test_simulated_clock_cascading_timers_wait_to_sleep() {
        use alloc::rc::Rc;
        use core::cell::RefCell;

        let json = r#"{
            "id": "april_sleep",
            "initial": "Wait",
            "states": {
                "Wait": {
                    "after": {
                        "60000": { "target": "Sleep", "actions": ["onEnterSleep"] }
                    }
                },
                "Sleep": {
                    "after": {
                        "3600000": { "target": "Wake", "actions": ["onWake"] }
                    }
                },
                "Wake": {}
            }
        }"#;

        let clock = SimulatedClock::new();
        let mut machine = RuntimeMachine::from_json(json).unwrap();
        machine.register_clock(&clock);

        let timestamps: Rc<RefCell<Vec<(String, u64, String)>>> = Rc::new(RefCell::new(Vec::new()));
        let ts_sleep = timestamps.clone();
        let clock_sleep = clock.clone();
        let ts_wake = timestamps.clone();
        let clock_wake = clock.clone();

        let actions: IndexMap<String, ActionFunction> = indexmap::indexmap! {
            "onEnterSleep".to_string() => Rc::new(move |_ctx: &mut Value, evt: &Value, _params: &Option<Value>| {
                let evt_type = evt.as_object().and_then(|m| m.get("type")).and_then(|v| v.as_string()).cloned().unwrap_or_default();
                ts_sleep.borrow_mut().push(("Sleep".to_string(), clock_sleep.now(), evt_type));
            }) as ActionFunction,
            "onWake".to_string() => Rc::new(move |_ctx: &mut Value, evt: &Value, _params: &Option<Value>| {
                let evt_type = evt.as_object().and_then(|m| m.get("type")).and_then(|v| v.as_string()).cloned().unwrap_or_default();
                ts_wake.borrow_mut().push(("Wake".to_string(), clock_wake.now(), evt_type));
            }) as ActionFunction,
        };

        machine.start(&IndexMap::new(), &actions);
        assert!(machine.matches("Wait"));
        assert_eq!(machine.next_timer_deadline(), Some(60_000));

        // Advance 24 hours (86,400,000 ms) in a single call
        clock.increment(86_400_000, &mut machine, &IndexMap::new(), &actions);

        assert!(machine.matches("Wake"));
        assert_eq!(clock.now(), 86_400_000);
        assert_eq!(machine.next_timer_deadline(), None);
        assert_eq!(
            *timestamps.borrow(),
            vec![
                ("Sleep".to_string(), 60_000, "xstate.after.Wait".to_string()),
                ("Wake".to_string(), 3_660_000, "xstate.after.Sleep".to_string()),
            ]
        );
    }

    #[test]
    fn test_simulated_clock_hierarchical_polling_and_parent_timeout_tie_break() {
        use alloc::rc::Rc;
        use core::cell::Cell;

        let json = r##"{
            "id": "network",
            "initial": "ConnectToNetwork",
            "states": {
                "ConnectToNetwork": {
                    "initial": "Polling",
                    "after": {
                        "300000": "Timeout"
                    },
                    "states": {
                        "Polling": {
                            "entry": ["poll"],
                            "after": {
                                "500": [
                                    { "target": "#Connected", "guard": "isConnected" },
                                    { "target": "Polling", "reenter": true }
                                ]
                            }
                        }
                    }
                },
                "Connected": {
                    "id": "Connected"
                },
                "Timeout": {}
            }
        }"##;

        // Case A: Guard stays false -> polls 600 times (at t=0, 500, ..., 299500), then parent timeout fires at t=300000 before a 601st poll
        {
            let clock = SimulatedClock::new();
            let mut machine = RuntimeMachine::from_json(json).unwrap();
            machine.register_clock(&clock);

            let poll_count = Rc::new(Cell::new(0u32));
            let poll_count_ref = poll_count.clone();

            let guards: IndexMap<String, GuardFunction> = indexmap::indexmap! {
                "isConnected".to_string() => Rc::new(|_ctx: &Value, _evt: &Value, _params: &Option<Value>| false) as GuardFunction,
            };
            let actions: IndexMap<String, ActionFunction> = indexmap::indexmap! {
                "poll".to_string() => Rc::new(move |_ctx: &mut Value, _evt: &Value, _params: &Option<Value>| {
                    poll_count_ref.set(poll_count_ref.get() + 1);
                }) as ActionFunction,
            };

            machine.start(&guards, &actions);
            assert!(machine.matches("ConnectToNetwork.Polling"));
            assert_eq!(poll_count.get(), 1); // Initial entry at t=0

            clock.increment(300_000, &mut machine, &guards, &actions);

            assert!(machine.matches("Timeout"));
            assert_eq!(clock.now(), 300_000);
            assert_eq!(poll_count.get(), 600);
            assert_eq!(machine.next_timer_deadline(), None);
        }

        // Case B: Guard becomes true on the 10th poll (at t=4500), transitioning to Connected at t=5000 and cancelling parent timeout
        {
            let clock = SimulatedClock::new();
            let mut machine = RuntimeMachine::from_json(json).unwrap();
            machine.register_clock(&clock);

            let poll_count = Rc::new(Cell::new(0u32));
            let is_connected = Rc::new(Cell::new(false));
            let poll_count_ref = poll_count.clone();
            let is_connected_action = is_connected.clone();
            let is_connected_guard = is_connected.clone();

            let guards: IndexMap<String, GuardFunction> = indexmap::indexmap! {
                "isConnected".to_string() => Rc::new(move |_ctx: &Value, _evt: &Value, _params: &Option<Value>| {
                    is_connected_guard.get()
                }) as GuardFunction,
            };
            let actions: IndexMap<String, ActionFunction> = indexmap::indexmap! {
                "poll".to_string() => Rc::new(move |_ctx: &mut Value, _evt: &Value, _params: &Option<Value>| {
                    let next = poll_count_ref.get() + 1;
                    poll_count_ref.set(next);
                    if next == 10 {
                        is_connected_action.set(true);
                    }
                }) as ActionFunction,
            };

            machine.start(&guards, &actions);
            clock.increment(300_000, &mut machine, &guards, &actions);

            assert!(machine.matches("Connected"));
            assert_eq!(clock.now(), 300_000);
            assert_eq!(poll_count.get(), 10);
            assert_eq!(machine.next_timer_deadline(), None);
        }
    }

    #[test]
    fn test_simulated_clock_high_frequency_polling_with_parent_timeout() {
        use alloc::rc::Rc;
        use core::cell::Cell;

        let json = r#"{
            "id": "ntp_sync",
            "initial": "StartNetworkTimeSync",
            "states": {
                "StartNetworkTimeSync": {
                    "initial": "Polling",
                    "after": {
                        "60000": "SyncTimeout"
                    },
                    "states": {
                        "Polling": {
                            "entry": ["tickPoll"],
                            "after": {
                                "1": { "target": "Polling", "reenter": true }
                            }
                        }
                    }
                },
                "SyncTimeout": {}
            }
        }"#;

        let clock = SimulatedClock::new();
        let mut machine = RuntimeMachine::from_json(json).unwrap();
        machine.register_clock(&clock);

        let polls = Rc::new(Cell::new(0u32));
        let polls_ref = polls.clone();
        let actions: IndexMap<String, ActionFunction> = indexmap::indexmap! {
            "tickPoll".to_string() => Rc::new(move |_ctx: &mut Value, _evt: &Value, _params: &Option<Value>| {
                polls_ref.set(polls_ref.get() + 1);
            }) as ActionFunction,
        };

        machine.start(&IndexMap::new(), &actions);
        clock.increment(60_000, &mut machine, &IndexMap::new(), &actions);

        assert!(machine.matches("SyncTimeout"));
        assert_eq!(clock.now(), 60_000);
        assert_eq!(polls.get(), 60_000);
    }

    #[test]
    fn test_simulated_clock_zero_delay_loop_is_bounded() {
        use alloc::rc::Rc;
        use core::cell::Cell;

        let json = r#"{
            "id": "zero_loop",
            "initial": "Loop",
            "states": {
                "Loop": {
                    "entry": ["inc"],
                    "after": {
                        "0": { "target": "Loop", "reenter": true }
                    }
                }
            }
        }"#;

        let clock = SimulatedClock::new();
        let mut machine = RuntimeMachine::from_json(json).unwrap();
        machine.register_clock(&clock);

        let count = Rc::new(Cell::new(0u32));
        let count_ref = count.clone();
        let actions: IndexMap<String, ActionFunction> = indexmap::indexmap! {
            "inc".to_string() => Rc::new(move |_ctx: &mut Value, _evt: &Value, _params: &Option<Value>| {
                count_ref.set(count_ref.get() + 1);
            }) as ActionFunction,
        };

        machine.start(&IndexMap::new(), &actions);
        clock.increment(1000, &mut machine, &IndexMap::new(), &actions);

        assert!(machine.matches("Loop"));
        assert_eq!(clock.now(), 1000);
        assert!(count.get() > 1 && count.get() <= 10_100);
    }

    #[test]
    fn test_simulated_clock_set_before_start_and_reentrant_transition_at_t_fire() {
        use alloc::rc::Rc;
        use core::cell::Cell;

        let json = r#"{
            "id": "reentrant_timer",
            "initial": "Boot",
            "states": {
                "Boot": {
                    "after": {
                        "500": [
                            { "target": "NeverTaken", "guard": "alwaysFalse" },
                            { "target": "Mid", "actions": ["sendReentrant"] }
                        ]
                    }
                },
                "Mid": {
                    "on": {
                        "REENTRANT": "ArmedAtTFire"
                    }
                },
                "ArmedAtTFire": {
                    "after": {
                        "1000": "Done"
                    }
                },
                "NeverTaken": {},
                "Done": {}
            }
        }"#;

        let clock = SimulatedClock::new();
        let mut machine = RuntimeMachine::from_json(json).unwrap();
        machine.register_clock(&clock);

        // Set clock to 2500 before start() -> Boot's 500ms timer must anchor at 2500 + 500 = 3000ms
        clock.set(2500, &mut machine, &IndexMap::new(), &IndexMap::new());
        assert_eq!(clock.now(), 2500);

        let machine_ptr: Rc<Cell<*mut RuntimeMachine>> = Rc::new(Cell::new(&mut machine as *mut RuntimeMachine));
        let machine_ptr_cb = machine_ptr.clone();
        let fired_done_at = Rc::new(Cell::new(0u64));
        let fired_done_cb = fired_done_at.clone();
        let clock_cb = clock.clone();

        let guards: IndexMap<String, GuardFunction> = indexmap::indexmap! {
            "alwaysFalse".to_string() => Rc::new(|_ctx: &Value, _evt: &Value, _params: &Option<Value>| false) as GuardFunction,
        };
        let actions: IndexMap<String, ActionFunction> = indexmap::indexmap! {
            "sendReentrant".to_string() => Rc::new(move |_ctx: &mut Value, _evt: &Value, _params: &Option<Value>| {
                assert_eq!(clock_cb.now(), 3000);
                fired_done_cb.set(clock_cb.now());
                unsafe {
                    let m = &mut *machine_ptr_cb.get();
                    m.transition("REENTRANT", &Value::Null, &IndexMap::new(), &IndexMap::new());
                }
            }) as ActionFunction,
        };

        machine.start(&guards, &actions);
        assert_eq!(machine.next_timer_deadline(), Some(3000));

        // Update pointer before calling clock.set
        machine_ptr.set(&mut machine as *mut RuntimeMachine);
        clock.set(5000, &mut machine, &guards, &actions);

        // At t=3000: Boot -> Mid, sendReentrant queues REENTRANT, which transitions Mid -> ArmedAtTFire at t=3000,
        // arming ArmedAtTFire's 1000ms timer for t=4000, which then fires at t=4000 -> Done!
        assert_eq!(fired_done_at.get(), 3000);
        assert!(machine.matches("Done"));
        assert_eq!(clock.now(), 5000);
    }

    #[test]
    fn test_wildcard_and_partial_wildcard_transitions() {
        use alloc::rc::Rc;

        // 1. Wildcard fallback and explicit priority over wildcard
        let json = r#"{
            "initial": "A",
            "states": {
                "A": {
                    "on": {
                        "*": "Wild",
                        "NEXT": "Explicit"
                    }
                },
                "Wild": {},
                "Explicit": {}
            }
        }"#;

        let mut m1 = RuntimeMachine::from_json(json).unwrap();
        m1.start(&IndexMap::new(), &IndexMap::new());
        m1.transition("BAR", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert!(m1.matches("Wild"));

        let mut m2 = RuntimeMachine::from_json(json).unwrap();
        m2.start(&IndexMap::new(), &IndexMap::new());
        m2.transition("NEXT", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert!(m2.matches("Explicit"));

        // 2. Specificity ordering: explicit > longer partial > shorter partial, with guarded fallback
        let json_partial = r#"{
            "initial": "A",
            "states": {
                "A": {
                    "on": {
                        "foo.*": "ShortPartial",
                        "foo.bar.*": {
                            "target": "LongPartial",
                            "guard": "allowLong"
                        },
                        "foo.bar": "Exact"
                    }
                },
                "ShortPartial": {},
                "LongPartial": {},
                "Exact": {}
            }
        }"#;

        // Exact wins over foo.* even for foo.bar
        let mut m_exact = RuntimeMachine::from_json(json_partial).unwrap();
        m_exact.start(&IndexMap::new(), &IndexMap::new());
        m_exact.transition("foo.bar", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert!(m_exact.matches("Exact"));

        // Longer partial wins over shorter partial when guard passes
        let guards_true: IndexMap<String, GuardFunction> = indexmap::indexmap! {
            "allowLong".to_string() => Rc::new(|_ctx: &Value, _evt: &Value, _params: &Option<Value>| true) as GuardFunction,
        };
        let mut m_long = RuntimeMachine::from_json(json_partial).unwrap();
        m_long.start(&guards_true, &IndexMap::new());
        m_long.transition("foo.bar.baz", &Value::Null, &guards_true, &IndexMap::new());
        assert!(m_long.matches("LongPartial"));

        // Guarded fallback from longer partial to shorter partial when guard returns false
        let guards_false: IndexMap<String, GuardFunction> = indexmap::indexmap! {
            "allowLong".to_string() => Rc::new(|_ctx: &Value, _evt: &Value, _params: &Option<Value>| false) as GuardFunction,
        };
        let mut m_fallback = RuntimeMachine::from_json(json_partial).unwrap();
        m_fallback.start(&guards_false, &IndexMap::new());
        m_fallback.transition("foo.bar.baz", &Value::Null, &guards_false, &IndexMap::new());
        assert!(m_fallback.matches("ShortPartial"));

        // 3. Prefix matching (+0, +1, +n) and rejection of non-tokenized / infix wildcards
        assert!(crate::matches_event_descriptor("event", "event.*"));
        assert!(crate::matches_event_descriptor("event.whatever", "event.*"));
        assert!(crate::matches_event_descriptor("event.first.second", "event.*"));
        assert!(crate::matches_event_descriptor("event.foo.bar.first.second", "event.foo.bar.*"));
        assert!(!crate::matches_event_descriptor("eventually", "event.*"));
        assert!(!crate::matches_event_descriptor("eventually.event", "event.*"));
        assert!(!crate::matches_event_descriptor("event", "event*"));
        assert!(!crate::matches_event_descriptor("eventually", "event*"));
        assert!(!crate::matches_event_descriptor("event.foo.bar.first.second", "event.*.bar.*"));
        assert!(!crate::matches_event_descriptor("whatever.event", "*.event.*"));
        assert!(!crate::matches_event_descriptor("eventually.bar.baz", "event*.bar.*"));
        assert!(!crate::matches_event_descriptor("prevent.whatever", "*event.*"));
    }
}
