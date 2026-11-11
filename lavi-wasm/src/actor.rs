use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use lavi::{IndexMap, IndexMapExt, RuntimeMachine, StateType, Value};

use crate::actions::{
    configure_logger, extract_unprocessed_fn_actions,
    parse_and_register_delays, parse_implementations, process_config,
};
use crate::state_node::{compute_history_value_obj, try_clone_state_node};
use crate::value_bridge::{js_to_value, unregister_live_actor_ref, value_to_js};

pub(crate) fn check_initial_entry_guards(runner: &ActorInner) {
    if runner.guards.is_empty() {
        return;
    }
    let init_event = Value::Object(lavi::indexmap! { "type".to_string() => Value::String("xstate.init".to_string()) });
    for action in &runner.machine.definition.entry {
        let name = match action {
            lavi::Action::Named(s) => s.as_str(),
            lavi::Action::Generic(g) => g.action_type.as_str(),
            _ => "",
        };
        if name.starts_with("auto_action_enqueue_root_") {
            if let Some(f) = runner.machine.actions_with_effects.get(name) {
                let mut dummy_ctx = runner.machine.context.clone();
                let mut snapshot_actions: IndexMap<String, lavi::ActionFunction> = IndexMap::new();
                snapshot_actions.insert(
                    "__lavi_snapshot_mode__".to_string(),
                    Rc::new(|_, _, _| {}),
                );
                let mut dummy_effects = lavi::ActionEffects::new();
                f(
                    &mut dummy_ctx,
                    &init_event,
                    &None,
                    &snapshot_actions,
                    &runner.guards,
                    &runner.machine.delays,
                    &runner.machine.current_state,
                    &mut dummy_effects,
                );
            }
        }
    }
}

#[wasm_bindgen]
#[derive(Clone, Default)]
pub struct SimulatedClock {
    clock: lavi::SimulatedClock,
    actors: Rc<RefCell<Vec<Actor>>>,
}

#[wasm_bindgen]
impl SimulatedClock {
    #[wasm_bindgen(constructor)]
    pub fn new() -> SimulatedClock {
        SimulatedClock {
            clock: lavi::SimulatedClock::new(),
            actors: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[wasm_bindgen(js_name = __registerActor)]
    pub fn register_actor(&mut self, actor: &Actor) {
        {
            let mut inner = actor.inner.borrow_mut();
            inner.is_simulated_clock = true;
            let tick_override = inner.tick_override.clone();
            let core_clock = self.clock.clone();
            inner.machine.register_clock(Rc::new(move || {
                tick_override.get().unwrap_or_else(|| core_clock.now())
            }));
        }
        self.actors.borrow_mut().push(actor.clone());
    }

    #[wasm_bindgen]
    pub fn now(&self) -> f64 {
        self.clock.now() as f64
    }

    #[wasm_bindgen]
    pub fn set(&mut self, timestamp: f64) {
        let target_ms = if timestamp.is_finite() && timestamp >= 0.0 {
            timestamp as u64
        } else {
            0
        };

        let mut last_deadline: Option<u64> = None;
        let mut same_deadline_steps = 0usize;
        let mut total_steps = 0usize;
        let mut transitioned_indices = Vec::new();

        loop {
            let actors = self.actors.borrow().clone();

            let next_deadline_opt = actors
                .iter()
                .filter_map(|a| {
                    let inner = a.inner.borrow();
                    if !inner.is_stopped && !inner.machine.is_done() && inner.machine.status != lavi::MachineStatus::Done {
                        inner.machine.next_timer_deadline()
                    } else {
                        None
                    }
                })
                .min();

            let Some(next_deadline) = next_deadline_opt else {
                break;
            };
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

            self.clock.set_now_raw(next_deadline);

            for (idx, a) in actors.iter().enumerate() {
                let step_state = {
                    let mut inner = a.inner.borrow_mut();
                    if !inner.is_stopped && !inner.machine.is_done() && inner.machine.status != lavi::MachineStatus::Done {
                        inner.is_in_tick = true;
                        Some((inner.machine.clone(), inner.guards.clone(), inner.actions.clone()))
                    } else {
                        None
                    }
                };
                if let Some((mut machine, guards, actions)) = step_state {
                    let mut did_transition = machine.tick(&guards, &actions).is_some();
                    if a.drain_reentrant(&mut machine, &guards, &actions) {
                        did_transition = true;
                    }
                    {
                        let mut inner = a.inner.borrow_mut();
                        if machine.inspector_fn.is_none() {
                            machine.inspector_fn = inner.machine.inspector_fn.clone();
                        }
                        if !machine.is_done() && machine.status != lavi::MachineStatus::Done {
                            inner.cached_output = None;
                            inner.is_completed_emitted = false;
                        }
                        inner.machine = machine;
                        inner.is_in_tick = false;
                    }
                    if did_transition && !transitioned_indices.contains(&idx) {
                        transitioned_indices.push(idx);
                    }
                }
            }
        }

        self.clock.set_now_raw(target_ms);

        let actors_snapshot = self.actors.borrow().clone();
        for idx in transitioned_indices {
            if let Some(a) = actors_snapshot.get(idx) {
                a.emit_snapshot();
            }
        }
    }

    #[wasm_bindgen]
    pub fn increment(&mut self, ms: f64) {
        let dt_ms = if ms.is_finite() && ms >= 0.0 {
            ms as u64
        } else {
            0
        };
        let target = self.clock.now().saturating_add(dt_ms);
        self.set(target as f64);
    }
}

static NEXT_SESSION_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

pub(crate) fn next_session_id() -> String {
    let n = NEXT_SESSION_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    format!("x:{}", n)
}

thread_local! {
    pub(crate) static IN_SYSTEM_RELAY: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    pub(crate) static RELAYED_QUEUE: RefCell<IndexMap<String, std::collections::VecDeque<bool>>> = RefCell::new(IndexMap::new());
}

pub(crate) fn create_system_js(inspector: Option<JsValue>) -> JsValue {
    let sys_obj = js_sys::Object::new();
    let inspectors_list = Rc::new(RefCell::new(Vec::<JsValue>::new()));
    if let Some(insp) = inspector {
        inspectors_list.borrow_mut().push(insp);
    }
    let actors_map = Rc::new(RefCell::new(IndexMap::<String, JsValue>::new()));

    let insp_for_reg = inspectors_list.clone();
    let actors_for_reg = actors_map.clone();
    let reg_fn = Closure::wrap(Box::new(move |arg0: JsValue, arg1: JsValue| -> JsValue {
        let (actor_ref, explicit_sid) = if arg1.is_object() && !arg1.is_null() {
            (arg1, arg0.as_string())
        } else {
            (arg0, None)
        };

        let sid = if let Some(s) = explicit_sid {
            s
        } else if actor_ref.is_object() && !actor_ref.is_null() {
            js_sys::Reflect::get(&actor_ref, &JsValue::from_str("sessionId"))
                .ok().and_then(|v| v.as_string()).unwrap_or_default()
        } else {
            String::new()
        };

        let custom_id = if actor_ref.is_object() && !actor_ref.is_null() {
            js_sys::Reflect::get(&actor_ref, &JsValue::from_str("id"))
                .ok().and_then(|v| v.as_string()).unwrap_or_default()
        } else {
            String::new()
        };

        let system_id = if actor_ref.is_object() && !actor_ref.is_null() {
            js_sys::Reflect::get(&actor_ref, &JsValue::from_str("systemId"))
                .ok().and_then(|v| v.as_string()).unwrap_or_default()
        } else {
            String::new()
        };

        {
            let mut map = actors_for_reg.borrow_mut();
            if !sid.is_empty() {
                map.insert(sid.clone(), actor_ref.clone());
            }
            if !custom_id.is_empty() {
                map.insert(custom_id.clone(), actor_ref.clone());
            }
            if !system_id.is_empty() {
                map.insert(system_id.clone(), actor_ref.clone());
            }
        }

        let ev = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&ev, &JsValue::from_str("type"), &JsValue::from_str("@xstate.actor"));
        let _ = js_sys::Reflect::set(&ev, &JsValue::from_str("actorRef"), &actor_ref);
        let root_id = if !sid.is_empty() { sid.clone() } else { custom_id.clone() };
        let _ = js_sys::Reflect::set(&ev, &JsValue::from_str("rootId"), &JsValue::from_str(&root_id));
        for insp in insp_for_reg.borrow().iter() {
            emit_to_system_inspector(insp, &ev);
        }
        JsValue::from_str(&sid)
    }) as Box<dyn Fn(JsValue, JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&sys_obj, &JsValue::from_str("_register"), reg_fn.as_ref());
    reg_fn.forget();

    let actors_for_unreg = actors_map.clone();
    let unreg_fn = Closure::wrap(Box::new(move |actor_ref: JsValue| {
        if actor_ref.is_object() && !actor_ref.is_null() {
            let mut map = actors_for_unreg.borrow_mut();
            if let Ok(sid_val) = js_sys::Reflect::get(&actor_ref, &JsValue::from_str("sessionId")) {
                if let Some(sid) = sid_val.as_string() {
                    map.shift_remove(&sid);
                }
            }
            if let Ok(id_val) = js_sys::Reflect::get(&actor_ref, &JsValue::from_str("id")) {
                if let Some(id) = id_val.as_string() {
                    map.shift_remove(&id);
                }
            }
            if let Ok(sys_id_val) = js_sys::Reflect::get(&actor_ref, &JsValue::from_str("systemId")) {
                if let Some(sys_id) = sys_id_val.as_string() {
                    map.shift_remove(&sys_id);
                }
            }
        }
    }) as Box<dyn Fn(JsValue)>);
    let _ = js_sys::Reflect::set(&sys_obj, &JsValue::from_str("_unregister"), unreg_fn.as_ref());
    unreg_fn.forget();

    let actors_for_set = actors_map.clone();
    let set_fn = Closure::wrap(Box::new(move |system_id: JsValue, actor_ref: JsValue| {
        if let Some(s) = system_id.as_string() {
            actors_for_set.borrow_mut().insert(s, actor_ref);
        }
    }) as Box<dyn Fn(JsValue, JsValue)>);
    let _ = js_sys::Reflect::set(&sys_obj, &JsValue::from_str("_set"), set_fn.as_ref());
    set_fn.forget();

    let insp_for_relay = inspectors_list.clone();
    let relay_fn = Closure::wrap(Box::new(move |source_ref: JsValue, target_ref: JsValue, event: JsValue| {
        let ev = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&ev, &JsValue::from_str("type"), &JsValue::from_str("@xstate.event"));
        let _ = js_sys::Reflect::set(&ev, &JsValue::from_str("actorRef"), &target_ref);
        let _ = js_sys::Reflect::set(&ev, &JsValue::from_str("sourceRef"), &source_ref);
        let _ = js_sys::Reflect::set(&ev, &JsValue::from_str("event"), &event);
        for insp in insp_for_relay.borrow().iter() {
            emit_to_system_inspector(insp, &ev);
        }
        if target_ref.is_object() && !target_ref.is_null() {
            if let Ok(send) = js_sys::Reflect::get(&target_ref, &JsValue::from_str("send")) {
                if let Ok(f) = send.dyn_into::<js_sys::Function>() {
                    IN_SYSTEM_RELAY.with(|c| c.set(true));
                    let _ = f.call1(&target_ref, &event);
                    IN_SYSTEM_RELAY.with(|c| c.set(false));
                }
            }
        }
    }) as Box<dyn Fn(JsValue, JsValue, JsValue)>);
    let _ = js_sys::Reflect::set(&sys_obj, &JsValue::from_str("_relay"), relay_fn.as_ref());
    relay_fn.forget();

    let insp_for_snap = inspectors_list.clone();
    let snap_fn = Closure::wrap(Box::new(move |actor_ref: JsValue, event: JsValue, snapshot: JsValue, status: JsValue| {
        let ev = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&ev, &JsValue::from_str("type"), &JsValue::from_str("@xstate.snapshot"));
        let _ = js_sys::Reflect::set(&ev, &JsValue::from_str("actorRef"), &actor_ref);
        let _ = js_sys::Reflect::set(&ev, &JsValue::from_str("event"), &event);
        let _ = js_sys::Reflect::set(&ev, &JsValue::from_str("snapshot"), &snapshot);
        let _ = js_sys::Reflect::set(&ev, &JsValue::from_str("status"), &status);
        for insp in insp_for_snap.borrow().iter() {
            emit_to_system_inspector(insp, &ev);
        }
    }) as Box<dyn Fn(JsValue, JsValue, JsValue, JsValue)>);
    let _ = js_sys::Reflect::set(&sys_obj, &JsValue::from_str("_snapshot"), snap_fn.as_ref());
    snap_fn.forget();

    let insp_for_emit = inspectors_list.clone();
    let emit_fn = Closure::wrap(Box::new(move |event_obj: JsValue| {
        for insp in insp_for_emit.borrow().iter() {
            emit_to_system_inspector(insp, &event_obj);
        }
    }) as Box<dyn Fn(JsValue)>);
    let _ = js_sys::Reflect::set(&sys_obj, &JsValue::from_str("_emit"), emit_fn.as_ref());
    emit_fn.forget();

