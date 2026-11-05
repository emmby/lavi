#![no_std]

extern crate alloc;

pub mod json;
pub use json::JsonError;

pub mod parser;
pub use parser::{assign, create_machine, from_json_str, setup, value_to_state_value, SetupObject};

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
                if name.is_empty() {
                    if !parent.is_empty() {
                        paths.push(parent.to_string());
                    }
                } else {
                    let full = if parent.is_empty() {
                        name.clone()
                    } else {
                        format!("{}.{}", parent, name)
                    };
                    paths.push(full);
                }
            }
            StateValue::Compound(name, child) => {
                let current = if parent.is_empty() {
                    name.clone()
                } else {
                    format!("{}.{}", parent, name)
                };
                child.collect_paths(&current, paths);
            }
            StateValue::Parallel(regions) => {
                if regions.is_empty() {
                    if !parent.is_empty() {
                        paths.push(parent.to_string());
                    }
                } else {
                    for (_region_name, region_value) in regions {
                        region_value.collect_paths(parent, paths);
                    }
                }
            }
        }
    }

    /// Returns true if the given dot-delimited state path (e.g. `"green"`, `"Display"`,
    /// `"Display.Theme"`, or `"Display.Power.On"`) is currently active.
    pub fn matches(&self, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }
        match self {
            StateValue::Atomic(name) => !name.is_empty() && path == name,
            StateValue::Compound(name, child) => {
                if path == name {
                    true
                } else if path.starts_with(name.as_str())
                    && path.as_bytes().get(name.len()) == Some(&b'.')
                {
                    child.matches(&path[name.len() + 1..])
                } else {
                    false
                }
            }
            StateValue::Parallel(regions) => regions
                .values()
                .any(|region_value| region_value.matches(path)),
        }
    }

    /// Returns true if all active paths in `partial` are active in `self`.
    pub fn matches_state_value(&self, partial: &StateValue) -> bool {
        if matches!(partial, StateValue::Parallel(regions) if regions.is_empty()) {
            return !matches!(self, StateValue::Atomic(_));
        }
        let paths = partial.get_active_paths();
        !paths.is_empty() && paths.iter().all(|p| self.matches(p))
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

    pub fn into_string(mut self) -> Option<String> {
        match &mut self {
            Value::String(s) => Some(core::mem::take(s)),
            _ => None,
        }
    }

    pub fn into_object(mut self) -> Option<IndexMap<String, Value>> {
        match &mut self {
            Value::Object(map) => Some(core::mem::take(map)),
            _ => None,
        }
    }

    pub fn into_array(mut self) -> Option<Vec<Value>> {
        match &mut self {
            Value::Array(arr) => Some(core::mem::take(arr)),
            _ => None,
        }
    }
}

