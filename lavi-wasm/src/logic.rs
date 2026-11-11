use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

static NEXT_TRANSITION_SUB_ID: AtomicU32 = AtomicU32::new(1);

fn resolve_input_option(options: &JsValue, actor_ref: &JsValue) -> JsValue {
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

fn system_register(system: &JsValue, actor_ref: &JsValue) {
    if system.is_object() && !system.is_null() {
        if let Ok(reg) = js_sys::Reflect::get(system, &JsValue::from_str("_register")) {
            if let Ok(f) = reg.dyn_into::<js_sys::Function>() {
                let _ = f.call1(system, actor_ref);
            }
        }
    }
}

fn system_relay(system: &JsValue, source_ref: &JsValue, target_ref: &JsValue, event: &JsValue) {
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

fn system_snapshot(system: &JsValue, actor_ref: &JsValue, event: &JsValue, snapshot: &JsValue, status: &str) {
    if system.is_object() && !system.is_null() {
        if let Ok(snap) = js_sys::Reflect::get(system, &JsValue::from_str("_snapshot")) {
            if let Ok(f) = snap.dyn_into::<js_sys::Function>() {
                let _ = f.call4(system, actor_ref, event, snapshot, &JsValue::from_str(status));
            }
        }
    }
}

#[wasm_bindgen(js_name = fromTransition)]
pub fn from_transition(transition_fn: JsValue, initial_context: JsValue) -> Result<JsValue, JsValue> {
    let logic_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("config"), &transition_fn);

    // transition function: (snapshot, event, actorScope) => { ...snapshot, context: transition_fn(snapshot.context, event, actorScope) }
    let trans_fn_clone = transition_fn.clone();
    let transition_closure = Closure::wrap(Box::new(move |snapshot: JsValue, event: JsValue, actor_scope: JsValue| -> Result<JsValue, JsValue> {
        let current_ctx = if snapshot.is_object() && !snapshot.is_null() {
            js_sys::Reflect::get(&snapshot, &JsValue::from_str("context")).unwrap_or(JsValue::UNDEFINED)
        } else {
            JsValue::UNDEFINED
        };
        let next_ctx = if let Ok(f) = trans_fn_clone.clone().dyn_into::<js_sys::Function>() {
            f.call3(&JsValue::NULL, &current_ctx, &event, &actor_scope)?
        } else {
            current_ctx
        };
        let next_snapshot = js_sys::Object::new();
        if snapshot.is_object() && !snapshot.is_null() {
            let snap_obj = js_sys::Object::from(snapshot);
            let keys = js_sys::Object::keys(&snap_obj);
            for i in 0..keys.length() {
                let k = keys.get(i);
                if let Ok(v) = js_sys::Reflect::get(&snap_obj, &k) {
                    let _ = js_sys::Reflect::set(&next_snapshot, &k, &v);
                }
            }
        }
        let _ = js_sys::Reflect::set(&next_snapshot, &JsValue::from_str("context"), &next_ctx);
        Ok(next_snapshot.into())
    }) as Box<dyn Fn(JsValue, JsValue, JsValue) -> Result<JsValue, JsValue>>);

    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("transition"), transition_closure.as_ref());
    transition_closure.forget();

    // getInitialSnapshot: (actorScope, input) => ({ status: 'active', output: undefined, error: undefined, context: typeof initialContext === 'function' ? initialContext({ input, self: actorScope?.self }) : initialContext })
    let init_ctx_clone = initial_context.clone();
    let get_init_closure = Closure::wrap(Box::new(move |actor_scope: JsValue, input: JsValue| -> Result<JsValue, JsValue> {
        let resolved_ctx = if let Ok(f) = init_ctx_clone.clone().dyn_into::<js_sys::Function>() {
            let arg_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("input"), &input);
            if actor_scope.is_object() && !actor_scope.is_null() {
                let self_val = js_sys::Reflect::get(&actor_scope, &JsValue::from_str("self")).unwrap_or(JsValue::UNDEFINED);
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &self_val);
            }
            f.call1(&JsValue::NULL, &arg_obj)?
        } else {
            init_ctx_clone.clone()
        };

        let snap_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("status"), &JsValue::from_str("active"));
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("output"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("error"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("context"), &resolved_ctx);
        Ok(snap_obj.into())
    }) as Box<dyn Fn(JsValue, JsValue) -> Result<JsValue, JsValue>>);

    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("getInitialSnapshot"), get_init_closure.as_ref());
    get_init_closure.forget();

    // getPersistedSnapshot: snapshot => snapshot
    let get_persisted_closure = Closure::wrap(Box::new(|snapshot: JsValue| -> JsValue {
        snapshot
    }) as Box<dyn Fn(JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("getPersistedSnapshot"), get_persisted_closure.as_ref());
    get_persisted_closure.forget();

    // restoreSnapshot: snapshot => snapshot
    let restore_closure = Closure::wrap(Box::new(|snapshot: JsValue| -> JsValue {
        snapshot
    }) as Box<dyn Fn(JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("restoreSnapshot"), restore_closure.as_ref());
    restore_closure.forget();

    // createActor: (options) => { ... }
    let logic_for_create = logic_obj.clone();
    let create_actor_closure = Closure::wrap(Box::new(move |options: JsValue| -> Result<JsValue, JsValue> {
        let actor_obj = js_sys::Object::new();
        let (custom_id, _parent_ref, system_ref) = if options.is_object() && !options.is_null() {
            let id = js_sys::Reflect::get(&options, &JsValue::from_str("id"))
                .ok()
                .and_then(|v| v.as_string());
            let parent = js_sys::Reflect::get(&options, &JsValue::from_str("parent")).unwrap_or(JsValue::UNDEFINED);
            let system = js_sys::Reflect::get(&options, &JsValue::from_str("system")).unwrap_or(JsValue::UNDEFINED);
            (id, parent, system)
        } else {
            (None, JsValue::UNDEFINED, JsValue::UNDEFINED)
        };
        let session_id = crate::actor::next_session_id();
        let id_str = custom_id.unwrap_or_else(|| session_id.clone());
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("id"), &JsValue::from_str(&id_str));
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("sessionId"), &JsValue::from_str(&session_id));
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("system"), &system_ref);
        let to_json_fn = js_sys::Function::new_no_args("return { id: this.id, xstate$$type: 1 };");
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("toJSON"), &to_json_fn);

        let input_val = resolve_input_option(&options, &actor_obj);
        let get_init = js_sys::Reflect::get(&logic_for_create, &JsValue::from_str("getInitialSnapshot"))?;
        let get_init_fn = get_init.dyn_into::<js_sys::Function>()?;
        let scope = js_sys::Object::new();
        let current_snap = Rc::new(RefCell::new(get_init_fn.call2(&logic_for_create, &scope, &input_val)?));

        let is_started = Rc::new(RefCell::new(false));
        let is_stopped = Rc::new(RefCell::new(false));

        // subscribers: (id, observer_object)
        let subscribers = Rc::new(RefCell::new(Vec::<(u32, js_sys::Object)>::new()));

        // getSnapshot
        let current_snap_for_get = current_snap.clone();
        let get_snap_closure = Closure::wrap(Box::new(move || -> JsValue {
            current_snap_for_get.borrow().clone()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("getSnapshot"), get_snap_closure.as_ref());
        get_snap_closure.forget();

        // send: (event) => ...
        let trans_fn_val = js_sys::Reflect::get(&logic_for_create, &JsValue::from_str("transition"))?;
        let trans_fn = trans_fn_val.dyn_into::<js_sys::Function>()?;
        let current_snap_for_send = current_snap.clone();
        let subscribers_for_send = subscribers.clone();
        let logic_for_send = logic_for_create.clone();
        let is_started_for_send = is_started.clone();
        let is_stopped_for_send = is_stopped.clone();
        let send_closure = Closure::wrap(Box::new(move |evt: JsValue| -> Result<JsValue, JsValue> {
            if !*is_started_for_send.borrow() || *is_stopped_for_send.borrow() {
                return Ok(JsValue::UNDEFINED);
            }
            let next_snap = trans_fn.call3(&logic_for_send, &current_snap_for_send.borrow(), &evt, &js_sys::Object::new())?;
            *current_snap_for_send.borrow_mut() = next_snap.clone();
            let observers: Vec<js_sys::Object> = subscribers_for_send
                .borrow()
                .iter()
                .map(|(_, o)| o.clone())
                .collect();
            for sub in observers {
                if let Ok(next_fn) = js_sys::Reflect::get(&sub, &JsValue::from_str("next")) {
                    if let Ok(f) = next_fn.dyn_into::<js_sys::Function>() {
                        let _ = f.call1(&JsValue::NULL, &next_snap);
                    }
                }
            }
            Ok(JsValue::UNDEFINED)
        }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("send"), send_closure.as_ref());
        send_closure.forget();

        // subscribe: (observer) => ({ unsubscribe })
        let subscribers_for_sub = subscribers.clone();
        let is_stopped_for_sub = is_stopped.clone();
        let sub_closure = Closure::wrap(Box::new(move |obs: JsValue| -> Result<JsValue, JsValue> {
            let obs_obj = if let Ok(f) = obs.clone().dyn_into::<js_sys::Function>() {
                let o = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&o, &JsValue::from_str("next"), &f);
                o
            } else if obs.is_object() && !obs.is_null() {
                js_sys::Object::from(obs)
            } else {
                js_sys::Object::new()
            };

            if *is_stopped_for_sub.borrow() {
                if let Ok(comp_fn) = js_sys::Reflect::get(&obs_obj, &JsValue::from_str("complete")) {
                    if let Ok(f) = comp_fn.dyn_into::<js_sys::Function>() {
                        let _ = f.call0(&JsValue::NULL);
                    }
                }
                let sub_res = js_sys::Object::new();
                let empty_unsub = Closure::wrap(Box::new(|| {}) as Box<dyn Fn()>);
                let _ = js_sys::Reflect::set(&sub_res, &JsValue::from_str("unsubscribe"), &empty_unsub.into_js_value());
                return Ok(sub_res.into());
            }

            let my_id = NEXT_TRANSITION_SUB_ID.fetch_add(1, Ordering::Relaxed);
            subscribers_for_sub.borrow_mut().push((my_id, obs_obj));

            let subscribers_weak = Rc::downgrade(&subscribers_for_sub);
            let unsub_closure = Closure::wrap(Box::new(move || {
                if let Some(subscribers_rc) = subscribers_weak.upgrade() {
                    if let Ok(mut subs) = subscribers_rc.try_borrow_mut() {
                        subs.retain(|(id, _)| *id != my_id);
                    }
                }
            }) as Box<dyn FnMut()>);

            let sub_res = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&sub_res, &JsValue::from_str("unsubscribe"), &unsub_closure.into_js_value());
            Ok(sub_res.into())
        }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("subscribe"), sub_closure.as_ref());
        sub_closure.forget();

        // start: () => actorRef
        let actor_for_start = actor_obj.clone();
        let is_started_for_start = is_started.clone();
        let is_stopped_for_start = is_stopped.clone();
        let start_closure = Closure::wrap(Box::new(move || -> JsValue {
            if !*is_stopped_for_start.borrow() {
                *is_started_for_start.borrow_mut() = true;
            }
            actor_for_start.clone().into()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("start"), start_closure.as_ref());
        start_closure.forget();

        // stop: () => actorRef
        let actor_for_stop = actor_obj.clone();
        let is_stopped_for_stop = is_stopped.clone();
        let current_snap_for_stop = current_snap.clone();
        let subscribers_for_stop = subscribers.clone();
        let stop_closure = Closure::wrap(Box::new(move || -> JsValue {
            if *is_stopped_for_stop.borrow() {
                return actor_for_stop.clone().into();
            }
            *is_stopped_for_stop.borrow_mut() = true;

            // Transition current_snap status to 'stopped'
            let stopped_snap = js_sys::Object::new();
            let cur = current_snap_for_stop.borrow().clone();
            if cur.is_object() && !cur.is_null() {
                let cur_obj = js_sys::Object::from(cur);
                let keys = js_sys::Object::keys(&cur_obj);
                for i in 0..keys.length() {
                    let k = keys.get(i);
                    if let Ok(v) = js_sys::Reflect::get(&cur_obj, &k) {
                        let _ = js_sys::Reflect::set(&stopped_snap, &k, &v);
                    }
                }
            }
            let _ = js_sys::Reflect::set(&stopped_snap, &JsValue::from_str("status"), &JsValue::from_str("stopped"));
            *current_snap_for_stop.borrow_mut() = stopped_snap.into();

            // Notify complete on observers
            let observers: Vec<js_sys::Object> = subscribers_for_stop
                .borrow()
                .iter()
                .map(|(_, o)| o.clone())
                .collect();
            for obs in observers {
                if let Ok(comp_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("complete")) {
                    if let Ok(f) = comp_fn.dyn_into::<js_sys::Function>() {
                        let _ = f.call0(&JsValue::NULL);
                    }
                }
            }

            subscribers_for_stop.borrow_mut().clear();

            actor_for_stop.clone().into()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("stop"), stop_closure.as_ref());
        stop_closure.forget();

        Ok(actor_obj.into())
    }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("createActor"), create_actor_closure.as_ref());
    create_actor_closure.forget();

    Ok(logic_obj.into())
}

#[wasm_bindgen(js_name = fromPromise)]
pub fn from_promise(promise_creator: JsValue) -> Result<JsValue, JsValue> {
    let logic_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("config"), &promise_creator);

    let get_init_closure = Closure::wrap(Box::new(|_scope: JsValue, input: JsValue| -> Result<JsValue, JsValue> {
        let snap_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("status"), &JsValue::from_str("active"));
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("output"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("error"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("input"), &input);
        Ok(snap_obj.into())
    }) as Box<dyn Fn(JsValue, JsValue) -> Result<JsValue, JsValue>>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("getInitialSnapshot"), get_init_closure.as_ref());
    get_init_closure.forget();

    let trans_closure = Closure::wrap(Box::new(|snapshot: JsValue, _event: JsValue, _scope: JsValue| -> Result<JsValue, JsValue> {
        Ok(snapshot)
    }) as Box<dyn Fn(JsValue, JsValue, JsValue) -> Result<JsValue, JsValue>>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("transition"), trans_closure.as_ref());
    trans_closure.forget();

    let get_persisted_closure = Closure::wrap(Box::new(|snapshot: JsValue| -> JsValue { snapshot }) as Box<dyn Fn(JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("getPersistedSnapshot"), get_persisted_closure.as_ref());
    get_persisted_closure.forget();

    let restore_closure = Closure::wrap(Box::new(|snapshot: JsValue| -> JsValue { snapshot }) as Box<dyn Fn(JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("restoreSnapshot"), restore_closure.as_ref());
    restore_closure.forget();

    let creator_for_actor = promise_creator.clone();
    let create_actor_closure = Closure::wrap(Box::new(move |options: JsValue| -> Result<JsValue, JsValue> {
        let actor_obj = js_sys::Object::new();
        let (custom_id, parent_ref, system_ref) = if options.is_object() && !options.is_null() {
            let id = js_sys::Reflect::get(&options, &JsValue::from_str("id"))
                .ok()
                .and_then(|v| v.as_string());
            let parent = js_sys::Reflect::get(&options, &JsValue::from_str("parent")).unwrap_or(JsValue::UNDEFINED);
            let system = js_sys::Reflect::get(&options, &JsValue::from_str("system")).unwrap_or(JsValue::UNDEFINED);
            (id, parent, system)
        } else {
            (None, JsValue::UNDEFINED, JsValue::UNDEFINED)
        };

        let session_id = crate::actor::next_session_id();
        let id_str = custom_id.unwrap_or_else(|| session_id.clone());
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("id"), &JsValue::from_str(&id_str));
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("sessionId"), &JsValue::from_str(&session_id));
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("system"), &system_ref);

        let to_json_fn = js_sys::Function::new_no_args("return { id: this.id, xstate$$type: 1 };");
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("toJSON"), &to_json_fn);

        let resolved_input = resolve_input_option(&options, &actor_obj);
        let init_snap = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("status"), &JsValue::from_str("active"));
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("output"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("error"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("input"), &resolved_input);
        let current_snap: Rc<RefCell<JsValue>> = Rc::new(RefCell::new(init_snap.into()));

        let is_started = Rc::new(RefCell::new(false));
        let is_stopped = Rc::new(RefCell::new(false));
        let subscribers = Rc::new(RefCell::new(Vec::<(u32, js_sys::Object)>::new()));

        let current_snap_for_get = current_snap.clone();
        let get_snap_closure = Closure::wrap(Box::new(move || -> JsValue {
            current_snap_for_get.borrow().clone()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("getSnapshot"), get_snap_closure.as_ref());
        get_snap_closure.forget();

        let subscribers_for_sub = subscribers.clone();
        let is_stopped_for_sub = is_stopped.clone();
        let current_snap_for_sub = current_snap.clone();
        let sub_closure = Closure::wrap(Box::new(move |obs: JsValue| -> Result<JsValue, JsValue> {
            let obs_obj = if let Ok(f) = obs.clone().dyn_into::<js_sys::Function>() {
                let o = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&o, &JsValue::from_str("next"), &f);
                o
            } else if obs.is_object() && !obs.is_null() {
                js_sys::Object::from(obs)
            } else {
                js_sys::Object::new()
            };

            let cur = current_snap_for_sub.borrow().clone();
            let status = js_sys::Reflect::get(&cur, &JsValue::from_str("status"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();

            if status == "done" || *is_stopped_for_sub.borrow() {
                if let Ok(next_fn) = js_sys::Reflect::get(&obs_obj, &JsValue::from_str("next")) {
                    if let Ok(f) = next_fn.dyn_into::<js_sys::Function>() {
                        let _ = f.call1(&JsValue::NULL, &cur);
                    }
                }
                if let Ok(comp_fn) = js_sys::Reflect::get(&obs_obj, &JsValue::from_str("complete")) {
                    if let Ok(f) = comp_fn.dyn_into::<js_sys::Function>() {
                        let _ = f.call0(&JsValue::NULL);
                    }
                }
                let sub_res = js_sys::Object::new();
                let empty_unsub = Closure::wrap(Box::new(|| {}) as Box<dyn Fn()>);
                let _ = js_sys::Reflect::set(&sub_res, &JsValue::from_str("unsubscribe"), &empty_unsub.into_js_value());
                return Ok(sub_res.into());
            }

            let my_id = NEXT_TRANSITION_SUB_ID.fetch_add(1, Ordering::Relaxed);
            subscribers_for_sub.borrow_mut().push((my_id, obs_obj));

            let subscribers_weak = Rc::downgrade(&subscribers_for_sub);
            let unsub_closure = Closure::wrap(Box::new(move || {
                if let Some(subscribers_rc) = subscribers_weak.upgrade() {
                    if let Ok(mut subs) = subscribers_rc.try_borrow_mut() {
                        subs.retain(|(id, _)| *id != my_id);
                    }
                }
            }) as Box<dyn FnMut()>);

            let sub_res = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&sub_res, &JsValue::from_str("unsubscribe"), &unsub_closure.into_js_value());
            Ok(sub_res.into())
        }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("subscribe"), sub_closure.as_ref());
        sub_closure.forget();

        let send_closure = Closure::wrap(Box::new(move |_evt: JsValue| -> Result<JsValue, JsValue> {
            Ok(JsValue::UNDEFINED)
        }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("send"), send_closure.as_ref());
        send_closure.forget();

        let actor_for_stop = actor_obj.clone();
        let is_stopped_for_stop = is_stopped.clone();
        let current_snap_for_stop = current_snap.clone();
        let subscribers_for_stop = subscribers.clone();
        let stop_closure = Closure::wrap(Box::new(move || -> JsValue {
            if *is_stopped_for_stop.borrow() {
                return actor_for_stop.clone().into();
            }
            *is_stopped_for_stop.borrow_mut() = true;

            let stopped_snap = js_sys::Object::new();
            let cur = current_snap_for_stop.borrow().clone();
            if cur.is_object() && !cur.is_null() {
                let cur_obj = js_sys::Object::from(cur);
                let keys = js_sys::Object::keys(&cur_obj);
                for i in 0..keys.length() {
                    let k = keys.get(i);
                    if let Ok(v) = js_sys::Reflect::get(&cur_obj, &k) {
                        let _ = js_sys::Reflect::set(&stopped_snap, &k, &v);
                    }
                }
            }
            let _ = js_sys::Reflect::set(&stopped_snap, &JsValue::from_str("status"), &JsValue::from_str("stopped"));
            *current_snap_for_stop.borrow_mut() = stopped_snap.into();

            let observers: Vec<js_sys::Object> = subscribers_for_stop
                .borrow()
                .iter()
                .map(|(_, o)| o.clone())
                .collect();
            for obs in observers {
                if let Ok(comp_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("complete")) {
                    if let Ok(f) = comp_fn.dyn_into::<js_sys::Function>() {
                        let _ = f.call0(&JsValue::NULL);
                    }
                }
            }
            subscribers_for_stop.borrow_mut().clear();
            actor_for_stop.clone().into()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("stop"), stop_closure.as_ref());
        stop_closure.forget();

        let actor_for_start = actor_obj.clone();
        let is_started_for_start = is_started.clone();
        let is_stopped_for_start = is_stopped.clone();
        let current_snap_for_start = current_snap.clone();
        let subscribers_for_start = subscribers.clone();
        let creator_for_start = creator_for_actor.clone();
        let resolved_input_for_start = resolved_input.clone();
        let parent_for_start = parent_ref.clone();
        let system_for_start = system_ref.clone();
        let id_for_start = id_str.clone();

        let start_closure = Closure::wrap(Box::new(move || -> JsValue {
            if *is_started_for_start.borrow() || *is_stopped_for_start.borrow() {
                return actor_for_start.clone().into();
            }
            *is_started_for_start.borrow_mut() = true;

            system_register(&system_for_start, &actor_for_start);
            let init_evt = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&init_evt, &JsValue::from_str("type"), &JsValue::from_str("xstate.init"));
            let _ = js_sys::Reflect::set(&init_evt, &JsValue::from_str("input"), &resolved_input_for_start);
            system_relay(&system_for_start, &parent_for_start, &actor_for_start, &init_evt);
            system_snapshot(&system_for_start, &actor_for_start, &init_evt, &current_snap_for_start.borrow(), "active");

            let arg_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("input"), &resolved_input_for_start);
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &actor_for_start);
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("system"), &system_for_start);

            let promise_res = if let Ok(f) = creator_for_start.clone().dyn_into::<js_sys::Function>() {
                f.call1(&JsValue::NULL, &arg_obj)
            } else {
                Ok(JsValue::UNDEFINED)
            };

            if let Ok(promise_val) = promise_res {
                let promise = js_sys::Promise::resolve(&promise_val);

                let is_stopped_then = is_stopped_for_start.clone();
                let current_snap_then = current_snap_for_start.clone();
                let subscribers_then = subscribers_for_start.clone();
                let actor_then = actor_for_start.clone();
                let parent_then = parent_for_start.clone();
                let system_then = system_for_start.clone();
                let id_then = id_for_start.clone();
                let resolved_inp_then = resolved_input_for_start.clone();

                let on_fulfilled = Closure::wrap(Box::new(move |val: JsValue| {
                    if *is_stopped_then.borrow() {
                        return;
                    }
                    let next_snap = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("status"), &JsValue::from_str("done"));
                    let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("output"), &val);
                    let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("error"), &JsValue::UNDEFINED);
                    let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("input"), &resolved_inp_then);
                    let snap_js: JsValue = next_snap.into();
                    *current_snap_then.borrow_mut() = snap_js.clone();

                    let resolve_evt = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&resolve_evt, &JsValue::from_str("type"), &JsValue::from_str("xstate.promise.resolve"));
                    let _ = js_sys::Reflect::set(&resolve_evt, &JsValue::from_str("data"), &val);
                    system_relay(&system_then, &actor_then, &actor_then, &resolve_evt);

                    if !parent_then.is_undefined() && !parent_then.is_null() {
                        let done_evt = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("type"), &JsValue::from_str(&format!("xstate.done.actor.{}", id_then)));
                        let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("output"), &val);
                        let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("actorId"), &JsValue::from_str(&id_then));
                        system_relay(&system_then, &actor_then, &parent_then, &done_evt);
                    }

                    system_snapshot(&system_then, &actor_then, &resolve_evt, &snap_js, "done");

                    let observers: Vec<js_sys::Object> = subscribers_then
                        .borrow()
                        .iter()
                        .map(|(_, o)| o.clone())
                        .collect();
                    for obs in observers {
                        if let Ok(next_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("next")) {
                            if let Ok(f) = next_fn.dyn_into::<js_sys::Function>() {
                                let _ = f.call1(&JsValue::NULL, &snap_js);
                            }
                        }
                        if let Ok(comp_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("complete")) {
                            if let Ok(f) = comp_fn.dyn_into::<js_sys::Function>() {
                                let _ = f.call0(&JsValue::NULL);
                            }
                        }
                    }
                    subscribers_then.borrow_mut().clear();
                }) as Box<dyn FnMut(JsValue)>);

                let is_stopped_catch = is_stopped_for_start.clone();
                let current_snap_catch = current_snap_for_start.clone();
                let subscribers_catch = subscribers_for_start.clone();
                let actor_catch = actor_for_start.clone();
                let parent_catch = parent_for_start.clone();
                let system_catch = system_for_start.clone();
                let id_catch = id_for_start.clone();
                let resolved_inp_catch = resolved_input_for_start.clone();

                let on_rejected = Closure::wrap(Box::new(move |err: JsValue| {
                    if *is_stopped_catch.borrow() {
                        return;
                    }
                    let next_snap = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("status"), &JsValue::from_str("error"));
                    let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("output"), &JsValue::UNDEFINED);
                    let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("error"), &err);
                    let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("input"), &resolved_inp_catch);
                    let snap_js: JsValue = next_snap.into();
                    *current_snap_catch.borrow_mut() = snap_js.clone();

                    if !parent_catch.is_undefined() && !parent_catch.is_null() {
                        let err_evt = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(&err_evt, &JsValue::from_str("type"), &JsValue::from_str(&format!("xstate.error.actor.{}", id_catch)));
                        let _ = js_sys::Reflect::set(&err_evt, &JsValue::from_str("error"), &err);
                        let _ = js_sys::Reflect::set(&err_evt, &JsValue::from_str("actorId"), &JsValue::from_str(&id_catch));
                        system_relay(&system_catch, &actor_catch, &parent_catch, &err_evt);
                    }

                    let observers: Vec<js_sys::Object> = subscribers_catch
                        .borrow()
                        .iter()
                        .map(|(_, o)| o.clone())
                        .collect();
                    for obs in observers {
                        if let Ok(next_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("next")) {
                            if let Ok(f) = next_fn.dyn_into::<js_sys::Function>() {
                                let _ = f.call1(&JsValue::NULL, &snap_js);
                            }
                        }
                        if let Ok(err_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("error")) {
                            if let Ok(f) = err_fn.dyn_into::<js_sys::Function>() {
                                let _ = f.call1(&JsValue::NULL, &err);
                            }
                        }
                    }
                    subscribers_catch.borrow_mut().clear();
                }) as Box<dyn FnMut(JsValue)>);

                let _ = promise.then2(&on_fulfilled, &on_rejected);
                on_fulfilled.forget();
                on_rejected.forget();
            }

            actor_for_start.clone().into()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("start"), start_closure.as_ref());
        start_closure.forget();

        Ok(actor_obj.into())
    }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);

    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("createActor"), create_actor_closure.as_ref());
    create_actor_closure.forget();

    Ok(logic_obj.into())
}

