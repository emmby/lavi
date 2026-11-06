use lavi::{create_machine, StateType};

#[test]
fn test_history_mapping() {
    let json = r#"{
      "initial": "on",
      "states": {
        "on": {
          "initial": "first",
          "states": {
            "first": {},
            "hist": { "type": "history" }
          }
        }
      }
    }"#;

    let def = create_machine(json).expect("Failed to parse");

    let on_state = def.states.get("on").expect("Missing 'on'");
    let hist_state = on_state.states.get("hist").expect("Missing 'hist'");

    println!("Hist Type: {:?}", hist_state.state_type);
    assert_eq!(
        hist_state.state_type,
        StateType::History { deep: false },
        "Should be History (shallow) type"
    );
}