    let actors_for_get = actors_map.clone();
    let get_fn = Closure::wrap(Box::new(move |id: JsValue| -> JsValue {
        if let Some(s) = id.as_string() {
            if let Some(act) = actors_for_get.borrow().get(&s) {
                return act.clone();
            }
        }
        JsValue::UNDEFINED
    }) as Box<dyn Fn(JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&sys_obj, &JsValue::from_str("get"), get_fn.as_ref());
    get_fn.forget();

    let insp_for_inspect = inspectors_list.clone();
    let inspect_fn = Closure::wrap(Box::new(move |observer_or_fn: JsValue| -> JsValue {
        insp_for_inspect.borrow_mut().push(observer_or_fn.clone());
        let insp_list_unsub = insp_for_inspect.clone();
        let target_insp = observer_or_fn.clone();
        let unsub = Closure::wrap(Box::new(move || {
            insp_list_unsub.borrow_mut().retain(|x| x != &target_insp);
        }) as Box<dyn FnMut()>);
        let unsub_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&unsub_obj, &JsValue::from_str("unsubscribe"), &unsub.into_js_value());
        unsub_obj.into()
    }) as Box<dyn Fn(JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&sys_obj, &JsValue::from_str("inspect"), inspect_fn.as_ref());
    inspect_fn.forget();

    sys_obj.into()
}

fn emit_to_system_inspector(insp: &JsValue, ev: &JsValue) {
    if insp.is_function() {
        let f: &js_sys::Function = insp.unchecked_ref();
        let _ = f.call1(&JsValue::NULL, ev);
    } else if insp.is_object() && !insp.is_null() {
        if let Ok(next) = js_sys::Reflect::get(insp, &JsValue::from_str("next")) {
            if let Ok(f) = next.dyn_into::<js_sys::Function>() {
                let _ = f.call1(insp, ev);
            }
        }
    }
}

#[wasm_bindgen(js_name = isMachineSnapshot)]
pub fn is_machine_snapshot(val: JsValue) -> bool {
    if !val.is_object() || val.is_null() {
        return false;
    }
    js_sys::Reflect::has(&val, &JsValue::from_str("value")).unwrap_or(false)
        && js_sys::Reflect::has(&val, &JsValue::from_str("status")).unwrap_or(false)
        && js_sys::Reflect::has(&val, &JsValue::from_str("context")).unwrap_or(false)
}

#[derive(Clone)]
pub(crate) enum Subscriber {
    Fn(js_sys::Function),
    Observer {
        next: Option<js_sys::Function>,
        complete: Option<js_sys::Function>,
        #[allow(dead_code)]
        error: Option<js_sys::Function>,
    },
}

pub(crate) struct ActiveTimeout {
    pub handle: JsValue,
}

pub(crate) struct ActorInner {
    pub machine: RuntimeMachine,
    pub inspector: Option<js_sys::Function>,
    pub inspectors: Vec<(u64, Subscriber)>,
    pub subscribers: Vec<(u64, Subscriber)>,
    pub guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    pub guards_holder: Rc<RefCell<IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>>>,
    pub actions: IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    pub action_descriptors: IndexMap<String, JsValue>,
    pub config: JsValue,
    pub is_stopped: bool,
    pub is_initialized: bool,
    pub is_in_tick: bool,
    pub tick_override: Rc<core::cell::Cell<Option<u64>>>,
    pub host_clock: Option<JsValue>,
    pub is_simulated_clock: bool,
    pub active_timeout: Option<ActiveTimeout>,
    pub cached_output: Option<JsValue>,
    pub is_completed_emitted: bool,
    pub is_done_event_sent: bool,
    pub logger: Option<js_sys::Function>,
    pub logger_holder: Rc<RefCell<Option<js_sys::Function>>>,
    pub input_js: Option<JsValue>,
    pub initialization_error: Option<JsValue>,
    pub session_id: String,
    pub custom_id: Option<String>,
    pub children: IndexMap<String, JsValue>,
    pub child_sources: IndexMap<String, Vec<String>>,
    pub parent: Option<JsValue>,
    pub system: Option<JsValue>,
    pub self_js: Option<JsValue>,
    pub implementations: JsValue,
    pub pending_start_actors: Vec<JsValue>,
    pub send_timer_handles: IndexMap<String, JsValue>,
}

pub(crate) struct PendingInvoke {
    pub invoke_id: String,
    pub path: String,
    pub resolved_src: JsValue,
    pub raw_input: JsValue,
}

impl ActorInner {
    pub(crate) fn prepare_invokes_for_path(&self, path: &str) -> Vec<PendingInvoke> {
        let invokes: Vec<(usize, lavi::InvokeDefinition)> = if path.is_empty() {
            self.machine.definition.invoke.iter().enumerate().map(|(i, inv)| (i, inv.clone())).collect()
        } else if let Some(def) = RuntimeMachine::get_state_def_recursive(&self.machine.definition.states, path) {
            def.invoke.iter().enumerate().map(|(i, inv)| (i, inv.clone())).collect()
        } else {
            Vec::new()
        };

        let mut pending = Vec::new();
        for (inv_idx, inv) in invokes {
            let state_id = if path.is_empty() {
                "(machine)".to_string()
            } else {
                format!("(machine).{}", path)
            };
            let invoke_id = inv.id.clone().unwrap_or_else(|| format!("{}.{}", inv_idx, state_id));

            if self.children.contains_key(&invoke_id) {
                continue;
            }

            let raw_desc_opt = self.action_descriptors.get(&format!("__xstate_invoke_desc_{}", invoke_id))
                .or_else(|| self.action_descriptors.get(&format!("__xstate_invoke_desc_{}.{}", inv_idx, path)))
                .or_else(|| if !path.is_empty() { self.action_descriptors.get(&format!("__xstate_invoke_desc_{}", path)) } else { None })
                .or_else(|| if let Some(ref eid) = inv.id { self.action_descriptors.get(&format!("__xstate_invoke_desc_{}", eid)) } else { None });

            let (raw_src, raw_input) = if let Some(raw_desc) = raw_desc_opt {
                let s = js_sys::Reflect::get(raw_desc, &JsValue::from_str("src")).unwrap_or(JsValue::UNDEFINED);
                let i = js_sys::Reflect::get(raw_desc, &JsValue::from_str("input")).unwrap_or(JsValue::UNDEFINED);
                (s, i)
            } else {
                (value_to_js(&inv.src), inv.input.as_ref().map(value_to_js).unwrap_or(JsValue::UNDEFINED))
            };

            let resolved_src = if let Some(src_str) = raw_src.as_string() {
                let mut found = JsValue::UNDEFINED;
                if self.implementations.is_object() && !self.implementations.is_null() {
                    if let Ok(actors) = js_sys::Reflect::get(&self.implementations, &JsValue::from_str("actors")) {
                        if actors.is_object() && !actors.is_null() {
                            if let Ok(act) = js_sys::Reflect::get(&actors, &JsValue::from_str(&src_str)) {
                                if !act.is_undefined() {
                                    found = act;
                                }
                            }
                        }
                    }
                }
                if found.is_undefined() && self.config.is_object() && !self.config.is_null() {
                    if let Ok(actors) = js_sys::Reflect::get(&self.config, &JsValue::from_str("actors")) {
                        if actors.is_object() && !actors.is_null() {
                            if let Ok(act) = js_sys::Reflect::get(&actors, &JsValue::from_str(&src_str)) {
                                if !act.is_undefined() {
                                    found = act;
                                }
                            }
                        }
                    }
                }
                if found.is_undefined() {
                    raw_src
                } else {
                    found
                }
            } else {
                raw_src
            };

            if resolved_src.is_undefined() {
                continue;
            }

            pending.push(PendingInvoke {
                invoke_id,
                path: path.to_string(),
                resolved_src,
                raw_input,
            });
        }
        pending
    }
}

pub(crate) fn execute_pending_invokes(
    inner_rc: &Rc<RefCell<ActorInner>>,
    pending: Vec<PendingInvoke>,
    current_event: &Value,
    current_context: &Value,
    defer_start: bool,
) {
    if pending.is_empty() {
        return;
    }

    let (self_js, system) = {
        let Ok(inner) = inner_rc.try_borrow() else {
            return;
        };
        let Some(s) = inner.self_js.clone() else {
            return;
        };
        (s, inner.system.clone())
    };

    let mut created_actors = Vec::new();
    for p in pending {
        let resolved_input = if let Ok(f) = p.raw_input.clone().dyn_into::<js_sys::Function>() {
            let arg = js_sys::Object::new();
            let ctx_js = value_to_js(current_context);
            let evt_js = Actor::event_to_js(current_event);
            let _ = js_sys::Reflect::set(&arg, &JsValue::from_str("context"), &ctx_js);
            let _ = js_sys::Reflect::set(&arg, &JsValue::from_str("event"), &evt_js);
            let _ = js_sys::Reflect::set(&arg, &JsValue::from_str("self"), &self_js);
            f.call1(&JsValue::NULL, &arg).unwrap_or(JsValue::UNDEFINED)
        } else {
            p.raw_input
        };

        let child_opts = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&child_opts, &JsValue::from_str("id"), &JsValue::from_str(&p.invoke_id));
        let _ = js_sys::Reflect::set(&child_opts, &JsValue::from_str("input"), &resolved_input);
        let _ = js_sys::Reflect::set(&child_opts, &JsValue::from_str("parent"), &self_js);
        if let Some(ref sys) = system {
            let _ = js_sys::Reflect::set(&child_opts, &JsValue::from_str("system"), sys);
        }

        let child_actor = match create_actor(p.resolved_src, Some(child_opts.into())) {
            Ok(act) => act,
            Err(e) => {
                web_sys::console::warn_2(&JsValue::from_str("Failed to invoke child:"), &e);
                continue;
            }
        };

        created_actors.push((p.invoke_id, p.path, child_actor));
    }

    let mut actors_to_start = Vec::new();
    if let Ok(mut inner) = inner_rc.try_borrow_mut() {
        for (invoke_id, path, child_actor) in created_actors {
            if inner.children.contains_key(&invoke_id) {
                continue;
            }
            inner.children.insert(invoke_id.clone(), child_actor.clone());
            inner.child_sources.entry(path).or_default().push(invoke_id);

            if defer_start {
                inner.pending_start_actors.push(child_actor);
            } else {
                actors_to_start.push(child_actor);
            }
        }
    }

    for act in actors_to_start {
        if let Ok(start_fn) = js_sys::Reflect::get(&act, &JsValue::from_str("start")) {
            if let Ok(f) = start_fn.dyn_into::<js_sys::Function>() {
                let _ = f.call0(&act);
            }
        }
    }
}

pub(crate) fn spawn_invokes_for_path_detached(
    inner_rc: &Rc<RefCell<ActorInner>>,
    path: &str,
    current_event: &Value,
    current_context: &Value,
    defer_start: bool,
) {
    let pending = {
        let Ok(inner) = inner_rc.try_borrow() else {
            return;
        };
        inner.prepare_invokes_for_path(path)
    };
    execute_pending_invokes(inner_rc, pending, current_event, current_context, defer_start);
}

pub(crate) fn collect_and_stop_invokes(inner_rc: &Rc<RefCell<ActorInner>>, paths: &[String]) {
    let mut actors_to_stop = Vec::new();
    if let Ok(mut inner) = inner_rc.try_borrow_mut() {
        for path in paths {
            if let Some(child_ids) = inner.child_sources.shift_remove(path) {
                for child_id in child_ids {
                    if let Some(child_actor) = inner.children.shift_remove(&child_id) {
                        actors_to_stop.push(child_actor);
                    }
                }
            }
        }
    }
    for act in actors_to_stop {
        if let Ok(stop_fn) = js_sys::Reflect::get(&act, &JsValue::from_str("stop")) {
            if let Ok(f) = stop_fn.dyn_into::<js_sys::Function>() {
                let _ = f.call0(&act);
            }
        }
    }
}

pub(crate) fn flush_pending_starts_detached(inner_rc: &Rc<RefCell<ActorInner>>) {
    let pending = if let Ok(mut inner) = inner_rc.try_borrow_mut() {
        core::mem::take(&mut inner.pending_start_actors)
    } else {
        Vec::new()
    };
    for act in pending {
        if let Ok(start_fn) = js_sys::Reflect::get(&act, &JsValue::from_str("start")) {
            if let Ok(f) = start_fn.dyn_into::<js_sys::Function>() {
                let _ = f.call0(&act);
            }
        }
    }
}

