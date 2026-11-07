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
        let mut stack: Vec<(&StateValue, String)> = vec![(self, parent.to_string())];
        while let Some((node, current_parent)) = stack.pop() {
            match node {
                StateValue::Atomic(name) => {
                    if name.is_empty() {
                        if !current_parent.is_empty() {
                            paths.push(current_parent);
                        }
                    } else {
                        let full = if current_parent.is_empty() {
                            name.clone()
                        } else {
                            format!("{}.{}", current_parent, name)
                        };
                        paths.push(full);
                    }
                }
                StateValue::Compound(name, child) => {
                    let next_parent = if current_parent.is_empty() {
                        name.clone()
                    } else {
                        format!("{}.{}", current_parent, name)
                    };
                    stack.push((child.as_ref(), next_parent));
                }
                StateValue::Parallel(regions) => {
                    if regions.is_empty() {
                        if !current_parent.is_empty() {
                            paths.push(current_parent);
                        }
                    } else {
                        for (_region_name, region_value) in regions.iter().rev() {
                            stack.push((region_value, current_parent.clone()));
                        }
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
        let mut stack: Vec<(&StateValue, &str)> = vec![(self, path)];
        while let Some((node, rem_path)) = stack.pop() {
            if rem_path.is_empty() {
                continue;
            }
            match node {
                StateValue::Atomic(name) => {
                    if !name.is_empty() && rem_path == name {
                        return true;
                    }
                }
                StateValue::Compound(name, child) => {
                    if rem_path == name {
                        return true;
                    } else if rem_path.starts_with(name.as_str())
                        && rem_path.as_bytes().get(name.len()) == Some(&b'.')
                    {
                        stack.push((child.as_ref(), &rem_path[name.len() + 1..]));
                    }
                }
                StateValue::Parallel(regions) => {
                    for region_value in regions.values().rev() {
                        stack.push((region_value, rem_path));
                    }
                }
            }
        }
        false
    }

    /// Returns true if all active paths in `partial` are active in `self`.
    pub fn matches_state_value(&self, partial: &StateValue) -> bool {
        if matches!(partial, StateValue::Parallel(regions) if regions.is_empty()) {
            return !matches!(self, StateValue::Atomic(_));
        }
        let paths = partial.get_active_paths();
        !paths.is_empty() && paths.iter().all(|p| self.matches(p))
    }

    /// Iteratively serializes the active state value into a canonical `Value` (string or nested object)
    /// without CPU call-stack recursion.
    pub fn to_value(&self) -> Value {
        let mut stack: Vec<StateValueToValueFrame<'_>> = Vec::new();
        let mut completed: Option<Value> = None;
        let mut next_node: Option<&StateValue> = Some(self);

        loop {
            if let Some(comp) = completed.take() {
                if let Some(top) = stack.last_mut() {
                    match top {
                        StateValueToValueFrame::Compound { key } => {
                            let key_str = (*key).to_string();
                            stack.pop();
                            let mut map = IndexMap::new();
                            map.insert(key_str, comp);
                            completed = Some(Value::Object(map));
                        }
                        StateValueToValueFrame::Parallel {
                            iter,
                            map,
                            pending_key,
                        } => {
                            if let Some(k) = pending_key.take() {
                                map.insert(k.to_string(), comp);
                            }
                            if let Some((next_k, next_node_ref)) = iter.next() {
                                *pending_key = Some(next_k.as_str());
                                match next_node_ref {
                                    StateValue::Compound(c_key, child) if c_key == next_k => {
                                        next_node = Some(child.as_ref());
                                    }
                                    StateValue::Atomic(a_key) if a_key == next_k => {
                                        completed = Some(Value::Object(IndexMap::new()));
                                    }
                                    other => {
                                        next_node = Some(other);
                                    }
                                }
                            } else {
                                let finished_map = core::mem::take(map);
                                stack.pop();
                                completed = Some(Value::Object(finished_map));
                            }
                        }
                    }
                } else {
                    return comp;
                }
            } else if let Some(node) = next_node.take() {
                match node {
                    StateValue::Atomic(name) => {
                        if name.is_empty() {
                            completed = Some(Value::Object(IndexMap::new()));
                        } else if name.contains('.') {
                            let mut iter = name.rsplit('.');
                            let last = iter.next().unwrap_or("").to_string();
                            let mut acc = Value::String(last);
                            for part in iter {
                                let mut map = IndexMap::new();
                                map.insert(part.to_string(), acc);
                                acc = Value::Object(map);
                            }
                            completed = Some(acc);
                        } else {
                            completed = Some(Value::String(name.clone()));
                        }
                    }
                    StateValue::Compound(name, child) => {
                        stack.push(StateValueToValueFrame::Compound {
                            key: name.as_str(),
                        });
                        next_node = Some(child.as_ref());
                    }
                    StateValue::Parallel(regions) => {
                        if regions.is_empty() {
                            completed = Some(Value::Object(IndexMap::new()));
                        } else {
                            let mut iter = regions.iter();
                            let (first_k, first_node) = iter.next().unwrap();
                            stack.push(StateValueToValueFrame::Parallel {
                                iter,
                                map: IndexMap::new(),
                                pending_key: Some(first_k.as_str()),
                            });
                            match first_node {
                                StateValue::Compound(c_key, child) if c_key == first_k => {
                                    next_node = Some(child.as_ref());
                                }
                                StateValue::Atomic(a_key) if a_key == first_k => {
                                    completed = Some(Value::Object(IndexMap::new()));
                                }
                                other => {
                                    next_node = Some(other);
                                }
                            }
                        }
                    }
                }
            } else {
                return completed.unwrap_or_else(|| Value::Object(IndexMap::new()));
            }
        }
    }
}

enum StateValueToValueFrame<'a> {
    Compound {
        key: &'a str,
    },
    Parallel {
        iter: indexmap::map::Iter<'a, String, StateValue>,
        map: IndexMap<String, Value>,
        pending_key: Option<&'a str>,
    },
}

fn push_state_value_children(val: &mut StateValue, stack: &mut Vec<StateValue>) {
    match val {
        StateValue::Atomic(_) => {}
        StateValue::Compound(_, child) => {
            let inner = core::mem::replace(&mut **child, StateValue::Atomic(String::new()));
            if !matches!(inner, StateValue::Atomic(_)) {
                stack.push(inner);
            }
        }
        StateValue::Parallel(regions) => {
            for (_k, item) in core::mem::take(regions) {
                if !matches!(item, StateValue::Atomic(_)) {
                    stack.push(item);
                }
            }
        }
    }
}

impl Drop for StateValue {
    fn drop(&mut self) {
        let has_children = match self {
            StateValue::Atomic(_) => false,
            StateValue::Compound(_, _) => true,
            StateValue::Parallel(regions) => !regions.is_empty(),
        };
        if !has_children {
            return;
        }
        let mut stack: Vec<StateValue> = Vec::new();
        push_state_value_children(self, &mut stack);
        while let Some(mut top) = stack.pop() {
            push_state_value_children(&mut top, &mut stack);
        }
    }
}

pub type ActionFunction = Rc<dyn Fn(&mut Value, &Value, &Option<Value>)>;
pub type GuardFunction = Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>;
pub type WarnFunction = Rc<dyn Fn(&str)>;

/// Checks if an event type matches an event descriptor, supporting wildcards (`"*"`)
/// and partial wildcards (`"prefix.*"`).
pub fn matches_event_descriptor(event_type: &str, descriptor: &str) -> bool {
    matches_event_descriptor_with_warn(event_type, descriptor, |_| {})
}

/// Checks if an event type matches an event descriptor, invoking `warn` with
/// XState 5 diagnostic messages if invalid wildcard placement is encountered.
pub fn matches_event_descriptor_with_warn<F: FnMut(String)>(
    event_type: &str,
    descriptor: &str,
    mut warn: F,
) -> bool {
    if descriptor == event_type {
        return true;
    }
    if descriptor == "*" {
        return true;
    }
    if !descriptor.ends_with(".*") {
        return false;
    }
    if descriptor[..descriptor.len() - 1].contains('*') {
        warn(format!(
            "Wildcards can only be the last token of an event descriptor (e.g., \"event.*\") or the entire event descriptor (\"*\"). Check the \"{}\" event.",
            descriptor
        ));
    }
    let partial_event_tokens: Vec<&str> = descriptor.split('.').collect();
    let event_tokens: Vec<&str> = event_type.split('.').collect();
    for (token_index, partial_event_token) in partial_event_tokens.iter().enumerate() {
        if *partial_event_token == "*" {
            let is_last_token = token_index == partial_event_tokens.len() - 1;
            if !is_last_token {
                warn(format!(
                    "Infix wildcards in transition events are not allowed. Check the \"{}\" transition.",
                    descriptor
                ));
            }
            return is_last_token;
        }
        let event_token = event_tokens.get(token_index).copied();
        if Some(*partial_event_token) != event_token {
            return false;
        }
    }
    true
}

#[derive(Debug)]
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

enum ValueCloneFrame<'a> {
    Array {
        iter: core::slice::Iter<'a, Value>,
        cloned: Vec<Value>,
    },
    Object {
        iter: indexmap::map::Iter<'a, String, Value>,
        cloned: IndexMap<String, Value>,
        pending_key: Option<&'a str>,
    },
}

