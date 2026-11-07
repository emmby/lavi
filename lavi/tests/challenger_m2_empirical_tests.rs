extern crate alloc;
extern crate std;

use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use lavi::{IndexMap, IndexMapExt};
use lavi::{MachineStatus, RuntimeMachine, Value};

#[test]
fn test_multi_level_cascading_on_done_3_levels() {
    // Level 3 (grandchild) -> Level 2 (child) -> Level 1 (root)
    let json = r#"{
        "id": "root",
        "initial": "parent",
        "states": {
            "parent": {
                "initial": "child",
                "states": {
                    "child": {
                        "initial": "grandchild_step",
                        "states": {
                            "grandchild_step": {
                                "on": { "FINISH": "grandchild_final" }
                            },
                            "grandchild_final": {
                                "type": "final"
                            }
                        },
                        "onDone": "child_done"
                    },
                    "child_done": {
                        "type": "final"
                    }
                },
                "onDone": "parent_done"
            },
            "parent_done": {
                "type": "final"
            }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    machine.start(&IndexMap::new(), &IndexMap::new());

    assert!(machine.current_state.matches("parent.child.grandchild_step"));
    assert_eq!(machine.status(), MachineStatus::Active);

    // Trigger FINISH in grandchild_step
    // Should cascade: grandchild_final -> child onDone -> child_done -> parent onDone -> parent_done
    machine.transition("FINISH", &Value::Null, &IndexMap::new(), &IndexMap::new());

    std::println!("After FINISH, state: {:?}", machine.current_state.get_active_paths());
    assert!(machine.current_state.matches("parent_done"));
    assert_eq!(machine.status(), MachineStatus::Done);
    assert!(machine.is_done());
}

#[test]
fn test_multi_level_cascading_on_done_4_levels_with_actions() {
    let trace: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let t_clone = trace.clone();

    let json = r#"{
        "id": "cascade4",
        "initial": "l1",
        "states": {
            "l1": {
                "initial": "l2",
                "states": {
                    "l2": {
                        "initial": "l3",
                        "states": {
                            "l3": {
                                "initial": "l4",
                                "states": {
                                    "l4": {
                                        "on": { "TRIGGER": "l4_done" }
                                    },
                                    "l4_done": { "type": "final" }
                                },
                                "onDone": {
                                    "target": "l3_done",
                                    "actions": "action_l3_done"
                                }
                            },
                            "l3_done": { "type": "final" }
                        },
                        "onDone": {
                            "target": "l2_done",
                            "actions": "action_l2_done"
                        }
                    },
                    "l2_done": { "type": "final" }
                },
                "onDone": {
                    "target": "l1_done",
                    "actions": "action_l1_done"
                }
            },
            "l1_done": { "type": "final" }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    let mut actions = IndexMap::new();

    let t1 = trace.clone();
    actions.insert("action_l3_done".to_string(), Rc::new(move |_ctx: &mut Value, _evt: &Value, _p: &Option<Value>| {
        t1.borrow_mut().push("l3_done_action".to_string());
    }) as lavi::ActionFunction);

    let t2 = trace.clone();
    actions.insert("action_l2_done".to_string(), Rc::new(move |_ctx: &mut Value, _evt: &Value, _p: &Option<Value>| {
        t2.borrow_mut().push("l2_done_action".to_string());
    }) as lavi::ActionFunction);

    let t3 = trace.clone();
    actions.insert("action_l1_done".to_string(), Rc::new(move |_ctx: &mut Value, _evt: &Value, _p: &Option<Value>| {
        t3.borrow_mut().push("l1_done_action".to_string());
    }) as lavi::ActionFunction);

    machine.start(&IndexMap::new(), &actions);
    assert!(machine.current_state.matches("l1.l2.l3.l4"));

    machine.transition("TRIGGER", &Value::Null, &IndexMap::new(), &actions);

    assert!(machine.current_state.matches("l1_done"));
    assert_eq!(machine.status(), MachineStatus::Done);

    let expected = vec![
        "l3_done_action".to_string(),
        "l2_done_action".to_string(),
        "l1_done_action".to_string(),
    ];
    assert_eq!(*t_clone.borrow(), expected);
}

#[test]
fn test_parallel_completion_4_regions_sequential() {
    let json = r#"{
        "id": "par4",
        "initial": "active",
        "states": {
            "active": {
                "type": "parallel",
                "states": {
                    "r1": {
                        "initial": "start",
                        "states": {
                            "start": { "on": { "FINISH_1": "done" } },
                            "done": { "type": "final" }
                        }
                    },
                    "r2": {
                        "initial": "start",
                        "states": {
                            "start": { "on": { "FINISH_2": "done" } },
                            "done": { "type": "final" }
                        }
                    },
                    "r3": {
                        "initial": "start",
                        "states": {
                            "start": { "on": { "FINISH_3": "done" } },
                            "done": { "type": "final" }
                        }
                    },
                    "r4": {
                        "initial": "start",
                        "states": {
                            "start": { "on": { "FINISH_4": "done" } },
                            "done": { "type": "final" }
                        }
                    }
                },
                "onDone": "finished"
            },
            "finished": { "type": "final" }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    machine.start(&IndexMap::new(), &IndexMap::new());

    assert_eq!(machine.status(), MachineStatus::Active);

    // Complete R1
    machine.transition("FINISH_1", &Value::Null, &IndexMap::new(), &IndexMap::new());
    assert!(machine.current_state.matches("active.r1.done"));
    assert!(machine.current_state.matches("active.r2.start"));
    assert!(!machine.current_state.matches("finished"));
    assert_eq!(machine.status(), MachineStatus::Active);

    // Complete R2
    machine.transition("FINISH_2", &Value::Null, &IndexMap::new(), &IndexMap::new());
    assert!(machine.current_state.matches("active.r2.done"));
    assert!(!machine.current_state.matches("finished"));
    assert_eq!(machine.status(), MachineStatus::Active);

    // Complete R3
    machine.transition("FINISH_3", &Value::Null, &IndexMap::new(), &IndexMap::new());
    assert!(machine.current_state.matches("active.r3.done"));
    assert!(!machine.current_state.matches("finished"));
    assert_eq!(machine.status(), MachineStatus::Active);

    // Complete R4 -> now all 4 regions are final -> onDone must fire!
    machine.transition("FINISH_4", &Value::Null, &IndexMap::new(), &IndexMap::new());
    assert!(machine.current_state.matches("finished"));
    assert_eq!(machine.status(), MachineStatus::Done);
}

#[test]
fn test_parallel_completion_4_regions_simultaneous() {
    let json = r#"{
        "id": "par4_simul",
        "initial": "active",
        "states": {
            "active": {
                "type": "parallel",
                "states": {
                    "r1": {
                        "initial": "start",
                        "states": {
                            "start": { "on": { "FINISH_ALL": "done" } },
                            "done": { "type": "final" }
                        }
                    },
                    "r2": {
                        "initial": "start",
                        "states": {
                            "start": { "on": { "FINISH_ALL": "done" } },
                            "done": { "type": "final" }
                        }
                    },
                    "r3": {
                        "initial": "start",
                        "states": {
                            "start": { "on": { "FINISH_ALL": "done" } },
                            "done": { "type": "final" }
                        }
                    },
                    "r4": {
                        "initial": "start",
                        "states": {
                            "start": { "on": { "FINISH_ALL": "done" } },
                            "done": { "type": "final" }
                        }
                    }
                },
                "onDone": "finished"
            },
            "finished": { "type": "final" }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    machine.start(&IndexMap::new(), &IndexMap::new());

    assert_eq!(machine.status(), MachineStatus::Active);

    // Single FINISH_ALL transitions all 4 regions simultaneously
    machine.transition("FINISH_ALL", &Value::Null, &IndexMap::new(), &IndexMap::new());

    assert!(machine.current_state.matches("finished"));
    assert_eq!(machine.status(), MachineStatus::Done);
}

#[test]
fn test_parallel_with_history_pseudo_states_completion() {
    let json = r#"{
        "id": "par_hist",
        "initial": "active",
        "states": {
            "active": {
                "type": "parallel",
                "states": {
                    "hist_root": {
                        "type": "history"
                    },
                    "r1": {
                        "initial": "start",
                        "states": {
                            "hist_r1": { "type": "history" },
                            "start": { "on": { "F1": "done" } },
                            "done": { "type": "final" }
                        }
                    },
                    "r2": {
                        "initial": "start",
                        "states": {
                            "start": { "on": { "F2": "done" } },
                            "done": { "type": "final" }
                        }
                    },
                    "r3": {
                        "initial": "start",
                        "states": {
                            "hist_r3": { "type": "history", "history": "deep" },
                            "start": { "on": { "F3": "done" } },
                            "done": { "type": "final" }
                        }
                    }
                },
                "onDone": "finished"
            },
            "finished": { "type": "final" }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    machine.start(&IndexMap::new(), &IndexMap::new());

    assert_eq!(machine.status(), MachineStatus::Active);

    machine.transition("F1", &Value::Null, &IndexMap::new(), &IndexMap::new());
    machine.transition("F2", &Value::Null, &IndexMap::new(), &IndexMap::new());
    machine.transition("F3", &Value::Null, &IndexMap::new(), &IndexMap::new());

    // Even though hist_root, hist_r1, and hist_r3 exist, all regions reached final
    assert!(machine.current_state.matches("finished"));
    assert_eq!(machine.status(), MachineStatus::Done);
}

#[test]
fn test_inner_parallel_on_done_to_active_state_does_not_call_root_exit_or_stop() {
    let root_exit_called = Rc::new(RefCell::new(false));
    let r_exit = root_exit_called.clone();

    let json = r#"{
        "id": "inner_par_machine",
        "initial": "processing",
        "exit": "root_exit",
        "states": {
            "processing": {
                "initial": "parallel_work",
                "states": {
                    "parallel_work": {
                        "type": "parallel",
                        "states": {
                            "w1": {
                                "initial": "s",
                                "states": {
                                    "s": { "on": { "W1_DONE": "d" } },
                                    "d": { "type": "final" }
                                }
                            },
                            "w2": {
                                "initial": "s",
                                "states": {
                                    "s": { "on": { "W2_DONE": "d" } },
                                    "d": { "type": "final" }
                                }
                            }
                        },
                        "onDone": "next_stage"
                    },
                    "next_stage": {
                        "on": {
                            "COMPLETE_ALL": "all_done"
                        }
                    },
                    "all_done": {
                        "type": "final"
                    }
                }
            }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    let mut actions = IndexMap::new();
    actions.insert("root_exit".to_string(), Rc::new(move |_ctx: &mut Value, _evt: &Value, _p: &Option<Value>| {
        *r_exit.borrow_mut() = true;
    }) as lavi::ActionFunction);

    machine.start(&IndexMap::new(), &actions);
    assert_eq!(machine.status(), MachineStatus::Active);

    machine.transition("W1_DONE", &Value::Null, &IndexMap::new(), &actions);
    machine.transition("W2_DONE", &Value::Null, &IndexMap::new(), &actions);

    // parallel_work is done, took onDone to next_stage!
    std::println!("Current state after W1 & W2: {:?}", machine.current_state.get_active_paths());
    std::println!("Machine status: {:?}", machine.status());
    std::println!("Root exit called: {}", *root_exit_called.borrow());

    assert!(machine.current_state.matches("processing.next_stage"));
    assert_eq!(machine.status(), MachineStatus::Active);
    assert!(!*root_exit_called.borrow(), "Root exit must NOT be called when inner parallel state completes to an active next_stage!");
}

#[test]
fn test_inner_compound_child_final_does_not_mark_root_done() {
    let json = r#"{
        "id": "root_active_test",
        "initial": "step1",
        "states": {
            "step1": {
                "initial": "sub",
                "states": {
                    "sub": {
                        "initial": "working",
                        "states": {
                            "working": {
                                "on": { "FINISH_INNER": "inner_done" }
                            },
                            "inner_done": {
                                "type": "final"
                            }
                        }
                    }
                },
                "on": {
                    "GO_TO_STEP2": "step2"
                }
            },
            "step2": {
                "on": {
                    "FINISH_ROOT": "root_done"
                }
            },
            "root_done": {
                "type": "final"
            }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    machine.start(&IndexMap::new(), &IndexMap::new());

    assert_eq!(machine.status(), MachineStatus::Active);
    assert!(!machine.is_done());

    // Transition inner compound state to its final state
    machine.transition("FINISH_INNER", &Value::Null, &IndexMap::new(), &IndexMap::new());

    std::println!("After FINISH_INNER: state = {:?}", machine.current_state.get_active_paths());
    std::println!("machine.status() = {:?}", machine.status());
    std::println!("machine.is_done() = {:?}", machine.is_done());

    // EXPECTATION: The machine MUST remain active because root is in `step1`, which is not final!
    // If machine.status() == MachineStatus::Done here, THIS IS A BUG!
    assert_eq!(
        machine.status(),
        MachineStatus::Active,
        "Machine status must remain Active when only inner compound child is in final state!"
    );
    assert!(
        !machine.is_done(),
        "Machine must not be done when only inner compound child is in final state!"
    );

    // Can we still send events to the machine?
    let next_state = machine.transition("GO_TO_STEP2", &Value::Null, &IndexMap::new(), &IndexMap::new());
    assert!(next_state.is_some(), "Machine should be able to transition to step2!");
    assert!(machine.current_state.matches("step2"));
    assert_eq!(machine.status(), MachineStatus::Active);

    // Now complete the root machine
    machine.transition("FINISH_ROOT", &Value::Null, &IndexMap::new(), &IndexMap::new());
    assert!(machine.current_state.matches("root_done"));
    assert_eq!(machine.status(), MachineStatus::Done);
    assert!(machine.is_done());
}

#[test]
fn test_inner_parallel_child_final_does_not_mark_root_done() {
    let json = r#"{
        "id": "inner_parallel_status_test",
        "initial": "processing",
        "states": {
            "processing": {
                "initial": "worker_parallel",
                "states": {
                    "worker_parallel": {
                        "type": "parallel",
                        "states": {
                            "p1": {
                                "initial": "s",
                                "states": {
                                    "s": { "on": { "P1_DONE": "d" } },
                                    "d": { "type": "final" }
                                }
                            },
                            "p2": {
                                "initial": "s",
                                "states": {
                                    "s": { "on": { "P2_DONE": "d" } },
                                    "d": { "type": "final" }
                                }
                            }
                        },
                        "onDone": "worker_done"
                    },
                    "worker_done": {
                        "on": { "PROCEED": "extra_step" }
                    },
                    "extra_step": {
                        "type": "final"
                    }
                },
                "on": {
                    "EXIT_PROCESSING": "final_state"
                }
            },
            "final_state": {
                "type": "final"
            }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    machine.start(&IndexMap::new(), &IndexMap::new());

    assert_eq!(machine.status(), MachineStatus::Active);

    // Finish P1 and P2
    machine.transition("P1_DONE", &Value::Null, &IndexMap::new(), &IndexMap::new());
    machine.transition("P2_DONE", &Value::Null, &IndexMap::new(), &IndexMap::new());

    // worker_parallel completed, took onDone to worker_done
    assert!(machine.current_state.matches("processing.worker_done"));
    assert_eq!(machine.status(), MachineStatus::Active);
    assert!(!machine.is_done());

    // Now transition to extra_step (which is a final state inside processing)
    machine.transition("PROCEED", &Value::Null, &IndexMap::new(), &IndexMap::new());
    assert!(machine.current_state.matches("processing.extra_step"));

    // Root is still in `processing`! Does the root machine say Active or Done?
    assert_eq!(
        machine.status(),
        MachineStatus::Active,
        "Machine status must remain Active when inner child extra_step is final, because root is in processing!"
    );
    assert!(!machine.is_done());

    // Exit processing to final_state
    machine.transition("EXIT_PROCESSING", &Value::Null, &IndexMap::new(), &IndexMap::new());
    assert!(machine.current_state.matches("final_state"));
    assert_eq!(machine.status(), MachineStatus::Done);
    assert!(machine.is_done());
}

#[test]
fn test_reverse_document_order_exit_actions_and_root_exit_action() {
    let trace: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let last_evt_recorded: Rc<RefCell<Value>> = Rc::new(RefCell::new(Value::Null));

    let json = r#"{
        "id": "exit_order_machine",
        "initial": "a",
        "exit": "root_exit_action",
        "states": {
            "a": {
                "initial": "a1",
                "exit": "a_exit_action",
                "states": {
                    "a1": {
                        "initial": "a1_sub",
                        "exit": "a1_exit_action",
                        "states": {
                            "a1_sub": {
                                "exit": "a1_sub_exit_action",
                                "on": {
                                    "FINISH": "done_state"
                                }
                            }
                        }
                    }
                }
            },
            "done_state": {
                "type": "final"
            }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    let mut actions = IndexMap::new();

    let t1 = trace.clone();
    actions.insert("a1_sub_exit_action".to_string(), Rc::new(move |_ctx: &mut Value, _evt: &Value, _p: &Option<Value>| {
        t1.borrow_mut().push("exit:a1_sub".to_string());
    }) as lavi::ActionFunction);

    let t2 = trace.clone();
    actions.insert("a1_exit_action".to_string(), Rc::new(move |_ctx: &mut Value, _evt: &Value, _p: &Option<Value>| {
        t2.borrow_mut().push("exit:a1".to_string());
    }) as lavi::ActionFunction);

    let t3 = trace.clone();
    actions.insert("a_exit_action".to_string(), Rc::new(move |_ctx: &mut Value, _evt: &Value, _p: &Option<Value>| {
        t3.borrow_mut().push("exit:a".to_string());
    }) as lavi::ActionFunction);

    let t4 = trace.clone();
    let l_evt = last_evt_recorded.clone();
    actions.insert("root_exit_action".to_string(), Rc::new(move |_ctx: &mut Value, evt: &Value, _p: &Option<Value>| {
        t4.borrow_mut().push("exit:root".to_string());
        *l_evt.borrow_mut() = evt.clone();
    }) as lavi::ActionFunction);

    machine.start(&IndexMap::new(), &actions);
    trace.borrow_mut().clear();

    let mut payload = IndexMap::new();
    payload.insert("type".to_string(), Value::String("FINISH".to_string()));
    payload.insert("secretCode".to_string(), Value::Number(12345.0));
    let finish_event = Value::Object(payload);

    machine.transition("FINISH", &finish_event, &IndexMap::new(), &actions);

    // Active state must be done_state
    assert!(machine.current_state.matches("done_state"));
    assert_eq!(machine.status(), MachineStatus::Done);

    // Exit order must be reverse document order: a1_sub -> a1 -> a -> root
    let expected_order = vec![
        "exit:a1_sub".to_string(),
        "exit:a1".to_string(),
        "exit:a".to_string(),
        "exit:root".to_string(),
    ];
    assert_eq!(*trace.borrow(), expected_order);

    // Root exit action must receive the triggering event with payload!
    let recorded = last_evt_recorded.borrow().clone();
    if let Value::Object(ref map) = recorded {
        assert_eq!(map.get("type"), Some(&Value::String("FINISH".to_string())));
        assert_eq!(map.get("secretCode"), Some(&Value::Number(12345.0)));
    } else {
        panic!("Root exit action did not receive event payload: {:?}", recorded);
    }
}

#[test]
fn test_on_done_followed_by_always_transition() {
    let json = r#"{
        "id": "on_done_always",
        "initial": "step1",
        "states": {
            "step1": {
                "initial": "inner",
                "states": {
                    "inner": {
                        "on": { "FINISH": "done" }
                    },
                    "done": { "type": "final" }
                },
                "onDone": "step2"
            },
            "step2": {
                "always": "step3"
            },
            "step3": {
                "type": "final"
            }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    machine.start(&IndexMap::new(), &IndexMap::new());

    assert!(machine.current_state.matches("step1.inner"));
    machine.transition("FINISH", &Value::Null, &IndexMap::new(), &IndexMap::new());

    assert!(machine.current_state.matches("step3"));
    assert_eq!(machine.status(), MachineStatus::Done);
}

#[test]
fn test_parallel_with_immediate_always_transition_in_region() {
    let json = r#"{
        "id": "par_always",
        "initial": "active",
        "states": {
            "active": {
                "type": "parallel",
                "states": {
                    "r1": {
                        "initial": "start",
                        "states": {
                            "start": { "always": "done" },
                            "done": { "type": "final" }
                        }
                    },
                    "r2": {
                        "initial": "start",
                        "states": {
                            "start": { "on": { "FINISH_R2": "done" } },
                            "done": { "type": "final" }
                        }
                    }
                },
                "onDone": "finished"
            },
            "finished": { "type": "final" }
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
    machine.start(&IndexMap::new(), &IndexMap::new());

    assert!(machine.current_state.matches("active.r1.done"));
    assert!(machine.current_state.matches("active.r2.start"));

    machine.transition("FINISH_R2", &Value::Null, &IndexMap::new(), &IndexMap::new());

    assert!(machine.current_state.matches("finished"));
    assert_eq!(machine.status(), MachineStatus::Done);
}


