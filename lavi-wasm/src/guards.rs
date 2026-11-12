use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use lavi::{IndexMap, IndexMapExt, Value};

use crate::actions::CURRENT_ACTOR;
use crate::actor::{Actor, ActorInner};
use crate::invoke::spawn_child_from_actor;
use crate::value_bridge::{js_to_value, value_to_js};

#[wasm_bindgen]
pub fn and(guards: JsValue) -> JsValue {
    let func = js_sys::Function::new_no_args("throw new Error(\"This isn't supposed to be called\");");
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.and"));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("guards"), &guards);
    func.into()
}

#[wasm_bindgen]
pub fn or(guards: JsValue) -> JsValue {
    let func = js_sys::Function::new_no_args("throw new Error(\"This isn't supposed to be called\");");
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.or"));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("guards"), &guards);
    func.into()
}

#[wasm_bindgen]
pub fn not(guard: JsValue) -> JsValue {
    let func = js_sys::Function::new_no_args("throw new Error(\"This isn't supposed to be called\");");
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.not"));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("guard"), &guard);
    let arr = js_sys::Array::new();
    arr.push(&guard);
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("guards"), &arr);
    func.into()
}

#[wasm_bindgen(js_name = stateIn)]
pub fn state_in(state_value: JsValue) -> JsValue {
    let func = js_sys::Function::new_no_args("throw new Error(\"This isn't supposed to be called\");");
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.stateIn"));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("stateValue"), &state_value);
    func.into()
}

pub(crate) fn evaluate_guard_entry(
    guard_item: &JsValue,
    guards_map: &js_sys::Object,
    arg_obj: &JsValue,
    params_js: &JsValue,
    depth: usize,
) -> Result<bool, String> {
    if depth > 50 {
        return Ok(false);
    }
    if guard_item.is_string() {
        let name = guard_item.as_string().unwrap();
        let target = js_sys::Reflect::get(guards_map, &JsValue::from_str(&name)).unwrap_or(JsValue::UNDEFINED);
        if target.is_undefined() {
            return Err(name);
        }
        return evaluate_guard_entry(&target, guards_map, arg_obj, &JsValue::UNDEFINED, depth + 1);
    }
    let type_key = JsValue::from_str("type");
    let guard_type = js_sys::Reflect::get(guard_item, &type_key)
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();

    if guard_type == "xstate.and" {
        let guards_key = JsValue::from_str("guards");
        let children = js_sys::Reflect::get(guard_item, &guards_key).unwrap_or(JsValue::UNDEFINED);
        if js_sys::Array::is_array(&children) {
            let arr = js_sys::Array::from(&children);
            for i in 0..arr.length() {
                let child = arr.get(i);
                match evaluate_guard_entry(&child, guards_map, arg_obj, &JsValue::UNDEFINED, depth + 1) {
                    Ok(true) => {}
                    Ok(false) => return Ok(false),
                    Err(e) => return Err(e),
                }
            }
        }
        return Ok(true);
    }

    if guard_type == "xstate.or" {
        let guards_key = JsValue::from_str("guards");
        let children = js_sys::Reflect::get(guard_item, &guards_key).unwrap_or(JsValue::UNDEFINED);
        if js_sys::Array::is_array(&children) {
            let arr = js_sys::Array::from(&children);
            for i in 0..arr.length() {
                let child = arr.get(i);
                match evaluate_guard_entry(&child, guards_map, arg_obj, &JsValue::UNDEFINED, depth + 1) {
                    Ok(true) => return Ok(true),
                    Ok(false) => {}
                    Err(e) => return Err(e),
                }
            }
        }
        return Ok(false);
    }

    if guard_type == "xstate.not" {
        let guard_key = JsValue::from_str("guard");
        let mut inner = js_sys::Reflect::get(guard_item, &guard_key).unwrap_or(JsValue::UNDEFINED);
        if inner.is_undefined() {
            let guards_key = JsValue::from_str("guards");
            if let Ok(g_val) = js_sys::Reflect::get(guard_item, &guards_key) {
                if js_sys::Array::is_array(&g_val) {
                    let arr = js_sys::Array::from(&g_val);
                    if arr.length() > 0 {
                        inner = arr.get(0);
                    }
                }
            }
        }
        let b = evaluate_guard_entry(&inner, guards_map, arg_obj, &JsValue::UNDEFINED, depth + 1)?;
        return Ok(!b);
    }

    if guard_item.is_object() && !guard_item.is_null() && !guard_item.is_function() && !guard_type.is_empty() {
        let params_key = JsValue::from_str("params");
        let mut obj_params = js_sys::Reflect::get(guard_item, &params_key).unwrap_or(JsValue::UNDEFINED);
        if obj_params.is_function() {
            let func: js_sys::Function = obj_params.dyn_into().unwrap();
            obj_params = func.call1(&JsValue::NULL, arg_obj).unwrap_or(JsValue::UNDEFINED);
        }
        let target = js_sys::Reflect::get(guards_map, &JsValue::from_str(&guard_type)).unwrap_or(JsValue::UNDEFINED);
        if target.is_undefined() {
            return Err(guard_type);
        }
        return evaluate_guard_entry(&target, guards_map, arg_obj, &obj_params, depth + 1);
    }

    if let Ok(func) = guard_item.clone().dyn_into::<js_sys::Function>() {
        let res = func.call2(&JsValue::NULL, arg_obj, params_js).unwrap_or(JsValue::FALSE);
        return Ok(res.as_bool().unwrap_or(false));
    }

    Ok(false)
}

