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
        } else if let Some(val) = pending_val.take() {
            match val {
                Value::String(s) => {
                    completed = Some(StateValue::Atomic(s));
                }
                Value::Object(obj) => {
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
    let mut root = match val {
        Value::Object(map) => map,
        _ => {
            return Err(JsonError {
                message: "Expected JSON object for machine definition".to_string(),
                offset: 0,
            });
        }
    };

    let id = match root.swap_remove("id") {
        Some(Value::String(s)) => s,
        _ => "(machine)".to_string(),
    };

    let raw_type = match root.swap_remove("type") {
        Some(Value::String(s)) => Some(s),
        _ => None,
    };

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
    let root_children = match states_val {
        Some(Value::Object(map)) => map.into_iter(),
        _ => return IndexMap::new(),
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

            let mut child_map = match child_val {
                Value::Object(map) => map,
                _ => IndexMap::new(),
            };

            let child_id = match child_map.swap_remove("id") {
                Some(Value::String(s)) => Some(s),
                _ => None,
            };

            if let Some(id) = &child_id {
                id_map.insert(id.clone(), child_path.clone());
            } else {
                let default_id = format!("{}.{}", machine_id, child_path);
                id_map.insert(default_id, child_path.clone());
            }

            let raw_type = match child_map.swap_remove("type") {
                Some(Value::String(s)) => Some(s),
                _ => None,
            };

            let (initial_str, initial_actions) =
                resolve_initial_info(child_map.swap_remove("initial"));

            let history_val = child_map.swap_remove("history");
            let history_present = match &history_val {
                None | Some(Value::Null) | Some(Value::Bool(false)) => false,
                _ => true,
            };
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

            let grandchild_map = match child_map.swap_remove("states") {
                Some(Value::Object(m)) => m,
                _ => IndexMap::new(),
            };
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

            let child_state_def = StateDefinition {
                id: child_id,
                type_: inferred_type,
                state_type,
                initial: initial_str,
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
            let finished = stack.pop().unwrap();
            if let Some(parent) = stack.last_mut() {
                parent
                    .state_def
                    .states
                    .insert(finished.key, finished.state_def);
            } else {
                return finished.state_def.states;
            }
        }
    }

    IndexMap::new()
}

fn resolve_initial_info(v: Option<Value>) -> (Option<String>, Vec<Action>) {
    match v {
        Some(Value::String(s)) => (Some(s), Vec::new()),
        Some(Value::Object(mut o)) => {
            let target = match o.swap_remove("target") {
                Some(Value::String(t)) => Some(t),
                Some(Value::Array(t_arr)) => t_arr.into_iter().find_map(|item| match item {
                    Value::String(s) => Some(s),
                    _ => None,
                }),
                _ => None,
            };
            let actions = parse_action_list(o.swap_remove("actions"));
            (target, actions)
        }
        _ => (None, Vec::new()),
    }
}

fn parse_string_list(v: Option<Value>) -> Vec<String> {
    match v {
        Some(Value::String(s)) => vec![s],
        Some(Value::Array(arr)) => arr
            .into_iter()
            .filter_map(|item| match item {
                Value::String(s) => Some(s),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_action(val: Value) -> Action {
    match val {
        Value::String(s) => Action::Named(s),
        Value::Object(mut obj) => {
            let action_type = match obj.swap_remove("type") {
                Some(Value::String(s)) => s,
                _ => "unknown".to_string(),
            };
            let assignment = obj.swap_remove("assignment");
            let params = obj.swap_remove("params");
            let event = obj.swap_remove("event");

            if action_type == "xstate.raise" {
                if let Some(e) = event {
                    return Action::Raise(e);
                } else if let Some(p) = params {
                    return Action::Raise(p);
                }
            }
            if action_type == "xstate.assign" {
                if let Some(a) = assignment {
                    return Action::Assign(AssignAction {
                        action_type,
                        assignment: a,
                    });
                } else if let Some(p) = params {
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
        Some(Value::Array(arr)) => arr.into_iter().map(parse_action).collect(),
        Some(Value::String(s)) => vec![Action::Named(s)],
        Some(Value::Object(obj)) => vec![parse_action(Value::Object(obj))],
        _ => Vec::new(),
    }
}

fn parse_guard(val: Value) -> Option<TransitionGuard> {
    match val {
        Value::String(name) => Some(TransitionGuard::Name(name)),
        Value::Object(mut obj) => {
            let guard_type = match obj.swap_remove("type") {
                Some(Value::String(s)) => s,
                _ => String::new(),
            };
            let params = obj.swap_remove("params");
            Some(TransitionGuard::Object(GuardObject { guard_type, params }))
        }
        _ => None,
    }
}

fn parse_transition_object(mut obj: IndexMap<String, Value>) -> TransitionDefinition {
    let target = match obj.swap_remove("target") {
        Some(Value::String(s)) => Some(vec![s]),
        Some(Value::Array(arr)) => {
            let v: Vec<String> = arr
                .into_iter()
                .filter_map(|item| match item {
                    Value::String(s) => Some(s),
                    _ => None,
                })
                .collect();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        }
        _ => None,
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

fn parse_single_transition(val: Value) -> TransitionDefinition {
    match val {
        Value::String(target) => TransitionDefinition {
            target: Some(vec![target]),
            cond: None,
            actions: Vec::new(),
            reenter: false,
        },
        Value::Object(obj) => parse_transition_object(obj),
        _ => TransitionDefinition {
            target: Some(vec![String::new()]),
            cond: None,
            actions: Vec::new(),
            reenter: false,
        },
    }
}

fn parse_transition_list(v: Option<Value>) -> Vec<TransitionDefinition> {
    match v {
        Some(Value::Array(arr)) => arr.into_iter().map(parse_single_transition).collect(),
        Some(Value::String(s)) => vec![TransitionDefinition {
            target: Some(vec![s]),
            cond: None,
            actions: Vec::new(),
            reenter: false,
        }],
        Some(Value::Object(obj)) => vec![parse_transition_object(obj)],
        _ => Vec::new(),
    }
}

fn parse_on(v: Option<Value>) -> IndexMap<String, Vec<TransitionDefinition>> {
    let mut result = IndexMap::new();
    if let Some(Value::Object(map)) = v {
        for (k, val) in map {
            result.insert(k, parse_transition_list(Some(val)));
        }
    }
    result
}

fn parse_after(v: Option<Value>) -> IndexMap<u64, Vec<TransitionDefinition>> {
    let mut result: IndexMap<u64, Vec<TransitionDefinition>> = IndexMap::new();
    match v {
        Some(Value::Object(map)) => {
            for (k, val) in map {
                if let Ok(delay) = k.parse::<u64>() {
                    let transitions = parse_transition_list(Some(val));
                    result.entry(delay).or_default().extend(transitions);
                }
            }
        }
        Some(Value::Array(arr)) => {
            for item in arr {
                if let Value::Object(mut map) = item {
                    if let Some(delay_val) = map.swap_remove("delay") {
                        let delay_opt = match delay_val {
                            Value::Number(n) => Some(n as u64),
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
    result
}

fn parse_invoke_list(v: Option<Value>) -> Vec<InvokeDefinition> {
    let items = match v {
        Some(Value::Array(arr)) => arr,
        Some(other) => vec![other],
        None => Vec::new(),
    };
    items
        .into_iter()
        .filter_map(|item| match item {
            Value::Object(mut o) => {
                let src = o.swap_remove("src").unwrap_or(Value::Null);
                let id = match o.swap_remove("id") {
                    Some(Value::String(s)) => Some(s),
                    _ => None,
                };
                Some(InvokeDefinition { src, id })
            }
            Value::String(s) => Some(InvokeDefinition {
                src: Value::String(s),
                id: None,
            }),
            _ => None,
        })
        .collect()
}
