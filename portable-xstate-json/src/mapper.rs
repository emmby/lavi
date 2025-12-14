use portable_xstate_core::{
    MachineDefinition, StateDefinition, TransitionDefinition, Action, AssignAction, GenericAction,
    GuardObject, TransitionGuard, Value
};
use crate::dto::{
    MachineDefinitionDto, StateDefinitionDto, TransitionDefinitionDto, ActionDto, 
    AssignActionDto, GenericActionDto, GuardObjectDto, TransitionGuardDto
};

pub fn map_machine(dto: MachineDefinitionDto) -> MachineDefinition {
    MachineDefinition {
        id: dto.id,
        initial: dto.initial,
        states: dto.states.into_iter().map(|(k, v)| (k, map_state(v))).collect(),
        context: dto.context.map(map_value),
    }
}

fn map_state(dto: StateDefinitionDto) -> StateDefinition {
    StateDefinition {
        initial: dto.initial,
        states: dto.states.into_iter().map(|(k, v)| (k, map_state(v))).collect(),
        on: dto.on.into_iter().map(|(k, v)| (k, v.into_iter().map(map_transition).collect())).collect(),
        entry: dto.entry.into_iter().map(map_action).collect(),
        exit: dto.exit.into_iter().map(map_action).collect(),
    }
}

fn map_transition(dto: TransitionDefinitionDto) -> TransitionDefinition {
    TransitionDefinition {
        target: dto.target,
        cond: dto.cond.map(map_guard),
        actions: dto.actions.into_iter().map(map_action).collect(),
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

fn map_value(v: serde_json::Value) -> Value {
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