fn push_value_children(val: &mut Value, stack: &mut Vec<Value>) {
    match val {
        Value::Array(arr) => {
            for item in core::mem::take(arr) {
                match item {
                    Value::Array(ref a) if !a.is_empty() => stack.push(item),
                    Value::Object(ref o) if !o.is_empty() => stack.push(item),
                    _ => {}
                }
            }
        }
        Value::Object(map) => {
            for (_k, item) in core::mem::take(map) {
                match item {
                    Value::Array(ref a) if !a.is_empty() => stack.push(item),
                    Value::Object(ref o) if !o.is_empty() => stack.push(item),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

impl Drop for Value {
    fn drop(&mut self) {
        let has_children = match self {
            Value::Array(arr) => !arr.is_empty(),
            Value::Object(map) => !map.is_empty(),
            _ => false,
        };

        if !has_children {
            return;
        }

        let mut stack: Vec<Value> = Vec::new();
        push_value_children(self, &mut stack);

        while let Some(mut top) = stack.pop() {
            push_value_children(&mut top, &mut stack);
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
    pub after: IndexMap<u64, Vec<TransitionDefinition>>,
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
            states: IndexMap::new(), on: IndexMap::new(), after: IndexMap::new(), entry: vec![], exit: vec![], 
            history: None, history_target: None, tags: vec![], invoke: vec![], history_actions: vec![]
        }
    }
}

impl Drop for StateDefinition {
    fn drop(&mut self) {
        if self.states.is_empty() {
            return;
        }

        let mut stack: Vec<StateDefinition> = Vec::new();
        for (_k, child) in core::mem::take(&mut self.states) {
            if !child.states.is_empty() {
                stack.push(child);
            }
        }

        while let Some(mut top) = stack.pop() {
            for (_k, child) in core::mem::take(&mut top.states) {
                if !child.states.is_empty() {
                    stack.push(child);
                }
            }
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
    Raise(Value),
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
    pub after: IndexMap<u64, Vec<TransitionDefinition>>,
    pub entry: Vec<Action>,
    pub exit: Vec<Action>,
    pub invoke: Vec<InvokeDefinition>,
    pub context: Option<Value>,
    pub types: Option<Value>,
    pub id_map: IndexMap<String, String>,
}

impl MachineDefinition {
    pub fn from_value(val: Value) -> Result<Self, JsonError> {
        parser::parse_machine_definition(val)
    }
}

#[derive(Clone)]
pub struct RuntimeMachine {
    pub definition: MachineDefinition,
    pub current_state: StateValue,
    pub context: Value,
    pub history_value: IndexMap<String, StateValue>,
    pub initial_actions: Vec<Action>,
    pub is_started: bool,
    pub active_timers: Vec<(String, u64, Vec<TransitionDefinition>)>,
    pub clock_millis_fn: Option<alloc::rc::Rc<dyn Fn() -> u64>>,
    pub is_processing: bool,
    pub external_queue: alloc::collections::VecDeque<(String, Value)>,
    pub internal_queue: alloc::collections::VecDeque<Value>,
}

impl RuntimeMachine {
    pub fn from_definition(mut definition: MachineDefinition) -> Result<Self, String> {
        let mut dummy = definition.initial_actions.clone();
        let initial_state = if matches!(definition.state_type, StateType::Parallel) {
             let mut regions = IndexMap::new();
             for (r_name, r_def) in &definition.states {
                 if matches!(r_def.state_type, StateType::History{..}) || r_def.history.is_some() { continue; }
                 let r_parts: Vec<&str> = vec![r_name];
                 if let Some(val) = Self::resolve_state_value_from_path(&definition, &r_parts, 0, &mut dummy) {
                      regions.insert(r_name.clone(), val);
                 }
             }
             StateValue::Parallel(regions)
        } else if !definition.initial.is_empty() {
             Self::resolve_initial_state(&definition, &definition.initial, &mut dummy)
                .ok_or_else(|| format!("Could not resolve initial state: {}", definition.initial))?
        } else if definition.states.is_empty() {
             StateValue::Parallel(IndexMap::new())
        } else {
             StateValue::Atomic("".to_string())
        };

        let context = definition.context.take().unwrap_or_else(|| Value::Object(IndexMap::new()));

        Ok(Self {
            definition,
            current_state: initial_state,
            context,
            history_value: IndexMap::new(),
            initial_actions: dummy,
            is_started: false,
            active_timers: Vec::new(),
            clock_millis_fn: None,
            is_processing: false,
            external_queue: alloc::collections::VecDeque::new(),
            internal_queue: alloc::collections::VecDeque::new(),
        })
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let def = crate::from_json_str(json).map_err(|e| e.to_string())?;
        Self::from_definition(def)
    }

    pub fn new(definition: &MachineDefinition) -> Result<Self, String> {
        Self::from_definition(definition.clone())
    }

    pub fn register_clock<C: IntoClockFn>(&mut self, clock: C) {
        self.clock_millis_fn = Some(clock.into_clock_fn());
    }

    pub fn next_timer_deadline(&self) -> Option<u64> {
        if !self.is_started {
            return None;
        }
        self.active_timers
            .iter()
            .map(|(_, deadline, _)| *deadline)
            .min()
    }

    pub fn start(
        &mut self,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) {
        if self.is_started {
            return;
        }
        self.is_started = true;
        self.is_processing = true;

        let mut init_map = IndexMap::new();
        init_map.insert("type".to_string(), Value::String("xstate.init".to_string()));
        let init_event = Value::Object(init_map);

        let _ = core::mem::take(&mut self.initial_actions);
        let mut init_actions_to_run = self.definition.entry.clone();
        let now = self.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(0);
        for (delay, t_defs) in &self.definition.after {
            self.active_timers
                .push(("".to_string(), now + delay, t_defs.clone()));
        }
        init_actions_to_run.extend(self.definition.initial_actions.clone());
        let root_init_key = if self.definition.initial.starts_with('#') {
            &self.definition.initial[1..]
        } else {
            &self.definition.initial
        };
        let root_init_resolved = self
            .definition
            .id_map
            .get(root_init_key)
            .map(|s| s.as_str())
            .unwrap_or(&self.definition.initial);
        if let Some(root_init_def) =
            Self::get_state_def_recursive(&self.definition.states, root_init_resolved)
        {
            if matches!(root_init_def.state_type, StateType::History { .. }) {
                init_actions_to_run.extend(root_init_def.history_actions.clone());
            }
        }

        let mut seen_paths = alloc::vec::Vec::new();
        for path in self.current_state.get_active_paths() {
            let parts: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
            for i in 1..=parts.len() {
                let curr_path = parts[0..i].join(".");
                if !seen_paths.contains(&curr_path) {
                    if let Some(def) =
                        Self::get_state_def_recursive(&self.definition.states, &curr_path)
                    {
                        init_actions_to_run.extend(def.entry.clone());
                        for (delay, t_defs) in &def.after {
                            self.active_timers.push((
                                curr_path.clone(),
                                now + delay,
                                t_defs.clone(),
                            ));
                        }
                        if matches!(def.state_type, StateType::Compound) {
                            init_actions_to_run.extend(def.initial_actions.clone());
                            if let Some(child_init) = &def.initial {
                                let child_path = format!("{}.{}", curr_path, child_init);
                                if let Some(child_def) = Self::get_state_def_recursive(
                                    &self.definition.states,
                                    &child_path,
                                ) {
                                    if matches!(child_def.state_type, StateType::History { .. }) {
                                        init_actions_to_run
                                            .extend(child_def.history_actions.clone());
                                    }
                                }
                            }
                        }
                    }
                    seen_paths.push(curr_path);
                }
            }
        }

        for action in &init_actions_to_run {
            Self::execute_action(
                &mut self.context,
                action,
                &init_event,
                actions,
                &mut self.internal_queue,
            );
        }

        // 1. Settle initial eventless (`always`) transitions first
        let _ = self.transition_internal("", &init_event, guards, actions, 0, None);

        // 2. Drain internal events raised during initial entry or initial `always` transitions
        let _ = self.drain_internal_queue(guards, actions);

        // 3. Complete initialization macrostep and drain any reentrant external events
        self.is_processing = false;
        let _ = self.drain_external_queue(guards, actions);
    }

    pub fn revive_timers(&mut self) {
        if !self.is_started { return; }
        self.active_timers.clear();
        let now = self.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(0);
        let active_leaves = self.current_state.get_active_paths();
        
        let mut paths_to_revive = alloc::vec::Vec::new();
        for leaf in &active_leaves {
            let mut parts: Vec<&str> = leaf.split('.').collect();
            while !parts.is_empty() {
                let p = parts.join(".");
                if !paths_to_revive.contains(&p) { paths_to_revive.push(p); }
                parts.pop();
            }
        }
        if !paths_to_revive.contains(&"".to_string()) {
            paths_to_revive.push("".to_string());
        }
        
        for path in paths_to_revive {
            let state_def_after = if path.is_empty() {
                Some(&self.definition.after)
            } else {
                Self::get_state_def_recursive(&self.definition.states, &path).map(|def| &def.after)
            };
            
            if let Some(after_map) = state_def_after {
                for (delay, t_defs) in after_map {
                    self.active_timers.push((path.clone(), now + delay, t_defs.clone()));
                }
            }
        }
    }

    pub fn matches(&self, path: &str) -> bool {
        self.current_state.matches(path)
    }

    pub fn get_state_definition(&self, path: &str) -> Option<&StateDefinition> {
        Self::get_state_def_recursive(&self.definition.states, path)
    }

    pub fn resolve_initial_state(definition: &MachineDefinition, initial: &str, collected_actions: &mut Vec<Action>) -> Option<StateValue> {
        let id_key = initial.strip_prefix('#').unwrap_or(initial);
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
                    
                    // Check if targeted
                    // 1. target_path starts with sub_path (Deep target inside this branch)
                    // 2. sub_path starts with target_path (Direct match or ancestor?)
                    //    If sub_path == target_path, we are targeting this branch directly. Use NEW.
                    
                    let is_targeted = if target_path.len() >= sub_path.len() {
                         target_path.starts_with(&sub_path) && (target_path.len() == sub_path.len() || target_path.chars().nth(sub_path.len()) == Some('.'))
                    } else {
                         // target_path < sub_path. e.g. target "on", sub "on.A".
                         true
                    };

                    if is_targeted {
                        if let Some(old_sub_val) = old_map.get(key) {
                            let merged_sub = Self::merge_state_value(old_sub_val, new_sub_val, current_path, target_path);
                            merged_map.insert(key.clone(), merged_sub);
                        }
                    } else {
                        // Not targeted. Preserve OLD if exists.
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
                     let _sub_path = if current_path.is_empty() { new_key.clone() } else { format!("{}.{}", current_path, new_key) };
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
        let id_key = target.strip_prefix('#').unwrap_or(target);
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
                           let id_key = default_target.strip_prefix('#').unwrap_or(default_target);
                           if let Some(res) = definition.id_map.get(id_key) {
                                let t_parts: Vec<&str> = res.split('.').collect();
                                collected_actions.extend(def.history_actions.clone());
                                Self::resolve_state_value_from_path(definition, &t_parts, 0, collected_actions)
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
                                 Self::resolve_state_value_from_path(definition, &t_parts, 0, collected_actions)
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
                      let mut compound_names: Vec<String> = vec![current_name.to_string()];
                      let mut curr_def = def;
                      collected_actions.extend(curr_def.initial_actions.clone());

                      while let Some(init) = &curr_def.initial {
                          if let Some(next_def) = curr_def.states.get(init) {
                              if matches!(next_def.state_type, StateType::Compound) {
                                  compound_names.push(init.clone());
                                  collected_actions.extend(next_def.initial_actions.clone());
                                  curr_def = next_def;
                              } else if matches!(next_def.state_type, StateType::Atomic | StateType::Final) {
                                  let mut res = StateValue::Atomic(init.clone());
                                  for name in compound_names.into_iter().rev() {
                                      res = StateValue::Compound(name, Box::new(res));
                                  }
                                  return Some(res);
                              } else {
                                  let full_child_path = if compound_names.len() == 1 {
                                      format!("{}.{}", current_path, init)
                                  } else {
                                      format!("{}.{}.{}", current_path, compound_names[1..].join("."), init)
                                  };
                                  let child_parts: Vec<&str> = full_child_path.split('.').collect();
                                  let leaf_val = Self::resolve_state_value_from_path(definition, &child_parts, child_parts.len() - 1, collected_actions)?;
                                  let mut res = leaf_val;
                                  for name in compound_names.into_iter().rev() {
                                      res = StateValue::Compound(name, Box::new(res));
                                  }
                                  return Some(res);
                              }
                          } else {
                              let base_path = parts[0..=depth].join(".");
                              let child_path = format!("{}.{}", base_path, init);
                              let child_parts: Vec<&str> = child_path.split('.').collect();
                              let val = Self::resolve_state_value_from_path(definition, &child_parts, depth + 1, collected_actions)?;
                              let mut res = val;
                              for name in compound_names.into_iter().rev() {
                                  res = StateValue::Compound(name, Box::new(res));
                              }
                              return Some(res);
                          }
                      }
                      None
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
    
    pub fn execute_action(ctx: &mut Value, action: &Action, event: &Value, actions_map: &IndexMap<String, ActionFunction>, internal_queue: &mut alloc::collections::VecDeque<Value>) {
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
            Action::Raise(payload) => {
                internal_queue.push_back(payload.clone());
            }
        }
    }
    
    // SEARCH PATH FIX, PER-LEAF FIX
    pub fn tick(
        &mut self,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) -> Option<StateValue> {
        if !self.is_started || self.is_processing {
            return None;
        }
        self.is_processing = true;
        let now = self.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(0);

        let mut last_ret = None;
        let mut steps = 0usize;
        while steps < 100 {
            let expired = self
                .active_timers
                .iter()
                .enumerate()
                .filter(|(_, (_, deadline, _))| *deadline <= now)
                .min_by_key(|(_, (_, deadline, _))| *deadline)
                .map(|(idx, (path, _, t_defs))| (idx, path.clone(), t_defs.clone()));

            if let Some((idx, path, t_defs)) = expired {
                steps += 1;
                self.active_timers.remove(idx);

                let evt_type = if path.is_empty() {
                    "xstate.after".to_string()
                } else {
                    format!("xstate.after.{}", path)
                };
                let mut after_map = IndexMap::new();
                after_map.insert("type".to_string(), Value::String(evt_type));
                let after_event = Value::Object(after_map);

                let mut ret = self.transition_internal(
                    "",
                    &after_event,
                    guards,
                    actions,
                    0,
                    Some((&path, &t_defs)),
                );
                if let Some(drained) = self.drain_internal_queue(guards, actions) {
                    ret = Some(drained);
                }
                if let Some(ext_drained) = self.drain_reentrant_external_queue(guards, actions) {
                    ret = Some(ext_drained);
                }
                if let Some(final_ret) = ret {
                    last_ret = Some(final_ret);
                }
            } else {
                break;
            }
        }

        if let Some(drained) = self.drain_internal_queue(guards, actions) {
            last_ret = Some(drained);
        }
        if let Some(ext_drained) = self.drain_reentrant_external_queue(guards, actions) {
            last_ret = Some(ext_drained);
        }

        self.is_processing = false;
        last_ret
    }

    pub fn transition(
        &mut self,
        event_type: &str,
        event_payload: &Value,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) -> Option<StateValue> {
        self.external_queue
            .push_back((event_type.to_string(), event_payload.clone()));
        self.drain_external_queue(guards, actions)
    }

    fn drain_reentrant_external_queue(
        &mut self,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) -> Option<StateValue> {
        let mut last_ret = None;
        let mut loop_count = 0;

        while !self.external_queue.is_empty() {
            if loop_count > 100 {
                panic!("Maximum external queue iteration depth exceeded!");
            }
            loop_count += 1;

            let (e_type, e_payload) = self.external_queue.pop_front().unwrap();
            let mut ret = self.transition_internal(&e_type, &e_payload, guards, actions, 0, None);
            if let Some(drained) = self.drain_internal_queue(guards, actions) {
                ret = Some(drained);
            }
            if let Some(final_ret) = ret {
                last_ret = Some(final_ret);
            }
        }

        last_ret
    }

    fn drain_external_queue(
        &mut self,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) -> Option<StateValue> {
        if self.is_processing {
            return Some(self.current_state.clone());
        }

        self.is_processing = true;
        let last_ret = self.drain_reentrant_external_queue(guards, actions);
        self.is_processing = false;
        last_ret
    }
    
    fn drain_internal_queue(&mut self, guards: &IndexMap<String, GuardFunction>, actions: &IndexMap<String, ActionFunction>) -> Option<StateValue> {
        let mut last_valid_state = None;
        let mut iterations = 0;
        while let Some(raised) = self.internal_queue.pop_front() {
            iterations += 1;
            if iterations > 100 {
                // Prevent infinite loops from cyclic raised events
                break;
            }
            let (e_type, e_payload) = match &raised {
                Value::String(s) => (s.as_str(), &Value::Null),
                Value::Object(map) => {
                    let t = map.get("type").and_then(|v| v.as_string()).map(|s| s.as_str()).unwrap_or("");
                    (t, &raised)
                },
                _ => ("", &Value::Null)
            };
            if e_type.is_empty() { continue; }
            if let Some(next) = self.transition_internal(e_type, e_payload, guards, actions, 0, None) {
                last_valid_state = Some(next);
            }
        }
        last_valid_state
    }

    fn transition_internal(
        &mut self,
        event_type: &str,
        event_payload: &Value,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
        depth: usize,
        // Bypasses the standard event matching to forcefully execute a transition. 
        // Used primarily for resolving event-less `always` transitions or executing queued delayed `after` transitions.
        force_transition: Option<(&str, &[TransitionDefinition])>,
    ) -> Option<StateValue> {
        if depth > 100 {
            panic!("Maximum transition depth exceeded!");
        }
        let active_leaves = self.current_state.get_active_paths();
        
        let mut all_search_paths = Vec::new();
        for leaf in &active_leaves {
            let mut parts: Vec<&str> = leaf.split('.').collect();
            while !parts.is_empty() {
                let p = parts.join(".");
                if !all_search_paths.contains(&p) { all_search_paths.push(p); }
                parts.pop();
            }
        }
        if !all_search_paths.contains(&"".to_string()) { all_search_paths.push("".to_string()); }
        all_search_paths.sort_by_key(|b| core::cmp::Reverse(b.matches('.').count()));

        struct SelectedTransition<'a> {
             source_path: String,
             t: &'a TransitionDefinition,
             lowest_domain_depth: usize,
        }
        let mut selected_transitions = Vec::new();
        let mut conflict_regions: Vec<String> = Vec::new();

        for current_path in all_search_paths {
            let transitions_opt = if let Some((f_path, f_t)) = force_transition {
                 if *f_path == current_path { Some(f_t) } else { None }
            } else {
                let def_opt = if current_path.is_empty() {
                     None 
                } else {
                     Self::get_state_def_recursive(&self.definition.states, &current_path)
                };
                
                if current_path.is_empty() {
                     self.definition.on.get(event_type).map(|v| v.as_slice())
                } else {
                     def_opt.and_then(|d| d.on.get(event_type)).map(|v| v.as_slice())
                }
            };
            
            if let Some(transitions) = transitions_opt {
                 for t in transitions {
                       if let Some(cond) = &t.cond {
                           let is_allowed = match cond {
                               TransitionGuard::Name(n) => guards.get(n).map(|f| f(&self.context, event_payload, &None)).unwrap_or(true),
                               TransitionGuard::Object(o) => guards.get(&o.guard_type).map(|f| f(&self.context, event_payload, &o.params)).unwrap_or(true),
                           };
                           if !is_allowed { continue; }
                       }

                       let current_parts: Vec<&str> = current_path.split('.').filter(|s| !s.is_empty()).collect();
                       
                       let mut lowest_domain_depth = current_parts.len();
                       if let Some(targets) = &t.target {
                           for raw in targets {
                                let current_search_path = if current_path.is_empty() { "".to_string() } else { current_path.clone() };
                                let target_full = self.resolve_target(&current_search_path, raw).unwrap_or_else(|| raw.clone());
                                let target_parts: Vec<&str> = target_full.split('.').filter(|s| !s.is_empty()).collect();
                                let mut dom_lca = 0;
                                while dom_lca < current_parts.len() && dom_lca < target_parts.len() && current_parts[dom_lca] == target_parts[dom_lca] { dom_lca += 1; }
                                
                                let mut domain_depth = dom_lca;
                                if (dom_lca == current_parts.len() && t.reenter)
                                    || (dom_lca == target_parts.len() && dom_lca < current_parts.len())
                                {
                                    domain_depth = dom_lca.saturating_sub(1);
                                }
                                if domain_depth < lowest_domain_depth { lowest_domain_depth = domain_depth; }
                           }
                       } else if t.reenter {
                           lowest_domain_depth = current_parts.len().saturating_sub(1);
                       }

                       let domain_path = current_parts[0..lowest_domain_depth].join(".");
                       let conflict_check_path = if t.target.is_some() || t.reenter { domain_path.clone() } else { current_path.clone() };
                       
                       let mut conflict = false;
                       for c in &conflict_regions {
                            let inter = (conflict_check_path.is_empty() || c.is_empty()) || 
                                        conflict_check_path == *c || 
                                        conflict_check_path.starts_with(&format!("{}.", c)) || 
                                        c.starts_with(&format!("{}.", conflict_check_path));
                            if inter { conflict = true; break; }
                       }
                       if conflict { continue; }

                       conflict_regions.push(conflict_check_path.clone());
                       selected_transitions.push(SelectedTransition {
                           source_path: current_path.clone(),
                           t,
                           lowest_domain_depth,
                       });
                       break;
                 }
            }
        }

        if selected_transitions.is_empty() { return None; }

        let mut accumulated_state = self.current_state.clone();
        let mut all_collected_actions = Vec::new();
        let mut all_transition_actions = Vec::new();

        for st in &selected_transitions {
             let t = st.t;
             let current_path = &st.source_path;
             
             if let Some(targets) = &t.target {
                  for raw in targets {
                      let current_search_path = if current_path.is_empty() { "".to_string() } else { current_path.clone() };
                      let target_full = self.resolve_target(&current_search_path, raw).unwrap_or_else(|| raw.clone());
                      let target_def_opt = Self::get_state_def_recursive(&self.definition.states, &target_full);
                      let mut next_val_opt: Option<StateValue> = None;
                      
                      if let Some(target_def) = target_def_opt {
                           if target_def.type_.as_deref() == Some("history") || matches!(target_def.state_type, StateType::History{..}) {
                                let history_parent = if let Some(idx) = target_full.rfind('.') { target_full[..idx].to_string() } else { "".to_string() };
                                let is_deep = target_def.history == Some(HistoryType::Deep) || matches!(target_def.state_type, StateType::History{deep: true});
                                let mut hist_opt = self.history_value.get(&history_parent).cloned();
                                if hist_opt.is_none() {
                                     let parent_is_active = active_leaves.iter().any(|p| p == &history_parent || p.starts_with(&format!("{}.", history_parent)));
                                     if parent_is_active {
                                         let mut is_internal = false;
                                         if !t.reenter && (target_full.starts_with(&format!("{}.", current_path)) || &target_full == current_path) {
                                              is_internal = true;
                                         }
                                         if !is_internal { hist_opt = Some(self.current_state.clone()); }
                                     }
                                }

                                if let Some(hist) = hist_opt {
                                    if is_deep { next_val_opt = Some(hist.clone()); }
                                    else {
                                         let active_hist_paths = hist.get_active_paths();
                                         let parent_len = if history_parent.is_empty() { 0 } else { history_parent.len() + 1 };
                                         if !active_hist_paths.is_empty() {
                                             let matching_path = active_hist_paths.iter().find(|p| { if history_parent.is_empty() { true } else { p.starts_with(&history_parent) && (p.len() == history_parent.len() || p.chars().nth(history_parent.len()) == Some('.')) } });
                                             if let Some(first) = matching_path { if first.len() > parent_len { let remainder = &first[parent_len..]; let component = if let Some(idx) = remainder.find('.') { &remainder[..idx] } else { remainder }; let shallow_target = if history_parent.is_empty() { component.to_string() } else { format!("{}.{}", history_parent, component) }; next_val_opt = Self::resolve_initial_state(&self.definition, &shallow_target, &mut all_collected_actions); } }
                                         }
                                    }
                                }
                                if next_val_opt.is_none() {
                                     if let Some(default_t) = &target_def.history_target { 
                                         let resolved = self.resolve_target(&history_parent, default_t).unwrap_or(default_t.clone()); 
                                         next_val_opt = Self::resolve_initial_state(&self.definition, &resolved, &mut all_collected_actions); 
                                     } 
                                     else { next_val_opt = Self::resolve_initial_state(&self.definition, &history_parent, &mut all_collected_actions); }
                                }
                           }
                      }
                      let next_val = next_val_opt.or_else(|| Self::resolve_initial_state(&self.definition, &target_full, &mut all_collected_actions)).unwrap_or(StateValue::Atomic(target_full.clone()));
                      accumulated_state = Self::merge_state_value(&accumulated_state, &next_val, "", &target_full);
                  }
             }
             all_transition_actions.extend(t.actions.clone());
        }

        let finalized_state = accumulated_state;
        let next_leaves = finalized_state.get_active_paths();
        
        let mut document_order_active = Vec::new();
        for leaf in &active_leaves {
            let leaf_parts: Vec<&str> = leaf.split('.').filter(|s| !s.is_empty()).collect();
            for i in 1..=leaf_parts.len() {
                let path = leaf_parts[0..i].join(".");
                if !document_order_active.contains(&path) {
                    document_order_active.push(path);
                }
            }
        }
        
        let mut paths_to_exit = Vec::new();
        for leaf in active_leaves.iter().rev() {
             let leaf_parts: Vec<&str> = leaf.split('.').filter(|s| !s.is_empty()).collect();
             let mut max_lca = 0;
             for next_leaf in &next_leaves {
                  let next_parts: Vec<&str> = next_leaf.split('.').filter(|s| !s.is_empty()).collect();
                  let mut lca = 0;
                  while lca < leaf_parts.len() && lca < next_parts.len() && leaf_parts[lca] == next_parts[lca] { lca += 1; }
                  
                  let mut leaf_is_in_any_domain = false;
                  let mut max_allowed_lca = leaf_parts.len();
                  for st in &selected_transitions {
                       let src_parts: Vec<&str> = st.source_path.split('.').filter(|s| !s.is_empty()).collect();
                       let is_in_domain = src_parts.iter().enumerate().all(|(idx, p)| idx < leaf_parts.len() && leaf_parts[idx] == *p);
                       if is_in_domain {
                            leaf_is_in_any_domain = true;
                            if st.lowest_domain_depth < max_allowed_lca { max_allowed_lca = st.lowest_domain_depth; }
                       }
                  }
                  
                  if leaf_is_in_any_domain && lca > max_allowed_lca { lca = max_allowed_lca; }
                  if lca > max_lca { max_lca = lca; }
             }
             for i in ((max_lca + 1)..=leaf_parts.len()).rev() {
                  let path = leaf_parts[0..i].join(".");
                  if !paths_to_exit.contains(&path) {
                      paths_to_exit.push(path);
                  }
             }
        }
        
        let mut final_exited_paths = Vec::new();
        for path in document_order_active.iter().rev() {
             if paths_to_exit.contains(path) {
                  final_exited_paths.push(path.clone());
             }
        }
        
        let mut exited_paths = Vec::new();
        for path in &final_exited_paths {
              self.history_value.insert(path.clone(), self.current_state.clone());
              if let Some(def) = Self::get_state_def_recursive(&self.definition.states, path) {
                   for action in &def.exit { 
                         // we can't easily console.log from here, but we can if we had access.
                         Self::execute_action(&mut self.context, action, event_payload, actions, &mut self.internal_queue); 
                   }
                   for inv in &def.invoke {
                        if let Some(src) = inv.src.as_string() {
                             Self::execute_action(&mut self.context, &Action::Named(src.to_string()), event_payload, actions, &mut self.internal_queue);
                        }
                   }
              }
              exited_paths.push(path.clone());
        }
        
        self.active_timers.retain(|(p, _, _)| !exited_paths.contains(p));
        
        for action in &all_transition_actions { Self::execute_action(&mut self.context, action, event_payload, actions, &mut self.internal_queue); }
        for action in &all_collected_actions { Self::execute_action(&mut self.context, action, event_payload, actions, &mut self.internal_queue); }


        let mut entered_paths = Vec::new();
        for leaf in &next_leaves {
             let leaf_parts: Vec<&str> = leaf.split('.').filter(|s| !s.is_empty()).collect();
             let mut max_lca = 0;
             for curr_leaf in active_leaves.iter().rev() {
                  let curr_parts: Vec<&str> = curr_leaf.split('.').filter(|s| !s.is_empty()).collect();
                  let mut lca = 0;
                  while lca < leaf_parts.len() && lca < curr_parts.len() && leaf_parts[lca] == curr_parts[lca] { lca += 1; }
                  
                  let mut leaf_is_in_any_domain = false;
                  let mut max_allowed_lca = leaf_parts.len();
                  for st in &selected_transitions {
                       let src_parts: Vec<&str> = st.source_path.split('.').filter(|s| !s.is_empty()).collect();
                       let is_in_domain = src_parts.iter().enumerate().all(|(idx, p)| idx < leaf_parts.len() && leaf_parts[idx] == *p);
                       if is_in_domain {
                            leaf_is_in_any_domain = true;
                            if st.lowest_domain_depth < max_allowed_lca { max_allowed_lca = st.lowest_domain_depth; }
                       }
                  }
                  
                  if leaf_is_in_any_domain && lca > max_allowed_lca { lca = max_allowed_lca; }
                  if lca > max_lca { max_lca = lca; }
             }
        // Evaluate timers and pop
        let now = self.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(0);
        for i in (max_lca + 1)..=leaf_parts.len() {
              let path = leaf_parts[0..i].join(".");
              if !entered_paths.contains(&path) {
                  if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &path) {
                       for action in &def.entry { Self::execute_action(&mut self.context, action, event_payload, actions, &mut self.internal_queue); }
                       for inv in &def.invoke {
                            if let Some(src) = inv.src.as_string() {
                                 Self::execute_action(&mut self.context, &Action::Named(src.to_string()), event_payload, actions, &mut self.internal_queue);
                            }
                       }
                       for (delay, t_defs) in &def.after {
                           self.active_timers.push((path.clone(), now + delay, t_defs.clone()));
                       }
                  }
                  entered_paths.push(path);
              }
          }
         }
        let _changed = finalized_state != self.current_state;
        self.current_state = finalized_state;

        let has_transition_or_actions = !selected_transitions.is_empty();
        if !event_type.is_empty() || _changed || force_transition.is_some() {
             if let Some(next) = self.transition_internal("", event_payload, guards, actions, depth + 1, None) {
                 return Some(next);
             }
        }
        
        if has_transition_or_actions {
            Some(self.current_state.clone())
        } else {
            None
        }
    }
}

pub trait IntoClockFn {
    fn into_clock_fn(self) -> alloc::rc::Rc<dyn Fn() -> u64>;
}

impl IntoClockFn for alloc::rc::Rc<dyn Fn() -> u64> {
    fn into_clock_fn(self) -> alloc::rc::Rc<dyn Fn() -> u64> {
        self
    }
}

impl<F: Fn() -> u64 + 'static> IntoClockFn for alloc::rc::Rc<F> {
    fn into_clock_fn(self) -> alloc::rc::Rc<dyn Fn() -> u64> {
        self
    }
}

impl<F: Fn() -> u64 + 'static> IntoClockFn for F {
    fn into_clock_fn(self) -> alloc::rc::Rc<dyn Fn() -> u64> {
        alloc::rc::Rc::new(self)
    }
}

#[derive(Clone, Default)]
pub struct SimulatedClock {
    now_ms: alloc::rc::Rc<core::cell::Cell<u64>>,
}

impl SimulatedClock {
    pub fn new() -> Self {
        Self {
            now_ms: alloc::rc::Rc::new(core::cell::Cell::new(0)),
        }
    }

    pub fn with_initial(initial_ms: u64) -> Self {
        Self {
            now_ms: alloc::rc::Rc::new(core::cell::Cell::new(initial_ms)),
        }
    }

    pub fn now(&self) -> u64 {
        self.now_ms.get()
    }

    pub fn set_now_raw(&self, now_ms: u64) {
        self.now_ms.set(now_ms);
    }

    pub fn as_clock_fn(&self) -> alloc::rc::Rc<dyn Fn() -> u64> {
        let cell = self.now_ms.clone();
        alloc::rc::Rc::new(move || cell.get())
    }

    pub fn set(
        &self,
        target_ms: u64,
        machine: &mut RuntimeMachine,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) -> Option<StateValue> {
        let mut last_ret = None;
        let mut last_deadline: Option<u64> = None;
        let mut same_deadline_steps = 0usize;
        let mut total_steps = 0usize;

        while let Some(next_deadline) = machine.next_timer_deadline() {
            if next_deadline > target_ms {
                break;
            }
            if last_deadline == Some(next_deadline) {
                same_deadline_steps += 1;
                if same_deadline_steps >= 100 {
                    break;
                }
            } else {
                last_deadline = Some(next_deadline);
                same_deadline_steps = 0;
            }
            total_steps += 1;
            if total_steps > 1_000_000 {
                break;
            }

            self.now_ms.set(next_deadline);
            if let Some(st) = machine.tick(guards, actions) {
                last_ret = Some(st);
            }
        }

        self.now_ms.set(target_ms);

        last_ret
    }

    pub fn increment(
        &self,
        dt_ms: u64,
        machine: &mut RuntimeMachine,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) -> Option<StateValue> {
        let target = self.now_ms.get().saturating_add(dt_ms);
        self.set(target, machine, guards, actions)
    }
}

impl IntoClockFn for &SimulatedClock {
    fn into_clock_fn(self) -> alloc::rc::Rc<dyn Fn() -> u64> {
        self.as_clock_fn()
    }
}

impl IntoClockFn for SimulatedClock {
    fn into_clock_fn(self) -> alloc::rc::Rc<dyn Fn() -> u64> {
        self.as_clock_fn()
    }
}

#[cfg(test)]
mod repro_test;
