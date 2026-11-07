extern crate alloc;
extern crate std;

use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use lavi::{IndexMap, IndexMapExt};
use lavi::{MachineStatus, RuntimeMachine, StateDefinition, StateType, Value};

/// 1. 6-level deep cascade with ancestor timer preservation and root exit action isolation
#[test]
fn test_6level_deep_cascade_with_ancestor_timer_preservation() {
    let trace: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let t_clone = trace.clone();

    // L1 (ancestor with timer) -> L2 -> L3 -> L4 -> L5 -> L6
    let json = r#"{
        "id": "root6",
        "initial": "l1",
        "exit": "root_exit",
        "states": {
            "l1": {
                "initial": "l2",
                "after": {
                    "30": { "target": "timed_out", "actions": "ancestor_timeout" }
                },
                "states": {
                    "l2": {
                        "initial": "l3",
                        "states": {
                            "l3": {
                                "initial": "l4",
                                "states": {
                                    "l4": {
                                        "initial": "l5",
                                        "states": {
                                            "l5": {
                                                "initial": "l6",
                                                "states": {
                                                    "l6": {
                                                        "initial": "step_a",
                                                        "states": {
                                                            "step_a": {
                                                                "on": { "FINISH_L6": "l6_final" }
                                                            },
                                                            "l6_final": { "type": "final" }
                                                        },
                                                        "onDone": "l5_wait"
                                                    }
                                                }
                                            },
                                            "l5_wait": {
                                                "on": { "FINISH_L5": "l5_final" }
                                            },
                                            "l5_final": { "type": "final" }
                                        },
                                        "onDone": "l4_final"
                                    },
                                    "l4_final": { "type": "final" }
                                },
                                "onDone": "l3_final"
                            },
                            "l3_final": { "type": "final" }
                        },
                        "onDone": "l2_wait"
                    },
                    "l2_wait": {
                        "on": { "FINISH_L2": "l2_final" }
                    },
                    "l2_final": { "type": "final" }
                },
                "onDone": "all_done"
            },
            "timed_out": {
                "type": "final"
            },
            "all_done": {
                "type": "final"
            }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");

    let mut actions = IndexMap::new();
    let t1 = t_clone.clone();
    actions.insert(
        "root_exit".to_string(),
        Rc::new(move |_c: &mut Value, _e: &Value, _p: &Option<Value>| {
            t1.borrow_mut().push("root_exit".to_string());
        }) as lavi::ActionFunction,
    );
    let t2 = t_clone.clone();
    actions.insert(
        "ancestor_timeout".to_string(),
        Rc::new(move |_c: &mut Value, _e: &Value, _p: &Option<Value>| {
            t2.borrow_mut().push("ancestor_timeout".to_string());
        }) as lavi::ActionFunction,
    );

    machine.start(&IndexMap::new(), &actions);
    assert_eq!(machine.status(), MachineStatus::Active);
    assert!(!machine.is_done());
    assert!(machine.current_state.matches("l1.l2.l3.l4.l5.l6.step_a"));

    // Step 1: Finish L6. This triggers onDone on L5 to l5_wait (an active intermediate state).
    machine.transition("FINISH_L6", &Value::Null, &IndexMap::new(), &actions);

    std::println!("Current active paths: {:?}", machine.current_state.get_active_paths());
    assert!(machine.current_state.matches("l1.l2.l3.l4.l5_wait"));
    assert_eq!(machine.status(), MachineStatus::Active);
    assert!(!machine.is_done());
    // Root exit must NOT have fired!
    assert!(
        !trace.borrow().contains(&"root_exit".to_string()),
        "Root exit must NOT fire when inner substate completes to intermediate wait state!"
    );

    // Step 2: Verify ancestor timer is STILL preserved in machine.active_timers
    assert!(
        !machine.active_timers.is_empty(),
        "Active timers on ancestor state l1 must NOT be cleared by inner substate completion!"
    );

    // Step 3: Finish L5. Cascades L5_final -> L4 onDone -> L4_final -> L3 onDone -> L3_final -> L2 onDone -> l2_wait
    machine.transition("FINISH_L5", &Value::Null, &IndexMap::new(), &actions);

    std::println!("Current active paths after FINISH_L5: {:?}", machine.current_state.get_active_paths());
    assert!(machine.current_state.matches("l1.l2_wait"));
    assert_eq!(machine.status(), MachineStatus::Active);
    assert!(!machine.is_done());
    assert!(
        !trace.borrow().contains(&"root_exit".to_string()),
        "Root exit must NOT fire when cascading onDone reaches l2_wait!"
    );

    // Step 4: Finish L2. Cascades L2_final -> L1 onDone -> all_done (root final state!)
    machine.transition("FINISH_L2", &Value::Null, &IndexMap::new(), &actions);

    assert!(machine.current_state.matches("all_done"));
    assert_eq!(machine.status(), MachineStatus::Done);
    assert!(machine.is_done());
    // Root exit MUST fire now!
    assert!(
        trace.borrow().contains(&"root_exit".to_string()),
        "Root exit must fire when root reaches final state all_done!"
    );
}

/// 2. Root Parallel state with 3 regions (nested parallel + compound) completing independently
#[test]
fn test_root_parallel_with_3_regions_and_sub_parallel_and_compound() {
    let trace: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let t_clone = trace.clone();

    let json = r#"{
        "id": "root_par3",
        "type": "parallel",
        "exit": "root_exit",
        "states": {
            "r1": {
                "initial": "comp",
                "states": {
                    "comp": {
                        "initial": "working",
                        "states": {
                            "working": { "on": { "R1_FINISH_INNER": "inner_done" } },
                            "inner_done": { "type": "final" }
                        },
                        "onDone": "comp_done"
                    },
                    "comp_done": {
                        "type": "final"
                    }
                }
            },
            "r2": {
                "initial": "sub_par",
                "states": {
                    "sub_par": {
                        "type": "parallel",
                        "states": {
                            "pA": {
                                "initial": "s",
                                "states": {
                                    "s": { "on": { "R2_A_DONE": "d" } },
                                    "d": { "type": "final" }
                                }
                            },
                            "pB": {
                                "initial": "s",
                                "states": {
                                    "s": { "on": { "R2_B_DONE": "d" } },
                                    "d": { "type": "final" }
                                }
                            }
                        },
                        "onDone": "r2_done"
                    },
                    "r2_done": {
                        "type": "final"
                    }
                }
            },
            "r3": {
                "initial": "idle",
                "states": {
                    "idle": {
                        "on": { "R3_DONE": "finished" }
                    },
                    "finished": {
                        "type": "final"
                    }
                }
            }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    let mut actions = IndexMap::new();
    actions.insert(
        "root_exit".to_string(),
        Rc::new(move |_c: &mut Value, _e: &Value, _p: &Option<Value>| {
            t_clone.borrow_mut().push("root_exit".to_string());
        }) as lavi::ActionFunction,
    );

    machine.start(&IndexMap::new(), &actions);
    assert_eq!(machine.status(), MachineStatus::Active);
    assert!(!machine.is_done());

    // 1. In R1, complete inner substate working -> inner_done. Cascades to comp_done (R1 final).
    machine.transition("R1_FINISH_INNER", &Value::Null, &IndexMap::new(), &actions);
    assert!(machine.current_state.matches("r1.comp_done"));
    assert_eq!(machine.status(), MachineStatus::Active, "Machine must be Active because R2 and R3 are not done!");
    assert!(!machine.is_done());
    assert!(!trace.borrow().contains(&"root_exit".to_string()));

    // 2. In R2, complete sub-region pA only.
    machine.transition("R2_A_DONE", &Value::Null, &IndexMap::new(), &actions);
    assert_eq!(machine.status(), MachineStatus::Active, "Machine must be Active because R2.pB and R3 are not done!");
    assert!(!machine.is_done());
    assert!(!trace.borrow().contains(&"root_exit".to_string()));

    // 3. In R2, complete sub-region pB. sub_par is now done -> cascades onDone to r2_done (R2 final).
    machine.transition("R2_B_DONE", &Value::Null, &IndexMap::new(), &actions);
    assert!(machine.current_state.matches("r2.r2_done"));
    assert_eq!(machine.status(), MachineStatus::Active, "Machine must be Active because R3 is still idle!");
    assert!(!machine.is_done());
    assert!(!trace.borrow().contains(&"root_exit".to_string()));

    // 4. Finally, complete R3 idle -> finished (R3 final).
    machine.transition("R3_DONE", &Value::Null, &IndexMap::new(), &actions);
    assert!(machine.current_state.matches("r3.finished"));

    // Now ALL 3 orthogonal regions are done! Root parallel machine must be Done!
    assert_eq!(machine.status(), MachineStatus::Done, "Root parallel machine must be Done when all regions are final!");
    assert!(machine.is_done());
    assert_eq!(trace.borrow().len(), 1, "Root exit must have been called exactly once!");
    assert_eq!(trace.borrow()[0], "root_exit");
}

/// 3. Stack-constrained thread (16 KB) verifying bounded CPU call-stack of StateDefinition::clone()
#[test]
fn test_state_definition_clone_deep_stack_constrained_16kb() {
    let builder = std::thread::Builder::new()
        .name("stack_constrained_test".to_string())
        .stack_size(16 * 1024); // 16 KB stack!

    let handler = builder.spawn(|| {
        // Build a 100-level deeply nested StateDefinition
        let mut leaf = StateDefinition::default_atomic();
        leaf.tags.push("leaf_tag".to_string());

        let mut current = leaf;
        for i in (1..100).rev() {
            let mut parent = StateDefinition::default_atomic();
            parent.state_type = StateType::Compound;
            parent.id = Some(format!("s_{}", i));
            parent.initial = Some(format!("s_{}", i + 1));
            parent.states.insert(format!("s_{}", i + 1), current);
            current = parent;
        }

        let mut root = StateDefinition::default_atomic();
        root.state_type = StateType::Compound;
        root.id = Some("root".to_string());
        root.initial = Some("s_1".to_string());
        root.states.insert("s_1".to_string(), current);

        // CLONE MUST NOT OVERFLOW THE 16 KB CPU CALL STACK!
        let cloned = root.clone();

        // Verify structure integrity
        assert_eq!(cloned.id.as_deref(), Some("root"));
        assert_eq!(cloned.states.len(), 1);

        // Verify deep leaf is reachable and intact
        let mut curr_ptr = &cloned;
        for i in 1..100 {
            let next_key = format!("s_{}", i);
            curr_ptr = curr_ptr.states.get(&next_key).expect("nested state must exist");
        }
        assert!(curr_ptr.states.contains_key("s_100"));
        let leaf_state = curr_ptr.states.get("s_100").unwrap();
        assert_eq!(leaf_state.tags, vec!["leaf_tag".to_string()]);
    }).expect("thread spawn");

    handler.join().expect("thread completed without stack overflow");
}

/// 4. Root re-entry targeting "#" with empty root ID
#[test]
fn test_root_reentry_target_hash_with_empty_root_id() {
    let trace: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let t_clone = trace.clone();

    // No "id" on root
    let json = r##"{
        "initial": "step1",
        "entry": "root_entry",
        "exit": "root_exit",
        "states": {
            "step1": {
                "entry": "step1_entry",
                "exit": "step1_exit",
                "on": {
                    "NEXT": "step2"
                }
            },
            "step2": {
                "entry": "step2_entry",
                "exit": "step2_exit",
                "on": {
                    "RESET_TO_ROOT": "#",
                    "RESET_TO_ROOT_REENTER": {
                        "target": "#",
                        "reenter": true
                    }
                }
            }
        }
    }"##;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    let mut actions = IndexMap::new();

    let names = ["root_entry", "root_exit", "step1_entry", "step1_exit", "step2_entry", "step2_exit"];
    for name in names {
        let t = t_clone.clone();
        let n = name.to_string();
        actions.insert(
            n.clone(),
            Rc::new(move |_c: &mut Value, _e: &Value, _p: &Option<Value>| {
                t.borrow_mut().push(n.clone());
            }) as lavi::ActionFunction,
        );
    }

    machine.start(&IndexMap::new(), &actions);
    assert!(machine.current_state.matches("step1"));
    assert_eq!(*trace.borrow(), vec!["root_entry", "step1_entry"]);
    trace.borrow_mut().clear();

    // Transition to step2
    machine.transition("NEXT", &Value::Null, &IndexMap::new(), &actions);
    assert!(machine.current_state.matches("step2"));
    assert_eq!(*trace.borrow(), vec!["step1_exit", "step2_entry"]);
    trace.borrow_mut().clear();

    // 1. Reset to root without reenter (reenter: false default)
    // Root is LCCA, so root does not exit/reenter!
    machine.transition("RESET_TO_ROOT", &Value::Null, &IndexMap::new(), &actions);
    assert!(machine.current_state.matches("step1"), "Target '#' must reset machine to initial state step1!");
    assert_eq!(machine.status(), MachineStatus::Active);
    assert_eq!(*trace.borrow(), vec!["step2_exit", "step1_entry"]);
    trace.borrow_mut().clear();

    // Transition to step2 again
    machine.transition("NEXT", &Value::Null, &IndexMap::new(), &actions);
    assert!(machine.current_state.matches("step2"));
    trace.borrow_mut().clear();

    // 2. Reset to root with reenter: true
    // Root MUST exit and reenter!
    machine.transition("RESET_TO_ROOT_REENTER", &Value::Null, &IndexMap::new(), &actions);
    assert!(machine.current_state.matches("step1"));
    assert_eq!(machine.status(), MachineStatus::Active);
    assert_eq!(
        *trace.borrow(),
        vec!["step2_exit", "root_exit", "root_entry", "step1_entry"]
    );
}

/// 5. Cascading onDone with conditional guards selecting proper branch
#[test]
fn test_cascading_on_done_with_guards() {
    let json = r##"{
        "id": "guarded_cascade",
        "initial": "processing",
        "context": { "score": 0 },
        "states": {
            "processing": {
                "initial": "sub",
                "states": {
                    "sub": {
                        "initial": "eval",
                        "states": {
                            "eval": {
                                "on": {
                                    "SET_HIGH": { "target": "eval_done", "actions": "set_score_high" },
                                    "SET_LOW": { "target": "eval_done", "actions": "set_score_low" }
                                }
                            },
                            "eval_done": {
                                "type": "final"
                            }
                        },
                        "onDone": [
                            { "guard": "is_high_score", "target": "#guarded_cascade.passed" },
                            { "target": "#guarded_cascade.failed" }
                        ]
                    }
                }
            },
            "passed": { "type": "final" },
            "failed": { "type": "final" }
        }
    }"##;

    // Test run 1: High score -> passed
    {
        let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
        let mut actions = IndexMap::new();
        actions.insert(
            "set_score_high".to_string(),
            Rc::new(|c: &mut Value, _e: &Value, _p: &Option<Value>| {
                if let Value::Object(map) = c {
                    map.insert("score".to_string(), Value::Number(95.0));
                }
            }) as lavi::ActionFunction,
        );
        actions.insert(
            "set_score_low".to_string(),
            Rc::new(|c: &mut Value, _e: &Value, _p: &Option<Value>| {
                if let Value::Object(map) = c {
                    map.insert("score".to_string(), Value::Number(40.0));
                }
            }) as lavi::ActionFunction,
        );

        let mut guards = IndexMap::new();
        guards.insert(
            "is_high_score".to_string(),
            Rc::new(|c: &Value, _e: &Value, _p: &Option<Value>| {
                if let Value::Object(map) = c {
                    if let Some(Value::Number(s)) = map.get("score") {
                        return *s >= 80.0;
                    }
                }
                false
            }) as lavi::GuardFunction,
        );

        machine.start(&guards, &actions);
        machine.transition("SET_HIGH", &Value::Null, &guards, &actions);

        assert!(machine.current_state.matches("passed"));
        assert_eq!(machine.status(), MachineStatus::Done);
        assert!(machine.is_done());
    }

    // Test run 2: Low score -> failed
    {
        let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
        let mut actions = IndexMap::new();
        actions.insert(
            "set_score_high".to_string(),
            Rc::new(|c: &mut Value, _e: &Value, _p: &Option<Value>| {
                if let Value::Object(map) = c {
                    map.insert("score".to_string(), Value::Number(95.0));
                }
            }) as lavi::ActionFunction,
        );
        actions.insert(
            "set_score_low".to_string(),
            Rc::new(|c: &mut Value, _e: &Value, _p: &Option<Value>| {
                if let Value::Object(map) = c {
                    map.insert("score".to_string(), Value::Number(40.0));
                }
            }) as lavi::ActionFunction,
        );

        let mut guards = IndexMap::new();
        guards.insert(
            "is_high_score".to_string(),
            Rc::new(|c: &Value, _e: &Value, _p: &Option<Value>| {
                if let Value::Object(map) = c {
                    if let Some(Value::Number(s)) = map.get("score") {
                        return *s >= 80.0;
                    }
                }
                false
            }) as lavi::GuardFunction,
        );

        machine.start(&guards, &actions);
        machine.transition("SET_LOW", &Value::Null, &guards, &actions);

        assert!(machine.current_state.matches("failed"));
        assert_eq!(machine.status(), MachineStatus::Done);
        assert!(machine.is_done());
    }
}
