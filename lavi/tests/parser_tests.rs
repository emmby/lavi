#[allow(unused_imports)]
use lavi::{
    assign, create_machine, setup, value_to_state_value, Action, HistoryType,
    RuntimeMachine, StateType, StateValue, TransitionGuard, Value,
};

#[test]
fn test_iterative_states_preserves_exact_insertion_order() {
    let json = r#"{
        "id": "orderTest",
        "states": {
            "z_first": {
                "states": {
                    "c_sub1": {},
                    "a_sub2": {},
                    "b_sub3": {}
                }
            },
            "a_second": {},
            "m_third": {}
        }
    }"#;
    let def = create_machine(json).expect("valid machine");
    // When initial is omitted at root, first key in states definition order is selected
    assert_eq!(def.initial, "z_first");
    let top_keys: Vec<&str> = def.states.keys().map(|k| k.as_str()).collect();
    assert_eq!(top_keys, vec!["z_first", "a_second", "m_third"]);

    let z_sub_keys: Vec<&str> = def.states["z_first"].states.keys().map(|k| k.as_str()).collect();
    assert_eq!(z_sub_keys, vec!["c_sub1", "a_sub2", "b_sub3"]);
}

#[test]
fn test_guard_alias_and_after_formats() {
    let json = r#"{
        "id": "poly",
        "initial": "s1",
        "states": {
            "s1": {
                "after": [
                    { "delay": 250, "target": "s2", "guard": "isReady" },
                    { "delay": "500", "target": "s3", "cond": { "type": "minCount", "params": { "min": 3 } } }
                ]
            },
            "s2": {
                "after": {
                    "1000": "s3"
                }
            },
            "s3": {}
        }
    }"#;
    let def = create_machine(json).expect("valid machine");
    let s1 = &def.states["s1"];
    assert_eq!(s1.after[&250][0].cond, Some(TransitionGuard::Name("isReady".to_string())));
    match &s1.after[&500][0].cond {
        Some(TransitionGuard::Object(g)) => assert_eq!(g.guard_type, "minCount"),
        other => panic!("Expected GuardObject, got {:?}", other),
    }
    assert_eq!(def.states["s2"].after[&1000][0].target, Some(vec!["s3".to_string()]));
}

#[test]
fn test_setup_assign_and_value_to_state_value() {
    let def = setup("{}").create_machine(r#"{"id":"m","initial":"a","states":{"a":{}}}"#).unwrap();
    assert_eq!(def.id, "m");

    let act = assign(r#"{"count": 5}"#).unwrap();
    match act {
        Action::Assign(a) => assert_eq!(a.action_type, "xstate.assign"),
        _ => panic!("Expected Action::Assign"),
    }

    let sv = value_to_state_value(Value::from_json(r#"{"parent":{"r1":"a","r2":"b"}}"#).unwrap());
    assert_eq!(sv.get_active_paths(), vec!["parent.r1.a".to_string(), "parent.r2.b".to_string()]);
}

#[test]
fn test_deep_machine_on_small_stack() {
    // Build a 32-level nested compound state machine
    let depth = 32;
    let mut json = String::from(r#"{"id":"deep","initial":"s0","context":{"n":1},"states":{"#);
    for i in 0..depth {
        if i < depth - 1 {
            json.push_str(&format!(r#""s{}":{{"initial":"s{}","states":{{"#, i, i + 1));
        } else {
            json.push_str(&format!(r#""s{}":{{"on":{{"PING":"s{}"}}}}"#, i, i));
        }
    }
    for _ in 0..depth - 1 {
        json.push_str("}}");
    }
    json.push_str("}}");

    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(move || {
            let mut machine = RuntimeMachine::from_json(&json).expect("should build on small stack");
            machine.start(&indexmap::IndexMap::new(), &indexmap::IndexMap::new());
            let expected_path = (0..depth)
                .map(|i| format!("s{}", i))
                .collect::<Vec<_>>()
                .join(".");
            assert!(machine.matches(&expected_path));
        })
        .unwrap();
    handle.join().unwrap();
}

#[test]
fn test_xstate_v5_params_event_and_assignment() {
    let json = r#"{
        "id": "v5Compat",
        "initial": "start",
        "context": { "count": 0 },
        "states": {
            "start": {
                "entry": [
                    {
                        "type": "xstate.assign",
                        "params": {
                            "assignment": { "count": 42 }
                        }
                    },
                    {
                        "type": "xstate.raise",
                        "params": {
                            "event": { "type": "AUTO_NEXT" }
                        }
                    }
                ],
                "on": {
                    "AUTO_NEXT": "doneState"
                }
            },
            "doneState": {}
        }
    }"#;

    let mut machine = RuntimeMachine::from_json(json).expect("should parse XState v5 serialized config");
    machine.start(&indexmap::IndexMap::new(), &indexmap::IndexMap::new());

    assert!(machine.matches("doneState"));
    let ctx = machine.context.as_object().expect("context object");
    assert_eq!(ctx.get("count"), Some(&Value::from_json("42").unwrap()));
    assert!(ctx.get("assignment").is_none());
}