#[wasm_bindgen(js_name = fromObservable)]
pub fn from_observable(observable_creator: JsValue) -> Result<JsValue, JsValue> {
    let logic_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("config"), &observable_creator);

    let get_init_closure = Closure::wrap(Box::new(|_scope: JsValue, input: JsValue| -> Result<JsValue, JsValue> {
        let snap_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("status"), &JsValue::from_str("active"));
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("output"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("error"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("context"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("input"), &input);
        Ok(snap_obj.into())
    }) as Box<dyn Fn(JsValue, JsValue) -> Result<JsValue, JsValue>>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("getInitialSnapshot"), get_init_closure.as_ref());
    get_init_closure.forget();

    let trans_closure = Closure::wrap(Box::new(|snapshot: JsValue, _event: JsValue, _scope: JsValue| -> Result<JsValue, JsValue> {
        Ok(snapshot)
    }) as Box<dyn Fn(JsValue, JsValue, JsValue) -> Result<JsValue, JsValue>>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("transition"), trans_closure.as_ref());
    trans_closure.forget();

    let get_persisted_closure = Closure::wrap(Box::new(|snapshot: JsValue| -> JsValue { snapshot }) as Box<dyn Fn(JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("getPersistedSnapshot"), get_persisted_closure.as_ref());
    get_persisted_closure.forget();

    let restore_closure = Closure::wrap(Box::new(|snapshot: JsValue| -> JsValue { snapshot }) as Box<dyn Fn(JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("restoreSnapshot"), restore_closure.as_ref());
    restore_closure.forget();

    let creator_for_actor = observable_creator.clone();
    let create_actor_closure = Closure::wrap(Box::new(move |options: JsValue| -> Result<JsValue, JsValue> {
        let actor_obj = js_sys::Object::new();
        let (custom_id, parent_ref, system_ref) = if options.is_object() && !options.is_null() {
            let id = js_sys::Reflect::get(&options, &JsValue::from_str("id"))
                .ok()
                .and_then(|v| v.as_string());
            let parent = js_sys::Reflect::get(&options, &JsValue::from_str("parent")).unwrap_or(JsValue::UNDEFINED);
            let system = js_sys::Reflect::get(&options, &JsValue::from_str("system")).unwrap_or(JsValue::UNDEFINED);
            (id, parent, system)
        } else {
            (None, JsValue::UNDEFINED, JsValue::UNDEFINED)
        };

        let session_id = crate::actor::next_session_id();
        let id_str = custom_id.unwrap_or_else(|| session_id.clone());
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("id"), &JsValue::from_str(&id_str));
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("sessionId"), &JsValue::from_str(&session_id));
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("system"), &system_ref);

        let to_json_fn = js_sys::Function::new_no_args("return { id: this.id, xstate$$type: 1 };");
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("toJSON"), &to_json_fn);

        let resolved_input = resolve_input_option(&options, &actor_obj);
        let init_snap = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("status"), &JsValue::from_str("active"));
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("output"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("error"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("context"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("input"), &resolved_input);
        let current_snap: Rc<RefCell<JsValue>> = Rc::new(RefCell::new(init_snap.into()));

        let is_started = Rc::new(RefCell::new(false));
        let is_stopped = Rc::new(RefCell::new(false));
        let subscription_handle = Rc::new(RefCell::new(JsValue::UNDEFINED));
        let subscribers = Rc::new(RefCell::new(Vec::<(u32, js_sys::Object)>::new()));

        let current_snap_for_get = current_snap.clone();
        let get_snap_closure = Closure::wrap(Box::new(move || -> JsValue {
            current_snap_for_get.borrow().clone()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("getSnapshot"), get_snap_closure.as_ref());
        get_snap_closure.forget();

        let subscribers_for_sub = subscribers.clone();
        let is_stopped_for_sub = is_stopped.clone();
        let current_snap_for_sub = current_snap.clone();
        let sub_closure = Closure::wrap(Box::new(move |obs: JsValue| -> Result<JsValue, JsValue> {
            let obs_obj = if let Ok(f) = obs.clone().dyn_into::<js_sys::Function>() {
                let o = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&o, &JsValue::from_str("next"), &f);
                o
            } else if obs.is_object() && !obs.is_null() {
                js_sys::Object::from(obs)
            } else {
                js_sys::Object::new()
            };

            let cur = current_snap_for_sub.borrow().clone();
            let status = js_sys::Reflect::get(&cur, &JsValue::from_str("status"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();

            if status == "done" || *is_stopped_for_sub.borrow() {
                if let Ok(next_fn) = js_sys::Reflect::get(&obs_obj, &JsValue::from_str("next")) {
                    if let Ok(f) = next_fn.dyn_into::<js_sys::Function>() {
                        let _ = f.call1(&JsValue::NULL, &cur);
                    }
                }
                if let Ok(comp_fn) = js_sys::Reflect::get(&obs_obj, &JsValue::from_str("complete")) {
                    if let Ok(f) = comp_fn.dyn_into::<js_sys::Function>() {
                        let _ = f.call0(&JsValue::NULL);
                    }
                }
                let sub_res = js_sys::Object::new();
                let empty_unsub = Closure::wrap(Box::new(|| {}) as Box<dyn Fn()>);
                let _ = js_sys::Reflect::set(&sub_res, &JsValue::from_str("unsubscribe"), &empty_unsub.into_js_value());
                return Ok(sub_res.into());
            }

            let my_id = NEXT_TRANSITION_SUB_ID.fetch_add(1, Ordering::Relaxed);
            subscribers_for_sub.borrow_mut().push((my_id, obs_obj));

            let subscribers_weak = Rc::downgrade(&subscribers_for_sub);
            let unsub_closure = Closure::wrap(Box::new(move || {
                if let Some(subscribers_rc) = subscribers_weak.upgrade() {
                    if let Ok(mut subs) = subscribers_rc.try_borrow_mut() {
                        subs.retain(|(id, _)| *id != my_id);
                    }
                }
            }) as Box<dyn FnMut()>);

            let sub_res = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&sub_res, &JsValue::from_str("unsubscribe"), &unsub_closure.into_js_value());
            Ok(sub_res.into())
        }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("subscribe"), sub_closure.as_ref());
        sub_closure.forget();

        let actor_for_stop = actor_obj.clone();
        let is_stopped_for_stop = is_stopped.clone();
        let sub_handle_for_stop = subscription_handle.clone();
        let current_snap_for_stop = current_snap.clone();
        let subscribers_for_stop = subscribers.clone();
        let stop_closure = Closure::wrap(Box::new(move || -> JsValue {
            if *is_stopped_for_stop.borrow() {
                return actor_for_stop.clone().into();
            }
            *is_stopped_for_stop.borrow_mut() = true;

            let handle = sub_handle_for_stop.borrow().clone();
            if handle.is_object() && !handle.is_null() {
                if let Ok(unsub) = js_sys::Reflect::get(&handle, &JsValue::from_str("unsubscribe")) {
                    if let Ok(f) = unsub.dyn_into::<js_sys::Function>() {
                        let _ = f.call0(&handle);
                    }
                }
            }

            let stopped_snap = js_sys::Object::new();
            let cur = current_snap_for_stop.borrow().clone();
            if cur.is_object() && !cur.is_null() {
                let cur_obj = js_sys::Object::from(cur);
                let keys = js_sys::Object::keys(&cur_obj);
                for i in 0..keys.length() {
                    let k = keys.get(i);
                    if let Ok(v) = js_sys::Reflect::get(&cur_obj, &k) {
                        let _ = js_sys::Reflect::set(&stopped_snap, &k, &v);
                    }
                }
            }
            let _ = js_sys::Reflect::set(&stopped_snap, &JsValue::from_str("status"), &JsValue::from_str("stopped"));
            *current_snap_for_stop.borrow_mut() = stopped_snap.into();

            let observers: Vec<js_sys::Object> = subscribers_for_stop
                .borrow()
                .iter()
                .map(|(_, o)| o.clone())
                .collect();
            for obs in observers {
                if let Ok(comp_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("complete")) {
                    if let Ok(f) = comp_fn.dyn_into::<js_sys::Function>() {
                        let _ = f.call0(&JsValue::NULL);
                    }
                }
            }
            subscribers_for_stop.borrow_mut().clear();
            actor_for_stop.clone().into()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("stop"), stop_closure.as_ref());
        stop_closure.forget();

        let actor_for_start = actor_obj.clone();
        let is_started_for_start = is_started.clone();
        let is_stopped_for_start = is_stopped.clone();
        let current_snap_for_start = current_snap.clone();
        let subscribers_for_start = subscribers.clone();
        let creator_for_start = creator_for_actor.clone();
        let resolved_input_for_start = resolved_input.clone();
        let parent_for_start = parent_ref.clone();
        let system_for_start = system_ref.clone();
        let id_for_start = id_str.clone();
        let sub_handle_for_start = subscription_handle.clone();

        let start_closure = Closure::wrap(Box::new(move || -> JsValue {
            if *is_started_for_start.borrow() || *is_stopped_for_start.borrow() {
                return actor_for_start.clone().into();
            }
            *is_started_for_start.borrow_mut() = true;

            system_register(&system_for_start, &actor_for_start);
            let init_evt = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&init_evt, &JsValue::from_str("type"), &JsValue::from_str("xstate.init"));
            let _ = js_sys::Reflect::set(&init_evt, &JsValue::from_str("input"), &resolved_input_for_start);
            system_relay(&system_for_start, &parent_for_start, &actor_for_start, &init_evt);
            system_snapshot(&system_for_start, &actor_for_start, &init_evt, &current_snap_for_start.borrow(), "active");

            let arg_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("input"), &resolved_input_for_start);
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &actor_for_start);
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("system"), &system_for_start);

            let obs_val = if let Ok(f) = creator_for_start.clone().dyn_into::<js_sys::Function>() {
                f.call1(&JsValue::NULL, &arg_obj).unwrap_or(JsValue::UNDEFINED)
            } else {
                JsValue::UNDEFINED
            };

            if obs_val.is_object() && !obs_val.is_null() {
                let sub_fn_opt = js_sys::Reflect::get(&obs_val, &JsValue::from_str("subscribe"))
                    .ok()
                    .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
                if let Some(sub_fn) = sub_fn_opt {
                    let is_stopped_obs = is_stopped_for_start.clone();
                    let current_snap_obs = current_snap_for_start.clone();
                    let subscribers_obs = subscribers_for_start.clone();
                    let parent_obs = parent_for_start.clone();
                    let system_obs = system_for_start.clone();
                    let actor_obs = actor_for_start.clone();
                    let id_obs = id_for_start.clone();
                    let resolved_inp_next = resolved_input_for_start.clone();
                    let resolved_inp_comp = resolved_input_for_start.clone();
                    let resolved_inp_err = resolved_input_for_start.clone();

                    let observer = js_sys::Object::new();
                    let next_closure = Closure::wrap(Box::new(move |val: JsValue| {
                        if *is_stopped_obs.borrow() { return; }
                        let next_snap = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("status"), &JsValue::from_str("active"));
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("output"), &JsValue::UNDEFINED);
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("error"), &JsValue::UNDEFINED);
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("context"), &val);
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("input"), &resolved_inp_next);
                        let snap_js: JsValue = next_snap.into();
                        *current_snap_obs.borrow_mut() = snap_js.clone();

                        let observers: Vec<js_sys::Object> = subscribers_obs
                            .borrow()
                            .iter()
                            .map(|(_, o)| o.clone())
                            .collect();
                        for obs in observers {
                            if let Ok(next_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("next")) {
                                if let Ok(f) = next_fn.dyn_into::<js_sys::Function>() {
                                    let _ = f.call1(&JsValue::NULL, &snap_js);
                                }
                            }
                        }
                    }) as Box<dyn FnMut(JsValue)>);
                    let _ = js_sys::Reflect::set(&observer, &JsValue::from_str("next"), next_closure.as_ref());
                    next_closure.forget();

                    let is_stopped_comp = is_stopped_for_start.clone();
                    let current_snap_comp = current_snap_for_start.clone();
                    let subscribers_comp = subscribers_for_start.clone();
                    let comp_closure = Closure::wrap(Box::new(move || {
                        if *is_stopped_comp.borrow() { return; }
                        let cur_ctx = js_sys::Reflect::get(&current_snap_comp.borrow(), &JsValue::from_str("context")).unwrap_or(JsValue::UNDEFINED);
                        let next_snap = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("status"), &JsValue::from_str("done"));
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("output"), &cur_ctx);
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("error"), &JsValue::UNDEFINED);
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("context"), &cur_ctx);
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("input"), &resolved_inp_comp);
                        let snap_js: JsValue = next_snap.into();
                        *current_snap_comp.borrow_mut() = snap_js.clone();

                        if !parent_obs.is_undefined() && !parent_obs.is_null() {
                            let done_evt = js_sys::Object::new();
                            let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("type"), &JsValue::from_str(&format!("xstate.done.actor.{}", id_obs)));
                            let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("output"), &cur_ctx);
                            let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("actorId"), &JsValue::from_str(&id_obs));
                            system_relay(&system_obs, &actor_obs, &parent_obs, &done_evt);
                        }

                        let observers: Vec<js_sys::Object> = subscribers_comp
                            .borrow()
                            .iter()
                            .map(|(_, o)| o.clone())
                            .collect();
                        for obs in observers {
                            if let Ok(next_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("next")) {
                                if let Ok(f) = next_fn.dyn_into::<js_sys::Function>() {
                                    let _ = f.call1(&JsValue::NULL, &snap_js);
                                }
                            }
                            if let Ok(comp_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("complete")) {
                                if let Ok(f) = comp_fn.dyn_into::<js_sys::Function>() {
                                    let _ = f.call0(&JsValue::NULL);
                                }
                            }
                        }
                        subscribers_comp.borrow_mut().clear();
                    }) as Box<dyn FnMut()>);
                    let _ = js_sys::Reflect::set(&observer, &JsValue::from_str("complete"), comp_closure.as_ref());
                    comp_closure.forget();

                    let is_stopped_err = is_stopped_for_start.clone();
                    let current_snap_err = current_snap_for_start.clone();
                    let subscribers_err = subscribers_for_start.clone();
                    let err_closure = Closure::wrap(Box::new(move |err: JsValue| {
                        if *is_stopped_err.borrow() { return; }
                        let cur_ctx = js_sys::Reflect::get(&current_snap_err.borrow(), &JsValue::from_str("context")).unwrap_or(JsValue::UNDEFINED);
                        let next_snap = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("status"), &JsValue::from_str("error"));
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("output"), &JsValue::UNDEFINED);
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("error"), &err);
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("context"), &cur_ctx);
                        let _ = js_sys::Reflect::set(&next_snap, &JsValue::from_str("input"), &resolved_inp_err);
                        let snap_js: JsValue = next_snap.into();
                        *current_snap_err.borrow_mut() = snap_js.clone();

                        let observers: Vec<js_sys::Object> = subscribers_err
                            .borrow()
                            .iter()
                            .map(|(_, o)| o.clone())
                            .collect();
                        for obs in observers {
                            if let Ok(err_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("error")) {
                                if let Ok(f) = err_fn.dyn_into::<js_sys::Function>() {
                                    let _ = f.call1(&JsValue::NULL, &err);
                                }
                            }
                        }
                        subscribers_err.borrow_mut().clear();
                    }) as Box<dyn FnMut(JsValue)>);
                    let _ = js_sys::Reflect::set(&observer, &JsValue::from_str("error"), err_closure.as_ref());
                    err_closure.forget();

                    if let Ok(sub_res) = sub_fn.call1(&obs_val, &observer) {
                        *sub_handle_for_start.borrow_mut() = sub_res;
                    }
                }
            }

            actor_for_start.clone().into()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("start"), start_closure.as_ref());
        start_closure.forget();

        Ok(actor_obj.into())
    }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);

    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("createActor"), create_actor_closure.as_ref());
    create_actor_closure.forget();

    Ok(logic_obj.into())
}

