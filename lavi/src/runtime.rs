use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::clock::IntoClockFn;
use crate::parser::value_to_state_value;
use crate::types::*;

#[derive(Clone)]
pub struct RuntimeMachine {
    pub definition: alloc::rc::Rc<MachineDefinition>,
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
    pub inspector_fn: Option<InspectFunction>,
    pub invoke_start_fn: Option<InvokeStartFunction>,
    pub invoke_stop_fn: Option<InvokeStopFunction>,
    pub active_invokes: Vec<(String, String)>,
    pub pending_init_actions: Option<Vec<InspectEvent>>,
    pub last_error: Option<String>,
    pub last_exited_paths: Vec<String>,
    pub last_entered_paths: Vec<String>,
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
    event_type: String,
    t: &'a TransitionDefinition,
    lowest_domain_depth: usize,
    is_targetless: bool,
    reenters_root: bool,
}

impl Default for RuntimeMachine {
    fn default() -> Self {
        Self {
            definition: alloc::rc::Rc::new(MachineDefinition::default()),
            current_state: StateValue::Parallel(IndexMap::new()),
            context: Value::Object(IndexMap::new()),
            input: None,
            history_value: IndexMap::new(),
            initial_actions: Vec::new(),
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
            state_outputs: IndexMap::new(),
            machine_output: None,
            output_resolvers: IndexMap::new(),
            completed_states: alloc::vec::Vec::new(),
            last_received_event: Value::Null,
            is_machine_completed: false,
            delayed_events: Vec::new(),
            delays: IndexMap::new(),
            actions: IndexMap::new(),
            actions_with_effects: IndexMap::new(),
            logger_fn: None,
            inspector_fn: None,
            invoke_start_fn: None,
            invoke_stop_fn: None,
            active_invokes: Vec::new(),
            pending_init_actions: None,
            last_error: None,
            last_exited_paths: Vec::new(),
            last_entered_paths: Vec::new(),
        }
    }
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
            definition: alloc::rc::Rc::new(definition),
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
            inspector_fn: None,
            invoke_start_fn: None,
            invoke_stop_fn: None,
            active_invokes: Vec::new(),
            pending_init_actions: None,
            last_error: None,
            last_exited_paths: Vec::new(),
            last_entered_paths: Vec::new(),
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

    pub fn register_inspector(&mut self, inspector: InspectFunction) {
        self.inspector_fn = Some(inspector);
    }

    pub fn register_invoke_callbacks(
        &mut self,
        on_start: Option<InvokeStartFunction>,
        on_stop: Option<InvokeStopFunction>,
    ) {
        self.invoke_start_fn = on_start;
        self.invoke_stop_fn = on_stop;
    }

    pub(crate) fn start_invokes_for_path(&mut self, path: &str, event_payload: &Value) {
        if self.invoke_start_fn.is_none() && self.invoke_stop_fn.is_none() {
            return;
        }
        let start_fn = self.invoke_start_fn.clone();
        let invokes = if path.is_empty() {
            self.definition.invoke.clone()
        } else if let Some(def) = Self::get_state_def_recursive(&self.definition.states, path) {
            def.invoke.clone()
        } else {
            return;
        };
        for (inv_idx, inv) in invokes.into_iter().enumerate() {
            let invoke_id = inv.id.unwrap_or_else(|| {
                if path.is_empty() {
                    let root_id = if self.definition.id.is_empty() {
                        "(machine)"
                    } else {
                        &self.definition.id
                    };
                    format!("{}.{}", inv_idx, root_id)
                } else {
                    format!("{}.{}", inv_idx, path)
                }
            });
            self.active_invokes
                .push((path.to_string(), invoke_id.clone()));
            if let Some(ref on_start) = start_fn {
                on_start(
                    &invoke_id,
                    &inv.src,
                    &inv.input,
                    &self.context,
                    event_payload,
                );
            }
        }
    }

    pub(crate) fn stop_invokes_for_path(&mut self, path: &str) {
        if self.active_invokes.is_empty() {
            return;
        }
        let mut stopped_ids = Vec::new();
        self.active_invokes.retain(|(p, id)| {
            let matches_path = if path.is_empty() {
                true
            } else {
                p == path || p.starts_with(&format!("{}.", path))
            };
            if matches_path {
                stopped_ids.push(id.clone());
                false
            } else {
                true
            }
        });
        if let Some(stop_fn) = self.invoke_stop_fn.clone() {
            for id in stopped_ids {
                stop_fn(&id);
            }
        }
    }

