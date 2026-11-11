use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use crate::{IndexMap, IndexMapExt};

use crate::json::JsonError;
use crate::{
    Action, AssignAction, CancelAction, Delay, DelayedRaiseAction, GenericAction, GuardObject,
    HistoryType, InvokeDefinition, LogAction, MachineDefinition, StateDefinition, StateType,
    StateValue, TransitionDefinition, TransitionGuard, Value,
};

pub struct SetupObject;

impl SetupObject {
    pub fn create_machine(&self, json: &str) -> Result<MachineDefinition, JsonError> {
        create_machine(json)
    }
}

pub fn setup(_options: &str) -> SetupObject {
    SetupObject
}

pub fn create_machine(json: &str) -> Result<MachineDefinition, JsonError> {
    from_json_str(json)
}

pub fn from_json_str(json: &str) -> Result<MachineDefinition, JsonError> {
    let val = Value::from_json(json)?;
    MachineDefinition::from_value(val)
}

pub fn assign(assignment_json: &str) -> Result<Action, JsonError> {
    let val = Value::from_json(assignment_json)?;
    Ok(Action::Assign(AssignAction {
        action_type: "xstate.assign".to_string(),
        assignment: val,
    }))
}

enum StateValueFrame {
    Compound {
        key: String,
    },
    Parallel {
        remaining: indexmap::map::IntoIter<String, Value>,
        regions: IndexMap<String, StateValue>,
        current_key: Option<String>,
    },
}

pub fn value_to_state_value(v: Value) -> StateValue {
    let mut stack: Vec<StateValueFrame> = Vec::new();
    let mut completed: Option<StateValue> = None;
    let mut pending_val: Option<Value> = Some(v);

    loop {
        if let Some(comp) = completed.take() {
            if let Some(top) = stack.last_mut() {
                match top {
                    StateValueFrame::Compound { .. } => {
                        let frame = stack.pop().unwrap();
                        if let StateValueFrame::Compound { key } = frame {
                            completed = Some(StateValue::Compound(key, Box::new(comp)));
                        }
                    }
                    StateValueFrame::Parallel {
                        remaining,
                        regions,
                        current_key,
                    } => {
                        if let Some(k) = current_key.take() {
                            let region_val = match comp {
                                StateValue::Parallel(ref m) if m.is_empty() => {
                                    StateValue::Atomic(k.clone())
                                }
                                StateValue::Atomic(ref s) if s.is_empty() => {
                                    StateValue::Atomic(k.clone())
                                }
                                other => StateValue::Compound(k.clone(), Box::new(other)),
                            };
                            regions.insert(k, region_val);
                        }
                        if let Some((next_key, next_val)) = remaining.next() {
                            *current_key = Some(next_key);
                            pending_val = Some(next_val);
                        } else {
                            let frame = stack.pop().unwrap();
                            if let StateValueFrame::Parallel { regions, .. } = frame {
                                completed = Some(StateValue::Parallel(regions));
                            }
                        }
                    }
                }
            } else {
                return comp;
            }
        } else if let Some(mut val) = pending_val.take() {
            match &mut val {
                Value::String(s) => {
                    let s = core::mem::take(s);
                    if s.contains('.') {
                        let mut iter = s.rsplit('.');
                        let last = iter.next().unwrap_or("").to_string();
                        let mut acc = StateValue::Atomic(last);
                        for part in iter {
                            acc = StateValue::Compound(part.to_string(), Box::new(acc));
                        }
                        completed = Some(acc);
                    } else {
                        completed = Some(StateValue::Atomic(s));
                    }
                }
                Value::Object(obj) => {
                    let obj = core::mem::take(obj);
                    if obj.is_empty() {
                        completed = Some(StateValue::Parallel(IndexMap::new()));
                    } else if obj.len() == 1 {
                        let mut iter = obj.into_iter();
                        let (k, next_val) = iter.next().unwrap();
                        stack.push(StateValueFrame::Compound { key: k });
                        pending_val = Some(next_val);
                    } else {
                        let mut iter = obj.into_iter();
                        let (first_key, first_val) = iter.next().unwrap();
                        stack.push(StateValueFrame::Parallel {
                            remaining: iter,
                            regions: IndexMap::new(),
                            current_key: Some(first_key),
                        });
                        pending_val = Some(first_val);
                    }
                }
                _ => {
                    completed = Some(StateValue::Atomic(String::new()));
                }
            }
        } else {
            return completed.unwrap_or_else(|| StateValue::Atomic(String::new()));
        }
    }
}

