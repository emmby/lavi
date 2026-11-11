use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use lavi::{IndexMap, IndexMapExt, RuntimeMachine, Value};

use crate::actions::{process_action_item, CURRENT_ACTOR};
use crate::actor::{create_actor, resolve_host_clock, Actor, ActorInner};
use crate::guards::process_guard_value;
use crate::value_bridge::value_to_js;

#[wasm_bindgen(js_name = sendTo)]
pub fn send_to(target: JsValue, event: JsValue, options: Option<JsValue>) -> JsValue {
    if event.is_string() {
        let str_val = event.as_string().unwrap_or_default();
        let func = js_sys::Function::new_no_args("return undefined;");
        let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.sendTo.invalid"));
        let err_msg = format!("Only event objects may be used with sendTo; use sendTo({{ type: \"{}\" }}) instead", str_val);
        let _ = js_sys::Reflect::set(&func, &JsValue::from_str("error"), &JsValue::from_str(&err_msg));
        return func.into();
    }
    let func = js_sys::Function::new_no_args("return undefined;");
    let params_obj = js_sys::Object::new();
    if let Some(target_str) = target.as_string() {
        let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("targetId"), &JsValue::from_str(&target_str));
        let _ = js_sys::Reflect::set(&func, &JsValue::from_str("targetId"), &JsValue::from_str(&target_str));
    }
    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("to"), &target);
    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("event"), &event);
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("to"), &target);
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("event"), &event);
    if let Some(ref opts) = options {
        if opts.is_object() && !opts.is_null() {
            let opts_obj = js_sys::Object::from(opts.clone());
            let keys = js_sys::Object::keys(&opts_obj);
            for i in 0..keys.length() {
                let k = keys.get(i);
                if let Ok(v) = js_sys::Reflect::get(&opts_obj, &k) {
                    let _ = js_sys::Reflect::set(&params_obj, &k, &v);
                    let _ = js_sys::Reflect::set(&func, &k, &v);
                }
            }
        }
    }
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.sendTo"));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("params"), &params_obj);
    func.into()
}

#[wasm_bindgen(js_name = sendParent)]
pub fn send_parent(event: JsValue, options: Option<JsValue>) -> JsValue {
    send_to(JsValue::from_str("#_parent"), event, options)
}

#[wasm_bindgen(js_name = forwardTo)]
pub fn forward_to(target: JsValue) -> JsValue {
    let func = js_sys::Function::new_no_args("return undefined;");
    let params_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("to"), &target);
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.forwardTo"));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("params"), &params_obj);
    func.into()
}

#[wasm_bindgen(js_name = stopChild)]
pub fn stop_child(child: JsValue) -> JsValue {
    let func = js_sys::Function::new_no_args("return undefined;");
    let params_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("child"), &child);
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.stopChild"));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("params"), &params_obj);
    func.into()
}

#[wasm_bindgen(js_name = spawnChild)]
pub fn spawn_child(src: JsValue, options: Option<JsValue>) -> JsValue {
    let func = js_sys::Function::new_no_args("return undefined;");
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.spawnChild"));
    let params_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("src"), &src);
    if let Some(opts) = options {
        if opts.is_object() && !opts.is_null() {
            let opts_obj = js_sys::Object::from(opts);
            let keys = js_sys::Object::keys(&opts_obj);
            for i in 0..keys.length() {
                let k = keys.get(i);
                if let Ok(v) = js_sys::Reflect::get(&opts_obj, &k) {
                    let _ = js_sys::Reflect::set(&params_obj, &k, &v);
                }
            }
        }
    }
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("params"), &params_obj);
    func.into()
}