impl Clone for Value {
    fn clone(&self) -> Self {
        match self {
            Value::Null => return Value::Null,
            Value::Bool(b) => return Value::Bool(*b),
            Value::Number(n) => return Value::Number(*n),
            Value::String(s) => return Value::String(s.clone()),
            Value::Array(a) if a.is_empty() => return Value::Array(Vec::new()),
            Value::Object(o) if o.is_empty() => return Value::Object(IndexMap::new()),
            _ => {}
        }

        let mut stack: Vec<ValueCloneFrame<'_>> = Vec::with_capacity(16);
        let mut completed: Option<Value> = None;

        match self {
            Value::Array(a) => {
                stack.push(ValueCloneFrame::Array {
                    iter: a.iter(),
                    cloned: Vec::with_capacity(a.len()),
                });
            }
            Value::Object(o) => {
                stack.push(ValueCloneFrame::Object {
                    iter: o.iter(),
                    cloned: IndexMap::with_capacity(o.len()),
                    pending_key: None,
                });
            }
            _ => unreachable!(),
        }

        loop {
            if let Some(comp) = completed.take() {
                if let Some(top) = stack.last_mut() {
                    match top {
                        ValueCloneFrame::Array { cloned, .. } => {
                            cloned.push(comp);
                        }
                        ValueCloneFrame::Object {
                            cloned,
                            pending_key,
                            ..
                        } => {
                            let key = pending_key
                                .take()
                                .expect("pending_key must exist when child container finishes");
                            cloned.insert(key.to_string(), comp);
                        }
                    }
                } else {
                    return comp;
                }
            } else if let Some(top) = stack.last_mut() {
                match top {
                    ValueCloneFrame::Array { iter, cloned } => {
                        let mut child_frame = None;
                        for item in iter.by_ref() {
                            match item {
                                Value::Null => cloned.push(Value::Null),
                                Value::Bool(b) => cloned.push(Value::Bool(*b)),
                                Value::Number(n) => cloned.push(Value::Number(*n)),
                                Value::String(s) => cloned.push(Value::String(s.clone())),
                                Value::Array(a) if a.is_empty() => {
                                    cloned.push(Value::Array(Vec::new()))
                                }
                                Value::Object(o) if o.is_empty() => {
                                    cloned.push(Value::Object(IndexMap::new()))
                                }
                                Value::Array(a) => {
                                    child_frame = Some(ValueCloneFrame::Array {
                                        iter: a.iter(),
                                        cloned: Vec::with_capacity(a.len()),
                                    });
                                    break;
                                }
                                Value::Object(o) => {
                                    child_frame = Some(ValueCloneFrame::Object {
                                        iter: o.iter(),
                                        cloned: IndexMap::with_capacity(o.len()),
                                        pending_key: None,
                                    });
                                    break;
                                }
                            }
                        }

                        if let Some(frame) = child_frame {
                            stack.push(frame);
                        } else {
                            let frame = stack.pop().unwrap();
                            if let ValueCloneFrame::Array { cloned, .. } = frame {
                                completed = Some(Value::Array(cloned));
                            }
                        }
                    }
                    ValueCloneFrame::Object {
                        iter,
                        cloned,
                        pending_key,
                    } => {
                        let mut child_frame = None;
                        for (k, v) in iter.by_ref() {
                            match v {
                                Value::Null => {
                                    cloned.insert(k.clone(), Value::Null);
                                }
                                Value::Bool(b) => {
                                    cloned.insert(k.clone(), Value::Bool(*b));
                                }
                                Value::Number(n) => {
                                    cloned.insert(k.clone(), Value::Number(*n));
                                }
                                Value::String(s) => {
                                    cloned.insert(k.clone(), Value::String(s.clone()));
                                }
                                Value::Array(a) if a.is_empty() => {
                                    cloned.insert(k.clone(), Value::Array(Vec::new()));
                                }
                                Value::Object(o) if o.is_empty() => {
                                    cloned.insert(k.clone(), Value::Object(IndexMap::new()));
                                }
                                Value::Array(a) => {
                                    *pending_key = Some(k.as_str());
                                    child_frame = Some(ValueCloneFrame::Array {
                                        iter: a.iter(),
                                        cloned: Vec::with_capacity(a.len()),
                                    });
                                    break;
                                }
                                Value::Object(o) => {
                                    *pending_key = Some(k.as_str());
                                    child_frame = Some(ValueCloneFrame::Object {
                                        iter: o.iter(),
                                        cloned: IndexMap::with_capacity(o.len()),
                                        pending_key: None,
                                    });
                                    break;
                                }
                            }
                        }

                        if let Some(frame) = child_frame {
                            stack.push(frame);
                        } else {
                            let frame = stack.pop().unwrap();
                            if let ValueCloneFrame::Object { cloned, .. } = frame {
                                completed = Some(Value::Object(cloned));
                            }
                        }
                    }
                }
            } else {
                return completed.unwrap_or(Value::Null);
            }
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        let mut stack: Vec<(&Value, &Value)> = Vec::new();
        stack.push((self, other));

        while let Some((a, b)) = stack.pop() {
            match (a, b) {
                (Value::Null, Value::Null) => {}
                (Value::Bool(b1), Value::Bool(b2)) => {
                    if b1 != b2 {
                        return false;
                    }
                }
                (Value::Number(n1), Value::Number(n2)) => {
                    if n1 != n2 {
                        return false;
                    }
                }
                (Value::String(s1), Value::String(s2)) => {
                    if s1 != s2 {
                        return false;
                    }
                }
                (Value::Array(arr1), Value::Array(arr2)) => {
                    if arr1.len() != arr2.len() {
                        return false;
                    }
                    for (item1, item2) in arr1.iter().zip(arr2.iter()) {
                        stack.push((item1, item2));
                    }
                }
                (Value::Object(map1), Value::Object(map2)) => {
                    if map1.len() != map2.len() {
                        return false;
                    }
                    for (k, v1) in map1 {
                        match map2.get(k) {
                            Some(v2) => stack.push((v1, v2)),
                            None => return false,
                        }
                    }
                }
                _ => return false,
            }
        }

        true
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
            id: None, state_type: StateType::Atomic, initial: None, 
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
    pub guard: Option<TransitionGuard>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineStatus {
    Active,
    Done,
    Stopped,
}

impl MachineStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MachineStatus::Active => "active",
            MachineStatus::Done => "done",
            MachineStatus::Stopped => "stopped",
        }
    }
}