impl ActorInner {
    pub(crate) fn restore_snapshot_option(&mut self, options: &JsValue) -> Result<(), JsValue> {
        if !options.is_object() || options.is_null() {
            return Ok(());
        }

        let mut snapshot_val = JsValue::UNDEFINED;
        let snapshot_key = JsValue::from_str("snapshot");
        if let Ok(val) = js_sys::Reflect::get(options, &snapshot_key) {
            if !val.is_undefined() {
                snapshot_val = val;
            }
        }
        if snapshot_val.is_undefined() {
            let state_key = JsValue::from_str("state");
            if let Ok(val) = js_sys::Reflect::get(options, &state_key) {
                if !val.is_undefined() {
                    snapshot_val = val;
                }
            }
        }

        if !snapshot_val.is_undefined() && !snapshot_val.is_null() {
            let json_str = if snapshot_val.is_string() {
                snapshot_val.as_string().unwrap()
            } else {
                js_sys::JSON::stringify(&snapshot_val)?
                    .as_string()
                    .unwrap_or_default()
            };

            if !json_str.is_empty() && json_str != "{}" {
                self.machine
                    .restore_from_snapshot_json(&json_str)
                    .map_err(|e| JsValue::from_str(&format!("Failed to restore snapshot: {}", e)))?;

                self.is_initialized = true;
                if self.machine.status == lavi::MachineStatus::Stopped {
                    self.is_stopped = true;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn sync_tracked_entry_exit(&mut self, config: &JsValue) {
        if !config.is_object() || config.is_null() {
            return;
        }
        let mut stack: Vec<(JsValue, String)> = vec![(config.clone(), String::new())];
        while let Some((node_val, path)) = stack.pop() {
            if !node_val.is_object() || node_val.is_null() {
                continue;
            }
            let node_obj = js_sys::Object::from(node_val);
            let path_label = if path.is_empty() { "__root__" } else { &path };
            let mut entry_actions = extract_unprocessed_fn_actions(
                &node_obj,
                "entry",
                path_label,
                &mut self.actions,
            );
            let mut exit_actions = extract_unprocessed_fn_actions(
                &node_obj,
                "exit",
                path_label,
                &mut self.actions,
            );
            if path.is_empty() {
                if !entry_actions.is_empty() || !exit_actions.is_empty() {
                    let root_def = Rc::make_mut(&mut self.machine.definition);
                    entry_actions.retain(|a| !root_def.entry.contains(a));
                    exit_actions.retain(|a| !root_def.exit.contains(a));
                    if !entry_actions.is_empty() {
                        entry_actions.append(&mut root_def.entry);
                        root_def.entry = entry_actions;
                    }
                    if !exit_actions.is_empty() {
                        exit_actions.append(&mut root_def.exit);
                        root_def.exit = exit_actions;
                    }
                }
            } else if !entry_actions.is_empty() || !exit_actions.is_empty() {
                let root_def = Rc::make_mut(&mut self.machine.definition);
                if let Some(def) = get_state_def_mut(&mut root_def.states, &path) {
                    entry_actions.retain(|a| !def.entry.contains(a));
                    exit_actions.retain(|a| !def.exit.contains(a));
                    if !entry_actions.is_empty() {
                        entry_actions.append(&mut def.entry);
                        def.entry = entry_actions;
                    }
                    if !exit_actions.is_empty() {
                        exit_actions.append(&mut def.exit);
                        def.exit = exit_actions;
                    }
                }
            }
            let states_val = js_sys::Reflect::get(&node_obj, &JsValue::from_str("states"))
                .unwrap_or(JsValue::UNDEFINED);
            if states_val.is_object() && !states_val.is_null() {
                let states_obj = js_sys::Object::from(states_val);
                let keys = js_sys::Object::keys(&states_obj);
                for i in 0..keys.length() {
                    let k = keys.get(i);
                    if let Some(k_str) = k.as_string() {
                        let child_val = js_sys::Reflect::get(&states_obj, &k)
                            .unwrap_or(JsValue::UNDEFINED);
                        let child_path = if path.is_empty() {
                            k_str
                        } else {
                            format!("{}.{}", path, k_str)
                        };
                        stack.push((child_val, child_path));
                    }
                }
            }
        }
    }
}

pub(crate) fn resolve_host_clock(custom_clock: Option<&JsValue>) -> Option<(js_sys::Function, js_sys::Function)> {
    if let Some(opts) = custom_clock {
        if opts.is_object() && !opts.is_null() {
            let st = js_sys::Reflect::get(opts, &JsValue::from_str("setTimeout")).unwrap_or(JsValue::UNDEFINED);
            let ct = js_sys::Reflect::get(opts, &JsValue::from_str("clearTimeout")).unwrap_or(JsValue::UNDEFINED);
            if let (Ok(st_fn), Ok(ct_fn)) = (st.dyn_into::<js_sys::Function>(), ct.dyn_into::<js_sys::Function>()) {
                return Some((st_fn, ct_fn));
            }
        }
    }
    let global = js_sys::global();
    let st = js_sys::Reflect::get(&global, &JsValue::from_str("setTimeout")).unwrap_or(JsValue::UNDEFINED);
    let ct = js_sys::Reflect::get(&global, &JsValue::from_str("clearTimeout")).unwrap_or(JsValue::UNDEFINED);
    if let (Ok(st_fn), Ok(ct_fn)) = (st.dyn_into::<js_sys::Function>(), ct.dyn_into::<js_sys::Function>()) {
        Some((st_fn, ct_fn))
    } else {
        None
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct Actor {
    pub(crate) inner: Rc<RefCell<ActorInner>>,
}

#[wasm_bindgen]
impl Actor {
    #[wasm_bindgen(getter)]
    pub fn is_stopped(&self) -> bool {
        self.inner.borrow().is_stopped
    }

    #[wasm_bindgen(constructor)]
    pub fn new(val: JsValue, options: JsValue) -> Result<Actor, JsValue> {
        let mut guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>> = IndexMap::new();
        let mut actions = IndexMap::new();
        let mut actions_with_effects = IndexMap::new();
        let mut action_descriptors = IndexMap::new();
        let mut guards_holder = Rc::new(RefCell::new(IndexMap::new()));
        let mut logger_holder = Rc::new(RefCell::new(None));

        let mut machine = if let Some(state_node) = try_clone_state_node(&val) {
            guards = state_node.guards;
            guards_holder = state_node.guards_holder;
            actions = state_node.actions;
            action_descriptors = state_node.action_descriptors;
            logger_holder = state_node.logger_holder;
            state_node.machine
        } else if val.is_string() {
            let json_definition = val.as_string().unwrap();
            let def = lavi::create_machine(&json_definition)
                .map_err(|e| JsValue::from_str(&format!("Failed to parse machine: {}", e)))?;
            RuntimeMachine::from_definition(def).map_err(|e| JsValue::from_str(&e))?
        } else {
            let mut counter = 0;
            process_config(
                &val,
                &mut guards,
                &mut actions,
                &mut actions_with_effects,
                &mut action_descriptors,
                0,
                &mut counter,
                "",
                &guards_holder,
                &logger_holder,
            );
            let normalized_val = normalize_undefined(&val, 0);
            if normalized_val.is_object() && !normalized_val.is_null() {
                let context_key = JsValue::from_str("context");
                let context_val =
                    js_sys::Reflect::get(&normalized_val, &context_key).unwrap_or(JsValue::UNDEFINED);
                if let Ok(func) = context_val.dyn_into::<js_sys::Function>() {
                    let arg_obj = js_sys::Object::new();
                    let _ =
                        js_sys::Reflect::set(&arg_obj, &JsValue::from_str("input"), &JsValue::UNDEFINED);
                    let dummy_spawn = js_sys::Function::new_no_args("return { send: function() {}, getSnapshot: function() { return { status: 'active' }; }, sessionId: 'x:dummy' };");
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("spawn"), &dummy_spawn);
                    let dummy_self = js_sys::Function::new_no_args("return { send: function() {}, getSnapshot: function() { return {}; } };");
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &dummy_self);
                    if let Ok(res) = func.call1(&JsValue::NULL, &arg_obj) {
                        let _ = js_sys::Reflect::set(&normalized_val, &context_key, &res);
                    }
                }
            }

            let json_string = js_sys::JSON::stringify(&normalized_val)
                .map_err(|_| JsValue::from_str("Failed to stringify machine definition"))?
                .as_string()
                .unwrap_or_default();
            let machine_def = lavi::create_machine(&json_string)
                .map_err(|e| JsValue::from_str(&format!("Failed to parse definition: {}", e)))?;
            let mut m = RuntimeMachine::from_definition(machine_def).map_err(|e| JsValue::from_str(&e))?;
            m.actions_with_effects.extend(actions_with_effects.drain(..));
            m
        };

        parse_implementations(
            &options,
            &mut guards,
            &mut actions,
            &mut machine.actions_with_effects,
            &mut action_descriptors,
            &logger_holder,
        );
        guards_holder.borrow_mut().extend(guards.clone());

        parse_and_register_delays(&options, &mut machine);

        let tick_override = Rc::new(core::cell::Cell::new(None));
        let override_cell = tick_override.clone();
        machine.register_clock(Rc::new(move || {
            override_cell.get().unwrap_or_else(|| js_sys::Date::now() as u64)
        }));
        machine.register_warn_fn(Rc::new(|msg| {
            web_sys::console::warn_1(&JsValue::from_str(msg));
        }));

        let mut output_funcs = IndexMap::new();
        if val.is_object() {
            collect_output_resolvers(&val, "", &mut output_funcs);
        }
        for (p, func) in output_funcs {
            let f = func.clone();
            let resolver: lavi::OutputResolver = Rc::new(move |ctx: &Value, evt: &Value| {
                let ctx_js = value_to_js(ctx);
                let evt_js = value_to_js(evt);
                let arg_obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
                if let Ok(res) = f.call1(&JsValue::NULL, &arg_obj) {
                    crate::utils::js_to_value(&res)
                } else {
                    Value::Null
                }
            });
            machine.register_output_resolver(&p, resolver);
        }

        let mut inner = ActorInner {
            machine,
            inspector: None,
            inspectors: Vec::new(),
            subscribers: Vec::new(),
            guards,
            guards_holder,
            actions,
            action_descriptors,
            config: val.clone(),
            is_stopped: false,
            is_initialized: false,
            is_in_tick: false,
            tick_override,
            host_clock: None,
            is_simulated_clock: false,
            active_timeout: None,
            cached_output: None,
            is_completed_emitted: false,
            is_done_event_sent: false,
            logger: None,
            logger_holder,
            input_js: None,
            initialization_error: None,
            session_id: String::new(),
            custom_id: None,
            children: IndexMap::new(),
            child_sources: IndexMap::new(),
            parent: None,
            system: None,
            self_js: None,
            implementations: js_sys::Object::new().into(),
            pending_start_actors: Vec::new(),
            send_timer_handles: IndexMap::new(),
        };
        configure_logger(&options, &mut inner);
        inner.restore_snapshot_option(&options)?;
        inner.sync_tracked_entry_exit(&val);
        let actor = Actor {
            inner: Rc::new(RefCell::new(inner)),
        };
        actor.apply_clock_option(&options);
        Ok(actor)
    }

    pub(crate) fn emit_error(&self, err_val: &JsValue) {
        let subscribers = self.inner.borrow().subscribers.clone();
        for (_, sub) in &subscribers {
            if let Subscriber::Observer { error: Some(ref err_fn), .. } = sub {
                let _ = err_fn.call1(&JsValue::NULL, err_val);
            }
        }
    }

    pub(crate) fn drain_reentrant(
        &self,
        machine: &mut RuntimeMachine,
        guards: &IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
        actions: &IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    ) -> bool {
        let mut transitioned = false;
        loop {
            let (is_now_stopped, sid) = {
                let inner = self.inner.borrow();
                (inner.is_stopped, inner.session_id.clone())
            };
            if is_now_stopped
                || machine.status == lavi::MachineStatus::Stopped
                || machine.status == lavi::MachineStatus::Done
                || machine.is_done()
            {
                RELAYED_QUEUE.with(|q| { q.borrow_mut().shift_remove(&sid); });
                machine.external_queue.clear();
                self.inner.borrow_mut().machine.external_queue.clear();
                if is_now_stopped || machine.status == lavi::MachineStatus::Stopped {
                    machine.stop();
                    self.inner.borrow_mut().is_stopped = true;
                }
                break;
            }
            let next_evt = machine
                .external_queue
                .pop_front()
                .or_else(|| self.inner.borrow_mut().machine.external_queue.pop_front());
            let Some((ev_name, ev_payload)) = next_evt else {
                break;
            };
            let is_relayed = RELAYED_QUEUE.with(|q| q.borrow_mut().get_mut(&sid).and_then(|q| q.pop_front())).unwrap_or(false);
            let prev_relay = IN_SYSTEM_RELAY.with(|c| {
                let old = c.get();
                if is_relayed {
                    c.set(true);
                }
                old
            });
            let transitioned_now = machine
                .transition(&ev_name, &ev_payload, guards, actions)
                .is_some();
            IN_SYSTEM_RELAY.with(|c| c.set(prev_relay));
            if transitioned_now {
                transitioned = true;
                if let Ok(mut inner) = self.inner.try_borrow_mut() {
                    inner.machine.context = machine.context.clone();
                    inner.machine.current_state = machine.current_state.clone();
                    inner.machine.status = machine.status;
                }
            }
        }
        transitioned
    }

    pub(crate) fn apply_clock_option(&self, options: &JsValue) {
        if !options.is_object() || options.is_null() {
            return;
        }
        let clock_key = JsValue::from_str("clock");
        let clock_val = js_sys::Reflect::get(options, &clock_key).unwrap_or(JsValue::UNDEFINED);
        if clock_val.is_undefined() || clock_val.is_null() {
            return;
        }

        let tick_override = self.inner.borrow().tick_override.clone();
        if let Ok(func) = clock_val.clone().dyn_into::<js_sys::Function>() {
            self.inner.borrow_mut().machine.register_clock(Rc::new(move || {
                if let Some(override_now) = tick_override.get() {
                    return override_now;
                }
                func.call0(&JsValue::NULL)
                    .ok()
                    .and_then(|v| v.as_f64())
                    .map(|v| v.max(0.0) as u64)
                    .unwrap_or(0)
            }));
            return;
        }

        if clock_val.is_object() {
            let attach = js_sys::Reflect::get(&clock_val, &JsValue::from_str("__registerActor"))
                .unwrap_or(JsValue::UNDEFINED);
            if let Ok(attach_fn) = attach.dyn_into::<js_sys::Function>() {
                self.inner.borrow_mut().is_simulated_clock = true;
                let actor_js: JsValue = self.clone().into();
                let _ = attach_fn.call1(&clock_val, &actor_js);
                return;
            }

            let now_prop = js_sys::Reflect::get(&clock_val, &JsValue::from_str("now"))
                .unwrap_or(JsValue::UNDEFINED);
            if let Ok(now_fn) = now_prop.dyn_into::<js_sys::Function>() {
                let clock_this = clock_val.clone();
                self.inner.borrow_mut().machine.register_clock(Rc::new(move || {
                    if let Some(override_now) = tick_override.get() {
                        return override_now;
                    }
                    now_fn
                        .call0(&clock_this)
                        .ok()
                        .and_then(|v| v.as_f64())
                        .map(|v| v.max(0.0) as u64)
                        .unwrap_or(0)
                }));
            }

            let st = js_sys::Reflect::get(&clock_val, &JsValue::from_str("setTimeout"))
                .unwrap_or(JsValue::UNDEFINED);
            let ct = js_sys::Reflect::get(&clock_val, &JsValue::from_str("clearTimeout"))
                .unwrap_or(JsValue::UNDEFINED);
            if st.is_function() && ct.is_function() {
                self.inner.borrow_mut().host_clock = Some(clock_val.clone());
            }
        }
    }

    pub(crate) fn make_actor_ref_js_parts(id: &str, session_id: &str) -> JsValue {
        let obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("id"), &JsValue::from_str(id));
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("sessionId"), &JsValue::from_str(session_id));
        let to_json_fn = js_sys::Function::new_no_args(
            "return { id: this.id, xstate$$type: 1 };",
        );
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("toJSON"), &to_json_fn);
        obj.into()
    }

    pub(crate) fn inspect_event_payload_to_js(evt: &Value, input_js: &Option<JsValue>) -> JsValue {
        if let Value::Object(map) = evt {
            if let Some(Value::String(t)) = map.get("type") {
                if t == "xstate.init" {
                    let obj = js_sys::Object::new();
                    let inp = input_js.clone().unwrap_or(JsValue::UNDEFINED);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("input"), &inp);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.init"));
                    return obj.into();
                }
                if t.starts_with("xstate.done.actor.") {
                    let js_evt = Self::event_to_js(evt);
                    let out_val = js_sys::Reflect::get(&js_evt, &JsValue::from_str("output")).unwrap_or(JsValue::UNDEFINED);
                    let _ = js_sys::Reflect::set(&js_evt, &JsValue::from_str("output"), &out_val);
                    return js_evt;
                }
            }
        }
        Self::event_to_js(evt)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build_inspect_snapshot_js(
        machine_template: &RuntimeMachine,
        state: &lavi::StateValue,
        context: &Value,
        status: lavi::MachineStatus,
        config: &JsValue,
        guards: &IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
        actions: &IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
        action_descriptors: &IndexMap<String, JsValue>,
        logger_holder: &Rc<RefCell<Option<js_sys::Function>>>,
        cached_output: Option<&JsValue>,
    ) -> JsValue {
        let mut temp_machine = machine_template.clone();
        temp_machine.current_state = state.clone();
        temp_machine.context = context.clone();
        temp_machine.status = status;

        let active_paths = state.get_active_paths();
        let value_val = Self::build_snapshot_state_value(&temp_machine, &active_paths);
        let ctx_val = match context {
            Value::Null => js_sys::Object::new().into(),
            other => value_to_js(other),
        };
        let history_obj = compute_history_value_obj(
            &temp_machine,
            guards,
            actions,
            action_descriptors,
            config,
            logger_holder,
        );
        let status_str = match status {
            lavi::MachineStatus::Done => "done",
            lavi::MachineStatus::Stopped => "stopped",
            lavi::MachineStatus::Active => "active",
        };
        let output_val = if status == lavi::MachineStatus::Done {
            if let Some(c) = cached_output {
                (*c).clone()
            } else {
                Self::compute_output_js(config, &temp_machine, &ctx_val)
            }
        } else {
            JsValue::UNDEFINED
        };
        let tags_arr = js_sys::Array::new();
        for tag in temp_machine.get_tags() {
            tags_arr.push(&JsValue::from_str(&tag));
        }

        let snap_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("children"), &js_sys::Object::new());
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("context"), &ctx_val);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("error"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("historyValue"), &history_obj);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("output"), &output_val);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("status"), &JsValue::from_str(status_str));
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("tags"), &tags_arr);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("value"), &value_val);
        snap_obj.into()
    }

    pub(crate) fn build_machine_inspector(&self) -> lavi::InspectFunction {
        let inner_weak = Rc::downgrade(&self.inner);
        let config_clone = self.inner.borrow().config.clone();
        Rc::new(move |ev: &lavi::InspectEvent| {
            if matches!(ev, lavi::InspectEvent::Event { .. }) && IN_SYSTEM_RELAY.with(|c| c.get()) {
                return;
            }
            let Some(inner_rc) = inner_weak.upgrade() else {
                return;
            };
            let (
                inspectors,
                session_id,
                custom_id,
                input_js,
                action_descriptors,
                machine_template,
                guards,
                actions,
                logger_holder,
                self_js_opt,
                parent_opt,
                system_opt,
            ) = {
                let Ok(shared) = inner_rc.try_borrow() else {
                    return;
                };
                if shared.inspectors.is_empty() && shared.inspector.is_none() && shared.system.is_none() {
                    return;
                }
                let mut list = shared.inspectors.clone();
                if let Some(ref legacy_fn) = shared.inspector {
                    list.push((0, Subscriber::Fn(legacy_fn.clone())));
                }
                (
                    list,
                    shared.session_id.clone(),
                    shared.custom_id.clone(),
                    shared.input_js.clone(),
                    shared.action_descriptors.clone(),
                    shared.machine.clone(),
                    shared.guards.clone(),
                    shared.actions.clone(),
                    shared.logger_holder.clone(),
                    shared.self_js.clone(),
                    shared.parent.clone(),
                    shared.system.clone(),
                )
            };

            let actor_id = custom_id.unwrap_or_else(|| session_id.clone());
            let actor_ref_js = if let Some(ref s) = self_js_opt {
                s.clone()
            } else {
                Self::make_actor_ref_js_parts(&actor_id, &session_id)
            };

            let ev_js: JsValue = match ev {
                lavi::InspectEvent::Actor => {
                    let obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("actorRef"), &actor_ref_js);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("rootId"), &JsValue::from_str(&actor_id));
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.actor"));
                    obj.into()
                }
                lavi::InspectEvent::Event { event } => {
                    let evt_js = Self::inspect_event_payload_to_js(event, &input_js);
                    let is_init = match event {
                        Value::Object(map) => map.get("type").and_then(|v| match v { Value::String(s) => Some(s.as_str()), _ => None }) == Some("xstate.init"),
                        Value::String(s) => s == "xstate.init",
                        _ => false,
                    };
                    let default_src = JsValue::UNDEFINED;
                    let source_ref = if is_init {
                        parent_opt.as_ref().unwrap_or(&default_src)
                    } else {
                        &default_src
                    };
                    let obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("actorRef"), &actor_ref_js);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("event"), &evt_js);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("rootId"), &JsValue::from_str(&actor_id));
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("sourceRef"), source_ref);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.event"));
                    obj.into()
                }
                lavi::InspectEvent::Action { action, params } => {
                    let action_obj = js_sys::Object::new();
                    match action {
                        lavi::Action::Raise(ev_val) => {
                            let params_obj = js_sys::Object::new();
                            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("delay"), &JsValue::UNDEFINED);
                            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("event"), &value_to_js(ev_val));
                            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("id"), &JsValue::UNDEFINED);
                            let _ = js_sys::Reflect::set(&action_obj, &JsValue::from_str("params"), &params_obj);
                            let _ = js_sys::Reflect::set(&action_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.raise"));
                        }
                        lavi::Action::DelayedRaise(dra) => {
                            let params_obj = js_sys::Object::new();
                            let delay_val = match &dra.delay {
                                lavi::Delay::Literal(ms) => JsValue::from_f64(*ms as f64),
                                lavi::Delay::Named(s) => JsValue::from_str(s),
                            };
                            let id_val = dra.id.as_ref().map(|s| JsValue::from_str(s)).unwrap_or(JsValue::UNDEFINED);
                            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("delay"), &delay_val);
                            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("event"), &value_to_js(&dra.event));
                            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("id"), &id_val);
                            let _ = js_sys::Reflect::set(&action_obj, &JsValue::from_str("params"), &params_obj);
                            let _ = js_sys::Reflect::set(&action_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.raise"));
                        }
                        lavi::Action::Cancel(ca) => {
                            let params_obj = js_sys::Object::new();
                            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("sendId"), &JsValue::from_str(&ca.send_id));
                            let _ = js_sys::Reflect::set(&action_obj, &JsValue::from_str("params"), &params_obj);
                            let _ = js_sys::Reflect::set(&action_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.cancel"));
                        }
                        lavi::Action::Log(la) => {
                            let params_obj = js_sys::Object::new();
                            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("value"), &value_to_js(&la.value));
                            let _ = js_sys::Reflect::set(&action_obj, &JsValue::from_str("params"), &params_obj);
                            let _ = js_sys::Reflect::set(&action_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.log"));
                        }
                        lavi::Action::Named(name) | lavi::Action::Generic(lavi::GenericAction { action_type: name, .. }) => {
                            let desc_opt = action_descriptors.get(name);
                            let resolved_type = if let Some(desc) = desc_opt {
                                let at = js_sys::Reflect::get(desc, &JsValue::from_str("actionType"))
                                    .ok()
                                    .and_then(|v| v.as_string())
                                    .unwrap_or_default();
                                if !at.is_empty() {
                                    at
                                } else {
                                    let dn = js_sys::Reflect::get(desc, &JsValue::from_str("displayName"))
                                        .ok()
                                        .and_then(|v| v.as_string())
                                        .unwrap_or_default();
                                    if !dn.is_empty() {
                                        dn
                                    } else if name.starts_with("auto_action_") || name.starts_with("generated_action_") {
                                        "(anonymous)".to_string()
                                    } else {
                                        name.clone()
                                    }
                                }
                            } else if name.starts_with("auto_action_") || name.starts_with("generated_action_") {
                                "(anonymous)".to_string()
                            } else {
                                name.clone()
                            };
                            if resolved_type == "xstate.assign" || resolved_type == "xstate.enqueueActions" {
                                return;
                            }
                            let resolved_params = if let Some(p) = params {
                                value_to_js(p)
                            } else if let Some(desc) = desc_opt {
                                js_sys::Reflect::get(desc, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED)
                            } else {
                                JsValue::UNDEFINED
                            };
                            let _ = js_sys::Reflect::set(&action_obj, &JsValue::from_str("params"), &resolved_params);
                            let _ = js_sys::Reflect::set(&action_obj, &JsValue::from_str("type"), &JsValue::from_str(&resolved_type));
                        }
                        _ => {}
                    }
                    let obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("action"), &action_obj);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("actorRef"), &actor_ref_js);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("rootId"), &JsValue::from_str(&actor_id));
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.action"));
                    obj.into()
                }
                lavi::InspectEvent::Microstep {
                    event,
                    transitions,
                    state,
                    context,
                    status,
                } => {
                    let transitions_arr = js_sys::Array::new();
                    for t in transitions {
                        let t_obj = js_sys::Object::new();
                        let actions_arr = js_sys::Array::new();
                        for act in &t.actions {
                            match act {
                                lavi::Action::Named(n) | lavi::Action::Generic(lavi::GenericAction { action_type: n, .. }) => {
                                    if let Some(desc) = action_descriptors.get(n) {
                                        let raw = js_sys::Reflect::get(desc, &JsValue::from_str("__raw_action")).unwrap_or(JsValue::UNDEFINED);
                                        if !raw.is_undefined() {
                                            actions_arr.push(&raw);
                                        } else {
                                            actions_arr.push(desc);
                                        }
                                    } else {
                                        let a_obj = js_sys::Object::new();
                                        let _ = js_sys::Reflect::set(&a_obj, &JsValue::from_str("type"), &JsValue::from_str(n));
                                        actions_arr.push(&a_obj);
                                    }
                                }
                                lavi::Action::Raise(ev_val) => {
                                    let a_obj = js_sys::Object::new();
                                    let params_obj = js_sys::Object::new();
                                    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("delay"), &JsValue::UNDEFINED);
                                    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("event"), &value_to_js(ev_val));
                                    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("id"), &JsValue::UNDEFINED);
                                    let _ = js_sys::Reflect::set(&a_obj, &JsValue::from_str("params"), &params_obj);
                                    let _ = js_sys::Reflect::set(&a_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.raise"));
                                    actions_arr.push(&a_obj);
                                }
                                lavi::Action::DelayedRaise(dra) => {
                                    let a_obj = js_sys::Object::new();
                                    let params_obj = js_sys::Object::new();
                                    let delay_val = match &dra.delay {
                                        lavi::Delay::Literal(ms) => JsValue::from_f64(*ms as f64),
                                        lavi::Delay::Named(s) => JsValue::from_str(s),
                                    };
                                    let id_val = dra.id.as_ref().map(|s| JsValue::from_str(s)).unwrap_or(JsValue::UNDEFINED);
                                    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("delay"), &delay_val);
                                    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("event"), &value_to_js(&dra.event));
                                    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("id"), &id_val);
                                    let _ = js_sys::Reflect::set(&a_obj, &JsValue::from_str("params"), &params_obj);
                                    let _ = js_sys::Reflect::set(&a_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.raise"));
                                    actions_arr.push(&a_obj);
                                }
                                lavi::Action::Cancel(ca) => {
                                    let a_obj = js_sys::Object::new();
                                    let params_obj = js_sys::Object::new();
                                    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("sendId"), &JsValue::from_str(&ca.send_id));
                                    let _ = js_sys::Reflect::set(&a_obj, &JsValue::from_str("params"), &params_obj);
                                    let _ = js_sys::Reflect::set(&a_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.cancel"));
                                    actions_arr.push(&a_obj);
                                }
                                lavi::Action::Log(la) => {
                                    let a_obj = js_sys::Object::new();
                                    let params_obj = js_sys::Object::new();
                                    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("value"), &value_to_js(&la.value));
                                    let _ = js_sys::Reflect::set(&a_obj, &JsValue::from_str("params"), &params_obj);
                                    let _ = js_sys::Reflect::set(&a_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.log"));
                                    actions_arr.push(&a_obj);
                                }
                                _ => {}
                            }
                        }
                        let guard_js = match &t.guard {
                            None => JsValue::UNDEFINED,
                            Some(lavi::TransitionGuard::Name(g_name))
                            | Some(lavi::TransitionGuard::Object(lavi::GuardObject { guard_type: g_name, .. })) => {
                                action_descriptors.get(g_name).cloned().unwrap_or_else(|| JsValue::from_str(g_name))
                            }
                            Some(_) => JsValue::UNDEFINED,
                        };
                        let source_obj = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(&source_obj, &JsValue::from_str("id"), &JsValue::from_str(&t.source_id));
                        let target_js = if let Some(ref targets) = t.target_ids {
                            let t_arr = js_sys::Array::new();
                            for tid in targets {
                                let tgt_obj = js_sys::Object::new();
                                let _ = js_sys::Reflect::set(&tgt_obj, &JsValue::from_str("id"), &JsValue::from_str(tid));
                                t_arr.push(&tgt_obj);
                            }
                            t_arr.into()
                        } else {
                            JsValue::UNDEFINED
                        };
                        let to_json_fn = js_sys::Function::new_no_args(
                            "return { actions: this.actions, eventType: this.eventType, guard: this.guard, reenter: this.reenter, source: typeof this.source === 'string' ? this.source : '#' + this.source.id, target: this.target ? this.target.map(function(x) { return typeof x === 'string' ? x : '#' + x.id; }) : undefined, toJSON: this.toJSON };",
                        );
                        let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("actions"), &actions_arr);
                        let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("eventType"), &JsValue::from_str(&t.event_type));
                        let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("guard"), &guard_js);
                        let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("reenter"), &JsValue::from_bool(t.reenter));
                        let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("source"), &source_obj);
                        let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("target"), &target_js);
                        let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("toJSON"), &to_json_fn);
                        transitions_arr.push(&t_obj);
                    }
                    let mut cached_out = None;
                    if *status == lavi::MachineStatus::Done {
                        if let Ok(mut inner) = inner_rc.try_borrow_mut() {
                            if let Some(ref c) = inner.cached_output {
                                cached_out = Some(c.clone());
                            } else {
                                let ctx_val = match context {
                                    Value::Null => js_sys::Object::new().into(),
                                    other => value_to_js(other),
                                };
                                let mut temp_m = machine_template.clone();
                                temp_m.current_state = state.clone();
                                temp_m.context = context.clone();
                                temp_m.status = *status;
                                let evaluated = Self::compute_output_js(&config_clone, &temp_m, &ctx_val);
                                inner.cached_output = Some(evaluated.clone());
                                cached_out = Some(evaluated);
                            }
                        }
                    }
                    let evt_js = Self::inspect_event_payload_to_js(event, &input_js);
                    let snap_js = Self::build_inspect_snapshot_js(
                        &machine_template,
                        state,
                        context,
                        *status,
                        &config_clone,
                        &guards,
                        &actions,
                        &action_descriptors,
                        &logger_holder,
                        cached_out.as_ref(),
                    );
                    let obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("_transitions"), &transitions_arr);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("actorRef"), &actor_ref_js);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("event"), &evt_js);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("rootId"), &JsValue::from_str(&actor_id));
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("snapshot"), &snap_js);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.microstep"));
                    obj.into()
                }
                lavi::InspectEvent::Snapshot {
                    event,
                    state,
                    context,
                    status,
                } => {
                    let children_obj = js_sys::Object::new();

                    let mut paths_to_stop = Vec::new();
                    let mut paths_to_spawn = Vec::new();
                    if let Ok(mut inner) = inner_rc.try_borrow_mut() {
                        inner.machine.context = context.clone();
                        inner.machine.current_state = state.clone();
                        inner.machine.status = *status;
                        inner.machine.is_started = true;

                        let active = state.get_active_paths();
                        let current_source_paths: Vec<String> = inner.child_sources.keys().cloned().collect();
                        for sp in current_source_paths {
                            if !sp.is_empty() && !active.iter().any(|a| a == &sp || a.starts_with(&format!("{}.", sp))) {
                                paths_to_stop.push(sp);
                            }
                        }
                        if *status != lavi::MachineStatus::Stopped && inner.machine.last_error.is_none() {
                            if !inner.child_sources.contains_key("") {
                                paths_to_spawn.push("".to_string());
                            }
                            for p in &active {
                                let parts: Vec<&str> = p.split('.').collect();
                                for i in 1..=parts.len() {
                                    let ancestor = parts[0..i].join(".");
                                    if !inner.child_sources.contains_key(&ancestor) && !paths_to_spawn.contains(&ancestor) {
                                        paths_to_spawn.push(ancestor);
                                    }
                                }
                            }
                        }
                    }

                    collect_and_stop_invokes(&inner_rc, &paths_to_stop);
                    flush_pending_starts_detached(&inner_rc);

                    for sp in paths_to_spawn {
                        spawn_invokes_for_path_detached(&inner_rc, &sp, event, context, false);
                    }

                    if let Ok(inner) = inner_rc.try_borrow() {
                        for (k, v) in &inner.children {
                            let _ = js_sys::Reflect::set(&children_obj, &JsValue::from_str(k), v);
                        }
                    }
                    let mut cached_out = None;
                    if *status == lavi::MachineStatus::Done {
                        if let Ok(mut inner) = inner_rc.try_borrow_mut() {
                            if let Some(ref c) = inner.cached_output {
                                cached_out = Some(c.clone());
                            } else {
                                let ctx_val = match context {
                                    Value::Null => js_sys::Object::new().into(),
                                    other => value_to_js(other),
                                };
                                let mut temp_m = machine_template.clone();
                                temp_m.current_state = state.clone();
                                temp_m.context = context.clone();
                                temp_m.status = *status;
                                let evaluated = Self::compute_output_js(&config_clone, &temp_m, &ctx_val);
                                inner.cached_output = Some(evaluated.clone());
                                cached_out = Some(evaluated);
                            }
                        }
                    }
                    let evt_js = Self::inspect_event_payload_to_js(event, &input_js);
                    let snap_js = Self::build_inspect_snapshot_js(
                        &machine_template,
                        state,
                        context,
                        *status,
                        &config_clone,
                        &guards,
                        &actions,
                        &action_descriptors,
                        &logger_holder,
                        cached_out.as_ref(),
                    );
                    let _ = js_sys::Reflect::set(&snap_js, &JsValue::from_str("children"), &children_obj);
                    let obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("actorRef"), &actor_ref_js);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("event"), &evt_js);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("rootId"), &JsValue::from_str(&actor_id));
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("snapshot"), &snap_js);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.snapshot"));

                    if *status == lavi::MachineStatus::Done {
                        if let Ok(mut inner) = inner_rc.try_borrow_mut() {
                            if !inner.is_done_event_sent {
                                inner.is_done_event_sent = true;
                                if let Some(ref parent) = inner.parent.clone() {
                                    let child_id = inner.custom_id.clone().unwrap_or_else(|| inner.session_id.clone());
                                    let done_evt = js_sys::Object::new();
                                    let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("type"), &JsValue::from_str(&format!("xstate.done.actor.{}", child_id)));
                                    let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("actorId"), &JsValue::from_str(&child_id));
                                    let out_val = inner.cached_output.clone().unwrap_or(JsValue::UNDEFINED);
                                    let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("output"), &out_val);
                                    if let Some(ref sys) = inner.system.clone() {
                                        if let Ok(relay) = js_sys::Reflect::get(sys, &JsValue::from_str("_relay")) {
                                            if let Ok(rf) = relay.dyn_into::<js_sys::Function>() {
                                                let _ = rf.call3(sys, &actor_ref_js, parent, &done_evt);
                                            }
                                        }
                                    } else if let Ok(send) = js_sys::Reflect::get(parent, &JsValue::from_str("send")) {
                                        if let Ok(sf) = send.dyn_into::<js_sys::Function>() {
                                            let _ = sf.call1(parent, &done_evt);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    obj.into()
                }
            };

            for (_, insp_sub) in &inspectors {
                match insp_sub {
                    Subscriber::Fn(func) => {
                        let _ = func.call1(&JsValue::NULL, &ev_js);
                    }
                    Subscriber::Observer { next: Some(next_fn), .. } => {
                        let _ = next_fn.call1(&JsValue::NULL, &ev_js);
                    }
                    _ => {}
                }
            }

            if let Some(ref sys) = system_opt {
                if let Ok(emit) = js_sys::Reflect::get(sys, &JsValue::from_str("_emit")) {
                    if let Ok(ef) = emit.dyn_into::<js_sys::Function>() {
                        let _ = ef.call1(sys, &ev_js);
                    }
                }
            }
        })
    }

    pub(crate) fn add_inspector(&self, observer_or_fn: JsValue, emit_actor_event: bool) -> Result<JsValue, JsValue> {
        let sub = if observer_or_fn.is_function() {
            Subscriber::Fn(observer_or_fn.unchecked_into())
        } else if observer_or_fn.is_object() && !observer_or_fn.is_null() {
            let obj = js_sys::Object::from(observer_or_fn);
            let next_fn = js_sys::Reflect::get(&obj, &JsValue::from_str("next"))
                .ok()
                .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
            let complete_fn = js_sys::Reflect::get(&obj, &JsValue::from_str("complete"))
                .ok()
                .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
            let error_fn = js_sys::Reflect::get(&obj, &JsValue::from_str("error"))
                .ok()
                .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
            Subscriber::Observer {
                next: next_fn,
                complete: complete_fn,
                error: error_fn,
            }
        } else {
            return Err(JsValue::from_str("inspect requires a function or observer"));
        };

        static NEXT_INSP_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);
        let insp_id = NEXT_INSP_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

        {
            let insp_fn = self.build_machine_inspector();
            let mut inner = self.inner.borrow_mut();
            if inner.machine.inspector_fn.is_none() {
                inner.machine.register_inspector(insp_fn);
            }
            inner.inspectors.push((insp_id, sub.clone()));
        }

        if emit_actor_event {
            let actor_id = self.id();
            let session_id = self.session_id();
            let actor_ref_js = Self::make_actor_ref_js_parts(&actor_id, &session_id);
            let obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("actorRef"), &actor_ref_js);
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("rootId"), &JsValue::from_str(&actor_id));
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.actor"));
            let ev_js: JsValue = obj.into();
            match &sub {
                Subscriber::Fn(func) => {
                    let _ = func.call1(&JsValue::NULL, &ev_js);
                }
                Subscriber::Observer { next: Some(next_fn), .. } => {
                    let _ = next_fn.call1(&JsValue::NULL, &ev_js);
                }
                _ => {}
            }
        }

        let inner_weak = Rc::downgrade(&self.inner);
        let unsub_closure = Closure::wrap(Box::new(move || {
            if let Some(inner_rc) = inner_weak.upgrade() {
                if let Ok(mut inner) = inner_rc.try_borrow_mut() {
                    inner.inspectors.retain(|(id, _)| *id != insp_id);
                    if inner.inspectors.is_empty() && inner.inspector.is_none() {
                        inner.machine.inspector_fn = None;
                    }
                }
            }
        }) as Box<dyn FnMut()>);

        let unsub_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &unsub_obj,
            &JsValue::from_str("unsubscribe"),
            &unsub_closure.into_js_value(),
        );
        Ok(unsub_obj.into())
    }

    #[wasm_bindgen(getter, js_name = sessionId)]
    pub fn session_id(&self) -> String {
        if let Ok(inner) = self.inner.try_borrow() {
            inner.session_id.clone()
        } else {
            String::new()
        }
    }

    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        if let Ok(inner) = self.inner.try_borrow() {
            inner.custom_id.clone().unwrap_or_else(|| inner.session_id.clone())
        } else {
            String::new()
        }
    }

    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json(&self) -> JsValue {
        let obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("id"), &JsValue::from_str(&self.id()));
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("xstate$$type"), &JsValue::from_f64(1.0));
        obj.into()
    }

    #[wasm_bindgen(getter)]
    pub fn system(&self) -> JsValue {
        if let Ok(inner) = self.inner.try_borrow() {
            if let Some(ref sys) = inner.system {
                return sys.clone();
            }
        }
        let actor_clone = self.clone();
        let inspect_closure = Closure::wrap(Box::new(move |observer_or_fn: JsValue| -> JsValue {
            actor_clone
                .add_inspector(observer_or_fn, false)
                .unwrap_or(JsValue::UNDEFINED)
        }) as Box<dyn FnMut(JsValue) -> JsValue>);
        let sys_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &sys_obj,
            &JsValue::from_str("inspect"),
            &inspect_closure.into_js_value(),
        );
        sys_obj.into()
    }

    #[wasm_bindgen(getter)]
    pub fn children(&self) -> JsValue {
        let obj = js_sys::Object::new();
        if let Ok(inner) = self.inner.try_borrow() {
            for (k, v) in &inner.children {
                let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(k), v);
            }
        }
        obj.into()
    }

    #[wasm_bindgen(getter)]
    pub fn config(&self) -> JsValue {
        if let Ok(inner) = self.inner.try_borrow() {
            inner.config.clone()
        } else {
            JsValue::UNDEFINED
        }
    }

    #[wasm_bindgen(getter, js_name = _parent)]
    pub fn parent(&self) -> JsValue {
        if let Ok(inner) = self.inner.try_borrow() {
            inner.parent.clone().unwrap_or(JsValue::UNDEFINED)
        } else {
            JsValue::UNDEFINED
        }
    }

    #[wasm_bindgen(getter, js_name = parent)]
    pub fn public_parent(&self) -> JsValue {
        if let Ok(inner) = self.inner.try_borrow() {
            inner.parent.clone().unwrap_or(JsValue::UNDEFINED)
        } else {
            JsValue::UNDEFINED
        }
    }

    #[wasm_bindgen(js_name = _ensureInvokeChild)]
    pub fn ensure_invoke_child(&self, id: String) -> JsValue {
        {
            let Ok(inner) = self.inner.try_borrow() else {
                return JsValue::UNDEFINED;
            };
            if let Some(child) = inner.children.get(&id) {
                return child.clone();
            }
        }

        let (init_evt, current_ctx, pending) = {
            let Ok(inner) = self.inner.try_borrow() else {
                return JsValue::UNDEFINED;
            };
            let init_evt = Value::Object(lavi::indexmap! { "type".to_string() => Value::String("xstate.init".to_string()) });
            let current_ctx = inner.machine.context.clone();

            let mut matched = Vec::new();
            for p in inner.prepare_invokes_for_path("") {
                if p.invoke_id == id {
                    matched.push(p);
                }
            }
            let mut all_paths = Vec::new();
            let mut stack: Vec<(&IndexMap<String, lavi::StateDefinition>, String)> =
                vec![(&inner.machine.definition.states, String::new())];
            while let Some((states, prefix)) = stack.pop() {
                for (name, def) in states.iter().rev() {
                    let path = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{}.{}", prefix, name)
                    };
                    all_paths.push(path.clone());
                    if !def.states.is_empty() {
                        stack.push((&def.states, path));
                    }
                }
            }
            for p_str in all_paths {
                for p in inner.prepare_invokes_for_path(&p_str) {
                    if p.invoke_id == id {
                        matched.push(p);
                    }
                }
            }
            (init_evt, current_ctx, matched)
        };

        if !pending.is_empty() {
            execute_pending_invokes(&self.inner, pending, &init_evt, &current_ctx, false);
            if let Ok(inner) = self.inner.try_borrow() {
                if let Some(child) = inner.children.get(&id) {
                    return child.clone();
                }
            }
        }
        JsValue::UNDEFINED
    }

    #[wasm_bindgen(js_name = _registerSendTimer)]
    pub fn register_send_timer(&self, send_id: String, handle: JsValue) {
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.send_timer_handles.insert(send_id, handle);
        }
    }

    #[wasm_bindgen(js_name = _cancelSendTimer)]
    pub fn cancel_send_timer(&self, send_id: String) {
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            if let Some(handle) = inner.send_timer_handles.shift_remove(&send_id) {
                if let Some((_, clear_timeout)) = resolve_host_clock(inner.host_clock.as_ref()) {
                    let _ = clear_timeout.call1(&JsValue::NULL, &handle);
                }
            }
        }
    }

    #[wasm_bindgen(js_name = _removeChild)]
    pub fn remove_child(&self, child_ref: JsValue) {
        let mut removed_actor = None;
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            if let Some(id_str) = child_ref.as_string() {
                removed_actor = inner.children.shift_remove(&id_str);
            } else if child_ref.is_object() && !child_ref.is_null() {
                let mut key_to_remove = None;
                for (k, v) in &inner.children {
                    if v == &child_ref {
                        key_to_remove = Some(k.clone());
                        break;
                    }
                }
                if let Some(k) = key_to_remove {
                    removed_actor = inner.children.shift_remove(&k);
                }
            }
        }
        if let Some(act) = removed_actor {
            if let Ok(inner) = self.inner.try_borrow() {
                if let Some(ref sys) = inner.system {
                    if let Ok(unreg) = js_sys::Reflect::get(sys, &JsValue::from_str("_unregister")) {
                        if let Ok(uf) = unreg.dyn_into::<js_sys::Function>() {
                            let _ = uf.call1(sys, &act);
                        }
                    }
                }
            }
        }
    }

    #[wasm_bindgen(js_name = _emitError)]
    pub fn emit_error_js(&self, error: JsValue) {
        self.emit_error(&error);
    }

    #[wasm_bindgen(getter, js_name = _implementations)]
    pub fn implementations(&self) -> JsValue {
        if let Ok(inner) = self.inner.try_borrow() {
            inner.implementations.clone()
        } else {
            JsValue::UNDEFINED
        }
    }

    #[wasm_bindgen(js_name = _registerChild)]
    pub fn register_child(&self, id: String, child_actor: JsValue) {
        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.children.insert(id, child_actor);
        }
    }

    fn get_history_value_obj(&self, machine: &RuntimeMachine) -> JsValue {
        let (guards, actions, action_descriptors, config, logger_holder) = {
            if let Ok(inner) = self.inner.try_borrow() {
                (
                    inner.guards.clone(),
                    inner.actions.clone(),
                    inner.action_descriptors.clone(),
                    inner.config.clone(),
                    inner.logger_holder.clone(),
                )
            } else {
                (
                    IndexMap::new(),
                    IndexMap::new(),
                    IndexMap::new(),
                    JsValue::UNDEFINED,
                    Rc::new(RefCell::new(None)),
                )
            }
        };
        compute_history_value_obj(
            machine,
            &guards,
            &actions,
            &action_descriptors,
            &config,
            &logger_holder,
        )
    }

    #[wasm_bindgen(js_name = getSnapshot)]
    pub fn get_snapshot(&self) -> Snapshot {
        let (
            init_err,
            mut active_machine,
            is_stopped,
            is_initialized,
            cached_out,
            guards,
            actions,
            action_descriptors,
            config,
            children_val,
        ) = {
            if let Ok(inner) = self.inner.try_borrow() {
                let obj = js_sys::Object::new();
                for (k, v) in &inner.children {
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(k), v);
                }
                (
                    inner.initialization_error.clone(),
                    inner.machine.clone(),
                    inner.is_stopped,
                    inner.is_initialized,
                    inner.cached_output.clone(),
                    inner.guards.clone(),
                    inner.actions.clone(),
                    inner.action_descriptors.clone(),
                    inner.config.clone(),
                    obj.into(),
                )
            } else {
                return Snapshot {
                    value: JsValue::UNDEFINED,
                    context: JsValue::UNDEFINED,
                    status: "active".to_string(),
                    machine_ref: RuntimeMachine::default(),
                    guards: IndexMap::new(),
                    history_value: js_sys::Object::new().into(),
                    config: JsValue::UNDEFINED,
                    output: JsValue::UNDEFINED,
                    error: JsValue::UNDEFINED,
                    children: js_sys::Object::new().into(),
                };
            }
        };
        if let Some(err) = init_err {
            return Snapshot {
                value: JsValue::UNDEFINED,
                context: JsValue::UNDEFINED,
                status: "error".to_string(),
                machine_ref: active_machine,
                guards,
                history_value: js_sys::Object::new().into(),
                config,
                output: JsValue::UNDEFINED,
                error: err,
                children: children_val,
            };
        }

        if !active_machine.is_started && !is_initialized {
            active_machine.inspector_fn = None;
            let snapshot_actions = Self::build_snapshot_actions(&actions, &action_descriptors);
            active_machine.start(&guards, &snapshot_actions);
        }
        let machine_ref = &active_machine;

        let active_paths = machine_ref.current_state.get_active_paths();
        let value_val = Self::build_snapshot_state_value(machine_ref, &active_paths);
        let ctx_val = match &machine_ref.context {
            Value::Null => js_sys::Object::new().into(),
            other => value_to_js(other),
        };
        let history_obj = self.get_history_value_obj(machine_ref);
        let status = if machine_ref.is_done() {
            "done".to_string()
        } else if is_stopped
            || machine_ref.status() == lavi::MachineStatus::Stopped
        {
            "stopped".to_string()
        } else {
            "active".to_string()
        };
        let mut final_machine = machine_ref.clone();
        if is_stopped
            && !final_machine.is_done()
            && final_machine.status != lavi::MachineStatus::Stopped
        {
            final_machine.stop();
        }

        let output_val = if status == "done" {
            if let Some(c) = cached_out {
                c
            } else {
                let evaluated = Self::compute_output_js(&config, machine_ref, &ctx_val);
                if let Ok(mut inner) = self.inner.try_borrow_mut() {
                    inner.cached_output = Some(evaluated.clone());
                }
                evaluated
            }
        } else {
            JsValue::UNDEFINED
        };

        Snapshot {
            value: value_val,
            context: ctx_val,
            status,
            machine_ref: final_machine,
            guards,
            history_value: history_obj,
            config,
            output: output_val,
            error: JsValue::UNDEFINED,
            children: children_val,
        }
    }

    pub(crate) fn compute_output_js(config: &JsValue, machine_ref: &RuntimeMachine, ctx_val: &JsValue) -> JsValue {
        if config.is_object() {
            let output_key = JsValue::from_str("output");
            let output_prop = js_sys::Reflect::get(config, &output_key).unwrap_or(JsValue::UNDEFINED);
            if !output_prop.is_undefined() {
                if let Ok(func) = output_prop.clone().dyn_into::<js_sys::Function>() {
                    let arg_obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), ctx_val);
                    let done_evt = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(
                        &done_evt,
                        &JsValue::from_str("type"),
                        &JsValue::from_str(&format!("xstate.done.state.{}", machine_ref.definition.id)),
                    );
                    let leaf_out = machine_ref.get_final_leaf_output();
                    let out_js = match leaf_out {
                        Some(val) => value_to_js(&val),
                        None => JsValue::UNDEFINED,
                    };
                    let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("output"), &out_js);
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &done_evt);
                    let self_obj = js_sys::Object::new();
                    let send_fn = js_sys::Function::new_no_args("");
                    let _ = js_sys::Reflect::set(&self_obj, &JsValue::from_str("send"), &send_fn);
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &self_obj);

                    if let Ok(res) = func.call1(&JsValue::NULL, &arg_obj) {
                        return res;
                    } else {
                        return JsValue::UNDEFINED;
                    }
                } else {
                    return output_prop;
                }
            }
        }
        if let Some(out) = machine_ref.get_output() {
            value_to_js(&out)
        } else {
            JsValue::UNDEFINED
        }
    }

    pub(crate) fn check_and_emit_done(&self, self_js: &Option<JsValue>) {
        let (is_done, cached_out, child_id) = {
            if let Ok(inner) = self.inner.try_borrow() {
                (
                    inner.machine.is_done() || inner.machine.status == lavi::MachineStatus::Done,
                    inner.cached_output.clone(),
                    inner.custom_id.clone().unwrap_or_else(|| inner.session_id.clone()),
                )
            } else {
                return;
            }
        };
        if is_done {
            let (parent_opt, sys_opt, is_done_sent) = {
                if let Ok(inner) = self.inner.try_borrow() {
                    (inner.parent.clone(), inner.system.clone(), inner.is_done_event_sent)
                } else {
                    return;
                }
            };
            if !is_done_sent {
                if let Some(ref parent) = parent_opt {
                    if let Ok(mut inner) = self.inner.try_borrow_mut() {
                        inner.is_done_event_sent = true;
                    }
                    let done_evt = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(
                        &done_evt,
                        &JsValue::from_str("type"),
                        &JsValue::from_str(&format!("xstate.done.actor.{}", child_id)),
                    );
                    let _ = js_sys::Reflect::set(
                        &done_evt,
                        &JsValue::from_str("actorId"),
                        &JsValue::from_str(&child_id),
                    );
                    let out_val = if let Some(ref c) = cached_out {
                        c.clone()
                    } else {
                        let (config, machine_ref, ctx_val) = if let Ok(inner) = self.inner.try_borrow() {
                            let ctx_val = match &inner.machine.context {
                                Value::Null => js_sys::Object::new().into(),
                                other => value_to_js(other),
                            };
                            (inner.config.clone(), inner.machine.clone(), ctx_val)
                        } else {
                            (JsValue::UNDEFINED, RuntimeMachine::default(), JsValue::UNDEFINED)
                        };
                        let evaluated = Self::compute_output_js(&config, &machine_ref, &ctx_val);
                        if let Ok(mut inner) = self.inner.try_borrow_mut() {
                            inner.cached_output = Some(evaluated.clone());
                        }
                        evaluated
                    };
                    let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("output"), &out_val);

                    let mut relayed = false;
                    if let Some(ref sys) = sys_opt {
                        if let Ok(relay) = js_sys::Reflect::get(sys, &JsValue::from_str("_relay")) {
                            if let Ok(rf) = relay.dyn_into::<js_sys::Function>() {
                                if let Some(ref s) = self_js {
                                    let _ = rf.call3(sys, s, parent, &done_evt);
                                    relayed = true;
                                }
                            }
                        }
                    }
                    if !relayed {
                        if let Ok(send) = js_sys::Reflect::get(parent, &JsValue::from_str("send")) {
                            if let Ok(f) = send.dyn_into::<js_sys::Function>() {
                                let _ = f.call1(parent, &done_evt);
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn event_to_js(evt: &Value) -> JsValue {
        let evt_js = value_to_js(evt);
        if let Value::Object(map) = evt {
            if let Some(Value::String(t)) = map.get("type") {
                if t.starts_with("xstate.done.state.") && !map.contains_key("output") {
                    let _ = js_sys::Reflect::set(&evt_js, &JsValue::from_str("output"), &JsValue::UNDEFINED);
                }
            }
        }
        evt_js
    }

    pub(crate) fn build_snapshot_state_value(machine: &RuntimeMachine, active_paths: &[String]) -> JsValue {
        if machine.definition.states.is_empty() {
            return js_sys::Object::new().into();
        }
        Self::build_state_value_for_node(machine, "", active_paths)
    }

    fn build_state_value_for_node(machine: &RuntimeMachine, state_path: &str, active_paths: &[String]) -> JsValue {
        let (is_parallel, states) = if state_path.is_empty() {
            (
                matches!(machine.definition.state_type, StateType::Parallel),
                &machine.definition.states,
            )
        } else if let Some(def) = machine.get_state_definition(state_path) {
            (
                matches!(def.state_type, StateType::Parallel),
                &def.states,
            )
        } else {
            return JsValue::NULL;
        };

        if is_parallel {
            let obj = js_sys::Object::new();
            for (r_name, r_def) in states {
                if matches!(r_def.state_type, StateType::History { .. }) || r_def.history.is_some() {
                    continue;
                }
                let child_path = if state_path.is_empty() {
                    r_name.clone()
                } else {
                    format!("{}.{}", state_path, r_name)
                };
                if r_def.states.is_empty() {
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(r_name), &js_sys::Object::new());
                } else {
                    let child_val = Self::build_state_value_for_node(machine, &child_path, active_paths);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(r_name), &child_val);
                }
            }
            obj.into()
        } else {
            let prefix = if state_path.is_empty() {
                "".to_string()
            } else {
                format!("{}.", state_path)
            };
            for path in active_paths {
                let direct_child = if prefix.is_empty() {
                    path.split('.').next().unwrap_or("")
                } else if let Some(rest) = path.strip_prefix(&prefix) {
                    rest.split('.').next().unwrap_or("")
                } else {
                    continue;
                };
                if direct_child.is_empty() {
                    continue;
                }
                if let Some(child_def) = states.get(direct_child) {
                    let child_path = if prefix.is_empty() {
                        direct_child.to_string()
                    } else {
                        format!("{}{}", prefix, direct_child)
                    };
                    if child_def.states.is_empty() {
                        return JsValue::from_str(direct_child);
                    } else {
                        let sub_val = Self::build_state_value_for_node(machine, &child_path, active_paths);
                        let obj = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(direct_child), &sub_val);
                        return obj.into();
                    }
                }
            }
            JsValue::NULL
        }
    }

    pub fn start_internal(&self) {
        let self_js = {
            let inner = self.inner.borrow();
            inner.self_js.clone()
        };

        let _actor_guard = crate::actions::CurrentActorGuard::enter(self_js.as_ref());

        let (mut machine, guards, actions, is_started, is_initialized, has_init_err) = {
            let mut inner = self.inner.borrow_mut();
            if inner.initialization_error.is_some() || inner.machine.is_started {
                return;
            }
            let cloned_machine = inner.machine.clone();
            inner.is_in_tick = true;
            inner.machine.is_started = true;
            (
                cloned_machine,
                inner.guards.clone(),
                inner.actions.clone(),
                false,
                inner.is_initialized,
                inner.initialization_error.is_some(),
            )
        };

        if has_init_err || is_started {
            return;
        }

        let has_invalid_init_action = machine.definition.entry.iter().any(|a| matches!(a, lavi::Action::RaiseInvalid(_)));
        if !has_invalid_init_action {
            let init_evt_for_inv = Value::Object(lavi::indexmap! { "type".to_string() => Value::String("xstate.init".to_string()) });
            let current_ctx = machine.context.clone();
            spawn_invokes_for_path_detached(&self.inner, "", &init_evt_for_inv, &current_ctx, true);
        }

        if !is_initialized {
            machine.start(&guards, &actions);
        } else {
            machine.is_started = true;
            if machine.status != lavi::MachineStatus::Done && !machine.is_done() {
                machine.revive_timers();
                let mut loop_count = 0;
                while loop_count < 100 {
                    let res = machine.transition("", &Value::Null, &guards, &actions);
                    if res.is_none() { break; }
                    loop_count += 1;
                }
            }
        }

        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.machine.context = machine.context.clone();
            inner.machine.current_state = machine.current_state.clone();
            inner.machine.status = machine.status;
        }

        self.drain_reentrant(&mut machine, &guards, &actions);

        let last_err = {
            let mut inner = self.inner.borrow_mut();
            if machine.inspector_fn.is_none() {
                machine.inspector_fn = inner.machine.inspector_fn.clone();
            }
            let err = machine.last_error.take();
            if !machine.is_done() && machine.status != lavi::MachineStatus::Done {
                inner.cached_output = None;
                inner.is_completed_emitted = false;
            }
            inner.machine = machine;
            inner.is_initialized = true;
            inner.is_in_tick = false;

            err
        };

        if let Some(err_msg) = last_err {
            let js_err = js_sys::Error::new(&err_msg);
            self.emit_error(&js_err.into());
        }

        flush_pending_starts_detached(&self.inner);
        self.check_and_emit_done(&self_js);

        // Emit initial snapshot
        self.emit_snapshot();

        // Schedule host timer for any active deadline
        Actor::reschedule_host_timer(&self.inner);
    }

    #[wasm_bindgen(unchecked_return_type = "Actor")]
    pub fn start(&self) -> JsValue {
        self.start_internal();
        if let Some(ref s) = self.inner.borrow().self_js {
            s.clone()
        } else {
            self.clone().into()
        }
    }

    pub fn start_native(&self) -> Actor {
        self.start_internal();
        self.clone()
    }

    fn emit_snapshot(&self) {
        let snapshot = self.get_snapshot();
        let status_str = snapshot.status();
        let should_complete = status_str == "done";
        let is_completed = if should_complete {
            let mut inner = self.inner.borrow_mut();
            if !inner.is_completed_emitted {
                inner.is_completed_emitted = true;
                true
            } else {
                false
            }
        } else {
            false
        };
        let obj: JsValue = snapshot.into();
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("status"), &JsValue::from_str(&status_str));

        let subscribers = {
            let inner = self.inner.borrow();
            inner.subscribers.clone()
        };

        for (_, sub) in &subscribers {
            match sub {
                Subscriber::Fn(func) => {
                    let _ = func.call1(&JsValue::NULL, &obj);
                }
                Subscriber::Observer { next, complete, .. } => {
                    if let Some(next_fn) = next {
                        let _ = next_fn.call1(&JsValue::NULL, &obj);
                    }
                    if is_completed {
                        if let Some(complete_fn) = complete {
                            let _ = complete_fn.call0(&JsValue::NULL);
                        }
                    }
                }
            }
        }
    }

    pub fn send(&self, event: JsValue) -> Result<JsValue, JsValue> {
        let self_js = {
            let inner = self.inner.borrow();
            inner.self_js.clone()
        };

        let _actor_guard = crate::actions::CurrentActorGuard::enter(self_js.as_ref());

        let (event_name, event_payload) = if event.is_string() {
            let s = event.as_string().unwrap();
            let payload = Value::Object(lavi::indexmap! { "type".to_string() => Value::String(s.clone()) });
            (s, payload)
        } else {
             // Assume object
             let payload = js_to_value(&event);
             let name = payload.as_object().and_then(|m| m.get("type")).and_then(|t| t.as_string()).map(|s| s.as_str()).unwrap_or("UNKNOWN").to_string();
             (name, payload)
        };

        let step_state = {
            let mut inner = self.inner.borrow_mut();
            let is_stopped = inner.is_stopped || inner.machine.status == lavi::MachineStatus::Stopped;
            let is_done = inner.machine.is_done();
            if is_stopped || is_done {
                return Ok(JsValue::UNDEFINED);
            }
            if inner.is_in_tick {
                let is_relayed = IN_SYSTEM_RELAY.with(|c| c.get());
                let is_child_done = event_name.starts_with("xstate.done.actor.");
                let is_child_error = event_name.starts_with("xstate.error.actor.");
                // If not yet initialized (during start_internal), only internal relayed/child-lifecycle
                // events are accepted as reentrant. Direct external actor.send() during initial entry
                // is strictly dropped per Level 1 zero pre-start buffering invariant.
                if !inner.is_initialized && !is_relayed && !is_child_done && !is_child_error {
                    return Ok(JsValue::UNDEFINED);
                }
                let sid = inner.session_id.clone();
                RELAYED_QUEUE.with(|q| q.borrow_mut().entry(sid).or_default().push_back(is_relayed));
                inner.machine.external_queue.push_back((event_name, event_payload));
                return Ok(JsValue::UNDEFINED);
            }
            if !inner.machine.is_started {
                return Ok(JsValue::UNDEFINED);
            }
            inner.is_in_tick = true;
            (inner.machine.clone(), inner.guards.clone(), inner.actions.clone())
        };

        let (mut machine, guards, actions) = step_state;

        let _ = machine.transition(&event_name, &event_payload, &guards, &actions);

        if let Ok(mut inner) = self.inner.try_borrow_mut() {
            inner.machine.context = machine.context.clone();
            inner.machine.current_state = machine.current_state.clone();
            inner.machine.status = machine.status;
        }

        self.drain_reentrant(&mut machine, &guards, &actions);

        let (last_err, exited, entered, current_ctx) = {
            let mut inner = self.inner.borrow_mut();
            if machine.inspector_fn.is_none() {
                machine.inspector_fn = inner.machine.inspector_fn.clone();
            }
            let err = machine.last_error.take();
            if !machine.is_done() && machine.status != lavi::MachineStatus::Done {
                inner.cached_output = None;
                inner.is_completed_emitted = false;
            }
            inner.machine = machine;
            inner.is_in_tick = false;

            let exited = inner.machine.last_exited_paths.clone();
            let entered = inner.machine.last_entered_paths.clone();
            let current_ctx = inner.machine.context.clone();

            (err, exited, entered, current_ctx)
        };

        collect_and_stop_invokes(&self.inner, &exited);
        for p in entered {
            spawn_invokes_for_path_detached(&self.inner, &p, &event_payload, &current_ctx, false);
        }

        self.check_and_emit_done(&self_js);

        if let Some(err_msg) = last_err {
            let js_err = js_sys::Error::new(&err_msg);
            self.emit_error(&js_err.into());
        }

        // 2. Inspect Snapshot
        self.emit_snapshot();

        // Host timer scheduling
        Actor::reschedule_host_timer(&self.inner);

        Ok(JsValue::UNDEFINED)
    }

    #[wasm_bindgen(js_name = subscribe)]
    pub fn subscribe_js(&self, observer_or_fn: JsValue) -> Result<JsValue, JsValue> {
        self.subscribe_internal(observer_or_fn)
    }

    fn subscribe_internal(&self, observer_or_fn: JsValue) -> Result<JsValue, JsValue> {
        let sub = if observer_or_fn.is_function() {
            Subscriber::Fn(observer_or_fn.unchecked_into())
        } else if observer_or_fn.is_object() && !observer_or_fn.is_null() {
            let obj = js_sys::Object::from(observer_or_fn);
            let next_fn = js_sys::Reflect::get(&obj, &JsValue::from_str("next"))
                .ok()
                .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
            let complete_fn = js_sys::Reflect::get(&obj, &JsValue::from_str("complete"))
                .ok()
                .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
            let error_fn = js_sys::Reflect::get(&obj, &JsValue::from_str("error"))
                .ok()
                .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
            Subscriber::Observer {
                next: next_fn,
                complete: complete_fn,
                error: error_fn,
            }
        } else {
            return Err(JsValue::from_str("subscribe requires a function or observer"));
        };

        static NEXT_SUB_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);
        let sub_id = NEXT_SUB_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

        let is_done = {
            let inner = self.inner.borrow();
            inner.machine.is_done() || inner.machine.status == lavi::MachineStatus::Done
        };

        if is_done {
            if let Subscriber::Observer { complete: Some(ref comp), .. } = sub {
                let _ = comp.call0(&JsValue::NULL);
            }
        }

        self.inner.borrow_mut().subscribers.push((sub_id, sub));

        let inner_weak = Rc::downgrade(&self.inner);
        let unsub_closure = Closure::wrap(Box::new(move || {
            if let Some(inner_rc) = inner_weak.upgrade() {
                if let Ok(mut inner) = inner_rc.try_borrow_mut() {
                    inner.subscribers.retain(|(id, _)| *id != sub_id);
                }
            }
        }) as Box<dyn FnMut()>);

        let unsub_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &unsub_obj,
            &JsValue::from_str("unsubscribe"),
            &unsub_closure.into_js_value(),
        );

        Ok(unsub_obj.into())
    }

    #[wasm_bindgen(unchecked_return_type = "Actor")]
    pub fn stop(&self) -> JsValue {
        let children_to_stop: Vec<JsValue> = {
            let mut inner = self.inner.borrow_mut();
            let kids: Vec<JsValue> = inner.children.values().cloned().collect();
            inner.children.clear();
            inner.child_sources.clear();
            kids
        };
        for child_actor in children_to_stop {
            if let Ok(stop_fn) = js_sys::Reflect::get(&child_actor, &JsValue::from_str("stop")) {
                if let Ok(f) = stop_fn.dyn_into::<js_sys::Function>() {
                    let _ = f.call0(&child_actor);
                }
            }
        }

        let mut inner = self.inner.borrow_mut();
        if let Some((_, clear_timeout)) = resolve_host_clock(inner.host_clock.as_ref()) {
            if let Some(active) = inner.active_timeout.take() {
                let _ = clear_timeout.call1(&JsValue::NULL, &active.handle);
            }
            for (_, handle) in inner.send_timer_handles.drain(..) {
                let _ = clear_timeout.call1(&JsValue::NULL, &handle);
            }
        } else {
            inner.send_timer_handles.clear();
        }
        unregister_live_actor_ref(&inner.session_id);
        if let Some(ref cid) = inner.custom_id {
            unregister_live_actor_ref(cid);
        }
        if inner.machine.status == lavi::MachineStatus::Done || inner.machine.is_done() {
            if let Some(ref s) = inner.self_js {
                return s.clone();
            }
            return self.clone().into();
        }
        inner.is_stopped = true;
        inner.machine.stop();
        if let Some(ref s) = inner.self_js {
            s.clone()
        } else {
            self.clone().into()
        }
    }

    #[wasm_bindgen(js_name = __resumeAfterCleanup)]
    pub fn resume_after_cleanup(&self) {
        {
            let mut inner = self.inner.borrow_mut();
            if !inner.is_initialized
                || inner.machine.is_done()
                || inner.machine.status == lavi::MachineStatus::Done
            {
                return;
            }
            inner.is_stopped = false;
            inner.machine.is_started = true;
            inner.machine.status = lavi::MachineStatus::Active;
            inner.machine.revive_timers();
        }
        Actor::reschedule_host_timer(&self.inner);
    }

    #[wasm_bindgen(js_name = __syncImplementations)]
    pub fn sync_implementations(&self, machine_or_logic: JsValue, options: JsValue) {
        let mut inner = self.inner.borrow_mut();
        let ActorInner {
            ref mut guards,
            ref guards_holder,
            ref mut actions,
            ref mut action_descriptors,
            ref mut machine,
            ref logger_holder,
            ..
        } = *inner;
        if let Some(state_node) = try_clone_state_node(&machine_or_logic) {
            guards.extend(state_node.guards);
            actions.extend(state_node.actions);
            action_descriptors.extend(state_node.action_descriptors);
            machine
                .actions_with_effects
                .extend(state_node.machine.actions_with_effects);
            machine.delays.extend(state_node.machine.delays);
        }
        if options.is_object() && !options.is_null() {
            parse_implementations(
                &options,
                guards,
                actions,
                &mut machine.actions_with_effects,
                action_descriptors,
                logger_holder,
            );
            parse_and_register_delays(&options, machine);
        }
        guards_holder.borrow_mut().extend(guards.clone());
    }

    fn build_snapshot_actions(
        actions: &IndexMap<String, lavi::ActionFunction>,
        action_descriptors: &IndexMap<String, JsValue>,
    ) -> IndexMap<String, lavi::ActionFunction> {
        let mut snapshot_actions: IndexMap<String, lavi::ActionFunction> = IndexMap::new();
        snapshot_actions.insert(
            "__lavi_snapshot_mode__".to_string(),
            Rc::new(|_, _, _| {}),
        );
        for (k, v) in actions {
            let is_assign = k.starts_with("generated_assign_")
                || k.starts_with("auto_action_assign_")
                || action_descriptors
                    .get(k)
                    .and_then(|d| js_sys::Reflect::get(d, &JsValue::from_str("actionType")).ok())
                    .and_then(|t| t.as_string())
                    .as_deref()
                    == Some("xstate.assign");
            if is_assign {
                snapshot_actions.insert(k.clone(), v.clone());
            }
        }
        for (k, d) in action_descriptors {
            let is_enqueue = js_sys::Reflect::get(d, &JsValue::from_str("actionType"))
                .ok()
                .and_then(|t| t.as_string())
                .as_deref()
                == Some("xstate.enqueueActions");
            if is_enqueue {
                snapshot_actions.insert(
                    format!("__lavi_enqueue_{}", k),
                    Rc::new(|_, _, _| {}),
                );
            }
        }
        snapshot_actions
    }

    #[wasm_bindgen(js_name = getPersistedSnapshot)]
    pub fn get_persisted_snapshot(&self) -> Result<JsValue, JsValue> {
        let inner = self.inner.borrow();
        let active_machine = &inner.machine;
        let is_stopped = inner.is_stopped;
        let is_initialized = inner.is_initialized;
        let preview;
        let machine_ref = if !active_machine.is_started && !is_initialized {
            let mut m = active_machine.clone();
            m.inspector_fn = None;
            let snapshot_actions = Self::build_snapshot_actions(&inner.actions, &inner.action_descriptors);
            m.start(&inner.guards, &snapshot_actions);
            preview = m;
            &preview
        } else if is_stopped
            && !active_machine.is_done()
            && active_machine.status != lavi::MachineStatus::Done
            && active_machine.status != lavi::MachineStatus::Stopped
        {
            let mut m = active_machine.clone();
            m.stop();
            preview = m;
            &preview
        } else {
            active_machine
        };

        let json_str = machine_ref.get_persisted_snapshot_json();
        js_sys::JSON::parse(&json_str)
    }

    #[wasm_bindgen(js_name = getNextDeadline)]
    pub fn get_next_deadline(&self) -> Option<f64> {
        let inner = self.inner.borrow();
        if inner.is_stopped {
            return None;
        }
        if inner.machine.is_done() || inner.machine.status == lavi::MachineStatus::Done {
            return None;
        }
        inner.machine.next_timer_deadline().map(|d| d as f64)
    }

    #[wasm_bindgen]
    pub fn tick(&self) -> Result<(), JsValue> {
        let (mut machine, guards, actions, tick_override) = {
            let inner = self.inner.borrow();
            if !inner.machine.is_started
                || inner.is_stopped
                || inner.machine.status == lavi::MachineStatus::Stopped
                || inner.machine.status == lavi::MachineStatus::Done
                || inner.machine.is_done()
            {
                return Ok(());
            }
            (
                inner.machine.clone(),
                inner.guards.clone(),
                inner.actions.clone(),
                inner.tick_override.clone(),
            )
        };

        let target_ms = if let Some(override_now) = tick_override.get() {
            override_now
        } else if let Some(clock_fn) = &machine.clock_millis_fn {
            clock_fn()
        } else {
            js_sys::Date::now() as u64
        };
        let mut transitioned = false;
        let mut last_deadline: Option<u64> = None;
        let mut same_deadline_steps = 0usize;
        let mut total_steps = 0usize;

        while let Some(deadline) = machine.next_timer_deadline() {
            if deadline > target_ms {
                break;
            }
            if last_deadline == Some(deadline) {
                same_deadline_steps += 1;
                if same_deadline_steps >= 100 {
                    break;
                }
            } else {
                last_deadline = Some(deadline);
                same_deadline_steps = 0;
            }
            total_steps += 1;
            if total_steps > 1_000_000 {
                break;
            }

            tick_override.set(Some(deadline));
            self.inner.borrow_mut().is_in_tick = true;
            let mut did_transition = machine.tick(&guards, &actions).is_some();
            if self.drain_reentrant(&mut machine, &guards, &actions) {
                did_transition = true;
            }
            self.inner.borrow_mut().is_in_tick = false;
            tick_override.set(None);

            if did_transition {
                transitioned = true;
            }
        }
        tick_override.set(None);
        {
            let mut inner = self.inner.borrow_mut();
            if machine.inspector_fn.is_none() {
                machine.inspector_fn = inner.machine.inspector_fn.clone();
            }
            if !machine.is_done() && machine.status != lavi::MachineStatus::Done {
                inner.cached_output = None;
                inner.is_completed_emitted = false;
            }
            inner.machine = machine;
            inner.is_in_tick = false;
        }
        if transitioned {
            self.emit_snapshot();
        }
        Actor::reschedule_host_timer(&self.inner);
        Ok(())
    }

    pub(crate) fn reschedule_host_timer(inner_rc: &Rc<RefCell<ActorInner>>) {
        let mut inner = inner_rc.borrow_mut();

        if inner.is_simulated_clock {
            return;
        }

        let Some((set_timeout, clear_timeout)) = resolve_host_clock(inner.host_clock.as_ref()) else {
            return;
        };

        // Cancel existing timer handle if any
        if let Some(active) = inner.active_timeout.take() {
            let _ = clear_timeout.call1(&JsValue::NULL, &active.handle);
        }

        // If stopped or done, do not schedule
        if inner.is_stopped
            || inner.machine.status == lavi::MachineStatus::Stopped
            || inner.machine.status == lavi::MachineStatus::Done
            || inner.machine.is_done()
        {
            return;
        }

        // Check if there is an active deadline
        let Some(deadline) = inner.machine.next_timer_deadline() else {
            return;
        };

        let now = inner
            .machine
            .clock_millis_fn
            .as_ref()
            .map(|c| c())
            .unwrap_or_else(|| js_sys::Date::now() as u64);

        let delay_ms = deadline.saturating_sub(now) as f64;

        let inner_clone = inner_rc.clone();
        let func = Closure::once_into_js(move || {
            Actor::on_host_timeout(&inner_clone);
        });
        let delay_val = JsValue::from_f64(delay_ms);

        if let Ok(handle) = set_timeout.call2(&JsValue::NULL, &func, &delay_val) {
            inner.active_timeout = Some(ActiveTimeout {
                handle,
            });
        }
    }

    fn on_host_timeout(inner_rc: &Rc<RefCell<ActorInner>>) {
        // Step 1: Pre-clear active_timeout handle so exiting source state doesn't cancel it
        {
            let mut inner = inner_rc.borrow_mut();
            inner.active_timeout = None;
            if inner.is_stopped
                || inner.machine.status == lavi::MachineStatus::Stopped
                || inner.machine.status == lavi::MachineStatus::Done
                || inner.machine.is_done()
            {
                return;
            }
            if let Some(dl) = inner.machine.next_timer_deadline() {
                inner.tick_override.set(Some(dl));
            }
        }

        // Step 2: Tick via Actor wrapper around inner_rc
        let actor = Actor {
            inner: inner_rc.clone(),
        };
        let _ = actor.tick();

        let last_err = inner_rc.borrow_mut().machine.last_error.take();
        if let Some(err_msg) = last_err {
            let js_err = js_sys::Error::new(&err_msg);
            actor.emit_error(&js_err.into());
        }

        // Step 3: Reschedule for next deadline if any
        Actor::reschedule_host_timer(inner_rc);
    }
}

