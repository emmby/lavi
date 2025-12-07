use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- Definition (Schema) ---

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MachineDefinition {
    pub id: String,
    pub initial: String,
    pub states: HashMap<String, StateDefinition>,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StateDefinition {
    #[serde(default)]
    pub on: HashMap<String, String>, // Event -> Target State
}

// --- Runtime ---

#[derive(Debug)]
pub struct RuntimeMachine {
    pub definition: MachineDefinition,
    pub current_state: String,
    pub context: serde_json::Value,
}

impl RuntimeMachine {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let definition: MachineDefinition = serde_json::from_str(json)?;
        let current_state = definition.initial.clone();
        let context = definition.context.clone().unwrap_or(serde_json::Value::Null);
        
        Ok(Self {
            definition,
            current_state,
            context,
        })
    }

    pub fn transition(&mut self, event: &str) -> Option<String> {
        // Simple look up: Current State -> on[Event] -> Target
        if let Some(state_def) = self.definition.states.get(&self.current_state) {
            if let Some(target_state) = state_def.on.get(event) {
                self.current_state = target_state.clone();
                return Some(self.current_state.clone());
            }
        }
        None
    }
    
    pub fn state(&self) -> &str {
        &self.current_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_traffic_light_runtime() {
        let json = r#"{
          "id": "trafficLight",
          "initial": "green",
          "states": {
            "green": { "on": { "TIMER": "yellow" } },
            "yellow": { "on": { "TIMER": "red" } },
            "red": { "on": { "TIMER": "green" } }
          },
          "context": { "cycles": 0 }
        }"#;

        let mut machine = RuntimeMachine::from_json(json).expect("Failed to parse");
        assert_eq!(machine.state(), "green");
        
        machine.transition("TIMER");
        assert_eq!(machine.state(), "yellow");
        
        machine.transition("TIMER");
        assert_eq!(machine.state(), "red");
        
        machine.transition("TIMER");
        assert_eq!(machine.state(), "green");
    }
}