pub fn parse_machine_definition(val: Value) -> Result<MachineDefinition, JsonError> {
    let mut root = match val.into_object() {
        Some(map) => map,
        None => {
            return Err(JsonError {
                message: "Expected JSON object for machine definition".to_string(),
                offset: 0,
            });
        }
    };

    let mut id = root
        .swap_remove("id")
        .and_then(|v| v.into_string())
        .unwrap_or_else(|| "(machine)".to_string());

    if id != "(machine)" {
        while let Some(stripped) = id.strip_prefix("(machine).") {
            id = stripped.to_string();
        }
    }

    let raw_type = root.swap_remove("type").and_then(|v| v.into_string());

    let (initial_str, initial_actions) = resolve_initial_info(root.swap_remove("initial"));
    let initial_str = initial_str.map(|s| normalize_initial_str(s, &id, "", None, None));
    let states_val = root.swap_remove("states");

    let mut on = parse_on(root.swap_remove("on"));
    let always = parse_transition_list(root.swap_remove("always"));
    if !always.is_empty() {
        on.insert(String::new(), always);
    }

    let (num_after, named_after) = parse_after(root.swap_remove("after"));
    let mut after = IndexMap::new();
    for (k, v) in num_after {
        after.insert(Delay::Literal(k), v);
    }
    for (k, v) in named_after {
        after.insert(Delay::Named(k), v);
    }
    let entry = parse_action_list(root.swap_remove("entry"));
    let exit = parse_action_list(root.swap_remove("exit"));
    let root_state_id = if id.is_empty() { "(machine)" } else { &id };
    let invoke = parse_invoke_list(root.swap_remove("invoke"), root_state_id, &mut on);
    let context = root.swap_remove("context");
    let types = root.swap_remove("types");
    let tags = parse_string_list(root.swap_remove("tags"));
    let meta = root.swap_remove("meta");
    let description = root.swap_remove("description").and_then(|v| v.into_string());
    let output = root.swap_remove("output");

    let mut id_map = IndexMap::new();
    if !id.is_empty() {
        id_map.insert(id.clone(), String::new());
    }
    let states = parse_states_iterative(states_val, &id, &mut id_map);

    let state_type = match raw_type.as_deref() {
        Some("parallel") => StateType::Parallel,
        Some("final") => StateType::Final,
        _ if !states.is_empty() => StateType::Compound,
        _ => StateType::Atomic,
    };

    let initial = if matches!(state_type, StateType::Parallel) {
        String::new()
    } else {
        initial_str
            .or_else(|| states.keys().next().cloned())
            .unwrap_or_default()
    };


    Ok(MachineDefinition {
        id,
        state_type,
        initial,
        initial_actions,
        states,
        on,
        after,
        entry,
        exit,
        invoke,
        context,
        types,
        id_map,
        tags,
        meta,
        description,
        input: None,
        output,
    })
}

struct StateWorkFrame {
    key: String,
    current_path: String,
    effective_id: String,
    state_def: StateDefinition,
    remaining_children: indexmap::map::IntoIter<String, Value>,
}

