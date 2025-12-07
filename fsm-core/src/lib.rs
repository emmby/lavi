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
pub struct TransitionDefinition {
    pub target: String,
    #[serde(default)]
    pub cond: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StateDefinition {
    #[serde(default)]
    pub on: HashMap<String, Vec<TransitionDefinition>>, // Event -> List of Transitions
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

    pub fn transition(
        &mut self,
        event: &str,
        guards: &HashMap<String, Box<dyn Fn(&serde_json::Value) -> bool>>,
    ) -> Option<String> {
        // Look up: Current State -> on[Event] -> List of Transitions
        if let Some(state_def) = self.definition.states.get(&self.current_state) {
            if let Some(transitions) = state_def.on.get(event) {
                for transition in transitions {
                    let should_transition = match &transition.cond {
                        Some(cond_name) => {
                            if let Some(guard) = guards.get(cond_name) {
                                guard(&self.context)
                            } else {
                                // Guard specified but not found in implementation map.
                                // In XState, this usually logs a warning and treats as false.
                                // For this simple implementation, we'll treat as false (block transition).
                                false
                            }
                        }
                        None => true, // No guard, always take it
                    };

                    if should_transition {
                        self.current_state = transition.target.clone();
                        return Some(self.current_state.clone());
                    }
                }
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
            "green": { "on": { "TIMER": [ { "target": "yellow" } ] } },
            "yellow": { "on": { "TIMER": [ { "target": "red" } ] } },
            "red": { "on": { "TIMER": [ { "target": "green" } ] } }
          },
          "context": { "cycles": 0 }
        }"#;

        let mut machine = RuntimeMachine::from_json(json).expect("Failed to parse");
        assert_eq!(machine.state(), "green");
        
        let guards = HashMap::new();

        machine.transition("TIMER", &guards);
        assert_eq!(machine.state(), "yellow");
        
        machine.transition("TIMER", &guards);
        assert_eq!(machine.state(), "red");
        
        machine.transition("TIMER", &guards);
        assert_eq!(machine.state(), "green");
    }

    #[test]
    fn test_unknown_event() {
        let json = r#"{
          "id": "test",
          "initial": "idle",
          "states": {
            "idle": { "on": { "START": [ { "target": "running" } ] } },
            "running": {}
          }
        }"#;

        let mut machine = RuntimeMachine::from_json(json).expect("Failed to parse");
        assert_eq!(machine.state(), "idle");

        let guards = HashMap::new();

        // "STOP" is not valid for "idle"
        let result = machine.transition("STOP", &guards);
        assert!(result.is_none());
        assert_eq!(machine.state(), "idle"); // State should remain unchanged
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

        let guards = HashMap::new();

        // Attempting to transition from a missing state should gracefully fail (return None)
        let result = machine.transition("ANY_EVENT", &guards);
        assert!(result.is_none());
    }

    #[test]
    fn test_guarded_transition() {
        let json = r#"{
          "id": "door",
          "initial": "closed",
          "states": {
            "closed": {
              "on": {
                "OPEN": [
                  { "target": "open", "cond": "can_open" },
                  { "target": "closed" } 
                ]
              }
            },
            "open": {
              "on": { "CLOSE": [ { "target": "closed" } ] }
            }
          },
          "context": { "locked": true }
        }"#;

        let mut machine = RuntimeMachine::from_json(json).expect("Failed to parse");
        
        // Define guards
        let mut guards: HashMap<String, Box<dyn Fn(&serde_json::Value) -> bool>> = HashMap::new();
        guards.insert("can_open".to_string(), Box::new(|ctx| {
            // Allow open if NOT locked
            if let Some(locked) = ctx.get("locked").and_then(|v| v.as_bool()) {
                !locked
            } else {
                true 
            }
        }));

        // 1. Initial State: Closed, Context: Locked=true
        assert_eq!(machine.state(), "closed");

        // 2. Try to OPEN. Guard 'can_open' checks !locked. locked is true, so !locked is false.
        // It should match the FIRST transition? No, XState: check first, if false check next.
        // Our definition: match 'can_open' -> false. Next: { target: "closed" } (implicit true).
        // So state stays 'closed'.
        machine.transition("OPEN", &guards);
        assert_eq!(machine.state(), "closed"); 

        // 3. Unlock the door (modify context manually for test)
        if let Some(obj) = machine.context.as_object_mut() {
            obj.insert("locked".to_string(), serde_json::Value::Bool(false));
        }

        // 4. Try to OPEN again. Guard 'can_open' -> true. Transition to 'open'.
        machine.transition("OPEN", &guards);
        assert_eq!(machine.state(), "open");
    }
}
