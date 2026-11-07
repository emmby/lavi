use std::cell::RefCell;
use std::rc::Rc;
use lavi::{IndexMap, IndexMapExt};
use lavi::{from_json_str, RuntimeMachine, Value, ActionFunction};

fn make_action(trace: Rc<RefCell<Vec<String>>>, name: &'static str) -> ActionFunction {
    Rc::new(move |_ctx, _evt, _params| {
        trace.borrow_mut().push(name.to_string());
    })
}

#[test]
fn test_lcca_ancestor_depth_progression() {
    // 5-level deep hierarchy: a -> b -> c -> d -> e
    let json = r##"{
        "id": "root",
        "entry": "entry_root",
        "exit": "exit_root",
        "initial": {
            "target": "a",
            "actions": "init_root"
        },
        "states": {
            "a": {
                "id": "state_a",
                "entry": "entry_a",
                "exit": "exit_a",
                "initial": {
                    "target": "b",
                    "actions": "init_a"
                },
                "states": {
                    "b": {
                        "id": "state_b",
                        "entry": "entry_b",
                        "exit": "exit_b",
                        "initial": {
                            "target": "c",
                            "actions": "init_b"
                        },
                        "states": {
                            "c": {
                                "id": "state_c",
                                "entry": "entry_c",
                                "exit": "exit_c",
                                "initial": {
                                    "target": "d",
                                    "actions": "init_c"
                                },
                                "states": {
                                    "d": {
                                        "id": "state_d",
                                        "entry": "entry_d",
                                        "exit": "exit_d",
                                        "initial": {
                                            "target": "e",
                                            "actions": "init_d"
                                        },
                                        "states": {
                                            "e": {
                                                "id": "state_e",
                                                "entry": "entry_e",
                                                "exit": "exit_e",
                                                "on": {
                                                    "TO_SELF": {
                                                        "target": "e",
                                                        "reenter": true
                                                    },
                                                    "TO_PARENT_D": {
                                                        "target": "#state_d"
                                                    },
                                                    "TO_GRANDPARENT_C": {
                                                        "target": "#state_c"
                                                    },
                                                    "TO_GREAT_GRANDPARENT_B": {
                                                        "target": "#state_b"
                                                    },
                                                    "TO_TOP_LEVEL_A": {
                                                        "target": "#state_a"
                                                    },
                                                    "TO_ROOT_REENTER": {
                                                        "target": "#root",
                                                        "reenter": true
                                                    },
                                                    "TO_ROOT_NO_REENTER": {
                                                        "target": "#root",
                                                        "reenter": false
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }"##;

    let trace = Rc::new(RefCell::new(Vec::new()));
    let action_names = [
        "entry_root", "exit_root", "init_root",
        "entry_a", "exit_a", "init_a",
        "entry_b", "exit_b", "init_b",
        "entry_c", "exit_c", "init_c",
        "entry_d", "exit_d", "init_d",
        "entry_e", "exit_e",
    ];

    let mut actions: IndexMap<String, ActionFunction> = IndexMap::new();
    for name in action_names {
        actions.insert(name.to_string(), make_action(trace.clone(), name));
    }
    let guards = IndexMap::new();

    let def = from_json_str(json).expect("valid machine");
    let mut machine = RuntimeMachine::new(&def).unwrap();

    // 1. Start machine
    machine.start(&guards, &actions);
    assert!(machine.matches("a.b.c.d.e"));
    assert_eq!(
        *trace.borrow(),
        vec![
            "entry_root", "init_root",
            "entry_a", "init_a",
            "entry_b", "init_b",
            "entry_c", "init_c",
            "entry_d", "init_d",
            "entry_e"
        ]
    );

    // 2. Self transition on atomic state e
    trace.borrow_mut().clear();
    machine.transition("TO_SELF", &Value::Null, &guards, &actions);
    assert!(machine.matches("a.b.c.d.e"));
    assert_eq!(*trace.borrow(), vec!["exit_e", "entry_e"]);

    // 3. Transition to immediate parent d (#state_d)
    // Ancestor target: should exit e, exit d; enter d, take d's initial (init_d), enter e.
    // Parent c, b, a must NOT exit or enter.
    trace.borrow_mut().clear();
    machine.transition("TO_PARENT_D", &Value::Null, &guards, &actions);
    assert!(machine.matches("a.b.c.d.e"));
    assert_eq!(*trace.borrow(), vec!["exit_e", "exit_d", "entry_d", "init_d", "entry_e"]);

    // 4. Transition to grandparent c (#state_c)
    // Ancestor target: exit e, d, c; enter c, init_c, enter d, init_d, enter e.
    // b, a must NOT exit or enter.
    trace.borrow_mut().clear();
    machine.transition("TO_GRANDPARENT_C", &Value::Null, &guards, &actions);
    assert!(machine.matches("a.b.c.d.e"));
    assert_eq!(
        *trace.borrow(),
        vec![
            "exit_e", "exit_d", "exit_c",
            "entry_c", "init_c",
            "entry_d", "init_d",
            "entry_e"
        ]
    );

    // 5. Transition to great-grandparent b (#state_b)
    trace.borrow_mut().clear();
    machine.transition("TO_GREAT_GRANDPARENT_B", &Value::Null, &guards, &actions);
    assert!(machine.matches("a.b.c.d.e"));
    assert_eq!(
        *trace.borrow(),
        vec![
            "exit_e", "exit_d", "exit_c", "exit_b",
            "entry_b", "init_b",
            "entry_c", "init_c",
            "entry_d", "init_d",
            "entry_e"
        ]
    );

    // 6. Transition to top-level state a (#state_a)
    // Root must NOT exit or enter!
    trace.borrow_mut().clear();
    machine.transition("TO_TOP_LEVEL_A", &Value::Null, &guards, &actions);
    assert!(machine.matches("a.b.c.d.e"));
    assert_eq!(
        *trace.borrow(),
        vec![
            "exit_e", "exit_d", "exit_c", "exit_b", "exit_a",
            "entry_a", "init_a",
            "entry_b", "init_b",
            "entry_c", "init_c",
            "entry_d", "init_d",
            "entry_e"
        ]
    );

    // 7. Transition to root with reenter: true (#root)
    // Root MUST exit and enter!
    trace.borrow_mut().clear();
    machine.transition("TO_ROOT_REENTER", &Value::Null, &guards, &actions);
    assert!(machine.matches("a.b.c.d.e"));
    assert_eq!(
        *trace.borrow(),
        vec![
            "exit_e", "exit_d", "exit_c", "exit_b", "exit_a", "exit_root",
            "entry_root", "init_root",
            "entry_a", "init_a",
            "entry_b", "init_b",
            "entry_c", "init_c",
            "entry_d", "init_d",
            "entry_e"
        ]
    );

    // 8. Transition to root with reenter: false (#root)
    // Root MUST NOT exit and MUST NOT enter, but all children exit and reenter through initial
    trace.borrow_mut().clear();
    machine.transition("TO_ROOT_NO_REENTER", &Value::Null, &guards, &actions);
    assert!(machine.matches("a.b.c.d.e"));
    assert_eq!(
        *trace.borrow(),
        vec![
            "exit_e", "exit_d", "exit_c", "exit_b", "exit_a",
            "entry_a", "init_a",
            "entry_b", "init_b",
            "entry_c", "init_c",
            "entry_d", "init_d",
            "entry_e"
        ]
    );
}

#[test]
fn test_compound_self_transition_reenter_flag() {
    let json = r##"{
        "initial": "parent",
        "states": {
            "parent": {
                "entry": "entry_parent",
                "exit": "exit_parent",
                "initial": {
                    "target": "child1",
                    "actions": "init_parent"
                },
                "on": {
                    "SELF_INTERNAL": {
                        "target": "parent",
                        "reenter": false
                    },
                    "SELF_EXTERNAL": {
                        "target": "parent",
                        "reenter": true
                    }
                },
                "states": {
                    "child1": {
                        "entry": "entry_child1",
                        "exit": "exit_child1",
                        "on": { "GO_2": "child2" }
                    },
                    "child2": {
                        "entry": "entry_child2",
                        "exit": "exit_child2"
                    }
                }
            }
        }
    }"##;

    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut actions: IndexMap<String, ActionFunction> = IndexMap::new();
    for name in [
        "entry_parent", "exit_parent", "init_parent",
        "entry_child1", "exit_child1", "entry_child2", "exit_child2"
    ] {
        actions.insert(name.to_string(), make_action(trace.clone(), name));
    }
    let guards = IndexMap::new();

    let def = from_json_str(json).expect("valid machine");
    let mut machine = RuntimeMachine::new(&def).unwrap();
    machine.start(&guards, &actions);

    // Transition to child2
    machine.transition("GO_2", &Value::Null, &guards, &actions);
    assert!(machine.matches("parent.child2"));

    // Case 1: SELF_INTERNAL (reenter: false)
    // Parent should NOT exit or enter. Only child2 exits, and initial child1 enters!
    trace.borrow_mut().clear();
    machine.transition("SELF_INTERNAL", &Value::Null, &guards, &actions);
    assert!(machine.matches("parent.child1"));
    assert_eq!(*trace.borrow(), vec!["exit_child2", "entry_child1"]);

    // Move back to child2
    machine.transition("GO_2", &Value::Null, &guards, &actions);
    assert!(machine.matches("parent.child2"));

    // Case 2: SELF_EXTERNAL (reenter: true)
    // Parent MUST exit, then parent enters, init_parent runs, child1 enters!
    trace.borrow_mut().clear();
    machine.transition("SELF_EXTERNAL", &Value::Null, &guards, &actions);
    assert!(machine.matches("parent.child1"));
    assert_eq!(
        *trace.borrow(),
        vec!["exit_child2", "exit_parent", "entry_parent", "init_parent", "entry_child1"]
    );
}

#[test]
fn test_direct_transition_bypasses_initial_actions() {
    let json = r##"{
        "initial": "a",
        "states": {
            "a": {
                "on": {
                    "TO_COMPOUND": "b",
                    "TO_LEAF_DIRECT": "b.sub.leaf"
                }
            },
            "b": {
                "entry": "entry_b",
                "initial": {
                    "target": "sub",
                    "actions": "init_b"
                },
                "states": {
                    "sub": {
                        "entry": "entry_sub",
                        "initial": {
                            "target": "leaf",
                            "actions": "init_sub"
                        },
                        "states": {
                            "leaf": {
                                "entry": "entry_leaf"
                            }
                        }
                    }
                }
            }
        }
    }"##;

    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut actions: IndexMap<String, ActionFunction> = IndexMap::new();
    for name in ["entry_b", "init_b", "entry_sub", "init_sub", "entry_leaf"] {
        actions.insert(name.to_string(), make_action(trace.clone(), name));
    }
    let guards = IndexMap::new();
    let def = from_json_str(json).expect("valid machine");

    // Case 1: Transitioning to b (compound) takes initial cascade:
    // entry_b -> init_b -> entry_sub -> init_sub -> entry_leaf
    {
        let mut machine = RuntimeMachine::new(&def).unwrap();
        machine.start(&guards, &actions);
        trace.borrow_mut().clear();
        machine.transition("TO_COMPOUND", &Value::Null, &guards, &actions);
        assert!(machine.matches("b.sub.leaf"));
        assert_eq!(
            *trace.borrow(),
            vec!["entry_b", "init_b", "entry_sub", "init_sub", "entry_leaf"]
        );
    }

    // Case 2: Transitioning directly to b.sub.leaf (leaf) bypasses initial transitions:
    // entry_b -> entry_sub -> entry_leaf (WITHOUT init_b or init_sub!)
    {
        let mut machine = RuntimeMachine::new(&def).unwrap();
        machine.start(&guards, &actions);
        trace.borrow_mut().clear();
        machine.transition("TO_LEAF_DIRECT", &Value::Null, &guards, &actions);
        assert!(machine.matches("b.sub.leaf"));
        assert_eq!(
            *trace.borrow(),
            vec!["entry_b", "entry_sub", "entry_leaf"]
        );
    }
}

#[test]
fn test_targetless_transitions_isolation_and_mixing() {
    let json = r##"{
        "type": "parallel",
        "states": {
            "r1": {
                "initial": "r1_a",
                "states": {
                    "r1_a": {
                        "entry": "entry_r1_a",
                        "exit": "exit_r1_a",
                        "on": {
                            "EVENT_MIXED": {
                                "target": "r1_b",
                                "actions": "action_r1_transition"
                            }
                        }
                    },
                    "r1_b": {
                        "entry": "entry_r1_b",
                        "exit": "exit_r1_b"
                    }
                }
            },
            "r2": {
                "initial": "r2_a",
                "states": {
                    "r2_a": {
                        "entry": "entry_r2_a",
                        "exit": "exit_r2_a",
                        "on": {
                            "EVENT_MIXED": {
                                "actions": "action_r2_targetless"
                            },
                            "EVENT_TARGETLESS_ONLY": {
                                "actions": "action_r2_targetless_only"
                            }
                        }
                    }
                }
            }
        }
    }"##;

    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut actions: IndexMap<String, ActionFunction> = IndexMap::new();
    for name in [
        "entry_r1_a", "exit_r1_a", "entry_r1_b", "exit_r1_b", "action_r1_transition",
        "entry_r2_a", "exit_r2_a", "action_r2_targetless", "action_r2_targetless_only"
    ] {
        actions.insert(name.to_string(), make_action(trace.clone(), name));
    }
    let guards = IndexMap::new();
    let def = from_json_str(json).expect("valid machine");
    let mut machine = RuntimeMachine::new(&def).unwrap();
    machine.start(&guards, &actions);

    assert!(machine.matches("r1.r1_a"));
    assert!(machine.matches("r2.r2_a"));

    // Case 1: Pure targetless event on r2
    // Must execute action_r2_targetless_only WITHOUT exiting or entering r2_a or r1
    trace.borrow_mut().clear();
    machine.transition("EVENT_TARGETLESS_ONLY", &Value::Null, &guards, &actions);
    assert_eq!(*trace.borrow(), vec!["action_r2_targetless_only"]);
    assert!(machine.matches("r1.r1_a"));
    assert!(machine.matches("r2.r2_a"));

    // Case 2: Mixed transition:
    // r1 takes external transition to r1_b (exiting r1_a, executing transition action, entering r1_b)
    // r2 takes targetless transition (executing targetless action, NEVER exiting r2_a)
    trace.borrow_mut().clear();
    machine.transition("EVENT_MIXED", &Value::Null, &guards, &actions);
    assert!(machine.matches("r1.r1_b"));
    assert!(machine.matches("r2.r2_a"));

    // Verify r2_a NEVER exited!
    assert!(!trace.borrow().contains(&"exit_r2_a".to_string()));
    assert!(!trace.borrow().contains(&"entry_r2_a".to_string()));
    // Verify both actions executed
    assert!(trace.borrow().contains(&"action_r1_transition".to_string()));
    assert!(trace.borrow().contains(&"action_r2_targetless".to_string()));
    // Verify exit_r1_a happened before entry_r1_b
    let exit_idx = trace.borrow().iter().position(|x| x == "exit_r1_a").unwrap();
    let entry_idx = trace.borrow().iter().position(|x| x == "entry_r1_b").unwrap();
    assert!(exit_idx < entry_idx);
}

#[test]
fn test_root_reentry_without_explicit_id() {
    let json = r##"{
        "entry": "entry_root",
        "exit": "exit_root",
        "initial": {
            "target": "s1",
            "actions": "init_root"
        },
        "on": {
            "REENTER_ROOT": {
                "target": "#(machine)",
                "reenter": true
            }
        },
        "states": {
            "s1": {
                "entry": "entry_s1",
                "exit": "exit_s1",
                "on": {
                    "NEXT": "s2"
                }
            },
            "s2": {
                "entry": "entry_s2",
                "exit": "exit_s2"
            }
        }
    }"##;

    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut actions: IndexMap<String, ActionFunction> = IndexMap::new();
    for name in [
        "entry_root", "exit_root", "init_root",
        "entry_s1", "exit_s1", "entry_s2", "exit_s2"
    ] {
        actions.insert(name.to_string(), make_action(trace.clone(), name));
    }
    let guards = IndexMap::new();
    let def = from_json_str(json).expect("valid machine");
    let mut machine = RuntimeMachine::new(&def).unwrap();
    machine.start(&guards, &actions);
    assert!(machine.matches("s1"));

    machine.transition("NEXT", &Value::Null, &guards, &actions);
    assert!(machine.matches("s2"));

    trace.borrow_mut().clear();
    machine.transition("REENTER_ROOT", &Value::Null, &guards, &actions);
    println!("State after REENTER_ROOT: {:?}", machine.current_state);
    println!("Trace after REENTER_ROOT: {:?}", *trace.borrow());
    assert!(machine.matches("s1"));

    // Root exit, entry, and init must execute
    assert_eq!(
        *trace.borrow(),
        vec!["exit_s2", "exit_root", "entry_root", "init_root", "entry_s1"]
    );
}

#[test]
fn test_deep_hierarchy_stack_safety_32kb() {
    let builder = std::thread::Builder::new().stack_size(32 * 1024);
    let handler = builder.spawn(|| {
        // Build 20-level deeply nested JSON state machine
        let depth = 20;
        let mut inner_json = String::from(r##"{"on": {"RESET": {"target": "#root", "reenter": true}}}"##);
        for i in (1..depth).rev() {
            inner_json = format!(
                r#"{{"initial": "s{}", "states": {{"s{}": {}}}}}"#,
                i + 1,
                i + 1,
                inner_json
            );
        }
        let full_json = format!(
            r#"{{"id": "root", "initial": "s1", "states": {{"s1": {}}}}}"#,
            inner_json
        );

        println!("Step 1: parsing JSON...");
        let def = from_json_str(&full_json).expect("valid json for deep hierarchy");
        println!("Step 2: creating RuntimeMachine via new(&def)...");
        let mut machine = RuntimeMachine::new(&def).unwrap();
        let guards = IndexMap::new();
        let actions = IndexMap::new();

        println!("Step 3: starting machine...");
        machine.start(&guards, &actions);

        // Verify leaf is active
        let mut expected_path = String::from("s1");
        for i in 2..=depth {
            expected_path.push_str(&format!(".s{}", i));
        }
        println!("Step 4: checking matches...");
        assert!(machine.matches(&expected_path));

        // Perform root re-entry from depth 20 under 32 KB thread stack
        println!("Step 5: transitioning RESET...");
        machine.transition("RESET", &Value::Null, &guards, &actions);
        println!("Step 6: checking matches after reset...");
        assert!(machine.matches(&expected_path));
        println!("Step 7: finished successfully!");
    }).expect("thread spawn succeeded");

    handler.join().expect("thread completed safely within 32 KB stack limit");
}

#[test]
fn test_deep_hierarchy_stack_safety_8kb() {
    // Build 25-level deeply nested JSON state machine
    let depth = 25;
    let mut inner_json = String::from(r##"{"on": {"RESET": {"target": "#root", "reenter": true}}}"##);
    for i in (1..depth).rev() {
        inner_json = format!(
            r#"{{"initial": "s{}", "states": {{"s{}": {}}}}}"#,
            i + 1,
            i + 1,
            inner_json
        );
    }
    let full_json = format!(
        r#"{{"id": "root", "initial": "s1", "states": {{"s1": {}}}}}"#,
        inner_json
    );

    let def = from_json_str(&full_json).expect("valid json for deep hierarchy");

    let stack_sz = if cfg!(debug_assertions) { 64 * 1024 } else { 8 * 1024 };
    let builder = std::thread::Builder::new().stack_size(stack_sz);
    let handler = match builder.spawn(move || {
        let mut machine = RuntimeMachine::new(&def).unwrap();
        println!("16KB: RuntimeMachine::new done!");

        let guards = IndexMap::new();
        let actions = IndexMap::new();

        println!("16KB: starting machine...");
        machine.start(&guards, &actions);
        println!("16KB: starting done!");

        let mut expected_path = String::from("s1");
        for i in 2..=depth {
            expected_path.push_str(&format!(".s{}", i));
        }
        assert!(machine.matches(&expected_path));

        println!("16KB: transitioning RESET...");
        machine.transition("RESET", &Value::Null, &guards, &actions);
        println!("16KB: transitioning done!");

        assert!(machine.matches(&expected_path));
    }) {
        Ok(h) => h,
        Err(e) => {
            // OS may enforce PTHREAD_STACK_MIN higher than 8KB (e.g. on macOS or Linux)
            println!("OS does not permit 8KB stack spawn: {:?}, skipping thread run", e);
            return;
        }
    };

    handler.join().expect("thread completed safely within 8 KB stack limit without stack overflow");
}

#[test]
fn test_parallel_ancestor_targeting_reenter() {
    let json = r##"{
        "initial": "comp",
        "states": {
            "comp": {
                "id": "comp_id",
                "entry": "entry_comp",
                "exit": "exit_comp",
                "initial": "par",
                "states": {
                    "par": {
                        "id": "par_id",
                        "type": "parallel",
                        "entry": "entry_par",
                        "exit": "exit_par",
                        "on": {
                            "RESET_PAR_REENTER": {
                                "target": "#par_id",
                                "reenter": true
                            },
                            "RESET_PAR_NO_REENTER": {
                                "target": "#par_id",
                                "reenter": false
                            }
                        },
                        "states": {
                            "r1": {
                                "initial": "r1_1",
                                "states": {
                                    "r1_1": {
                                        "entry": "entry_r1_1",
                                        "exit": "exit_r1_1",
                                        "on": {
                                            "STEP_R1": "r1_2"
                                        }
                                    },
                                    "r1_2": {
                                        "entry": "entry_r1_2",
                                        "exit": "exit_r1_2"
                                    }
                                }
                            },
                            "r2": {
                                "initial": "r2_1",
                                "states": {
                                    "r2_1": {
                                        "entry": "entry_r2_1",
                                        "exit": "exit_r2_1",
                                        "on": { "STEP_R2": "r2_2" }
                                    },
                                    "r2_2": {
                                        "entry": "entry_r2_2",
                                        "exit": "exit_r2_2"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }"##;

    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut actions: IndexMap<String, ActionFunction> = IndexMap::new();
    for name in [
        "entry_comp", "exit_comp",
        "entry_par", "exit_par",
        "entry_r1_1", "exit_r1_1", "entry_r1_2", "exit_r1_2",
        "entry_r2_1", "exit_r2_1", "entry_r2_2", "exit_r2_2"
    ] {
        actions.insert(name.to_string(), make_action(trace.clone(), name));
    }
    let guards = IndexMap::new();
    let def = from_json_str(json).expect("valid machine");

    // Case 1: RESET_PAR_REENTER (reenter: true) from r1_1
    // Step both regions forward first
    {
        let mut machine = RuntimeMachine::new(&def).unwrap();
        machine.start(&guards, &actions);
        machine.transition("STEP_R1", &Value::Null, &guards, &actions);
        machine.transition("STEP_R2", &Value::Null, &guards, &actions);
        assert!(machine.matches("comp.par.r1.r1_2"));
        assert!(machine.matches("comp.par.r2.r2_2"));

        trace.borrow_mut().clear();
        machine.transition("RESET_PAR_REENTER", &Value::Null, &guards, &actions);
        println!("State after RESET_PAR_REENTER: {:?}", machine.current_state);
        println!("Trace after RESET_PAR_REENTER: {:?}", *trace.borrow());
        assert!(machine.matches("comp.par.r1.r1_1"));
        assert!(machine.matches("comp.par.r2.r2_1"));

        // Both r1 and r2 states must exit, par must exit and reenter, comp must NOT exit!
        assert!(!trace.borrow().contains(&"exit_comp".to_string()));
        assert!(!trace.borrow().contains(&"entry_comp".to_string()));
        assert!(trace.borrow().contains(&"exit_par".to_string()));
        assert!(trace.borrow().contains(&"entry_par".to_string()));
        assert!(trace.borrow().contains(&"exit_r1_2".to_string()));
        assert!(trace.borrow().contains(&"exit_r2_2".to_string()));
        assert!(trace.borrow().contains(&"entry_r1_1".to_string()));
        assert!(trace.borrow().contains(&"entry_r2_1".to_string()));
    }

    // Case 2: RESET_PAR_NO_REENTER (reenter: false)
    {
        let mut machine = RuntimeMachine::new(&def).unwrap();
        machine.start(&guards, &actions);
        machine.transition("STEP_R1", &Value::Null, &guards, &actions);
        machine.transition("STEP_R2", &Value::Null, &guards, &actions);
        assert!(machine.matches("comp.par.r1.r1_2"));
        assert!(machine.matches("comp.par.r2.r2_2"));

        trace.borrow_mut().clear();
        machine.transition("RESET_PAR_NO_REENTER", &Value::Null, &guards, &actions);
        assert!(machine.matches("comp.par.r1.r1_1"));
        assert!(machine.matches("comp.par.r2.r2_1"));

        // When targeting parallel ancestor, SCXML treats targeting ancestor as exiting that ancestor
        // since r1_1 is a descendant of par.
        assert!(!trace.borrow().contains(&"exit_comp".to_string()));
        assert!(!trace.borrow().contains(&"entry_comp".to_string()));
    }
}

#[test]
fn test_preemption_child_targetless_vs_parent_external() {
    let json = r##"{
        "initial": "parent",
        "states": {
            "parent": {
                "on": {
                    "CONFLICT_EVENT": {
                        "target": "other",
                        "actions": "parent_action"
                    }
                },
                "initial": "child",
                "states": {
                    "child": {
                        "on": {
                            "CONFLICT_EVENT": {
                                "actions": "child_action"
                            }
                        }
                    }
                }
            },
            "other": {}
        }
    }"##;

    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut actions: IndexMap<String, ActionFunction> = IndexMap::new();
    actions.insert("parent_action".to_string(), make_action(trace.clone(), "parent_action"));
    actions.insert("child_action".to_string(), make_action(trace.clone(), "child_action"));
    let guards = IndexMap::new();

    let def = from_json_str(json).expect("valid machine");
    let mut machine = RuntimeMachine::new(&def).unwrap();
    machine.start(&guards, &actions);
    assert!(machine.matches("parent.child"));

    trace.borrow_mut().clear();
    machine.transition("CONFLICT_EVENT", &Value::Null, &guards, &actions);

    // Child transition must preempt parent transition
    // State remains in parent.child, only child_action runs!
    assert!(machine.matches("parent.child"));
    assert_eq!(*trace.borrow(), vec!["child_action"]);
}

#[test]
fn test_timer_lifecycle_on_ancestor_transitions() {
    let json = r##"{
        "initial": "parent",
        "states": {
            "parent": {
                "after": {
                    "1000": "timeout_parent"
                },
                "initial": "child",
                "states": {
                    "child": {
                        "after": {
                            "500": "timeout_child"
                        },
                        "on": {
                            "REENTER_PARENT": {
                                "target": "#parent_id",
                                "reenter": true
                            }
                        }
                    }
                },
                "id": "parent_id"
            },
            "timeout_parent": {},
            "timeout_child": {}
        }
    }"##;

    let def = from_json_str(json).expect("valid machine");
    let mut machine = RuntimeMachine::new(&def).unwrap();
    let clock = lavi::SimulatedClock::new();
    machine.register_clock(clock.clone());

    let guards = IndexMap::new();
    let actions = IndexMap::new();
    machine.start(&guards, &actions);

    assert!(machine.matches("parent.child"));
    // Timers scheduled: parent timer at 1000, child timer at 500
    assert_eq!(machine.active_timers.len(), 2);

    // Advance clock to 200ms
    clock.set(200, &mut machine, &guards, &actions);

    // Reenter parent
    machine.transition("REENTER_PARENT", &Value::Null, &guards, &actions);
    assert!(machine.matches("parent.child"));

    // Both timers should be rescheduled with new base time = 200
    // Child: 200 + 500 = 700. Parent: 200 + 1000 = 1200.
    assert_eq!(machine.active_timers.len(), 2);
    let child_timer = machine.active_timers.iter().find(|(p, _, _)| p == "parent.child").unwrap();
    assert_eq!(child_timer.1, 700);
    let parent_timer = machine.active_timers.iter().find(|(p, _, _)| p == "parent").unwrap();
    assert_eq!(parent_timer.1, 1200);
}

#[test]
fn test_literal_hash_target_root_reentry_matrix() {
    // 1. Compound machine with NO explicit ID
    let json_no_id = r##"{
        "entry": "entry_root",
        "exit": "exit_root",
        "initial": {
            "target": "step1",
            "actions": "init_root"
        },
        "states": {
            "step1": {
                "entry": "entry_step1",
                "exit": "exit_step1",
                "on": { "GO": "step2" }
            },
            "step2": {
                "entry": "entry_step2",
                "exit": "exit_step2",
                "on": {
                    "HASH_REENTER": {
                        "target": "#",
                        "reenter": true
                    },
                    "HASH_NO_REENTER": {
                        "target": "#",
                        "reenter": false
                    },
                    "EMPTY_REENTER": {
                        "target": "",
                        "reenter": true
                    }
                }
            }
        }
    }"##;

    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut actions: IndexMap<String, ActionFunction> = IndexMap::new();
    for name in ["entry_root", "exit_root", "init_root", "entry_step1", "exit_step1", "entry_step2", "exit_step2"] {
        actions.insert(name.to_string(), make_action(trace.clone(), name));
    }
    let guards = IndexMap::new();

    // Subcase 1A: target: "#", reenter: true
    {
        let def = from_json_str(json_no_id).expect("valid machine");
        let mut machine = RuntimeMachine::new(&def).unwrap();
        machine.start(&guards, &actions);
        machine.transition("GO", &Value::Null, &guards, &actions);
        assert!(machine.matches("step2"));

        trace.borrow_mut().clear();
        machine.transition("HASH_REENTER", &Value::Null, &guards, &actions);

        // Must match initial state step1, NOT corrupted Atomic("#")!
        assert!(machine.matches("step1"));
        assert!(!format!("{:?}", machine.current_state).contains("#"));
        assert_eq!(
            *trace.borrow(),
            vec!["exit_step2", "exit_root", "entry_root", "init_root", "entry_step1"]
        );
    }

    // Subcase 1B: target: "#", reenter: false
    {
        let def = from_json_str(json_no_id).expect("valid machine");
        let mut machine = RuntimeMachine::new(&def).unwrap();
        machine.start(&guards, &actions);
        machine.transition("GO", &Value::Null, &guards, &actions);
        assert!(machine.matches("step2"));

        trace.borrow_mut().clear();
        machine.transition("HASH_NO_REENTER", &Value::Null, &guards, &actions);

        // Root must NOT exit or enter, but step2 exits and step1 enters!
        assert!(machine.matches("step1"));
        assert!(!format!("{:?}", machine.current_state).contains("#"));
        assert_eq!(*trace.borrow(), vec!["exit_step2", "entry_step1"]);
    }

    // Subcase 1C: target: "", reenter: true
    {
        let def = from_json_str(json_no_id).expect("valid machine");
        let mut machine = RuntimeMachine::new(&def).unwrap();
        machine.start(&guards, &actions);
        machine.transition("GO", &Value::Null, &guards, &actions);
        assert!(machine.matches("step2"));

        trace.borrow_mut().clear();
        machine.transition("EMPTY_REENTER", &Value::Null, &guards, &actions);

        assert!(machine.matches("step1"));
        assert_eq!(
            *trace.borrow(),
            vec!["exit_step2", "exit_root", "entry_root", "init_root", "entry_step1"]
        );
    }

    // 2. Machine WITH explicit ID
    let json_with_id = r##"{
        "id": "my_workflow",
        "entry": "entry_root",
        "exit": "exit_root",
        "initial": {
            "target": "active",
            "actions": "init_root"
        },
        "states": {
            "active": {
                "entry": "entry_active",
                "exit": "exit_active",
                "on": {
                    "RESET_HASH": {
                        "target": "#",
                        "reenter": true
                    },
                    "RESET_EXPLICIT_ID": {
                        "target": "#my_workflow",
                        "reenter": true
                    }
                }
            }
        }
    }"##;

    let mut actions2: IndexMap<String, ActionFunction> = IndexMap::new();
    for name in ["entry_root", "exit_root", "init_root", "entry_active", "exit_active"] {
        actions2.insert(name.to_string(), make_action(trace.clone(), name));
    }

    // Subcase 2A: target: "#" with explicit ID
    {
        let def = from_json_str(json_with_id).expect("valid machine");
        let mut machine = RuntimeMachine::new(&def).unwrap();
        machine.start(&guards, &actions2);
        assert!(machine.matches("active"));

        trace.borrow_mut().clear();
        machine.transition("RESET_HASH", &Value::Null, &guards, &actions2);

        assert!(machine.matches("active"));
        assert!(!format!("{:?}", machine.current_state).contains("#"));
        assert_eq!(
            *trace.borrow(),
            vec!["exit_active", "exit_root", "entry_root", "init_root", "entry_active"]
        );
    }

    // Subcase 2B: target: "#my_workflow"
    {
        let def = from_json_str(json_with_id).expect("valid machine");
        let mut machine = RuntimeMachine::new(&def).unwrap();
        machine.start(&guards, &actions2);
        assert!(machine.matches("active"));

        trace.borrow_mut().clear();
        machine.transition("RESET_EXPLICIT_ID", &Value::Null, &guards, &actions2);

        assert!(machine.matches("active"));
        assert_eq!(
            *trace.borrow(),
            vec!["exit_active", "exit_root", "entry_root", "init_root", "entry_active"]
        );
    }

    // 3. Parallel machine targeting root "#"
    let json_parallel = r##"{
        "type": "parallel",
        "entry": "entry_root",
        "exit": "exit_root",
        "states": {
            "reg1": {
                "initial": "r1_a",
                "states": {
                    "r1_a": {
                        "entry": "entry_r1_a",
                        "exit": "exit_r1_a",
                        "on": { "GO_R1": "r1_b" }
                    },
                    "r1_b": {
                        "entry": "entry_r1_b",
                        "exit": "exit_r1_b",
                        "on": {
                            "RESET_ROOT_FROM_R1": {
                                "target": "#",
                                "reenter": true
                            }
                        }
                    }
                }
            },
            "reg2": {
                "initial": "r2_a",
                "states": {
                    "r2_a": {
                        "entry": "entry_r2_a",
                        "exit": "exit_r2_a",
                        "on": { "GO_R2": "r2_b" }
                    },
                    "r2_b": {
                        "entry": "entry_r2_b",
                        "exit": "exit_r2_b"
                    }
                }
            }
        }
    }"##;

    let mut actions3: IndexMap<String, ActionFunction> = IndexMap::new();
    for name in [
        "entry_root", "exit_root",
        "entry_r1_a", "exit_r1_a", "entry_r1_b", "exit_r1_b",
        "entry_r2_a", "exit_r2_a", "entry_r2_b", "exit_r2_b",
    ] {
        actions3.insert(name.to_string(), make_action(trace.clone(), name));
    }

    {
        let def = from_json_str(json_parallel).expect("valid machine");
        let mut machine = RuntimeMachine::new(&def).unwrap();
        machine.start(&guards, &actions3);
        machine.transition("GO_R1", &Value::Null, &guards, &actions3);
        machine.transition("GO_R2", &Value::Null, &guards, &actions3);
        assert!(machine.matches("reg1.r1_b"));
        assert!(machine.matches("reg2.r2_b"));

        trace.borrow_mut().clear();
        machine.transition("RESET_ROOT_FROM_R1", &Value::Null, &guards, &actions3);

        // Both regions must reset to initial states
        assert!(machine.matches("reg1.r1_a"));
        assert!(machine.matches("reg2.r2_a"));
        assert!(!format!("{:?}", machine.current_state).contains("#"));

        // Both active regions must exit, root exit must run, root entry runs, both regions enter
        assert!(trace.borrow().contains(&"exit_r1_b".to_string()));
        assert!(trace.borrow().contains(&"exit_r2_b".to_string()));
        assert!(trace.borrow().contains(&"exit_root".to_string()));
        assert!(trace.borrow().contains(&"entry_root".to_string()));
        assert!(trace.borrow().contains(&"entry_r1_a".to_string()));
        assert!(trace.borrow().contains(&"entry_r2_a".to_string()));
    }
}