impl Actor {
    pub fn subscribe<T: Into<JsValue>>(&self, observer_or_fn: T) -> JsValue {
        self.subscribe_internal(observer_or_fn.into())
            .unwrap_or(JsValue::UNDEFINED)
    }
}

pub(crate) fn is_valid_state_value_js(val: &JsValue) -> bool {
    if val.is_string() {
        return true;
    }
    if val.is_object() && !val.is_null() && !js_sys::Array::is_array(val) && !val.is_function() {
        let obj = js_sys::Object::from(val.clone());
        let keys = js_sys::Object::keys(&obj);
        for i in 0..keys.length() {
            let k = keys.get(i);
            let child = js_sys::Reflect::get(&obj, &k).unwrap_or(JsValue::UNDEFINED);
            if !is_valid_state_value_js(&child) {
                return false;
            }
        }
        return true;
    }
    false
}

#[wasm_bindgen]
pub struct Snapshot {
    pub(crate) value: JsValue,
    pub(crate) context: JsValue,
    pub(crate) status: String,
    pub(crate) machine_ref: RuntimeMachine, // Cloned for now
    pub(crate) guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    pub(crate) history_value: JsValue,
    #[allow(dead_code)]
    pub(crate) config: JsValue,
    pub(crate) output: JsValue,
    pub(crate) error: JsValue,
    pub(crate) children: JsValue,
}

