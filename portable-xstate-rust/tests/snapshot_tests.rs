extern crate alloc;

use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::RefCell;
use indexmap::IndexMap;
use portable_xstate_rust::{
    MachineStatus, RuntimeMachine, StateValue, Value,
};

#[test]
fn test_snapshot_atomic_and_context_roundtrip() {
    let json = r#"{
        "id": "toggle",
        "initial": "inactive",
        "context": { "count": 10, "label": "test" },
        "states": {
            "inactive": {
                "tags": ["off"],
                "on": {
                    "TOGGLE": {
                        "target": "active",
                        "actions": [
                            { "type": "xstate.assign", "assignment": { "count": 42, "label": "updated" } }
                        ]
                    }
                }
            },
            "active": {
                "tags": ["on", "busy"],
                "on": {
                    "TOGGLE": "inactive",
                    "FINISH": "done"
                }
            },
            "done": {
                "type": "final",
                "tags": ["finished"]
            }
        }
    }"#;

    let mut m = RuntimeMachine::from_json(json).expect("valid machine");
    m.start(&IndexMap::new(), &IndexMap::new());

    // Initially inactive
    assert!(m.current_state.matches("inactive"));
    assert!(m.has_tag("off"));
    assert!(!m.has_tag("on"));

    // Serialize snapshot in initial state
    let snap_str = m.get_persisted_snapshot_json();
    assert!(snap_str.contains("\"status\":\"active\""));
    assert!(snap_str.contains("\"value\":\"inactive\""));
    assert!(snap_str.contains("\"count\":10"));

    // Transition to active with context assignment
    let toggle_ev = Value::event("TOGGLE");
    m.transition("TOGGLE", &toggle_ev, &IndexMap::new(), &IndexMap::new());
    assert!(m.current_state.matches("active"));
    assert!(m.has_tag("on"));
    assert!(m.has_tag("busy"));

    // Snapshot in active state
    let active_snap = m.get_persisted_snapshot_json();
    assert!(active_snap.contains("\"value\":\"active\""));
    assert!(active_snap.contains("\"count\":42"));
    assert!(active_snap.contains("\"label\":\"updated\""));

    // Restore snapshot into a fresh machine
    let mut m_restored = RuntimeMachine::from_json(json).expect("valid machine");
    m_restored.restore_from_snapshot_json(&active_snap).expect("restore succeeded");

    // Restored machine should have active state, tags, and updated context
    assert!(m_restored.current_state.matches("active"));
    assert!(m_restored.has_tag("on"));
    assert!(m_restored.has_tag("busy"));
    let tags = m_restored.get_tags();
    assert!(tags.contains(&"on".to_string()));
    assert!(tags.contains(&"busy".to_string()));

    let ctx_obj = m_restored.context.as_object().unwrap();
    assert_eq!(ctx_obj.get("count").and_then(|v| v.as_f64()), Some(42.0));
    assert_eq!(
        ctx_obj.get("label").and_then(|v| v.as_string()),
        Some(&"updated".to_string())
    );

    // Starting the restored machine must NOT re-run initial entry actions
    m_restored.start(&IndexMap::new(), &IndexMap::new());
    assert!(m_restored.current_state.matches("active"));

    // Can transition from restored state to final
    let finish_ev = Value::event("FINISH");
    m_restored.transition("FINISH", &finish_ev, &IndexMap::new(), &IndexMap::new());
    assert!(m_restored.current_state.matches("done"));
    assert!(m_restored.is_done());
    assert_eq!(m_restored.status(), MachineStatus::Done);
}

