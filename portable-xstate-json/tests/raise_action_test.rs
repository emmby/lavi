use portable_xstate_core::{RuntimeMachine, Value};
use portable_xstate_json::from_json_str;
use indexmap::IndexMap;

#[test]
fn test_raise_action_internal_queue() {
    let json = r#"{
        "id": "raise_test",
        "initial": "step1",
        "states": {
            "step1": {
                "on": {
                    "START": {
                        "target": "step2",
                        "actions": [
                            {
                                "type": "xstate.raise",
                                "event": { "type": "INTERNAL_EV" }
                            }
                        ]
                    }
                }
            },
            "step2": {
                "on": {
                    "INTERNAL_EV": "step3"
                }
            },
            "step3": {
                "type": "final"
            }
        }
    }"#;

    let machine_def = from_json_str(json).expect("valid json");
    let mut machine = RuntimeMachine::new(&machine_def).unwrap();
    let actions = IndexMap::new();
    let guards = IndexMap::new();

    machine.start(&guards, &actions);
    
    // Send external START event
    let payload = Value::Null;
    machine.transition("START", &payload, &guards, &actions);

    // After transition("START"), it should go to step2, 
    // enqueue "INTERNAL_EV", and then autonomously drain it to reach step3.
    let current = machine.state();
    assert_eq!(current, "step3", "Machine didn't autonomously process the raised internal event");
}