    pub fn get_state_node_id(&self, path: &str) -> String {
        let machine_id = if self.definition.id.is_empty() {
            "(machine)"
        } else {
            &self.definition.id
        };
        if path.is_empty() {
            return machine_id.to_string();
        }
        if let Some(def) = Self::get_state_def_recursive(&self.definition.states, path) {
            if let Some(ref explicit_id) = def.id {
                return explicit_id.clone();
            }
        }
        format!("{}.{}", machine_id, path)
    }

    pub(crate) fn make_inspect_action_event(action: &Action) -> Option<InspectEvent> {
        match action {
            Action::Assign(_) | Action::RaiseInvalid(_) => None,
            Action::Named(name) => {
                if name.starts_with("auto_action_assign_")
                    || name.starts_with("generated_assign_")
                    || name.starts_with("auto_action_enqueue_")
                    || name.starts_with("auto_action_dynparams_")
                {
                    None
                } else {
                    Some(InspectEvent::Action {
                        action: action.clone(),
                        params: None,
                    })
                }
            }
            Action::Generic(gen) => {
                if gen.action_type == "xstate.assign"
                    || gen.action_type.starts_with("auto_action_assign_")
                    || gen.action_type.starts_with("generated_assign_")
                    || gen.action_type.starts_with("auto_action_enqueue_")
                    || gen.action_type.starts_with("auto_action_dynparams_")
                {
                    None
                } else {
                    Some(InspectEvent::Action {
                        action: action.clone(),
                        params: gen.params.clone(),
                    })
                }
            }
            Action::Raise(_) | Action::DelayedRaise(_) | Action::Cancel(_) | Action::Log(_) => {
                Some(InspectEvent::Action {
                    action: action.clone(),
                    params: None,
                })
            }
        }
    }

    pub(crate) fn emit_action_inspection(
        inspector_fn: &Option<InspectFunction>,
        pending_init_actions: &mut Option<Vec<InspectEvent>>,
        action: &Action,
    ) {
        if inspector_fn.is_none() {
            return;
        }
        let Some(evt) = Self::make_inspect_action_event(action) else {
            return;
        };
        if let Some(ref mut pending) = pending_init_actions {
            pending.push(evt);
        } else if let Some(ref insp) = inspector_fn {
            insp(&evt);
        }
    }

    pub(crate) fn emit_extra_inspected_actions(
        inspector_fn: &Option<InspectFunction>,
        pending_init_actions: &mut Option<Vec<InspectEvent>>,
        extra_events: Vec<InspectEvent>,
    ) {
        if inspector_fn.is_none() || extra_events.is_empty() {
            return;
        }
        if let Some(ref mut pending) = pending_init_actions {
            pending.extend(extra_events);
        } else if let Some(ref insp) = inspector_fn {
            for evt in &extra_events {
                insp(evt);
            }
        }
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
        m.inspector_fn = None;
        m.invoke_start_fn = None;
        m.invoke_stop_fn = None;
        m.pending_init_actions = None;
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
        m.inspector_fn = None;
        m.invoke_start_fn = None;
        m.invoke_stop_fn = None;
        m.pending_init_actions = None;
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
                Action::Named(name) if name.starts_with("auto_action_assign_") => {
                    if let Some(f) = actions.get(name) {
                        f(&mut m.context, &init_event, &None);
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
                Action::Named(name) if name.starts_with("auto_action_assign_") => {
                    if let Some(f) = actions.get(name) {
                        f(&mut m.context, &init_event, &None);
                    }
                }
                _ => {
                    collected_actions.push(act.clone());
                }
            }
        }

        // Root invoke -> Action::Generic(xstate.spawnChild)
        for (inv_idx, inv) in m.definition.invoke.iter().enumerate() {
            let root_id = if m.definition.id.is_empty() { "(machine)" } else { &m.definition.id };
            let invoke_id = inv.id.clone().unwrap_or_else(|| format!("{}.{}", inv_idx, root_id));
            let mut p_map = IndexMap::new();
            p_map.insert("id".to_string(), Value::String(invoke_id));
            p_map.insert("src".to_string(), inv.src.clone());
            if let Some(ref inp) = inv.input {
                p_map.insert("input".to_string(), inp.clone());
            }
            collected_actions.push(Action::Generic(GenericAction {
                action_type: "xstate.spawnChild".to_string(),
                params: Some(Value::Object(p_map)),
            }));
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
                                Action::Named(name) if name.starts_with("auto_action_assign_") => {
                                    if let Some(f) = actions.get(name) {
                                        f(&mut m.context, &init_event, &None);
                                    }
                                }
                                _ => {
                                    collected_actions.push(act.clone());
                                }
                            }
                        }

