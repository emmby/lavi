use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::system::{
    next_session_id, resolve_input_option, system_register, system_relay, system_snapshot,
};

static NEXT_TRANSITION_SUB_ID: AtomicU32 = AtomicU32::new(1);

type SubscribersList = Rc<RefCell<Vec<(u32, js_sys::Object)>>>;

fn make_snapshot(
    status: &str,
    output: &JsValue,
    error: &JsValue,
    context: Option<&JsValue>,
    input: Option<&JsValue>,
) -> JsValue {
    let snap_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("status"), &JsValue::from_str(status));
    let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("output"), output);
    let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("error"), error);
    if let Some(ctx) = context {
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("context"), ctx);
    }
    if let Some(inp) = input {
        let _ = js_sys::Reflect::set(&snap_obj, &JsValue::from_str("input"), inp);
    }
    snap_obj.into()
}

fn clone_snapshot_with_field(snapshot: &JsValue, key: &str, val: &JsValue) -> JsValue {
    let next_snapshot = js_sys::Object::new();
    if snapshot.is_object() && !snapshot.is_null() {
        let snap_obj = js_sys::Object::from(snapshot.clone());
        let keys = js_sys::Object::keys(&snap_obj);
        for i in 0..keys.length() {
            let k = keys.get(i);
            if let Ok(v) = js_sys::Reflect::get(&snap_obj, &k) {
                let _ = js_sys::Reflect::set(&next_snapshot, &k, &v);
            }
        }
    }
    let _ = js_sys::Reflect::set(&next_snapshot, &JsValue::from_str(key), val);
    next_snapshot.into()
}

fn notify_next(subscribers: &SubscribersList, snap: &JsValue) {
    let observers: Vec<js_sys::Object> = subscribers
        .borrow()
        .iter()
        .map(|(_, o)| o.clone())
        .collect();
    for obs in observers {
        if let Ok(next_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("next")) {
            if let Ok(f) = next_fn.dyn_into::<js_sys::Function>() {
                let _ = f.call1(&JsValue::NULL, snap);
            }
        }
    }
}

fn notify_complete_and_clear(subscribers: &SubscribersList, snap: Option<&JsValue>) {
    let observers: Vec<js_sys::Object> = subscribers
        .borrow()
        .iter()
        .map(|(_, o)| o.clone())
        .collect();
    for obs in observers {
        if let Some(s) = snap {
            if let Ok(next_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("next")) {
                if let Ok(f) = next_fn.dyn_into::<js_sys::Function>() {
                    let _ = f.call1(&JsValue::NULL, s);
                }
            }
        }
        if let Ok(comp_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("complete")) {
            if let Ok(f) = comp_fn.dyn_into::<js_sys::Function>() {
                let _ = f.call0(&JsValue::NULL);
            }
        }
    }
    subscribers.borrow_mut().clear();
}

fn notify_error_and_clear(subscribers: &SubscribersList, snap: Option<&JsValue>, err: &JsValue) {
    let observers: Vec<js_sys::Object> = subscribers
        .borrow()
        .iter()
        .map(|(_, o)| o.clone())
        .collect();
    for obs in observers {
        if let Some(s) = snap {
            if let Ok(next_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("next")) {
                if let Ok(f) = next_fn.dyn_into::<js_sys::Function>() {
                    let _ = f.call1(&JsValue::NULL, s);
                }
            }
        }
        if let Ok(err_fn) = js_sys::Reflect::get(&obs, &JsValue::from_str("error")) {
            if let Ok(f) = err_fn.dyn_into::<js_sys::Function>() {
                let _ = f.call1(&JsValue::NULL, err);
            }
        }
    }
    subscribers.borrow_mut().clear();
}

struct LogicActorState {
    actor_obj: js_sys::Object,
    id_str: String,
    parent_ref: JsValue,
    system_ref: JsValue,
    resolved_input: JsValue,
    current_snap: Rc<RefCell<JsValue>>,
    is_started: Rc<RefCell<bool>>,
    is_stopped: Rc<RefCell<bool>>,
    subscribers: SubscribersList,
}

