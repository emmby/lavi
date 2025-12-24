use portable_xstate_json::dto::TransitionDefinitionDto;
use serde_json::from_str;

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