fn parse_states_iterative(
    states_val: Option<Value>,
    machine_id: &str,
    id_map: &mut IndexMap<String, String>,
) -> IndexMap<String, StateDefinition> {
    let root_children = match states_val.and_then(|v| v.into_object()) {
        Some(map) => map.into_iter(),
        None => return IndexMap::new(),
    };

    let root_effective_id = if machine_id.is_empty() {
        "(machine)".to_string()
    } else {
        machine_id.to_string()
    };

    let root_frame = StateWorkFrame {
        key: String::new(),
        current_path: String::new(),
        effective_id: root_effective_id,
        state_def: StateDefinition::default_atomic(),
        remaining_children: root_children,
    };

    let mut stack: Vec<StateWorkFrame> = vec![root_frame];

    while let Some(top) = stack.last_mut() {
        if let Some((child_key, child_val)) = top.remaining_children.next() {
            let child_path = if top.current_path.is_empty() {
                child_key.clone()
            } else {
                format!("{}.{}", top.current_path, child_key)
            };

            let mut child_map = child_val.into_object().unwrap_or_default();
            let child_id = child_map.swap_remove("id").and_then(|v| v.into_string());

            let child_effective_id = if let Some(ref cid) = child_id {
                cid.clone()
            } else if top.effective_id.is_empty() || top.effective_id == "(machine)" {
                child_path.clone()
            } else {
                format!("{}.{}", top.effective_id, child_key)
            };

            let default_id = format!("{}.{}", machine_id, child_path);
            if let Some(id) = &child_id {
                id_map.insert(id.clone(), child_path.clone());
                id_map.insert(default_id.clone(), child_path.clone());
            } else {
                id_map.insert(default_id.clone(), child_path.clone());
            }
            if child_effective_id != default_id && child_id.as_deref() != Some(&child_effective_id) {
                id_map.insert(child_effective_id.clone(), child_path.clone());
            }

            let output_opt = child_map.swap_remove("output");

            let raw_type = child_map.swap_remove("type").and_then(|v| v.into_string());

            let (initial_str, initial_actions) =
                resolve_initial_info(child_map.swap_remove("initial"));
            let initial_str = initial_str.map(|s| {
                normalize_initial_str(
                    s,
                    machine_id,
                    &child_path,
                    Some(&child_effective_id),
                    child_id.as_deref(),
                )
            });

            let history_val = child_map.swap_remove("history");
            let history_present = !matches!(
                &history_val,
                None | Some(Value::Null) | Some(Value::Bool(false))
            );
            let is_deep_history = match &history_val {
                Some(Value::String(s)) if s == "deep" => true,
                Some(Value::Object(o)) => {
                    matches!(o.get("history"), Some(Value::String(h)) if h == "deep")
                }
                _ => false,
            };

            let target = parse_string_list(child_map.swap_remove("target"));
            let history_target = target.into_iter().next().map(normalize_transition_target);
            let history_actions = parse_action_list(child_map.swap_remove("actions"));

            let on_done = parse_transition_list(child_map.swap_remove("onDone"));
            let mut on_map = parse_on(child_map.swap_remove("on"));
            let always = parse_transition_list(child_map.swap_remove("always"));
            if !always.is_empty() {
                on_map.insert(String::new(), always);
            }
            if !on_done.is_empty() {
                let state_id = child_id.clone().unwrap_or_else(|| default_id.clone());
                on_map.insert(format!("xstate.done.state.{}", state_id), on_done.clone());
                if let Some(ref custom_id) = child_id {
                    if *custom_id != default_id {
                        on_map.insert(format!("xstate.done.state.{}", default_id), on_done);
                    }
                }
            }

            let (after_map, named_after_map) = parse_after(child_map.swap_remove("after"));
            let entry = parse_action_list(child_map.swap_remove("entry"));
            let exit = parse_action_list(child_map.swap_remove("exit"));
            let tags = parse_string_list(child_map.swap_remove("tags"));
            let inv_state_id = child_id.as_deref().unwrap_or(&default_id);
            let invoke = parse_invoke_list(child_map.swap_remove("invoke"), inv_state_id, &mut on_map);
            let meta = child_map.swap_remove("meta");
            let description = child_map.swap_remove("description").and_then(|v| v.into_string());

            let grandchild_map = child_map
                .swap_remove("states")
                .and_then(|v| v.into_object())
                .unwrap_or_default();
            let is_compound = !grandchild_map.is_empty();

            let state_type = match raw_type.as_deref() {
                Some("parallel") => StateType::Parallel,
                Some("history") => StateType::History {
                    deep: is_deep_history,
                },
                Some("final") => StateType::Final,
                Some("compound") => StateType::Compound,
                Some("atomic") => StateType::Atomic,
                _ => {
                    if history_present {
                        StateType::History {
                            deep: is_deep_history,
                        }
                    } else if is_compound {
                        StateType::Compound
                    } else {
                        StateType::Atomic
                    }
                }
            };

            let history = if matches!(state_type, StateType::History { .. }) || history_present {
                if is_deep_history {
                    Some(HistoryType::Deep)
                } else {
                    Some(HistoryType::Shallow)
                }
            } else {
                None
            };

            let initial_val = if matches!(state_type, StateType::Parallel) {
                None
            } else if let Some(init) = initial_str {
                if grandchild_map.contains_key(&init) {
                    Some(init)
                } else {
                    let matched_key = grandchild_map.iter().find_map(|(k, v)| {
                        if let Value::Object(o) = v {
                            if let Some(Value::String(cid)) = o.get("id") {
                                if cid == &init {
                                    return Some(k.clone());
                                }
                            }
                        }
                        None
                    });
                    Some(matched_key.unwrap_or(init))
                }
            } else if grandchild_map.len() == 1 {
                grandchild_map.keys().next().cloned()
            } else {
                None
            };

            let child_state_def = StateDefinition {
                id: child_id,
                state_type,
                initial: initial_val,
                initial_actions,
                states: IndexMap::new(),
                on: on_map,
                after: after_map,
                named_after: named_after_map,
                entry,
                exit,
                history,
                history_target,
                tags,
                invoke,
                history_actions,
                meta,
                description,
                output: output_opt,
            };

            stack.push(StateWorkFrame {
                key: child_key,
                current_path: child_path,
                effective_id: child_effective_id,
                state_def: child_state_def,
                remaining_children: grandchild_map.into_iter(),
            });
        } else {
            let mut finished = stack.pop().unwrap();
            if let Some(parent) = stack.last_mut() {
                parent
                    .state_def
                    .states
                    .insert(finished.key, finished.state_def);
            } else {
                return core::mem::take(&mut finished.state_def.states);
            }
        }
    }

    IndexMap::new()
}

