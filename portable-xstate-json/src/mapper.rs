use portable_xstate_core::{
    MachineDefinition, StateDefinition, TransitionDefinition, Action, AssignAction, GenericAction,
    GuardObject, TransitionGuard, Value, HistoryType
};
use indexmap::IndexMap;
use alloc::vec::Vec;
use alloc::vec;
use alloc::string::{String, ToString};
use alloc::format;
use crate::dto::{
    MachineDefinitionDto, StateDefinitionDto, TransitionDefinitionDto, ActionDto, 
    TransitionGuardDto
};

fn resolve_initial(v: Option<serde_json::Value>) -> Option<String> {
    match v {
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Object(o)) => {
            if let Some(serde_json::Value::String(target)) = o.get("target") {
                Some(target.clone())
            } else {
                None
            }
        },
        _ => None
    }
}

pub fn map_machine(dto: MachineDefinitionDto) -> MachineDefinition {
    let mut id_map = IndexMap::new();
    let states = map_states(dto.states, "", &mut id_map);
    
    let initial = resolve_initial(dto.initial).or_else(|| {
        states.keys().next().cloned()
    }).unwrap_or_default();

    MachineDefinition {
        id: dto.id.unwrap_or_else(|| String::from("(machine)")),
        initial, 
        states,
        on: dto.on.into_iter().map(|(k, v)| (k, v.into_iter().map(map_transition).collect())).collect(),
        entry: dto.entry.into_iter().map(map_action).collect(),
        exit: dto.exit.into_iter().map(map_action).collect(),
        context: dto.context.map(map_value),
        types: dto.types.map(map_value),
        id_map,
    }
}

fn map_states(
    states: IndexMap<String, StateDefinitionDto>, 
    parent_path: &str, 
    id_map: &mut IndexMap<String, String>
) -> IndexMap<String, StateDefinition> {
    states.into_iter().map(|(key, dto)| {
        let current_path = if parent_path.is_empty() { 
            key.clone() 
        } else { 
            format!("{}.{}", parent_path, key)
        };

        if let Some(id) = &dto.id {
            id_map.insert(id.clone(), current_path.clone());
        }

        // Destructure to avoid partial move
        let StateDefinitionDto {
            id, type_, initial, states: child_states, on, entry, exit, always, tags, history
        } = dto;

        let initial = resolve_initial(initial);

        // Validation: Compound states must have initial state (unless type is parallel)
        let is_compound = !child_states.is_empty();
        let is_parallel = type_.as_deref() == Some("parallel");
        
        // Infer Type
        let type_ = type_.or_else(|| {
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
        
        if is_compound && !is_parallel && initial.is_none() {
             panic!("Compound state must specify an initial state (state: '{}').", key);
        }

        let children = map_states(child_states, &current_path, id_map);
        
        let mut on_map: IndexMap<String, Vec<TransitionDefinition>> = on.into_iter()
            .map(|(k, v)| (k, v.into_iter().map(map_transition).collect()))
            .collect();

        if !always.is_empty() {
            let mapped = always.into_iter().map(map_transition).collect();
            on_map.insert("".to_string(), mapped); 
        }

        let state_def = StateDefinition {
            id,
            type_: type_.clone(),
            initial,
            states: children,
            on: on_map,
            entry: entry.into_iter().map(map_action).collect(),
            exit: exit.into_iter().map(map_action).collect(),
            tags,
            history: if type_.as_deref() == Some("history") || history.is_some() {
                match history {
                     Some(serde_json::Value::String(s)) if s == "deep" => Some(HistoryType::Deep),
                     Some(serde_json::Value::Object(o)) => {
                         if let Some(serde_json::Value::String(h)) = o.get("history") {
                             if h == "deep" { Some(HistoryType::Deep) } else { Some(HistoryType::Shallow) }
                         } else {
                             Some(HistoryType::Shallow)
                         }
                     },
                     _ => Some(HistoryType::Shallow),
                }
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
        },
        TransitionDefinitionDto::Object(dto) => TransitionDefinition {
            target: dto.target.and_then(|t| {
                match t {
                    serde_json::Value::String(s) => Some(vec![s]),
                    serde_json::Value::Array(arr) => {
                        let mut v = Vec::new();
                        for item in arr {
                             if let serde_json::Value::String(s) = item {
                                 v.push(s);
                             }
                        }
                        if v.is_empty() { None } else { Some(v) }
                    },
                    _ => None,
                }
            }),
            cond: dto.cond.map(map_guard),
            actions: dto.actions.into_iter().map(map_action).collect(),
        },
    }
}


fn map_action(dto: ActionDto) -> Action {
    match dto {
        ActionDto::Named(s) => Action::Named(s),
        ActionDto::Assign(a) => Action::Assign(AssignAction {
            action_type: a.action_type,
            assignment: map_value(a.assignment),
        }),
        ActionDto::Generic(g) => {
            Action::Generic(GenericAction {
                action_type: g.action_type,
                params: g.params.map(|p| map_value(p)),
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
        serde_json::Value::Number(n) => {
             if let Some(f) = n.as_f64() {
                 Value::Number(f)
             } else if let Some(i) = n.as_i64() {
                 Value::Number(i as f64)
             } else {
                 Value::Number(0.0) // Fallback
             }
        },
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(arr) => Value::Array(arr.into_iter().map(map_value).collect()),
        serde_json::Value::Object(obj) => Value::Object(obj.into_iter().map(|(k, v)| (k, map_value(v))).collect()),
    }
}
