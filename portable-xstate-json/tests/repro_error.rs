use portable_xstate_core::{RuntimeMachine, StateValue, Value};
use portable_xstate_json::{create_machine, from_json_str};
use std::collections::HashMap;

#[test]
fn test_parallel_deep_history_repro() {
    let json = r#"{
      "initial": "off",
      "states": {
        "off": {
          "on": {
            "SWITCH": "on",
            "DEEP_POWER": "on.deepHistory",
            "PARALLEL_TEST": { "target": ["on.A.hist", "on.K.hist"] }
          }
        },
        "on": {
          "type": "parallel",
          "states": {
            "A": {
              "initial": "B",
              "states": {
                "B": {
                  "on": { "INNER_A": "C" }
                },
                "C": {
                  "initial": "D",
                  "states": {
                    "D": {
                      "on": { "INNER_A": "E" }
                    },
                    "E": {}
                  }
                },
                "hist": { "history": true },
                "deepHistory": {
                  "history": "deep"
                }
              }
            },
            "K": {
              "initial": "L",
              "states": {
                "L": {
                  "on": { "INNER_K": "M" }
                },
                "M": {
                  "initial": "N",
                  "states": {
                    "N": {
                      "on": { "INNER_K": "O" }
                    },
                    "O": {}
                  }
                },
                "hist": { "history": true },
                "deepHistory": {
                  "history": "deep"
                }
              }
            },
            "deepHistory": {
              "history": "deep"
            }
          },
          "on": {
            "POWER": "off"
          }
        }
      }
    }"#;

    let definition = create_machine(json).expect("Failed to parse machine");
    let mut machine = RuntimeMachine::new(&definition).expect("Failed to create machine");

    // Inspect "on.A.states.B.on"
    if let Some(on_state) = definition.states.get("on") {
        if let Some(a_state) = on_state.states.get("A") {
            if let Some(b_state) = a_state.states.get("B") {
                println!("State B Transitions: {:?}", b_state.on);
            }
        }
    }

    let guards = indexmap::IndexMap::new();
    let actions = indexmap::IndexMap::new();
    machine.start(&actions);

    // 1. SWITCH -> on
    machine.transition("SWITCH", &Value::Null, &guards, &actions);
    println!("After SWITCH: {:?}", machine.current_state);

    // 2. INNER_A -> on.A.C.D
    machine.transition("INNER_A", &Value::Null, &guards, &actions);
    println!("After INNER_A (1): {:?}", machine.current_state);
    
    // 3. INNER_A -> on.A.C.E
    machine.transition("INNER_A", &Value::Null, &guards, &actions);
    println!("After INNER_A (2): {:?}", machine.current_state);

    // 4. INNER_K -> on.K.M.N
    machine.transition("INNER_K", &Value::Null, &guards, &actions);
    println!("After INNER_K (1): {:?}", machine.current_state);

    // 5. INNER_K -> on.K.M.O
    machine.transition("INNER_K", &Value::Null, &guards, &actions);
    println!("After INNER_K (2): {:?}", machine.current_state);

    // Check we are in "on" with correct sub-states
    println!("State before POWER: {:?}", machine.current_state);

    // 6. POWER -> off
    machine.transition("POWER", &Value::Null, &guards, &actions);
    println!("After POWER: {:?}", machine.current_state);
    // Should be "off"

    // 7. DEEP_POWER -> on.deepHistory
    machine.transition("DEEP_POWER", &Value::Null, &guards, &actions);

    println!("Restored State: {:?}", machine.current_state);
    
    // Check paths
    let paths = machine.current_state.get_active_paths();
    assert!(paths.contains(&"on.A.C.E".to_string()), "Missing on.A.C.E");
    assert!(paths.contains(&"on.K.M.O".to_string()), "Missing on.K.M.O");
    
    // 8. POWER -> off (Store history again)
    machine.transition("POWER", &Value::Null, &guards, &actions);
    println!("After POWER (2): {:?}", machine.current_state);
    
    // 9. PARALLEL_TEST -> ["on.A.hist", "on.K.hist"]
    machine.transition("PARALLEL_TEST", &Value::Null, &guards, &actions);
    println!("After PARALLEL_TEST: {:?}", machine.current_state);
    
    let paths_multi = machine.current_state.get_active_paths();
    assert!(paths_multi.contains(&"on.A.C.D".to_string()), "Missing on.A.C.D in Multi-Target");
    assert!(paths_multi.contains(&"on.K.M.N".to_string()), "Missing on.K.M.N in Multi-Target");
}