#[wasm_bindgen]
impl Snapshot {
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> JsValue {
        self.value.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn context(&self) -> JsValue {
        self.context.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn children(&self) -> JsValue {
        if self.children.is_undefined() {
            js_sys::Object::new().into()
        } else {
            self.children.clone()
        }
    }

    #[wasm_bindgen(getter)]
    pub fn status(&self) -> String {
        self.status.clone()
    }

    #[wasm_bindgen(getter, js_name = historyValue)]
    pub fn history_value(&self) -> JsValue {
        self.history_value.clone()
    }

    #[wasm_bindgen]
    pub fn matches(&self, test_value: JsValue) -> bool {
        if self.status == "error" {
            return false;
        }
        if let Some(path) = test_value.as_string() {
            self.machine_ref.current_state.matches(&path)
        } else if is_valid_state_value_js(&test_value) {
            let val = js_to_value(&test_value);
            let partial = lavi::value_to_state_value(val);
            self.machine_ref.current_state.matches_state_value(&partial)
        } else {
            false
        }
    }

    #[wasm_bindgen]
    pub fn can(&self, event: JsValue) -> bool {
        if self.status == "error" {
            return false;
        }
        let (event_name, event_payload) = if event.is_string() {
            let s = event.as_string().unwrap();
            let payload = Value::Object(lavi::indexmap! {
                "type".to_string() => Value::String(s.clone()),
            });
            (s, payload)
        } else {
            let payload = js_to_value(&event);
            let name = payload
                .as_object()
                .and_then(|m| m.get("type"))
                .and_then(|t| t.as_string())
                .unwrap_or(&"UNKNOWN".to_string())
                .clone();
            (name, payload)
        };
        self.machine_ref
            .can_with_guards(&event_name, &event_payload, &self.guards)
    }
    #[wasm_bindgen(js_name = hasTag)]
    pub fn has_tag(&self, tag: &str) -> bool {
        if self.status == "error" {
            return false;
        }
        self.machine_ref.has_tag(tag)
    }

    #[wasm_bindgen(getter)]
    pub fn tags(&self) -> js_sys::Set {
        if self.status == "error" {
            return js_sys::Set::new(&JsValue::UNDEFINED);
        }
        let set = js_sys::Set::new(&JsValue::UNDEFINED);
        for tag in self.machine_ref.get_tags() {
            set.add(&JsValue::from_str(&tag));
        }
        set
    }

    #[wasm_bindgen(js_name = getMeta)]
    pub fn get_meta(&self) -> JsValue {
        let meta_val = self.machine_ref.get_meta();
        value_to_js(&meta_val)
    }

    #[wasm_bindgen(getter)]
    pub fn error(&self) -> JsValue {
        self.error.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn output(&self) -> JsValue {
        self.output.clone()
    }

    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json(&self) -> JsValue {
        let json_str = self.machine_ref.get_persisted_snapshot_json();
        let val = js_sys::JSON::parse(&json_str).unwrap_or(JsValue::NULL);
        if val.is_object() && !val.is_null() {
            let _ = js_sys::Reflect::set(&val, &JsValue::from_str("error"), &self.error);
            let _ = js_sys::Reflect::set(&val, &JsValue::from_str("output"), &self.output);
        }
        val
    }
}

pub(crate) fn evaluate_functional_input(
    raw_input: &JsValue,
    runner_js: &JsValue,
    inner_rc: &Rc<RefCell<ActorInner>>,
) -> JsValue {
    if raw_input.is_function() {
        if let Ok(func) = raw_input.clone().dyn_into::<js_sys::Function>() {
            let arg_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), runner_js);
            match func.call1(&JsValue::NULL, &arg_obj) {
                Ok(res) => res,
                Err(err) => {
                    if let Ok(mut inner) = inner_rc.try_borrow_mut() {
                        inner.initialization_error = Some(err);
                    }
                    JsValue::UNDEFINED
                }
            }
        } else {
            raw_input.clone()
        }
    } else {
        raw_input.clone()
    }
}

pub(crate) fn evaluate_functional_context(
    config: &JsValue,
    resolved_input: &JsValue,
    runner_js: &JsValue,
    inner_rc: &Rc<RefCell<ActorInner>>,
) {
    if !config.is_object() || config.is_null() {
        return;
    }
    let context_key = JsValue::from_str("context");
    let context_val = js_sys::Reflect::get(config, &context_key).unwrap_or(JsValue::UNDEFINED);
    if context_val.is_function() {
        if let Ok(func) = context_val.dyn_into::<js_sys::Function>() {
            let arg_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("input"), resolved_input);
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), runner_js);
            if let Ok(sys) = js_sys::Reflect::get(runner_js, &JsValue::from_str("system")) {
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("system"), &sys);
            }
            let runner_for_spawn = runner_js.clone();
            let spawn_fn = Closure::wrap(Box::new(move |src: JsValue, options: JsValue| -> JsValue {
                let opts = if options.is_undefined() {
                    js_sys::Object::new().into()
                } else {
                    options
                };
                crate::actions::spawn_child_from_actor(&runner_for_spawn, &src, &opts)
            }) as Box<dyn Fn(JsValue, JsValue) -> JsValue>);
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("spawn"), spawn_fn.as_ref());

            let call_res = func.call1(&JsValue::NULL, &arg_obj);
            drop(spawn_fn);

            match call_res {
                Ok(res) => {
                    if let Ok(mut inner) = inner_rc.try_borrow_mut() {
                        inner.machine.context = js_to_value(&res);
                    }
                }
                Err(err) => {
                    if let Ok(mut inner) = inner_rc.try_borrow_mut() {
                        inner.initialization_error = Some(err);
                    }
                }
            }
        }
    }
}