pub(crate) fn spawn_child_from_actor(
    parent: &JsValue,
    src: &JsValue,
    options: &JsValue,
) -> JsValue {
    let resolved_src = if let Some(src_str) = src.as_string() {
        let mut found = JsValue::UNDEFINED;
        if let Ok(impls) = js_sys::Reflect::get(parent, &JsValue::from_str("_implementations")) {
            if impls.is_object() && !impls.is_null() {
                if let Ok(actors) = js_sys::Reflect::get(&impls, &JsValue::from_str("actors")) {
                    if actors.is_object() && !actors.is_null() {
                        if let Ok(act) = js_sys::Reflect::get(&actors, &JsValue::from_str(&src_str)) {
                            if !act.is_undefined() {
                                found = act;
                            }
                        }
                    }
                }
            }
        }
        if found.is_undefined() {
            if let Ok(config) = js_sys::Reflect::get(parent, &JsValue::from_str("config")) {
                if config.is_object() && !config.is_null() {
                    if let Ok(actors) = js_sys::Reflect::get(&config, &JsValue::from_str("actors")) {
                        if actors.is_object() && !actors.is_null() {
                            if let Ok(act) = js_sys::Reflect::get(&actors, &JsValue::from_str(&src_str)) {
                                if !act.is_undefined() {
                                    found = act;
                                }
                            }
                        }
                    }
                }
            }
        }
        if found.is_undefined() {
            src.clone()
        } else {
            found
        }
    } else {
        src.clone()
    };

    let raw_input = if options.is_object() && !options.is_null() {
        js_sys::Reflect::get(options, &JsValue::from_str("input")).unwrap_or(JsValue::UNDEFINED)
    } else {
        JsValue::UNDEFINED
    };
    let resolved_input = if let Ok(f) = raw_input.clone().dyn_into::<js_sys::Function>() {
        let arg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&arg, &JsValue::from_str("self"), parent);
        f.call1(&JsValue::NULL, &arg).unwrap_or(JsValue::UNDEFINED)
    } else {
        raw_input
    };

    let child_id_opt = if options.is_object() && !options.is_null() {
        js_sys::Reflect::get(options, &JsValue::from_str("id"))
            .ok()
            .and_then(|v| v.as_string())
    } else {
        None
    }
    .or_else(|| {
        if let Some(s) = src.as_string() {
            Some(s)
        } else if let Ok(id_val) = js_sys::Reflect::get(src, &JsValue::from_str("id")) {
            id_val.as_string()
        } else {
            None
        }
    });

    let child_opts = js_sys::Object::new();
    if let Some(ref cid) = child_id_opt {
        let _ = js_sys::Reflect::set(&child_opts, &JsValue::from_str("id"), &JsValue::from_str(cid));
    }
    let _ = js_sys::Reflect::set(&child_opts, &JsValue::from_str("input"), &resolved_input);
    let _ = js_sys::Reflect::set(&child_opts, &JsValue::from_str("parent"), parent);
    if let Ok(sys) = js_sys::Reflect::get(parent, &JsValue::from_str("system")) {
        let _ = js_sys::Reflect::set(&child_opts, &JsValue::from_str("system"), &sys);
    }

    let child_actor = match create_actor(resolved_src, Some(child_opts.into())) {
        Ok(act) => act,
        Err(e) => {
            web_sys::console::warn_2(&JsValue::from_str("Failed to spawn child:"), &e);
            return JsValue::UNDEFINED;
        }
    };

    let register_fn = js_sys::Reflect::get(parent, &JsValue::from_str("_registerChild")).unwrap_or(JsValue::UNDEFINED);
    if let Ok(f) = register_fn.dyn_into::<js_sys::Function>() {
        let actual_id = js_sys::Reflect::get(&child_actor, &JsValue::from_str("id"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let _ = f.call2(parent, &JsValue::from_str(&actual_id), &child_actor);
        if let Some(ref cid) = child_id_opt {
            if *cid != actual_id {
                let _ = f.call2(parent, &JsValue::from_str(cid), &child_actor);
            }
        }
    }

    if let Ok(start_fn) = js_sys::Reflect::get(&child_actor, &JsValue::from_str("start")) {
        if let Ok(f) = start_fn.dyn_into::<js_sys::Function>() {
            let _ = f.call0(&child_actor);
        }
    }

    child_actor
}

pub(crate) fn execute_send_to_action(
    action_item: &JsValue,
    ctx_val: &mut Value,
    evt_val: &Value,
    params_val: &Option<Value>,
) {
    let current_actor_opt = CURRENT_ACTOR.with(|cell| cell.borrow().clone());
    let current_actor = match current_actor_opt {
        Some(a) => a,
        None => return,
    };

    let params = js_sys::Reflect::get(action_item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
    if !params.is_object() || params.is_null() {
        return;
    }

    let to_val = js_sys::Reflect::get(&params, &JsValue::from_str("to")).unwrap_or(JsValue::UNDEFINED);
    let ctx_js = value_to_js(ctx_val);
    let evt_js = Actor::event_to_js(evt_val);
    let params_js = if let Some(p) = params_val {
        value_to_js(p)
    } else {
        JsValue::UNDEFINED
    };

    let arg_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &current_actor);
    let sys = js_sys::Reflect::get(&current_actor, &JsValue::from_str("system")).unwrap_or(JsValue::UNDEFINED);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("system"), &sys);

    let actor_for_spawn = current_actor.clone();
    let _spawn_fn = Closure::wrap(Box::new(move |src: JsValue, options: JsValue| -> JsValue {
        spawn_child_from_actor(&actor_for_spawn, &src, &options)
    }) as Box<dyn Fn(JsValue, JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("spawn"), _spawn_fn.as_ref());

    let resolved_to = if let Ok(f) = to_val.clone().dyn_into::<js_sys::Function>() {
        f.call2(&JsValue::NULL, &arg_obj, &params_js).unwrap_or(JsValue::UNDEFINED)
    } else {
        to_val
    };

    let mut target_id_opt: Option<String> = None;
    let target = if let Some(to_str) = resolved_to.as_string() {
        target_id_opt = Some(to_str.clone());
        if to_str == "#_parent" {
            let mut p = js_sys::Reflect::get(&current_actor, &JsValue::from_str("parent")).unwrap_or(JsValue::UNDEFINED);
            if p.is_undefined() {
                p = js_sys::Reflect::get(&current_actor, &JsValue::from_str("_parent")).unwrap_or(JsValue::UNDEFINED);
            }
            p
        } else {
            let mut target_actor = JsValue::UNDEFINED;
            if let Ok(snap) = js_sys::Reflect::get(&current_actor, &JsValue::from_str("getSnapshot")) {
                if let Ok(f) = snap.dyn_into::<js_sys::Function>() {
                    if let Ok(s) = f.call0(&current_actor) {
                        if let Ok(children) = js_sys::Reflect::get(&s, &JsValue::from_str("children")) {
                            if children.is_object() && !children.is_null() {
                                if let Ok(c) = js_sys::Reflect::get(&children, &JsValue::from_str(&to_str)) {
                                    if !c.is_undefined() {
                                        target_actor = c;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if target_actor.is_undefined() {
                if let Ok(ensure_fn) = js_sys::Reflect::get(&current_actor, &JsValue::from_str("_ensureInvokeChild")) {
                    if let Ok(f) = ensure_fn.dyn_into::<js_sys::Function>() {
                        if let Ok(child) = f.call1(&current_actor, &JsValue::from_str(&to_str)) {
                            if !child.is_undefined() {
                                target_actor = child;
                            }
                        }
                    }
                }
            }
            if target_actor.is_undefined() {
                if let Ok(sys) = js_sys::Reflect::get(&current_actor, &JsValue::from_str("system")) {
                    if let Ok(get_fn) = js_sys::Reflect::get(&sys, &JsValue::from_str("get")) {
                        if let Ok(f) = get_fn.dyn_into::<js_sys::Function>() {
                            if let Ok(act) = f.call1(&sys, &JsValue::from_str(&to_str)) {
                                if !act.is_undefined() {
                                    target_actor = act;
                                }
                            }
                        }
                    }
                }
            }
            target_actor
        }
    } else if resolved_to.is_object() && !resolved_to.is_null() {
        resolved_to
    } else {
        JsValue::UNDEFINED
    };

    let evt_expr = js_sys::Reflect::get(&params, &JsValue::from_str("event")).unwrap_or(JsValue::UNDEFINED);
    let resolved_evt = if let Ok(f) = evt_expr.clone().dyn_into::<js_sys::Function>() {
        f.call2(&JsValue::NULL, &arg_obj, &params_js).unwrap_or(JsValue::UNDEFINED)
    } else {
        evt_expr
    };

    if resolved_evt.is_undefined() || resolved_evt.is_null() {
        return;
    }

    let delay_raw = js_sys::Reflect::get(&params, &JsValue::from_str("delay")).unwrap_or(JsValue::UNDEFINED);
    let id_raw = js_sys::Reflect::get(&params, &JsValue::from_str("id")).unwrap_or(JsValue::UNDEFINED);
    let send_id_opt = id_raw.as_string();

    if !delay_raw.is_undefined() {
        let delay_ms = if delay_raw.is_function() {
            let func: js_sys::Function = delay_raw.unchecked_into();
            func.call2(&JsValue::NULL, &arg_obj, &params_js)
                .ok()
                .and_then(|v| {
                    if let Some(n) = v.as_f64() {
                        Some(n.max(0.0) as u32)
                    } else if let Some(s) = v.as_string() {
                        s.parse::<u32>().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0)
        } else if let Some(n) = delay_raw.as_f64() {
            n.max(0.0) as u32
        } else if let Some(s) = delay_raw.as_string() {
            s.parse::<u32>().unwrap_or(0)
        } else {
            0
        };

        let target_captured = target.clone();
        let evt_captured = resolved_evt.clone();
        let current_actor_captured = current_actor.clone();
        let send_id_captured = send_id_opt.clone();
        let target_id_captured = target_id_opt.clone();

        let timer_cb = Closure::wrap(Box::new(move || {
            if let Some(ref sid) = send_id_captured {
                if let Ok(cancel_fn) = js_sys::Reflect::get(&current_actor_captured, &JsValue::from_str("_cancelSendTimer")) {
                    if let Ok(f) = cancel_fn.dyn_into::<js_sys::Function>() {
                        let _ = f.call1(&current_actor_captured, &JsValue::from_str(sid));
                    }
                }
            }

            if target_captured.is_object() && !target_captured.is_null() {
                let mut is_stopped = false;
                if let Ok(snap_fn) = js_sys::Reflect::get(&target_captured, &JsValue::from_str("getSnapshot")) {
                    if let Ok(f) = snap_fn.dyn_into::<js_sys::Function>() {
                        if let Ok(s) = f.call0(&target_captured) {
                            if let Ok(status) = js_sys::Reflect::get(&s, &JsValue::from_str("status")) {
                                if let Some(status_str) = status.as_string() {
                                    if status_str == "stopped" || status_str == "done" {
                                        is_stopped = true;
                                    }
                                }
                            }
                        }
                    }
                }
                if !is_stopped {
                    if let Ok(status) = js_sys::Reflect::get(&target_captured, &JsValue::from_str("status")) {
                        if let Some(status_str) = status.as_string() {
                            if status_str == "stopped" || status_str == "done" {
                                is_stopped = true;
                            }
                        }
                    }
                }

                if is_stopped {
                    let id_str = js_sys::Reflect::get(&target_captured, &JsValue::from_str("id"))
                        .ok()
                        .and_then(|v| v.as_string())
                        .or_else(|| target_id_captured.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    let session_id_str = js_sys::Reflect::get(&target_captured, &JsValue::from_str("sessionId"))
                        .ok()
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();
                    let event_type = js_sys::Reflect::get(&evt_captured, &JsValue::from_str("type"))
                        .ok()
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();
                    let evt_json_str = js_sys::JSON::stringify(&evt_captured)
                        .ok()
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();
                    let warn_msg = format!(
                        "Event \"{}\" was sent to stopped actor \"{} ({})\". This actor has already reached its final state, and will not transition.\nEvent: {}",
                        event_type, id_str, session_id_str, evt_json_str
                    );
                    web_sys::console::warn_1(&JsValue::from_str(&warn_msg));
                    return;
                }

                let sys = js_sys::Reflect::get(&current_actor_captured, &JsValue::from_str("system")).unwrap_or(JsValue::UNDEFINED);
                if sys.is_object() && !sys.is_null() {
                    if let Ok(relay) = js_sys::Reflect::get(&sys, &JsValue::from_str("_relay")) {
                        if let Ok(f) = relay.dyn_into::<js_sys::Function>() {
                            let _ = f.call3(&sys, &current_actor_captured, &target_captured, &evt_captured);
                            return;
                        }
                    }
                }
                if let Ok(send) = js_sys::Reflect::get(&target_captured, &JsValue::from_str("send")) {
                    if let Ok(f) = send.dyn_into::<js_sys::Function>() {
                        let _ = f.call1(&target_captured, &evt_captured);
                    }
                }
            }
        }) as Box<dyn FnMut()>);

        let host_clock_val = js_sys::Reflect::get(&current_actor, &JsValue::from_str("clock")).ok();
        let (st_fn, _): (js_sys::Function, js_sys::Function) = resolve_host_clock(host_clock_val.as_ref()).unwrap_or_else(|| {
            let global = js_sys::global();
            let st = js_sys::Reflect::get(&global, &JsValue::from_str("setTimeout")).unwrap();
            let ct = js_sys::Reflect::get(&global, &JsValue::from_str("clearTimeout")).unwrap();
            (st.unchecked_into(), ct.unchecked_into())
        });

        if let Ok(handle) = st_fn.call2(&JsValue::NULL, timer_cb.as_ref().unchecked_ref(), &JsValue::from_f64(delay_ms as f64)) {
            if let Some(ref sid) = send_id_opt {
                if let Ok(reg_fn) = js_sys::Reflect::get(&current_actor, &JsValue::from_str("_registerSendTimer")) {
                    if let Ok(f) = reg_fn.dyn_into::<js_sys::Function>() {
                        let _ = f.call2(&current_actor, &JsValue::from_str(sid), &handle);
                    }
                }
            }
        }
        timer_cb.forget();
        return;
    }

    if target.is_object() && !target.is_null() {
        let sys = js_sys::Reflect::get(&current_actor, &JsValue::from_str("system")).unwrap_or(JsValue::UNDEFINED);
        if sys.is_object() && !sys.is_null() {
            if let Ok(relay) = js_sys::Reflect::get(&sys, &JsValue::from_str("_relay")) {
                if let Ok(f) = relay.dyn_into::<js_sys::Function>() {
                    let _ = f.call3(&sys, &current_actor, &target, &resolved_evt);
                    return;
                }
            }
        }
        if let Ok(send) = js_sys::Reflect::get(&target, &JsValue::from_str("send")) {
            if let Ok(f) = send.dyn_into::<js_sys::Function>() {
                let _ = f.call1(&target, &resolved_evt);
            }
        }
    }
}

pub(crate) fn execute_stop_child_action(
    action_item: &JsValue,
    ctx_val: &mut Value,
    evt_val: &Value,
    params_val: &Option<Value>,
) {
    let current_actor_opt = CURRENT_ACTOR.with(|cell| cell.borrow().clone());
    let current_actor = match current_actor_opt {
        Some(a) => a,
        None => return,
    };
    let params = js_sys::Reflect::get(action_item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
    let child_expr = if params.is_object() && !params.is_null() {
        js_sys::Reflect::get(&params, &JsValue::from_str("child")).unwrap_or(JsValue::UNDEFINED)
    } else {
        js_sys::Reflect::get(action_item, &JsValue::from_str("child")).unwrap_or(JsValue::UNDEFINED)
    };
    let ctx_js = value_to_js(ctx_val);
    let evt_js = Actor::event_to_js(evt_val);
    let params_js = if let Some(p) = params_val {
        value_to_js(p)
    } else {
        JsValue::UNDEFINED
    };
    let arg_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &current_actor);
    let resolved_child = if let Ok(f) = child_expr.clone().dyn_into::<js_sys::Function>() {
        f.call2(&JsValue::NULL, &arg_obj, &params_js).unwrap_or(JsValue::UNDEFINED)
    } else {
        child_expr
    };

    let target = if let Some(child_id_str) = resolved_child.as_string() {
        let mut target_actor = JsValue::UNDEFINED;
        if let Ok(snap) = js_sys::Reflect::get(&current_actor, &JsValue::from_str("getSnapshot")) {
            if let Ok(f) = snap.dyn_into::<js_sys::Function>() {
                if let Ok(s) = f.call0(&current_actor) {
                    if let Ok(children) = js_sys::Reflect::get(&s, &JsValue::from_str("children")) {
                        if children.is_object() && !children.is_null() {
                            if let Ok(c) = js_sys::Reflect::get(&children, &JsValue::from_str(&child_id_str)) {
                                if !c.is_undefined() {
                                    target_actor = c;
                                }
                            }
                        }
                    }
                }
            }
        }
        if target_actor.is_undefined() {
            if let Ok(ensure_fn) = js_sys::Reflect::get(&current_actor, &JsValue::from_str("_ensureInvokeChild")) {
                if let Ok(f) = ensure_fn.dyn_into::<js_sys::Function>() {
                    if let Ok(child) = f.call1(&current_actor, &JsValue::from_str(&child_id_str)) {
                        if !child.is_undefined() {
                            target_actor = child;
                        }
                    }
                }
            }
        }
        target_actor
    } else if resolved_child.is_object() && !resolved_child.is_null() {
        resolved_child
    } else {
        JsValue::UNDEFINED
    };

    if target.is_object() && !target.is_null() {
        if let Ok(stop_fn) = js_sys::Reflect::get(&target, &JsValue::from_str("stop")) {
            if let Ok(f) = stop_fn.dyn_into::<js_sys::Function>() {
                let _ = f.call0(&target);
            }
        }
        if let Ok(rm_fn) = js_sys::Reflect::get(&current_actor, &JsValue::from_str("_removeChild")) {
            if let Ok(f) = rm_fn.dyn_into::<js_sys::Function>() {
                let _ = f.call1(&current_actor, &target);
            }
        }
        if let Ok(sys) = js_sys::Reflect::get(&current_actor, &JsValue::from_str("system")) {
            if let Ok(unreg_fn) = js_sys::Reflect::get(&sys, &JsValue::from_str("_unregister")) {
                if let Ok(f) = unreg_fn.dyn_into::<js_sys::Function>() {
                    let _ = f.call1(&sys, &target);
                }
            }
        }
    }
}

pub(crate) fn execute_forward_to_action(
    action_item: &JsValue,
    ctx_val: &mut Value,
    evt_val: &Value,
    params_val: &Option<Value>,
    effects: &mut lavi::ActionEffects,
) {
    let current_actor_opt = CURRENT_ACTOR.with(|cell| cell.borrow().clone());
    let current_actor = match current_actor_opt {
        Some(a) => a,
        None => return,
    };
    let params = js_sys::Reflect::get(action_item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
    let to_expr = if params.is_object() && !params.is_null() {
        js_sys::Reflect::get(&params, &JsValue::from_str("to")).unwrap_or(JsValue::UNDEFINED)
    } else {
        js_sys::Reflect::get(action_item, &JsValue::from_str("to")).unwrap_or(JsValue::UNDEFINED)
    };
    if to_expr.is_undefined() || to_expr.is_null() {
        let err_msg = "Attempted to forward event to undefined actor. This risks an infinite loop in the sender.";
        effects.error = Some(err_msg.to_string());
        return;
    }

    let ctx_js = value_to_js(ctx_val);
    let evt_js = Actor::event_to_js(evt_val);
    let params_js = if let Some(p) = params_val {
        value_to_js(p)
    } else {
        JsValue::UNDEFINED
    };
    let arg_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &current_actor);
    let resolved_to = if let Ok(f) = to_expr.clone().dyn_into::<js_sys::Function>() {
        f.call2(&JsValue::NULL, &arg_obj, &params_js).unwrap_or(JsValue::UNDEFINED)
    } else {
        to_expr
    };

    if resolved_to.is_undefined() || resolved_to.is_null() {
        let err_msg = "Attempted to forward event to undefined actor. This risks an infinite loop in the sender.";
        effects.error = Some(err_msg.to_string());
        return;
    }

    let target = if let Some(to_str) = resolved_to.as_string() {
        if to_str == "#_parent" {
            let mut p = js_sys::Reflect::get(&current_actor, &JsValue::from_str("parent")).unwrap_or(JsValue::UNDEFINED);
            if p.is_undefined() {
                p = js_sys::Reflect::get(&current_actor, &JsValue::from_str("_parent")).unwrap_or(JsValue::UNDEFINED);
            }
            p
        } else {
            let mut target_actor = JsValue::UNDEFINED;
            if let Ok(snap) = js_sys::Reflect::get(&current_actor, &JsValue::from_str("getSnapshot")) {
                if let Ok(f) = snap.dyn_into::<js_sys::Function>() {
                    if let Ok(s) = f.call0(&current_actor) {
                        if let Ok(children) = js_sys::Reflect::get(&s, &JsValue::from_str("children")) {
                            if children.is_object() && !children.is_null() {
                                if let Ok(c) = js_sys::Reflect::get(&children, &JsValue::from_str(&to_str)) {
                                    if !c.is_undefined() {
                                        target_actor = c;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if target_actor.is_undefined() {
                if let Ok(ensure_fn) = js_sys::Reflect::get(&current_actor, &JsValue::from_str("_ensureInvokeChild")) {
                    if let Ok(f) = ensure_fn.dyn_into::<js_sys::Function>() {
                        if let Ok(child) = f.call1(&current_actor, &JsValue::from_str(&to_str)) {
                            if !child.is_undefined() {
                                target_actor = child;
                            }
                        }
                    }
                }
            }
            if target_actor.is_undefined() {
                if let Ok(sys) = js_sys::Reflect::get(&current_actor, &JsValue::from_str("system")) {
                    if let Ok(get_fn) = js_sys::Reflect::get(&sys, &JsValue::from_str("get")) {
                        if let Ok(f) = get_fn.dyn_into::<js_sys::Function>() {
                            if let Ok(act) = f.call1(&sys, &JsValue::from_str(&to_str)) {
                                if !act.is_undefined() {
                                    target_actor = act;
                                }
                            }
                        }
                    }
                }
            }
            target_actor
        }
    } else if resolved_to.is_object() && !resolved_to.is_null() {
        resolved_to
    } else {
        JsValue::UNDEFINED
    };

    if target.is_object() && !target.is_null() {
        let sys = js_sys::Reflect::get(&current_actor, &JsValue::from_str("system")).unwrap_or(JsValue::UNDEFINED);
        if sys.is_object() && !sys.is_null() {
            if let Ok(relay) = js_sys::Reflect::get(&sys, &JsValue::from_str("_relay")) {
                if let Ok(f) = relay.dyn_into::<js_sys::Function>() {
                    let _ = f.call3(&sys, &current_actor, &target, &evt_js);
                    return;
                }
            }
        }
        if let Ok(send) = js_sys::Reflect::get(&target, &JsValue::from_str("send")) {
            if let Ok(f) = send.dyn_into::<js_sys::Function>() {
                let _ = f.call1(&target, &evt_js);
            }
        }
    }
}

pub(crate) fn execute_spawn_child_action(action_item: &JsValue) {
    let current_actor_opt = CURRENT_ACTOR.with(|cell| cell.borrow().clone());
    let current_actor = match current_actor_opt {
        Some(a) => a,
        None => return,
    };
    let params = js_sys::Reflect::get(action_item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
    if !params.is_object() || params.is_null() {
        return;
    }
    let src = js_sys::Reflect::get(&params, &JsValue::from_str("src")).unwrap_or(JsValue::UNDEFINED);
    spawn_child_from_actor(&current_actor, &src, &params);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_invoke_list(
    state_obj: &js_sys::Object,
    guards: &mut IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    actions_with_effects: &mut IndexMap<String, lavi::ActionWithEffectsFunction>,
    action_descriptors: &mut IndexMap<String, JsValue>,
    counter: &mut u32,
    path: &str,
    guards_holder: &Rc<RefCell<IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>>>,
    logger_holder: &Rc<RefCell<Option<js_sys::Function>>>,
) {
    let invoke_key = JsValue::from_str("invoke");
    let val = js_sys::Reflect::get(state_obj, &invoke_key).unwrap_or(JsValue::UNDEFINED);

    if val.is_undefined() {
        return;
    }

    let register_invoke = |item: &JsValue, index: usize, action_descriptors: &mut IndexMap<String, JsValue>| {
        if item.is_object() && !item.is_null() {
            let item_obj = js_sys::Object::from(item.clone());
            let explicit_id = js_sys::Reflect::get(&item_obj, &JsValue::from_str("id"))
                .ok()
                .and_then(|v| v.as_string());
            let state_id = if path.is_empty() {
                "(machine)".to_string()
            } else {
                format!("(machine).{}", path)
            };
            let invoke_id = explicit_id.clone().unwrap_or_else(|| format!("{}.{}", index, state_id));
            let key = format!("__xstate_invoke_desc_{}", invoke_id);
            action_descriptors.insert(key, item.clone());
            action_descriptors.insert(format!("__xstate_invoke_desc_{}.{}", index, path), item.clone());
            if !path.is_empty() {
                action_descriptors.insert(format!("__xstate_invoke_desc_{}", path), item.clone());
            }
            if let Some(ref eid) = explicit_id {
                action_descriptors.insert(format!("__xstate_invoke_desc_{}", eid), item.clone());
            }
        }
    };

    if js_sys::Array::is_array(&val) {
        let arr = js_sys::Array::from(&val);
        for i in 0..arr.length() {
            let item = arr.get(i);
            process_invoke_item(
                &item,
                guards,
                actions_out,
                actions_with_effects,
                action_descriptors,
                counter,
                path,
                guards_holder,
                logger_holder,
            );
            register_invoke(&item, i as usize, action_descriptors);
            if let Some(new_item) = convert_inline_invoke(&item, actions_out, counter) {
                arr.set(i, new_item);
            }
        }
    } else {
        process_invoke_item(
            &val,
            guards,
            actions_out,
            actions_with_effects,
            action_descriptors,
            counter,
            path,
            guards_holder,
            logger_holder,
        );
        register_invoke(&val, 0, action_descriptors);
        if let Some(new_item) = convert_inline_invoke(&val, actions_out, counter) {
            let _ = js_sys::Reflect::set(state_obj, &invoke_key, &new_item);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_invoke_item(
    item: &JsValue,
    guards: &mut IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    actions_with_effects: &mut IndexMap<String, lavi::ActionWithEffectsFunction>,
    action_descriptors: &mut IndexMap<String, JsValue>,
    counter: &mut u32,
    path: &str,
    guards_holder: &Rc<RefCell<IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>>>,
    logger_holder: &Rc<RefCell<Option<js_sys::Function>>>,
) {
    if item.is_object() && !item.is_null() {
        let item_obj = js_sys::Object::from(item.clone());
        for prop in &["onDone", "onError", "onSnapshot"] {
            let prop_key = JsValue::from_str(prop);
            let trans_val = js_sys::Reflect::get(&item_obj, &prop_key).unwrap_or(JsValue::UNDEFINED);
            if !trans_val.is_undefined() && !trans_val.is_null() {
                process_invoke_transition(
                    &trans_val,
                    guards,
                    actions_out,
                    actions_with_effects,
                    action_descriptors,
                    counter,
                    path,
                    guards_holder,
                    logger_holder,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_invoke_transition(
    trans_val: &JsValue,
    guards: &mut IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    actions_with_effects: &mut IndexMap<String, lavi::ActionWithEffectsFunction>,
    action_descriptors: &mut IndexMap<String, JsValue>,
    counter: &mut u32,
    path: &str,
    guards_holder: &Rc<RefCell<IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>>>,
    logger_holder: &Rc<RefCell<Option<js_sys::Function>>>,
) {
    if js_sys::Array::is_array(trans_val) {
        let arr: &js_sys::Array = trans_val.unchecked_ref();
        for i in 0..arr.length() {
            let elem = arr.get(i);
            if elem.is_object() && !elem.is_null() && !elem.is_function() {
                let trans_obj = js_sys::Object::from(elem);
                process_invoke_transition_object(
                    &trans_obj,
                    guards,
                    actions_out,
                    actions_with_effects,
                    action_descriptors,
                    counter,
                    path,
                    guards_holder,
                    logger_holder,
                );
            }
        }
    } else if trans_val.is_object() && !trans_val.is_null() && !trans_val.is_function() {
        let trans_obj = js_sys::Object::from(trans_val.clone());
        process_invoke_transition_object(
            &trans_obj,
            guards,
            actions_out,
            actions_with_effects,
            action_descriptors,
            counter,
            path,
            guards_holder,
            logger_holder,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_invoke_transition_object(
    trans_obj: &js_sys::Object,
    guards: &mut IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    actions_with_effects: &mut IndexMap<String, lavi::ActionWithEffectsFunction>,
    action_descriptors: &mut IndexMap<String, JsValue>,
    counter: &mut u32,
    path: &str,
    guards_holder: &Rc<RefCell<IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>>>,
    logger_holder: &Rc<RefCell<Option<js_sys::Function>>>,
) {
    let guard_key = JsValue::from_str("guard");
    let cond_key = JsValue::from_str("cond");
    let guard_val = js_sys::Reflect::get(trans_obj, &guard_key).unwrap_or(JsValue::UNDEFINED);
    let (used_key, g_val) = if !guard_val.is_undefined() {
        (guard_key, guard_val)
    } else {
        let c_val = js_sys::Reflect::get(trans_obj, &cond_key).unwrap_or(JsValue::UNDEFINED);
        (cond_key, c_val)
    };
    if !g_val.is_undefined() && !g_val.is_null() {
        let processed_guard = process_guard_value(
            &g_val,
            guards,
            action_descriptors,
            counter,
            guards_holder,
        );
        let _ = js_sys::Reflect::set(trans_obj, &used_key, &processed_guard);
    }

    let actions_key = JsValue::from_str("actions");
    let actions_val = js_sys::Reflect::get(trans_obj, &actions_key).unwrap_or(JsValue::UNDEFINED);
    if !actions_val.is_undefined() && !actions_val.is_null() {
        if js_sys::Array::is_array(&actions_val) {
            let arr: &js_sys::Array = actions_val.unchecked_ref();
            for j in 0..arr.length() {
                let it = arr.get(j);
                let processed = process_action_item(
                    &it,
                    actions_out,
                    actions_with_effects,
                    action_descriptors,
                    counter,
                    path,
                    logger_holder,
                );
                arr.set(j, processed);
            }
        } else {
            let processed = process_action_item(
                &actions_val,
                actions_out,
                actions_with_effects,
                action_descriptors,
                counter,
                path,
                logger_holder,
            );
            let _ = js_sys::Reflect::set(trans_obj, &actions_key, &processed);
        }
    }
}

pub(crate) fn convert_inline_invoke(
    invoke_item: &JsValue,
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    counter: &mut u32,
) -> Option<JsValue> {
    if !invoke_item.is_object() {
        return None;
    }
    let obj = js_sys::Object::from(invoke_item.clone());
    let src_key = JsValue::from_str("src");
    let src_val = js_sys::Reflect::get(&obj, &src_key).unwrap_or(JsValue::UNDEFINED);

    if src_val.is_function() {
        if let Ok(func) = src_val.clone().dyn_into::<js_sys::Function>() {
            *counter += 1;
            let new_src_name = format!("generated_invoke_{}", counter);
            let func_clone = func.clone();
            actions_out.insert(
                new_src_name.clone(),
                Rc::new(move |ctx_mut, evt, params| {
                    let ctx_js = value_to_js(ctx_mut);
                    let evt_js = value_to_js(evt);
                    let arg_obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
                    if let Some(p) = params {
                        let p_js = value_to_js(p);
                        let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("params"), &p_js);
                    }
                    if let Err(e) = func_clone.call1(&JsValue::NULL, &arg_obj) {
                        web_sys::console::log_2(&JsValue::from_str("Error executing invoked function:"), &e);
                    }
                }),
            );

            let _ = js_sys::Reflect::set(&obj, &src_key, &JsValue::from_str(&new_src_name));
            return Some(obj.into());
        }
    }
    None
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
            self.machine
                .definition
                .invoke
                .iter()
                .enumerate()
                .map(|(i, inv)| (i, inv.clone()))
                .collect()
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

            let raw_desc_opt = self
                .action_descriptors
                .get(&format!("__xstate_invoke_desc_{}", invoke_id))
                .or_else(|| self.action_descriptors.get(&format!("__xstate_invoke_desc_{}.{}", inv_idx, path)))
                .or_else(|| {
                    if !path.is_empty() {
                        self.action_descriptors.get(&format!("__xstate_invoke_desc_{}", path))
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    if let Some(ref eid) = inv.id {
                        self.action_descriptors.get(&format!("__xstate_invoke_desc_{}", eid))
                    } else {
                        None
                    }
                });

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

    pub(crate) fn spawn_invokes_for_path(
        inner_rc: &Rc<RefCell<Self>>,
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

    pub(crate) fn ensure_invoke_child(inner_rc: &Rc<RefCell<Self>>, id: &str) -> JsValue {
        {
            let Ok(inner) = inner_rc.try_borrow() else {
                return JsValue::UNDEFINED;
            };
            if let Some(child) = inner.children.get(id) {
                return child.clone();
            }
        }

        let (init_evt, current_ctx, pending) = {
            let Ok(inner) = inner_rc.try_borrow() else {
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
            execute_pending_invokes(inner_rc, pending, &init_evt, &current_ctx, false);
            if let Ok(inner) = inner_rc.try_borrow() {
                if let Some(child) = inner.children.get(id) {
                    return child.clone();
                }
            }
        }
        JsValue::UNDEFINED
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
    ActorInner::spawn_invokes_for_path(inner_rc, path, current_event, current_context, defer_start);
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
