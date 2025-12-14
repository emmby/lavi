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
pub struct GuardObject {
    #[serde(rename = "type")]
    pub guard_type: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum TransitionGuard {
    Name(String),
    Object(GuardObject),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TransitionDefinition {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "guard")]
    pub cond: Option<TransitionGuard>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub actions: Vec<Action>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StateDefinition {
    #[serde(default)]
    pub initial: Option<String>,
    #[serde(default)]
    pub states: HashMap<String, StateDefinition>,

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
        let initial_top = definition.initial.clone();
        // Resolve deep initial state
        let current_state = Self::resolve_initial_state(&definition.states, &initial_top);
        let context = definition.context.clone().unwrap_or(serde_json::Value::Null);
        
        Ok(Self {
            definition,
            current_state,
            context,
        })
    }

    pub fn resolve_initial_state(states: &HashMap<String, StateDefinition>, start_key: &str) -> String {
        let mut current_key = start_key.to_string();
        // let current_states = states; // Removed unused variable
        
        // Loop to dig deeper if there's an initial state
        loop {
            // Find the state definition for the current key's leaf
            // (Note: This simple logic assumes start_key is a direct child of 'states' for the first step)
            // For proper recursion, we need to traverse the path if it's already dotted.
            // But from_json starts with top-level key.
            
            // NOTE: This logic assumes we build the path incrementally. 
            // If start_key is "active", we look in states["active"].
            // If that has initial="one", we append ".one" and look in states["active"].states["one"].
            
            // Actually, let's just implement the "descent" from the current level.
            // But we need to look up `current_key` in the generic map.
            // It's cleaner to traverse.
            
            // Simplified approach: Just verify if we can go deeper. 
            // We need a helper that takes the FULL path and returns the Definition + Context for next step?
            // Or just recursive helper.
            
            // Let's restart logic:
            // 1. We have a path (e.g. "active"). 
            // 2. Resolve definition.
            // 3. If definition has `initial`, append it (e.g. "active.one") and recurse.
            // 4. Else return path.
             
            if let Some(def) = Self::get_state_def_recursive(states, &current_key) {
                 if let Some(ref init) = def.initial {
                     current_key = format!("{}.{}", current_key, init);
                     // Continue loop with new key
                 } else {
                     break;
                 }
            } else {
                break;
            }
        }
        current_key
    }

    pub fn get_state_definition(&self, path: &str) -> Option<&StateDefinition> {
        Self::get_state_def_recursive(&self.definition.states, path)
    }

