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

    #[test]
    fn test_unknown_event() {
        let json = r#"{
          "id": "test",
          "initial": "idle",
          "states": {
            "idle": { "on": { "START": "running" } },
            "running": {}
          }
        }"#;

        let mut machine = RuntimeMachine::from_json(json).expect("Failed to parse");
        assert_eq!(machine.state(), "idle");

        // "STOP" is not valid for "idle"
        let result = machine.transition("STOP");
        assert!(result.is_none());
        assert_eq!(machine.state(), "idle"); // State should remain unchanged
    }

    #[test]
    fn test_invalid_json() {
        let json = r#"{ "id": "broken", "initial": "idle", "states": ... }"#; // Invalid syntax
        let result = RuntimeMachine::from_json(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_initial_state() {
        // Initial state "phantom" does not exist in definitions
        let json = r#"{
          "id": "phantom_initial",
          "initial": "phantom",
          "states": {
            "real": { "on": {} }
          }
        }"#;

        let mut machine = RuntimeMachine::from_json(json).expect("Failed to parse");
        // It initializes successfully with current_state = "phantom"
        assert_eq!(machine.state(), "phantom");

        // Attempting to transition from a missing state should gracefully fail (return None)
        let result = machine.transition("ANY_EVENT");
        assert!(result.is_none());
    }
}
