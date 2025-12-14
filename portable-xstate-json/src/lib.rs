mod dto;
mod mapper;

use portable_xstate_core::MachineDefinition;
pub use dto::MachineDefinitionDto;

pub fn from_json_str(json: &str) -> Result<MachineDefinition, serde_json::Error> {
    let dto: MachineDefinitionDto = serde_json::from_str(json)?;
    Ok(mapper::map_machine(dto))
}

#[cfg(test)]
mod tests {
    use super::*;
    use portable_xstate_core::{RuntimeMachine, Value};
    use std::collections::HashMap;

    #[test]
    fn test_traffic_light_runtime_via_json() {
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

         let definition = from_json_str(json).expect("Failed to parse");
         let mut machine = RuntimeMachine::new(definition);
         
         assert_eq!(machine.state(), "green");
         
         let guards = HashMap::new();
         let actions = HashMap::new();
         let payload = Value::Null;

         machine.transition("TIMER", &payload, &guards, &actions);
         assert_eq!(machine.state(), "yellow");
    }
}