pub type Event = Value;

impl Value {
    pub fn event(event_type: &str) -> Self {
        let mut map = IndexMap::new();
        map.insert("type".to_string(), Value::String(event_type.to_string()));
        Value::Object(map)
    }

    pub fn event_with_payload(event_type: &str, mut payload: IndexMap<String, Value>) -> Self {
        payload.insert("type".to_string(), Value::String(event_type.to_string()));
        Value::Object(payload)
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
    pub is_initialized: bool,
    pub status: MachineStatus,
    pub guards: IndexMap<String, GuardFunction>,
    pub active_timers: Vec<(String, u64, Vec<TransitionDefinition>)>,
    pub clock_millis_fn: Option<alloc::rc::Rc<dyn Fn() -> u64>>,
    pub warn_fn: Option<WarnFunction>,
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

        let mut machine = Self {
            definition,
            current_state: initial_state,
            context,
            history_value: IndexMap::new(),
            initial_actions: dummy,
            is_started: false,
            is_initialized: false,
            status: MachineStatus::Active,
            guards: IndexMap::new(),
            active_timers: Vec::new(),
            clock_millis_fn: None,
            warn_fn: None,
            is_processing: false,
            external_queue: alloc::collections::VecDeque::new(),
            internal_queue: alloc::collections::VecDeque::new(),
        };
        machine.refresh_status();
        Ok(machine)
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

    pub fn register_warn_fn(&mut self, warn_fn: WarnFunction) {
        self.warn_fn = Some(warn_fn);
    }

    pub fn register_guard(&mut self, name: &str, guard: GuardFunction) {
        self.guards.insert(name.to_string(), guard);
    }

    fn compute_is_done(&self) -> bool {
        if matches!(self.definition.state_type, StateType::Final) {
            return true;
        }
        let active_paths = self.current_state.get_active_paths();
        !active_paths.is_empty()
            && active_paths.iter().all(|p| {
                self.get_state_definition(p)
                    .map(|def| matches!(def.state_type, StateType::Final))
                    .unwrap_or(false)
            })
    }

    fn refresh_status(&mut self) {
        if self.status == MachineStatus::Stopped {
            return;
        }
        if self.compute_is_done() {
            self.status = MachineStatus::Done;
        } else {
            self.status = MachineStatus::Active;
        }
    }

    pub fn status(&self) -> MachineStatus {
        if self.status == MachineStatus::Stopped {
            MachineStatus::Stopped
        } else if self.compute_is_done() {
            MachineStatus::Done
        } else {
            MachineStatus::Active
        }
    }

    pub fn is_done(&self) -> bool {
        self.status() == MachineStatus::Done
    }

    pub fn stop(&mut self) {
        self.status = MachineStatus::Stopped;
        self.active_timers.clear();
        self.external_queue.clear();
        self.internal_queue.clear();
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        let active_paths = self.current_state.get_active_paths();
        for path in &active_paths {
            let mut current_path: &str = path.as_str();
            while !current_path.is_empty() {
                if let Some(def) = Self::get_state_def_recursive(&self.definition.states, current_path) {
                    if def.tags.iter().any(|t| t == tag) {
                        return true;
                    }
                }
                if let Some(idx) = current_path.rfind('.') {
                    current_path = &current_path[..idx];
                } else {
                    break;
                }
            }
        }
        false
    }

    pub fn get_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        let active_paths = self.current_state.get_active_paths();
        for path in &active_paths {
            let mut current_path: &str = path.as_str();
            while !current_path.is_empty() {
                if let Some(def) = Self::get_state_def_recursive(&self.definition.states, current_path) {
                    for tag in &def.tags {
                        if !tags.contains(tag) {
                            tags.push(tag.clone());
                        }
                    }
                }
                if let Some(idx) = current_path.rfind('.') {
                    current_path = &current_path[..idx];
                } else {
                    break;
                }
            }
        }
        tags
    }

