#![no_std]
extern crate alloc;

pub mod dto;
pub mod mapper;

use alloc::string::ToString;
use portable_xstate_core::{Action, AssignAction, MachineDefinition};

// Re-export DTO if needed for low-level access, but API prefers core types
pub use dto::MachineDefinitionDto;

// --- XState 5 API ---

/// Parses a JSON string into a MachineDefinition.
/// Mirrors `createMachine(config)` from XState.
pub fn create_machine(json: &str) -> Result<MachineDefinition, serde_json::Error> {
    from_json_str(json)
}

/// Helper to parse JSON string to MachineDefinitionDto then map to MachineDefinition
pub fn from_json_str(json: &str) -> Result<MachineDefinition, serde_json::Error> {
    let dto: MachineDefinitionDto = serde_json::from_str(json)?;
    Ok(mapper::map_machine(dto))
}

pub struct SetupObject {
    // In strict Rust no_std, we can't easily hold closures for implementations here
    // without generics or concrete types. For now, this is a pass-through
    // to allow the API syntax `setup(...).createMachine(...)`.
}

impl SetupObject {
    pub fn create_machine(&self, json: &str) -> Result<MachineDefinition, serde_json::Error> {
        create_machine(json)
    }
}

/// Mirrors `setup(options)` from XState.
/// Currently just returns a wrapper to call createMachine.
/// In the future, this could validate actors/actions against the generic types.
pub fn setup(_options: &str) -> SetupObject {
    SetupObject {}
}

/// Mirrors `assign(assignment)` from XState.
/// Returns an Action::Assign.
/// `assignment` can be a JSON string representing the assignment object or value.
pub fn assign(assignment_json: &str) -> Result<Action, serde_json::Error> {
    let val: serde_json::Value = serde_json::from_str(assignment_json)?;
    let core_val = mapper::map_value(val);
    Ok(Action::Assign(AssignAction {
        action_type: "xstate.assign".to_string(),
        assignment: core_val,
    }))
}

// Make mapper public so we can use map_value in assign
pub use mapper::{map_value, map_value_to_state_value};

#[cfg(test)]
mod tests {
    use super::*;
    use portable_xstate_core::{RuntimeMachine, Value};
    // use alloc::collections::BTreeMap; // Unused

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
         assert_eq!(machine.current_state, portable_xstate_core::StateValue::Atomic("green".to_string()));
    }

    #[test]
    fn test_setup_api() {
        let json = r#"{
           "id": "setupMachine",
           "initial": "a",
           "states": { "a": {} }
         }"#;
        let setup_obj = setup("{}");
        let definition = setup_obj.create_machine(json).expect("Failed to create machine via setup");
        assert_eq!(definition.id, "setupMachine");
    }

    #[test]
    fn test_assign_api() {
        let assign_json = r#"{ "count": 1 }"#;
        let action = assign(assign_json).expect("Failed to create assign action");
        match action {
            Action::Assign(a) => {
                assert_eq!(a.action_type, "xstate.assign");
                match a.assignment {
                    Value::Object(map) => {
                         assert_eq!(map.get("count"), Some(&Value::Number(1.0)));
                    }
                    _ => panic!("Expected Object assignment"),
                }
            },
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
         assert!(second.states.contains_key("hist"), "Missing hist state in second");
         
         let hist = second.states.get("hist").unwrap();
         assert_eq!(hist.type_.as_deref(), Some("history"));
         assert_eq!(hist.history, Some(portable_xstate_core::HistoryType::Shallow));
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
         assert_eq!(hist.history, Some(portable_xstate_core::HistoryType::Shallow));
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
         assert_eq!(hist.history, Some(portable_xstate_core::HistoryType::Deep));
    }
}