impl LogicActorState {
    fn new(
        options: &JsValue,
        initial_snap_fn: impl FnOnce(&JsValue, &js_sys::Object) -> Result<JsValue, JsValue>,
        check_done_on_subscribe: bool,
    ) -> Result<Self, JsValue> {
        let actor_obj = js_sys::Object::new();
        let (custom_id, parent_ref, system_ref) = if options.is_object() && !options.is_null() {
            let id = js_sys::Reflect::get(options, &JsValue::from_str("id"))
                .ok()
                .and_then(|v| v.as_string());
            let parent = js_sys::Reflect::get(options, &JsValue::from_str("parent")).unwrap_or(JsValue::UNDEFINED);
            let system = js_sys::Reflect::get(options, &JsValue::from_str("system")).unwrap_or(JsValue::UNDEFINED);
            (id, parent, system)
        } else {
            (None, JsValue::UNDEFINED, JsValue::UNDEFINED)
        };

        let session_id = next_session_id();
        let id_str = custom_id.unwrap_or_else(|| session_id.clone());
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("id"), &JsValue::from_str(&id_str));
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("sessionId"), &JsValue::from_str(&session_id));
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("system"), &system_ref);

        let to_json_fn = js_sys::Function::new_no_args("return { id: this.id, xstate$$type: 1 };");
        let _ = js_sys::Reflect::set(&actor_obj, &JsValue::from_str("toJSON"), &to_json_fn);

        let resolved_input = resolve_input_option(options, &actor_obj);
        let init_snap = initial_snap_fn(&resolved_input, &actor_obj)?;
        let current_snap = Rc::new(RefCell::new(init_snap));
        let is_started = Rc::new(RefCell::new(false));
        let is_stopped = Rc::new(RefCell::new(false));
        let subscribers: SubscribersList = Rc::new(RefCell::new(Vec::new()));

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
            let is_done = if check_done_on_subscribe {
                js_sys::Reflect::get(&cur, &JsValue::from_str("status"))
                    .ok()
                    .and_then(|v| v.as_string())
                    .as_deref()
                    == Some("done")
            } else {
                false
            };

            if is_done || *is_stopped_for_sub.borrow() {
                if check_done_on_subscribe {
                    if let Ok(next_fn) = js_sys::Reflect::get(&obs_obj, &JsValue::from_str("next")) {
                        if let Ok(f) = next_fn.dyn_into::<js_sys::Function>() {
                            let _ = f.call1(&JsValue::NULL, &cur);
                        }
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

        Ok(Self {
            actor_obj,
            id_str,
            parent_ref,
            system_ref,
            resolved_input,
            current_snap,
            is_started,
            is_stopped,
            subscribers,
        })
    }

    fn attach_stop(&self, notify_subscribers: bool, on_stop: Option<Box<dyn Fn()>>) {
        let actor_for_stop = self.actor_obj.clone();
        let is_stopped_for_stop = self.is_stopped.clone();
        let current_snap_for_stop = self.current_snap.clone();
        let subscribers_for_stop = self.subscribers.clone();
        let stop_closure = Closure::wrap(Box::new(move || -> JsValue {
            if *is_stopped_for_stop.borrow() {
                return actor_for_stop.clone().into();
            }
            *is_stopped_for_stop.borrow_mut() = true;

            if let Some(ref cleanup) = on_stop {
                cleanup();
            }

            let cur = current_snap_for_stop.borrow().clone();
            let stopped_snap = clone_snapshot_with_field(&cur, "status", &JsValue::from_str("stopped"));
            *current_snap_for_stop.borrow_mut() = stopped_snap;

            if notify_subscribers {
                notify_complete_and_clear(&subscribers_for_stop, None);
            }

            actor_for_stop.clone().into()
        }) as Box<dyn Fn() -> JsValue>);
        let _ = js_sys::Reflect::set(&self.actor_obj, &JsValue::from_str("stop"), stop_closure.as_ref());
        stop_closure.forget();
    }
}

fn register_and_emit_init(
    system: &JsValue,
    parent: &JsValue,
    actor: &JsValue,
    input: &JsValue,
    current_snap: &JsValue,
) {
    system_register(system, actor);
    let init_evt = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&init_evt, &JsValue::from_str("type"), &JsValue::from_str("xstate.init"));
    let _ = js_sys::Reflect::set(&init_evt, &JsValue::from_str("input"), input);
    system_relay(system, parent, actor, &init_evt);
    system_snapshot(system, actor, &init_evt, current_snap, "active");
}