#[wasm_bindgen(js_name = fromCallback)]
pub fn from_callback(callback_fn: JsValue) -> Result<JsValue, JsValue> {
    let logic_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("config"), &callback_fn);

    let get_init_closure = Closure::wrap(Box::new(|_scope: JsValue, input: JsValue| -> Result<JsValue, JsValue> {
        let snap_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("status"), &JsValue::from_str("active"));
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("output"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("error"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("input"), &input);
        Ok(snap_obj.into())
    }) as Box<dyn Fn(JsValue, JsValue) -> Result<JsValue, JsValue>>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("getInitialSnapshot"), get_init_closure.as_ref());
    get_init_closure.forget();

    let trans_closure = Closure::wrap(Box::new(|snapshot: JsValue, _event: JsValue, _scope: JsValue| -> Result<JsValue, JsValue> {
        Ok(snapshot)
    }) as Box<dyn Fn(JsValue, JsValue, JsValue) -> Result<JsValue, JsValue>>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("transition"), trans_closure.as_ref());
    trans_closure.forget();

    let get_persisted_closure = Closure::wrap(Box::new(|snapshot: JsValue| -> JsValue { snapshot }) as Box<dyn Fn(JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("getPersistedSnapshot"), get_persisted_closure.as_ref());
    get_persisted_closure.forget();

    let restore_closure = Closure::wrap(Box::new(|snapshot: JsValue| -> JsValue { snapshot }) as Box<dyn Fn(JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("restoreSnapshot"), restore_closure.as_ref());
    restore_closure.forget();

    let cb_for_actor = callback_fn.clone();
    let create_actor_closure = Closure::wrap(Box::new(move |options: JsValue| -> Result<JsValue, JsValue> {
        let actor_obj = js_sys::Object::new();
        let (custom_id, parent_ref, system_ref) = if options.is_object() && !options.is_null() {
            let id = js_sys::Reflect::get(&options, &JsValue::from_str("id"))
                .ok()
                .and_then(|v| v.as_string());
            let parent = js_sys::Reflect::get(&options, &JsValue::from_str("parent")).unwrap_or(JsValue::UNDEFINED);
            let system = js_sys::Reflect::get(&options, &JsValue::from_str("system")).unwrap_or(JsValue::UNDEFINED);
            (id, parent, system)
        } else {
            (None, JsValue::UNDEFINED, JsValue::UNDEFINED)
        };

        let session_id = crate::actor::next_session_id();
        let id_str = custom_id.unwrap_or_else(|| session_id.clone());
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("id"), &JsValue::from_str(&id_str));
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("sessionId"), &JsValue::from_str(&session_id));
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("system"), &system_ref);

        let to_json_fn = js_sys::Function::new_no_args("return { id: this.id, xstate$$type: 1 };");
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("toJSON"), &to_json_fn);

        let resolved_input = resolve_input_option(&options, &actor_obj);
        let init_snap = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("status"), &JsValue::from_str("active"));
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("output"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("error"), &JsValue::UNDEFINED);
        let _ = js_sys::Reflect::set(&init_snap, &JsValue::from_str("input"), &resolved_input);
        let current_snap: Rc<RefCell<JsValue>> = Rc::new(RefCell::new(init_snap.into()));

        let is_started = Rc::new(RefCell::new(false));
        let is_stopped = Rc::new(RefCell::new(false));
        let cleanup_handle = Rc::new(RefCell::new(JsValue::UNDEFINED));
        let receivers = Rc::new(RefCell::new(Vec::<js_sys::Function>::new()));
        let subscribers = Rc::new(RefCell::new(Vec::<(u32, js_sys::Object)>::new()));

        let current_snap_for_get = current_snap.clone();
        let get_snap_closure = Closure::wrap(Box::new(move || -> JsValue {
            current_snap_for_get.borrow().clone()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("getSnapshot"), get_snap_closure.as_ref());
        get_snap_closure.forget();

        let subscribers_for_sub = subscribers.clone();
        let sub_closure = Closure::wrap(Box::new(move |obs: JsValue| -> Result<JsValue, JsValue> {
            let obs_obj = if let Ok(f) = obs.clone().dyn_into::<js_sys::Function>() {
                let o = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&o, &JsValue::from_str("next"), &f);
                o
            } else if obs.is_object() && !obs.is_null() {
                js_sys::Object::from(obs)
            } else {
                js_sys::Object::new()
            };

            let my_id = NEXT_TRANSITION_SUB_ID.fetch_add(1, Ordering::Relaxed);
            subscribers_for_sub.borrow_mut().push((my_id, obs_obj));

            let subscribers_weak = Rc::downgrade(&subscribers_for_sub);
            let unsub_closure = Closure::wrap(Box::new(move || {
                if let Some(subscribers_rc) = subscribers_weak.upgrade() {
                    if let Ok(mut subs) = subscribers_rc.try_borrow_mut() {
                        subs.retain(|(id, _)| *id != my_id);
                    }
                }
            }) as Box<dyn FnMut()>);

            let sub_res = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&sub_res, &JsValue::from_str("unsubscribe"), &unsub_closure.into_js_value());
            Ok(sub_res.into())
        }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("subscribe"), sub_closure.as_ref());
        sub_closure.forget();

        let receivers_for_send = receivers.clone();
        let is_stopped_for_send = is_stopped.clone();
        let send_closure = Closure::wrap(Box::new(move |evt: JsValue| -> Result<JsValue, JsValue> {
            if *is_stopped_for_send.borrow() { return Ok(JsValue::UNDEFINED); }
            let r_list = receivers_for_send.borrow().clone();
            for r in r_list {
                let _ = r.call1(&JsValue::NULL, &evt);
            }
            Ok(JsValue::UNDEFINED)
        }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("send"), send_closure.as_ref());
        send_closure.forget();

        let actor_for_stop = actor_obj.clone();
        let is_stopped_for_stop = is_stopped.clone();
        let cleanup_for_stop = cleanup_handle.clone();
        let current_snap_for_stop = current_snap.clone();
        let stop_closure = Closure::wrap(Box::new(move || -> JsValue {
            if *is_stopped_for_stop.borrow() {
                return actor_for_stop.clone().into();
            }
            *is_stopped_for_stop.borrow_mut() = true;

            let cleanup_val = cleanup_for_stop.borrow().clone();
            if let Ok(f) = cleanup_val.dyn_into::<js_sys::Function>() {
                let _ = f.call0(&JsValue::NULL);
            }

            let stopped_snap = js_sys::Object::new();
            let cur = current_snap_for_stop.borrow().clone();
            if cur.is_object() && !cur.is_null() {
                let cur_obj = js_sys::Object::from(cur);
                let keys = js_sys::Object::keys(&cur_obj);
                for i in 0..keys.length() {
                    let k = keys.get(i);
                    if let Ok(v) = js_sys::Reflect::get(&cur_obj, &k) {
                        let _ = js_sys::Reflect::set(&stopped_snap, &k, &v);
                    }
                }
            }
            let _ = js_sys::Reflect::set(&stopped_snap, &JsValue::from_str("status"), &JsValue::from_str("stopped"));
            *current_snap_for_stop.borrow_mut() = stopped_snap.into();

            actor_for_stop.clone().into()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("stop"), stop_closure.as_ref());
        stop_closure.forget();

        let actor_for_start = actor_obj.clone();
        let is_started_for_start = is_started.clone();
        let is_stopped_for_start = is_stopped.clone();
        let cb_for_start = cb_for_actor.clone();
        let resolved_input_for_start = resolved_input.clone();
        let parent_for_start = parent_ref.clone();
        let system_for_start = system_ref.clone();
        let receivers_for_start = receivers.clone();
        let cleanup_for_start = cleanup_handle.clone();
        let current_snap_for_start = current_snap.clone();

        let start_closure = Closure::wrap(Box::new(move || -> JsValue {
            if *is_started_for_start.borrow() || *is_stopped_for_start.borrow() {
                return actor_for_start.clone().into();
            }
            *is_started_for_start.borrow_mut() = true;

            system_register(&system_for_start, &actor_for_start);
            let init_evt = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&init_evt, &JsValue::from_str("type"), &JsValue::from_str("xstate.init"));
            let _ = js_sys::Reflect::set(&init_evt, &JsValue::from_str("input"), &resolved_input_for_start);
            system_relay(&system_for_start, &parent_for_start, &actor_for_start, &init_evt);
            system_snapshot(&system_for_start, &actor_for_start, &init_evt, &current_snap_for_start.borrow(), "active");

            let parent_for_sendback = parent_for_start.clone();
            let actor_for_sendback = actor_for_start.clone();
            let system_for_sendback = system_for_start.clone();
            let send_back_closure = Closure::wrap(Box::new(move |ev: JsValue| {
                if !parent_for_sendback.is_undefined() && !parent_for_sendback.is_null() {
                    system_relay(&system_for_sendback, &actor_for_sendback, &parent_for_sendback, &ev);
                }
            }) as Box<dyn FnMut(JsValue)>);

            let receivers_clone = receivers_for_start.clone();
            let receive_closure = Closure::wrap(Box::new(move |listener: JsValue| {
                if let Ok(f) = listener.dyn_into::<js_sys::Function>() {
                    receivers_clone.borrow_mut().push(f);
                }
            }) as Box<dyn FnMut(JsValue)>);

            let arg_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("input"), &resolved_input_for_start);
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &actor_for_start);
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("system"), &system_for_start);
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("sendBack"), send_back_closure.as_ref());
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("receive"), receive_closure.as_ref());
            send_back_closure.forget();
            receive_closure.forget();

            if let Ok(f) = cb_for_start.clone().dyn_into::<js_sys::Function>() {
                if let Ok(res) = f.call1(&JsValue::NULL, &arg_obj) {
                    *cleanup_for_start.borrow_mut() = res;
                }
            }

            actor_for_start.clone().into()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("start"), start_closure.as_ref());
        start_closure.forget();

        Ok(actor_obj.into())
    }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);

    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("createActor"), create_actor_closure.as_ref());
    create_actor_closure.forget();

    Ok(logic_obj.into())
}