#[wasm_bindgen(js_name = createActor, unchecked_return_type = "Actor")]
pub fn create_actor(machine_def: JsValue, options: Option<JsValue>) -> Result<JsValue, JsValue> {
    let options = options.unwrap_or(JsValue::UNDEFINED);

    // 1. Check for createActor factory method (StateNode or deferred actor logic)
    if machine_def.is_object() {
         let spawn_fn = js_sys::Reflect::get(&machine_def, &JsValue::from_str("createActor")).unwrap_or(JsValue::UNDEFINED);
         if let Ok(func) = spawn_fn.dyn_into::<js_sys::Function>() {
              // Call createActor(options)
              let runner_val = func.call1(&machine_def, &options)?;
              return Ok(runner_val);
         }
    }

    // 2. Treat as Machine Config Object (POJO)
    let config = machine_def.clone();
    let runner = Actor::new(config.clone(), options.clone())?;
    let runner_js: JsValue = runner.clone().into();
    let parent_opt = if options.is_object() && !options.is_null() {
        let p = js_sys::Reflect::get(&options, &JsValue::from_str("parent")).unwrap_or(JsValue::UNDEFINED);
        if !p.is_undefined() && !p.is_null() {
            Some(p)
        } else {
            None
        }
    } else {
        None
    };
    let system_js = if options.is_object() && !options.is_null() {
        let sys = js_sys::Reflect::get(&options, &JsValue::from_str("system")).unwrap_or(JsValue::UNDEFINED);
        if sys.is_object() && !sys.is_null() {
            sys
        } else {
            let inspect_val = js_sys::Reflect::get(&options, &JsValue::from_str("inspect")).unwrap_or(JsValue::UNDEFINED);
            if !inspect_val.is_undefined() && !inspect_val.is_null() {
                create_system_js(Some(inspect_val))
            } else {
                create_system_js(None)
            }
        }
    } else {
        create_system_js(None)
    };

    let impl_obj = js_sys::Object::new();
    if options.is_object() && !options.is_null() {
        let opts_obj = js_sys::Object::from(options.clone());
        let keys = js_sys::Object::keys(&opts_obj);
        for i in 0..keys.length() {
            let k = keys.get(i);
            if let Ok(v) = js_sys::Reflect::get(&opts_obj, &k) {
                let _ = js_sys::Reflect::set(&impl_obj, &k, &v);
            }
        }
    }

    let session_id = next_session_id();
    let custom_id = if options.is_object() && !options.is_null() {
        js_sys::Reflect::get(&options, &JsValue::from_str("id"))
            .ok()
            .and_then(|v| v.as_string())
    } else {
        None
    };

    // 1. Populate identity and references on inner before invoking any user closures
    {
        let mut inner = runner.inner.borrow_mut();
        inner.self_js = Some(runner_js.clone());
        inner.parent = parent_opt.clone();
        inner.system = Some(system_js.clone());
        inner.implementations = impl_obj.into();
        inner.session_id = session_id;
        inner.custom_id = custom_id;
    }

    let raw_input = if options.is_object() && !options.is_null() {
        js_sys::Reflect::get(&options, &JsValue::from_str("input")).unwrap_or(JsValue::UNDEFINED)
    } else {
        JsValue::UNDEFINED
    };

    // 2. Evaluate functional input with { self: runner_js }
    let resolved_input = evaluate_functional_input(&raw_input, &runner_js, &runner.inner);
    if !resolved_input.is_undefined() {
        let mut inner = runner.inner.borrow_mut();
        inner.input_js = Some(resolved_input.clone());
        inner.machine.input = Some(js_to_value(&resolved_input));
    }

    let _ = js_sys::Reflect::set(&runner_js, &JsValue::from_str("system"), &system_js);
    let _ = js_sys::Reflect::set(&runner_js, &JsValue::from_str("parent"), &parent_opt.clone().unwrap_or(JsValue::UNDEFINED));
    let _ = js_sys::Reflect::set(&runner_js, &JsValue::from_str("_parent"), &parent_opt.unwrap_or(JsValue::UNDEFINED));
    let _ = js_sys::Reflect::set(&runner_js, &JsValue::from_str("_implementations"), &runner.inner.borrow().implementations);
    let _ = js_sys::Reflect::set(&runner_js, &JsValue::from_str("config"), &config);

    let inner_rc = runner.inner.clone();
    let reg_child_fn = Closure::wrap(Box::new(move |id: JsValue, child_actor: JsValue| {
        if let Some(id_str) = id.as_string() {
            inner_rc.borrow_mut().children.insert(id_str, child_actor);
        }
    }) as Box<dyn Fn(JsValue, JsValue)>);
    let _ = js_sys::Reflect::set(&runner_js, &JsValue::from_str("_registerChild"), reg_child_fn.as_ref());
    reg_child_fn.forget();

    if let Ok(reg) = js_sys::Reflect::get(&system_js, &JsValue::from_str("_register")) {
        if let Ok(f) = reg.dyn_into::<js_sys::Function>() {
            let _ = f.call1(&system_js, &runner_js);
        }
    }

    // 3. Evaluate functional context with { input: resolved_input, self: runner_js, spawn }
    if runner.inner.borrow().initialization_error.is_none() {
        evaluate_functional_context(&config, &resolved_input, &runner_js, &runner.inner);
    }

    // 4. Apply options.context override if specified
    if options.is_object() {
        let context_opt = js_sys::Reflect::get(&options, &JsValue::from_str("context")).unwrap_or(JsValue::UNDEFINED);
        if !context_opt.is_undefined() {
            runner.inner.borrow_mut().machine.context = js_to_value(&context_opt);
        }
    }

    check_initial_entry_guards(&runner.inner.borrow());

    let insp_fn = runner.build_machine_inspector();
    runner.inner.borrow_mut().machine.register_inspector(insp_fn);

    Ok(runner_js)
}

