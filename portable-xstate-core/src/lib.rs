#![no_std]

extern crate alloc;

use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use alloc::vec;
use indexmap::IndexMap;
// use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
// #[serde(untagged)]
pub enum Value {
    String(String),
    Number(f64),
    Bool(bool),
    Object(IndexMap<String, Value>),
    Array(Vec<Value>),
    Null,
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_object(&self) -> Option<&IndexMap<String, Value>> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }
    pub fn as_object_mut(&mut self) -> Option<&mut IndexMap<String, Value>> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(o) => o.get(key),
            _ => None,
        }
    }
}

// --- DTOs (Domain Transfer Objects) ---

#[derive(Debug, Clone, PartialEq)]
// #[serde(untagged)]
pub enum Action {
    Named(String),
    Assign(AssignAction),
    Generic(GenericAction),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssignAction {
    // #[serde(rename = "type")]
    pub action_type: String,
    pub assignment: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenericAction {
    // #[serde(rename = "type")]
    pub action_type: String,
    // #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
// #[serde(untagged)]
pub enum TransitionGuard {
    Name(String),
    Object(GuardObject),
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuardObject {
    // #[serde(rename = "type")]
    pub guard_type: String,
    // #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransitionDefinition {
    pub target: Option<Vec<String>>,
    pub cond: Option<TransitionGuard>,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HistoryType {
    Shallow,
    Deep,
}

#[derive(Debug, Clone)]
pub struct StateDefinition {
    pub id: Option<String>,
    pub type_: Option<String>,
    pub initial: Option<String>,
    pub states: IndexMap<String, StateDefinition>,
    pub on: IndexMap<String, Vec<TransitionDefinition>>, 
    pub entry: Vec<Action>,
    pub exit: Vec<Action>,

    pub tags: Vec<String>,
    pub history: Option<HistoryType>,
}

#[derive(Debug, Clone)]
pub struct MachineDefinition {
    pub id: String,
    pub initial: String,
    pub states: IndexMap<String, StateDefinition>,
    pub on: IndexMap<String, Vec<TransitionDefinition>>,
    pub entry: Vec<Action>,
    pub exit: Vec<Action>,
    pub context: Option<Value>,
    pub types: Option<Value>,
    pub id_map: IndexMap<String, String>,
}

// --- Runtime ---

#[derive(Debug, Clone)]
pub struct RuntimeMachine {
    pub definition: MachineDefinition,
    pub current_state: String,

    pub context: Value,
    pub history_value: IndexMap<String, String>,
}

impl RuntimeMachine {
    pub fn new(definition: MachineDefinition) -> Self {
        let initial_top = definition.initial.clone();
        let current_state = Self::resolve_initial_state(&definition.states, &initial_top);
        let context = definition.context.clone().unwrap_or(Value::Null);
        
        Self {
            definition,
            current_state,
            context,
            history_value: IndexMap::new(),
        }
    }

    fn resolve_initial_state(states: &IndexMap<String, StateDefinition>, initial: &str) -> String {
        let mut current_path = initial.to_string();
        while let Some(def) = Self::get_state_def_recursive(states, &current_path) {
            if let Some(sub_initial) = &def.initial {
                current_path = format!("{}.{}", current_path, sub_initial);
            } else {
                break;
            }
        }
        current_path
    }

    pub fn state(&self) -> String {
        self.current_state.clone()
    }

    pub fn context(&self) -> Value {
        self.context.clone()
    }

    pub fn get_state_definition(&self, path: &str) -> Option<&StateDefinition> {
        Self::get_state_def_recursive(&self.definition.states, path)
    }

    pub fn get_state_def_recursive<'a>(states: &'a IndexMap<String, StateDefinition>, path: &str) -> Option<&'a StateDefinition> {
        if path.is_empty() {
            return None;
        }
        let parts: Vec<&str> = path.split('.').collect();
        let mut current_states = states;
        let mut final_def = None;

        for (i, part) in parts.iter().enumerate() {
            if let Some(def) = current_states.get(*part) {
                if i == parts.len() - 1 {
                    final_def = Some(def);
                } else {
                    current_states = &def.states;
                }
            } else {
                return None;
            }
        }
        final_def
    }

    fn resolve_target(&self, source_path: &str, target: &str) -> Option<String> {
        // 1. Check ID map
        if let Some(path) = self.definition.id_map.get(target) {
            return Some(path.clone());
        }

        // 2. Resolve relative to source (ancestors)
        let mut parts: Vec<&str> = source_path.split('.').collect();
        while !parts.is_empty() {
             // Try prefix + target
             let prefix = parts.join(".");
             let candidate = if prefix.is_empty() { target.to_string() } else { format!("{}.{}", prefix, target) };
             if Self::get_state_def_recursive(&self.definition.states, &candidate).is_some() {
                 return Some(candidate);
             }
             // Pop last part and try again
             parts.pop();
        }
        
        // 3. Try as absolute path from root
        if Self::get_state_def_recursive(&self.definition.states, target).is_some() {
            return Some(target.to_string());
        }
        
        None
    }

    pub fn transition(
        &mut self,
        event_type: &str,
        event_payload: &Value,
        guards: &IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
        actions: &IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>)>>,
    ) -> Option<String> {
        let state_path = self.current_state.clone();
        let mut parts: Vec<String> = state_path.split('.').map(|s: &str| s.to_string()).collect();

        // Search for transition from current state up to root
        while !parts.is_empty() {
            let mut current_search_path = parts.join(".");
            if let Some(state_def) = Self::get_state_def_recursive(&self.definition.states, &current_search_path) {
                if let Some(transitions) = state_def.on.get(event_type) {
                    for transition in transitions {
                        // Check guard
                        let mut guard_ok = true;
                        if let Some(cond) = &transition.cond {
                             match cond {
                                 TransitionGuard::Name(name) => {
                                     if let Some(guard_fn) = guards.get(name) {
                                         guard_ok = guard_fn(&self.context, event_payload, &None);
                                     }
                                 },
                                 TransitionGuard::Object(obj) => {
                                     if let Some(guard_fn) = guards.get(&obj.guard_type) {
                                         guard_ok = guard_fn(&self.context, event_payload, &obj.params);
                                     }
                                 }
                             }
                        }

                        if !guard_ok { continue; }

                        // Transition matched!
                        if let Some(targets) = &transition.target {
                            // Support only first target for now
                            let raw_target = &targets[0];
                            let mut target_state = self.resolve_target(&current_search_path, raw_target).unwrap_or_else(|| raw_target.clone());
                            
                            // History State Resolution
                            if let Some(target_def) = Self::get_state_def_recursive(&self.definition.states, &target_state) {
                                if target_def.type_.as_deref() == Some("history") {
                                    let history_parent = if let Some(dot_idx) = target_state.rfind('.') {
                                        target_state[..dot_idx].to_string()
                                    } else {
                                        "".to_string()
                                    };

                                    if let Some(prev_state) = self.history_value.get(&history_parent) {
                                        target_state = prev_state.clone();

                                        // Deep History: Recursively resolve children history
                                        if let Some(history_type) = &target_def.history {
                                            match history_type {
                                                HistoryType::Deep => {
                                                    loop {
                                                        // target_state is now e.g. "on.second"
                                                        // Check if "on.second" has a history entry
                                                        if let Some(deep_res) = self.history_value.get(&target_state) {
                                                            target_state = deep_res.clone();
                                                        } else {
                                                            break;
                                                        }
                                                    }
                                                },
                                                _ => {}
                                            }
                                        }
                                    } else {
                                        // History fallback
                                        if let Some(hist_initial) = &target_def.initial {
                                             target_state = format!("{}.{}", target_state, hist_initial);
                                        } else if let Some(always_transitions) = target_def.on.get("") {
                                              if let Some(t) = always_transitions.first() {
                                                  if let Some(targ) = &t.target {
                                                      target_state = targ[0].clone();
                                                  }
                                              }
                                        } else {
                                            // Fallback to parent initial
                                            if let Some(parent_def) = Self::get_state_def_recursive(&self.definition.states, &history_parent) {
                                                if let Some(pi) = &parent_def.initial {
                                                     target_state = format!("{}.{}", history_parent, pi);
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            let next_state_full_path = Self::resolve_initial_state(&self.definition.states, &target_state);
                            
                            // Calculate LCA (Least Common Ancestor) for entry/exit
                            let current_parts: Vec<&str> = self.current_state.split('.').filter(|s| !s.is_empty()).collect();
                            let target_parts: Vec<&str> = next_state_full_path.split('.').filter(|s| !s.is_empty()).collect();
                            
                            let mut lca_len = 0;
                            // LCA calculation: common prefix steps
                            while lca_len < current_parts.len() && lca_len < target_parts.len() && current_parts[lca_len] == target_parts[lca_len] {
                                lca_len += 1;
                            }
                            
                            // 3. Exit Actions (Exited states: from deepest up to LCA)
                            if !self.current_state.is_empty() {
                                // Update history for exited states
                                for i in lca_len..current_parts.len() {
                                     // std::println!("DEBUG: i={}, current_parts={:?}, current_state={}", i, current_parts, self.current_state);
                                     if i > 0 {
                                         let parent_path = current_parts[0..i].join(".");
                                         let exited_path = current_parts[0..=i].join(".");
                                         // std::println!("DEBUG: inserting history: {} -> {}", parent_path, exited_path);
                                         self.history_value.insert(parent_path, exited_path);
                                     }
                                }

                                // Exit actions (reverse order, deepest first)
                                for i in (lca_len..current_parts.len()).rev() {
                                     let exited_path = current_parts[0..=i].join(".");
                                     if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &exited_path) {
                                         for action in &def.exit {
                                             Self::execute_action(&mut self.context, action, event_payload, actions);
                                         }
                                     }
                                }
                            }

                            // 4. Transition actions
                            for action in &transition.actions {
                                Self::execute_action(&mut self.context, action, event_payload, actions);
                            }
                            
                            // 5. Update State
                            self.current_state = next_state_full_path.clone(); 
                            
                             // 6. Entry Actions (Entered states: from LCA down to target)
                            for i in lca_len..target_parts.len() {
                                 let entered_path = target_parts[0..=i].join(".");
                                 if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &entered_path) {
                                     for action in &def.entry {
                                          Self::execute_action(&mut self.context, action, event_payload, actions);
                                     }
                                 }
                            }
                        } else {
                            // Internal transition: only transition actions
                            for action in &transition.actions {
                                Self::execute_action(&mut self.context, action, event_payload, actions);
                            }
                        }
                        
                        return Some(self.current_state.clone());
                    }
                }
            }
            
            // Bubble up
            if let Some(dot_idx) = current_search_path.rfind('.') {
               current_search_path = current_search_path[..dot_idx].to_string();
               parts = current_search_path.split('.').map(|s: &str| s.to_string()).collect();
            } else {
               parts = vec![];
            }
        }
        
        None
    }

    fn execute_action(
        context: &mut Value,
        action: &Action,
        event_payload: &Value,
        actions: &IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>)>>,
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
                if let Some(obj) = assign_action.assignment.as_object() {
                    if let Some(ctx_obj) = context.as_object_mut() {
                        for (k, v) in obj {
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
    use indexmap::IndexMap;

    #[test]
    fn test_initial_state_resolution() {
        let mut states = IndexMap::default();
        states.insert("a".to_string(), StateDefinition {
            id: None, type_: None, initial: None, states: IndexMap::default(), on: IndexMap::default(), entry: vec![], exit: vec![], tags: vec![], history: None
        });
        
        let path = RuntimeMachine::resolve_initial_state(&states, &"a".to_string());
        assert_eq!(path, "a");
    }

    #[test]
    fn test_nested_initial_state_resolution() {
        let mut child_states = IndexMap::default();
        child_states.insert("b".to_string(), StateDefinition {
            id: None, type_: None, initial: None, states: IndexMap::default(), on: IndexMap::default(), entry: vec![], exit: vec![], tags: vec![], history: None
        });
        
        let mut states = IndexMap::default();
        states.insert("a".to_string(), StateDefinition {
            id: None, type_: None, initial: Some("b".to_string()), states: child_states, on: IndexMap::default(), entry: vec![], exit: vec![], tags: vec![], history: None
        });
        
        let path = RuntimeMachine::resolve_initial_state(&states, &"a".to_string());
        assert_eq!(path, "a.b");
    }

    #[test]
    fn test_bubble_up_transition() {
        let mut child_states = IndexMap::default();
        child_states.insert("b".to_string(), StateDefinition {
            id: None, type_: None, initial: None, states: IndexMap::default(), on: IndexMap::default(), entry: vec![], exit: vec![], tags: vec![], history: None
        });
        
        let mut a_on = IndexMap::default();
        a_on.insert("EVENT".to_string(), vec![TransitionDefinition {
            target: Some(vec!["other".to_string()]),
            cond: None,
            actions: vec![],
        }]);

        let mut states = IndexMap::default();
        states.insert("a".to_string(), StateDefinition {
            id: None, type_: None, initial: Some("b".to_string()), states: child_states, on: a_on, entry: vec![], exit: vec![], tags: vec![], history: None
        });
        states.insert("other".to_string(), StateDefinition {
            id: None, type_: None, initial: None, states: IndexMap::default(), on: IndexMap::default(), entry: vec![], exit: vec![], tags: vec![], history: None
        });

        let definition = MachineDefinition {
            id: "m".to_string(),
            initial: "a".to_string(),
            states,
            on: IndexMap::default(),
            entry: vec![],
            exit: vec![],
            context: None,
            types: None,
            id_map: IndexMap::default(),
        };

        let mut machine = RuntimeMachine::new(definition);
        assert_eq!(machine.state(), "a.b");

        // Event should bubble up from 'a.b' to 'a'
        machine.transition("EVENT", &Value::Null, &IndexMap::default(), &IndexMap::default());
        assert_eq!(machine.state(), "other");
    }

    #[test]
    fn test_history_transition() {
        let mut nested_states = IndexMap::default();
        nested_states.insert("one".to_string(), StateDefinition {
             id: None, type_: None, initial: None, states: IndexMap::default(), on: IndexMap::default(), entry: vec![], exit: vec![], tags: vec![], history: None
        });
        nested_states.insert("two".to_string(), StateDefinition {
             id: None, type_: None, initial: None, states: IndexMap::default(), on: IndexMap::default(), entry: vec![], exit: vec![], tags: vec![], history: None
        });
        nested_states.insert("hist".to_string(), StateDefinition {
             id: None, type_: Some("history".to_string()), initial: None, states: IndexMap::default(), on: IndexMap::default(), entry: vec![], exit: vec![], tags: vec![], history: None
        });

        let mut a_on = IndexMap::default();
        a_on.insert("EXIT".to_string(), vec![TransitionDefinition {
            target: Some(vec!["b".to_string()]),
            cond: None,
            actions: vec![],
        }]);

        let mut states = IndexMap::default();
        states.insert("a".to_string(), StateDefinition {
            id: None, type_: None, initial: Some("one".to_string()), states: nested_states, on: a_on, entry: vec![], exit: vec![], tags: vec![], history: None
        });
        
        let mut b_on = IndexMap::default();
        b_on.insert("REENTER".to_string(), vec![TransitionDefinition {
            target: Some(vec!["a.hist".to_string()]),
            cond: None,
            actions: vec![],
        }]);
        states.insert("b".to_string(), StateDefinition {
            id: None, type_: None, initial: None, states: IndexMap::default(), on: b_on, entry: vec![], exit: vec![], tags: vec![], history: None
        });

        let definition = MachineDefinition {
            id: "m".to_string(),
            initial: "a".to_string(),
            states,
            on: IndexMap::default(),
            entry: vec![],
            exit: vec![],
            context: None,
            types: None,
            id_map: IndexMap::default(),
        };

        let mut machine = RuntimeMachine::new(definition);
        assert_eq!(machine.state(), "a.one");

        machine.transition("EXIT", &Value::Null, &IndexMap::default(), &IndexMap::default());
        assert_eq!(machine.state(), "b");

        machine.transition("REENTER", &Value::Null, &IndexMap::default(), &IndexMap::default());
        // Default history for 'a' was 'one' (initial state)
        assert_eq!(machine.state(), "a.one");
    }
}