fn normalize_initial_str(
    s: String,
    machine_id: &str,
    parent_path: &str,
    effective_id: Option<&str>,
    custom_id: Option<&str>,
) -> String {
    let mut clean = s.as_str();
    if let Some(stripped) = clean.strip_prefix('#') {
        clean = stripped;
    }
    while let Some(stripped) = clean.strip_prefix("(machine).") {
        clean = stripped;
    }
    if !machine_id.is_empty() && machine_id != "(machine)" {
        let prefix = format!("{}.", machine_id);
        while let Some(stripped) = clean.strip_prefix(&prefix) {
            clean = stripped;
        }
        while let Some(stripped) = clean.strip_prefix("(machine).") {
            clean = stripped;
        }
    }
    if let Some(cid) = custom_id {
        if !cid.is_empty() && cid != "(machine)" {
            let prefix = format!("{}.", cid);
            while let Some(stripped) = clean.strip_prefix(&prefix) {
                clean = stripped;
            }
            while let Some(stripped) = clean.strip_prefix("(machine).") {
                clean = stripped;
            }
        }
    }
    if let Some(eid) = effective_id {
        if !eid.is_empty() && eid != "(machine)" {
            let prefix = format!("{}.", eid);
            while let Some(stripped) = clean.strip_prefix(&prefix) {
                clean = stripped;
            }
            while let Some(stripped) = clean.strip_prefix("(machine).") {
                clean = stripped;
            }
        }
    }
    if !parent_path.is_empty() {
        let prefix = format!("{}.", parent_path);
        while let Some(stripped) = clean.strip_prefix(&prefix) {
            clean = stripped;
        }
        while let Some(stripped) = clean.strip_prefix("(machine).") {
            clean = stripped;
        }
    }
    clean.to_string()
}