    fn get_state_def_recursive<'a>(
        root_states: &'a HashMap<String, StateDefinition>, 
        path: &str
    ) -> Option<&'a StateDefinition> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current_map = root_states;
        let mut last_def = None;

        for part in parts.iter() {
            if let Some(def) = current_map.get(*part) {
                last_def = Some(def);
                current_map = &def.states;
            } else {
                return None;
            }
        }
        last_def
    }


    pub fn transition(
        &mut self,
        event: &str,
        event_payload: &serde_json::Value,
        guards: &HashMap<String, Rc<dyn Fn(&serde_json::Value, &serde_json::Value, &Option<serde_json::Value>) -> bool>>,
        actions: &HashMap<String, Rc<dyn Fn(&mut serde_json::Value, &serde_json::Value, &Option<serde_json::Value>)>>,
    ) -> Option<String> {
        // Look up: Current State -> on[Event] -> List of Transitions
        // Support hierarchy: If current state is "active.one", check "active.one", then "active".
        
        let mut state_path = self.current_state.clone();
        
        loop {
            if let Some(state_def) = Self::get_state_def_recursive(&self.definition.states, &state_path) {
                if let Some(transitions) = state_def.on.get(event) {
                    for transition in transitions {
                        let should_transition = match &transition.cond {
                            Some(guard_def) => {
                                let (guard_name, params) = match guard_def {
                                    TransitionGuard::Name(s) => (s, &None),
                                    TransitionGuard::Object(obj) => (&obj.guard_type, &obj.params),
                                };

                                if let Some(guard) = guards.get(guard_name) {
                                    guard(&self.context, event_payload, params)
                                } else {
                                    false
                                }
                            }
                            None => true,
                        };

                        if should_transition {
                            let target_base = transition.target.clone().unwrap_or(state_path.clone());
                            // If target is sibling, resolve relative to parent? 
                            // XState resolution is complex. For cheatsheet:
                            // If target="inactive", and we are at "active.one", we look for "active.inactive" then "inactive".
                            // For this MVP, assume absolute paths or siblings of ROOT unless formatted.
                            // Actually, standard string targets often imply "sibling" or "top-level".
                            // Simplification: Try to find target in Root. If not, maybe implementation specific.
                            // The test uses "inactive" (sibling of active) and "two" (sibling of one).
                            // So "two" -> "active.two". "inactive" -> "inactive".

                            let next_state_full_path = if transition.target.is_none() {
                                state_path.clone() // Internal / stay
                            } else {
                                // Resolve target path
                                // 1. Try absolute/top-level
                                if self.definition.states.contains_key(&target_base) {
                                    Self::resolve_initial_state(&self.definition.states, &target_base)
                                } else {
                                    // 2. Try sibling of current matched state (state_path)
                                    // If state_path is "active", sibling checks root.
                                    // If state_path is "active.one", sibling is "active.two".
                                    if let Some(parent_dot) = state_path.rfind('.') {
                                        let parent_path = &state_path[..parent_dot];
                                        let sibling_path = format!("{}.{}", parent_path, target_base);
                                        // Verify it exists
                                        if Self::get_state_def_recursive(&self.definition.states, &sibling_path).is_some() {
                                            Self::resolve_initial_state(&self.definition.states, &sibling_path)
                                        } else {
                                            // Fallback to absolute/base (might fail but best guess)
                                            target_base
                                        }
                                    } else {
                                        // No parent, so we already checked root. Just return base.
                                        target_base
                                    }
                                }
                            };
                            
                            let is_internal = transition.target.is_none(); 

                            if !is_internal {
                                // 1. Execute Exit Actions for Current State (and parents up to LCCA?)
                                // MVP: Just exit current leaf.
                                if let Some(current_state_def) = Self::get_state_def_recursive(&self.definition.states, &self.current_state) {
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
                                self.current_state = next_state_full_path.clone();

                                // 4. Execute Entry Actions for New State (and deep children)
                                // MVP: Just enter the final leaf. Correct XState enters all down the path.
                                // For now, let's just execute the leaf's entry.
                                if let Some(new_state_def) = Self::get_state_def_recursive(&self.definition.states, &self.current_state) {
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
            
            // Bubble up
            if let Some(dot_idx) = state_path.rfind('.') {
                state_path = state_path[..dot_idx].to_string();
            } else {
                break; // Reached root, no handled event
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
        let mut guards: HashMap<String, Rc<dyn Fn(&serde_json::Value, &serde_json::Value, &Option<serde_json::Value>) -> bool>> = HashMap::new();
        guards.insert("can_open".to_string(), Rc::new(|ctx, _evt, _params| {
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

    #[test]
    fn test_parameterized_guard() {
        let json = r#"{
          "id": "param_test",
          "initial": "a",
          "states": {
            "a": {
              "on": {
                "NEXT": [
                  { "target": "b", "cond": { "type": "greaterThan", "params": { "min": 10 } } }
                ]
              }
            },
            "b": {}
          },
          "context": { "val": 5 }
        }"#;

        let mut machine = RuntimeMachine::from_json(json).expect("Failed to parse");
        
        let mut guards: HashMap<String, Rc<dyn Fn(&serde_json::Value, &serde_json::Value, &Option<serde_json::Value>) -> bool>> = HashMap::new();
        guards.insert("greaterThan".to_string(), Rc::new(|ctx, _evt, params| {
            let val = ctx.get("val").and_then(|v| v.as_i64()).unwrap_or(0);
            if let Some(p) = params {
                let min = p.get("min").and_then(|v| v.as_i64()).unwrap_or(0);
                val > min
            } else {
                false
            }
        }));
        
        let actions = HashMap::new();
        let payload = serde_json::Value::Null;

        // 1. val is 5, min is 10. 5 > 10 is false. Should NOT transition.
        machine.transition("NEXT", &payload, &guards, &actions);
        assert_eq!(machine.state(), "a");

        // 2. Update context val to 15.
        if let Some(obj) = machine.context.as_object_mut() {
            obj.insert("val".to_string(), serde_json::Value::Number(15.into()));
        }

        // 3. val is 15, min is 10. 15 > 10 is true. Should transition.
        machine.transition("NEXT", &payload, &guards, &actions);
        assert_eq!(machine.state(), "b");
    }
}

