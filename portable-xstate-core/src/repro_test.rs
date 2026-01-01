
#[cfg(test)]
mod tests {
    extern crate std;
    use crate::*;
    use indexmap::IndexMap;
    use alloc::vec;
    use alloc::string::ToString;
    use alloc::boxed::Box;

    #[test]
    fn test_parallel_history_repro_rust() {
        let definition = MachineDefinition {
            id: "machine".to_string(),
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
}
