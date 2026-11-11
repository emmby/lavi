use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use lavi::{IndexMap, IndexMapExt};

static NEXT_SESSION_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

pub fn next_session_id() -> String {
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
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default()
        } else {
            String::new()
        };

        let custom_id = if actor_ref.is_object() && !actor_ref.is_null() {
            js_sys::Reflect::get(&actor_ref, &JsValue::from_str("id"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default()
        } else {
            String::new()
        };

        let system_id = if actor_ref.is_object() && !actor_ref.is_null() {
            js_sys::Reflect::get(&actor_ref, &JsValue::from_str("systemId"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default()
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

pub(crate) fn emit_to_system_inspector(insp: &JsValue, ev: &JsValue) {
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

pub(crate) fn resolve_input_option(options: &JsValue, actor_ref: &JsValue) -> JsValue {
    if !options.is_object() || options.is_null() {
        return JsValue::UNDEFINED;
    }
    let input_val = js_sys::Reflect::get(options, &JsValue::from_str("input")).unwrap_or(JsValue::UNDEFINED);
    if input_val.is_function() {
        let f: js_sys::Function = input_val.unchecked_into();
        let arg_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), actor_ref);
        f.call1(&JsValue::NULL, &arg_obj).unwrap_or(JsValue::UNDEFINED)
    } else {
        input_val
    }
}

pub(crate) fn system_register(system: &JsValue, actor_ref: &JsValue) {
    if system.is_object() && !system.is_null() {
        if let Ok(reg) = js_sys::Reflect::get(system, &JsValue::from_str("_register")) {
            if let Ok(f) = reg.dyn_into::<js_sys::Function>() {
                let _ = f.call1(system, actor_ref);
            }
        }
    }
}

pub(crate) fn system_relay(system: &JsValue, source_ref: &JsValue, target_ref: &JsValue, event: &JsValue) {
    if system.is_object() && !system.is_null() {
        if let Ok(relay) = js_sys::Reflect::get(system, &JsValue::from_str("_relay")) {
            if let Ok(f) = relay.dyn_into::<js_sys::Function>() {
                let _ = f.call3(system, source_ref, target_ref, event);
                return;
            }
        }
    }
    if target_ref.is_object() && !target_ref.is_null() {
        if let Ok(send) = js_sys::Reflect::get(target_ref, &JsValue::from_str("send")) {
            if let Ok(f) = send.dyn_into::<js_sys::Function>() {
                let _ = f.call1(target_ref, event);
            }
        }
    }
}

pub(crate) fn system_snapshot(system: &JsValue, actor_ref: &JsValue, event: &JsValue, snapshot: &JsValue, status: &str) {
    if system.is_object() && !system.is_null() {
        if let Ok(snap) = js_sys::Reflect::get(system, &JsValue::from_str("_snapshot")) {
            if let Ok(f) = snap.dyn_into::<js_sys::Function>() {
                let _ = f.call4(system, actor_ref, event, snapshot, &JsValue::from_str(status));
            }
        }
    }
}