#[test]
fn test_snapshot_compound_and_parallel_states() {
    let json = r#"{
        "id": "parallelMachine",
        "type": "parallel",
        "states": {
            "Audio": {
                "initial": "Muted",
                "states": {
                    "Muted": { "on": { "UNMUTE": "Playing" } },
                    "Playing": { "on": { "MUTE": "Muted" } }
                }
            },
            "Display": {
                "initial": "Dark",
                "states": {
                    "Dark": { "on": { "LIGHT": "Bright" } },
                    "Bright": { "on": { "DARK": "Dark" } }
                }
            }
        }
    }"#;

    let mut m = RuntimeMachine::from_json(json).expect("valid machine");
    m.start(&IndexMap::new(), &IndexMap::new());

    // Initially Audio.Muted and Display.Dark
    assert!(m.current_state.matches("Audio.Muted"));
    assert!(m.current_state.matches("Display.Dark"));

    let unmute_ev = Value::event("UNMUTE");
    m.transition("UNMUTE", &unmute_ev, &IndexMap::new(), &IndexMap::new());
    assert!(m.current_state.matches("Audio.Playing"));
    assert!(m.current_state.matches("Display.Dark"));

    let snap_val = m.get_persisted_snapshot_value();
    let val_obj = snap_val
        .as_object()
        .and_then(|o| o.get("value"))
        .and_then(|v| v.as_object())
        .expect("snapshot value is object");
    assert_eq!(
        val_obj.get("Audio").and_then(|v| v.as_string()),
        Some(&"Playing".to_string())
    );
    assert_eq!(
        val_obj.get("Display").and_then(|v| v.as_string()),
        Some(&"Dark".to_string())
    );

    let snap_json = m.get_persisted_snapshot_json();

    // Restore into fresh machine
    let mut m2 = RuntimeMachine::from_json(json).expect("valid machine");
    m2.restore_from_snapshot_json(&snap_json).expect("restore succeeded");

    assert!(m2.current_state.matches("Audio.Playing"));
    assert!(m2.current_state.matches("Display.Dark"));
    assert!(!m2.current_state.matches("Audio.Muted"));
}

#[test]
fn test_snapshot_history_revival_and_default_omission() {
    let json = r#"{
        "id": "player",
        "initial": "off",
        "states": {
            "off": {
                "on": {
                    "POWER": "on.hist",
                    "SWITCH": "on"
                }
            },
            "on": {
                "initial": "first",
                "states": {
                    "hist": {
                        "type": "history"
                    },
                    "first": {
                        "on": { "NEXT": "second" }
                    },
                    "second": {
                        "id": "secondNode",
                        "on": { "PREV": "first" }
                    }
                }
            }
        }
    }"#;

    let mut m = RuntimeMachine::from_json(json).expect("valid machine");
    m.start(&IndexMap::new(), &IndexMap::new());

    // Switch to on -> enters on.first (default initial)
    let switch_ev = Value::event("SWITCH");
    m.transition("SWITCH", &switch_ev, &IndexMap::new(), &IndexMap::new());
    assert!(m.current_state.matches("on.first"));

    // In default initial state `on.first`, history should be omitted (empty historyValue)
    let initial_hist = m.compute_persisted_history_value();
    assert_eq!(initial_hist, Value::Object(IndexMap::new()));

    // Advance to on.second
    let next_ev = Value::event("NEXT");
    m.transition("NEXT", &next_ev, &IndexMap::new(), &IndexMap::new());
    assert!(m.current_state.matches("on.second"));

    // In non-default `secondNode`, history should be serialized
    let active_hist = m.compute_persisted_history_value();
    let hist_map = active_hist.as_object().unwrap();
    assert!(hist_map.contains_key("(machine).on.hist") || hist_map.contains_key("player.on.hist"));
    let entries = hist_map.values().next().unwrap().as_array().unwrap();
    assert_eq!(
        entries[0]
            .as_object()
            .unwrap()
            .get("id")
            .unwrap()
            .as_string(),
        Some(&"secondNode".to_string())
    );

    let snap_str = m.get_persisted_snapshot_json();

    // Restore into fresh machine and power cycle through history
    let mut m2 = RuntimeMachine::from_json(json).expect("valid machine");
    m2.restore_from_snapshot_json(&snap_str).expect("restore succeeded");

    // m2 is in `on.second`. Send POWER -> off
    let off_json = r#"{
        "status": "active",
        "value": "off",
        "context": {},
        "historyValue": {
            "player.on.hist": [{ "id": "secondNode" }]
        },
        "tags": [],
        "children": {}
    }"#;

    let mut m3 = RuntimeMachine::from_json(json).expect("valid machine");
    m3.restore_from_snapshot_json(off_json).expect("restore succeeded");
    assert!(m3.current_state.matches("off"));

    // Now send POWER to on.hist -> should resume into on.second, NOT on.first!
    let power_ev = Value::event("POWER");
    m3.transition("POWER", &power_ev, &IndexMap::new(), &IndexMap::new());
    assert!(m3.current_state.matches("on.second"));
    assert!(!m3.current_state.matches("on.first"));
}

