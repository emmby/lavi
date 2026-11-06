use lavi::{
    assign, create_machine, setup, Action, HistoryType, RuntimeMachine, StateValue, Value,
};

#[test]
fn test_deserialize_states() {
    let json = r#"{
      "initial": "a",
      "states": {
        "a": { "on": { "T": "b" } },
        "b": {}
      }
    }"#;
    let m = create_machine(json).expect("Failed to parse machine");

    assert_eq!(m.initial, "a");
    assert_eq!(m.states.len(), 2, "Should have 2 states");
    assert!(m.states.contains_key("a"));
    assert!(m.states.contains_key("b"));
}

#[test]
fn test_deserialize_always_string() {
    let json = r#"{
      "states": {
        "s": { "always": "target" }
      }
    }"#;
    let m = create_machine(json).expect("Failed to parse machine");
    let s = m.states.get("s").expect("Missing state s");
    let always = s.on.get("").expect("always should be present in on");

    assert!(!always.is_empty(), "always should not be empty");
    assert_eq!(always[0].target, Some(vec!["target".to_string()]));

    // Also verify root machine always
    let root_json = r#"{ "always": "target" }"#;
    let root_m = create_machine(root_json).expect("Failed to parse root machine");
    let root_always = root_m.on.get("").expect("always should be present in root on");
    assert!(!root_always.is_empty(), "root always should not be empty");
    assert_eq!(root_always[0].target, Some(vec!["target".to_string()]));
}

#[test]
fn test_reproduce_vec_or_single_error() {
    let json = r#"{
        "id": "guards-machine",
        "initial": "start",
        "context": { "count": 0 },
        "states": {
            "start": {
                "on": {
                    "INC": {
                        "target": null,
                        "cond": null,
                        "actions": [{ "type": "generated_assign_1", "params": null }]
                    },
                    "NEXT": [
                        { 
                            "target": "high", 
                            "cond": { "type": "greaterThan", "params": { "min": 5 } } 
                        },
                        {
                            "target": "medium",
                            "cond": { "type": "greaterThan", "params": { "min": 2 } }
                        },
                        { 
                            "target": "start" 
                        }
                    ]
                }
            },
            "medium": {
                "on": { "RESET": { "target": "start", "actions": ["resetCount"] } }
            },
            "high": {
                "on": { "RESET": { "target": "start", "actions": ["resetCount"] } }
            }
        }
    }"#;

    let result = create_machine(json);
    assert!(result.is_ok());
    let machine = result.unwrap();
    assert_eq!(machine.id, "guards-machine");
    assert_eq!(machine.states.len(), 3);
}

#[test]
fn test_vec_or_single_null() {
    let json = r#"{
        "on": {
            "EVENT": null
        }
    }"#;
    let result = create_machine(json);
    assert!(result.is_ok());
    let machine = result.unwrap();
    assert!(machine.on.contains_key("EVENT"));
    assert_eq!(machine.on.get("EVENT").unwrap().len(), 0);

    let state_json = r#"{
        "states": {
            "s": {
                "on": {
                    "EVENT": null
                }
            }
        }
    }"#;
    let state_res = create_machine(state_json);
    assert!(state_res.is_ok());
    let s = state_res.unwrap().states.get("s").cloned().unwrap();
    assert!(s.on.contains_key("EVENT"));
    assert_eq!(s.on.get("EVENT").unwrap().len(), 0);
}

#[test]
fn test_create_machine_api() {
    let json = r#"{
      "id": "trafficLight",
      "initial": "green",
      "states": {
        "green": { "on": { "TIMER": [ { "target": "yellow" } ] } },
        "yellow": { "on": { "TIMER": [ { "target": "red" } ] } },
        "red": { "on": { "TIMER": [ { "target": "green" } ] } }
      },
      "context": { "cycles": 0 }
    }"#;

    let definition = create_machine(json).expect("Failed to create machine");
    let machine = RuntimeMachine::new(&definition).expect("Failed to create runtime machine");
    assert_eq!(
        machine.current_state,
        StateValue::Atomic("green".to_string())
    );
}

#[test]
fn test_setup_api() {
    let json = r#"{
      "id": "setupMachine",
      "initial": "a",
      "states": { "a": {} }
    }"#;
    let setup_obj = setup("{}");
    let definition = setup_obj
        .create_machine(json)
        .expect("Failed to create machine via setup");
    assert_eq!(definition.id, "setupMachine");
}

#[test]
fn test_assign_api() {
    let assign_json = r#"{ "count": 1 }"#;
    let action = assign(assign_json).expect("Failed to create assign action");
    match action {
        Action::Assign(a) => {
            assert_eq!(a.action_type, "xstate.assign");
            match &a.assignment {
                Value::Object(map) => {
                    assert_eq!(map.get("count"), Some(&Value::Number(1.0)));
                }
                _ => panic!("Expected Object assignment"),
            }
        }
        _ => panic!("Expected Assign action"),
    }
}

#[test]
fn test_history_mapping() {
    let json = r#"{
      "initial": "first",
      "states": {
        "first": {},
        "second": {
           "initial": "nested",
           "states": {
             "nested": {},
             "hist": { "type": "history" }
           },
           "on": { "SWITCH": "hist" }
        }
      }
    }"#;
    let definition = create_machine(json).expect("Failed to parse history machine");

    // Verify "second" state exists
    let second = definition.states.get("second").expect("Missing second state");

    // Verify "hist" in "second"
    assert!(
        second.states.contains_key("hist"),
        "Missing hist state in second"
    );

    let hist = second.states.get("hist").unwrap();
    assert_eq!(hist.type_.as_deref(), Some("history"));
    assert_eq!(hist.history, Some(HistoryType::Shallow));
}

#[test]
fn test_implicit_history_mapping() {
    let json = r#"{
      "initial": "first",
      "states": {
        "first": {},
        "second": {
           "initial": "nested",
           "states": {
             "nested": {},
             "hist": { "history": true }
           },
           "on": { "SWITCH": "hist" }
        }
      }
    }"#;
    let definition = create_machine(json).expect("Failed to parse history machine");

    let second = definition.states.get("second").expect("Missing second state");
    let hist = second.states.get("hist").unwrap();

    // Should infer history type even if type field is missing
    assert_eq!(hist.history, Some(HistoryType::Shallow));
}

#[test]
fn test_explicit_deep_history_mapping() {
    let json = r#"{
      "initial": "first",
      "states": {
        "first": {},
        "second": {
           "initial": "nested",
           "states": {
             "nested": {},
             "hist": { "type": "history", "history": "deep" }
           },
           "on": { "SWITCH": "hist" }
        }
      }
    }"#;
    let definition = create_machine(json).expect("Failed to parse history machine");

    let second = definition.states.get("second").expect("Missing second state");
    let hist = second.states.get("hist").unwrap();

    assert_eq!(hist.type_.as_deref(), Some("history"));
    assert_eq!(hist.history, Some(HistoryType::Deep));
}