pub(crate) fn get_state_def_mut<'a>(
    mut states: &'a mut IndexMap<String, lavi::StateDefinition>,
    path: &str,
) -> Option<&'a mut lavi::StateDefinition> {
    let mut parts = path.split('.').peekable();
    while let Some(part) = parts.next() {
        let def = states.get_mut(part)?;
        if parts.peek().is_none() {
            return Some(def);
        }
        states = &mut def.states;
    }
    None
}

pub(crate) fn normalize_undefined(val: &JsValue, depth: usize) -> JsValue {
    if depth > 100 { return val.clone(); }
    if val.is_object() && !val.is_null() {
        if js_sys::Array::is_array(val) {
            let arr = js_sys::Array::from(val);
            let res_arr = js_sys::Array::new();
            for i in 0..arr.length() {
                let item = arr.get(i);
                res_arr.push(&normalize_undefined(&item, depth + 1));
            }
            return res_arr.into();
        }
        if val.is_function() { return val.clone(); }
        let obj = js_sys::Object::from(val.clone());
        let keys = js_sys::Object::keys(&obj);
        let res_obj = js_sys::Object::new();
        for i in 0..keys.length() {
            let key = keys.get(i);
            let v = js_sys::Reflect::get(&obj, &key).unwrap_or(JsValue::UNDEFINED);
            if v.is_undefined() {
                // Convert undefined to null to preserve keys in JSON/Serde
                let _ = js_sys::Reflect::set(&res_obj, &key, &JsValue::NULL);
            } else {
                let _ = js_sys::Reflect::set(&res_obj, &key, &normalize_undefined(&v, depth + 1));
            }
        }
        return res_obj.into();
    }
    val.clone()
}

