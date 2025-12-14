use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::rc::Rc;

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
#[serde(untagged)]
pub enum Action {
    Named(String),
    Assign(AssignAction),
    Generic(GenericAction),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AssignAction {
    #[serde(rename = "type")]
    pub action_type: String, // Expect "xstate.assign"
    pub assignment: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GenericAction {
    #[serde(rename = "type")]
    pub action_type: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TransitionDefinition {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "guard")]
    pub cond: Option<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub actions: Vec<Action>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StateDefinition {
    #[serde(default, deserialize_with = "deserialize_map_vec_or_single")]
    pub on: HashMap<String, Vec<TransitionDefinition>>, // Event -> List of Transitions
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub entry: Vec<Action>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub exit: Vec<Action>,
}

// ... helper functions omitted for brevity in replacement chunk ...

fn deserialize_vec_or_single<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VecOrSingle<T> {
        Vec(Vec<T>),
        Single(T),
    }

    match VecOrSingle::deserialize(deserializer)? {
        VecOrSingle::Vec(v) => Ok(v),
        VecOrSingle::Single(s) => Ok(vec![s]),
    }
}

fn deserialize_map_vec_or_single<'de, D, T>(deserializer: D) -> Result<HashMap<String, Vec<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VecOrSingle<T> {
        Vec(Vec<T>),
        Single(T),
    }

    let map: HashMap<String, VecOrSingle<T>> = HashMap::deserialize(deserializer)?;
    
    let mut result = HashMap::new();
    for (key, value) in map {
        let vec = match value {
            VecOrSingle::Vec(v) => v,
            VecOrSingle::Single(s) => vec![s],
        };
        result.insert(key, vec);
    }
    Ok(result)
}

// --- Runtime ---

#[derive(Debug, Clone)]
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
        guards: &HashMap<String, Rc<dyn Fn(&serde_json::Value, &serde_json::Value) -> bool>>,
        actions: &HashMap<String, Rc<dyn Fn(&mut serde_json::Value, &serde_json::Value, &Option<serde_json::Value>)>>,
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
                                for action in &current_state_def.exit {
                                    Self::execute_action(&mut self.context, action, event_payload, actions);
                                }
                            }
                        }

                        // 2. Execute Transition Actions
                        for action in &transition.actions {
                            Self::execute_action(&mut self.context, action, event_payload, actions);
                        }

                        if !is_internal {
                            // 3. Update State
                            self.current_state = next_state_name.clone();

                            // 4. Execute Entry Actions for New State
                            if let Some(new_state_def) = self.definition.states.get(&self.current_state) {
                                for action in &new_state_def.entry {
                                    Self::execute_action(&mut self.context, action, event_payload, actions);
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

    fn execute_action(
        context: &mut serde_json::Value,
        action: &Action,
        event_payload: &serde_json::Value,
        actions: &HashMap<String, Rc<dyn Fn(&mut serde_json::Value, &serde_json::Value, &Option<serde_json::Value>)>>,
    ) {
        match action {
            Action::Named(name) => {
                if let Some(action_fn) = actions.get(name) {
                    action_fn(context, event_payload, &None);
                }
            }
            Action::Generic(generic_action) => {
                 if let Some(action_fn) = actions.get(&generic_action.action_type) {
                     action_fn(context, event_payload, &generic_action.params);
                 }
            }
            Action::Assign(assign_action) => {
                // Handle static assignment
                if let Some(ctx_obj) = context.as_object_mut() {
                    // Check if assignment is a partial object and merge it
                    if let Some(partial) = assign_action.assignment.as_object() {
                        for (k, v) in partial {
                            ctx_obj.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
        }
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
        let mut guards: HashMap<String, Rc<dyn Fn(&serde_json::Value, &serde_json::Value) -> bool>> = HashMap::new();
        guards.insert("can_open".to_string(), Rc::new(|ctx, _evt| {
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

    #[test]
    fn test_assign_action() {
        let json = r#"{
          "id": "counter",
          "initial": "a",
          "context": { "count": 0, "fixed": "no" },
          "states": {
            "a": {
              "on": {
                "INC": [
                  { 
                    "target": "b",
                    "actions": [
                      { "type": "xstate.assign", "assignment": { "count": 1 } },
                      { "type": "xstate.assign", "assignment": { "fixed": "yes" } }
                    ]
                  }
                ]
              }
            },
            "b": {}
          }
        }"#;

        let mut machine = RuntimeMachine::from_json(json).expect("Failed to parse");
        
        let guards = HashMap::new();
        let actions = HashMap::new();
        let payload = serde_json::Value::Null;

        assert_eq!(machine.state(), "a");
        let ctx = machine.context.as_object().unwrap();
        assert_eq!(ctx.get("count").unwrap().as_i64(), Some(0));
        assert_eq!(ctx.get("fixed").unwrap().as_str(), Some("no"));

        machine.transition("INC", &payload, &guards, &actions);
        
        assert_eq!(machine.state(), "b");
        let ctx_new = machine.context.as_object().unwrap();
        assert_eq!(ctx_new.get("count").unwrap().as_i64(), Some(1));
        assert_eq!(ctx_new.get("fixed").unwrap().as_str(), Some("yes"));
    }

    #[test]
    fn test_single_action_deserialization() {
        let json = r#"{
          "id": "single_action",
          "initial": "active",
          "context": { "count": 0 },
          "states": {
            "active": {
              "entry": { 
                 "type": "assign", 
                 "assignment": {} 
              },
              "on": { "NEXT": [ { "target": "inactive" } ] }
            },
            "inactive": {}
          }
        }"#;

        let machine = RuntimeMachine::from_json(json);
        assert!(machine.is_ok(), "Failed to parse single action object: {:?}", machine.err());
        
        let machine = machine.unwrap();
        if let Some(state) = machine.definition.states.get("active") {
            assert_eq!(state.entry.len(), 1);
            if let Action::Assign(assign) = &state.entry[0] {
                assert_eq!(assign.action_type, "assign");
            } else {
                panic!("Expected Assign action");
            }
        }
    }
}