fn resolve_initial_info(v: Option<Value>) -> (Option<String>, Vec<Action>) {
    match v {
        Some(mut val) => match &mut val {
            Value::String(s) => (Some(core::mem::take(s)), Vec::new()),
            Value::Object(o) => {
                let mut o = core::mem::take(o);
                let target = match o.swap_remove("target") {
                    Some(mut t) => match &mut t {
                        Value::String(s) => Some(core::mem::take(s)),
                        Value::Array(t_arr) => core::mem::take(t_arr)
                            .into_iter()
                            .find_map(|item| item.into_string()),
                        _ => None,
                    },
                    None => None,
                };
                let actions = parse_action_list(o.swap_remove("actions"));
                (target, actions)
            }
            _ => (None, Vec::new()),
        },
        None => (None, Vec::new()),
    }
}

fn parse_string_list(v: Option<Value>) -> Vec<String> {
    match v {
        Some(mut val) => match &mut val {
            Value::String(s) => vec![core::mem::take(s)],
            Value::Array(arr) => core::mem::take(arr)
                .into_iter()
                .filter_map(|item| item.into_string())
                .collect(),
            _ => Vec::new(),
        },
        None => Vec::new(),
    }
}

fn parse_action(mut val: Value) -> Action {
    match &mut val {
        Value::String(s) => Action::Named(core::mem::take(s)),
        Value::Object(obj) => {
            let mut obj = core::mem::take(obj);
            let action_type = obj
                .swap_remove("type")
                .and_then(|v| v.into_string())
                .unwrap_or_else(|| "unknown".to_string());
            let assignment = obj.swap_remove("assignment");
            let mut params = obj.swap_remove("params");
            let event = obj.swap_remove("event");

            if action_type == "xstate.raise.delayed" || action_type == "xstate.raise" {
                let delay_val = obj.swap_remove("delay").or_else(|| {
                    if let Some(Value::Object(ref mut m)) = params {
                        m.swap_remove("delay")
                    } else {
                        None
                    }
                });
                let id_val = obj.swap_remove("id").or_else(|| {
                    if let Some(Value::Object(ref mut m)) = params {
                        m.swap_remove("id")
                    } else {
                        None
                    }
                }).and_then(|v| v.into_string());

                if let Some(d) = delay_val {
                    let delay = match &d {
                        Value::Number(n) => Delay::Literal(*n as u64),
                        Value::String(s) => {
                            if let Ok(ms) = s.parse::<u64>() {
                                Delay::Literal(ms)
                            } else {
                                Delay::Named(s.clone())
                            }
                        }
                        _ => Delay::Literal(0),
                    };
                    let event_val = event.or_else(|| {
                        if let Some(Value::Object(ref mut m)) = params {
                            m.swap_remove("event")
                        } else {
                            None
                        }
                    }).unwrap_or(Value::Null);
                    return Action::DelayedRaise(DelayedRaiseAction {
                        event: event_val,
                        delay,
                        id: id_val,
                    });
                } else if let Some(e) = event {
                    return Action::Raise(e);
                } else if let Some(mut p) = params {
                    if let Value::Object(ref mut m) = p {
                        if let Some(inner_evt) = m.swap_remove("event") {
                            return Action::Raise(inner_evt);
                        }
                    }
                    return Action::Raise(p);
                }
            }
            if action_type == "xstate.cancel" {
                let send_id = obj.swap_remove("sendId")
                    .or_else(|| obj.swap_remove("id"))
                    .and_then(|v| v.into_string())
                    .or_else(|| {
                        if let Some(Value::Object(ref mut m)) = params {
                            m.swap_remove("sendId")
                                .or_else(|| m.swap_remove("id"))
                                .and_then(|v| v.into_string())
                        } else {
                            params.and_then(|v| v.into_string())
                        }
                    })
                    .unwrap_or_default();
                return Action::Cancel(CancelAction { send_id });
            }
            if action_type == "xstate.raise.invalid" || action_type == "xstate.sendTo.invalid" {
                let error_msg = obj
                    .swap_remove("error")
                    .and_then(|v| v.into_string())
                    .unwrap_or_default();
                return Action::RaiseInvalid(error_msg);
            }
            if action_type == "xstate.log" {
                let value = obj.swap_remove("value")
                    .or_else(|| {
                        if let Some(Value::Object(ref mut m)) = params {
                            m.swap_remove("value")
                        } else {
                            None
                        }
                    })
                    .unwrap_or(Value::Null);
                let label = obj.swap_remove("label")
                    .or_else(|| {
                        if let Some(Value::Object(ref mut m)) = params {
                            m.swap_remove("label")
                        } else {
                            None
                        }
                    })
                    .and_then(|v| v.into_string());
                return Action::Log(LogAction { value, label });
            }
            if action_type == "xstate.assign" {
                if let Some(a) = assignment {
                    return Action::Assign(AssignAction {
                        action_type,
                        assignment: a,
                    });
                } else if let Some(mut p) = params {
                    if let Value::Object(ref mut m) = p {
                        if let Some(inner_assign) = m.swap_remove("assignment") {
                            return Action::Assign(AssignAction {
                                action_type,
                                assignment: inner_assign,
                            });
                        }
                    }
                    return Action::Assign(AssignAction {
                        action_type,
                        assignment: p,
                    });
                }
            }
            Action::Generic(GenericAction {
                action_type,
                params,
            })
        }
        _ => Action::Named("unknown".to_string()),
    }
}