pub(crate) fn evaluate_guard_in_check(
    guard_arg: &JsValue,
    ctx: &Value,
    event: &Value,
    guards: &IndexMap<String, lavi::GuardFunction>,
    current_state: &lavi::StateValue,
    depth: usize,
) -> bool {
    if depth > 50 {
        return false;
    }

    if guard_arg.is_string() {
        let name = guard_arg.as_string().unwrap_or_default();
        return guards.get(&name).map(|f| f(ctx, event, &None)).unwrap_or(false);
    }

    let t_val = if (guard_arg.is_object() && !guard_arg.is_null()) || guard_arg.is_function() {
        js_sys::Reflect::get(guard_arg, &JsValue::from_str("type")).unwrap_or(JsValue::UNDEFINED)
    } else {
        JsValue::UNDEFINED
    };
    let t_str = t_val.as_string().unwrap_or_default();

    if t_str == "xstate.not" {
        let inner = js_sys::Reflect::get(guard_arg, &JsValue::from_str("guard"))
            .unwrap_or(JsValue::UNDEFINED);
        let inner = if inner.is_undefined() {
            let g_val = js_sys::Reflect::get(guard_arg, &JsValue::from_str("guards")).unwrap_or(JsValue::UNDEFINED);
            if js_sys::Array::is_array(&g_val) {
                let arr = js_sys::Array::from(&g_val);
                if arr.length() > 0 { arr.get(0) } else { JsValue::UNDEFINED }
            } else {
                JsValue::UNDEFINED
            }
        } else {
            inner
        };
        return !evaluate_guard_in_check(&inner, ctx, event, guards, current_state, depth + 1);
    }

    if t_str == "xstate.and" {
        let children = js_sys::Reflect::get(guard_arg, &JsValue::from_str("guards")).unwrap_or(JsValue::UNDEFINED);
        if js_sys::Array::is_array(&children) {
            let arr = js_sys::Array::from(&children);
            for i in 0..arr.length() {
                if !evaluate_guard_in_check(&arr.get(i), ctx, event, guards, current_state, depth + 1) {
                    return false;
                }
            }
        }
        return true;
    }

    if t_str == "xstate.or" {
        let children = js_sys::Reflect::get(guard_arg, &JsValue::from_str("guards")).unwrap_or(JsValue::UNDEFINED);
        if js_sys::Array::is_array(&children) {
            let arr = js_sys::Array::from(&children);
            for i in 0..arr.length() {
                if evaluate_guard_in_check(&arr.get(i), ctx, event, guards, current_state, depth + 1) {
                    return true;
                }
            }
        }
        return false;
    }

    if t_str == "xstate.stateIn" {
        let sv = js_sys::Reflect::get(guard_arg, &JsValue::from_str("stateValue")).unwrap_or(JsValue::UNDEFINED);
        if sv.is_string() {
            let s = sv.as_string().unwrap_or_default();
            return current_state.matches(&s);
        } else if sv.is_object() && !sv.is_null() {
            let val = js_to_value(&sv);
            let st_val = lavi::value_to_state_value(val);
            return current_state.matches_state_value(&st_val);
        }
        return false;
    }

    if !t_str.is_empty() {
        if let Some(g_fn) = guards.get(&t_str) {
            let p_val = js_sys::Reflect::get(guard_arg, &JsValue::from_str("params")).ok();
            let p_val_resolved = if let Some(p_js) = p_val {
                if p_js.is_function() {
                    let p_fn: js_sys::Function = p_js.unchecked_into();
                    let arg = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&arg, &JsValue::from_str("context"), &value_to_js(ctx));
                    let _ = js_sys::Reflect::set(&arg, &JsValue::from_str("event"), &Actor::event_to_js(event));
                    p_fn.call1(&JsValue::NULL, &arg).ok().map(|v| js_to_value(&v))
                } else if !p_js.is_undefined() {
                    Some(js_to_value(&p_js))
                } else {
                    None
                }
            } else {
                None
            };
            return g_fn(ctx, event, &p_val_resolved);
        }
        return false;
    }

    if guard_arg.is_function() {
        let func: js_sys::Function = guard_arg.clone().unchecked_into();
        let arg_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &value_to_js(ctx));
        let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &Actor::event_to_js(event));
        return func.call1(&JsValue::NULL, &arg_obj).ok().and_then(|v| v.as_bool()).unwrap_or(false);
    }

    false
}