fn emit_done_to_parent(system: &JsValue, actor: &JsValue, parent: &JsValue, actor_id: &str, output: &JsValue) {
    if !parent.is_undefined() && !parent.is_null() {
        let done_evt = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &done_evt,
            &JsValue::from_str("type"),
            &JsValue::from_str(&format!("xstate.done.actor.{}", actor_id)),
        );
        let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("output"), output);
        let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("actorId"), &JsValue::from_str(actor_id));
        system_relay(system, actor, parent, &done_evt);
    }
}

fn create_logic_object(
    config: &JsValue,
    get_init_fn: Box<dyn Fn(JsValue, JsValue) -> Result<JsValue, JsValue>>,
    transition_fn: Box<dyn Fn(JsValue, JsValue, JsValue) -> Result<JsValue, JsValue>>,
    create_actor_fn: impl FnOnce(js_sys::Object) -> Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>,
) -> JsValue {
    let logic_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("config"), config);

    let get_init_closure = Closure::wrap(get_init_fn);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("getInitialSnapshot"), get_init_closure.as_ref());
    get_init_closure.forget();

    let trans_closure = Closure::wrap(transition_fn);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("transition"), trans_closure.as_ref());
    trans_closure.forget();

    let get_persisted_closure = Closure::wrap(Box::new(|snapshot: JsValue| -> JsValue { snapshot }) as Box<dyn Fn(JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("getPersistedSnapshot"), get_persisted_closure.as_ref());
    get_persisted_closure.forget();

    let restore_closure = Closure::wrap(Box::new(|snapshot: JsValue| -> JsValue { snapshot }) as Box<dyn Fn(JsValue) -> JsValue>);
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("restoreSnapshot"), restore_closure.as_ref());
    restore_closure.forget();

    let create_actor_closure = Closure::wrap(create_actor_fn(logic_obj.clone()));
    let _ = js_sys::Reflect::set(&logic_obj, &JsValue::from_str("createActor"), create_actor_closure.as_ref());
    create_actor_closure.forget();

    logic_obj.into()
}

