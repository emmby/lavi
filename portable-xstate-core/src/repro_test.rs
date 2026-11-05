
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
                "Theme".to_string() => StateValue::Atomic("Light".to_string()),
                "Power".to_string() => StateValue::Atomic("On".to_string()),
            })),
        );
        assert!(parallel.matches_state_value(&partial_both_regions));

        let partial_mismatch = StateValue::Compound(
            "Display".to_string(),
            Box::new(StateValue::Parallel(indexmap::indexmap! {
                "Theme".to_string() => StateValue::Atomic("Light".to_string()),
                "Power".to_string() => StateValue::Atomic("Off".to_string()),
            })),
        );
        assert!(!parallel.matches_state_value(&partial_mismatch));
    }
}
