#![no_std]
extern crate alloc;

use indexmap::IndexMap;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;


// --- Domain Model ---

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(IndexMap<String, Value>),
}

impl Value {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Number(n) => Some(*n as i64),
            _ => None,
        }
    }
    
    pub fn as_f64(&self) -> Option<f64> {
         match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&IndexMap<String, Value>> {
        match self {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut IndexMap<String, Value>> {
        match self {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }
    
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object().and_then(|obj| obj.get(key))
    }
}

// Convert common types to Value (helpers)
impl From<bool> for Value {
    fn from(v: bool) -> Self { Value::Bool(v) }
}
impl From<String> for Value {
    fn from(v: String) -> Self { Value::String(v) }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self { Value::String(v.to_string()) }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self { Value::Number(v as f64) }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self { Value::Number(v) }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self { Value::Number(v as f64) }
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
}

#[derive(Debug, Clone)]
pub enum Action {
    Named(String),
    Assign(AssignAction),
    Generic(GenericAction),
}

#[derive(Debug, Clone)]
pub struct AssignAction {
    pub action_type: String, // Expect "xstate.assign"
    pub assignment: Value,
}

#[derive(Debug, Clone)]
pub struct GenericAction {
    pub action_type: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct GuardObject {
    pub guard_type: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone)]
pub enum TransitionGuard {
    Name(String),
    Object(GuardObject),
}

#[derive(Debug, Clone)]
pub struct TransitionDefinition {
    pub target: Option<String>,
    pub cond: Option<TransitionGuard>,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone)]
pub struct StateDefinition {
    pub type_: Option<String>,
    pub initial: Option<String>,
    pub states: IndexMap<String, StateDefinition>,
    pub on: IndexMap<String, Vec<TransitionDefinition>>, 
    pub entry: Vec<Action>,
    pub exit: Vec<Action>,
}

// --- Runtime ---

#[derive(Debug, Clone)]
pub struct RuntimeMachine {
    pub definition: MachineDefinition,
    pub current_state: String,
    pub context: Value,
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
        }
    }

    pub fn resolve_initial_state(states: &IndexMap<String, StateDefinition>, start_key: &str) -> String {
        let mut current_key = start_key.to_string();
        
        loop {
            if let Some(def) = Self::get_state_def_recursive(states, &current_key) {
                 if let Some(ref init) = def.initial {
                     current_key = format!("{}.{}", current_key, init);
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
        root_states: &'a IndexMap<String, StateDefinition>, 
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
        event_payload: &Value,
        guards: &IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
        actions: &IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>)>>,
    ) -> Option<String> {
        let mut state_path = self.current_state.clone();
        
        loop {
            // Determine transitions based on current path (Root or State)
            let transitions = if state_path.is_empty() {
                self.definition.on.get(event)
            } else {
                Self::get_state_def_recursive(&self.definition.states, &state_path)
                    .and_then(|def| def.on.get(event))
            };

            if let Some(transitions) = transitions {
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
                        let next_state_full_path = if transition.target.is_none() {
                            state_path.clone() // Internal / stay
                        } else {
                            // Resolve target path
                            if self.definition.states.contains_key(&target_base) {
                                Self::resolve_initial_state(&self.definition.states, &target_base)
                            } else {
                                if let Some(parent_dot) = state_path.rfind('.') {
                                    let parent_path = &state_path[..parent_dot];
                                    let sibling_path = format!("{}.{}", parent_path, target_base);
                                    if Self::get_state_def_recursive(&self.definition.states, &sibling_path).is_some() {
                                        Self::resolve_initial_state(&self.definition.states, &sibling_path)
                                    } else {
                                        target_base
                                    }
                                } else {
                                    target_base
                                }
                            }
                        };
                        
                        let is_internal = transition.target.is_none(); 

                        if !is_internal {
                            if let Some(current_state_def) = Self::get_state_def_recursive(&self.definition.states, &self.current_state) {
                                for action in &current_state_def.exit {
                                    Self::execute_action(&mut self.context, action, event_payload, actions);
                                }
                            }
                        }

                        for action in &transition.actions {
                            Self::execute_action(&mut self.context, action, event_payload, actions);
                        }

                        if !is_internal {
                            self.current_state = next_state_full_path.clone();

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
            
            // Bubble up
            if let Some(dot_idx) = state_path.rfind('.') {
                state_path = state_path[..dot_idx].to_string();
            } else if !state_path.is_empty() {
                state_path = "".to_string(); // Try root
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