    pub fn compute_persisted_history_value(&self) -> Value {
        let mut history_map = IndexMap::new();
        let active_paths = self.current_state.get_active_paths();

        for (id, path) in &self.definition.id_map {
            if let Some(def) = self.get_state_definition(path) {
                if matches!(def.state_type, StateType::History { .. }) {
                    let is_deep = def.history == Some(HistoryType::Deep)
                        || matches!(def.state_type, StateType::History { deep: true });
                    let parent_path = if let Some(idx) = path.rfind('.') {
                        &path[..idx]
                    } else {
                        ""
                    };
                    let prefix_len = if parent_path.is_empty() {
                        0
                    } else {
                        parent_path.len() + 1
                    };

                    let mut raw_paths: Vec<String> = active_paths
                        .iter()
                        .filter(|p| {
                            if parent_path.is_empty() {
                                !p.is_empty()
                            } else {
                                p.len() > prefix_len
                                    && p.starts_with(parent_path)
                                    && p.as_bytes().get(parent_path.len()) == Some(&b'.')
                            }
                        })
                        .cloned()
                        .collect();

                    if raw_paths.is_empty() {
                        if let Some(stored) = self.history_value.get(parent_path) {
                            let stored_paths = stored.get_active_paths();
                            let matching_stored: Vec<String> = stored_paths
                                .iter()
                                .filter(|p| {
                                    if parent_path.is_empty() {
                                        !p.is_empty()
                                    } else {
                                        p.len() > prefix_len
                                            && p.starts_with(parent_path)
                                            && p.as_bytes().get(parent_path.len()) == Some(&b'.')
                                    }
                                })
                                .cloned()
                                .collect();
                            if !matching_stored.is_empty() {
                                raw_paths = matching_stored;
                            } else {
                                raw_paths = stored_paths
                                    .into_iter()
                                    .filter(|p| !p.is_empty())
                                    .collect();
                            }
                        }
                    }

                    let mut targets_to_serialize: Vec<String> = Vec::new();
                    for raw_path in raw_paths {
                        let target_path = if is_deep {
                            raw_path
                        } else if raw_path.len() > prefix_len {
                            let rest = &raw_path[prefix_len..];
                            let child = if let Some(end) = rest.find('.') {
                                &rest[..end]
                            } else {
                                rest
                            };
                            if parent_path.is_empty() {
                                child.to_string()
                            } else {
                                alloc::format!("{}.{}", parent_path, child)
                            }
                        } else {
                            raw_path
                        };
                        if !targets_to_serialize.contains(&target_path) {
                            targets_to_serialize.push(target_path);
                        }
                    }

                    if !self.history_value.contains_key(parent_path)
                        && def.history_target.is_none()
                    {
                        if let Some(p_def) = self.get_state_definition(parent_path) {
                            if let Some(initial_key) = &p_def.initial {
                                let full_initial = if parent_path.is_empty() {
                                    initial_key.clone()
                                } else {
                                    alloc::format!("{}.{}", parent_path, initial_key)
                                };
                                if targets_to_serialize.len() == 1
                                    && targets_to_serialize[0] == full_initial
                                {
                                    targets_to_serialize.clear();
                                }
                            }
                        }
                    }

                    if !targets_to_serialize.is_empty() {
                        let mut entries = Vec::with_capacity(targets_to_serialize.len());
                        for target_path in targets_to_serialize {
                            let target_id = self
                                .definition
                                .id_map
                                .iter()
                                .find(|(k, v)| {
                                    v.as_str() == target_path.as_str() && !k.starts_with('(')
                                })
                                .map(|(k, _)| k.clone())
                                .unwrap_or_else(|| {
                                    self.definition
                                        .id_map
                                        .iter()
                                        .find(|(_, v)| v.as_str() == target_path.as_str())
                                        .map(|(k, _)| k.clone())
                                        .unwrap_or_else(|| {
                                            alloc::format!("{}.{}", self.definition.id, target_path)
                                        })
                                });

                            let mut entry_obj = IndexMap::new();
                            entry_obj.insert("id".to_string(), Value::String(target_id));
                            entries.push(Value::Object(entry_obj));
                        }
                        history_map.insert(id.clone(), Value::Array(entries));
                    }
                }
            }
        }

        Value::Object(history_map)
    }

