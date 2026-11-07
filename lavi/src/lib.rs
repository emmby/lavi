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
use alloc::rc::Rc;
use core::hash::{BuildHasherDefault, Hasher};

/// A fast, lightweight, non-cryptographic FNV-1a hasher for `#![no_std]` environments.
#[derive(Clone, Copy, Debug)]
pub struct FnvHasher(u64);

impl Default for FnvHasher {
    #[inline]
    fn default() -> Self {
        Self(0xcbf29ce484222325)
    }
}

impl Hasher for FnvHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut hash = self.0;
        for &byte in bytes {
            hash = (hash ^ (byte as u64)).wrapping_mul(0x100000001b3);
        }
        self.0 = hash;
    }
}

pub type DefaultHashBuilder = BuildHasherDefault<FnvHasher>;
pub type IndexMap<K, V, S = DefaultHashBuilder> = indexmap::IndexMap<K, V, S>;

pub trait IndexMapExt<K, V, S> {
    fn new() -> Self;
    fn with_capacity(n: usize) -> Self;
}

impl<K, V, S: Default> IndexMapExt<K, V, S> for indexmap::IndexMap<K, V, S> {
    #[inline]
    fn new() -> Self {
        Self::with_capacity_and_hasher(0, S::default())
    }
    #[inline]
    fn with_capacity(n: usize) -> Self {
        Self::with_capacity_and_hasher(n, S::default())
    }
}