                        // State invoke -> Action::Generic(xstate.spawnChild)
                        for (inv_idx, inv) in def.invoke.iter().enumerate() {
                            let invoke_id = inv.id.clone().unwrap_or_else(|| format!("{}.{}", inv_idx, curr_path));
                            let mut p_map = IndexMap::new();
                            p_map.insert("id".to_string(), Value::String(invoke_id));
                            p_map.insert("src".to_string(), inv.src.clone());
                            if let Some(ref inp) = inv.input {
                                p_map.insert("input".to_string(), inp.clone());
                            }
                            collected_actions.push(Action::Generic(GenericAction {
                                action_type: "xstate.spawnChild".to_string(),
                                params: Some(Value::Object(p_map)),
                            }));
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
                                    Action::Named(name) if name.starts_with("auto_action_assign_") => {
                                        if let Some(f) = actions.get(name) {
                                            f(&mut m.context, &init_event, &None);
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
                Action::Named(name) if name.starts_with("auto_action_assign_") => {
                    if let Some(f) = actions.get(name) {
                        f(&mut m.context, &init_event, &None);
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

    pub fn initial_transition_pure_value(&self, input: Option<&Value>) -> Value {
        let (state, context, actions) = self.initial_transition_pure(input);
        let mut map = IndexMap::new();
        map.insert("value".to_string(), state.to_value());
        map.insert("context".to_string(), context);
        let actions_arr = actions.iter().map(Action::to_value).collect();
        map.insert("actions".to_string(), Value::Array(actions_arr));
        Value::Object(map)
    }

    pub fn initial_transition_pure_json(&self, input_json: Option<&str>) -> Result<String, String> {
        let parsed_input = match input_json {
            Some(s) if !s.trim().is_empty() => {
                Some(Value::from_json(s).map_err(|e| e.to_string())?)
            }
            _ => None,
        };
        Ok(self
            .initial_transition_pure_value(parsed_input.as_ref())
            .to_json_string())
    }

    pub fn transition_pure_snapshot_value(
        &self,
        state_or_snapshot: &Value,
        event: &Value,
    ) -> Result<Value, String> {
        let (state_val, ctx_val, history_opt) = match state_or_snapshot {
            Value::Object(obj) if obj.contains_key("value") => {
                let sv = value_to_state_value(obj.get("value").unwrap().clone());
                let ctx = obj
                    .get("context")
                    .cloned()
                    .unwrap_or_else(|| self.context.clone());
                let hist = if let Some(Value::Object(h_map)) = obj.get("historyValue") {
                    let mut parsed_hist = IndexMap::new();
                    for (k, v) in h_map {
                        parsed_hist.insert(k.clone(), value_to_state_value(v.clone()));
                    }
                    Some(parsed_hist)
                } else {
                    None
                };
                (sv, ctx, hist)
            }
            other => {
                let sv = value_to_state_value(other.clone());
                (sv, self.context.clone(), None)
            }
        };

        let (event_type, event_payload) = match event {
            Value::String(s) => (s.clone(), Value::event(s)),
            Value::Object(map) => {
                let ev_type = map
                    .get("type")
                    .and_then(|v| v.as_string())
                    .cloned()
                    .ok_or_else(|| "Event object must contain a string 'type' field".to_string())?;
                (ev_type, event.clone())
            }
            _ => return Err("Event must be a string or an object with a 'type' field".to_string()),
        };

        let (next_state, next_ctx, actions, next_history) = self
            .transition_pure_with_context_and_history(
                &state_val,
                &ctx_val,
                history_opt.as_ref(),
                &event_type,
                &event_payload,
            );

        let mut map = IndexMap::new();
        map.insert("value".to_string(), next_state.to_value());
        map.insert("context".to_string(), next_ctx);
        let actions_arr = actions.iter().map(Action::to_value).collect();
        map.insert("actions".to_string(), Value::Array(actions_arr));
        let mut hist_map = IndexMap::new();
        for (k, v) in next_history {
            hist_map.insert(k, v.to_value());
        }
        map.insert("historyValue".to_string(), Value::Object(hist_map));
        Ok(Value::Object(map))
    }

    pub fn transition_pure_json(
        &self,
        state_or_snapshot_json: &str,
        event_json: &str,
    ) -> Result<String, String> {
        let state_or_snap_val = match Value::from_json(state_or_snapshot_json) {
            Ok(v) => v,
            Err(_) => Value::String(state_or_snapshot_json.to_string()),
        };
        let event_val = match Value::from_json(event_json) {
            Ok(v) => v,
            Err(_) => Value::String(event_json.to_string()),
        };
        let res = self.transition_pure_snapshot_value(&state_or_snap_val, &event_val)?;
        Ok(res.to_json_string())
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

    pub fn get_final_leaf_output(&self) -> Option<Value> {
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
                            Self::emit_action_inspection(
                                &self.inspector_fn,
                                &mut self.pending_init_actions,
                                act,
                            );
                            let extra_inspected = Self::execute_action_with_effects(
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
                            Self::emit_extra_inspected_actions(
                                &self.inspector_fn,
                                &mut self.pending_init_actions,
                                extra_inspected,
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
                    Self::emit_action_inspection(
                        &self.inspector_fn,
                        &mut self.pending_init_actions,
                        act,
                    );
                    let extra_inspected = Self::execute_action_with_effects(
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
                    Self::emit_extra_inspected_actions(
                        &self.inspector_fn,
                        &mut self.pending_init_actions,
                        extra_inspected,
                    );
                }
            }
            self.stop_invokes_for_path("");
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
        self.stop_invokes_for_path("");
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
            for (_, t) in candidates {
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
    ) -> Vec<(&'a str, &'a TransitionDefinition)> {
        if let Some((k, exact)) = on_map.get_key_value(event_type) {
            return exact.iter().map(|t| (k.as_str(), t)).collect();
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
                candidates.extend(transitions.iter().map(|t| (key.as_str(), t)));
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
            self.last_exited_paths.clear();
            self.last_entered_paths.clear();
            if self.inspector_fn.is_some() {
                self.pending_init_actions = Some(Vec::new());
            }
            let mut init_map = IndexMap::new();
            init_map.insert("type".to_string(), Value::String("xstate.init".to_string()));
            if let Some(ref inp) = self.input {
                init_map.insert("input".to_string(), inp.clone());
            }
            let init_event = Value::Object(init_map);
            self.last_received_event = init_event.clone();
            let _ = self.transition_internal("", &init_event, guards, actions, 0, None, None);
            let _ = self.drain_internal_queue(guards, actions);
            self.refresh_status();
            self.handle_machine_completion(&init_event, actions, guards, None);
            let pending_actions = self.pending_init_actions.take().unwrap_or_default();
            if let Some(ref insp) = self.inspector_fn {
                insp(&InspectEvent::Event {
                    event: init_event.clone(),
                });
                for act_evt in pending_actions {
                    insp(&act_evt);
                }
                insp(&InspectEvent::Snapshot {
                    event: init_event.clone(),
                    state: self.current_state.clone(),
                    context: self.context.clone(),
                    status: self.status(),
                });
            }
            self.is_processing = false;
            let _ = self.drain_external_queue(guards, actions);
            return;
        }

        self.is_initialized = true;
        self.is_processing = true;
        self.last_exited_paths.clear();
        self.last_entered_paths.clear();
        if self.inspector_fn.is_some() {
            self.pending_init_actions = Some(Vec::new());
        }

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
        self.last_entered_paths = seen_paths.clone();

        for action in &init_actions_to_run {
            Self::emit_action_inspection(
                &self.inspector_fn,
                &mut self.pending_init_actions,
                action,
            );
            let extra_inspected = Self::execute_action_with_effects(
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
            Self::emit_extra_inspected_actions(
                &self.inspector_fn,
                &mut self.pending_init_actions,
                extra_inspected,
            );
        }

        if self.last_error.is_some() {
            self.status = MachineStatus::Stopped;
            self.is_processing = false;
            return;
        }

        if self.invoke_start_fn.is_some() || self.invoke_stop_fn.is_some() {
            self.start_invokes_for_path("", &init_event);
            for p in &seen_paths {
                self.start_invokes_for_path(p, &init_event);
            }
        }

        self.check_state_completions();
        if self.is_root_done() {
            self.refresh_status();
            self.handle_machine_completion(&init_event, actions, guards, None);
            let pending_actions = self.pending_init_actions.take().unwrap_or_default();
            if let Some(ref insp) = self.inspector_fn {
                insp(&InspectEvent::Event {
                    event: init_event.clone(),
                });
                for act_evt in pending_actions {
                    insp(&act_evt);
                }
                insp(&InspectEvent::Snapshot {
                    event: init_event.clone(),
                    state: self.current_state.clone(),
                    context: self.context.clone(),
                    status: self.status(),
                });
            }
            self.is_processing = false;
            return;
        }

        // 1. Settle initial eventless (`always`) transitions first
        let _ = self.transition_internal("", &init_event, guards, actions, 0, None, None);

        // 2. Drain internal events raised during initial entry or initial `always` transitions
        let _ = self.drain_internal_queue(guards, actions);

        self.refresh_status();
        self.handle_machine_completion(&init_event, actions, guards, None);
        let pending_actions = self.pending_init_actions.take().unwrap_or_default();
        if let Some(ref insp) = self.inspector_fn {
            insp(&InspectEvent::Event {
                event: init_event.clone(),
            });
            for act_evt in pending_actions {
                insp(&act_evt);
            }
            insp(&InspectEvent::Snapshot {
                event: init_event.clone(),
                state: self.current_state.clone(),
                context: self.context.clone(),
                status: self.status(),
            });
        }

        // 3. Complete initialization macrostep and drain any reentrant external events
        self.is_processing = false;
        let _ = self.drain_external_queue(guards, actions);
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
        let mut clean_init = initial.strip_prefix('#').unwrap_or(initial);
        while let Some(stripped) = clean_init.strip_prefix("(machine).") {
            clean_init = stripped;
        }
        if !definition.id.is_empty() && definition.id != "(machine)" {
            let m_prefix = format!("{}.", definition.id);
            while let Some(stripped) = clean_init.strip_prefix(&m_prefix) {
                clean_init = stripped;
            }
            while let Some(stripped) = clean_init.strip_prefix("(machine).") {
                clean_init = stripped;
            }
        }
        let raw_clean = initial.strip_prefix('#').unwrap_or(initial);
        let named_key = format!("{}.{}", definition.id, clean_init);
        let resolved = if clean_init.is_empty() || initial == "#" || clean_init == definition.id {
            ""
        } else if let Some(path) = definition.id_map.get(raw_clean) {
            path.as_str()
        } else if let Some(path) = definition.id_map.get(clean_init) {
            path.as_str()
        } else if let Some(path) = definition.id_map.get(&named_key) {
            path.as_str()
        } else {
            clean_init
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
        let raw_clean = target.strip_prefix('#').unwrap_or(target);
        if raw_clean == self.definition.id || raw_clean.is_empty() {
            return Some(String::new());
        }

        let mut clean_target = raw_clean;
        while let Some(stripped) = clean_target.strip_prefix("(machine).") {
            clean_target = stripped;
        }
        let machine_id = if self.definition.id.is_empty() {
            "(machine)"
        } else {
            &self.definition.id
        };
        if machine_id != "(machine)" {
            let prefix = format!("{}.", machine_id);
            while let Some(stripped) = clean_target.strip_prefix(&prefix) {
                clean_target = stripped;
            }
            while let Some(stripped) = clean_target.strip_prefix("(machine).") {
                clean_target = stripped;
            }
        }

        // 1. Check ID map with multiple candidates
        if !raw_clean.starts_with("__") {
            if let Some(path) = self.definition.id_map.get(raw_clean) {
                return Some(path.clone());
            }
        }
        if !clean_target.is_empty() && !clean_target.starts_with("__") {
            if let Some(path) = self.definition.id_map.get(clean_target) {
                return Some(path.clone());
            }
            let named_key = format!("{}.{}", machine_id, clean_target);
            if let Some(path) = self.definition.id_map.get(&named_key) {
                return Some(path.clone());
            }
            let anon_key = format!("(machine).{}", clean_target);
            if let Some(path) = self.definition.id_map.get(&anon_key) {
                return Some(path.clone());
            }
        }

        // 2. Resolve relative to source (ancestors)
        let mut parts: Vec<&str> = source_path.split('.').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            parts.push("");
        }
        while !parts.is_empty() {
            let prefix = parts.join(".");
            let candidate = if clean_target.starts_with('.') {
                if prefix.is_empty() {
                    clean_target.strip_prefix('.').unwrap_or(clean_target).to_string()
                } else {
                    format!("{}{}", prefix, clean_target)
                }
            } else if prefix.is_empty() {
                clean_target.to_string()
            } else {
                format!("{}.{}", prefix, clean_target)
            };
            if Self::get_state_def_recursive(&self.definition.states, &candidate).is_some() {
                return Some(candidate);
            }
            parts.pop();
        }

        // 3. Try as absolute path from root
        let clean_root = clean_target.strip_prefix('.').unwrap_or(clean_target);
        if Self::get_state_def_recursive(&self.definition.states, clean_root).is_some() {
            return Some(clean_root.to_string());
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
                                let clean_init = init.strip_prefix('#').unwrap_or(init);
                                let lookup_key = if curr_def.states.contains_key(clean_init) {
                                    clean_init
                                } else if curr_def.states.contains_key(init) {
                                    init.as_str()
                                } else if let Some(mapped_path) = definition.id_map.get(clean_init).or_else(|| definition.id_map.get(init)) {
                                    if let Some(pos) = mapped_path.rfind('.') {
                                        &mapped_path[pos + 1..]
                                    } else {
                                        mapped_path.as_str()
                                    }
                                } else if let Some(pos) = clean_init.rfind('.') {
                                    let suffix = &clean_init[pos + 1..];
                                    if curr_def.states.contains_key(suffix) {
                                        suffix
                                    } else {
                                        clean_init
                                    }
                                } else {
                                    clean_init
                                };

                                if let Some(next_def) = curr_def.states.get(lookup_key) {
                                    if matches!(next_def.state_type, StateType::Compound) {
                                        compound_names.push(lookup_key.to_string());
                                        curr_def = next_def;
                                    } else if matches!(next_def.state_type, StateType::Atomic | StateType::Final) {
                                        let mut res = StateValue::Atomic(lookup_key.to_string());
                                        for name in compound_names.into_iter().rev() {
                                            res = StateValue::Compound(name, Box::new(res));
                                        }
                                        completed = Some(Some(res));
                                        resolved_directly = true;
                                        break;
                                    } else {
                                        let full_child_path = if compound_names.len() == 1 {
                                            format!("{}.{}", current_path, lookup_key)
                                        } else {
                                            format!("{}.{}.{}", current_path, compound_names[1..].join("."), lookup_key)
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
            let d = mp.get(p)?;
            current_def = Some(d);
            mp = &d.states;
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
    ) -> Vec<InspectEvent> {
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
        let mut delegated_steps = 0usize;
        while !effects.delegated_actions.is_empty() && delegated_steps < 64 {
            delegated_steps += 1;
            let pending = core::mem::take(&mut effects.delegated_actions);
            for gen in pending {
                if let Some(f) = actions_with_effects.get(&gen.action_type) {
                    f(ctx, event, &gen.params, actions_map, guards_map, delays, current_state, &mut effects);
                } else if let Some(f) = actions_map.get(&gen.action_type) {
                    f(ctx, event, &gen.params);
                }
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
        effects.inspected_actions
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
        self.last_exited_paths.clear();
        self.last_entered_paths.clear();
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

                        if let Some(ref insp) = self.inspector_fn {
                            insp(&InspectEvent::Event {
                                event: after_event.clone(),
                            });
                        }
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
                        self.refresh_status();
                        self.handle_machine_completion(&after_event, actions, guards, None);
                        if let Some(ref insp) = self.inspector_fn {
                            insp(&InspectEvent::Snapshot {
                                event: after_event,
                                state: self.current_state.clone(),
                                context: self.context.clone(),
                                status: self.status(),
                            });
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
                        if let Some(ref insp) = self.inspector_fn {
                            insp(&InspectEvent::Event {
                                event: d_evt.clone(),
                            });
                        }
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
                        self.refresh_status();
                        self.handle_machine_completion(&d_evt, actions, guards, None);
                        if let Some(ref insp) = self.inspector_fn {
                            insp(&InspectEvent::Snapshot {
                                event: d_evt,
                                state: self.current_state.clone(),
                                context: self.context.clone(),
                                status: self.status(),
                            });
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

                    if let Some(ref insp) = self.inspector_fn {
                        insp(&InspectEvent::Event {
                            event: after_event.clone(),
                        });
                    }
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
                    self.refresh_status();
                    self.handle_machine_completion(&after_event, actions, guards, None);
                    if let Some(ref insp) = self.inspector_fn {
                        insp(&InspectEvent::Snapshot {
                            event: after_event,
                            state: self.current_state.clone(),
                            context: self.context.clone(),
                            status: self.status(),
                        });
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
                    if let Some(ref insp) = self.inspector_fn {
                        insp(&InspectEvent::Event {
                            event: d_evt.clone(),
                        });
                    }
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
                    self.refresh_status();
                    self.handle_machine_completion(&d_evt, actions, guards, None);
                    if let Some(ref insp) = self.inspector_fn {
                        insp(&InspectEvent::Snapshot {
                            event: d_evt,
                            state: self.current_state.clone(),
                            context: self.context.clone(),
                            status: self.status(),
                        });
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
            if let Some(ref insp) = self.inspector_fn {
                insp(&InspectEvent::Event {
                    event: e_payload.clone(),
                });
            }
            let mut ret = self.transition_internal(&e_type, &e_payload, guards, actions, 0, None, None);
            if let Some(drained) = self.drain_internal_queue(guards, actions) {
                ret = Some(drained);
            }
            self.refresh_status();
            self.handle_machine_completion(&e_payload, actions, guards, None);
            if let Some(ref insp) = self.inspector_fn {
                insp(&InspectEvent::Snapshot {
                    event: e_payload,
                    state: self.current_state.clone(),
                    context: self.context.clone(),
                    status: self.status(),
                });
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
        self.last_exited_paths.clear();
        self.last_entered_paths.clear();
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
        inspector_fn: &Option<InspectFunction>,
        pending_init_actions: &mut Option<Vec<InspectEvent>>,
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
            Self::emit_action_inspection(inspector_fn, pending_init_actions, act);
            let extra_inspected = Self::execute_action_with_effects(
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
            Self::emit_extra_inspected_actions(inspector_fn, pending_init_actions, extra_inspected);
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

        if !self.is_processing {
            self.last_exited_paths.clear();
            self.last_entered_paths.clear();
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
            let candidates: Vec<(&str, &TransitionDefinition)> = if let Some((f_path, f_t)) = current_force {
                if *f_path == current_path {
                    f_t.iter().map(|t| (current_event_type, t)).collect()
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
                                        cands.extend(t_defs.iter().map(|t| (current_event_type, t)));
                                    }
                                } else if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &current_path) {
                                    if let Some(t_defs) = def.after.get(&delay_num) {
                                        cands.extend(t_defs.iter().map(|t| (current_event_type, t)));
                                    }
                                }
                            }
                            if cands.is_empty() {
                                if current_path.is_empty() {
                                    if let Some(t_defs) = self.definition.after.get(&Delay::Named(delay_str.to_string())) {
                                        cands.extend(t_defs.iter().map(|t| (current_event_type, t)));
                                    }
                                } else if let Some(def) = Self::get_state_def_recursive(&self.definition.states, &current_path) {
                                    if let Some(t_defs) = def.named_after.get(delay_str) {
                                        cands.extend(t_defs.iter().map(|t| (current_event_type, t)));
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
                  for (matched_event_type, t) in candidates {
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
                           event_type: matched_event_type.to_string(),
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
            if !current_event_type.is_empty() && action_collector.is_none() {
                if let Some(ref insp) = self.inspector_fn {
                    insp(&InspectEvent::Microstep {
                        event: event_payload.clone(),
                        transitions: Vec::new(),
                        state: self.current_state.clone(),
                        context: self.context.clone(),
                        status: self.status(),
                    });
                }
            }
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
        let reenters_root = selected_transitions.iter().any(|st| st.reenters_root);
        let has_targets = selected_transitions.iter().any(|st| {
            st.t.reenter || st.t.target.as_ref().map(|v| !v.is_empty()).unwrap_or(false)
        });
        let prev_context = if !has_targets {
            Some(self.context.clone())
        } else {
            None
        };
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
                    &self.inspector_fn,
                    &mut self.pending_init_actions,
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
                   if let Some(col) = action_collector.as_deref_mut() {
                       for (inv_idx, inv) in def.invoke.iter().enumerate() {
                           let invoke_id = inv.id.clone().unwrap_or_else(|| format!("{}.{}", inv_idx, path));
                           let mut p_map = IndexMap::new();
                           p_map.insert("id".to_string(), Value::String(invoke_id));
                           col.push(Action::Generic(GenericAction {
                               action_type: "xstate.stopChild".to_string(),
                               params: Some(Value::Object(p_map)),
                           }));
                       }
                   } else {
                       let mut stopped = Vec::new();
                       self.active_invokes.retain(|(sp, id)| {
                           if sp == path {
                               stopped.push(id.clone());
                               false
                           } else {
                               true
                           }
                       });
                       if let Some(ref on_stop) = self.invoke_stop_fn {
                           for id in stopped {
                               on_stop(&id);
                           }
                       }
                   }
              }
              exited_paths.push(path.clone());
              self.completed_states.retain(|s| s != path && !s.starts_with(&format!("{}.", path)));
        }

        let entered_paths = Self::calculate_entered_paths(&active_leaves, &next_leaves, &selected_transitions);
        for p in &final_exited_paths {
            if !self.last_exited_paths.contains(p) {
                self.last_exited_paths.push(p.clone());
            }
            self.last_entered_paths.retain(|entry| entry != p && !entry.starts_with(&format!("{}.", p)));
        }
        for p in &entered_paths {
            if !self.last_entered_paths.contains(p) {
                self.last_entered_paths.push(p.clone());
            }
        }
        let inspected_transitions: Vec<InspectedTransition> =
            if action_collector.is_none() && self.inspector_fn.is_some() {
                selected_transitions
                    .iter()
                    .map(|st| {
                        let source_id = self.get_state_node_id(&st.source_path);
                        let target_ids = st.t.target.as_ref().and_then(|targets| {
                            if targets.is_empty() {
                                None
                            } else {
                                let current_search_path = if st.source_path.is_empty() {
                                    "".to_string()
                                } else {
                                    st.source_path.clone()
                                };
                                Some(
                                    targets
                                        .iter()
                                        .map(|raw| {
                                            let target_full = self
                                                .resolve_target(&current_search_path, raw)
                                                .unwrap_or_else(|| raw.clone());
                                            self.get_state_node_id(&target_full)
                                        })
                                        .collect(),
                                )
                            }
                        });
                        InspectedTransition {
                            source_id,
                            target_ids,
                            event_type: st.event_type.clone(),
                            actions: st.t.actions.clone(),
                            guard: st.t.guard.clone(),
                            reenter: st.t.reenter,
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };
        drop(selected_transitions);

        if reenters_root {
            for action in &self.definition.exit {
                exec_action!(action);
            }
            if let Some(col) = action_collector.as_deref_mut() {
                let root_id = if self.definition.id.is_empty() { "(machine)" } else { &self.definition.id };
                for (inv_idx, inv) in self.definition.invoke.iter().enumerate() {
                    let invoke_id = inv.id.clone().unwrap_or_else(|| format!("{}.{}", inv_idx, root_id));
                    let mut p_map = IndexMap::new();
                    p_map.insert("id".to_string(), Value::String(invoke_id));
                    col.push(Action::Generic(GenericAction {
                        action_type: "xstate.stopChild".to_string(),
                        params: Some(Value::Object(p_map)),
                    }));
                }
            } else {
                self.stop_invokes_for_path("");
            }
            self.completed_states.clear();
            self.is_machine_completed = false;
            self.status = MachineStatus::Active;
        }
        
        self.active_timers.retain(|(p, _, _)| !exited_paths.contains(p));
        let changed = has_targets || (finalized_state != self.current_state);
        self.current_state = finalized_state;
        self.last_received_event = event_payload.clone();
        
        for action in &all_transition_actions { exec_action!(action); }
        for action in &all_collected_actions { exec_action!(action); }

        if reenters_root {
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
            if let Some(col) = action_collector.as_deref_mut() {
                for (inv_idx, inv) in self.definition.invoke.iter().enumerate() {
                    let root_id = if self.definition.id.is_empty() { "(machine)" } else { &self.definition.id };
                    let invoke_id = inv.id.clone().unwrap_or_else(|| format!("{}.{}", inv_idx, root_id));
                    let mut p_map = IndexMap::new();
                    p_map.insert("id".to_string(), Value::String(invoke_id));
                    p_map.insert("src".to_string(), inv.src.clone());
                    if let Some(ref inp) = inv.input {
                        p_map.insert("input".to_string(), inp.clone());
                    }
                    col.push(Action::Generic(GenericAction {
                        action_type: "xstate.spawnChild".to_string(),
                        params: Some(Value::Object(p_map)),
                    }));
                }
            } else {
                self.start_invokes_for_path("", event_payload);
            }
            for action in &self.definition.initial_actions {
                exec_action!(action);
            }
        }

        for path in &entered_paths {
            if let Some(def) = Self::get_state_def_recursive(&self.definition.states, path) {
                for action in &def.entry { exec_action!(action); }
                if let Some(col) = action_collector.as_deref_mut() {
                    for (inv_idx, inv) in def.invoke.iter().enumerate() {
                        let invoke_id = inv.id.clone().unwrap_or_else(|| format!("{}.{}", inv_idx, path));
                        let mut p_map = IndexMap::new();
                        p_map.insert("id".to_string(), Value::String(invoke_id));
                        p_map.insert("src".to_string(), inv.src.clone());
                        if let Some(ref inp) = inv.input {
                            p_map.insert("input".to_string(), inp.clone());
                        }
                        col.push(Action::Generic(GenericAction {
                            action_type: "xstate.spawnChild".to_string(),
                            params: Some(Value::Object(p_map)),
                        }));
                    }
                } else if self.invoke_start_fn.is_some() || self.invoke_stop_fn.is_some() {
                    for (inv_idx, inv) in def.invoke.iter().enumerate() {
                        let invoke_id = inv.id.clone().unwrap_or_else(|| format!("{}.{}", inv_idx, path));
                        self.active_invokes.push((path.clone(), invoke_id.clone()));
                        if let Some(ref on_start) = self.invoke_start_fn {
                            on_start(&invoke_id, &inv.src, &inv.input, &self.context, event_payload);
                        }
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
        self.refresh_status();
        self.handle_machine_completion(event_payload, actions, guards, action_collector);
        if !inspected_transitions.is_empty() {
            if let Some(ref insp) = self.inspector_fn {
                insp(&InspectEvent::Microstep {
                    event: event_payload.clone(),
                    transitions: inspected_transitions,
                    state: self.current_state.clone(),
                    context: self.context.clone(),
                    status: self.status(),
                });
            }
        }
        Some(changed || prev_context.as_ref().is_some_and(|prev| &self.context != prev))
    }
}
