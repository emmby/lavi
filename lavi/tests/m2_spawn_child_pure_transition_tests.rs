use lavi::{create_machine, Action, RuntimeMachine, StateValue, Value};

#[test]
fn test_initial_transition_pure_emits_spawn_child_for_root_and_state_invoke() {
    let json = r#"{
        "id": "myMachine",
        "initial": "a",
        "invoke": {
            "src": "rootChildService",
            "id": "rootActor",
            "input": { "mode": "fast" }
        },
        "states": {
            "a": {
                "invoke": {
                    "src": "stateChildService",
                    "id": "stateActor"
                },
                "on": {
                    "NEXT": "b"
                }
            },
            "b": {
                "invoke": {
                    "src": "anotherService"
                }
            }
        }
    }"#;

    let def = create_machine(json).expect("Failed to parse machine");
    let rm = RuntimeMachine::from_definition(def).expect("Failed to create runtime machine");

    let (state_val, _ctx, actions) = rm.initial_transition_pure(None);

    assert_eq!(state_val, StateValue::Atomic("a".to_string()));

    // Verify root invoke emitted
    let root_spawn = actions.iter().find(|act| {
        if let Action::Generic(ga) = act {
            if ga.action_type == "xstate.spawnChild" {
                if let Some(Value::Object(p)) = &ga.params {
                    return p.get("id").and_then(|v| v.as_string()).map(|s| s.as_str()) == Some("rootActor");
                }
            }
        }
        false
    });
    assert!(root_spawn.is_some(), "Root invoke xstate.spawnChild should be emitted");

    if let Some(Action::Generic(ga)) = root_spawn {
        let p = ga.params.as_ref().unwrap().as_object().unwrap();
        assert_eq!(p.get("src").and_then(|v| v.as_string()).map(|s| s.as_str()), Some("rootChildService"));
        assert!(p.get("input").is_some());
    }

    // Verify state invoke emitted
    let state_spawn = actions.iter().find(|act| {
        if let Action::Generic(ga) = act {
            if ga.action_type == "xstate.spawnChild" {
                if let Some(Value::Object(p)) = &ga.params {
                    return p.get("id").and_then(|v| v.as_string()).map(|s| s.as_str()) == Some("stateActor");
                }
            }
        }
        false
    });
    assert!(state_spawn.is_some(), "State invoke xstate.spawnChild should be emitted");

    // Now test transition NEXT -> b
    let (next_state, trans_actions) = rm.transition_pure(&state_val, "NEXT", &Value::Null);
    assert_eq!(next_state, StateValue::Atomic("b".to_string()));

    let b_spawn = trans_actions.iter().find(|act| {
        if let Action::Generic(ga) = act {
            if ga.action_type == "xstate.spawnChild" {
                if let Some(Value::Object(p)) = &ga.params {
                    return p.get("src").and_then(|v| v.as_string()).map(|s| s.as_str()) == Some("anotherService");
                }
            }
        }
        false
    });
    assert!(b_spawn.is_some(), "Entering b should emit xstate.spawnChild for b's invoke");
}