#[wasm_bindgen(js_name = fromTransition)]
pub fn from_transition(transition_fn: JsValue, initial_context: JsValue) -> Result<JsValue, JsValue> {
    let init_ctx_clone = initial_context.clone();
    let get_init = Box::new(move |actor_scope: JsValue, input: JsValue| -> Result<JsValue, JsValue> {
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
        Ok(make_snapshot("active", &JsValue::UNDEFINED, &JsValue::UNDEFINED, Some(&resolved_ctx), None))
    });

    let trans_fn_clone = transition_fn.clone();
    let trans = Box::new(move |snapshot: JsValue, event: JsValue, actor_scope: JsValue| -> Result<JsValue, JsValue> {
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
        Ok(clone_snapshot_with_field(&snapshot, "context", &next_ctx))
    });

    Ok(create_logic_object(&transition_fn, get_init, trans, |logic_for_create| {
        Box::new(move |options: JsValue| -> Result<JsValue, JsValue> {
            let logic_init = logic_for_create.clone();
            let state = LogicActorState::new(
                &options,
                |input_val, _actor_obj| {
                    let get_init = js_sys::Reflect::get(&logic_init, &JsValue::from_str("getInitialSnapshot"))?;
                    let get_init_fn = get_init.dyn_into::<js_sys::Function>()?;
                    let scope = js_sys::Object::new();
                    get_init_fn.call2(&logic_init, &scope, input_val)
                },
                false,
            )?;

            let trans_fn_val = js_sys::Reflect::get(&logic_for_create, &JsValue::from_str("transition"))?;
            let trans_fn = trans_fn_val.dyn_into::<js_sys::Function>()?;
            let current_snap_for_send = state.current_snap.clone();
            let subscribers_for_send = state.subscribers.clone();
            let logic_for_send = logic_for_create.clone();
            let is_started_for_send = state.is_started.clone();
            let is_stopped_for_send = state.is_stopped.clone();
            let send_closure = Closure::wrap(Box::new(move |evt: JsValue| -> Result<JsValue, JsValue> {
                if !*is_started_for_send.borrow() || *is_stopped_for_send.borrow() {
                    return Ok(JsValue::UNDEFINED);
                }
                let next_snap = trans_fn.call3(&logic_for_send, &current_snap_for_send.borrow(), &evt, &js_sys::Object::new())?;
                *current_snap_for_send.borrow_mut() = next_snap.clone();
                notify_next(&subscribers_for_send, &next_snap);
                Ok(JsValue::UNDEFINED)
            }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);
            let _ = js_sys::Reflect::set(&state.actor_obj, &JsValue::from_str("send"), send_closure.as_ref());
            send_closure.forget();

            let actor_for_start = state.actor_obj.clone();
            let is_started_for_start = state.is_started.clone();
            let is_stopped_for_start = state.is_stopped.clone();
            let start_closure = Closure::wrap(Box::new(move || -> JsValue {
                if !*is_stopped_for_start.borrow() {
                    *is_started_for_start.borrow_mut() = true;
                }
                actor_for_start.clone().into()
            }) as Box<dyn Fn() -> JsValue>);
            let _ = js_sys::Reflect::set(&state.actor_obj, &JsValue::from_str("start"), start_closure.as_ref());
            start_closure.forget();

            state.attach_stop(true, None);
            Ok(state.actor_obj.into())
        })
    }))
}

#[wasm_bindgen(js_name = fromPromise)]
pub fn from_promise(promise_creator: JsValue) -> Result<JsValue, JsValue> {
    let get_init = Box::new(|_scope: JsValue, input: JsValue| -> Result<JsValue, JsValue> {
        Ok(make_snapshot("active", &JsValue::UNDEFINED, &JsValue::UNDEFINED, None, Some(&input)))
    });
    let trans = Box::new(|snapshot: JsValue, _event: JsValue, _scope: JsValue| -> Result<JsValue, JsValue> {
        Ok(snapshot)
    });
    let creator_for_actor = promise_creator.clone();

    Ok(create_logic_object(&promise_creator, get_init, trans, move |_logic_obj| {
        Box::new(move |options: JsValue| -> Result<JsValue, JsValue> {
            let state = LogicActorState::new(
                &options,
                |resolved_input, _| Ok(make_snapshot("active", &JsValue::UNDEFINED, &JsValue::UNDEFINED, None, Some(resolved_input))),
                true,
            )?;

            let send_closure = Closure::wrap(Box::new(move |_evt: JsValue| -> Result<JsValue, JsValue> {
                Ok(JsValue::UNDEFINED)
            }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);
            let _ = js_sys::Reflect::set(&state.actor_obj, &JsValue::from_str("send"), send_closure.as_ref());
            send_closure.forget();

            state.attach_stop(true, None);

            let actor_for_start = state.actor_obj.clone();
            let is_started_for_start = state.is_started.clone();
            let is_stopped_for_start = state.is_stopped.clone();
            let current_snap_for_start = state.current_snap.clone();
            let subscribers_for_start = state.subscribers.clone();
            let creator_for_start = creator_for_actor.clone();
            let resolved_input_for_start = state.resolved_input.clone();
            let parent_for_start = state.parent_ref.clone();
            let system_for_start = state.system_ref.clone();
            let id_for_start = state.id_str.clone();

            let start_closure = Closure::wrap(Box::new(move || -> JsValue {
                if *is_started_for_start.borrow() || *is_stopped_for_start.borrow() {
                    return actor_for_start.clone().into();
                }
                *is_started_for_start.borrow_mut() = true;

                register_and_emit_init(
                    &system_for_start,
                    &parent_for_start,
                    &actor_for_start,
                    &resolved_input_for_start,
                    &current_snap_for_start.borrow(),
                );

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
                        let snap_js = make_snapshot("done", &val, &JsValue::UNDEFINED, None, Some(&resolved_inp_then));
                        *current_snap_then.borrow_mut() = snap_js.clone();

                        let resolve_evt = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(&resolve_evt, &JsValue::from_str("type"), &JsValue::from_str("xstate.promise.resolve"));
                        let _ = js_sys::Reflect::set(&resolve_evt, &JsValue::from_str("data"), &val);
                        system_relay(&system_then, &actor_then, &actor_then, &resolve_evt);

                        emit_done_to_parent(&system_then, &actor_then, &parent_then, &id_then, &val);
                        system_snapshot(&system_then, &actor_then, &resolve_evt, &snap_js, "done");
                        notify_complete_and_clear(&subscribers_then, Some(&snap_js));
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
                        let snap_js = make_snapshot("error", &JsValue::UNDEFINED, &err, None, Some(&resolved_inp_catch));
                        *current_snap_catch.borrow_mut() = snap_js.clone();

                        if !parent_catch.is_undefined() && !parent_catch.is_null() {
                            let err_evt = js_sys::Object::new();
                            let _ = js_sys::Reflect::set(&err_evt, &JsValue::from_str("type"), &JsValue::from_str(&format!("xstate.error.actor.{}", id_catch)));
                            let _ = js_sys::Reflect::set(&err_evt, &JsValue::from_str("error"), &err);
                            let _ = js_sys::Reflect::set(&err_evt, &JsValue::from_str("actorId"), &JsValue::from_str(&id_catch));
                            system_relay(&system_catch, &actor_catch, &parent_catch, &err_evt);
                        }

                        notify_error_and_clear(&subscribers_catch, Some(&snap_js), &err);
                    }) as Box<dyn FnMut(JsValue)>);

                    let _ = promise.then2(&on_fulfilled, &on_rejected);
                    on_fulfilled.forget();
                    on_rejected.forget();
                }

                actor_for_start.clone().into()
            }) as Box<dyn Fn() -> JsValue>);
            let _ = js_sys::Reflect::set(&state.actor_obj, &JsValue::from_str("start"), start_closure.as_ref());
            start_closure.forget();

            Ok(state.actor_obj.into())
        })
    }))
}