fn parse_action_list(v: Option<Value>) -> Vec<Action> {
    match v {
        Some(mut val) => match &mut val {
            Value::Array(arr) => core::mem::take(arr).into_iter().map(parse_action).collect(),
            Value::String(s) => vec![Action::Named(core::mem::take(s))],
            Value::Object(obj) => vec![parse_action(Value::Object(core::mem::take(obj)))],
            _ => Vec::new(),
        },
        None => Vec::new(),
    }
}

enum ParseGuardFrame {
    And {
        children: Vec<TransitionGuard>,
        remaining: Vec<Value>,
    },
    Or {
        children: Vec<TransitionGuard>,
        remaining: Vec<Value>,
    },
    Not,
}

fn parse_guard(val: Value) -> Option<TransitionGuard> {
    let mut stack: Vec<ParseGuardFrame> = Vec::new();
    let mut current_val: Option<Value> = Some(val);
    let mut last_guard: Option<TransitionGuard> = None;

    loop {
        if let Some(mut v) = current_val.take() {
            match &mut v {
                Value::String(name) => {
                    last_guard = Some(TransitionGuard::Name(core::mem::take(name)));
                }
                Value::Object(obj) => {
                    let mut obj = core::mem::take(obj);
                    let guard_type = obj
                        .swap_remove("type")
                        .and_then(|t| t.into_string())
                        .unwrap_or_default();
                    match guard_type.as_str() {
                        "xstate.and" => {
                            let mut guards_arr = obj
                                .swap_remove("guards")
                                .and_then(|v| v.into_array())
                                .unwrap_or_default();
                            guards_arr.reverse();
                            if guards_arr.is_empty() {
                                last_guard = Some(TransitionGuard::And(Vec::new()));
                            } else {
                                let first = guards_arr.pop().unwrap();
                                stack.push(ParseGuardFrame::And {
                                    children: Vec::new(),
                                    remaining: guards_arr,
                                });
                                current_val = Some(first);
                            }
                        }
                        "xstate.or" => {
                            let mut guards_arr = obj
                                .swap_remove("guards")
                                .and_then(|v| v.into_array())
                                .unwrap_or_default();
                            guards_arr.reverse();
                            if guards_arr.is_empty() {
                                last_guard = Some(TransitionGuard::Or(Vec::new()));
                            } else {
                                let first = guards_arr.pop().unwrap();
                                stack.push(ParseGuardFrame::Or {
                                    children: Vec::new(),
                                    remaining: guards_arr,
                                });
                                current_val = Some(first);
                            }
                        }
                        "xstate.not" => {
                            let inner = obj.swap_remove("guard")
                                .or_else(|| {
                                    obj.swap_remove("guards")
                                        .and_then(|v| v.into_array())
                                        .and_then(|mut arr| if !arr.is_empty() { Some(arr.swap_remove(0)) } else { None })
                                })
                                .unwrap_or(Value::Null);
                            stack.push(ParseGuardFrame::Not);
                            current_val = Some(inner);
                        }
                        "xstate.stateIn" => {
                            let state_val = obj.swap_remove("stateValue").unwrap_or(Value::Null);
                            last_guard = Some(TransitionGuard::StateIn(state_val));
                        }
                        _ => {
                            let params = obj.swap_remove("params");
                            last_guard = Some(TransitionGuard::Object(GuardObject { guard_type, params }));
                        }
                    }
                }
                _ => {
                    last_guard = None;
                }
            }
        } else if let Some(g) = last_guard.take() {
            if let Some(top) = stack.pop() {
                match top {
                    ParseGuardFrame::Not => {
                        last_guard = Some(TransitionGuard::Not(Box::new(g)));
                    }
                    ParseGuardFrame::And { mut children, mut remaining } => {
                        children.push(g);
                        if let Some(next) = remaining.pop() {
                            stack.push(ParseGuardFrame::And { children, remaining });
                            current_val = Some(next);
                        } else {
                            last_guard = Some(TransitionGuard::And(children));
                        }
                    }
                    ParseGuardFrame::Or { mut children, mut remaining } => {
                        children.push(g);
                        if let Some(next) = remaining.pop() {
                            stack.push(ParseGuardFrame::Or { children, remaining });
                            current_val = Some(next);
                        } else {
                            last_guard = Some(TransitionGuard::Or(children));
                        }
                    }
                }
            } else {
                return Some(g);
            }
        } else {
            break;
        }
    }
    last_guard
}

