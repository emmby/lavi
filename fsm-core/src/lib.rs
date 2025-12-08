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
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub cond: Option<String>,
    #[serde(default)]
    pub actions: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StateDefinition {
    #[serde(default)]
    pub on: HashMap<String, Vec<TransitionDefinition>>, // Event -> List of Transitions
    #[serde(default)]
    pub entry: Vec<String>,
    #[serde(default)]
    pub exit: Vec<String>,
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
        event_payload: &serde_json::Value,
        guards: &HashMap<String, Box<dyn Fn(&serde_json::Value, &serde_json::Value) -> bool>>,
        actions: &HashMap<String, Box<dyn Fn(&mut serde_json::Value, &serde_json::Value)>>,
    ) -> Option<String> {
        // Look up: Current State -> on[Event] -> List of Transitions
        if let Some(state_def) = self.definition.states.get(&self.current_state) {
            if let Some(transitions) = state_def.on.get(event) {
                for transition in transitions {
                    let should_transition = match &transition.cond {
                        Some(cond_name) => {
                            if let Some(guard) = guards.get(cond_name) {
                                guard(&self.context, event_payload)
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
                        // Determine next state (if target is None, it's a self-transition to current state, or rather internal?)
                        // In XState:
                        // - Internal (no target): actions run, no exit/entry.
                        // - External (target=current): exit, actions, entry.
                        // For simplicity here: Treat None as Internal (stay, no exit/entry, just actions).
                        // If user wants re-entry, they should specify target explicitly.
                        
                        let next_state_name = transition.target.clone().unwrap_or(self.current_state.clone());
                        let is_internal = transition.target.is_none(); 

                        if !is_internal {
                            // 1. Execute Exit Actions for Current State
                            if let Some(current_state_def) = self.definition.states.get(&self.current_state) {
                                for action_name in &current_state_def.exit {
                                    if let Some(action_fn) = actions.get(action_name) {
                                        action_fn(&mut self.context, event_payload);
                                    }
                                }
                            }
                        }

                        // 2. Execute Transition Actions
                        for action_name in &transition.actions {
                            if let Some(action_fn) = actions.get(action_name) {
                                action_fn(&mut self.context, event_payload);
                            }
                        }

                        if !is_internal {
                            // 3. Update State
                            self.current_state = next_state_name.clone();

                            // 4. Execute Entry Actions for New State
                            if let Some(new_state_def) = self.definition.states.get(&self.current_state) {
                                for action_name in &new_state_def.entry {
                                    if let Some(action_fn) = actions.get(action_name) {
                                        action_fn(&mut self.context, event_payload);
                                    }
                                }
                            }
                        }

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
mod reference_test;

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
        let actions = HashMap::new();
        let payload = serde_json::Value::Null;

        machine.transition("TIMER", &payload, &guards, &actions);
        assert_eq!(machine.state(), "yellow");
        
        machine.transition("TIMER", &payload, &guards, &actions);
        assert_eq!(machine.state(), "red");
        
        machine.transition("TIMER", &payload, &guards, &actions);
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
        let actions = HashMap::new();
        let payload = serde_json::Value::Null;

        // "STOP" is not valid for "idle"
        let result = machine.transition("STOP", &payload, &guards, &actions);
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
        let actions = HashMap::new();
        let payload = serde_json::Value::Null;

        // Attempting to transition from a missing state should gracefully fail (return None)
        let result = machine.transition("ANY_EVENT", &payload, &guards, &actions);
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
        let mut guards: HashMap<String, Box<dyn Fn(&serde_json::Value, &serde_json::Value) -> bool>> = HashMap::new();
        guards.insert("can_open".to_string(), Box::new(|ctx, _evt| {
            // Allow open if NOT locked
            if let Some(locked) = ctx.get("locked").and_then(|v| v.as_bool()) {
                !locked
            } else {
                true 
            }
        }));
        
        let actions = HashMap::new();
        let payload = serde_json::Value::Null;

        // 1. Initial State: Closed, Context: Locked=true
        assert_eq!(machine.state(), "closed");

        // 2. Try to OPEN. Guard 'can_open' checks !locked. locked is true, so !locked is false.
        machine.transition("OPEN", &payload, &guards, &actions);
        assert_eq!(machine.state(), "closed"); 

        // 3. Unlock the door (modify context manually for test)
        if let Some(obj) = machine.context.as_object_mut() {
            obj.insert("locked".to_string(), serde_json::Value::Bool(false));
        }

        // 4. Try to OPEN again. Guard 'can_open' -> true. Transition to 'open'.
        machine.transition("OPEN", &payload, &guards, &actions);
        assert_eq!(machine.state(), "open");
    }
}
