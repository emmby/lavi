use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use lavi::{IndexMap, RuntimeMachine, Value};

use crate::actor::{Actor, Subscriber};
use crate::invoke::{
    collect_and_stop_invokes, flush_pending_starts_detached, spawn_invokes_for_path_detached,
};
use crate::state_node::compute_history_value_obj;
use crate::system::IN_SYSTEM_RELAY;
use crate::value_bridge::value_to_js;

pub fn emit_to_inspector(sub: &Subscriber, ev_js: &JsValue) {
    match sub {
        Subscriber::Fn(func) => {
            let _ = func.call1(&JsValue::NULL, ev_js);
        }
        Subscriber::Observer {
            next: Some(next_fn),
            ..
        } => {
            let _ = next_fn.call1(&JsValue::NULL, ev_js);
        }
        _ => {}
    }
}

impl Actor {
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
                        Value::Object(map) => {
                            map.get("type").and_then(|v| match v {
                                Value::String(s) => Some(s.as_str()),
                                _ => None,
                            }) == Some("xstate.init")
                        }
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
                        lavi::Action::Named(name)
                        | lavi::Action::Generic(lavi::GenericAction {
                            action_type: name,
                            ..
                        }) => {
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
                                lavi::Action::Named(n)
                                | lavi::Action::Generic(lavi::GenericAction { action_type: n, .. }) => {
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
                                    let _ = js_sys::Reflect::set(
                                        &done_evt,
                                        &JsValue::from_str("type"),
                                        &JsValue::from_str(&format!("xstate.done.actor.{}", child_id)),
                                    );
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
                emit_to_inspector(insp_sub, &ev_js);
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

    pub fn setup_inspector(&self) {
        let insp_fn = self.build_machine_inspector();
        self.inner.borrow_mut().machine.register_inspector(insp_fn);
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
            emit_to_inspector(&sub, &ev_js);
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

    pub(crate) fn emit_snapshot(&self) {
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

    pub(crate) fn emit_error(&self, err_val: &JsValue) {
        let subscribers = self.inner.borrow().subscribers.clone();
        for (_, sub) in &subscribers {
            if let Subscriber::Observer { error: Some(ref err_fn), .. } = sub {
                let _ = err_fn.call1(&JsValue::NULL, err_val);
            }
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
}
