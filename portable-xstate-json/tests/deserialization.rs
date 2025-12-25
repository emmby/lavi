use portable_xstate_json::dto::{StateDefinitionDto, TransitionDefinitionDto};
use serde_json::{from_str, json};

#[test]
fn test_deserialize_states() {
    let json = r#"{
      "initial": "a",
      "states": {
        "a": { "on": { "T": "b" } },
        "b": {}
      }
    }"#;
    let m: portable_xstate_json::dto::MachineDefinitionDto = from_str(json).expect("Failed to parse machine");
    
    assert_eq!(m.initial, Some("a".to_string()));
    assert_eq!(m.states.len(), 2, "Should have 2 states");
    assert!(m.states.contains_key("a"));
    assert!(m.states.contains_key("b"));
}

#[test]
fn test_deserialize_always_string() {
    let json = r#"{ "always": "target" }"#;
    let s: portable_xstate_json::dto::StateDefinitionDto = from_str(json).expect("Failed to parse state");
    
    // assert!(s.always.is_some(), "always should be Some"); // Now it is Vec
    assert!(!s.always.is_empty(), "always should not be empty");
    match &s.always[0] {
        TransitionDefinitionDto::Target(t) => assert_eq!(t, "target"),
        _ => panic!("Expected Target variant"),
    }
}

#[test]
fn test_reproduce_vec_or_single_error() {
    let machine_json = json!({
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
    });

    let result: Result<StateDefinitionDto, _> = serde_json::from_value(machine_json);
    if let Err(e) = &result {
        println!("Error: {}", e);
    }
    assert!(result.is_ok());
}

#[test]
fn test_vec_or_single_null() {
    let json = json!({
        "on": {
            "EVENT": null
        }
    });
    let result: Result<StateDefinitionDto, _> = serde_json::from_value(json);
    assert!(result.is_ok());
}
