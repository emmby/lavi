#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::vec;
use alloc::format;
use indexmap::IndexMap;
use alloc::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum StateValue {
    Atomic(String),
    Compound(String, Box<StateValue>),
    Parallel(IndexMap<String, StateValue>),
}

impl StateValue {
    /// Helper to get all active leaf paths (dot-separated)
    pub fn get_active_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        self.collect_paths("", &mut paths);
        paths
    }

    fn collect_paths(&self, parent: &str, paths: &mut Vec<String>) {
        match self {
            StateValue::Atomic(name) => {
                let full = if parent.is_empty() { name.clone() } else { format!("{}.{}", parent, name) };
                paths.push(full);
            }
            StateValue::Compound(name, child) => {
                let current = if parent.is_empty() { name.clone() } else { format!("{}.{}", parent, name) };
                child.collect_paths(&current, paths);
            }
            StateValue::Parallel(regions) => {
                for (_region_name, region_value) in regions {
                     region_value.collect_paths(parent, paths);
                }
            }
        }
    }
}

pub type ActionFunction = Rc<dyn Fn(&mut Value, &Value, &Option<Value>)>;
pub type GuardFunction = Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>;

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

    pub fn as_string(&self) -> Option<&String> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
    
    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(arr) => Some(arr),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StateType {
    Atomic,
    Compound,
    Parallel,
    Final,
    History { deep: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateDefinition {
    pub id: Option<String>,
    pub type_: Option<String>, // "parallel", "compound", etc. (legacy string)
    pub state_type: StateType, // Enum representation
    pub initial: Option<String>,
    pub initial_actions: Vec<Action>,
    pub states: IndexMap<String, StateDefinition>,
    pub on: IndexMap<String, Vec<TransitionDefinition>>,
    pub entry: Vec<Action>,
    pub exit: Vec<Action>,
    pub history: Option<HistoryType>, // If this state IS a history state
    pub history_target: Option<String>, // Default target for history
    pub tags: Vec<String>,
    pub invoke: Vec<InvokeDefinition>,
    pub history_actions: Vec<Action>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InvokeDefinition {
    pub src: Value,
    pub id: Option<String>,
}

impl StateDefinition {
    pub fn default_atomic() -> Self {
        Self {
            id: None, type_: None, state_type: StateType::Atomic, initial: None, 
            initial_actions: vec![],
            states: IndexMap::new(), on: IndexMap::new(), entry: vec![], exit: vec![], 
            history: None, history_target: None, tags: vec![], invoke: vec![], history_actions: vec![]
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HistoryType {
    Shallow,
    Deep,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransitionDefinition {
    pub target: Option<Vec<String>>,
    pub cond: Option<TransitionGuard>,
    pub actions: Vec<Action>,
    pub reenter: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransitionGuard {
    Name(String),
    Object(GuardObject),
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuardObject {
    pub guard_type: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Named(String),
    Assign(AssignAction),
    Generic(GenericAction),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssignAction {
    pub action_type: String, // "xstate.assign"
    pub assignment: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenericAction {
    pub action_type: String,
    pub params: Option<Value>,
}

#[derive(Clone)]
pub struct MachineDefinition {
    pub id: String,
    pub type_: Option<String>,
    pub state_type: StateType,
    pub initial: String, pub initial_actions: Vec<Action>,
    pub states: IndexMap<String, StateDefinition>,
    pub on: IndexMap<String, Vec<TransitionDefinition>>,
    pub entry: Vec<Action>,
    pub exit: Vec<Action>,
    pub invoke: Vec<InvokeDefinition>,
    pub context: Option<Value>,
    pub types: Option<Value>,
    pub id_map: IndexMap<String, String>,
}

#[derive(Clone)]
pub struct RuntimeMachine {
    pub definition: MachineDefinition,
    pub current_state: StateValue,
    pub context: Value,
    pub history_value: IndexMap<String, StateValue>,
}

impl RuntimeMachine {
    pub fn new(definition: &MachineDefinition) -> Result<Self, String> {
        let mut dummy = vec![];
        let initial_state = if matches!(definition.state_type, StateType::Parallel) {
             let mut regions = IndexMap::new();
             for (r_name, r_def) in &definition.states {
                 if matches!(r_def.state_type, StateType::History{..}) || r_def.history.is_some() { continue; }
                 let r_parts: Vec<&str> = vec![r_name];
                 if let Some(val) = Self::resolve_state_value_from_path(definition, &r_parts, 0, &mut dummy) {
                      regions.insert(r_name.clone(), val);
                 }
             }
             StateValue::Parallel(regions)
        } else if !definition.initial.is_empty() {
             Self::resolve_initial_state(definition, &definition.initial, &mut dummy)
                .ok_or_else(|| format!("Could not resolve initial state: {}", definition.initial))?
        } else {
             if definition.states.is_empty() {
                  StateValue::Parallel(IndexMap::new())
             } else {
                  StateValue::Atomic("".to_string())
             }
        };

        Ok(Self {
            definition: definition.clone(),
            current_state: initial_state,
            context: definition.context.clone().unwrap_or(Value::Object(IndexMap::new())),
            history_value: IndexMap::new(),
        })
    }

    pub fn state(&self) -> String {
        self.current_state.get_active_paths().first().cloned().unwrap_or_default()
    }

    pub fn get_state_definition(&self, path: &str) -> Option<&StateDefinition> {
        Self::get_state_def_recursive(&self.definition.states, path)
    }

    pub fn resolve_initial_state(definition: &MachineDefinition, initial: &str, collected_actions: &mut Vec<Action>) -> Option<StateValue> {
        let id_key = if initial.starts_with('#') { &initial[1..] } else { initial };
        let resolved = definition.id_map.get(id_key).map(|s| s.as_str()).unwrap_or(initial);
        let parts: Vec<&str> = resolved.split('.').collect();
        Self::resolve_state_value_from_path(definition, &parts, 0, collected_actions)
    }


    pub fn merge_state_value(old: &StateValue, new: &StateValue, current_path: &str, target_path: &str) -> StateValue {
        match (old, new) {
            (StateValue::Compound(old_key, old_val), StateValue::Compound(new_key, new_val)) => {
                if old_key == new_key {
                     let sub_path = if current_path.is_empty() { old_key.clone() } else { format!("{}.{}", current_path, old_key) };
                     let merged_val = Self::merge_state_value(old_val, new_val, &sub_path, target_path);
                     StateValue::Compound(new_key.clone(), Box::new(merged_val))
                } else {
                     new.clone()
                }
            },
            (StateValue::Parallel(old_map), StateValue::Parallel(new_map)) => {
                let mut merged_map = new_map.clone();
                for (key, new_sub_val) in new_map {
                    let sub_path = if current_path.is_empty() { key.clone() } else { format!("{}.{}", current_path, key) };
                    let is_targeted = if target_path.len() >= sub_path.len() {
                         target_path.starts_with(&sub_path) && (target_path.len() == sub_path.len() || target_path.chars().nth(sub_path.len()) == Some('.'))
                    } else {
                         true
                    };

                    if is_targeted {
                        if let Some(old_sub_val) = old_map.get(key) {
                            let merged_sub = Self::merge_state_value(old_sub_val, new_sub_val, current_path, target_path);
                            merged_map.insert(key.clone(), merged_sub);
                        }
                    } else {
                        if let Some(old_sub_val) = old_map.get(key) {
                            merged_map.insert(key.clone(), old_sub_val.clone());
                        }
                    }
                }
                StateValue::Parallel(merged_map)
            },
            (StateValue::Parallel(old_map), StateValue::Compound(new_key, _)) => {
                let mut merged_map = old_map.clone();
                if let Some(old_sub_val) = old_map.get(new_key) {
                     let sub_path = if current_path.is_empty() { new_key.clone() } else { format!("{}.{}", current_path, new_key) };
                     merged_map.insert(new_key.clone(), Self::merge_state_value(old_sub_val, new, current_path, target_path));
                }
                StateValue::Parallel(merged_map)
            },
            (StateValue::Parallel(old_map), StateValue::Atomic(new_key)) => {
                let mut merged_map = old_map.clone();
                if old_map.contains_key(new_key) {
                     merged_map.insert(new_key.clone(), new.clone());
                }
                StateValue::Parallel(merged_map)
            },
            _ => new.clone()
        }
    }

    pub fn resolve_target(&self, source_path: &str, target: &str) -> Option<String> {
        // 1. Check ID map
        let id_key = if target.starts_with('#') { &target[1..] } else { target };
        if let Some(path) = self.definition.id_map.get(id_key) {
            return Some(path.clone());
        }

        // 2. Resolve relative to source (ancestors)
        let mut parts: Vec<&str> = source_path.split('.').collect();
        while !parts.is_empty() {
             // Try prefix + target
             let prefix = parts.join(".");
             let candidate = if target.starts_with('.') {
                 format!("{}{}", prefix, target)
             } else if prefix.is_empty() {
                 target.to_string()
             } else {
                 format!("{}.{}", prefix, target)
             };
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
    
    // FIX APPLIED HERE: Prioritize Path over Initial
                            fn resolve_state_value_from_path(
        definition: &MachineDefinition,
        parts: &[&str],
        depth: usize,
        collected_actions: &mut Vec<Action>
    ) -> Option<StateValue> {
        if depth >= parts.len() { return None; }
        let current_path = parts[0..=depth].join(".");
        let current_name = parts[depth];
        let def = Self::get_state_def_recursive(&definition.states, &current_path)?;
        
        if depth == parts.len() - 1 {
             match def.state_type {
                 StateType::Atomic | StateType::Final => Some(StateValue::Atomic(current_name.to_string())),
                 StateType::History{..} => {
                      if let Some(default_target) = &def.history_target {
                           let id_key = if default_target.starts_with('#') { &default_target[1..] } else { default_target };
                           if let Some(res) = definition.id_map.get(id_key) {
                                let t_parts: Vec<&str> = res.split('.').collect();
                                collected_actions.extend(def.history_actions.clone());
                                return Self::resolve_state_value_from_path(definition, &t_parts, 0, collected_actions);
                           } else {
                                 // Try relative resolution
                                 if let Some(idx) = current_path.rfind('.') {
                                     let parent_path = &current_path[..idx];
                                     let full_target = format!("{}.{}", parent_path, default_target);
                                     let t_parts: Vec<&str> = full_target.split('.').collect();
                                     if let Some(res) = Self::resolve_state_value_from_path(definition, &t_parts, depth, collected_actions) { // Use depth to retry relative? No, default targets are usually absolute or relative to parent. 
                                         collected_actions.extend(def.history_actions.clone());
                                         return Some(res);
                                     }
                                 }
                                 let t_parts: Vec<&str> = default_target.split('.').collect();
                                 collected_actions.extend(def.history_actions.clone());
                                 return Self::resolve_state_value_from_path(definition, &t_parts, 0, collected_actions);
                            }
                      } else {
                           if let Some(idx) = current_path.rfind('.') {
                               let parent_path = &current_path[..idx];
                               if let Some(parent_def) = Self::get_state_def_recursive(&definition.states, parent_path) {
                                   if let Some(parent_init) = &parent_def.initial {
                                        let full_init = format!("{}.{}", parent_path, parent_init);
                                        let fi_parts: Vec<&str> = full_init.split('.').collect();
                                         return Self::resolve_state_value_from_path(definition, &fi_parts, depth, collected_actions);
                                   }
                               }
                           }
                           Some(StateValue::Atomic(current_name.to_string()))
                      }
                 },
                 StateType::Compound => {
                      if let Some(init) = &def.initial {
                           collected_actions.extend(def.initial_actions.clone());
                           let child_path = format!("{}.{}", current_path, init);
                           let child_parts: Vec<&str> = child_path.split('.').collect();
                            if let Some(val) = Self::resolve_state_value_from_path(definition, &child_parts, depth + 1, collected_actions) {
                               Some(StateValue::Compound(current_name.to_string(), Box::new(val)))
                           } else { None }
                      } else { None }
                 },
                 StateType::Parallel => {
                      let mut regions = IndexMap::new();
                      for (r_name, r_def) in &def.states {
                          if matches!(r_def.state_type, StateType::History{..}) || r_def.history.is_some() { continue; }
                          let r_path = format!("{}.{}", current_path, r_name);
                          let r_parts: Vec<&str> = r_path.split('.').collect();
                           if let Some(val) = Self::resolve_state_value_from_path(definition, &r_parts, depth + 1, collected_actions) {
                               regions.insert(r_name.clone(), val);
                          }
                      }
                      Some(StateValue::Compound(current_name.to_string(), Box::new(StateValue::Parallel(regions))))
                 }
             }
        } else {
             match def.state_type {
                 StateType::Compound => {
                       if let Some(val) = Self::resolve_state_value_from_path(definition, parts, depth + 1, collected_actions) {
                          return Some(StateValue::Compound(current_name.to_string(), Box::new(val)));
                      }
                      Some(StateValue::Atomic(current_name.to_string()))
                 },
                 StateType::Parallel => {
                      let mut regions = IndexMap::new();
                      let next_part = parts[depth+1];
                      for (r_name, r_def) in &def.states {
                          if matches!(r_def.state_type, StateType::History{..}) || r_def.history.is_some() { continue; }
                          if r_name == next_part {
                               if let Some(val) = Self::resolve_state_value_from_path(definition, parts, depth + 1, collected_actions) {
                                    regions.insert(r_name.clone(), val);
                               }
                          } else {
                               let r_path = format!("{}.{}", current_path, r_name);
                               let r_parts: Vec<&str> = r_path.split('.').collect();
                               if let Some(val) = Self::resolve_state_value_from_path(definition, &r_parts, depth + 1, collected_actions) {
                                    regions.insert(r_name.clone(), val);
                               }
                          }
                      }
                      Some(StateValue::Compound(current_name.to_string(), Box::new(StateValue::Parallel(regions))))
                 },
                 _ => None
             }
        }
    }







    pub fn get_state_def_recursive<'a>(states: &'a IndexMap<String, StateDefinition>, path: &str) -> Option<&'a StateDefinition> {
        if path.is_empty() { return None; }
        let parts: Vec<&str> = path.split('.').collect();
        let mut mp = states;
        for (i, p) in parts.iter().enumerate() {
            if let Some(d) = mp.get(*p) {
                if i == parts.len() - 1 { return Some(d); }
                mp = &d.states;
            } else {
                return None;
            }
        }
        None
    }
    
    pub fn execute_action(ctx: &mut Value, action: &Action, event: &Value, actions_map: &IndexMap<String, ActionFunction>) {
        match action {
            Action::Named(name) => {
                if let Some(f) = actions_map.get(name) {
                    f(ctx, event, &None);
                }
            }
            Action::Assign(assign) => {
                if let Value::Object(new_vals) = &assign.assignment {
                    if let Value::Object(ctx_map) = ctx {
                        for (k, v) in new_vals {
                            ctx_map.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            Action::Generic(gen) => {
                if let Some(f) = actions_map.get(&gen.action_type) {
                    f(ctx, event, &gen.params);
                }
            }
        }
    }
    
    // SEARCH PATH FIX, PER-LEAF FIX
    pub fn transition(
        &mut self,
        event_type: &str,
        event_payload: &Value,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) -> Option<StateValue> {
        let active_leaves = self.current_state.get_active_paths();
        
        let mut all_search_paths = Vec::new();
        for leaf in active_leaves {
            let mut parts: Vec<&str> = leaf.split('.').collect();
            while !parts.is_empty() {
                let p = parts.join(".");
                if !all_search_paths.contains(&p) { all_search_paths.push(p); }
                parts.pop();
            }
        }
        // FIX: Add Root
        if !all_search_paths.contains(&"".to_string()) { all_search_paths.push("".to_string()); }
        
        all_search_paths.sort_by(|a, b| b.matches('.').count().cmp(&a.matches('.').count()));

        for current_path in all_search_paths {
            let def_opt = if current_path.is_empty() {
                 // Root handling?
                 // definition.on
                 None 
            } else {
                 Self::get_state_def_recursive(&self.definition.states, &current_path)
            };
            
            let transitions_opt = if current_path.is_empty() {
                 self.definition.on.get(event_type)
            } else {
                 def_opt.and_then(|d| d.on.get(event_type))
            };
            
            if let Some(transitions) = transitions_opt {
                 for t in transitions {
                       // Guard check
                       if let Some(cond) = &t.cond {
                           let is_allowed = match cond {
                               TransitionGuard::Name(n) => guards.get(n).map(|f| f(&self.context, event_payload, &None)).unwrap_or(true),
                               TransitionGuard::Object(o) => guards.get(&o.guard_type).map(|f| f(&self.context, event_payload, &o.params)).unwrap_or(true),
                           };
                           if !is_allowed { continue; }
                       }

                       let mut accumulated_state = self.current_state.clone();
                       if let Some(targets) = &t.target {
                            let mut collected_actions = Vec::new();
                            for raw in targets {
                                let current_search_path_for_target = if current_path.is_empty() { "".to_string() } else { current_path.clone() };
                                let target_full = self.resolve_target(&current_search_path_for_target, raw).unwrap_or_else(|| raw.clone());
                                let target_def_opt = Self::get_state_def_recursive(&self.definition.states, &target_full);
                                let mut next_val_opt: Option<StateValue> = None;
                                if let Some(target_def) = target_def_opt {
                                    if target_def.type_.as_deref() == Some("history") || matches!(target_def.state_type, StateType::History{..}) {
                                        let history_parent = if let Some(idx) = target_full.rfind('.') { target_full[..idx].to_string() } else { "".to_string() };
                                        let is_deep = target_def.history == Some(HistoryType::Deep) || matches!(target_def.state_type, StateType::History{deep: true});
                                        let mut hist_opt = self.history_value.get(&history_parent).cloned();
                                        if hist_opt.is_none() {
                                             // Fallback: If parent is currently active, use current state as history
                                             // BUT ONLY if this is a re-entering transition or an external one.
                                             // If it's an internal transition (target is descendant of source), we shouldn't fallback
                                             // to current config if history is empty; we should use initial state.
                                             
                                             let active_paths = self.current_state.get_active_paths();
                                             let parent_is_active = active_paths.iter().any(|p| p == &history_parent || p.starts_with(&format!("{}.", history_parent)));
                                             
                                             if parent_is_active {
                                                 let mut is_internal = false;
                                                 if !t.reenter {
                                                      if target_full.starts_with(&format!("{}.", current_path)) || target_full == current_path {
                                                           is_internal = true;
                                                      }
                                                 }
                                                 if !is_internal {
                                                      hist_opt = Some(self.current_state.clone());
                                                 }
                                             }
                                        }

                                        if let Some(hist) = hist_opt {
                                            if is_deep { next_val_opt = Some(hist.clone()); }
                                            else {
                                                 let active_hist_paths = hist.get_active_paths();
                                                 let parent_len = if history_parent.is_empty() { 0 } else { history_parent.len() + 1 };
                                                 if !active_hist_paths.is_empty() {
                                                     let matching_path = active_hist_paths.iter().find(|p| { if history_parent.is_empty() { true } else { p.starts_with(&history_parent) && (p.len() == history_parent.len() || p.chars().nth(history_parent.len()) == Some('.')) } });
                                                     if let Some(first) = matching_path { if first.len() > parent_len { let remainder = &first[parent_len..]; let component = if let Some(idx) = remainder.find('.') { &remainder[..idx] } else { remainder }; let shallow_target = if history_parent.is_empty() { component.to_string() } else { format!("{}.{}", history_parent, component) }; next_val_opt = Self::resolve_initial_state(&self.definition, &shallow_target, &mut collected_actions); } }
                                                 }
                                            }
                                        }
                                        if next_val_opt.is_none() {
                                             if let Some(default_t) = &target_def.history_target { 
                                                 let resolved = self.resolve_target(&history_parent, default_t).unwrap_or(default_t.clone()); 
                                                 next_val_opt = Self::resolve_initial_state(&self.definition, &resolved, &mut collected_actions); 
                                             } 
                                             else { next_val_opt = Self::resolve_initial_state(&self.definition, &history_parent, &mut collected_actions); }
                                        }
                                    }
                                }
                                let next_val = next_val_opt.or_else(|| Self::resolve_initial_state(&self.definition, &target_full, &mut collected_actions)).unwrap_or(StateValue::Atomic(target_full.clone()));
                                accumulated_state = Self::merge_state_value(&accumulated_state, &next_val, "", &target_full);
                            }

                            let finalized_state = accumulated_state;
                            
                            // Execute collected history/initial actions -> BUT wait, we are in a loop for targets.
                            // We should execute them? 
                            // Yes, if we are entering states, we should execute them.
                            for action in &collected_actions {
                                Self::execute_action(&mut self.context, action, event_payload, actions);
                            }

                            let current_leaves = self.current_state.get_active_paths();
                            let next_leaves = finalized_state.get_active_paths();
                            
                            let current_parts: Vec<&str> = current_path.split('.').filter(|s| !s.is_empty()).collect();
                            let mut global_max_lca_limit = current_parts.len();
                            if let Some(targets) = &t.target {
                                let mut lowest_domain_depth = current_parts.len();
                                for raw in targets {
                                     let current_search_path = if current_path.is_empty() { "".to_string() } else { current_path.clone() };
                                     let target_full = self.resolve_target(&current_search_path, raw).unwrap_or_else(|| raw.clone());
                                     let target_parts: Vec<&str> = target_full.split('.').filter(|s| !s.is_empty()).collect();
                                     let mut dom_lca = 0;
                                     while dom_lca < current_parts.len() && dom_lca < target_parts.len() && current_parts[dom_lca] == target_parts[dom_lca] { dom_lca += 1; }
                                     
                                     let mut domain_depth = dom_lca;
                                     if dom_lca == current_parts.len() && dom_lca == target_parts.len() {
                                          if t.reenter { domain_depth = dom_lca.saturating_sub(1); }
                                     } else if dom_lca == target_parts.len() {
                                          domain_depth = dom_lca.saturating_sub(1);
                                     } else if dom_lca == current_parts.len() {
                                          if t.reenter { domain_depth = dom_lca.saturating_sub(1); }
                                     }
                                     if domain_depth < lowest_domain_depth { lowest_domain_depth = domain_depth; }
                                }
                                global_max_lca_limit = lowest_domain_depth;
                            } else {
                                if t.reenter { global_max_lca_limit = current_parts.len().saturating_sub(1); }
                            }
                            
                            let mut exited_paths = Vec::new();
                            for leaf in current_leaves.iter().rev() {
                                 let leaf_parts: Vec<&str> = leaf.split('.').filter(|s| !s.is_empty()).collect();
                                 let mut max_lca = 0;
                                 for next_leaf in &next_leaves {
                                      let next_parts: Vec<&str> = next_leaf.split('.').filter(|s| !s.is_empty()).collect();
                                      let mut lca = 0;
                                      while lca < leaf_parts.len() && lca < next_parts.len() && leaf_parts[lca] == next_parts[lca] { lca += 1; }
                                      
                                      let mut is_in_domain = true;
                                      for (idx, p) in current_parts.iter().enumerate() {
                                          if idx >= leaf_parts.len() || leaf_parts[idx] != *p { is_in_domain = false; break; }
                                      }
                                      if is_in_domain && lca > global_max_lca_limit { lca = global_max_lca_limit; }
                                      if lca > max_lca { max_lca = lca; }
                                 }
                                 for i in ((max_lca + 1)..=leaf_parts.len()).rev() {
                                      let path = leaf_parts[0..i].join(".");
                                      if !exited_paths.contains(&path) {
                                          self.history_value.insert(path.clone(), self.current_state.clone());
                                          if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &path) {
                                               for action in &def.exit { Self::execute_action(&mut self.context, action, event_payload, actions); }
                                               for inv in &def.invoke {
                                                    if let Some(src) = inv.src.as_string() {
                                                         Self::execute_action(&mut self.context, &Action::Named(src.to_string()), event_payload, actions);
                                                    }
                                               }
                                          }
                                          exited_paths.push(path);
                                      }
                                 }
                            }
                            for action in &t.actions { Self::execute_action(&mut self.context, action, event_payload, actions); }
                            let mut entered_paths = Vec::new();
                            for leaf in &next_leaves {
                                 let leaf_parts: Vec<&str> = leaf.split('.').filter(|s| !s.is_empty()).collect();
                                 let mut max_lca = 0;
                                 for curr_leaf in current_leaves.iter().rev() {
                                      let curr_parts: Vec<&str> = curr_leaf.split('.').filter(|s| !s.is_empty()).collect();
                                      let mut lca = 0;
                                      while lca < leaf_parts.len() && lca < curr_parts.len() && leaf_parts[lca] == curr_parts[lca] { lca += 1; }
                                      
                                      let mut is_in_domain = true;
                                      for (idx, p) in current_parts.iter().enumerate() {
                                          if idx >= leaf_parts.len() || leaf_parts[idx] != *p { is_in_domain = false; break; }
                                      }
                                      if is_in_domain && lca > global_max_lca_limit { lca = global_max_lca_limit; }
                                      if lca > max_lca { max_lca = lca; }
                                 }
                                 for i in (max_lca + 1)..=leaf_parts.len() {
                                      let path = leaf_parts[0..i].join(".");
                                      if !entered_paths.contains(&path) {
                                          if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &path) {
                                               for action in &def.entry { Self::execute_action(&mut self.context, action, event_payload, actions); }
                                               for inv in &def.invoke {
                                                    if let Some(src) = inv.src.as_string() {
                                                         Self::execute_action(&mut self.context, &Action::Named(src.to_string()), event_payload, actions);
                                                    }
                                               }
                                                
                                                if i < leaf_parts.len() {
                                                    // Removed implicit initial action execution logic
                                                }
                                          }
                                          entered_paths.push(path);
                                      }
                                 }
                            }
                            let _changed = finalized_state != self.current_state;
                            self.current_state = finalized_state;
                            if !event_type.is_empty() || _changed {
                                if let Some(next) = self.transition("", event_payload, guards, actions) {
                                    return Some(next);
                                }
                            }
                            return Some(self.current_state.clone());
                       } else if !t.actions.is_empty() {
                            // Internal transition (no target, but has actions)
                            for action in &t.actions { Self::execute_action(&mut self.context, action, event_payload, actions); }
                            if !event_type.is_empty() {
                                if let Some(next) = self.transition("", event_payload, guards, actions) { return Some(next); }
                            }
                            return Some(self.current_state.clone());
                       }
                 }
            }
        }
        None
    }
}

#[cfg(test)]
mod repro_test;
