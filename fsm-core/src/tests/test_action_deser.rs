use super::*;
use serde_json::json;

#[test]
fn test_generic_action_deserialization_null_params() {
    let json_str = r#"{"type": "generated_action", "params": null}"#;
    let action: Action = serde_json::from_str(json_str).unwrap();

    if let Action::Generic(g) = action {
        assert_eq!(g.action_type, "generated_action");
        assert_eq!(g.params, None); // Expect None for null
    } else {
        panic!("Expected GenericAction, got {:?}", action);
    }
}

#[test]
fn test_assign_action_deserialization_fail() {
    // Should fail to deserialize as AssignAction because 'assignment' is missing
    let json_str = r#"{"type": "generated_action", "params": null}"#;
    let res: Result<AssignAction, _> = serde_json::from_str(json_str);
    assert!(res.is_err(), "Should fail to deserialize AssignAction without assignment");
}

#[test]
fn test_action_enum_resolution() {
    let json_str = r#"{"type": "generated_action", "params": null}"#;
    let action: Action = serde_json::from_str(json_str).unwrap();
    match action {
        Action::Generic(_) => {},
        _ => panic!("Should match GenericAction, matched {:?}", action),
    }
}
