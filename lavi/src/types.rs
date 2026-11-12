use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::hash::{BuildHasherDefault, Hasher};

use crate::json::JsonError;
use crate::parser;

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
    pub inspected_actions: Vec<InspectEvent>,
    pub delegated_actions: Vec<GenericAction>,
    pub error: Option<String>,
}

impl ActionEffects {
    pub fn new() -> Self {
        Self {
            raised: alloc::collections::VecDeque::new(),
            delayed_events: Vec::new(),
            cancel_ids: Vec::new(),
            inspected_actions: Vec::new(),
            delegated_actions: Vec::new(),
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
pub type InvokeStartFunction = Rc<dyn Fn(&str, &Value, &Option<Value>, &Value, &Value)>;
pub type InvokeStopFunction = Rc<dyn Fn(&str)>;

#[derive(Debug, Clone, PartialEq)]
pub struct InspectedTransition {
    pub source_id: String,
    pub target_ids: Option<Vec<String>>,
    pub event_type: String,
    pub actions: Vec<Action>,
    pub guard: Option<TransitionGuard>,
    pub reenter: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InspectEvent {
    Actor,
    Event {
        event: Value,
    },
    Action {
        action: Action,
        params: Option<Value>,
    },
    Microstep {
        event: Value,
        transitions: Vec<InspectedTransition>,
        state: StateValue,
        context: Value,
        status: MachineStatus,
    },
    Snapshot {
        event: Value,
        state: StateValue,
        context: Value,
        status: MachineStatus,
    },
}

pub type InspectFunction = Rc<dyn Fn(&InspectEvent)>;

impl InspectEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            InspectEvent::Actor => "@xstate.actor",
            InspectEvent::Event { .. } => "@xstate.event",
            InspectEvent::Action { .. } => "@xstate.action",
            InspectEvent::Microstep { .. } => "@xstate.microstep",
            InspectEvent::Snapshot { .. } => "@xstate.snapshot",
        }
    }

    pub fn to_value(&self) -> Value {
        let mut map = IndexMap::new();
        match self {
            InspectEvent::Actor => {
                map.insert("type".to_string(), Value::String("@xstate.actor".to_string()));
            }
            InspectEvent::Event { event } => {
                map.insert("type".to_string(), Value::String("@xstate.event".to_string()));
                map.insert("event".to_string(), event.clone());
            }
            InspectEvent::Action { action, params } => {
                map.insert("type".to_string(), Value::String("@xstate.action".to_string()));
                let mut action_map = IndexMap::new();
                let (action_type, resolved_params) = match action {
                    Action::Named(name) => (name.clone(), params.clone()),
                    Action::Generic(g) => (g.action_type.clone(), params.clone().or_else(|| g.params.clone())),
                    Action::Assign(a) => (a.action_type.clone(), params.clone()),
                    Action::Raise(_) | Action::DelayedRaise(_) => ("xstate.raise".to_string(), params.clone()),
                    Action::Cancel(_) => ("xstate.cancel".to_string(), params.clone()),
                    Action::Log(_) => ("xstate.log".to_string(), params.clone()),
                    Action::RaiseInvalid(s) => (s.clone(), params.clone()),
                };
                action_map.insert("type".to_string(), Value::String(action_type));
                if let Some(p) = resolved_params {
                    action_map.insert("params".to_string(), p);
                }
                map.insert("action".to_string(), Value::Object(action_map));
            }
            InspectEvent::Microstep {
                event,
                transitions,
                state,
                context,
                status,
            } => {
                map.insert("type".to_string(), Value::String("@xstate.microstep".to_string()));
                map.insert("event".to_string(), event.clone());
                let mut trans_arr = Vec::with_capacity(transitions.len());
                for t in transitions {
                    let mut t_map = IndexMap::new();
                    t_map.insert("eventType".to_string(), Value::String(t.event_type.clone()));
                    t_map.insert("reenter".to_string(), Value::Bool(t.reenter));
                    t_map.insert("source".to_string(), Value::String(format!("#{}", t.source_id)));
                    if let Some(ref targets) = t.target_ids {
                        let target_vals = targets
                            .iter()
                            .map(|id| Value::String(format!("#{}", id)))
                            .collect();
                        t_map.insert("target".to_string(), Value::Array(target_vals));
                    }
                    trans_arr.push(Value::Object(t_map));
                }
                map.insert("_transitions".to_string(), Value::Array(trans_arr));
                let mut snap_map = IndexMap::new();
                snap_map.insert("status".to_string(), Value::String(status.as_str().to_string()));
                snap_map.insert("value".to_string(), state.to_value());
                snap_map.insert("context".to_string(), context.clone());
                map.insert("snapshot".to_string(), Value::Object(snap_map));
            }
            InspectEvent::Snapshot {
                event,
                state,
                context,
                status,
            } => {
                map.insert("type".to_string(), Value::String("@xstate.snapshot".to_string()));
                map.insert("event".to_string(), event.clone());
                let mut snap_map = IndexMap::new();
                snap_map.insert("status".to_string(), Value::String(status.as_str().to_string()));
                snap_map.insert("value".to_string(), state.to_value());
                snap_map.insert("context".to_string(), context.clone());
                map.insert("snapshot".to_string(), Value::Object(snap_map));
            }
        }
        Value::Object(map)
    }

    pub fn to_json_string(&self) -> String {
        self.to_value().to_json_string()
    }
}

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
    pub input: Option<Value>,
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

impl Action {
    pub fn to_value(&self) -> Value {
        let mut map = IndexMap::new();
        match self {
            Action::Named(name) => {
                map.insert("type".to_string(), Value::String(name.clone()));
            }
            Action::Assign(assign) => {
                map.insert("type".to_string(), Value::String(assign.action_type.clone()));
                let mut p_map = IndexMap::new();
                p_map.insert("assignment".to_string(), assign.assignment.clone());
                map.insert("params".to_string(), Value::Object(p_map));
            }
            Action::Generic(gen) => {
                map.insert("type".to_string(), Value::String(gen.action_type.clone()));
                if let Some(ref params) = gen.params {
                    map.insert("params".to_string(), params.clone());
                }
            }
            Action::Raise(ev) => {
                map.insert("type".to_string(), Value::String("xstate.raise".to_string()));
                let mut p_map = IndexMap::new();
                p_map.insert("event".to_string(), ev.clone());
                map.insert("params".to_string(), Value::Object(p_map));
            }
            Action::DelayedRaise(dra) => {
                map.insert("type".to_string(), Value::String("xstate.raise".to_string()));
                let mut p_map = IndexMap::new();
                p_map.insert("event".to_string(), dra.event.clone());
                match &dra.delay {
                    Delay::Literal(ms) => {
                        p_map.insert("delay".to_string(), Value::Number(*ms as f64));
                    }
                    Delay::Named(name) => {
                        p_map.insert("delay".to_string(), Value::String(name.clone()));
                    }
                }
                if let Some(ref id) = dra.id {
                    p_map.insert("id".to_string(), Value::String(id.clone()));
                }
                map.insert("params".to_string(), Value::Object(p_map));
            }
            Action::Cancel(cancel) => {
                map.insert("type".to_string(), Value::String("xstate.cancel".to_string()));
                let mut p_map = IndexMap::new();
                p_map.insert("sendId".to_string(), Value::String(cancel.send_id.clone()));
                map.insert("params".to_string(), Value::Object(p_map));
            }
            Action::Log(log) => {
                map.insert("type".to_string(), Value::String("xstate.log".to_string()));
                let mut p_map = IndexMap::new();
                p_map.insert("value".to_string(), log.value.clone());
                if let Some(ref lbl) = log.label {
                    p_map.insert("label".to_string(), Value::String(lbl.clone()));
                }
                map.insert("params".to_string(), Value::Object(p_map));
            }
            Action::RaiseInvalid(s) => {
                map.insert("type".to_string(), Value::String(s.clone()));
            }
        }
        Value::Object(map)
    }
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