    pub fn get_persisted_snapshot_value(&self) -> Value {
        let mut map = IndexMap::new();
        map.insert("status".to_string(), Value::String(self.status().as_str().to_string()));
        map.insert("value".to_string(), self.current_state.to_value());
        map.insert("context".to_string(), self.context.clone());
        map.insert("historyValue".to_string(), self.compute_persisted_history_value());
        map.insert(
            "tags".to_string(),
            Value::Array(self.get_tags().into_iter().map(Value::String).collect()),
        );
        map.insert("children".to_string(), Value::Object(IndexMap::new()));
        Value::Object(map)
    }

    pub fn get_persisted_snapshot_json(&self) -> alloc::string::String {
        self.get_persisted_snapshot_value().to_json_string()
    }

    pub fn get_snapshot_json(&self) -> alloc::string::String {
        self.get_persisted_snapshot_json()
    }

    pub fn restore_from_snapshot_value_owned(&mut self, val: Value) -> Result<(), &'static str> {
        let mut map = match val.into_object() {
            Some(m) => m,
            None => return Err("Snapshot must be a JSON object"),
        };

        // 1. Status
        if let Some(status_val) = map.swap_remove("status") {
            if let Some(s) = status_val.as_string() {
                match s.as_str() {
                    "stopped" => self.status = MachineStatus::Stopped,
                    "done" => self.status = MachineStatus::Done,
                    "active" => self.status = MachineStatus::Active,
                    _ => {}
                }
            }
        }