pub(crate) fn collect_output_resolvers(val: &JsValue, path: &str, resolvers: &mut IndexMap<String, js_sys::Function>) {
    if !val.is_object() || val.is_null() {
        return;
    }
    let obj = js_sys::Object::from(val.clone());
    let output_key = JsValue::from_str("output");
    let out_val = js_sys::Reflect::get(&obj, &output_key).unwrap_or(JsValue::UNDEFINED);
    if !path.is_empty() {
        if let Ok(func) = out_val.dyn_into::<js_sys::Function>() {
            resolvers.insert(path.to_string(), func);
        }
    }
    let states_key = JsValue::from_str("states");
    let states_val = js_sys::Reflect::get(&obj, &states_key).unwrap_or(JsValue::UNDEFINED);
    if states_val.is_object() && !states_val.is_null() {
        let states_obj = js_sys::Object::from(states_val);
        let keys = js_sys::Object::keys(&states_obj);
        for i in 0..keys.length() {
            let k = keys.get(i);
            let k_str = k.as_string().unwrap_or_default();
            let child_val = js_sys::Reflect::get(&states_obj, &k).unwrap_or(JsValue::UNDEFINED);
            let child_path = if path.is_empty() { k_str } else { format!("{}.{}", path, k_str) };
            collect_output_resolvers(&child_val, &child_path, resolvers);
        }
    }
}