fn normalize_transition_target(s: String) -> String {
    let mut clean = s.as_str();
    let had_hash = clean.starts_with('#');
    if let Some(stripped) = clean.strip_prefix('#') {
        clean = stripped;
    }
    while clean.starts_with("(machine).(machine).") {
        clean = &clean["(machine).".len()..];
    }
    if had_hash {
        format!("#{}", clean)
    } else {
        clean.to_string()
    }
}

fn parse_transition_object(mut obj: IndexMap<String, Value>) -> TransitionDefinition {
    let target = match obj.swap_remove("target") {
        Some(mut val) => match &mut val {
            Value::String(s) => Some(vec![normalize_transition_target(core::mem::take(s))]),
            Value::Array(arr) => {
                let v: Vec<String> = core::mem::take(arr)
                    .into_iter()
                    .filter_map(|item| item.into_string().map(normalize_transition_target))
                    .collect();
                if v.is_empty() {
                    None
                } else {
                    Some(v)
                }
            }
            _ => None,
        },
        None => None,
    };
    let guard_val = obj.swap_remove("guard").or_else(|| obj.swap_remove("cond"));
    let guard = guard_val.and_then(parse_guard);
    let actions = parse_action_list(obj.swap_remove("actions"));
    let reenter = obj
        .swap_remove("reenter")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let description = obj
        .swap_remove("description")
        .and_then(|v| v.into_string());

    TransitionDefinition {
        target,
        guard,
        actions,
        reenter,
        description,
    }
}

fn parse_single_transition(mut val: Value) -> TransitionDefinition {
    match &mut val {
        Value::String(target) => TransitionDefinition {
            target: Some(vec![normalize_transition_target(core::mem::take(target))]),
            guard: None,
            actions: Vec::new(),
            reenter: false,
            description: None,
        },
        Value::Object(obj) => parse_transition_object(core::mem::take(obj)),
        _ => TransitionDefinition {
            target: None,
            guard: None,
            actions: Vec::new(),
            reenter: false,
            description: None,
        },
    }
}