        // 2. Value
        if let Some(state_val) = map.swap_remove("value") {
            if !matches!(state_val, Value::Null) {
                self.current_state = value_to_state_value(state_val);
                self.is_initialized = true;
            }
        }

        // 3. Context
        if let Some(ctx_val) = map.swap_remove("context") {
            if !matches!(ctx_val, Value::Null) {
                self.context = ctx_val;
            }
        }

        // 4. History Value
        let history_entry = map.swap_remove("historyValue").or_else(|| map.swap_remove("history"));
        if let Some(h_val) = history_entry {
            if let Some(hist_map) = h_val.into_object() {
                self.history_value.clear();
                let mut paths_by_parent: IndexMap<String, Vec<String>> = IndexMap::new();

                for (k, mut v) in hist_map {
                    let parent_path = if let Some(history_node_path) = self.definition.id_map.get(&k) {
                        if let Some(idx) = history_node_path.rfind('.') {
                            history_node_path[0..idx].to_string()
                        } else {
                            alloc::string::String::new()
                        }
                    } else if let Some(idx) = k.rfind('.') {
                        k[0..idx].to_string()
                    } else {
                        k
                    };

                    match &mut v {
                        Value::Array(arr) => {
                            for item in arr.iter() {
                                if let Some(id_val) = item
                                    .as_object()
                                    .and_then(|o| o.get("id"))
                                    .and_then(|i| i.as_string())
                                {
                                    if let Some(path) = self.definition.id_map.get(id_val) {
                                        let entry = paths_by_parent
                                            .entry(parent_path.clone())
                                            .or_default();
                                        if !entry.contains(path) {
                                            entry.push(path.clone());
                                        }
                                    } else if let Some(warn) = &self.warn_fn {
                                        warn(&alloc::format!(
                                            "Could not resolve StateNode for id: {}",
                                            id_val
                                        ));
                                    }
                                }
                            }
                        }
                        Value::String(s) => {
                            let s_taken = core::mem::take(s);
                            if !s_taken.is_empty() {
                                let entry = paths_by_parent.entry(parent_path).or_default();
                                if !entry.contains(&s_taken) {
                                    entry.push(s_taken);
                                }
                            }
                        }
                        Value::Object(_) => {
                            let obj = core::mem::replace(&mut v, Value::Null);
                            self.history_value
                                .insert(parent_path, value_to_state_value(obj));
                        }
                        _ => {}
                    }
                }

                for (parent_path, paths) in paths_by_parent {
                    let leaf_paths: Vec<String> = paths
                        .iter()
                        .filter(|p| {
                            let prefix = alloc::format!("{}.", p);
                            !paths
                                .iter()
                                .any(|other| other.len() > p.len() && other.starts_with(&prefix))
                        })
                        .cloned()
                        .collect();

                    let mut merged: Option<StateValue> = None;
                    for p in leaf_paths {
                        let mut dummy_actions = Vec::new();
                        let sv =
                            Self::resolve_initial_state(&self.definition, &p, &mut dummy_actions)
                                .unwrap_or_else(|| value_to_state_value(Value::String(p.clone())));
                        merged = Some(match merged {
                            None => sv,
                            Some(acc) => Self::merge_state_value(&acc, &sv, "", &p),
                        });
                    }
                    if let Some(state_val) = merged {
                        self.history_value.insert(parent_path, state_val);
                    }
                }
            }
        }

