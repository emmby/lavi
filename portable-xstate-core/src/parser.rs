use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use indexmap::IndexMap;

use crate::json::JsonError;
use crate::{
    Action, AssignAction, GenericAction, GuardObject, HistoryType, InvokeDefinition,
    MachineDefinition, StateDefinition, StateType, StateValue, TransitionDefinition,
    TransitionGuard, Value,
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
                            let region_val = match &comp {
                                StateValue::Compound(sub_k, _) if sub_k == &k => comp,
                                _ => StateValue::Compound(k.clone(), Box::new(comp)),
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
                    completed = Some(StateValue::Atomic(core::mem::take(s)));
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

    let id = root
        .swap_remove("id")
        .and_then(|v| v.into_string())
        .unwrap_or_else(|| "(machine)".to_string());

    let raw_type = root.swap_remove("type").and_then(|v| v.into_string());

    let (initial_str, initial_actions) = resolve_initial_info(root.swap_remove("initial"));
    let states_val = root.swap_remove("states");

    let mut on = parse_on(root.swap_remove("on"));
    let always = parse_transition_list(root.swap_remove("always"));
    if !always.is_empty() {
        on.insert(String::new(), always);
    }

    let after = parse_after(root.swap_remove("after"));
    let entry = parse_action_list(root.swap_remove("entry"));
    let exit = parse_action_list(root.swap_remove("exit"));
    let invoke = parse_invoke_list(root.swap_remove("invoke"));
    let context = root.swap_remove("context");
    let types = root.swap_remove("types");

    let mut id_map = IndexMap::new();
    let states = parse_states_iterative(states_val, &id, &mut id_map);

    let initial = initial_str
        .or_else(|| states.keys().next().cloned())
        .unwrap_or_default();

    let is_compound = !states.is_empty();
    let is_parallel = raw_type.as_deref() == Some("parallel");
    let inferred_type = raw_type.or_else(|| {
        if is_compound {
            Some("compound".to_string())
        } else if is_parallel {
            Some("parallel".to_string())
        } else {
            Some("atomic".to_string())
        }
    });

    let state_type = match inferred_type.as_deref() {
        Some("parallel") => StateType::Parallel,
        _ if !states.is_empty() => StateType::Compound,
        _ => StateType::Atomic,
    };

    Ok(MachineDefinition {
        id,
        type_: inferred_type,
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
    })
}

struct StateWorkFrame {
    key: String,
    current_path: String,
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

    let root_frame = StateWorkFrame {
        key: String::new(),
        current_path: String::new(),
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

            if let Some(id) = &child_id {
                id_map.insert(id.clone(), child_path.clone());
            } else {
                let default_id = format!("{}.{}", machine_id, child_path);
                id_map.insert(default_id, child_path.clone());
            }

            let raw_type = child_map.swap_remove("type").and_then(|v| v.into_string());

            let (initial_str, initial_actions) =
                resolve_initial_info(child_map.swap_remove("initial"));

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
            let history_target = target.into_iter().next();
            let history_actions = parse_action_list(child_map.swap_remove("actions"));

            let mut on_map = parse_on(child_map.swap_remove("on"));
            let always = parse_transition_list(child_map.swap_remove("always"));
            if !always.is_empty() {
                on_map.insert(String::new(), always);
            }

            let after_map = parse_after(child_map.swap_remove("after"));
            let entry = parse_action_list(child_map.swap_remove("entry"));
            let exit = parse_action_list(child_map.swap_remove("exit"));
            let tags = parse_string_list(child_map.swap_remove("tags"));
            let invoke = parse_invoke_list(child_map.swap_remove("invoke"));

            let grandchild_map = child_map
                .swap_remove("states")
                .and_then(|v| v.into_object())
                .unwrap_or_default();
            let is_compound = !grandchild_map.is_empty();
            let is_parallel = raw_type.as_deref() == Some("parallel");

            let inferred_type = raw_type.or_else(|| {
                if history_present {
                    Some("history".to_string())
                } else if is_compound {
                    Some("compound".to_string())
                } else if is_parallel {
                    Some("parallel".to_string())
                } else {
                    Some("atomic".to_string())
                }
            });

            let state_type = match inferred_type.as_deref() {
                Some("parallel") => StateType::Parallel,
                Some("history") => StateType::History {
                    deep: is_deep_history,
                },
                Some("final") => StateType::Final,
                _ if is_compound => StateType::Compound,
                _ => StateType::Atomic,
            };

            let history = if inferred_type.as_deref() == Some("history") || history_present {
                if is_deep_history {
                    Some(HistoryType::Deep)
                } else {
                    Some(HistoryType::Shallow)
                }
            } else {
                None
            };

            let resolved_initial = if matches!(state_type, StateType::Compound) {
                initial_str.or_else(|| grandchild_map.keys().next().cloned())
            } else {
                initial_str
            };

            let child_state_def = StateDefinition {
                id: child_id,
                type_: inferred_type,
                state_type,
                initial: resolved_initial,
                initial_actions,
                states: IndexMap::new(),
                on: on_map,
                after: after_map,
                entry,
                exit,
                history,
                history_target,
                tags,
                invoke,
                history_actions,
            };

            stack.push(StateWorkFrame {
                key: child_key,
                current_path: child_path,
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
            let params = obj.swap_remove("params");
            let event = obj.swap_remove("event");

            if action_type == "xstate.raise" {
                if let Some(e) = event {
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

fn parse_guard(mut val: Value) -> Option<TransitionGuard> {
    match &mut val {
        Value::String(name) => Some(TransitionGuard::Name(core::mem::take(name))),
        Value::Object(obj) => {
            let mut obj = core::mem::take(obj);
            let guard_type = obj
                .swap_remove("type")
                .and_then(|v| v.into_string())
                .unwrap_or_default();
            let params = obj.swap_remove("params");
            Some(TransitionGuard::Object(GuardObject { guard_type, params }))
        }
        _ => None,
    }
}

fn parse_transition_object(mut obj: IndexMap<String, Value>) -> TransitionDefinition {
    let target = match obj.swap_remove("target") {
        Some(mut val) => match &mut val {
            Value::String(s) => Some(vec![core::mem::take(s)]),
            Value::Array(arr) => {
                let v: Vec<String> = core::mem::take(arr)
                    .into_iter()
                    .filter_map(|item| item.into_string())
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
    let cond_val = obj.swap_remove("guard").or_else(|| obj.swap_remove("cond"));
    let cond = cond_val.and_then(parse_guard);
    let actions = parse_action_list(obj.swap_remove("actions"));
    let reenter = obj
        .swap_remove("reenter")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    TransitionDefinition {
        target,
        cond,
        actions,
        reenter,
    }
}

fn parse_single_transition(mut val: Value) -> TransitionDefinition {
    match &mut val {
        Value::String(target) => TransitionDefinition {
            target: Some(vec![core::mem::take(target)]),
            cond: None,
            actions: Vec::new(),
            reenter: false,
        },
        Value::Object(obj) => parse_transition_object(core::mem::take(obj)),
        _ => TransitionDefinition {
            target: None,
            cond: None,
            actions: Vec::new(),
            reenter: false,
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
                target: Some(vec![core::mem::take(s)]),
                cond: None,
                actions: Vec::new(),
                reenter: false,
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

fn parse_after(v: Option<Value>) -> IndexMap<u64, Vec<TransitionDefinition>> {
    let mut result: IndexMap<u64, Vec<TransitionDefinition>> = IndexMap::new();
    if let Some(mut val) = v {
        match &mut val {
            Value::Object(map) => {
                for (k, val) in core::mem::take(map) {
                    if let Ok(delay) = k.parse::<u64>() {
                        let transitions = parse_transition_list(Some(val));
                        result.entry(delay).or_default().extend(transitions);
                    }
                }
            }
            Value::Array(arr) => {
                for item in core::mem::take(arr) {
                    if let Some(mut map) = item.into_object() {
                        if let Some(delay_val) = map.swap_remove("delay") {
                            let delay_opt = match &delay_val {
                                Value::Number(n) => Some(*n as u64),
                                Value::String(s) => s.parse::<u64>().ok(),
                                _ => None,
                            };
                            if let Some(delay) = delay_opt {
                                let transition = parse_transition_object(map);
                                result.entry(delay).or_default().push(transition);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    result
}

fn parse_invoke_list(v: Option<Value>) -> Vec<InvokeDefinition> {
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
        .filter_map(|mut item| match &mut item {
            Value::Object(o) => {
                let mut o = core::mem::take(o);
                let src = o.swap_remove("src").unwrap_or(Value::Null);
                let id = o.swap_remove("id").and_then(|v| v.into_string());
                Some(InvokeDefinition { src, id })
            }
            Value::String(s) => Some(InvokeDefinition {
                src: Value::String(core::mem::take(s)),
                id: None,
            }),
            _ => None,
        })
        .collect()
}