#[test]
fn test_snapshot_history_unresolved_id_warning() {
    let json = r#"{
        "id": "warnMachine",
        "initial": "off",
        "states": {
            "off": {
                "on": { "POWER": "on.hist" }
            },
            "on": {
                "initial": "first",
                "states": {
                    "hist": { "type": "history" },
                    "first": {},
                    "second": {}
                }
            }
        }
    }"#;

    let mut m = RuntimeMachine::from_json(json).expect("valid machine");
    let warnings = Rc::new(RefCell::new(Vec::new()));
    let warnings_clone = warnings.clone();
    m.register_warn_fn(Rc::new(move |msg: &str| {
        warnings_clone.borrow_mut().push(msg.to_string());
    }));

    // Restore snapshot with nonexistent id in history
    let bad_hist_json = r#"{
        "status": "active",
        "value": "off",
        "context": {},
        "historyValue": {
            "warnMachine.on.hist": [{ "id": "nonexistent_node" }]
        },
        "tags": [],
        "children": {}
    }"#;

    assert!(m.restore_from_snapshot_json(bad_hist_json).is_ok());

    let logged = warnings.borrow();
    assert_eq!(logged.len(), 1);
    assert_eq!(
        logged[0],
        "Could not resolve StateNode for id: nonexistent_node"
    );

    // Fallback: transitioning to on.hist should fall back to initial `first`
    let power_ev = Value::event("POWER");
    m.transition("POWER", &power_ev, &IndexMap::new(), &IndexMap::new());
    assert!(m.current_state.matches("on.first"));
}

#[test]
fn test_snapshot_handles_primitive_and_null_history_gracefully() {
    let json = r#"{
        "id": "primitiveHist",
        "initial": "a",
        "states": {
            "a": { "on": { "GO": "b.hist" } },
            "b": {
                "initial": "b1",
                "states": {
                    "hist": { "type": "history" },
                    "b1": {}
                }
            }
        }
    }"#;

    let primitives = [
        "null",
        "42",
        "\"a string\"",
        "true",
        "false",
        "[]",
    ];

    for prim in primitives {
        let snap = alloc::format!(
            r#"{{ "status": "active", "value": "a", "context": {{}}, "historyValue": {}, "tags": [], "children": {{}} }}"#,
            prim
        );
        let mut m = RuntimeMachine::from_json(json).expect("valid machine");
        assert!(m.restore_from_snapshot_json(&snap).is_ok());
        assert!(m.current_state.matches("a"));
    }
}

#[test]
fn test_snapshot_stopped_status_and_tags() {
    let json = r#"{
        "id": "stopMachine",
        "initial": "running",
        "states": {
            "running": {
                "tags": ["alive"],
                "on": { "PAUSE": "paused" }
            },
            "paused": {
                "tags": ["idle"]
            }
        }
    }"#;

    let mut m = RuntimeMachine::from_json(json).expect("valid machine");
    m.start(&IndexMap::new(), &IndexMap::new());
    m.stop();
    assert_eq!(m.status(), MachineStatus::Stopped);

    let snap_str = m.get_persisted_snapshot_json();
    assert!(snap_str.contains("\"status\":\"stopped\""));

    let mut m2 = RuntimeMachine::from_json(json).expect("valid machine");
    m2.restore_from_snapshot_json(&snap_str).expect("restore ok");
    assert_eq!(m2.status(), MachineStatus::Stopped);
}

#[test]
fn test_snapshot_64kb_stack_deep_nesting() {
    // 300-depth state tree serialization and restoration in a thread with a 64 KB stack
    let builder = std::thread::Builder::new().stack_size(64 * 1024);
    let handler = builder
        .spawn(|| {
            // Build a 300-depth StateValue
            let depth = 300;
            let mut curr = StateValue::Atomic(alloc::format!("leaf_{}", depth));
            for i in (0..depth).rev() {
                curr = StateValue::Compound(alloc::format!("level_{}", i), alloc::boxed::Box::new(curr));
            }

            // Test to_value on 300-depth tree (must not overflow 64 KB stack)
            let val = curr.to_value();
            assert!(val.as_object().is_some());

            // Serialize to JSON string
            let json_str = val.to_json_string();
            assert!(json_str.contains("leaf_300"));

            // Parse back from JSON string
            let parsed_val = Value::from_json(&json_str).expect("parsed back");
            let reconstructed = portable_xstate_rust::value_to_state_value(parsed_val);

            // Verify matches at deep leaf
            let mut expected_path = String::new();
            for i in 0..depth {
                expected_path.push_str(&alloc::format!("level_{}.", i));
            }
            expected_path.push_str(&alloc::format!("leaf_{}", depth));

            assert!(reconstructed.matches(&expected_path));
        })
        .unwrap();

    handler.join().expect("64 KB stack deep nesting test passed without overflow");
}