fn build_observable_logic(observable_creator: JsValue, relay_events_to_parent: bool) -> Result<JsValue, JsValue> {
    let get_init = Box::new(|_scope: JsValue, input: JsValue| -> Result<JsValue, JsValue> {
        Ok(make_snapshot(
            "active",
            &JsValue::UNDEFINED,
            &JsValue::UNDEFINED,
            Some(&JsValue::UNDEFINED),
            Some(&input),
        ))
    });
    let trans = Box::new(|snapshot: JsValue, _event: JsValue, _scope: JsValue| -> Result<JsValue, JsValue> {
        Ok(snapshot)
    });
    let creator_for_actor = observable_creator.clone();

    Ok(create_logic_object(&observable_creator, get_init, trans, move |_logic_obj| {
        Box::new(move |options: JsValue| -> Result<JsValue, JsValue> {
            let state = LogicActorState::new(
                &options,
                |resolved_input, _| {
                    Ok(make_snapshot(
                        "active",
                        &JsValue::UNDEFINED,
                        &JsValue::UNDEFINED,
                        Some(&JsValue::UNDEFINED),
                        Some(resolved_input),
                    ))
                },
                true,
            )?;

            let subscription_handle = Rc::new(RefCell::new(JsValue::UNDEFINED));
            let sub_handle_for_stop = subscription_handle.clone();
            state.attach_stop(
                true,
                Some(Box::new(move || {
                    let handle = sub_handle_for_stop.borrow().clone();
                    if handle.is_object() && !handle.is_null() {
                        if let Ok(unsub) = js_sys::Reflect::get(&handle, &JsValue::from_str("unsubscribe")) {
                            if let Ok(f) = unsub.dyn_into::<js_sys::Function>() {
                                let _ = f.call0(&handle);
                            }
                        }
                    }
                })),
            );

            let actor_for_start = state.actor_obj.clone();
            let is_started_for_start = state.is_started.clone();
            let is_stopped_for_start = state.is_stopped.clone();
            let current_snap_for_start = state.current_snap.clone();
            let subscribers_for_start = state.subscribers.clone();
            let creator_for_start = creator_for_actor.clone();
            let resolved_input_for_start = state.resolved_input.clone();
            let parent_for_start = state.parent_ref.clone();
            let system_for_start = state.system_ref.clone();
            let id_for_start = state.id_str.clone();
            let sub_handle_for_start = subscription_handle.clone();

            let start_closure = Closure::wrap(Box::new(move || -> JsValue {
                if *is_started_for_start.borrow() || *is_stopped_for_start.borrow() {
                    return actor_for_start.clone().into();
                }
                *is_started_for_start.borrow_mut() = true;

                register_and_emit_init(
                    &system_for_start,
                    &parent_for_start,
                    &actor_for_start,
                    &resolved_input_for_start,
                    &current_snap_for_start.borrow(),
                );

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

                        let parent_next = parent_obs.clone();
                        let system_next = system_obs.clone();
                        let actor_next = actor_obs.clone();

                        let observer = js_sys::Object::new();
                        let next_closure = Closure::wrap(Box::new(move |val: JsValue| {
                            if *is_stopped_obs.borrow() {
                                return;
                            }
                            if relay_events_to_parent
                                && val.is_object()
                                && !val.is_null()
                                && !parent_next.is_undefined()
                                && !parent_next.is_null()
                            {
                                system_relay(&system_next, &actor_next, &parent_next, &val);
                            }
                            let snap_js = make_snapshot(
                                "active",
                                &JsValue::UNDEFINED,
                                &JsValue::UNDEFINED,
                                Some(&val),
                                Some(&resolved_inp_next),
                            );
                            *current_snap_obs.borrow_mut() = snap_js.clone();
                            notify_next(&subscribers_obs, &snap_js);
                        }) as Box<dyn FnMut(JsValue)>);
                        let _ = js_sys::Reflect::set(&observer, &JsValue::from_str("next"), next_closure.as_ref());
                        next_closure.forget();

                        let is_stopped_comp = is_stopped_for_start.clone();
                        let current_snap_comp = current_snap_for_start.clone();
                        let subscribers_comp = subscribers_for_start.clone();
                        let comp_closure = Closure::wrap(Box::new(move || {
                            if *is_stopped_comp.borrow() {
                                return;
                            }
                            let cur_ctx = js_sys::Reflect::get(&current_snap_comp.borrow(), &JsValue::from_str("context"))
                                .unwrap_or(JsValue::UNDEFINED);
                            let snap_js = make_snapshot(
                                "done",
                                &cur_ctx,
                                &JsValue::UNDEFINED,
                                Some(&cur_ctx),
                                Some(&resolved_inp_comp),
                            );
                            *current_snap_comp.borrow_mut() = snap_js.clone();

                            emit_done_to_parent(&system_obs, &actor_obs, &parent_obs, &id_obs, &cur_ctx);
                            notify_complete_and_clear(&subscribers_comp, Some(&snap_js));
                        }) as Box<dyn FnMut()>);
                        let _ = js_sys::Reflect::set(&observer, &JsValue::from_str("complete"), comp_closure.as_ref());
                        comp_closure.forget();

                        let is_stopped_err = is_stopped_for_start.clone();
                        let current_snap_err = current_snap_for_start.clone();
                        let subscribers_err = subscribers_for_start.clone();
                        let err_closure = Closure::wrap(Box::new(move |err: JsValue| {
                            if *is_stopped_err.borrow() {
                                return;
                            }
                            let cur_ctx = js_sys::Reflect::get(&current_snap_err.borrow(), &JsValue::from_str("context"))
                                .unwrap_or(JsValue::UNDEFINED);
                            let snap_js = make_snapshot(
                                "error",
                                &JsValue::UNDEFINED,
                                &err,
                                Some(&cur_ctx),
                                Some(&resolved_inp_err),
                            );
                            *current_snap_err.borrow_mut() = snap_js;
                            notify_error_and_clear(&subscribers_err, None, &err);
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
            let _ = js_sys::Reflect::set(&state.actor_obj, &JsValue::from_str("start"), start_closure.as_ref());
            start_closure.forget();

            Ok(state.actor_obj.into())
        })
    }))
}

#[wasm_bindgen(js_name = fromObservable)]
pub fn from_observable(observable_creator: JsValue) -> Result<JsValue, JsValue> {
    build_observable_logic(observable_creator, false)
}

#[wasm_bindgen(js_name = fromEventObservable)]
pub fn from_event_observable(observable_creator: JsValue) -> Result<JsValue, JsValue> {
    build_observable_logic(observable_creator, true)
}

#[wasm_bindgen(js_name = fromCallback)]
pub fn from_callback(callback_fn: JsValue) -> Result<JsValue, JsValue> {
    let get_init = Box::new(|_scope: JsValue, input: JsValue| -> Result<JsValue, JsValue> {
        Ok(make_snapshot("active", &JsValue::UNDEFINED, &JsValue::UNDEFINED, None, Some(&input)))
    });
    let trans = Box::new(|snapshot: JsValue, _event: JsValue, _scope: JsValue| -> Result<JsValue, JsValue> {
        Ok(snapshot)
    });
    let cb_for_actor = callback_fn.clone();

    Ok(create_logic_object(&callback_fn, get_init, trans, move |_logic_obj| {
        Box::new(move |options: JsValue| -> Result<JsValue, JsValue> {
            let state = LogicActorState::new(
                &options,
                |resolved_input, _| Ok(make_snapshot("active", &JsValue::UNDEFINED, &JsValue::UNDEFINED, None, Some(resolved_input))),
                false,
            )?;

            let cleanup_handle = Rc::new(RefCell::new(JsValue::UNDEFINED));
            let receivers = Rc::new(RefCell::new(Vec::<js_sys::Function>::new()));

            let receivers_for_send = receivers.clone();
            let is_stopped_for_send = state.is_stopped.clone();
            let send_closure = Closure::wrap(Box::new(move |evt: JsValue| -> Result<JsValue, JsValue> {
                if *is_stopped_for_send.borrow() {
                    return Ok(JsValue::UNDEFINED);
                }
                let r_list = receivers_for_send.borrow().clone();
                for r in r_list {
                    let _ = r.call1(&JsValue::NULL, &evt);
                }
                Ok(JsValue::UNDEFINED)
            }) as Box<dyn Fn(JsValue) -> Result<JsValue, JsValue>>);
            let _ = js_sys::Reflect::set(&state.actor_obj, &JsValue::from_str("send"), send_closure.as_ref());
            send_closure.forget();

            let cleanup_for_stop = cleanup_handle.clone();
            state.attach_stop(
                false,
                Some(Box::new(move || {
                    let cleanup_val = cleanup_for_stop.borrow().clone();
                    if let Ok(f) = cleanup_val.dyn_into::<js_sys::Function>() {
                        let _ = f.call0(&JsValue::NULL);
                    }
                })),
            );

            let actor_for_start = state.actor_obj.clone();
            let is_started_for_start = state.is_started.clone();
            let is_stopped_for_start = state.is_stopped.clone();
            let cb_for_start = cb_for_actor.clone();
            let resolved_input_for_start = state.resolved_input.clone();
            let parent_for_start = state.parent_ref.clone();
            let system_for_start = state.system_ref.clone();
            let receivers_for_start = receivers.clone();
            let cleanup_for_start = cleanup_handle.clone();
            let current_snap_for_start = state.current_snap.clone();

            let start_closure = Closure::wrap(Box::new(move || -> JsValue {
                if *is_started_for_start.borrow() || *is_stopped_for_start.borrow() {
                    return actor_for_start.clone().into();
                }
                *is_started_for_start.borrow_mut() = true;

                register_and_emit_init(
                    &system_for_start,
                    &parent_for_start,
                    &actor_for_start,
                    &resolved_input_for_start,
                    &current_snap_for_start.borrow(),
                );

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
            let _ = js_sys::Reflect::set(&state.actor_obj, &JsValue::from_str("start"), start_closure.as_ref());
            start_closure.forget();

            Ok(state.actor_obj.into())
        })
    }))
}