pub(crate) fn process_guard_value(
    guard_val: &JsValue,
    guards: &mut IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    action_descriptors: &mut IndexMap<String, JsValue>,
    counter: &mut u32,
    guards_holder: &Rc<RefCell<IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>>>,
) -> JsValue {
    if !guard_val.is_object() && !guard_val.is_function() || guard_val.is_null() {
        return guard_val.clone();
    }
    let guard_type = {
        let type_key = JsValue::from_str("type");
        js_sys::Reflect::get(guard_val, &type_key)
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default()
    };

    if guard_type == "xstate.and" || guard_type == "xstate.or" {
        let guards_key = JsValue::from_str("guards");
        let children = js_sys::Reflect::get(guard_val, &guards_key).unwrap_or(JsValue::UNDEFINED);
        let new_arr = js_sys::Array::new();
        if js_sys::Array::is_array(&children) {
            let arr = js_sys::Array::from(&children);
            for i in 0..arr.length() {
                let child = arr.get(i);
                let processed_child = process_guard_value(&child, guards, action_descriptors, counter, guards_holder);
                new_arr.push(&processed_child);
            }
        }
        let out_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&out_obj, &JsValue::from_str("type"), &JsValue::from_str(&guard_type));
        let _ = js_sys::Reflect::set(&out_obj, &guards_key, &new_arr);
        return out_obj.into();
    }

    if guard_type == "xstate.not" {
        let guard_key = JsValue::from_str("guard");
        let inner = js_sys::Reflect::get(guard_val, &guard_key).unwrap_or(JsValue::UNDEFINED);
        let processed_inner = if !inner.is_undefined() {
            process_guard_value(&inner, guards, action_descriptors, counter, guards_holder)
        } else {
            let guards_key = JsValue::from_str("guards");
            let g_val = js_sys::Reflect::get(guard_val, &guards_key).unwrap_or(JsValue::UNDEFINED);
            if js_sys::Array::is_array(&g_val) {
                let arr = js_sys::Array::from(&g_val);
                if arr.length() > 0 {
                    let child = arr.get(0);
                    process_guard_value(&child, guards, action_descriptors, counter, guards_holder)
                } else {
                    JsValue::UNDEFINED
                }
            } else {
                JsValue::UNDEFINED
            }
        };
        let out_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&out_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.not"));
        let _ = js_sys::Reflect::set(&out_obj, &guard_key, &processed_inner);
        return out_obj.into();
    }

    if guard_type == "xstate.stateIn" {
        let state_val_key = JsValue::from_str("stateValue");
        let sv = js_sys::Reflect::get(guard_val, &state_val_key).unwrap_or(JsValue::UNDEFINED);
        let out_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&out_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.stateIn"));
        let _ = js_sys::Reflect::set(&out_obj, &state_val_key, &sv);
        return out_obj.into();
    }

    let params_key = JsValue::from_str("params");
    let params_val = js_sys::Reflect::get(guard_val, &params_key).unwrap_or(JsValue::UNDEFINED);
    if params_val.is_function() && !guard_type.is_empty() {
        *counter += 1;
        let id = format!("auto_dynamic_guard_{}", counter);
        let params_fn: js_sys::Function = params_val.unchecked_into();
        let gh = guards_holder.clone();
        let target_name = guard_type.clone();
        let closure = Rc::new(move |context: &Value, event: &Value, _guard_params: &Option<Value>| -> bool {
            let ctx_js = value_to_js(context);
            let evt_js = Actor::event_to_js(event);
            let arg_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
            let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
            let p_res = params_fn.call1(&JsValue::NULL, &arg_obj).unwrap_or(JsValue::UNDEFINED);
            let resolved_p = js_to_value(&p_res);
            let borrowed = gh.borrow();
            if let Some(target_guard) = borrowed.get(&target_name) {
                target_guard(context, event, &Some(resolved_p))
            } else {
                false
            }
        });
        guards.insert(id.clone(), closure.clone());
        guards_holder.borrow_mut().insert(id.clone(), closure);
        action_descriptors.insert(id.clone(), guard_val.clone());
        let replace_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&replace_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
        return replace_obj.into();
    }

    if guard_val.is_function() {
        *counter += 1;
        let id = format!("auto_guard_{}", counter);
        let func: js_sys::Function = guard_val.clone().unchecked_into();
        let closure = Rc::new(move |context: &Value, event: &Value, guard_params: &Option<Value>| -> bool {
            evaluate_js_guard(&func, context, event, guard_params)
        });
        guards.insert(id.clone(), closure.clone());
        guards_holder.borrow_mut().insert(id.clone(), closure);
        action_descriptors.insert(id.clone(), guard_val.clone());
        let replace_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&replace_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
        return replace_obj.into();
    }

    guard_val.clone()
}

pub(crate) fn evaluate_js_guard(func: &js_sys::Function, context: &Value, event: &Value, guard_params: &Option<Value>) -> bool {
    let ctx_js = value_to_js(context);
    let evt_js = Actor::event_to_js(event);
    let arg_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);

    let current_actor_opt = CURRENT_ACTOR.with(|cell| cell.borrow().clone());
    let _spawn_fn_keepalive = if let Some(ref current_actor) = current_actor_opt {
        let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), current_actor);
        let actor_for_spawn = current_actor.clone();
        let spawn_fn = Closure::wrap(Box::new(move |src: JsValue, options: JsValue| -> JsValue {
            spawn_child_from_actor(&actor_for_spawn, &src, &options)
        }) as Box<dyn Fn(JsValue, JsValue) -> JsValue>);
        let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("spawn"), spawn_fn.as_ref());
        Some(spawn_fn)
    } else {
        let self_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &self_obj);
        None
    };
    let params_js = if let Some(p) = guard_params {
        value_to_js(p)
    } else {
        JsValue::UNDEFINED
    };
    if let Ok(res) = func.call2(&JsValue::NULL, &arg_obj, &params_js) {
        res.as_bool().unwrap_or(false)
    } else {
        false
    }
}

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
