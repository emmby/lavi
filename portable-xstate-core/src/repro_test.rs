
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
                            on: indexmap::indexmap!{},
                            states: indexmap::indexmap! {
                                "B".to_string() => StateDefinition {
                                    id: Some("B".to_string()),
                                    state_type: StateType::Atomic,
                                    type_: None,
                                    initial: None, initial_actions: vec![],
                                    entry: vec![],
                                    exit: vec![],
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
                                    on: indexmap::indexmap!{},
                                    states: indexmap::indexmap! {
                                         "D".to_string() => StateDefinition {
                                            id: Some("D".to_string()),
                                            state_type: StateType::Atomic,
                                            type_: None,
                                            initial: None, initial_actions: vec![],
                                            entry: vec![],
                                            exit: vec![],
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
            on: indexmap::indexmap!{},
            entry: vec![],
            exit: vec![],
            id_map: IndexMap::new(), // Populate if needed
        };

        let mut machine = RuntimeMachine::new(&definition).unwrap();
        machine.start(&IndexMap::new());
        
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
        machine.start(&IndexMap::new());
        // Combinatorial machines (no states) should have an empty object state or something that represents root
        // In our current implementation, empty states -> Parallel(IndexMap::new())
        
        machine.transition("INC", &Value::Null, &IndexMap::new(), &IndexMap::new());
        
        let ctx = machine.context.as_object().unwrap();
        assert_eq!(ctx.get("value").unwrap(), &Value::Number(43.0));
    }

    #[test]
    fn test_unstarted_machine_drops_events() {
        let definition = MachineDefinition {
            id: "test".to_string(),
            type_: None,
            state_type: StateType::Atomic,
            initial: "on".to_string(),
            initial_actions: vec![],
            states: indexmap::indexmap! {
                "on".to_string() => StateDefinition {
                    on: indexmap::indexmap! {
                        "TEST_EVENT".to_string() => vec![TransitionDefinition {
                            target: Some(vec!["off".to_string()]),
                            cond: None,
                            actions: vec![],
                            reenter: false,
                        }]
                    },
                    ..StateDefinition::default_atomic()
                },
                "off".to_string() => StateDefinition::default_atomic()
            },
            on: indexmap::indexmap!{},
            entry: vec![],
            exit: vec![],
            invoke: vec![],
            context: None,
            types: None,
            id_map: IndexMap::new(),
        };
        let mut machine = RuntimeMachine::new(&definition).unwrap();
        
        // Ensure transition without start returns None
        let result = machine.transition("TEST_EVENT", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert_eq!(result, None);
        
        machine.start(&IndexMap::new());
        // Transition starts computing
        let result2 = machine.transition("TEST_EVENT", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert_eq!(result2, Some(StateValue::Atomic("off".to_string())));
    }
}