        self.revive_timers();
        self.refresh_status();
        Ok(())
    }

    pub fn restore_from_snapshot_value(&mut self, val: &Value) -> Result<(), &'static str> {
        self.restore_from_snapshot_value_owned(val.clone())
    }

    pub fn restore_from_snapshot_json(&mut self, json_str: &str) -> Result<(), &'static str> {
        let val = Value::from_json(json_str).map_err(|_| "Invalid JSON for snapshot")?;
        self.restore_from_snapshot_value_owned(val)
    }

    pub fn can(&self, event_name: &str) -> bool {
        let payload = Value::event(event_name);
        self.can_with_guards(event_name, &payload, &self.guards)
    }

    pub fn can_event(&self, event: &Event) -> bool {
        match event {
            Value::String(s) => self.can(s),
            Value::Object(map) => {
                let event_type = map
                    .get("type")
                    .and_then(|v| v.as_string())
                    .map(|s| s.as_str())
                    .unwrap_or("");
                if event_type.is_empty() {
                    return false;
                }
                self.can_with_guards(event_type, event, &self.guards)
            }
            _ => false,
        }
    }

    pub fn can_with_guards(
        &self,
        event_type: &str,
        event_payload: &Value,
        guards: &IndexMap<String, GuardFunction>,
    ) -> bool {
        if event_type.is_empty() || self.status() == MachineStatus::Stopped {
            return false;
        }
        let active_leaves = self.current_state.get_active_paths();
        let mut all_search_paths = Vec::new();
        for leaf in &active_leaves {
            let mut parts: Vec<&str> = leaf.split('.').collect();
            while !parts.is_empty() {
                let p = parts.join(".");
                if !all_search_paths.contains(&p) {
                    all_search_paths.push(p);
                }
                parts.pop();
            }
        }
        if !all_search_paths.contains(&"".to_string()) {
            all_search_paths.push("".to_string());
        }
        all_search_paths.sort_by_key(|b| core::cmp::Reverse(b.matches('.').count()));

        for current_path in all_search_paths {
            let on_map_opt = if current_path.is_empty() {
                Some(&self.definition.on)
            } else {
                Self::get_state_def_recursive(&self.definition.states, &current_path)
                    .map(|d| &d.on)
            };
            let candidates = if let Some(on_map) = on_map_opt {
                Self::get_candidates(on_map, event_type, &None)
            } else {
                Vec::new()
            };
            for t in candidates {
                if let Some(guard) = &t.guard {
                    let is_allowed = match guard {
                        TransitionGuard::Name(n) => guards
                            .get(n)
                            .or_else(|| self.guards.get(n))
                            .map(|f| f(&self.context, event_payload, &None))
                            .unwrap_or(true),
                        TransitionGuard::Object(o) => guards
                            .get(&o.guard_type)
                            .or_else(|| self.guards.get(&o.guard_type))
                            .map(|f| f(&self.context, event_payload, &o.params))
                            .unwrap_or(true),
                    };
                    if !is_allowed {
                        continue;
                    }
                }
                return true;
            }
        }
        false
    }

    fn get_candidates<'a>(
        on_map: &'a IndexMap<String, Vec<TransitionDefinition>>,
        event_type: &str,
        warn_fn: &Option<WarnFunction>,
    ) -> Vec<&'a TransitionDefinition> {
        if let Some(exact) = on_map.get(event_type) {
            return exact.iter().collect();
        }
        if event_type.is_empty() {
            return Vec::new();
        }
        let mut matching_keys: Vec<&String> = on_map
            .keys()
            .filter(|descriptor| {
                !descriptor.is_empty()
                    && matches_event_descriptor_with_warn(event_type, descriptor, |msg| {
                        if let Some(w) = warn_fn {
                            w(&msg);
                        }
                    })
            })
            .collect();
        matching_keys.sort_by_key(|b| core::cmp::Reverse(b.len()));
        let mut candidates = Vec::new();
        for key in matching_keys {
            if let Some(transitions) = on_map.get(key) {
                candidates.extend(transitions.iter());
            }
        }
        candidates
    }

    pub fn next_timer_deadline(&self) -> Option<u64> {
        if !self.is_started || self.status == MachineStatus::Stopped {
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
        if self.is_started || self.status == MachineStatus::Stopped {
            return;
        }
        for (k, v) in guards {
            self.guards.insert(k.clone(), v.clone());
        }
        self.is_started = true;

        if self.is_initialized {
            self.revive_timers();
            self.is_processing = true;
            let mut init_map = IndexMap::new();
            init_map.insert("type".to_string(), Value::String("xstate.init".to_string()));
            let init_event = Value::Object(init_map);
            let _ = self.transition_internal("", &init_event, guards, actions, 0, None);
            let _ = self.drain_internal_queue(guards, actions);
            self.is_processing = false;
            let _ = self.drain_external_queue(guards, actions);
            self.refresh_status();
            return;
        }

        self.is_initialized = true;
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
        self.refresh_status();
    }

    pub fn revive_timers(&mut self) {
        if !self.is_started || self.status == MachineStatus::Stopped { return; }
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
        if !self.is_started || self.is_processing || self.status == MachineStatus::Stopped {
            return None;
        }
        for (k, v) in guards {
            self.guards.insert(k.clone(), v.clone());
        }
        self.is_processing = true;
        let now = self.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(0);

        let mut last_ret = None;
        let mut steps = 0usize;
        while steps < 100 {
            if self.status == MachineStatus::Stopped {
                break;
            }
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
        self.refresh_status();
        last_ret
    }

    pub fn transition(
        &mut self,
        event_type: &str,
        event_payload: &Value,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) -> Option<StateValue> {
        if self.status == MachineStatus::Stopped {
            return None;
        }
        for (k, v) in guards {
            self.guards.insert(k.clone(), v.clone());
        }
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
            if self.status == MachineStatus::Stopped {
                self.external_queue.clear();
                break;
            }
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
        self.refresh_status();
        last_ret
    }
    
    fn drain_internal_queue(&mut self, guards: &IndexMap<String, GuardFunction>, actions: &IndexMap<String, ActionFunction>) -> Option<StateValue> {
        let mut last_valid_state = None;
        let mut iterations = 0;
        while let Some(raised) = self.internal_queue.pop_front() {
            if self.status == MachineStatus::Stopped {
                self.internal_queue.clear();
                break;
            }
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
        if self.status == MachineStatus::Stopped {
            return None;
        }
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
            let candidates: Vec<&TransitionDefinition> = if let Some((f_path, f_t)) = force_transition {
                if *f_path == current_path {
                    f_t.iter().collect()
                } else {
                    Vec::new()
                }
            } else {
                let on_map_opt = if current_path.is_empty() {
                    Some(&self.definition.on)
                } else {
                    Self::get_state_def_recursive(&self.definition.states, &current_path)
                        .map(|d| &d.on)
                };
                if let Some(on_map) = on_map_opt {
                    Self::get_candidates(on_map, event_type, &self.warn_fn)
                } else {
                    Vec::new()
                }
            };

            if !candidates.is_empty() {
                  for t in candidates {
                       if let Some(guard) = &t.guard {
                           let is_allowed = match guard {
                               TransitionGuard::Name(n) => guards
                                   .get(n)
                                   .or_else(|| self.guards.get(n))
                                   .map(|f| f(&self.context, event_payload, &None))
                                   .unwrap_or(true),
                               TransitionGuard::Object(o) => guards
                                   .get(&o.guard_type)
                                   .or_else(|| self.guards.get(&o.guard_type))
                                   .map(|f| f(&self.context, event_payload, &o.params))
                                   .unwrap_or(true),
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
                           if matches!(target_def.state_type, StateType::History { .. }) {
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
        
        let _changed = finalized_state != self.current_state;
        self.current_state = finalized_state.clone();

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

        let has_transition_or_actions = !selected_transitions.is_empty();
        self.refresh_status();
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