fn parse_transition_list(v: Option<Value>) -> Vec<TransitionDefinition> {
    match v {
        Some(mut val) => match &mut val {
            Value::Array(arr) => core::mem::take(arr)
                .into_iter()
                .map(parse_single_transition)
                .collect(),
            Value::String(s) => vec![TransitionDefinition {
                target: Some(vec![normalize_transition_target(core::mem::take(s))]),
                guard: None,
                actions: Vec::new(),
                reenter: false,
                description: None,
            }],
            Value::Object(obj) => vec![parse_transition_object(core::mem::take(obj))],
            _ => Vec::new(),
        },
        None => Vec::new(),
    }
}

fn parse_on(v: Option<Value>) -> IndexMap<String, Vec<TransitionDefinition>> {
    let mut result = IndexMap::new();
    if let Some(map) = v.and_then(|v| v.into_object()) {
        for (k, val) in map {
            result.insert(k, parse_transition_list(Some(val)));
        }
    }
    result
}

fn parse_after(v: Option<Value>) -> (IndexMap<u64, Vec<TransitionDefinition>>, IndexMap<String, Vec<TransitionDefinition>>) {
    let mut num_after: IndexMap<u64, Vec<TransitionDefinition>> = IndexMap::new();
    let mut named_after: IndexMap<String, Vec<TransitionDefinition>> = IndexMap::new();
    if let Some(mut val) = v {
        match &mut val {
            Value::Object(map) => {
                for (k, val) in core::mem::take(map) {
                    let transitions = parse_transition_list(Some(val));
                    if let Ok(ms) = k.parse::<u64>() {
                        num_after.entry(ms).or_default().extend(transitions);
                    } else {
                        named_after.entry(k).or_default().extend(transitions);
                    }
                }
            }
            Value::Array(arr) => {
                for item in core::mem::take(arr) {
                    if let Some(mut map) = item.into_object() {
                        if let Some(delay_val) = map.swap_remove("delay") {
                            let transition = parse_transition_object(map);
                            match &delay_val {
                                Value::Number(n) => {
                                    num_after.entry(*n as u64).or_default().push(transition);
                                }
                                Value::String(s) => {
                                    if let Ok(ms) = s.parse::<u64>() {
                                        num_after.entry(ms).or_default().push(transition);
                                    } else {
                                        named_after.entry(s.clone()).or_default().push(transition);
                                    }
                                }
                                _ => {
                                    num_after.entry(0).or_default().push(transition);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    (num_after, named_after)
}

fn parse_invoke_list(
    v: Option<Value>,
    state_id: &str,
    on_map: &mut IndexMap<String, Vec<TransitionDefinition>>,
) -> Vec<InvokeDefinition> {
    let items = match v {
        Some(mut v) => {
            if let Value::Array(arr) = &mut v {
                core::mem::take(arr)
            } else {
                vec![v]
            }
        }
        None => Vec::new(),
    };
    items
        .into_iter()
        .enumerate()
        .filter_map(|(index, mut item)| match &mut item {
            Value::Object(o) => {
                let mut o = core::mem::take(o);
                let src = o.swap_remove("src").unwrap_or(Value::Null);
                let id = o.swap_remove("id").and_then(|v| v.into_string());
                let input = o.swap_remove("input");
                let invoke_id = id.clone().unwrap_or_else(|| format!("{}.{}", index, state_id));

                if let Some(on_done) = o.swap_remove("onDone") {
                    let event_name = format!("xstate.done.actor.{}", invoke_id);
                    if !on_map.contains_key(&event_name) {
                        let transitions = parse_transition_list(Some(on_done));
                        on_map.insert(event_name, transitions);
                    }
                }

                if let Some(on_error) = o.swap_remove("onError") {
                    let event_name = format!("xstate.error.actor.{}", invoke_id);
                    if !on_map.contains_key(&event_name) {
                        let transitions = parse_transition_list(Some(on_error));
                        on_map.insert(event_name, transitions);
                    }
                }

                Some(InvokeDefinition {
                    src,
                    id: Some(invoke_id),
                    input,
                })
            }
            Value::String(s) => {
                let invoke_id = format!("{}.{}", index, state_id);
                Some(InvokeDefinition {
                    src: Value::String(core::mem::take(s)),
                    id: Some(invoke_id),
                    input: None,
                })
            }
            _ => None,
        })
        .collect()
}
