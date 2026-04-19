use portable_xstate_core::{
    MachineDefinition, StateDefinition, TransitionDefinition, Action, 
    GuardObject, TransitionGuard, Value, HistoryType, StateType
};
use indexmap::IndexMap;
use alloc::vec::Vec;
use alloc::boxed::Box;
use alloc::vec;
use alloc::string::{String, ToString};
use alloc::format;
use crate::dto::{
    MachineDefinitionDto, StateDefinitionDto, TransitionDefinitionDto, ActionDto, 
    TransitionGuardDto
};

fn resolve_initial_info(v: Option<serde_json::Value>) -> (Option<String>, Vec<Action>) {
    match v {
        Some(serde_json::Value::String(s)) => (Some(s), vec![]),
        Some(serde_json::Value::Object(o)) => {
            let target = if let Some(serde_json::Value::String(t)) = o.get("target") {
                Some(t.clone())
            } else if let Some(serde_json::Value::Array(t_arr)) = o.get("target") {
                if let Some(serde_json::Value::String(first)) = t_arr.get(0) {
                     Some(first.clone())
                } else { None }
            } else {
                None
            };
            let actions = if let Some(serde_json::Value::Array(a_arr)) = o.get("actions") {
                a_arr.clone().into_iter().map(|item| {
                    let dto_res: Result<ActionDto, _> = serde_json::from_value(item);
                    if let Ok(dto) = dto_res { map_action(dto) } else { Action::Named("unknown".to_string()) }
                }).collect()
            } else if let Some(a_obj) = o.get("actions") {
                 let dto_res: Result<ActionDto, _> = serde_json::from_value(a_obj.clone());
                 if let Ok(dto) = dto_res { vec![map_action(dto)] } else { vec![] }
            } else {
                vec![]
            };
            (target, actions)
        },
        _ => (None, vec![])
    }
}

