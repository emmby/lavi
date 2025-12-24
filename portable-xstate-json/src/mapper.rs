use portable_xstate_core::{
    MachineDefinition, StateDefinition, TransitionDefinition, Action, AssignAction, GenericAction,
    GuardObject, TransitionGuard, Value
};
use indexmap::IndexMap;
use alloc::vec::Vec;
use alloc::string::{String, ToString};
use crate::dto::{
    MachineDefinitionDto, StateDefinitionDto, TransitionDefinitionDto, ActionDto, 
    TransitionGuardDto
};

pub fn map_machine(dto: MachineDefinitionDto) -> MachineDefinition {
    MachineDefinition {
        id: dto.id.unwrap_or_else(|| String::from("(machine)")),
        initial: dto.initial.unwrap_or_default(), // Or handle missing initial logic?
        // Core MachineDefinition expects String for initial.
        // If missing, what should it be? Empty string?
        // If atomic machine, initial is meaningless.
        // Let's use unwrap_or_default for now (empty string).
        states: dto.states.into_iter().map(|(k, v)| (k, map_state(v))).collect(),
        on: dto.on.into_iter().map(|(k, v)| (k, v.into_iter().map(map_transition).collect())).collect(),
        entry: dto.entry.into_iter().map(map_action).collect(),
        exit: dto.exit.into_iter().map(map_action).collect(),
        context: dto.context.map(map_value),
        types: dto.types.map(map_value),
    }
}

fn map_state(dto: StateDefinitionDto) -> StateDefinition {
    // Validation: Compound states must have initial state (unless type is parallel)
    let is_compound = !dto.states.is_empty();
    let is_parallel = dto.type_.as_deref() == Some("parallel");
    
    if is_compound && !is_parallel && dto.initial.is_none() {
         panic!("Compound state must specify an initial state.");
    }

    let mut on_map: IndexMap<String, Vec<TransitionDefinition>> = dto.on.into_iter()
        .map(|(k, v)| (k, v.into_iter().map(map_transition).collect()))
        .collect();

    // Mapping 'always' property to 'xs.always' or empty string event
    // The runner must know which event string corresponds to always.
    // Let's use "always" (assuming no user event named always) or "xstate.always" (if we control).
    // Or "" (empty string).
    if !dto.always.is_empty() {
        let mapped = dto.always.into_iter().map(map_transition).collect();
        // IndexMap insert overwrites if key exists. But 'always' is additive usually? 
        // No, 'always' is array.
        on_map.insert("".to_string(), mapped); 
    }

    StateDefinition {
        type_: dto.type_,
        initial: dto.initial,
        states: dto.states.into_iter().map(|(k, v)| (k, map_state(v))).collect(),
        on: on_map,
        entry: dto.entry.into_iter().map(map_action).collect(),
        exit: dto.exit.into_iter().map(map_action).collect(),
    }
}

fn map_transition(dto: TransitionDefinitionDto) -> TransitionDefinition {
    match dto {
        TransitionDefinitionDto::Target(target) => TransitionDefinition {
            target: Some(target),
            cond: None,
            actions: Vec::new(),
        },
        TransitionDefinitionDto::Object(dto) => TransitionDefinition {
            target: dto.target,
            cond: dto.cond.map(map_guard),
            actions: dto.actions.into_iter().map(map_action).collect(),
        },
    }
}


fn map_action(dto: ActionDto) -> Action {
    match dto {
        ActionDto::Named(name) => Action::Named(name),
        ActionDto::Assign(assign) => Action::Assign(AssignAction {
            action_type: assign.action_type,
            assignment: map_value(assign.assignment),
        }),
        ActionDto::Generic(generic) => Action::Generic(GenericAction {
            action_type: generic.action_type,
            params: generic.params.map(map_value),
        }),
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