#[macro_export]
macro_rules! indexmap {
    () => {
        $crate::IndexMap::new()
    };
    ($($key:expr => $value:expr),* $(,)?) => {{
        let mut map = $crate::IndexMap::new();
        $( map.insert($key, $value); )*
        map
    }};
}


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
        enum WorkItem<'a> {
            Node(&'a StateValue, String),
            Leaf(String),
        }
        let mut stack: Vec<WorkItem> = vec![WorkItem::Node(self, parent.to_string())];
        while let Some(item) = stack.pop() {
            match item {
                WorkItem::Leaf(path) => {
                    paths.push(path);
                }
                WorkItem::Node(node, current_parent) => {
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
                            stack.push(WorkItem::Node(child.as_ref(), next_parent));
                        }
                        StateValue::Parallel(regions) => {
                            if regions.is_empty() {
                                if !current_parent.is_empty() {
                                    paths.push(current_parent);
                                }
                            } else {
                                for (region_name, region_value) in regions.iter().rev() {
                                    let next_parent = if current_parent.is_empty() {
                                        region_name.clone()
                                    } else {
                                        format!("{}.{}", current_parent, region_name)
                                    };
                                    match region_value {
                                        StateValue::Atomic(a_name) if a_name == region_name => {
                                            stack.push(WorkItem::Leaf(next_parent));
                                        }
                                        StateValue::Compound(c_name, child) if c_name == region_name => {
                                            stack.push(WorkItem::Node(child.as_ref(), next_parent));
                                        }
                                        other => {
                                            stack.push(WorkItem::Node(other, next_parent));
                                        }
                                    }
                                }
                            }
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
                    for (region_name, region_value) in regions.iter().rev() {
                        if rem_path == region_name.as_str() {
                            return true;
                        }
                        match region_value {
                            StateValue::Atomic(a_name) if a_name == region_name => {}
                            StateValue::Compound(c_name, child) if c_name == region_name => {
                                if rem_path.starts_with(region_name.as_str())
                                    && rem_path.as_bytes().get(region_name.len()) == Some(&b'.')
                                {
                                    stack.push((child.as_ref(), &rem_path[region_name.len() + 1..]));
                                } else {
                                    stack.push((region_value, rem_path));
                                }
                            }
                            other => {
                                if rem_path.starts_with(region_name.as_str())
                                    && rem_path.as_bytes().get(region_name.len()) == Some(&b'.')
                                {
                                    stack.push((other, &rem_path[region_name.len() + 1..]));
                                } else {
                                    stack.push((other, rem_path));
                                }
                            }
                        }
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

#[derive(Debug, Clone, PartialEq)]
pub struct DelayedEvent {
    pub id: Option<String>,
    pub deadline: u64,
    pub event: Value,
}

pub struct ActionEffects {
    pub raised: alloc::collections::VecDeque<Value>,
    pub delayed_events: Vec<DelayedEvent>,
    pub cancel_ids: Vec<String>,
    pub error: Option<String>,
}

impl ActionEffects {
    pub fn new() -> Self {
        Self {
            raised: alloc::collections::VecDeque::new(),
            delayed_events: Vec::new(),
            cancel_ids: Vec::new(),
            error: None,
        }
    }
}

impl Default for ActionEffects {
    fn default() -> Self {
        Self::new()
    }
}

pub type ActionFunction = Rc<dyn Fn(&mut Value, &Value, &Option<Value>)>;
pub type ActionWithEffectsFunction = Rc<
    dyn Fn(
        &mut Value,
        &Value,
        &Option<Value>,
        &IndexMap<String, ActionFunction>,
        &IndexMap<String, GuardFunction>,
        &IndexMap<String, DelayFunction>,
        &StateValue,
        &mut ActionEffects,
    ),
>;
pub type GuardFunction = Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>;
pub type DelayFunction = Rc<dyn Fn(&Value, &Value) -> u64>;
pub type LoggerFunction = Rc<dyn Fn(&Option<String>, &Value)>;
pub type WarnFunction = Rc<dyn Fn(&str)>;
pub type OutputResolver = Rc<dyn Fn(&Value, &Value) -> Value>;

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Delay {
    Literal(u64),
    Named(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StateType {
    Atomic,
    Compound,
    Parallel,
    Final,
    History { deep: bool },
}

#[derive(Debug, PartialEq)]
pub struct StateDefinition {
    pub id: Option<String>,
    pub state_type: StateType, // Enum representation
    pub initial: Option<String>,
    pub initial_actions: Vec<Action>,
    pub states: IndexMap<String, StateDefinition>,
    pub on: IndexMap<String, Vec<TransitionDefinition>>,
    pub after: IndexMap<u64, Vec<TransitionDefinition>>,
    pub named_after: IndexMap<String, Vec<TransitionDefinition>>,
    pub entry: Vec<Action>,
    pub exit: Vec<Action>,
    pub history: Option<HistoryType>, // If this state IS a history state
    pub history_target: Option<String>, // Default target for history
    pub tags: Vec<String>,
    pub invoke: Vec<InvokeDefinition>,
    pub history_actions: Vec<Action>,
    pub meta: Option<Value>,
    pub description: Option<String>,
    pub output: Option<Value>,
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
            states: IndexMap::new(), on: IndexMap::new(), after: IndexMap::new(), named_after: IndexMap::new(), entry: vec![], exit: vec![], 
            history: None, history_target: None, tags: vec![], invoke: vec![], history_actions: vec![],
            meta: None, description: None, output: None,
        }
    }

    pub fn clone_shallow(&self) -> Self {
        Self {
            id: self.id.clone(),
            state_type: self.state_type.clone(),
            initial: self.initial.clone(),
            initial_actions: self.initial_actions.clone(),
            states: IndexMap::with_capacity(self.states.len()),
            on: self.on.clone(),
            after: self.after.clone(),
            named_after: self.named_after.clone(),
            entry: self.entry.clone(),
            exit: self.exit.clone(),
            history: self.history.clone(),
            history_target: self.history_target.clone(),
            tags: self.tags.clone(),
            invoke: self.invoke.clone(),
            history_actions: self.history_actions.clone(),
            meta: self.meta.clone(),
            description: self.description.clone(),
            output: self.output.clone(),
        }
    }
}

impl Default for StateDefinition {
    fn default() -> Self {
        Self::default_atomic()
    }
}

struct StateCloneFrame<'a> {
    key: String,
    state_def: StateDefinition,
    remaining_children: indexmap::map::Iter<'a, String, StateDefinition>,
}

impl Clone for StateDefinition {
    fn clone(&self) -> Self {
        if self.states.is_empty() {
            return self.clone_shallow();
        }

        let root_frame = StateCloneFrame {
            key: String::new(),
            state_def: self.clone_shallow(),
            remaining_children: self.states.iter(),
        };

        let mut stack: Vec<StateCloneFrame<'_>> = vec![root_frame];

        while let Some(top) = stack.last_mut() {
            if let Some((child_key, child_def)) = top.remaining_children.next() {
                if child_def.states.is_empty() {
                    top.state_def.states.insert(child_key.clone(), child_def.clone_shallow());
                } else {
                    let child_frame = StateCloneFrame {
                        key: child_key.clone(),
                        state_def: child_def.clone_shallow(),
                        remaining_children: child_def.states.iter(),
                    };
                    stack.push(child_frame);
                }
            } else {
                let finished = stack.pop().unwrap();
                if let Some(parent) = stack.last_mut() {
                    parent.state_def.states.insert(finished.key, finished.state_def);
                } else {
                    return finished.state_def;
                }
            }
        }

        unreachable!()
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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TransitionDefinition {
    pub target: Option<Vec<String>>,
    pub guard: Option<TransitionGuard>,
    pub actions: Vec<Action>,
    pub reenter: bool,
    pub description: Option<String>,
}


#[derive(Debug)]
pub enum TransitionGuard {
    Name(String),
    Object(GuardObject),
    And(Vec<TransitionGuard>),
    Or(Vec<TransitionGuard>),
    Not(Box<TransitionGuard>),
    StateIn(Value),
}

enum GuardCloneFrame<'a> {
    Not,
    And {
        iter: core::slice::Iter<'a, TransitionGuard>,
        cloned: Vec<TransitionGuard>,
    },
    Or {
        iter: core::slice::Iter<'a, TransitionGuard>,
        cloned: Vec<TransitionGuard>,
    },
}

#[inline]
fn clone_leaf_guard(guard: &TransitionGuard) -> Option<TransitionGuard> {
    match guard {
        TransitionGuard::Name(s) => Some(TransitionGuard::Name(s.clone())),
        TransitionGuard::Object(o) => Some(TransitionGuard::Object(o.clone())),
        TransitionGuard::StateIn(v) => Some(TransitionGuard::StateIn(v.clone())),
        TransitionGuard::And(c) if c.is_empty() => Some(TransitionGuard::And(Vec::new())),
        TransitionGuard::Or(c) if c.is_empty() => Some(TransitionGuard::Or(Vec::new())),
        _ => None,
    }
}

#[inline]
fn advance_guard_children<'a>(
    iter: &mut core::slice::Iter<'a, TransitionGuard>,
    cloned: &mut Vec<TransitionGuard>,
) -> Option<&'a TransitionGuard> {
    for child in iter.by_ref() {
        if let Some(leaf) = clone_leaf_guard(child) {
            cloned.push(leaf);
        } else {
            return Some(child);
        }
    }
    None
}

impl Clone for TransitionGuard {
    fn clone(&self) -> Self {
        if let Some(leaf) = clone_leaf_guard(self) {
            return leaf;
        }

        let mut stack: Vec<GuardCloneFrame<'_>> = Vec::with_capacity(16);
        let mut current: Option<&TransitionGuard> = Some(self);
        let mut completed: Option<TransitionGuard> = None;

        loop {
            if let Some(guard) = current.take() {
                if let Some(leaf) = clone_leaf_guard(guard) {
                    completed = Some(leaf);
                } else {
                    match guard {
                        TransitionGuard::Not(inner) => {
                            stack.push(GuardCloneFrame::Not);
                            current = Some(inner.as_ref());
                        }
                        TransitionGuard::And(children) => {
                            let mut iter = children.iter();
                            let mut cloned = Vec::with_capacity(children.len());
                            if let Some(non_leaf) = advance_guard_children(&mut iter, &mut cloned) {
                                stack.push(GuardCloneFrame::And { iter, cloned });
                                current = Some(non_leaf);
                            } else {
                                completed = Some(TransitionGuard::And(cloned));
                            }
                        }
                        TransitionGuard::Or(children) => {
                            let mut iter = children.iter();
                            let mut cloned = Vec::with_capacity(children.len());
                            if let Some(non_leaf) = advance_guard_children(&mut iter, &mut cloned) {
                                stack.push(GuardCloneFrame::Or { iter, cloned });
                                current = Some(non_leaf);
                            } else {
                                completed = Some(TransitionGuard::Or(cloned));
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            } else if let Some(comp) = completed.take() {
                if let Some(top) = stack.last_mut() {
                    match top {
                        GuardCloneFrame::Not => {
                            stack.pop();
                            completed = Some(TransitionGuard::Not(Box::new(comp)));
                        }
                        GuardCloneFrame::And { iter, cloned } => {
                            cloned.push(comp);
                            if let Some(non_leaf) = advance_guard_children(iter, cloned) {
                                current = Some(non_leaf);
                            } else {
                                let frame = stack.pop().unwrap();
                                if let GuardCloneFrame::And { cloned, .. } = frame {
                                    completed = Some(TransitionGuard::And(cloned));
                                }
                            }
                        }
                        GuardCloneFrame::Or { iter, cloned } => {
                            cloned.push(comp);
                            if let Some(non_leaf) = advance_guard_children(iter, cloned) {
                                current = Some(non_leaf);
                            } else {
                                let frame = stack.pop().unwrap();
                                if let GuardCloneFrame::Or { cloned, .. } = frame {
                                    completed = Some(TransitionGuard::Or(cloned));
                                }
                            }
                        }
                    }
                } else {
                    return comp;
                }
            } else {
                unreachable!();
            }
        }
    }
}

impl PartialEq for TransitionGuard {
    fn eq(&self, other: &Self) -> bool {
        let mut stack: Vec<(&TransitionGuard, &TransitionGuard)> = Vec::new();
        stack.push((self, other));

        while let Some((a, b)) = stack.pop() {
            match (a, b) {
                (TransitionGuard::Name(s1), TransitionGuard::Name(s2)) => {
                    if s1 != s2 {
                        return false;
                    }
                }
                (TransitionGuard::Object(o1), TransitionGuard::Object(o2)) => {
                    if o1 != o2 {
                        return false;
                    }
                }
                (TransitionGuard::StateIn(v1), TransitionGuard::StateIn(v2)) => {
                    if v1 != v2 {
                        return false;
                    }
                }
                (TransitionGuard::Not(n1), TransitionGuard::Not(n2)) => {
                    stack.push((n1.as_ref(), n2.as_ref()));
                }
                (TransitionGuard::And(v1), TransitionGuard::And(v2)) => {
                    if v1.len() != v2.len() {
                        return false;
                    }
                    for (item1, item2) in v1.iter().zip(v2.iter()) {
                        stack.push((item1, item2));
                    }
                }
                (TransitionGuard::Or(v1), TransitionGuard::Or(v2)) => {
                    if v1.len() != v2.len() {
                        return false;
                    }
                    for (item1, item2) in v1.iter().zip(v2.iter()) {
                        stack.push((item1, item2));
                    }
                }
                _ => return false,
            }
        }

        true
    }
}

impl Drop for TransitionGuard {
    fn drop(&mut self) {
        let mut stack: Vec<TransitionGuard> = Vec::new();
        match self {
            TransitionGuard::And(children) | TransitionGuard::Or(children) => {
                stack.append(children);
            }
            TransitionGuard::Not(child) => {
                let inner = core::mem::replace(child.as_mut(), TransitionGuard::Name(String::new()));
                stack.push(inner);
            }
            _ => {}
        }
        while let Some(mut guard) = stack.pop() {
            match &mut guard {
                TransitionGuard::And(children) | TransitionGuard::Or(children) => {
                    stack.append(children);
                }
                TransitionGuard::Not(child) => {
                    let inner = core::mem::replace(child.as_mut(), TransitionGuard::Name(String::new()));
                    stack.push(inner);
                }
                _ => {}
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuardObject {
    pub guard_type: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DelayedRaiseAction {
    pub event: Value,
    pub delay: Delay,
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CancelAction {
    pub send_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogAction {
    pub value: Value,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Named(String),
    Assign(AssignAction),
    Generic(GenericAction),
    Raise(Value),
    DelayedRaise(DelayedRaiseAction),
    Cancel(CancelAction),
    Log(LogAction),
    RaiseInvalid(String),
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
    pub after: IndexMap<Delay, Vec<TransitionDefinition>>,
    pub entry: Vec<Action>,
    pub exit: Vec<Action>,
    pub invoke: Vec<InvokeDefinition>,
    pub context: Option<Value>,
    pub types: Option<Value>,
    pub id_map: IndexMap<String, String>,
    pub tags: Vec<String>,
    pub meta: Option<Value>,
    pub description: Option<String>,
    pub input: Option<Value>,
    pub output: Option<Value>,
}

impl MachineDefinition {
    pub fn from_value(val: Value) -> Result<Self, JsonError> {
        parser::parse_machine_definition(val)
    }
}

impl Default for MachineDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            state_type: StateType::Compound,
            initial: String::new(),
            initial_actions: Vec::new(),
            states: IndexMap::new(),
            on: IndexMap::new(),
            after: IndexMap::new(),
            entry: Vec::new(),
            exit: Vec::new(),
            invoke: Vec::new(),
            context: None,
            types: None,
            id_map: IndexMap::new(),
            tags: Vec::new(),
            meta: None,
            description: None,
            input: None,
            output: None,
        }
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
    pub input: Option<Value>,
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
    pub state_outputs: IndexMap<String, Value>,
    pub machine_output: Option<Value>,
    pub output_resolvers: IndexMap<String, OutputResolver>,
    pub completed_states: alloc::vec::Vec<String>,
    pub last_received_event: Value,
    pub is_machine_completed: bool,
    pub delayed_events: Vec<DelayedEvent>,
    pub delays: IndexMap<String, DelayFunction>,
    pub actions: IndexMap<String, ActionFunction>,
    pub actions_with_effects: IndexMap<String, ActionWithEffectsFunction>,
    pub logger_fn: Option<LoggerFunction>,
    pub last_error: Option<String>,
}

enum MergeWorkFrame<'a> {
    Compound {
        key: String,
    },
    Parallel {
        iter: indexmap::map::Iter<'a, String, StateValue>,
        old_map: &'a IndexMap<String, StateValue>,
        merged_map: IndexMap<String, StateValue>,
        current_path: String,
        pending_key: Option<String>,
    },
    ParallelWithCompound {
        merged_map: IndexMap<String, StateValue>,
        key: String,
    },
}

struct ResolveTask {
    parts: Vec<String>,
    depth: usize,
}

enum ResolveFrame {
    CompoundFallback {
        name: String,
    },
    CompoundChain {
        names: Vec<String>,
    },
    ParallelRegions {
        compound_name: String,
        regions: IndexMap<String, StateValue>,
        pending_regions: Vec<(String, Vec<String>, usize)>,
        current_region: Option<String>,
    },
    HistoryFallback {
        fallback_parts: Vec<String>,
        history_actions: Vec<Action>,
    },
}

struct SelectedTransition<'a> {
    source_path: String,
    t: &'a TransitionDefinition,
    lowest_domain_depth: usize,
    is_targetless: bool,
    reenters_root: bool,
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

        let state_outputs = IndexMap::new();
        let machine_output = definition.output.clone();

        let input = definition.input.clone();
        let mut machine = Self {
            definition,
            current_state: initial_state,
            context,
            input,
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
            state_outputs,
            machine_output,
            output_resolvers: IndexMap::new(),
            completed_states: alloc::vec::Vec::new(),
            last_received_event: Value::Null,
            is_machine_completed: false,
            delayed_events: Vec::new(),
            delays: IndexMap::new(),
            actions: IndexMap::new(),
            actions_with_effects: IndexMap::new(),
            logger_fn: None,
            last_error: None,
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

    pub fn register_action(&mut self, name: &str, action: ActionFunction) {
        self.actions.insert(name.to_string(), action);
    }

    pub fn register_action_with_effects(&mut self, name: &str, action: ActionWithEffectsFunction) {
        self.actions_with_effects.insert(name.to_string(), action);
    }

    pub fn register_delay(&mut self, name: &str, delay: DelayFunction) {
        self.delays.insert(name.to_string(), delay);
    }

    pub fn register_logger(&mut self, logger: LoggerFunction) {
        self.logger_fn = Some(logger);
    }

    pub fn cancel_delayed_event(&mut self, send_id: &str) {
        self.delayed_events.retain(|e| e.id.as_deref() != Some(send_id));
    }

    pub fn resolve_delay(&self, delay: &Delay, event_payload: &Value) -> u64 {
        match delay {
            Delay::Literal(ms) => *ms,
            Delay::Named(name) => {
                if let Some(f) = self.delays.get(name) {
                    f(&self.context, event_payload)
                } else {
                    name.parse::<u64>().unwrap_or_default()
                }
            }
        }
    }

    pub fn transition_pure_with_context_and_history(
        &self,
        state: &StateValue,
        context: &Value,
        history: Option<&IndexMap<String, StateValue>>,
        event_type: &str,
        event_payload: &Value,
    ) -> (StateValue, Value, Vec<Action>, IndexMap<String, StateValue>) {
        let mut m = self.clone();
        m.current_state = state.clone();
        m.context = context.clone();
        if let Some(hist) = history {
            m.history_value = hist.clone();
        }
        m.is_started = true;
        m.is_initialized = true;
        m.status = MachineStatus::Active;
        m.revive_timers();
        m.refresh_status();

        let mut collected_actions = Vec::new();
        let guards = m.guards.clone();
        let actions = m.actions.clone();

        let _ = m.transition_internal(
            event_type,
            event_payload,
            &guards,
            &actions,
            0,
            None,
            Some(&mut collected_actions),
        );
        let _ = m.drain_internal_queue_with_collector(&guards, &actions, Some(&mut collected_actions));
        m.refresh_status();

        (m.current_state, m.context, collected_actions, m.history_value)
    }

    pub fn transition_pure_with_context(
        &self,
        state: &StateValue,
        context: &Value,
        event_type: &str,
        event_payload: &Value,
    ) -> (StateValue, Value, Vec<Action>) {
        let (next_state, next_ctx, actions, _) = self.transition_pure_with_context_and_history(
            state,
            context,
            None,
            event_type,
            event_payload,
        );
        (next_state, next_ctx, actions)
    }

    pub fn transition_pure(
        &self,
        state: &StateValue,
        event: &str,
        payload: &Value,
    ) -> (StateValue, Vec<Action>) {
        let (next_state, _, actions) = self.transition_pure_with_context(state, &self.context, event, payload);
        (next_state, actions)
    }

    pub fn initial_transition_pure(
        &self,
        input: Option<&Value>,
    ) -> (StateValue, Value, Vec<Action>) {
        let mut m = self.clone();
        if let Some(inp) = input.or(self.input.as_ref()) {
            m.input = Some(inp.clone());
        }
        let mut collected_actions = Vec::new();
        let mut init_actions_collected = Vec::new();
        let initial_state = if matches!(m.definition.state_type, StateType::Parallel) {
            let mut regions = IndexMap::new();
            for (r_name, r_def) in &m.definition.states {
                if matches!(r_def.state_type, StateType::History { .. }) || r_def.history.is_some() {
                    continue;
                }
                let r_parts: Vec<&str> = vec![r_name];
                if let Some(val) = Self::resolve_state_value_from_path(
                    &m.definition,
                    &r_parts,
                    0,
                    &mut init_actions_collected,
                ) {
                    regions.insert(r_name.clone(), val);
                }
            }
            StateValue::Parallel(regions)
        } else if !m.definition.initial.is_empty() {
            Self::resolve_initial_state(
                &m.definition,
                &m.definition.initial,
                &mut init_actions_collected,
            )
            .unwrap_or_else(|| StateValue::Atomic(m.definition.initial.clone()))
        } else if m.definition.states.is_empty() {
            StateValue::Parallel(IndexMap::new())
        } else {
            StateValue::Atomic(String::new())
        };
        m.current_state = initial_state;
        m.is_started = true;
        m.is_initialized = true;
        m.status = MachineStatus::Active;

        let mut init_map = IndexMap::new();
        init_map.insert("type".to_string(), Value::String("xstate.init".to_string()));
        if let Some(ref inp) = m.input {
            init_map.insert("input".to_string(), inp.clone());
        }
        let init_event = Value::Object(init_map);
        m.last_received_event = init_event.clone();

        let guards = m.guards.clone();
        let actions = m.actions.clone();

        // 1. Root entry actions
        for act in &m.definition.entry {
            match act {
                Action::Assign(assign) => {
                    if let Value::Object(new_vals) = &assign.assignment {
                        if let Value::Object(ctx_map) = &mut m.context {
                            for (k, v) in new_vals {
                                ctx_map.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
                Action::Raise(p) => {
                    m.internal_queue.push_back(p.clone());
                    collected_actions.push(act.clone());
                }
                Action::Named(name) => {
                    if name.starts_with("auto_action_assign_") {
                        if let Some(f) = actions.get(name) {
                            f(&mut m.context, &init_event, &None);
                        }
                    } else {
                        collected_actions.push(act.clone());
                    }
                }
                _ => {
                    collected_actions.push(act.clone());
                }
            }
        }

        // Root after delays -> Action::DelayedRaise
        for (delay, _) in &m.definition.after {
            let delay_ms = match delay {
                Delay::Literal(ms) => *ms,
                Delay::Named(name) => {
                    if let Some(f) = m.delays.get(name) {
                        f(&m.context, &init_event)
                    } else {
                        name.parse::<u64>().unwrap_or_default()
                    }
                }
            };
            let mut evt_map = IndexMap::new();
            evt_map.insert("type".to_string(), Value::String(format!("xstate.after.{}.{}", delay_ms, m.definition.id)));
            collected_actions.push(Action::DelayedRaise(DelayedRaiseAction {
                event: Value::Object(evt_map),
                delay: Delay::Literal(delay_ms),
                id: None,
            }));
        }

        // Root initial actions
        for act in &m.definition.initial_actions {
            match act {
                Action::Assign(assign) => {
                    if let Value::Object(new_vals) = &assign.assignment {
                        if let Value::Object(ctx_map) = &mut m.context {
                            for (k, v) in new_vals {
                                ctx_map.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
                Action::Raise(p) => {
                    m.internal_queue.push_back(p.clone());
                    collected_actions.push(act.clone());
                }
                Action::Named(name) => {
                    if name.starts_with("auto_action_assign_") {
                        if let Some(f) = actions.get(name) {
                            f(&mut m.context, &init_event, &None);
                        }
                    } else {
                        collected_actions.push(act.clone());
                    }
                }
                _ => {
                    collected_actions.push(act.clone());
                }
            }
        }

        // 2. Active paths entry actions and after delays
        let mut seen_paths = Vec::new();
        for path in m.current_state.get_active_paths() {
            let parts: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
            for i in 1..=parts.len() {
                let curr_path = parts[0..i].join(".");
                if !seen_paths.contains(&curr_path) {
                    if let Some(def) = Self::get_state_def_recursive(&m.definition.states, &curr_path) {
                        for act in &def.entry {
                            match act {
                                Action::Assign(assign) => {
                                    if let Value::Object(new_vals) = &assign.assignment {
                                        if let Value::Object(ctx_map) = &mut m.context {
                                            for (k, v) in new_vals {
                                                ctx_map.insert(k.clone(), v.clone());
                                            }
                                        }
                                    }
                                }
                                Action::Raise(p) => {
                                    m.internal_queue.push_back(p.clone());
                                    collected_actions.push(act.clone());
                                }
                                Action::Named(name) => {
                                    if name.starts_with("auto_action_assign_") {
                                        if let Some(f) = actions.get(name) {
                                            f(&mut m.context, &init_event, &None);
                                        }
                                    } else {
                                        collected_actions.push(act.clone());
                                    }
                                }
                                _ => {
                                    collected_actions.push(act.clone());
                                }
                            }
                        }

                        // State after delays -> Action::DelayedRaise
                        for (delay, _) in &def.after {
                            let mut evt_map = IndexMap::new();
                            let evt_type = format!("xstate.after.{}.{}.{}", delay, m.definition.id, curr_path);
                            evt_map.insert("type".to_string(), Value::String(evt_type));
                            collected_actions.push(Action::DelayedRaise(DelayedRaiseAction {
                                event: Value::Object(evt_map),
                                delay: Delay::Literal(*delay),
                                id: None,
                            }));
                        }
                        for (delay_name, _) in &def.named_after {
                            let delay_ms = m.resolve_delay(&Delay::Named(delay_name.clone()), &init_event);
                            let mut evt_map = IndexMap::new();
                            let evt_type = format!("xstate.after.{}.{}.{}", delay_ms, m.definition.id, curr_path);
                            evt_map.insert("type".to_string(), Value::String(evt_type));
                            collected_actions.push(Action::DelayedRaise(DelayedRaiseAction {
                                event: Value::Object(evt_map),
                                delay: Delay::Literal(delay_ms),
                                id: None,
                            }));
                        }

                        if matches!(def.state_type, StateType::Compound) {
                            for act in &def.initial_actions {
                                match act {
                                    Action::Assign(assign) => {
                                        if let Value::Object(new_vals) = &assign.assignment {
                                            if let Value::Object(ctx_map) = &mut m.context {
                                                for (k, v) in new_vals {
                                                    ctx_map.insert(k.clone(), v.clone());
                                                }
                                            }
                                        }
                                    }
                                    Action::Raise(p) => {
                                        m.internal_queue.push_back(p.clone());
                                    }
                                    Action::Named(name) => {
                                        if name.starts_with("auto_action_assign_") {
                                            if let Some(f) = actions.get(name) {
                                                f(&mut m.context, &init_event, &None);
                                            }
                                        } else {
                                            collected_actions.push(act.clone());
                                        }
                                    }
                                    _ => {
                                        collected_actions.push(act.clone());
                                    }
                                }
                            }
                        }
                    }
                    seen_paths.push(curr_path);
                }
            }
        }

        for act in &init_actions_collected {
            match act {
                Action::Assign(assign) => {
                    if let Value::Object(new_vals) = &assign.assignment {
                        if let Value::Object(ctx_map) = &mut m.context {
                            for (k, v) in new_vals {
                                ctx_map.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
                Action::Raise(p) => {
                    m.internal_queue.push_back(p.clone());
                    collected_actions.push(act.clone());
                }
                Action::Named(name) => {
                    if name.starts_with("auto_action_assign_") {
                        if let Some(f) = actions.get(name) {
                            f(&mut m.context, &init_event, &None);
                        }
                    } else {
                        collected_actions.push(act.clone());
                    }
                }
                _ => {
                    collected_actions.push(act.clone());
                }
            }
        }

        // Settle eventless transitions
        let _ = m.transition_internal("", &init_event, &guards, &actions, 0, None, Some(&mut collected_actions));
        let _ = m.drain_internal_queue_with_collector(&guards, &actions, Some(&mut collected_actions));
        m.refresh_status();

        (m.current_state, m.context, collected_actions)
    }

    pub fn evaluate_guard_ast(
        &self,
        root_guard: &TransitionGuard,
        event_payload: &Value,
        guards: &IndexMap<String, GuardFunction>,
    ) -> Result<bool, String> {
        enum GuardEvalFrame<'a> {
            And {
                children: &'a [TransitionGuard],
                next_idx: usize,
            },
            Or {
                children: &'a [TransitionGuard],
                next_idx: usize,
            },
            Not,
        }

        let mut stack: Vec<GuardEvalFrame> = Vec::new();
        let mut current_guard: Option<&TransitionGuard> = Some(root_guard);
        let mut last_result: Option<bool> = None;

        loop {
            if let Some(guard) = current_guard.take() {
                match guard {
                    TransitionGuard::Name(n) => {
                        let f = guards.get(n).or_else(|| self.guards.get(n));
                        match f {
                            Some(func) => {
                                let res = func(&self.context, event_payload, &None);
                                last_result = Some(res);
                            }
                            None => return Err(n.clone()),
                        }
                    }
                    TransitionGuard::Object(o) => {
                        let f = guards.get(&o.guard_type).or_else(|| self.guards.get(&o.guard_type));
                        match f {
                            Some(func) => {
                                let res = func(&self.context, event_payload, &o.params);
                                last_result = Some(res);
                            }
                            None => return Err(o.guard_type.clone()),
                        }
                    }
                    TransitionGuard::StateIn(val) => {
                        let matched = match val {
                            Value::String(s) => {
                                if let Some(stripped) = s.strip_prefix('#') {
                                    if let Some(path) = self.definition.id_map.get(stripped) {
                                        self.current_state.matches(path)
                                    } else {
                                        false
                                    }
                                } else {
                                    self.current_state.matches(s)
                                }
                            }
                            Value::Object(_) => {
                                let sv = value_to_state_value(val.clone());
                                self.current_state.matches_state_value(&sv)
                            }
                            _ => false,
                        };
                        last_result = Some(matched);
                    }
                    TransitionGuard::Not(inner) => {
                        stack.push(GuardEvalFrame::Not);
                        current_guard = Some(inner.as_ref());
                    }
                    TransitionGuard::And(children) => {
                        if children.is_empty() {
                            last_result = Some(true);
                        } else {
                            stack.push(GuardEvalFrame::And {
                                children: children.as_slice(),
                                next_idx: 1,
                            });
                            current_guard = Some(&children[0]);
                        }
                    }
                    TransitionGuard::Or(children) => {
                        if children.is_empty() {
                            last_result = Some(false);
                        } else {
                            stack.push(GuardEvalFrame::Or {
                                children: children.as_slice(),
                                next_idx: 1,
                            });
                            current_guard = Some(&children[0]);
                        }
                    }
                }
            } else if let Some(res) = last_result.take() {
                if let Some(top) = stack.last_mut() {
                    match top {
                        GuardEvalFrame::Not => {
                            stack.pop();
                            last_result = Some(!res);
                        }
                        GuardEvalFrame::And { children, next_idx } => {
                            if !res {
                                stack.pop();
                                last_result = Some(false);
                            } else if *next_idx < children.len() {
                                let next_guard = &children[*next_idx];
                                *next_idx += 1;
                                current_guard = Some(next_guard);
                            } else {
                                stack.pop();
                                last_result = Some(true);
                            }
                        }
                        GuardEvalFrame::Or { children, next_idx } => {
                            if res {
                                stack.pop();
                                last_result = Some(true);
                            } else if *next_idx < children.len() {
                                let next_guard = &children[*next_idx];
                                *next_idx += 1;
                                current_guard = Some(next_guard);
                            } else {
                                stack.pop();
                                last_result = Some(false);
                            }
                        }
                    }
                } else {
                    return Ok(res);
                }
            } else {
                break;
            }
        }
        Ok(last_result.unwrap_or(true))
    }

    pub fn format_guard_error(
        &self,
        missing_guard: &str,
        event_type: &str,
        current_path: &str,
    ) -> String {
        let machine_prefix = if self.definition.id.is_empty() {
            "(machine)"
        } else {
            &self.definition.id
        };
        let custom_id = Self::get_state_def_recursive(&self.definition.states, current_path)
            .and_then(|d| d.id.clone());
        let state_node_id = if let Some(cid) = custom_id {
            cid
        } else if current_path.is_empty() {
            machine_prefix.to_string()
        } else {
            format!("{}.{}", machine_prefix, current_path)
        };
        format!(
            "Unable to evaluate guard '{}' in transition for event '{}' in state node '{}':\nGuard '{}' is not implemented.'.",
            missing_guard, event_type, state_node_id, missing_guard
        )
    }

    pub fn evaluate_guard(
        &mut self,
        guard: &TransitionGuard,
        event_payload: &Value,
        guards: &IndexMap<String, GuardFunction>,
        event_type: &str,
        current_path: &str,
    ) -> bool {
        match self.evaluate_guard_ast(guard, event_payload, guards) {
            Ok(result) => result,
            Err(missing_guard) => {
                let err_msg = self.format_guard_error(&missing_guard, event_type, current_path);
                self.last_error = Some(err_msg);
                false
            }
        }
    }

    pub fn register_output_resolver(&mut self, state_path: &str, resolver: OutputResolver) {
        self.output_resolvers.insert(state_path.to_string(), resolver);
    }

    pub fn is_state_done(&self, root_path: &str) -> bool {
        let mut stack = alloc::vec![root_path.to_string()];
        let active_paths = self.current_state.get_active_paths();

        while let Some(path) = stack.pop() {
            let def = match Self::get_state_def_recursive(&self.definition.states, &path) {
                Some(d) => d,
                None => return false,
            };
            match def.state_type {
                StateType::Final => {
                    if !active_paths.iter().any(|p| p == &path) {
                        return false;
                    }
                }
                StateType::Compound => {
                    let prefix = format!("{}.", path);
                    let mut has_final_direct_child = false;
                    for active in &active_paths {
                        if let Some(rest) = active.strip_prefix(&prefix) {
                            let direct_child = rest.split('.').next().unwrap_or("");
                            if let Some(child_def) = def.states.get(direct_child) {
                                if matches!(child_def.state_type, StateType::Final) {
                                    has_final_direct_child = true;
                                    break;
                                }
                            }
                        }
                    }
                    if !has_final_direct_child {
                        return false;
                    }
                }
                StateType::Parallel => {
                    if def.states.is_empty() {
                        return false;
                    }
                    for (r_name, r_def) in &def.states {
                        if matches!(r_def.state_type, StateType::History { .. }) || r_def.history.is_some() {
                            continue;
                        }
                        stack.push(format!("{}.{}", path, r_name));
                    }
                }
                StateType::Atomic | StateType::History { .. } => return false,
            }
        }
        true
    }

    pub fn compute_is_done(&self) -> bool {
        self.is_root_done()
    }

    pub fn is_root_done(&self) -> bool {
        match self.definition.state_type {
            StateType::Final => true,
            StateType::Parallel => {
                if self.definition.states.is_empty() {
                    return false;
                }
                for (r_name, r_def) in &self.definition.states {
                    if matches!(r_def.state_type, StateType::History { .. }) || r_def.history.is_some() {
                        continue;
                    }
                    if !self.is_state_done(r_name) {
                        return false;
                    }
                }
                true
            }
            StateType::Compound => {
                let active_paths = self.current_state.get_active_paths();
                if active_paths.is_empty() {
                    return false;
                }
                active_paths.iter().all(|path| {
                    let top_child = path.split('.').next().unwrap_or("");
                    self.definition
                        .states
                        .get(top_child)
                        .map(|top_def| matches!(top_def.state_type, StateType::Final))
                        .unwrap_or(false)
                })
            }
            StateType::Atomic | StateType::History { .. } => false,
        }
    }

    pub fn get_child_final_output(&self, parent_path: &str) -> Option<Value> {
        let prefix = format!("{}.", parent_path);
        for active in self.current_state.get_active_paths() {
            if let Some(rest) = active.strip_prefix(&prefix) {
                let direct_child = rest.split('.').next().unwrap_or("");
                let child_path = format!("{}.{}", parent_path, direct_child);
                if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &child_path) {
                    if matches!(def.state_type, StateType::Final) {
                        if let Some(res) = self.output_resolvers.get(&child_path) {
                            return Some(res(&self.context, &self.last_received_event));
                        }
                        if let Some(out) = self.state_outputs.get(&child_path).or(def.output.as_ref()) {
                            return Some(out.clone());
                        }
                        return None;
                    }
                }
            }
        }
        None
    }

    pub fn check_state_completions(&mut self) -> bool {
        if self.status == MachineStatus::Stopped {
            return false;
        }

        let active_leaves = self.current_state.get_active_paths();
        if active_leaves.is_empty() {
            return false;
        }

        let mut candidate_paths: Vec<String> = Vec::new();
        for leaf in &active_leaves {
            let mut parts: Vec<&str> = leaf.split('.').collect();
            while parts.len() > 1 {
                parts.pop();
                let parent_path = parts.join(".");
                if !candidate_paths.contains(&parent_path) {
                    candidate_paths.push(parent_path);
                }
            }
        }
        candidate_paths.sort_by_key(|b| core::cmp::Reverse(b.split('.').count()));

        let mut emitted = false;
        for path in candidate_paths {
            if self.completed_states.contains(&path) {
                continue;
            }

            if self.is_state_done(&path) {
                self.completed_states.push(path.clone());

                let (state_id, final_output) = if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &path) {
                    let id = def.id.clone().unwrap_or_else(|| format!("{}.{}", self.definition.id, path));
                    let out = self.get_child_final_output(&path);
                    (id, out)
                } else {
                    (format!("{}.{}", self.definition.id, path), None)
                };

                let mut evt_map = IndexMap::new();
                evt_map.insert("type".to_string(), Value::String(format!("xstate.done.state.{}", state_id)));
                if let Some(out) = final_output {
                    evt_map.insert("output".to_string(), out);
                }
                self.internal_queue.push_back(Value::Object(evt_map));
                emitted = true;
            }
        }

        emitted
    }

    pub fn get_output(&self) -> Option<Value> {
        if self.status != MachineStatus::Done {
            return None;
        }
        if let Some(res) = self.output_resolvers.get("") {
            return Some(res(&self.context, &self.last_received_event));
        }
        if let Some(out) = self.machine_output.as_ref().or(self.definition.output.as_ref()) {
            return Some(out.clone());
        }
        for leaf in self.current_state.get_active_paths() {
            if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &leaf) {
                if matches!(def.state_type, StateType::Final) {
                    if let Some(res) = self.output_resolvers.get(&leaf) {
                        return Some(res(&self.context, &self.last_received_event));
                    }
                    if let Some(out) = self.state_outputs.get(&leaf).or(def.output.as_ref()) {
                        return Some(out.clone());
                    }
                }
            }
        }
        None
    }

    fn handle_machine_completion(
        &mut self,
        event_payload: &Value,
        actions: &IndexMap<String, ActionFunction>,
        guards: &IndexMap<String, GuardFunction>,
        mut action_collector: Option<&mut Vec<Action>>,
    ) {
        if self.compute_is_done() && !self.is_machine_completed && self.status != MachineStatus::Stopped {
            self.is_machine_completed = true;
            self.status = MachineStatus::Done;
            let active_leaves = self.current_state.get_active_paths();
            let mut doc_order = Vec::new();
            for leaf in &active_leaves {
                let parts: Vec<&str> = leaf.split('.').filter(|s| !s.is_empty()).collect();
                for i in 1..=parts.len() {
                    let p = parts[0..i].join(".");
                    if !doc_order.contains(&p) {
                        doc_order.push(p);
                    }
                }
            }
            let now = self.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(0);
            for path in doc_order.iter().rev() {
                if let Some(def) = Self::get_state_def_recursive(&self.definition.states, path) {
                    for act in &def.exit {
                        if let Some(ref mut col) = action_collector {
                            match act {
                                Action::Assign(assign) => {
                                    if let Value::Object(new_vals) = &assign.assignment {
                                        if let Value::Object(ctx_map) = &mut self.context {
                                            for (k, v) in new_vals {
                                                ctx_map.insert(k.clone(), v.clone());
                                            }
                                        }
                                    }
                                }
                                Action::Raise(payload) => {
                                    self.internal_queue.push_back(payload.clone());
                                }
                                Action::Named(name) => {
                                    if name.starts_with("auto_action_assign_") {
                                        if let Some(f) = actions.get(name) {
                                            f(&mut self.context, event_payload, &None);
                                        }
                                    } else {
                                        col.push(act.clone());
                                    }
                                }
                                Action::DelayedRaise(_) | Action::Cancel(_) | Action::Log(_) | Action::Generic(_) => {
                                    col.push(act.clone());
                                }
                                Action::RaiseInvalid(msg) => {
                                    self.last_error = Some(msg.clone());
                                }
                            }
                        } else {
                            Self::execute_action_with_effects(
                                &mut self.context,
                                act,
                                event_payload,
                                actions,
                                &self.actions_with_effects,
                                guards,
                                &mut self.internal_queue,
                                &mut self.delayed_events,
                                &self.delays,
                                &self.current_state,
                                now,
                                &self.logger_fn,
                                &mut self.last_error,
                            );
                        }
                    }
                }
            }
            for act in &self.definition.exit {
                if let Some(ref mut col) = action_collector {
                    match act {
                        Action::Assign(assign) => {
                            if let Value::Object(new_vals) = &assign.assignment {
                                if let Value::Object(ctx_map) = &mut self.context {
                                    for (k, v) in new_vals {
                                        ctx_map.insert(k.clone(), v.clone());
                                    }
                                }
                            }
                        }
                        Action::Raise(payload) => {
                            self.internal_queue.push_back(payload.clone());
                        }
                        Action::Named(name) => {
                            if name.starts_with("auto_action_assign_") {
                                if let Some(f) = actions.get(name) {
                                    f(&mut self.context, event_payload, &None);
                                }
                            } else {
                                col.push(act.clone());
                            }
                        }
                        Action::DelayedRaise(_) | Action::Cancel(_) | Action::Log(_) | Action::Generic(_) => {
                            col.push(act.clone());
                        }
                        Action::RaiseInvalid(msg) => {
                            self.last_error = Some(msg.clone());
                        }
                    }
                } else {
                    Self::execute_action_with_effects(
                        &mut self.context,
                        act,
                        event_payload,
                        actions,
                        &self.actions_with_effects,
                        guards,
                        &mut self.internal_queue,
                        &mut self.delayed_events,
                        &self.delays,
                        &self.current_state,
                        now,
                        &self.logger_fn,
                        &mut self.last_error,
                    );
                }
            }
            self.active_timers.clear();
            self.internal_queue.clear();
            self.external_queue.clear();
        }
    }

    pub fn refresh_status(&mut self) {
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
        if self.definition.tags.iter().any(|t| t == tag) {
            return true;
        }
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
        for tag in &self.definition.tags {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }
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

    pub fn get_meta(&self) -> Value {
        let mut map = IndexMap::new();
        let machine_id = if self.definition.id.is_empty() {
            "(machine)"
        } else {
            &self.definition.id
        };

        if let Some(ref root_meta) = self.definition.meta {
            map.insert(machine_id.to_string(), root_meta.clone());
        }

        let active_paths = self.current_state.get_active_paths();
        for path in &active_paths {
            let parts: Vec<&str> = path.split('.').collect();
            let mut prefix = String::new();
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    prefix.push('.');
                }
                prefix.push_str(part);

                if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &prefix) {
                    if let Some(ref meta) = def.meta {
                        let key = if let Some(ref id) = def.id {
                            id.clone()
                        } else {
                            format!("{}.{}", machine_id, prefix)
                        };
                        map.insert(key, meta.clone());
                    }
                }
            }
        }

        Value::Object(map)
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

    /// Parse a `Value::Object` (e.g. from `historyValue` or `history`) into an `IndexMap<String, StateValue>`.
    /// Resolves target state IDs back to parent paths via `definition.id_map` and merges parallel regions iteratively.
    pub fn parse_history_value(&self, hist_val: &Value) -> IndexMap<String, StateValue> {
        let mut history_value = IndexMap::new();
        if let Some(hist_map) = hist_val.as_object() {
            let mut paths_by_parent: IndexMap<String, Vec<String>> = IndexMap::new();

            for (k, v) in hist_map {
                let parent_path = if let Some(history_node_path) = self.definition.id_map.get(k) {
                    if let Some(idx) = history_node_path.rfind('.') {
                        history_node_path[0..idx].to_string()
                    } else {
                        alloc::string::String::new()
                    }
                } else if let Some(idx) = k.rfind('.') {
                    k[0..idx].to_string()
                } else {
                    k.clone()
                };

                match v {
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
                        if !s.is_empty() {
                            let entry = paths_by_parent.entry(parent_path).or_default();
                            if !entry.contains(s) {
                                entry.push(s.clone());
                            }
                        }
                    }
                    Value::Object(_) => {
                        history_value.insert(parent_path, value_to_state_value(v.clone()));
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
                    history_value.insert(parent_path, state_val);
                }
            }
        }
        history_value
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
            self.history_value = self.parse_history_value(&h_val);
        }

        self.revive_timers();
        self.refresh_status();
        if self.status != MachineStatus::Stopped {
            self.is_started = true;
        }
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
                    let is_allowed = self.evaluate_guard_ast(guard, event_payload, guards).unwrap_or(false);
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
        let state_deadline = self.active_timers.iter().map(|(_, deadline, _)| *deadline).min();
        let event_deadline = self.delayed_events.iter().map(|e| e.deadline).min();
        match (state_deadline, event_deadline) {
            (Some(d1), Some(d2)) => Some(core::cmp::min(d1, d2)),
            (Some(d1), None) => Some(d1),
            (None, Some(d2)) => Some(d2),
            (None, None) => None,
        }
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
        for (k, v) in actions {
            self.actions.insert(k.clone(), v.clone());
        }
        self.is_started = true;

        if self.is_initialized {
            self.revive_timers();
            self.is_processing = true;
            let mut init_map = IndexMap::new();
            init_map.insert("type".to_string(), Value::String("xstate.init".to_string()));
            if let Some(ref inp) = self.input {
                init_map.insert("input".to_string(), inp.clone());
            }
            let init_event = Value::Object(init_map);
            self.last_received_event = init_event.clone();
            let _ = self.transition_internal("", &init_event, guards, actions, 0, None, None);
            let _ = self.drain_internal_queue(guards, actions);
            self.is_processing = false;
            let _ = self.drain_external_queue(guards, actions);
            self.refresh_status();
            self.handle_machine_completion(&init_event, actions, guards, None);
            return;
        }

        self.is_initialized = true;
        self.is_processing = true;

        let mut init_map = IndexMap::new();
        init_map.insert("type".to_string(), Value::String("xstate.init".to_string()));
        if let Some(ref inp) = self.input {
            init_map.insert("input".to_string(), inp.clone());
        }
        let init_event = Value::Object(init_map);
        self.last_received_event = init_event.clone();

        let _ = core::mem::take(&mut self.initial_actions);
        let mut init_actions_to_run = self.definition.entry.clone();
        let now = self.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(0);
        for (delay, t_defs) in &self.definition.after {
            let delay_ms = self.resolve_delay(delay, &init_event);
            self.active_timers
                .push(("".to_string(), now + delay_ms, t_defs.clone()));
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
                        for (delay_name, t_defs) in &def.named_after {
                            let delay_ms = self.resolve_delay(&Delay::Named(delay_name.clone()), &init_event);
                            self.active_timers.push((
                                curr_path.clone(),
                                now + delay_ms,
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
            Self::execute_action_with_effects(
                &mut self.context,
                action,
                &init_event,
                actions,
                &self.actions_with_effects,
                guards,
                &mut self.internal_queue,
                &mut self.delayed_events,
                &self.delays,
                &self.current_state,
                now,
                &self.logger_fn,
                &mut self.last_error,
            );
        }

        self.check_state_completions();
        if self.is_root_done() {
            self.refresh_status();
            self.handle_machine_completion(&init_event, actions, guards, None);
            self.is_processing = false;
            return;
        }

        // 1. Settle initial eventless (`always`) transitions first
        let _ = self.transition_internal("", &init_event, guards, actions, 0, None, None);

        // 2. Drain internal events raised during initial entry or initial `always` transitions
        let _ = self.drain_internal_queue(guards, actions);

        // 3. Complete initialization macrostep and drain any reentrant external events
        self.is_processing = false;
        let _ = self.drain_external_queue(guards, actions);
        self.refresh_status();
        self.handle_machine_completion(&init_event, actions, guards, None);
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
            if path.is_empty() {
                for (delay, t_defs) in &self.definition.after {
                    let duration = self.resolve_delay(delay, &Value::Null);
                    self.active_timers.push(("".to_string(), now + duration, t_defs.clone()));
                }
            } else if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &path) {
                for (delay, t_defs) in &def.after {
                    self.active_timers.push((path.clone(), now + delay, t_defs.clone()));
                }
                for (delay_name, t_defs) in &def.named_after {
                    let duration = self.resolve_delay(&Delay::Named(delay_name.clone()), &Value::Null);
                    self.active_timers.push((path.clone(), now + duration, t_defs.clone()));
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
        let resolved = if id_key.is_empty() || initial == "#" || id_key == definition.id {
            ""
        } else {
            definition.id_map.get(id_key).map(|s| s.as_str()).unwrap_or(initial)
        };
        if resolved.is_empty() {
            if matches!(definition.state_type, StateType::Parallel) {
                let mut r_map = IndexMap::new();
                for (r_name, r_def) in &definition.states {
                    if matches!(r_def.state_type, StateType::History { .. }) || r_def.history.is_some() {
                        continue;
                    }
                    if let Some(val) = Self::resolve_initial_state(definition, r_name, collected_actions) {
                        r_map.insert(r_name.clone(), val);
                    }
                }
                return Some(StateValue::Parallel(r_map));
            }
            if !definition.initial.is_empty() && definition.initial != initial && definition.initial != "#" {
                return Self::resolve_initial_state(definition, &definition.initial, collected_actions);
            }
            return None;
        }
        let parts: Vec<&str> = resolved.split('.').collect();
        Self::resolve_state_value_from_path(definition, &parts, 0, collected_actions)
    }


    pub fn merge_state_value(
        old: &StateValue,
        new: &StateValue,
        current_path: &str,
        target_path: &str,
    ) -> StateValue {
        let mut stack: Vec<MergeWorkFrame<'_>> = Vec::new();
        let mut completed: Option<StateValue> = None;
        let mut next_pair: Option<(&StateValue, &StateValue, String)> =
            Some((old, new, current_path.to_string()));

        loop {
            if let Some(comp) = completed.take() {
                if let Some(top) = stack.last_mut() {
                    match top {
                        MergeWorkFrame::Compound { key } => {
                            let k = core::mem::take(key);
                            stack.pop();
                            completed = Some(StateValue::Compound(k, Box::new(comp)));
                        }
                        MergeWorkFrame::Parallel {
                            iter,
                            old_map,
                            merged_map,
                            current_path,
                            pending_key,
                        } => {
                            if let Some(k) = pending_key.take() {
                                merged_map.insert(k, comp);
                            }
                            let mut advanced = false;
                            for (k, new_sub_val) in iter.by_ref() {
                                let sub_path = if current_path.is_empty() {
                                    k.clone()
                                } else {
                                    format!("{}.{}", current_path, k)
                                };

                                let is_targeted = if target_path.len() >= sub_path.len() {
                                    target_path.starts_with(&sub_path)
                                        && (target_path.len() == sub_path.len()
                                            || target_path.as_bytes().get(sub_path.len()) == Some(&b'.'))
                                } else {
                                    true
                                };

                                if is_targeted {
                                    if let Some(old_sub_val) = old_map.get(k) {
                                        *pending_key = Some(k.clone());
                                        let path_to_descend = match (old_sub_val, new_sub_val) {
                                            (StateValue::Compound(ok, _), StateValue::Compound(nk, _))
                                                if ok == k && nk == k =>
                                            {
                                                current_path.clone()
                                            }
                                            _ => sub_path,
                                        };
                                        next_pair = Some((old_sub_val, new_sub_val, path_to_descend));
                                        advanced = true;
                                        break;
                                    } else {
                                        merged_map.insert(k.clone(), new_sub_val.clone());
                                    }
                                } else if let Some(old_sub_val) = old_map.get(k) {
                                    merged_map.insert(k.clone(), old_sub_val.clone());
                                } else {
                                    merged_map.insert(k.clone(), new_sub_val.clone());
                                }
                            }

                            if !advanced {
                                let finished_map = core::mem::take(merged_map);
                                stack.pop();
                                completed = Some(StateValue::Parallel(finished_map));
                            }
                        }
                        MergeWorkFrame::ParallelWithCompound { merged_map, key } => {
                            let k = core::mem::take(key);
                            merged_map.insert(k, comp);
                            let finished_map = core::mem::take(merged_map);
                            stack.pop();
                            completed = Some(StateValue::Parallel(finished_map));
                        }
                    }
                } else {
                    return comp;
                }
            } else if let Some((curr_old, curr_new, curr_path)) = next_pair.take() {
                match (curr_old, curr_new) {
                    (StateValue::Compound(old_key, old_val), StateValue::Compound(new_key, new_val)) => {
                        if old_key == new_key {
                            let sub_path = if curr_path.is_empty() {
                                old_key.clone()
                            } else {
                                format!("{}.{}", curr_path, old_key)
                            };
                            stack.push(MergeWorkFrame::Compound {
                                key: new_key.clone(),
                            });
                            next_pair = Some((old_val.as_ref(), new_val.as_ref(), sub_path));
                        } else {
                            completed = Some(curr_new.clone());
                        }
                    }
                    (StateValue::Parallel(old_map), StateValue::Parallel(new_map)) => {
                        let mut merged_map = IndexMap::with_capacity(new_map.len());
                        let mut iter = new_map.iter();
                        let mut pending = None;
                        let mut to_descend = None;

                        for (k, new_sub_val) in iter.by_ref() {
                            let sub_path = if curr_path.is_empty() {
                                k.clone()
                            } else {
                                format!("{}.{}", curr_path, k)
                            };

                            let is_targeted = if target_path.len() >= sub_path.len() {
                                target_path.starts_with(&sub_path)
                                    && (target_path.len() == sub_path.len()
                                        || target_path.as_bytes().get(sub_path.len()) == Some(&b'.'))
                            } else {
                                true
                            };

                            if is_targeted {
                                if let Some(old_sub_val) = old_map.get(k) {
                                    pending = Some(k.clone());
                                    let path_to_descend = match (old_sub_val, new_sub_val) {
                                        (StateValue::Compound(ok, _), StateValue::Compound(nk, _))
                                            if ok == k && nk == k =>
                                        {
                                            curr_path.clone()
                                        }
                                        _ => sub_path,
                                    };
                                    to_descend = Some((old_sub_val, new_sub_val, path_to_descend));
                                    break;
                                } else {
                                    merged_map.insert(k.clone(), new_sub_val.clone());
                                }
                            } else if let Some(old_sub_val) = old_map.get(k) {
                                merged_map.insert(k.clone(), old_sub_val.clone());
                            } else {
                                merged_map.insert(k.clone(), new_sub_val.clone());
                            }
                        }

                        if let Some(descend_tuple) = to_descend {
                            stack.push(MergeWorkFrame::Parallel {
                                iter,
                                old_map,
                                merged_map,
                                current_path: curr_path,
                                pending_key: pending,
                            });
                            next_pair = Some(descend_tuple);
                        } else {
                            completed = Some(StateValue::Parallel(merged_map));
                        }
                    }
                    (StateValue::Parallel(old_map), StateValue::Compound(new_key, _)) => {
                        if let Some(old_sub_val) = old_map.get(new_key) {
                            let merged_map = old_map.clone();
                            stack.push(MergeWorkFrame::ParallelWithCompound {
                                merged_map,
                                key: new_key.clone(),
                            });
                            let path_to_pass = match old_sub_val {
                                StateValue::Compound(ok, _) if ok == new_key => curr_path,
                                _ => {
                                    if curr_path.is_empty() {
                                        new_key.clone()
                                    } else {
                                        format!("{}.{}", curr_path, new_key)
                                    }
                                }
                            };
                            next_pair = Some((old_sub_val, curr_new, path_to_pass));
                        } else {
                            completed = Some(StateValue::Parallel(old_map.clone()));
                        }
                    }
                    (StateValue::Parallel(old_map), StateValue::Atomic(new_key)) => {
                        let mut merged_map = old_map.clone();
                        if old_map.contains_key(new_key) {
                            merged_map.insert(new_key.clone(), curr_new.clone());
                        }
                        completed = Some(StateValue::Parallel(merged_map));
                    }
                    _ => {
                        completed = Some(curr_new.clone());
                    }
                }
            } else {
                return completed.unwrap_or_else(|| new.clone());
            }
        }
    }

    pub fn resolve_target(&self, source_path: &str, target: &str) -> Option<String> {
        if target.starts_with("__") {
            return None;
        }
        if target == "#" || target.is_empty() {
            return Some(String::new());
        }
        // 1. Check ID map
        let id_key = target.strip_prefix('#').unwrap_or(target);
        if id_key == self.definition.id || id_key.is_empty() {
            return Some(String::new());
        }
        if !id_key.starts_with("__") {
            if let Some(path) = self.definition.id_map.get(id_key) {
                return Some(path.clone());
            }
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

    pub fn resolve_state_value_from_path(
        definition: &MachineDefinition,
        parts: &[&str],
        depth: usize,
        collected_actions: &mut Vec<Action>,
    ) -> Option<StateValue> {
        if depth >= parts.len() {
            return None;
        }

        let mut stack: Vec<ResolveFrame> = Vec::new();
        let mut completed: Option<Option<StateValue>> = None;
        let mut current_task: Option<ResolveTask> = Some(ResolveTask {
            parts: parts.iter().map(|s| s.to_string()).collect(),
            depth,
        });

        loop {
            if let Some(comp_opt) = completed.take() {
                if let Some(top) = stack.last_mut() {
                    match top {
                        ResolveFrame::CompoundFallback { name } => {
                            let n = core::mem::take(name);
                            stack.pop();
                            match comp_opt {
                                Some(val) => {
                                    completed = Some(Some(StateValue::Compound(n, Box::new(val))));
                                }
                                None => {
                                    completed = Some(Some(StateValue::Atomic(n)));
                                }
                            }
                        }
                        ResolveFrame::CompoundChain { names } => {
                            let chain = core::mem::take(names);
                            stack.pop();
                            match comp_opt {
                                Some(val) => {
                                    let mut res = val;
                                    for n in chain.into_iter().rev() {
                                        res = StateValue::Compound(n, Box::new(res));
                                    }
                                    completed = Some(Some(res));
                                }
                                None => {
                                    completed = Some(None);
                                }
                            }
                        }
                        ResolveFrame::ParallelRegions {
                            compound_name,
                            regions,
                            pending_regions,
                            current_region,
                        } => {
                            if let Some(r_name) = current_region.take() {
                                if let Some(val) = comp_opt {
                                    regions.insert(r_name, val);
                                }
                            }
                            if let Some((next_r_name, next_parts, next_depth)) = pending_regions.pop() {
                                *current_region = Some(next_r_name);
                                current_task = Some(ResolveTask {
                                    parts: next_parts,
                                    depth: next_depth,
                                });
                            } else {
                                let c_name = core::mem::take(compound_name);
                                let r_map = core::mem::take(regions);
                                stack.pop();
                                completed = Some(Some(StateValue::Compound(
                                    c_name,
                                    Box::new(StateValue::Parallel(r_map)),
                                )));
                            }
                        }
                        ResolveFrame::HistoryFallback {
                            fallback_parts,
                            history_actions,
                        } => {
                            match comp_opt {
                                Some(val) => {
                                    collected_actions.extend(core::mem::take(history_actions));
                                    stack.pop();
                                    completed = Some(Some(val));
                                }
                                None => {
                                    let parts = core::mem::take(fallback_parts);
                                    collected_actions.extend(core::mem::take(history_actions));
                                    stack.pop();
                                    current_task = Some(ResolveTask { parts, depth: 0 });
                                }
                            }
                        }
                    }
                } else {
                    return comp_opt;
                }
            } else if let Some(task) = current_task.take() {
                let task_depth = task.depth;
                if task_depth >= task.parts.len() {
                    completed = Some(None);
                    continue;
                }
                let current_path = task.parts[0..=task_depth].join(".");
                let current_name = task.parts[task_depth].clone();
                let def = match Self::get_state_def_recursive(&definition.states, &current_path) {
                    Some(d) => d,
                    None => {
                        completed = Some(None);
                        continue;
                    }
                };

                if task_depth == task.parts.len() - 1 {
                    match def.state_type {
                        StateType::Atomic | StateType::Final => {
                            completed = Some(Some(StateValue::Atomic(current_name)));
                        }
                        StateType::History { .. } => {
                            if let Some(default_target) = &def.history_target {
                                let id_key = default_target.strip_prefix('#').unwrap_or(default_target);
                                if let Some(res) = definition.id_map.get(id_key) {
                                    collected_actions.extend(def.history_actions.clone());
                                    let t_parts: Vec<String> = res.split('.').map(|s| s.to_string()).collect();
                                    current_task = Some(ResolveTask {
                                        parts: t_parts,
                                        depth: 0,
                                    });
                                } else {
                                    let fallback_parts: Vec<String> =
                                        default_target.split('.').map(|s| s.to_string()).collect();
                                    if let Some(idx) = current_path.rfind('.') {
                                        let parent_path = &current_path[..idx];
                                        let full_target = format!("{}.{}", parent_path, default_target);
                                        let t_parts: Vec<String> =
                                            full_target.split('.').map(|s| s.to_string()).collect();

                                        stack.push(ResolveFrame::HistoryFallback {
                                            fallback_parts,
                                            history_actions: def.history_actions.clone(),
                                        });
                                        current_task = Some(ResolveTask {
                                            parts: t_parts,
                                            depth: task_depth,
                                        });
                                    } else {
                                        collected_actions.extend(def.history_actions.clone());
                                        current_task = Some(ResolveTask {
                                            parts: fallback_parts,
                                            depth: 0,
                                        });
                                    }
                                }
                            } else {
                                if let Some(idx) = current_path.rfind('.') {
                                    let parent_path = &current_path[..idx];
                                    if let Some(parent_def) =
                                        Self::get_state_def_recursive(&definition.states, parent_path)
                                    {
                                        if let Some(parent_init) = &parent_def.initial {
                                            let full_init = format!("{}.{}", parent_path, parent_init);
                                            let fi_parts: Vec<String> =
                                                full_init.split('.').map(|s| s.to_string()).collect();
                                            current_task = Some(ResolveTask {
                                                parts: fi_parts,
                                                depth: task_depth,
                                            });
                                            continue;
                                        }
                                    }
                                }
                                completed = Some(Some(StateValue::Atomic(current_name)));
                            }
                        }
                        StateType::Compound => {
                            let mut compound_names: Vec<String> = vec![current_name];
                            let mut curr_def = def;
                            let mut resolved_directly = false;

                            while let Some(init) = &curr_def.initial {
                                if let Some(next_def) = curr_def.states.get(init) {
                                    if matches!(next_def.state_type, StateType::Compound) {
                                        compound_names.push(init.clone());
                                        curr_def = next_def;
                                    } else if matches!(next_def.state_type, StateType::Atomic | StateType::Final) {
                                        let mut res = StateValue::Atomic(init.clone());
                                        for name in compound_names.into_iter().rev() {
                                            res = StateValue::Compound(name, Box::new(res));
                                        }
                                        completed = Some(Some(res));
                                        resolved_directly = true;
                                        break;
                                    } else {
                                        let full_child_path = if compound_names.len() == 1 {
                                            format!("{}.{}", current_path, init)
                                        } else {
                                            format!("{}.{}.{}", current_path, compound_names[1..].join("."), init)
                                        };
                                        let child_parts: Vec<String> =
                                            full_child_path.split('.').map(|s| s.to_string()).collect();
                                        let c_depth = child_parts.len().saturating_sub(1);
                                        stack.push(ResolveFrame::CompoundChain {
                                            names: compound_names,
                                        });
                                        current_task = Some(ResolveTask {
                                            parts: child_parts,
                                            depth: c_depth,
                                        });
                                        resolved_directly = true;
                                        break;
                                    }
                                } else {
                                    let base_path = if compound_names.len() == 1 {
                                        current_path.clone()
                                    } else {
                                        format!("{}.{}", current_path, compound_names[1..].join("."))
                                    };
                                    let clean_init = init.strip_prefix('.').unwrap_or(init);
                                    let child_path = format!("{}.{}", base_path, clean_init);
                                    let child_parts: Vec<String> =
                                        child_path.split('.').map(|s| s.to_string()).collect();
                                    let next_depth = task_depth + compound_names.len();
                                    stack.push(ResolveFrame::CompoundChain {
                                        names: compound_names,
                                    });
                                    current_task = Some(ResolveTask {
                                        parts: child_parts,
                                        depth: next_depth,
                                    });
                                    resolved_directly = true;
                                    break;
                                }
                            }

                            if !resolved_directly {
                                completed = Some(None);
                            }
                        }
                        StateType::Parallel => {
                            let mut pending_regions = Vec::new();
                            for (r_name, r_def) in def.states.iter().rev() {
                                if matches!(r_def.state_type, StateType::History { .. })
                                    || r_def.history.is_some()
                                {
                                    continue;
                                }
                                let r_path = format!("{}.{}", current_path, r_name);
                                let r_parts: Vec<String> =
                                    r_path.split('.').map(|s| s.to_string()).collect();
                                pending_regions.push((r_name.clone(), r_parts, task_depth + 1));
                            }

                            if let Some((first_r_name, first_parts, first_depth)) = pending_regions.pop() {
                                stack.push(ResolveFrame::ParallelRegions {
                                    compound_name: current_name,
                                    regions: IndexMap::new(),
                                    pending_regions,
                                    current_region: Some(first_r_name),
                                });
                                current_task = Some(ResolveTask {
                                    parts: first_parts,
                                    depth: first_depth,
                                });
                            } else {
                                completed = Some(Some(StateValue::Compound(
                                    current_name,
                                    Box::new(StateValue::Parallel(IndexMap::new())),
                                )));
                            }
                        }
                    }
                } else {
                    match def.state_type {
                        StateType::Compound => {
                            stack.push(ResolveFrame::CompoundFallback { name: current_name });
                            current_task = Some(ResolveTask {
                                parts: task.parts,
                                depth: task_depth + 1,
                            });
                        }
                        StateType::Parallel => {
                            let next_part = &task.parts[task_depth + 1];
                            let mut pending_regions = Vec::new();

                            for (r_name, r_def) in def.states.iter().rev() {
                                if matches!(r_def.state_type, StateType::History { .. })
                                    || r_def.history.is_some()
                                {
                                    continue;
                                }
                                if r_name == next_part {
                                    pending_regions.push((r_name.clone(), task.parts.clone(), task_depth + 1));
                                } else {
                                    let r_path = format!("{}.{}", current_path, r_name);
                                    let r_parts: Vec<String> =
                                        r_path.split('.').map(|s| s.to_string()).collect();
                                    pending_regions.push((r_name.clone(), r_parts, task_depth + 1));
                                }
                            }

                            if let Some((first_r_name, first_parts, first_depth)) = pending_regions.pop() {
                                stack.push(ResolveFrame::ParallelRegions {
                                    compound_name: current_name,
                                    regions: IndexMap::new(),
                                    pending_regions,
                                    current_region: Some(first_r_name),
                                });
                                current_task = Some(ResolveTask {
                                    parts: first_parts,
                                    depth: first_depth,
                                });
                            } else {
                                completed = Some(Some(StateValue::Compound(
                                    current_name,
                                    Box::new(StateValue::Parallel(IndexMap::new())),
                                )));
                            }
                        }
                        _ => {
                            completed = Some(None);
                        }
                    }
                }
            } else {
                return completed.and_then(|c| c);
            }
        }
    }







    pub fn get_state_def_recursive<'a>(states: &'a IndexMap<String, StateDefinition>, path: &str) -> Option<&'a StateDefinition> {
        if path.is_empty() {
            return None;
        }
        let mut mp = states;
        let mut current_def = None;
        for p in path.split('.') {
            if let Some(d) = mp.get(p) {
                current_def = Some(d);
                mp = &d.states;
            } else {
                return None;
            }
        }
        current_def
    }
    
    #[allow(clippy::too_many_arguments)]
    pub fn execute_action(
        ctx: &mut Value,
        action: &Action,
        event: &Value,
        actions_map: &IndexMap<String, ActionFunction>,
        internal_queue: &mut alloc::collections::VecDeque<Value>,
        delayed_events: &mut Vec<DelayedEvent>,
        delays: &IndexMap<String, DelayFunction>,
        now: u64,
        logger_fn: &Option<LoggerFunction>,
    ) {
        let mut last_error = None;
        Self::execute_action_with_effects(
            ctx,
            action,
            event,
            actions_map,
            &IndexMap::new(),
            &IndexMap::new(),
            internal_queue,
            delayed_events,
            delays,
            &StateValue::Atomic(String::new()),
            now,
            logger_fn,
            &mut last_error,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn execute_action_with_effects(
        ctx: &mut Value,
        action: &Action,
        event: &Value,
        actions_map: &IndexMap<String, ActionFunction>,
        actions_with_effects: &IndexMap<String, ActionWithEffectsFunction>,
        guards_map: &IndexMap<String, GuardFunction>,
        internal_queue: &mut alloc::collections::VecDeque<Value>,
        delayed_events: &mut Vec<DelayedEvent>,
        delays: &IndexMap<String, DelayFunction>,
        current_state: &StateValue,
        now: u64,
        logger_fn: &Option<LoggerFunction>,
        last_error: &mut Option<String>,
    ) {
        let mut effects = ActionEffects::new();
        match action {
            Action::Named(name) => {
                if let Some(f) = actions_with_effects.get(name) {
                    f(ctx, event, &None, actions_map, guards_map, delays, current_state, &mut effects);
                } else if let Some(f) = actions_map.get(name) {
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
                if let Some(f) = actions_with_effects.get(&gen.action_type) {
                    f(ctx, event, &gen.params, actions_map, guards_map, delays, current_state, &mut effects);
                } else if let Some(f) = actions_map.get(&gen.action_type) {
                    f(ctx, event, &gen.params);
                }
            }
            Action::Raise(payload) => {
                internal_queue.push_back(payload.clone());
            }
            Action::DelayedRaise(dra) => {
                let delay_ms = match &dra.delay {
                    Delay::Literal(ms) => *ms,
                    Delay::Named(name) => {
                        if let Some(f) = delays.get(name) {
                            f(ctx, event)
                        } else {
                            name.parse::<u64>().unwrap_or_default()
                        }
                    }
                };
                delayed_events.push(DelayedEvent {
                    id: dra.id.clone(),
                    deadline: now + delay_ms,
                    event: dra.event.clone(),
                });
            }
            Action::Cancel(ca) => {
                delayed_events.retain(|e| e.id.as_deref() != Some(&ca.send_id));
            }
            Action::Log(la) => {
                if let Some(logger) = logger_fn {
                    logger(&la.label, &la.value);
                }
            }
            Action::RaiseInvalid(msg) => {
                effects.error = Some(msg.clone());
            }
        }
        for raised in effects.raised {
            internal_queue.push_back(raised);
        }
        for mut delayed in effects.delayed_events {
            delayed.deadline += now;
            delayed_events.push(delayed);
        }
        for id in effects.cancel_ids {
            delayed_events.retain(|e| e.id.as_deref() != Some(&id));
        }
        if let Some(err) = effects.error {
            *last_error = Some(err);
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
        for (k, v) in actions {
            self.actions.insert(k.clone(), v.clone());
        }
        self.is_processing = true;
        let now = self.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(0);

        let mut last_ret = None;
        let mut steps = 0usize;
        while steps < 100 {
            if self.status == MachineStatus::Stopped {
                break;
            }
            let expired_state_timer = self
                .active_timers
                .iter()
                .enumerate()
                .filter(|(_, (_, deadline, _))| *deadline <= now)
                .min_by_key(|(_, (_, deadline, _))| *deadline)
                .map(|(idx, (path, deadline, t_defs))| (idx, *deadline, path.clone(), t_defs.clone()));

            let expired_delayed_event = self
                .delayed_events
                .iter()
                .enumerate()
                .filter(|(_, e)| e.deadline <= now)
                .min_by_key(|(_, e)| e.deadline)
                .map(|(idx, e)| (idx, e.deadline, e.event.clone()));

            match (expired_state_timer, expired_delayed_event) {
                (Some((s_idx, s_dead, s_path, s_defs)), Some((d_idx, d_dead, d_evt))) => {
                    steps += 1;
                    if s_dead <= d_dead {
                        self.active_timers.remove(s_idx);
                        let evt_type = if s_path.is_empty() {
                            "xstate.after".to_string()
                        } else {
                            format!("xstate.after.{}", s_path)
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
                            Some((&s_path, &s_defs)),
                            None,
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
                        self.delayed_events.remove(d_idx);
                        let e_type = d_evt
                            .as_object()
                            .and_then(|m| m.get("type"))
                            .and_then(|v| v.as_string())
                            .map(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let mut ret = self.transition_internal(
                            &e_type,
                            &d_evt,
                            guards,
                            actions,
                            0,
                            None,
                            None,
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
                    }
                }
                (Some((s_idx, _, s_path, s_defs)), None) => {
                    steps += 1;
                    self.active_timers.remove(s_idx);
                    let evt_type = if s_path.is_empty() {
                        "xstate.after".to_string()
                    } else {
                        format!("xstate.after.{}", s_path)
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
                        Some((&s_path, &s_defs)),
                        None,
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
                }
                (None, Some((d_idx, _, d_evt))) => {
                    steps += 1;
                    self.delayed_events.remove(d_idx);
                    let e_type = d_evt
                        .as_object()
                        .and_then(|m| m.get("type"))
                        .and_then(|v| v.as_string())
                        .map(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    let mut ret = self.transition_internal(
                        &e_type,
                        &d_evt,
                        guards,
                        actions,
                        0,
                        None,
                        None,
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
                }
                (None, None) => {
                    break;
                }
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
        let last_evt = self.last_received_event.clone();
        self.handle_machine_completion(&last_evt, actions, guards, None);
        last_ret
    }

    pub fn transition(
        &mut self,
        event_type: &str,
        event_payload: &Value,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) -> Option<StateValue> {
        if !self.is_started || self.status == MachineStatus::Stopped {
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
                break;
            }
            loop_count += 1;

            let (e_type, e_payload) = self.external_queue.pop_front().unwrap();
            let mut ret = self.transition_internal(&e_type, &e_payload, guards, actions, 0, None, None);
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
        let last_evt = self.last_received_event.clone();
        self.handle_machine_completion(&last_evt, actions, guards, None);
        last_ret
    }
    
    fn drain_internal_queue(&mut self, guards: &IndexMap<String, GuardFunction>, actions: &IndexMap<String, ActionFunction>) -> Option<StateValue> {
        self.drain_internal_queue_with_collector(guards, actions, None)
    }

    fn drain_internal_queue_with_collector(
        &mut self,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
        mut action_collector: Option<&mut Vec<Action>>,
    ) -> Option<StateValue> {
        let mut last_valid_state = None;
        let mut iterations = 0;
        loop {
            self.check_state_completions();
            let raised = match self.internal_queue.pop_front() {
                Some(r) => r,
                None => break,
            };
            if self.status == MachineStatus::Stopped {
                self.internal_queue.clear();
                break;
            }
            iterations += 1;
            if iterations > 100 {
                // Prevent infinite loops from cyclic raised events
                self.internal_queue.clear();
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
            if let Some(next) = self.transition_internal(e_type, e_payload, guards, actions, 0, None, action_collector.as_deref_mut()) {
                last_valid_state = Some(next);
            }
        }
        last_valid_state
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn execute_step_action(
        ctx: &mut Value,
        act: &Action,
        event_payload: &Value,
        actions: &IndexMap<String, ActionFunction>,
        actions_with_effects: &IndexMap<String, ActionWithEffectsFunction>,
        guards: &IndexMap<String, GuardFunction>,
        internal_queue: &mut alloc::collections::VecDeque<Value>,
        delayed_events: &mut Vec<DelayedEvent>,
        delays: &IndexMap<String, DelayFunction>,
        current_state: &StateValue,
        now: u64,
        logger_fn: &Option<LoggerFunction>,
        last_error: &mut Option<String>,
        action_collector: &mut Option<&mut Vec<Action>>,
    ) {
        if let Some(col) = action_collector.as_deref_mut() {
            match act {
                Action::Assign(assign) => {
                    if let Value::Object(new_vals) = &assign.assignment {
                        if let Value::Object(ctx_map) = ctx {
                            for (k, v) in new_vals {
                                ctx_map.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    col.push(act.clone());
                }
                Action::Raise(payload) => {
                    internal_queue.push_back(payload.clone());
                    col.push(act.clone());
                }
                Action::Named(name) => {
                    if name.starts_with("auto_action_assign_") {
                        if let Some(f) = actions.get(name) {
                            f(ctx, event_payload, &None);
                        }
                    } else {
                        col.push(act.clone());
                    }
                }
                Action::DelayedRaise(_) | Action::Cancel(_) | Action::Log(_) | Action::Generic(_) => {
                    col.push(act.clone());
                }
                Action::RaiseInvalid(msg) => {
                    *last_error = Some(msg.clone());
                    col.push(act.clone());
                }
            }
        } else {
            Self::execute_action_with_effects(
                ctx,
                act,
                event_payload,
                actions,
                actions_with_effects,
                guards,
                internal_queue,
                delayed_events,
                delays,
                current_state,
                now,
                logger_fn,
                last_error,
            );
        }
    }

    #[inline(never)]
    fn calculate_exit_paths(
        active_leaves: &[String],
        next_leaves: &[String],
        selected_transitions: &[SelectedTransition],
    ) -> Vec<String> {
        let mut document_order_active = Vec::new();
        for leaf in active_leaves {
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
             for next_leaf in next_leaves {
                  let next_parts: Vec<&str> = next_leaf.split('.').filter(|s| !s.is_empty()).collect();
                  let mut lca = 0;
                  while lca < leaf_parts.len() && lca < next_parts.len() && leaf_parts[lca] == next_parts[lca] { lca += 1; }
                  
                  let mut leaf_is_in_any_domain = false;
                  let mut max_allowed_lca = leaf_parts.len();
                  for st in selected_transitions {
                       if st.is_targetless { continue; }
                       let src_parts: Vec<&str> = st.source_path.split('.').filter(|s| !s.is_empty()).collect();
                       let domain_len = st.lowest_domain_depth;
                       let is_in_domain = domain_len <= src_parts.len() && (0..domain_len).all(|idx| idx < leaf_parts.len() && leaf_parts[idx] == src_parts[idx]);
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
        final_exited_paths
    }

    #[inline(never)]
    fn calculate_entered_paths(
        active_leaves: &[String],
        next_leaves: &[String],
        selected_transitions: &[SelectedTransition],
    ) -> Vec<String> {
        let mut entered_paths = Vec::new();
        for leaf in next_leaves {
             let leaf_parts: Vec<&str> = leaf.split('.').filter(|s| !s.is_empty()).collect();
             let mut max_lca = 0;
             for curr_leaf in active_leaves.iter().rev() {
                  let curr_parts: Vec<&str> = curr_leaf.split('.').filter(|s| !s.is_empty()).collect();
                  let mut lca = 0;
                  while lca < leaf_parts.len() && lca < curr_parts.len() && leaf_parts[lca] == curr_parts[lca] { lca += 1; }
                  
                  let mut leaf_is_in_any_domain = false;
                  let mut max_allowed_lca = leaf_parts.len();
                  for st in selected_transitions {
                       if st.is_targetless { continue; }
                       let src_parts: Vec<&str> = st.source_path.split('.').filter(|s| !s.is_empty()).collect();
                       let domain_len = st.lowest_domain_depth;
                       let is_in_domain = domain_len <= src_parts.len() && (0..domain_len).all(|idx| idx < leaf_parts.len() && leaf_parts[idx] == src_parts[idx]);
                       if is_in_domain {
                            leaf_is_in_any_domain = true;
                            if st.lowest_domain_depth < max_allowed_lca { max_allowed_lca = st.lowest_domain_depth; }
                       }
                  }
                  
                  if leaf_is_in_any_domain && lca > max_allowed_lca { lca = max_allowed_lca; }
                  if lca > max_lca { max_lca = lca; }
             }
             for i in (max_lca + 1)..=leaf_parts.len() {
                   let path = leaf_parts[0..i].join(".");
                   if !entered_paths.contains(&path) {
                       entered_paths.push(path);
                   }
             }
        }
        entered_paths
    }

    #[allow(clippy::too_many_arguments)]
    fn transition_internal(
        &mut self,
        event_type: &str,
        event_payload: &Value,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
        _depth: usize,
        // Bypasses the standard event matching to forcefully execute a transition. 
        // Used primarily for resolving event-less `always` transitions or executing queued delayed `after` transitions.
        force_transition: Option<(&str, &[TransitionDefinition])>,
        mut action_collector: Option<&mut Vec<Action>>,
    ) -> Option<StateValue> {
        if self.status == MachineStatus::Stopped {
            return None;
        }

        let mut current_event_type = event_type;
        let mut current_force = force_transition;
        let mut step_count = 0usize;
        let mut had_any_transition = false;

        loop {
            if self.status == MachineStatus::Stopped {
                return if had_any_transition {
                    Some(self.current_state.clone())
                } else {
                    None
                };
            }
            if step_count > 100 {
                return if had_any_transition {
                    Some(self.current_state.clone())
                } else {
                    None
                };
            }

            match self.microstep(
                current_event_type,
                event_payload,
                guards,
                actions,
                current_force,
                action_collector.as_deref_mut(),
            ) {
                Some(changed) => {
                    had_any_transition = true;
                    let should_continue = !current_event_type.is_empty() || changed || current_force.is_some();
                    if !should_continue {
                        return Some(self.current_state.clone());
                    }
                    current_event_type = "";
                    current_force = None;
                    step_count += 1;
                }
                None => {
                    return if had_any_transition {
                        Some(self.current_state.clone())
                    } else {
                        None
                    };
                }
            }
        }
    }

    #[inline(never)]
    fn microstep(
        &mut self,
        current_event_type: &str,
        event_payload: &Value,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
        current_force: Option<(&str, &[TransitionDefinition])>,
        mut action_collector: Option<&mut Vec<Action>>,
    ) -> Option<bool> {
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

        let mut selected_transitions = Vec::new();
        let mut conflict_regions: Vec<String> = Vec::new();
        let mut pending_error: Option<String> = None;

        for current_path in all_search_paths {
            let candidates: Vec<&TransitionDefinition> = if let Some((f_path, f_t)) = current_force {
                if *f_path == current_path {
                    f_t.iter().collect()
                } else {
                    Vec::new()
                }
            } else {
                let mut cands = Vec::new();
                if let Some(remainder) = current_event_type.strip_prefix("xstate.after.") {
                    if let Some(first_dot) = remainder.find('.') {
                        let delay_str = &remainder[..first_dot];
                        let target_id = &remainder[first_dot + 1..];
                        let machine_id = if self.definition.id.is_empty() { "(machine)" } else { &self.definition.id };
                        let matches_state = if current_path.is_empty() {
                            target_id.is_empty() || target_id == "#" || target_id == machine_id
                        } else {
                            let def_opt = Self::get_state_def_recursive(&self.definition.states, &current_path);
                            let explicit_id = def_opt.as_ref().and_then(|d| d.id.as_deref());
                            let synthetic_id = format!("{}.{}", machine_id, current_path);
                            target_id == current_path
                                || explicit_id == Some(target_id)
                                || target_id == synthetic_id
                                || target_id == format!("(machine).{}", current_path)
                        };
                        if matches_state {
                            if let Ok(delay_num) = delay_str.parse::<u64>() {
                                if current_path.is_empty() {
                                    if let Some(t_defs) = self.definition.after.get(&Delay::Literal(delay_num)) {
                                        cands.extend(t_defs.iter());
                                    }
                                } else if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &current_path) {
                                    if let Some(t_defs) = def.after.get(&delay_num) {
                                        cands.extend(t_defs.iter());
                                    }
                                }
                            }
                            if cands.is_empty() {
                                if current_path.is_empty() {
                                    if let Some(t_defs) = self.definition.after.get(&Delay::Named(delay_str.to_string())) {
                                        cands.extend(t_defs.iter());
                                    }
                                } else if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &current_path) {
                                    if let Some(t_defs) = def.named_after.get(delay_str) {
                                        cands.extend(t_defs.iter());
                                    }
                                }
                            }
                        }
                    }
                }
                if cands.is_empty() {
                    let on_map_opt = if current_path.is_empty() {
                        Some(&self.definition.on)
                    } else {
                        Self::get_state_def_recursive(&self.definition.states, &current_path)
                            .map(|d| &d.on)
                    };
                    if let Some(on_map) = on_map_opt {
                        Self::get_candidates(on_map, current_event_type, &self.warn_fn)
                    } else {
                        Vec::new()
                    }
                } else {
                    cands
                }
            };

            if !candidates.is_empty() {
                  for t in candidates {
                       if let Some(guard) = &t.guard {
                           match self.evaluate_guard_ast(guard, event_payload, guards) {
                               Ok(is_allowed) => {
                                   if !is_allowed { continue; }
                               }
                               Err(missing_guard) => {
                                   if pending_error.is_none() {
                                       pending_error = Some(self.format_guard_error(&missing_guard, current_event_type, &current_path));
                                   }
                                   continue;
                               }
                           }
                       }

                       let is_targetless = (t.target.is_none() || t.target.as_ref().is_some_and(|v| v.is_empty())) && !t.reenter;
                       let is_root_reenter = (current_path.is_empty() && t.reenter)
                           || t.target.as_ref().is_some_and(|targets| {
                               targets.iter().any(|raw| {
                                   let id_key = raw.strip_prefix('#').unwrap_or(raw);
                                   (id_key == self.definition.id || raw.is_empty() || raw == "#") && t.reenter
                               })
                           });

                       let current_parts: Vec<&str> = current_path.split('.').filter(|s| !s.is_empty()).collect();
                       let mut lowest_domain_depth = current_parts.len();

                       if let Some(targets) = &t.target {
                           for raw in targets {
                                let current_search_path = if current_path.is_empty() { "".to_string() } else { current_path.clone() };
                                let target_full = self.resolve_target(&current_search_path, raw).unwrap_or_else(|| raw.clone());
                                if target_full.is_empty() {
                                    lowest_domain_depth = 0;
                                    continue;
                                }
                                let target_parts: Vec<&str> = target_full.split('.').filter(|s| !s.is_empty()).collect();
                                let mut dom_lca = 0;
                                while dom_lca < current_parts.len() && dom_lca < target_parts.len() && current_parts[dom_lca] == target_parts[dom_lca] { dom_lca += 1; }
                                
                                let mut domain_depth = dom_lca;
                                if (dom_lca == current_parts.len() && t.reenter)
                                    || (dom_lca == target_parts.len() && target_parts.len() < current_parts.len())
                                {
                                    domain_depth = dom_lca.saturating_sub(1);
                                }
                                if domain_depth < lowest_domain_depth { lowest_domain_depth = domain_depth; }
                           }
                       } else if t.reenter {
                           lowest_domain_depth = current_parts.len().saturating_sub(1);
                       }

                       if is_root_reenter {
                           lowest_domain_depth = 0;
                       }

                       let domain_path = current_parts[0..lowest_domain_depth].join(".");
                       let conflict_check_path = if (t.target.is_some() || t.reenter) && !is_targetless { domain_path.clone() } else { current_path.clone() };
                       
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
                           is_targetless,
                           reenters_root: is_root_reenter,
                       });
                       break;
                  }
            }
        }

        if let Some(err) = pending_error {
            self.last_error = Some(err);
        }

        if selected_transitions.is_empty() {
            return None;
        }

        let mut accumulated_state = self.current_state.clone();
        let mut all_collected_actions = Vec::new();
        let mut all_transition_actions = Vec::new();
        let mut history_fallback_parents = Vec::new();

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
                                     else {
                                         next_val_opt = Self::resolve_initial_state(&self.definition, &history_parent, &mut all_collected_actions);
                                         history_fallback_parents.push(history_parent.clone());
                                     }
                                }
                           }
                      }
                      let next_val = next_val_opt.or_else(|| Self::resolve_initial_state(&self.definition, &target_full, &mut all_collected_actions)).unwrap_or(StateValue::Atomic(target_full.clone()));
                      if target_full.is_empty() {
                          accumulated_state = next_val;
                      } else {
                          accumulated_state = Self::merge_state_value(&accumulated_state, &next_val, "", &target_full);
                      }
                  }
             }
             all_transition_actions.extend(t.actions.clone());
        }

        let mut targeted_paths = Vec::new();
        for st in &selected_transitions {
            if let Some(targets) = &st.t.target {
                for raw in targets {
                    let current_search_path = if st.source_path.is_empty() { "".to_string() } else { st.source_path.clone() };
                    let target_full = self.resolve_target(&current_search_path, raw).unwrap_or_else(|| raw.clone());
                    targeted_paths.push(target_full);
                }
            } else if st.reenters_root {
                targeted_paths.push("".to_string());
            }
        }

        let finalized_state = accumulated_state;
        let next_leaves = finalized_state.get_active_paths();
        let final_exited_paths = Self::calculate_exit_paths(&active_leaves, &next_leaves, &selected_transitions);
        
        let now = self.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(0);
        macro_rules! exec_action {
            ($act:expr) => {
                Self::execute_step_action(
                    &mut self.context,
                    $act,
                    event_payload,
                    actions,
                    &self.actions_with_effects,
                    guards,
                    &mut self.internal_queue,
                    &mut self.delayed_events,
                    &self.delays,
                    &self.current_state,
                    now,
                    &self.logger_fn,
                    &mut self.last_error,
                    &mut action_collector,
                );
            };
        }
        let mut exited_paths = Vec::new();
        for path in &final_exited_paths {
              let has_history_child = if path.is_empty() {
                  self.definition.states.values().any(|s| matches!(s.state_type, StateType::History { .. }) || s.history.is_some())
              } else if let Some(def) = Self::get_state_def_recursive(&self.definition.states, path) {
                  def.states.values().any(|s| matches!(s.state_type, StateType::History { .. }) || s.history.is_some())
              } else {
                  false
              };
              if has_history_child {
                  self.history_value.insert(path.clone(), self.current_state.clone());
              }
              if let Some(def) = Self::get_state_def_recursive(&self.definition.states, path) {
                   for action in &def.exit { 
                         exec_action!(action); 
                   }
              }
              exited_paths.push(path.clone());
              self.completed_states.retain(|s| s != path && !s.starts_with(&format!("{}.", path)));
        }

        if selected_transitions.iter().any(|st| st.reenters_root) {
            for action in &self.definition.exit {
                exec_action!(action);
            }
            self.completed_states.clear();
            self.is_machine_completed = false;
            self.status = MachineStatus::Active;
        }
        
        self.active_timers.retain(|(p, _, _)| !exited_paths.contains(p));
        
        for action in &all_transition_actions { exec_action!(action); }
        for action in &all_collected_actions { exec_action!(action); }

        if selected_transitions.iter().any(|st| st.reenters_root) {
            for (delay, t_defs) in &self.definition.after {
                let delay_ms = self.resolve_delay(delay, event_payload);
                self.active_timers.push(("".to_string(), now + delay_ms, t_defs.clone()));
                if let Some(col) = action_collector.as_deref_mut() {
                    let machine_id = if self.definition.id.is_empty() { "(machine)" } else { &self.definition.id };
                    let (delay_str, delay_obj) = match delay {
                        Delay::Literal(ms) => (ms.to_string(), Delay::Literal(*ms)),
                        Delay::Named(name) => (name.clone(), Delay::Named(name.clone())),
                    };
                    let evt_type = format!("xstate.after.{}.{}", delay_str, machine_id);
                    let mut evt_map = IndexMap::new();
                    evt_map.insert("type".to_string(), Value::String(evt_type));
                    col.push(Action::DelayedRaise(DelayedRaiseAction {
                        event: Value::Object(evt_map),
                        delay: delay_obj,
                        id: None,
                    }));
                }
            }
            for action in &self.definition.entry {
                exec_action!(action);
            }
            for action in &self.definition.initial_actions {
                exec_action!(action);
            }
        }

        let entered_paths = Self::calculate_entered_paths(&active_leaves, &next_leaves, &selected_transitions);
        for path in &entered_paths {
            if let Some(def) = Self::get_state_def_recursive(&self.definition.states, path) {
                for action in &def.entry { exec_action!(action); }
                for inv in &def.invoke {
                    if let Some(src) = inv.src.as_string() {
                        let act = Action::Named(src.to_string());
                        exec_action!(&act);
                    }
                }
                for (delay, t_defs) in &def.after {
                    self.active_timers.push((path.clone(), now + delay, t_defs.clone()));
                    if let Some(col) = action_collector.as_deref_mut() {
                        let machine_id = if self.definition.id.is_empty() { "(machine)" } else { &self.definition.id };
                        let state_identifier = if let Some(explicit_id) = &def.id {
                            explicit_id.clone()
                        } else {
                            format!("{}.{}", machine_id, path)
                        };
                        let evt_type = format!("xstate.after.{}.{}", delay, state_identifier);
                        let mut evt_map = IndexMap::new();
                        evt_map.insert("type".to_string(), Value::String(evt_type));
                        col.push(Action::DelayedRaise(DelayedRaiseAction {
                            event: Value::Object(evt_map),
                            delay: Delay::Literal(*delay),
                            id: None,
                        }));
                    }
                }
                for (delay_name, t_defs) in &def.named_after {
                    let delay_ms = self.resolve_delay(&Delay::Named(delay_name.clone()), event_payload);
                    self.active_timers.push((path.clone(), now + delay_ms, t_defs.clone()));
                    if let Some(col) = action_collector.as_deref_mut() {
                        let machine_id = if self.definition.id.is_empty() { "(machine)" } else { &self.definition.id };
                        let state_identifier = if let Some(explicit_id) = &def.id {
                            explicit_id.clone()
                        } else {
                            format!("{}.{}", machine_id, path)
                        };
                        let evt_type = format!("xstate.after.{}.{}", delay_name, state_identifier);
                        let mut evt_map = IndexMap::new();
                        evt_map.insert("type".to_string(), Value::String(evt_type));
                        col.push(Action::DelayedRaise(DelayedRaiseAction {
                            event: Value::Object(evt_map),
                            delay: Delay::Named(delay_name.clone()),
                            id: None,
                        }));
                    }
                }
                if matches!(def.state_type, StateType::Compound) {
                    let took_initial = targeted_paths.iter().any(|t| t == path || path.starts_with(&format!("{}.", t)) || t.is_empty())
                        || history_fallback_parents.iter().any(|p| p == path);
                    if took_initial {
                        for action in &def.initial_actions {
                            exec_action!(action);
                        }
                    }
                }
            }
        }
        let has_targets = selected_transitions.iter().any(|st| {
            st.t.reenter || st.t.target.as_ref().map(|v| !v.is_empty()).unwrap_or(false)
        });
        drop(selected_transitions);
        let changed = has_targets || (finalized_state != self.current_state);
        self.current_state = finalized_state;
        self.last_received_event = event_payload.clone();
        self.refresh_status();
        self.handle_machine_completion(event_payload, actions, guards, action_collector);
        Some(changed)
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

#[cfg(test)]
mod test_m2_parallel_history_on_done {
    extern crate std;
    use super::*;

    #[test]
    fn test_parallel_history_on_done_fires() {
        let json = r#"{
            "initial": "parallelSteps",
            "states": {
                "parallelSteps": {
                    "type": "parallel",
                    "states": {
                        "hist": {
                            "type": "history"
                        },
                        "one": {
                            "initial": "wait_one",
                            "states": {
                                "wait_one": {
                                    "on": { "finish_one": "done" }
                                },
                                "done": { "type": "final" }
                            }
                        },
                        "two": {
                            "initial": "wait_two",
                            "states": {
                                "wait_two": {
                                    "on": { "finish_two": "done" }
                                },
                                "done": { "type": "final" }
                            }
                        }
                    },
                    "onDone": "finished"
                },
                "finished": {}
            }
        }"#;
        let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
        machine.start(&IndexMap::new(), &IndexMap::new());
        assert!(machine.current_state.matches("parallelSteps.one.wait_one"));
        assert!(machine.current_state.matches("parallelSteps.two.wait_two"));
        machine.transition("finish_one", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert!(machine.current_state.matches("parallelSteps.one.done"));
        machine.transition("finish_two", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert!(machine.current_state.matches("finished"));
    }

    #[test]
    fn test_final_output_secret() {
        let json = r#"{
            "initial": "secret",
            "context": { "revealedSecret": null },
            "states": {
                "secret": {
                    "initial": "wait",
                    "states": {
                        "wait": {
                            "on": { "REQUEST_SECRET": "reveal" }
                        },
                        "reveal": {
                            "type": "final"
                        }
                    },
                    "onDone": {
                        "target": "success"
                    }
                },
                "success": {
                    "type": "final"
                }
            }
        }"#;
        let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
        machine.start(&IndexMap::new(), &IndexMap::new());
        assert!(machine.current_state.matches("secret.wait"));
        machine.transition("REQUEST_SECRET", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert!(machine.current_state.matches("success"));
        assert!(machine.is_done());
    }
}