pub fn map_machine(dto: MachineDefinitionDto) -> MachineDefinition {
    let machine_id = dto.id.as_deref().unwrap_or("(machine)");
    let mut id_map = IndexMap::new();
    let states = map_states(dto.states, "", machine_id, &mut id_map);
    
    let (initial_str, initial_actions) = resolve_initial_info(dto.initial);
    let initial = initial_str.or_else(|| {
        states.keys().next().cloned()
    }).unwrap_or_default();

    let is_compound = !states.is_empty();
    let is_parallel = dto.type_.as_deref() == Some("parallel");
    let inferred_type = dto.type_.or_else(|| {
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

    MachineDefinition {
        id: dto.id.unwrap_or_else(|| String::from("(machine)")),
        type_: inferred_type,
        state_type,
        initial, 
        initial_actions,
        states,
        on: dto.on.into_iter().map(|(k, v): (String, _)| (k, v.into_iter().map(map_transition).collect())).collect(),
        after: dto.after.into_iter()
            .filter_map(|(k, v)| {
                let delay = k.parse::<u64>().ok()?;
                Some((delay, v.into_iter().map(map_transition).collect()))
            })
            .collect(),
        entry: dto.entry.into_iter().map(map_action).collect(),
        exit: dto.exit.into_iter().map(map_action).collect(),
        invoke: dto.invoke.into_iter().map(|inv| portable_xstate_core::InvokeDefinition {
            src: map_value(inv.src),
            id: inv.id,
        }).collect(),
        context: dto.context.map(map_value),
        types: None,
        id_map,
    }
}

fn map_states(
    states: IndexMap<String, StateDefinitionDto>, 
    parent_path: &str,
    machine_id: &str,
    id_map: &mut IndexMap<String, String>
) -> IndexMap<String, StateDefinition> {
    states.into_iter().map(|(key, dto): (String, _)| {
        let current_path = if parent_path.is_empty() { 
            key.clone() 
        } else { 
            format!("{}.{}", parent_path, key)
        };

        if let Some(id) = &dto.id {
            id_map.insert(id.clone(), current_path.clone());
        } else {
             let default_id = format!("{}.{}", machine_id, current_path);
             id_map.insert(default_id, current_path.clone());
        }

        let StateDefinitionDto {
            id, type_, initial, states: child_states, on, after, entry, exit, always, tags, invoke, history, target, actions, ..
        } = dto;

        let (initial, initial_actions) = resolve_initial_info(initial);

        let is_compound = !child_states.is_empty();
        let is_parallel = type_.as_deref() == Some("parallel");
        
        let inferred_type = type_.or_else(|| {
             if history.is_some() {
                 Some("history".to_string())
             } else if is_compound {
                 Some("compound".to_string()) 
             } else if is_parallel {
                 Some("parallel".to_string()) 
             } else {
                 Some("atomic".to_string())
             }
        });
        
        let children = map_states(child_states, &current_path, machine_id, id_map);
        
        let mut on_map: IndexMap<String, Vec<TransitionDefinition>> = on.into_iter()
            .map(|(k, v): (String, _)| (k, v.into_iter().map(map_transition).collect()))
            .collect();

        if !always.is_empty() {
            let mapped = always.into_iter().map(map_transition).collect();
            on_map.insert("".to_string(), mapped); 
        }

        let after_map: IndexMap<u64, Vec<TransitionDefinition>> = after.into_iter()
            .filter_map(|(k, v)| {
                let delay = k.parse::<u64>().ok()?;
                Some((delay, v.into_iter().map(map_transition).collect()))
            })
            .collect();

        let is_deep_history = match &history {
            Some(serde_json::Value::String(s)) if s == "deep" => true,
            Some(serde_json::Value::Object(o)) => {
                matches!(o.get("history"), Some(serde_json::Value::String(h)) if h == "deep")
            },
            _ => false,
        };
        
        let state_type = match inferred_type.as_deref() {
            Some("parallel") => StateType::Parallel,
            Some("history") => StateType::History { deep: is_deep_history },
            Some("final") => StateType::Final,
            _ if !children.is_empty() => StateType::Compound,
            _ => StateType::Atomic,
        };

        let state_def = StateDefinition {
            id,
            type_: inferred_type.clone(),
            state_type,
            initial, 
            initial_actions,
            states: children,
            on: on_map,
            after: after_map,
            entry: entry.into_iter().map(map_action).collect(),
            exit: exit.into_iter().map(map_action).collect(),
            tags,
            invoke: invoke.into_iter().map(|inv| portable_xstate_core::InvokeDefinition {
                src: map_value(inv.src),
                id: inv.id,
            }).collect(),
            history: if inferred_type.as_deref() == Some("history") || history.is_some() {
                if is_deep_history { Some(HistoryType::Deep) } else { Some(HistoryType::Shallow) }
            } else {
                None
            },
            history_actions: actions.into_iter().map(map_action).collect(),
            history_target: if !target.is_empty() {
                Some(target[0].clone())
            } else {
                None
            },
        };
        
        (key, state_def)
    }).collect()
}

fn map_transition(dto: TransitionDefinitionDto) -> TransitionDefinition {
    match dto {
        TransitionDefinitionDto::Target(target) => TransitionDefinition {
            target: Some(vec![target]),
            cond: None,
            actions: Vec::new(),
            reenter: false,
        },
        TransitionDefinitionDto::Object(dto) => TransitionDefinition {
            target: dto.target.and_then(|t| {
                match t {
                    serde_json::Value::String(s) => Some(vec![s]),
                    serde_json::Value::Array(arr) => {
                        let v: Vec<String> = arr.into_iter().filter_map(|item| item.as_str().map(|s| s.to_string())).collect();
                        if v.is_empty() { None } else { Some(v) }
                    },
                    _ => None,
                }
            }),
            cond: dto.cond.map(map_guard),
            actions: dto.actions.into_iter().map(map_action).collect(),
            reenter: dto.reenter,
        },
    }
}

fn map_action(dto: ActionDto) -> Action {
    match dto {
        ActionDto::Name(s) => Action::Named(s),
        ActionDto::Object { action_type, assignment, params, event } => {
            if action_type == "xstate.raise" {
                if let Some(e) = event {
                    return Action::Raise(map_value(e));
                } else if let Some(p) = params.clone() {
                    return Action::Raise(map_value(p));
                }
            }
            if action_type == "xstate.assign" {
                if let Some(a) = assignment {
                    return Action::Assign(portable_xstate_core::AssignAction {
                        action_type,
                        assignment: map_value(a),
                    });
                } else if let Some(p) = params.clone() {
                    return Action::Assign(portable_xstate_core::AssignAction {
                        action_type,
                        assignment: map_value(p),
                    });
                }
            }
            Action::Generic(portable_xstate_core::GenericAction {
                action_type,
                params: params.map(|p| map_value(p)),
            })
        }
    }
}

fn map_guard(dto: TransitionGuardDto) -> TransitionGuard {
    match dto {
        TransitionGuardDto::Name(name) => TransitionGuard::Name(name),
        TransitionGuardDto::Object(obj) => TransitionGuard::Object(GuardObject {
            guard_type: obj.guard_type,
            params: obj.params.map(map_value),
        }),
    }
}

pub fn map_value(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(arr) => Value::Array(arr.into_iter().map(map_value).collect()),
        serde_json::Value::Object(obj) => Value::Object(obj.into_iter().map(|(k, v)| (k, map_value(v))).collect()),
    }
}
 
pub fn map_value_to_state_value(v: serde_json::Value) -> portable_xstate_core::StateValue {
    match v {
        serde_json::Value::String(s) => portable_xstate_core::StateValue::Atomic(s),
        serde_json::Value::Object(obj) => {
             // Handle parallel state case or nested compound
             // XState 5 represents compound as { parent: 'child' } 
             // and parallel as { root: { r1: 's1', r2: 's2' } }
             if obj.len() == 1 {
                 let (k, v) = obj.into_iter().next().unwrap();
                 portable_xstate_core::StateValue::Compound(k, Box::new(map_value_to_state_value(v)))
             } else {
                 let mut regions = IndexMap::new();
                 for (k, v) in obj {
                     regions.insert(k, map_value_to_state_value(v));
                 }
                 portable_xstate_core::StateValue::Parallel(regions)
             }
        },
        _ => portable_xstate_core::StateValue::Atomic("".to_string()),
    }
}