#[test]
fn test_snapshot_status_roundtrip_all_states() {
    let json = r#"{
        "id": "statusMachine",
        "initial": "step1",
        "states": {
            "step1": {
                "on": {
                    "NEXT": "step2",
                    "FINISH": "completed"
                }
            },
            "step2": {
                "on": {
                    "FINISH": "completed"
                }
            },
            "completed": {
                "type": "final"
            }
        }
    }"#;

    // 1. ACTIVE STATE
    let mut m_active = RuntimeMachine::from_json(json).expect("valid machine");
    m_active.start(&IndexMap::new(), &IndexMap::new());
    let next_ev = Value::event("NEXT");
    m_active.transition("NEXT", &next_ev, &IndexMap::new(), &IndexMap::new());
    assert_eq!(m_active.status(), MachineStatus::Active);
    assert!(!m_active.is_done());
    assert!(m_active.current_state.matches("step2"));

    let active_snap = m_active.get_persisted_snapshot_json();
    assert!(active_snap.contains("\"status\":\"active\""));

    let mut m_active_restored = RuntimeMachine::from_json(json).expect("valid machine");
    m_active_restored.restore_from_snapshot_json(&active_snap).expect("restore ok");
    assert_eq!(m_active_restored.status(), MachineStatus::Active);
    assert!(!m_active_restored.is_done());
    assert!(m_active_restored.current_state.matches("step2"));
    assert_eq!(
        m_active_restored.get_persisted_snapshot_json(),
        active_snap
    );

    // Responds to event
    let finish_ev = Value::event("FINISH");
    m_active_restored.transition("FINISH", &finish_ev, &IndexMap::new(), &IndexMap::new());
    assert_eq!(m_active_restored.status(), MachineStatus::Done);
    assert!(m_active_restored.is_done());
    assert!(m_active_restored.current_state.matches("completed"));

    // 2. DONE STATE
    let mut m_done = RuntimeMachine::from_json(json).expect("valid machine");
    m_done.start(&IndexMap::new(), &IndexMap::new());
    m_done.transition("FINISH", &finish_ev, &IndexMap::new(), &IndexMap::new());
    assert_eq!(m_done.status(), MachineStatus::Done);
    assert!(m_done.is_done());

    let done_snap = m_done.get_persisted_snapshot_json();
    assert!(done_snap.contains("\"status\":\"done\""));

    let mut m_done_restored = RuntimeMachine::from_json(json).expect("valid machine");
    m_done_restored.restore_from_snapshot_json(&done_snap).expect("restore ok");
    assert_eq!(m_done_restored.status(), MachineStatus::Done);
    assert!(m_done_restored.is_done());
    assert!(m_done_restored.current_state.matches("completed"));
    assert_eq!(
        m_done_restored.get_persisted_snapshot_json(),
        done_snap
    );

    // Ignores event
    let dummy_ev = Value::event("NEXT");
    let res = m_done_restored.transition("NEXT", &dummy_ev, &IndexMap::new(), &IndexMap::new());
    assert!(res.is_none());
    assert_eq!(m_done_restored.status(), MachineStatus::Done);
    assert!(m_done_restored.current_state.matches("completed"));

    // 3. STOPPED STATE
    let mut m_stopped = RuntimeMachine::from_json(json).expect("valid machine");
    m_stopped.start(&IndexMap::new(), &IndexMap::new());
    m_stopped.transition("NEXT", &next_ev, &IndexMap::new(), &IndexMap::new());
    m_stopped.stop();
    assert_eq!(m_stopped.status(), MachineStatus::Stopped);
    assert!(!m_stopped.is_done());

    let stopped_snap = m_stopped.get_persisted_snapshot_json();
    assert!(stopped_snap.contains("\"status\":\"stopped\""));

    let mut m_stopped_restored = RuntimeMachine::from_json(json).expect("valid machine");
    m_stopped_restored.restore_from_snapshot_json(&stopped_snap).expect("restore ok");
    assert_eq!(m_stopped_restored.status(), MachineStatus::Stopped);
    assert!(!m_stopped_restored.is_done());
    assert!(m_stopped_restored.current_state.matches("step2"));
    assert_eq!(
        m_stopped_restored.get_persisted_snapshot_json(),
        stopped_snap
    );

    // Drops event
    let res = m_stopped_restored.transition("FINISH", &finish_ev, &IndexMap::new(), &IndexMap::new());
    assert!(res.is_none());
    assert_eq!(m_stopped_restored.status(), MachineStatus::Stopped);
    assert!(m_stopped_restored.current_state.matches("step2"));
}

