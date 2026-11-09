use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use lavi::{IndexMap, IndexMapExt, RuntimeMachine, Value};

use crate::actor::Actor;
use crate::value_bridge::{js_to_value, value_to_js};

#[wasm_bindgen(js_name = assertEvent)]
pub fn assert_event(event: JsValue, expected_type: JsValue) -> Result<(), JsValue> {
    let mut types: Vec<String> = Vec::new();
    if js_sys::Array::is_array(&expected_type) {
        let arr = js_sys::Array::from(&expected_type);
        for i in 0..arr.length() {
            if let Some(s) = arr.get(i).as_string() {
                types.push(s);
            }
        }
    } else if let Some(s) = expected_type.as_string() {
        types.push(s);
    }

    let event_type = if event.is_object() && !event.is_null() {
        js_sys::Reflect::get(&event, &JsValue::from_str("type"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let matches = types.iter().any(|descriptor| {
        lavi::matches_event_descriptor_with_warn(&event_type, descriptor, |msg| {
            web_sys::console::warn_1(&JsValue::from_str(&msg));
        })
    });

    if !matches {
        let types_text = if types.len() == 1 {
            format!("type matching \"{}\"", types[0])
        } else {
            format!("one of types matching \"{}\"", types.join("\", \""))
        };
        let event_json = js_sys::JSON::stringify(&event)
            .ok()
            .and_then(|s| s.as_string())
            .unwrap_or_else(|| "undefined".to_string());
        let msg = format!("Expected event {} to have {}", event_json, types_text);
        return Err(js_sys::Error::new(&msg).into());
    }
    Ok(())
}

#[wasm_bindgen]
pub fn assign(assignment: JsValue) -> JsValue {
    // Create a function that does nothing when called, but has 'type' and 'assignment' properties.
    // This satisfies XState (which expects a function) and our processing (which expects properties).
    let func = js_sys::Function::new_no_args("return undefined;");
    let type_key = JsValue::from_str("type");
    let assign_key = JsValue::from_str("assignment");
    let _ = js_sys::Reflect::set(&func, &type_key, &JsValue::from_str("xstate.assign"));
    let _ = js_sys::Reflect::set(&func, &assign_key, &assignment);
    func.into()
}

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

#[wasm_bindgen]
pub fn cancel(id: String) -> JsValue {
    let func = js_sys::Function::new_no_args("return undefined;");
    let params_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("sendId"), &JsValue::from_str(&id));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.cancel"));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("sendId"), &JsValue::from_str(&id));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("params"), &params_obj);
    func.into()
}

#[wasm_bindgen]
pub fn log(value_or_expr: JsValue, label: Option<String>) -> JsValue {
    let func = js_sys::Function::new_no_args("return undefined;");
    let params_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("value"), &value_or_expr);
    if let Some(ref l) = label {
        let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("label"), &JsValue::from_str(l));
        let _ = js_sys::Reflect::set(&func, &JsValue::from_str("label"), &JsValue::from_str(l));
    }
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.log"));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("value"), &value_or_expr);
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("params"), &params_obj);
    func.into()
}

#[wasm_bindgen(js_name = enqueueActions)]
pub fn enqueue_actions(collect: js_sys::Function) -> JsValue {
    let func = js_sys::Function::new_no_args("return undefined;");
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.enqueueActions"));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("collect"), &collect);
    func.into()
}

#[wasm_bindgen]
pub fn raise(event: JsValue, options: Option<JsValue>) -> JsValue {
    if event.is_string() {
        let str_val = event.as_string().unwrap_or_default();
        let func = js_sys::Function::new_no_args("return undefined;");
        let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.raise.invalid"));
        let err_msg = format!("Only event objects may be used with raise; use raise({{ type: \"{}\" }}) instead", str_val);
        let _ = js_sys::Reflect::set(&func, &JsValue::from_str("error"), &JsValue::from_str(&err_msg));
        return func.into();
    }
    let obj = js_sys::Object::new();
    let type_key = JsValue::from_str("type");
    let params_key = JsValue::from_str("params");

    let params_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("event"), &event);
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
    let _ = js_sys::Reflect::set(&obj, &params_key, &params_obj);

    let _ = js_sys::Reflect::set(&obj, &type_key, &JsValue::from_str("xstate.raise"));

    obj.into()
}

#[wasm_bindgen]
pub fn emit(event_expr: JsValue) -> JsValue {
    let func = js_sys::Function::new_no_args("return undefined;");
    let params_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("event"), &event_expr);
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.emit"));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("params"), &params_obj);
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
                    if arr.length() > 0 { inner = arr.get(0); }
                }
            }
        }
        match evaluate_guard_entry(&inner, guards_map, arg_obj, &JsValue::UNDEFINED, depth + 1) {
            Ok(b) => return Ok(!b),
            Err(e) => return Err(e),
        }
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

pub(crate) fn parse_and_register_delays(options: &JsValue, machine: &mut RuntimeMachine) {
    if !options.is_object() || options.is_null() {
        return;
    }
    let delays_key = JsValue::from_str("delays");
    let delays_val = js_sys::Reflect::get(options, &delays_key).unwrap_or(JsValue::UNDEFINED);
    if !delays_val.is_object() || delays_val.is_null() {
        return;
    }
    let delays_obj = js_sys::Object::from(delays_val);
    let keys = js_sys::Object::keys(&delays_obj);
    for i in 0..keys.length() {
        let k = keys.get(i);
        let k_str = k.as_string().unwrap_or_default();
        let d_val = js_sys::Reflect::get(&delays_obj, &k).unwrap_or(JsValue::UNDEFINED);
        if let Ok(f) = d_val.clone().dyn_into::<js_sys::Function>() {
            let func_rc = Rc::new(f);
            let delay_fn: lavi::DelayFunction = Rc::new(move |ctx: &Value, evt: &Value| {
                let arg_obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &value_to_js(ctx));
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &Actor::event_to_js(evt));
                if let Ok(res) = func_rc.call1(&JsValue::NULL, &arg_obj) {
                    if let Some(n) = res.as_f64() {
                        return n.max(0.0) as u64;
                    }
                }
                0
            });
            machine.register_delay(&k_str, delay_fn);
        } else if let Some(n) = d_val.as_f64() {
            let n_u64 = n.max(0.0) as u64;
            let delay_fn: lavi::DelayFunction = Rc::new(move |_ctx: &Value, _evt: &Value| n_u64);
            machine.register_delay(&k_str, delay_fn);
        } else if let Some(s) = d_val.as_string() {
            if let Ok(n_u64) = s.parse::<u64>() {
                let delay_fn: lavi::DelayFunction = Rc::new(move |_ctx: &Value, _evt: &Value| n_u64);
                machine.register_delay(&k_str, delay_fn);
            }
        }
    }
}

pub(crate) fn configure_logger(options: &JsValue, runner: &mut Actor) {
    let custom_logger = if options.is_object() && !options.is_null() {
        let l_key = JsValue::from_str("logger");
        let l_val = js_sys::Reflect::get(options, &l_key).unwrap_or(JsValue::UNDEFINED);
        l_val.dyn_into::<js_sys::Function>().ok()
    } else {
        None
    };
    runner.logger = custom_logger.clone();
    *runner.logger_holder.borrow_mut() = custom_logger.clone();
    let logger_cb = Rc::new(move |label: &Option<String>, val: &Value| {
        let val_js = value_to_js(val);
        if let Some(ref l_fn) = custom_logger {
            if let Some(lbl) = label {
                let _ = l_fn.call2(&JsValue::NULL, &JsValue::from_str(lbl), &val_js);
            } else {
                let _ = l_fn.call1(&JsValue::NULL, &val_js);
            }
        } else if let Some(lbl) = label {
            web_sys::console::log_2(&JsValue::from_str(lbl), &val_js);
        } else {
            web_sys::console::log_1(&val_js);
        }
    });
    runner.machine.register_logger(logger_cb);
}

// Helper to parse "guards" and "actions" from a JS object (either implementations object or options object)
pub(crate) fn parse_implementations(
    impl_obj: &JsValue,
    guards_out: &mut IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    actions_with_effects_out: &mut IndexMap<String, lavi::ActionWithEffectsFunction>,
) {
    if !impl_obj.is_object() {
        return;
    }
    let obj = js_sys::Object::from(impl_obj.clone());

    // --- Guards ---
    let guards_key = JsValue::from_str("guards");
    let guards_val = js_sys::Reflect::get(&obj, &guards_key).unwrap_or(JsValue::UNDEFINED);
    if guards_val.is_object() && !guards_val.is_null() {
         let guards_map = js_sys::Object::from(guards_val);
         let keys = js_sys::Object::keys(&guards_map);
         for i in 0..keys.length() {
             let key_js = keys.get(i);
             let key_str = key_js.as_string().unwrap_or_default();
             let guard_item_js = js_sys::Reflect::get(&guards_map, &key_js).unwrap_or(JsValue::UNDEFINED);
             let guards_map_clone = guards_map.clone();
             let guard_item_clone = guard_item_js.clone();
             guards_out.insert(key_str.clone(), Rc::new(move |ctx, evt, params| {
                 let ctx_js = value_to_js(ctx);
                 let evt_js = Actor::event_to_js(evt);
                 let params_js = if let Some(p) = params {
                     value_to_js(p)
                 } else {
                     JsValue::UNDEFINED
                 };
                 let arg_obj = js_sys::Object::new();
                 let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                 let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
                 evaluate_guard_entry(&guard_item_clone, &guards_map_clone, &arg_obj, &params_js, 0).unwrap_or(false)
             }));
         }
    }
    // --- Actions ---
    let actions_key = JsValue::from_str("actions");
    let actions_val = js_sys::Reflect::get(&obj, &actions_key).unwrap_or(JsValue::UNDEFINED);
    if actions_val.is_object() {
         let actions_map = js_sys::Object::from(actions_val);
         let keys = js_sys::Object::keys(&actions_map);
         for i in 0..keys.length() {
             let key_js = keys.get(i);
             let key_str = key_js.as_string().unwrap_or_default();
             let action_item = js_sys::Reflect::get(&actions_map, &key_js).unwrap_or(JsValue::UNDEFINED);

             let type_key = JsValue::from_str("type");
             let type_val = js_sys::Reflect::get(&action_item, &type_key).unwrap_or(JsValue::UNDEFINED);
             if type_val == JsValue::from_str("xstate.enqueueActions") {
                 let collect_val = js_sys::Reflect::get(&action_item, &JsValue::from_str("collect")).unwrap_or(JsValue::UNDEFINED);
                 if let Ok(collect_fn) = collect_val.dyn_into::<js_sys::Function>() {
                     actions_with_effects_out.insert(
                         key_str.clone(),
                         Rc::new(move |ctx, evt, _params, actions_map, guards_map, delays_map, current_state, effects| {
                             execute_enqueue_actions_collect(&collect_fn, ctx, evt, actions_map, guards_map, delays_map, current_state, effects);
                         }),
                     );
                 }
                 continue;
             }

             // Check for xstate 5 assign (dynamic function or partial object)
             let assign_key = JsValue::from_str("assignment");
             let assign_val = js_sys::Reflect::get(&action_item, &assign_key).unwrap_or(JsValue::UNDEFINED);
             if !assign_val.is_undefined() {
                 let assign_val_clone = assign_val.clone();
                 actions_out.insert(
                     key_str,
                     Rc::new(move |ctx_mut, evt, params| {
                         evaluate_assignment_value(&assign_val_clone, ctx_mut, evt, params);
                     }),
                 );
                 continue;
             }

             // Fallback: Treat as function (Generic Action)
             if let Ok(action_fn) = action_item.dyn_into::<js_sys::Function>() {
                 actions_out.insert(key_str, Rc::new(move |ctx_mut: &mut Value, evt: &Value, params: &Option<Value>| {
                     let ctx_js = value_to_js(ctx_mut);
                     let evt_js = Actor::event_to_js(evt);
                     let params_js = if let Some(p) = params {
                         value_to_js(p)
                     } else {
                         JsValue::UNDEFINED
                     };

                     let arg_obj = js_sys::Object::new();
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("params"), &params_js);

                     // Call action with ({ context, event, params }, params)
                     match action_fn.call2(&JsValue::NULL, &arg_obj, &params_js) {
                         Ok(res) => {
                             if res.is_object() || res.is_function() {
                                 let type_key = JsValue::from_str("type");
                                 let type_val = js_sys::Reflect::get(&res, &type_key).unwrap_or(JsValue::UNDEFINED);
                                 if type_val == JsValue::from_str("xstate.assign") {
                                     let assign_key = JsValue::from_str("assignment");
                                     let assignment = js_sys::Reflect::get(&res, &assign_key).unwrap_or(JsValue::UNDEFINED);
                                     if !assignment.is_undefined() {
                                         evaluate_assignment_value(&assignment, ctx_mut, evt, params);
                                     }
                                 }
                             }
                         }
                         Err(e) => {
                             web_sys::console::error_2(&JsValue::from_str("Error executing generic action:"), &e);
                         }
                     }
                 }));
             }
         }
    }
}

// Helper function to extract inline assignments
pub(crate) fn extract_inline_assignments(val: &JsValue) -> Result<(JsValue, IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>), JsValue> {
    let mut actions_out = IndexMap::new();
    let mut action_counter = 0;

    if !val.is_object() {
         return Ok((val.clone(), actions_out));
    }
    let definition = js_sys::Object::from(val.clone());

    // Process Root context (evaluate if function)
    let context_key = JsValue::from_str("context");
    let context_val = js_sys::Reflect::get(&definition, &context_key).unwrap_or(JsValue::UNDEFINED);
    if context_val.is_function() {
        if let Ok(func) = context_val.dyn_into::<js_sys::Function>() {
             let arg_obj = js_sys::Object::new();
             let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("input"), &JsValue::UNDEFINED);
             if let Ok(res) = func.call1(&JsValue::NULL, &arg_obj) {
                 let _ = js_sys::Reflect::set(&definition, &context_key, &res);
             }
        }
    }

    // Process Root entry
    process_action_list(&definition, "entry", &mut actions_out, &mut action_counter);
    // Process Root exit
    process_action_list(&definition, "exit", &mut actions_out, &mut action_counter);

    extract_recursive(&definition, &mut actions_out, &mut action_counter);

    Ok((val.clone(), actions_out))
}

pub(crate) fn extract_recursive(obj: &js_sys::Object, actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>, counter: &mut u32) {
    // Process entry/exit
    process_action_list(obj, "entry", actions_out, counter);
    process_action_list(obj, "exit", actions_out, counter);

    // Process on
    let on_key = JsValue::from_str("on");
    let on_val = js_sys::Reflect::get(obj, &on_key).unwrap_or(JsValue::UNDEFINED);
    if on_val.is_object() && !on_val.is_null() {
        let on_obj = js_sys::Object::from(on_val);
        let event_keys = js_sys::Object::keys(&on_obj);
        for i in 0..event_keys.length() {
            let event_key = event_keys.get(i);
            let trans_val = js_sys::Reflect::get(&on_obj, &event_key).unwrap_or(JsValue::UNDEFINED);
            if js_sys::Array::is_array(&trans_val) {
                let arr = js_sys::Array::from(&trans_val);
                for j in 0..arr.length() {
                    let item = arr.get(j);
                    if item.is_object() {
                        process_action_list(&js_sys::Object::from(item), "actions", actions_out, counter);
                    }
                }
            } else if trans_val.is_object() && !trans_val.is_null() {
                process_action_list(&js_sys::Object::from(trans_val), "actions", actions_out, counter);
            }
        }
    }

    // Process always
    let always_key = JsValue::from_str("always");
    let always_val = js_sys::Reflect::get(obj, &always_key).unwrap_or(JsValue::UNDEFINED);
    if js_sys::Array::is_array(&always_val) {
        let arr = js_sys::Array::from(&always_val);
        for j in 0..arr.length() {
            let t_val = arr.get(j);
            if t_val.is_object() {
                process_action_list(&js_sys::Object::from(t_val), "actions", actions_out, counter);
            }
        }
    } else if always_val.is_object() {
        process_action_list(&js_sys::Object::from(always_val), "actions", actions_out, counter);
    }

    // Process initial (if object)
    let init_key = JsValue::from_str("initial");
    let init_val = js_sys::Reflect::get(obj, &init_key).unwrap_or(JsValue::UNDEFINED);
    if init_val.is_object() && !js_sys::Array::is_array(&init_val) {
        process_action_list(&js_sys::Object::from(init_val), "actions", actions_out, counter);
    }

    // Recurse into states
    let states_key = JsValue::from_str("states");
    let states_val = js_sys::Reflect::get(obj, &states_key).unwrap_or(JsValue::UNDEFINED);
    if states_val.is_object() {
        let states_obj = js_sys::Object::from(states_val);
        let s_keys = js_sys::Object::keys(&states_obj);
        for i in 0..s_keys.length() {
            let s_key = s_keys.get(i);
            let s_node = js_sys::Reflect::get(&states_obj, &s_key).unwrap_or(JsValue::UNDEFINED);
            if s_node.is_object() {
                let s_obj = js_sys::Object::from(s_node);
                extract_recursive(&s_obj, actions_out, counter);
            }
        }
    }
}

pub(crate) fn process_action_list(
    parent_obj: &js_sys::Object, 
    prop_name: &str, 
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    counter: &mut u32
) {
    let prop_key = JsValue::from_str(prop_name);
    let val = js_sys::Reflect::get(parent_obj, &prop_key).unwrap_or(JsValue::UNDEFINED);

    if val.is_undefined() { 
        return; 
    }

    if js_sys::Array::is_array(&val) {
        let arr = js_sys::Array::from(&val);
        for i in 0..arr.length() {
            let item = arr.get(i);
            if let Some(new_item) = convert_inline_action(&item, actions_out, counter) {
                arr.set(i, new_item);
            }
        }
    } else {
        // Single object
        if let Some(new_item) = convert_inline_action(&val, actions_out, counter) {
            let _ = js_sys::Reflect::set(parent_obj, &prop_key, &new_item);
        }
    }
}

pub(crate) fn convert_inline_action(
    action_item: &JsValue,
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>, // Changed to ActionFunction in instruction, but keeping original type for consistency with surrounding code.
    counter: &mut u32
) -> Option<JsValue> {
     let is_obj = action_item.is_object();
     let is_func = action_item.is_function();
     if !is_obj && !is_func { return None; }
     let type_key = JsValue::from_str("type");
     let type_val = js_sys::Reflect::get(action_item, &type_key).unwrap_or(JsValue::UNDEFINED);
     let type_str = type_val.as_string().unwrap_or_default();
     if type_str == "xstate.enqueueActions" || type_str.starts_with("auto_action_") {
         return None;
     }
     if type_str == "xstate.assign" {
         let assign_key = JsValue::from_str("assignment");
         let assign_val = js_sys::Reflect::get(action_item, &assign_key).unwrap_or(JsValue::UNDEFINED);
         if !assign_val.is_undefined() {
             *counter += 1;
             let new_action_name = format!("generated_assign_{}", counter);
             let assign_val_clone = assign_val.clone();
             actions_out.insert(new_action_name.clone(), Rc::new(move |ctx_mut, evt, params| {
                 evaluate_assignment_value(&assign_val_clone, ctx_mut, evt, params);
             }));
             let new_obj = js_sys::Object::new();
             let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&new_action_name));
             return Some(new_obj.into());
         }
     } else if is_func {
         if let Ok(func) = action_item.clone().dyn_into::<js_sys::Function>() {
             *counter += 1;
             let new_action_name = format!("generated_action_{}", counter);
             let func_clone = func.clone();
             actions_out.insert(new_action_name.clone(), Rc::new(move |ctx_mut, evt, params| {
                  let ctx_js = value_to_js(ctx_mut);
                  let evt_js = Actor::event_to_js(evt);
                  let arg_obj = js_sys::Object::new();
                  let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                  let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
                  let params_js = if let Some(p) = params {
                       let p_js = value_to_js(p);
                       let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("params"), &p_js);
                       p_js
                  } else {
                       JsValue::UNDEFINED
                  };
                  match func_clone.call2(&JsValue::NULL, &arg_obj, &params_js) {
                      Ok(res) => {
                           if res.is_object() || res.is_function() {
                                let type_key = JsValue::from_str("type");
                                let type_val = js_sys::Reflect::get(&res, &type_key).unwrap_or(JsValue::UNDEFINED);
                                if type_val == JsValue::from_str("xstate.assign") {
                                     let assign_key = JsValue::from_str("assignment");
                                     let assignment = js_sys::Reflect::get(&res, &assign_key).unwrap_or(JsValue::UNDEFINED);
                                     if !assignment.is_undefined() {
                                          evaluate_assignment_value(&assignment, ctx_mut, evt, params);
                                     }
                                }
                           }
                      },
                      Err(e) => { web_sys::console::log_2(&JsValue::from_str("Error executing dynamic action:"), &e); }
                  }
             }));
             let new_obj = js_sys::Object::new();
             let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&new_action_name));
             return Some(new_obj.into());
         }
     }
     None
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_enqueue_actions_collect(
    collect: &js_sys::Function,
    ctx: &mut Value,
    event: &Value,
    actions_map: &IndexMap<String, lavi::ActionFunction>,
    guards_map: &IndexMap<String, lavi::GuardFunction>,
    delays_map: &IndexMap<String, lavi::DelayFunction>,
    current_state: &lavi::StateValue,
    effects: &mut lavi::ActionEffects,
) {
    let ctx_rc = Rc::new(RefCell::new(ctx.clone()));
    let effects_rc = Rc::new(RefCell::new(Vec::<Value>::new()));
    let cancel_rc = Rc::new(RefCell::new(Vec::<String>::new()));
    let delayed_events_rc = Rc::new(RefCell::new(Vec::<lavi::DelayedEvent>::new()));
    let inspected_rc = Rc::new(RefCell::new(Vec::<lavi::InspectEvent>::new()));

    let is_snapshot_mode = actions_map.contains_key("__lavi_snapshot_mode__");
    let actions_map_rc = Rc::new(actions_map.clone());
    let guards_map_rc = Rc::new(guards_map.clone());
    let delays_map_rc = Rc::new(delays_map.clone());
    let event_val = event.clone();

    let ctx_for_enqueue = ctx_rc.clone();
    let effects_for_enqueue = effects_rc.clone();
    let actions_for_enqueue = actions_map_rc.clone();
    let delays_for_enqueue = delays_map_rc.clone();
    let event_for_enqueue = event_val.clone();
    let cancel_for_enqueue = cancel_rc.clone();
    let delayed_for_enqueue = delayed_events_rc.clone();
    let inspected_for_enqueue = inspected_rc.clone();

    let enqueue_closure = Closure::wrap(Box::new(move |action_arg: JsValue| {
        let t_val = if action_arg.is_function() || (action_arg.is_object() && !action_arg.is_null()) {
            js_sys::Reflect::get(&action_arg, &JsValue::from_str("type")).unwrap_or(JsValue::UNDEFINED)
        } else {
            JsValue::UNDEFINED
        };
        let t_str = t_val.as_string().unwrap_or_default();

        if t_str == "xstate.assign" {
            let assign_key = JsValue::from_str("assignment");
            let mut assign_val = js_sys::Reflect::get(&action_arg, &assign_key).unwrap_or(JsValue::UNDEFINED);
            if assign_val.is_undefined() {
                let params = js_sys::Reflect::get(&action_arg, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
                if params.is_object() {
                    assign_val = js_sys::Reflect::get(&params, &assign_key).unwrap_or(JsValue::UNDEFINED);
                }
            }
            if !assign_val.is_undefined() {
                let mut ctx_borrow = ctx_for_enqueue.borrow_mut();
                evaluate_assignment_value(&assign_val, &mut ctx_borrow, &event_for_enqueue, &None);
            }
        } else if t_str == "xstate.raise" || t_str == "xstate.raise.delayed" {
            if !is_snapshot_mode {
                let params = js_sys::Reflect::get(&action_arg, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
                let evt_val = if params.is_object() && !params.is_null() {
                    let p_obj = js_sys::Object::from(params.clone());
                    js_sys::Reflect::get(&p_obj, &JsValue::from_str("event")).unwrap_or(action_arg.clone())
                } else {
                    action_arg.clone()
                };
                let delay_val = if params.is_object() && !params.is_null() {
                    let p_obj = js_sys::Object::from(params.clone());
                    js_sys::Reflect::get(&p_obj, &JsValue::from_str("delay")).unwrap_or(JsValue::UNDEFINED)
                } else {
                    JsValue::UNDEFINED
                };
                let id_val = if params.is_object() && !params.is_null() {
                    let p_obj = js_sys::Object::from(params);
                    js_sys::Reflect::get(&p_obj, &JsValue::from_str("id")).ok().and_then(|v| v.as_string())
                } else {
                    None
                };
                let is_delayed = t_str == "xstate.raise.delayed" || !delay_val.is_undefined();
                let arg_obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &value_to_js(&ctx_for_enqueue.borrow()));
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &Actor::event_to_js(&event_for_enqueue));
                let evt_to_raise = if evt_val.is_function() {
                    let func: js_sys::Function = evt_val.unchecked_into();
                    func.call1(&JsValue::NULL, &arg_obj).unwrap_or(JsValue::UNDEFINED)
                } else {
                    evt_val
                };
                let raised_val = js_to_value(&evt_to_raise);
                if is_delayed {
                    let (delay_ms, delay_enum) = if delay_val.is_function() {
                        let d_fn: js_sys::Function = delay_val.unchecked_into();
                        let ms = d_fn.call1(&JsValue::NULL, &arg_obj).ok().and_then(|v| {
                            if let Some(n) = v.as_f64() {
                                Some(n.max(0.0) as u64)
                            } else if let Some(s) = v.as_string() {
                                s.parse::<u64>().ok()
                            } else {
                                None
                            }
                        }).unwrap_or(0);
                        (ms, lavi::Delay::Literal(ms))
                    } else if delay_val.is_string() {
                        let name = delay_val.as_string().unwrap_or_default();
                        let ms = if let Some(d_fn) = delays_for_enqueue.get(&name) {
                            d_fn(&ctx_for_enqueue.borrow(), &event_for_enqueue)
                        } else {
                            name.parse::<u64>().unwrap_or(0)
                        };
                        (ms, lavi::Delay::Named(name))
                    } else if let Some(n) = delay_val.as_f64() {
                        let ms = n.max(0.0) as u64;
                        (ms, lavi::Delay::Literal(ms))
                    } else {
                        (0, lavi::Delay::Literal(0))
                    };
                    inspected_for_enqueue.borrow_mut().push(lavi::InspectEvent::Action {
                        action: lavi::Action::DelayedRaise(lavi::DelayedRaiseAction {
                            event: raised_val.clone(),
                            delay: delay_enum,
                            id: id_val.clone(),
                        }),
                        params: None,
                    });
                    delayed_for_enqueue.borrow_mut().push(lavi::DelayedEvent {
                        id: id_val,
                        deadline: delay_ms,
                        event: raised_val,
                    });
                } else {
                    inspected_for_enqueue.borrow_mut().push(lavi::InspectEvent::Action {
                        action: lavi::Action::Raise(raised_val.clone()),
                        params: None,
                    });
                    effects_for_enqueue.borrow_mut().push(raised_val);
                }
            }
        } else if t_str == "xstate.cancel" {
            if !is_snapshot_mode {
                let send_id = js_sys::Reflect::get(&action_arg, &JsValue::from_str("sendId"))
                    .or_else(|_| {
                        let params = js_sys::Reflect::get(&action_arg, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
                        js_sys::Reflect::get(&params, &JsValue::from_str("sendId"))
                    })
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default();
                if !send_id.is_empty() {
                    inspected_for_enqueue.borrow_mut().push(lavi::InspectEvent::Action {
                        action: lavi::Action::Cancel(lavi::CancelAction {
                            send_id: send_id.clone(),
                        }),
                        params: None,
                    });
                    cancel_for_enqueue.borrow_mut().push(send_id);
                }
            }
        } else if let Some(act) = actions_for_enqueue.get(&t_str) {
            if !is_snapshot_mode {
                let params_val = js_sys::Reflect::get(&action_arg, &JsValue::from_str("params"))
                    .ok()
                    .and_then(|p| if p.is_undefined() { None } else { Some(js_to_value(&p)) });
                inspected_for_enqueue.borrow_mut().push(lavi::InspectEvent::Action {
                    action: lavi::Action::Generic(lavi::GenericAction {
                        action_type: t_str.clone(),
                        params: params_val.clone(),
                    }),
                    params: params_val.clone(),
                });
                let mut ctx_borrow = ctx_for_enqueue.borrow_mut();
                act(&mut ctx_borrow, &event_for_enqueue, &params_val);
            }
        } else if action_arg.is_string() && !is_snapshot_mode {
            let name = action_arg.as_string().unwrap_or_default();
            inspected_for_enqueue.borrow_mut().push(lavi::InspectEvent::Action {
                action: lavi::Action::Named(name.clone()),
                params: None,
            });
            if let Some(act) = actions_for_enqueue.get(&name) {
                let mut ctx_borrow = ctx_for_enqueue.borrow_mut();
                act(&mut ctx_borrow, &event_for_enqueue, &None);
            }
        } else if action_arg.is_function() && !is_snapshot_mode {
            let func: js_sys::Function = action_arg.unchecked_into();
            let fn_name: String = func.name().into();
            let mut ctx_borrow = ctx_for_enqueue.borrow_mut();
            let res = execute_js_action(&func, &mut ctx_borrow, &event_for_enqueue, &None);
            let mut was_assign = false;
            if res.is_object() || res.is_function() {
                let t_val = js_sys::Reflect::get(&res, &JsValue::from_str("type")).unwrap_or(JsValue::UNDEFINED);
                if t_val == JsValue::from_str("xstate.assign") {
                    was_assign = true;
                    let a_val = js_sys::Reflect::get(&res, &JsValue::from_str("assignment")).unwrap_or(JsValue::UNDEFINED);
                    if !a_val.is_undefined() {
                        evaluate_assignment_value(&a_val, &mut ctx_borrow, &event_for_enqueue, &None);
                    }
                }
            }
            if !was_assign {
                let action_name = if fn_name.is_empty() {
                    "(anonymous)".to_string()
                } else {
                    fn_name
                };
                inspected_for_enqueue.borrow_mut().push(lavi::InspectEvent::Action {
                    action: lavi::Action::Named(action_name),
                    params: None,
                });
            }
        }
    }) as Box<dyn FnMut(JsValue)>);

    let enqueue_js_fn = enqueue_closure.as_ref().unchecked_ref::<js_sys::Function>().clone();

    let ctx_for_assign = ctx_rc.clone();
    let event_for_assign = event_val.clone();
    let assign_closure = Closure::wrap(Box::new(move |assignment: JsValue| {
        let mut ctx_borrow = ctx_for_assign.borrow_mut();
        evaluate_assignment_value(&assignment, &mut ctx_borrow, &event_for_assign, &None);
    }) as Box<dyn FnMut(JsValue)>);
    let _ = js_sys::Reflect::set(&enqueue_js_fn, &JsValue::from_str("assign"), assign_closure.as_ref().unchecked_ref());

    let effects_for_raise = effects_rc.clone();
    let inspected_for_raise = inspected_rc.clone();
    let raise_closure = Closure::wrap(Box::new(move |evt_arg: JsValue| {
        if !is_snapshot_mode {
            let raised_val = js_to_value(&evt_arg);
            inspected_for_raise.borrow_mut().push(lavi::InspectEvent::Action {
                action: lavi::Action::Raise(raised_val.clone()),
                params: None,
            });
            effects_for_raise.borrow_mut().push(raised_val);
        }
    }) as Box<dyn FnMut(JsValue)>);
    let _ = js_sys::Reflect::set(&enqueue_js_fn, &JsValue::from_str("raise"), raise_closure.as_ref().unchecked_ref());

    let ctx_for_check = ctx_rc.clone();
    let guards_for_check = guards_map_rc.clone();
    let event_for_check = event_val.clone();
    let state_for_check = current_state.clone();
    let check_closure = Closure::wrap(Box::new(move |guard_arg: JsValue| -> bool {
        let ctx_borrow = ctx_for_check.borrow();
        evaluate_guard_in_check(&guard_arg, &ctx_borrow, &event_for_check, &guards_for_check, &state_for_check, 0)
    }) as Box<dyn FnMut(JsValue) -> bool>);

    let arg_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("enqueue"), &enqueue_js_fn);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("check"), check_closure.as_ref().unchecked_ref());
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &value_to_js(&ctx_rc.borrow()));
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &Actor::event_to_js(&event_val));
    
    let self_obj = js_sys::Object::new();
    let send_dummy = js_sys::Function::new_no_args("return undefined;");
    let _ = js_sys::Reflect::set(&self_obj, &JsValue::from_str("send"), &send_dummy);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &self_obj);

    let _ = collect.call1(&JsValue::NULL, &arg_obj);

    *ctx = ctx_rc.borrow().clone();
    for raised in effects_rc.borrow().iter() {
        effects.raised.push_back(raised.clone());
    }
    for id in cancel_rc.borrow().iter() {
        effects.cancel_ids.push(id.clone());
    }
    for delayed in delayed_events_rc.borrow().iter() {
        effects.delayed_events.push(delayed.clone());
    }
    for insp in inspected_rc.borrow().iter() {
        effects.inspected_actions.push(insp.clone());
    }
}

pub(crate) fn process_action_item(
    item: &JsValue,
    actions: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    actions_with_effects: &mut IndexMap<String, lavi::ActionWithEffectsFunction>,
    action_descriptors: &mut IndexMap<String, JsValue>,
    counter: &mut u32,
    path: &str,
    logger_holder: &Rc<RefCell<Option<js_sys::Function>>>,
) -> JsValue {
    if item.is_string() {
        let s = item.as_string().unwrap_or_default();
        let act_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&act_obj, &JsValue::from_str("type"), item);
        let _ = js_sys::Reflect::set(&act_obj, &JsValue::from_str("__raw_action"), item);
        action_descriptors.insert(s, act_obj.into());
        return item.clone();
    }
    if item.is_function() || (item.is_object() && !item.is_null()) {
        let type_key = JsValue::from_str("type");
        let type_val = js_sys::Reflect::get(item, &type_key).unwrap_or(JsValue::UNDEFINED);
        let type_str = type_val.as_string().unwrap_or_default();

        if type_str == "xstate.enqueueActions" {
            *counter += 1;
            let id = format!("auto_action_enqueue_{}_{}", if path.is_empty() { "root" } else { path }, counter);
            let collect_val = js_sys::Reflect::get(item, &JsValue::from_str("collect")).unwrap_or(JsValue::UNDEFINED);
            if let Ok(collect_fn) = collect_val.clone().dyn_into::<js_sys::Function>() {
                actions_with_effects.insert(
                    id.clone(),
                    Rc::new(move |ctx, evt, _params, actions_map, guards_map, delays_map, current_state, effects| {
                        execute_enqueue_actions_collect(&collect_fn, ctx, evt, actions_map, guards_map, delays_map, current_state, effects);
                    }),
                );
            }
            let new_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("actionType"), &JsValue::from_str("xstate.enqueueActions"));
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("collect"), &collect_val);
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("__raw_action"), item);
            action_descriptors.insert(id.clone(), new_obj.clone().into());
            return new_obj.into();
        }


        if type_str == "xstate.assign" {
            *counter += 1;
            let id = format!("auto_action_assign_{}_{}", if path.is_empty() { "root" } else { path }, counter);
            let assign_key = JsValue::from_str("assignment");
            let mut assignment = js_sys::Reflect::get(item, &assign_key).unwrap_or(JsValue::UNDEFINED);
            if assignment.is_undefined() && item.is_function() {
                let func: js_sys::Function = item.clone().unchecked_into();
                assignment = js_sys::Reflect::get(&func, &assign_key).unwrap_or(JsValue::UNDEFINED);
            }
            let assignment_clone = assignment.clone();
            let closure = Rc::new(move |ctx_mut: &mut Value, evt: &Value, params: &Option<Value>| {
                if !assignment_clone.is_undefined() {
                    evaluate_assignment_value(&assignment_clone, ctx_mut, evt, params);
                }
            });
            actions.insert(id.clone(), closure);
            let new_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("actionType"), &JsValue::from_str("xstate.assign"));
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("assignment"), &assignment);
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("__raw_action"), item);
            let params_val = js_sys::Reflect::get(item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
            if !params_val.is_undefined() {
                let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("params"), &params_val);
            }
            action_descriptors.insert(id.clone(), new_obj.clone().into());
            return new_obj.into();
        }

        if type_str == "xstate.cancel" {
            let new_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.cancel"));
            let send_id_val = js_sys::Reflect::get(item, &JsValue::from_str("sendId"))
                .or_else(|_| {
                    let params = js_sys::Reflect::get(item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
                    js_sys::Reflect::get(&params, &JsValue::from_str("sendId"))
                })
                .unwrap_or(JsValue::UNDEFINED);
            let params_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("sendId"), &send_id_val);
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("params"), &params_obj);
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("sendId"), &send_id_val);
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("__raw_action"), item);
            action_descriptors.insert("xstate.cancel".to_string(), new_obj.clone().into());
            return new_obj.into();
        }

        if type_str == "xstate.raise.invalid" {
            let new_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.raise.invalid"));
            let err_val = js_sys::Reflect::get(item, &JsValue::from_str("error")).unwrap_or(JsValue::UNDEFINED);
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("error"), &err_val);
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("__raw_action"), item);
            action_descriptors.insert("xstate.raise.invalid".to_string(), new_obj.clone().into());
            return new_obj.into();
        }

        if type_str == "xstate.raise" || type_str == "xstate.raise.delayed" {
            let params = js_sys::Reflect::get(item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
            let event_val = if params.is_object() && !params.is_null() {
                js_sys::Reflect::get(&params, &JsValue::from_str("event")).unwrap_or(JsValue::UNDEFINED)
            } else {
                JsValue::UNDEFINED
            };
            let delay_val = if params.is_object() && !params.is_null() {
                js_sys::Reflect::get(&params, &JsValue::from_str("delay")).unwrap_or(JsValue::UNDEFINED)
            } else {
                JsValue::UNDEFINED
            };
            let is_dynamic_event = event_val.is_function();
            let is_dynamic_delay = delay_val.is_function();
            if is_dynamic_event || is_dynamic_delay {
                *counter += 1;
                let id = format!("auto_action_raise_expr_{}_{}", if path.is_empty() { "root" } else { path }, counter);
                let is_delayed = type_str == "xstate.raise.delayed" || !delay_val.is_undefined();
                let id_val = if params.is_object() && !params.is_null() {
                    js_sys::Reflect::get(&params, &JsValue::from_str("id")).ok().and_then(|v| v.as_string())
                } else {
                    None
                };
                actions_with_effects.insert(
                    id.clone(),
                    Rc::new(move |ctx, evt, _params, _actions_map, _guards_map, delays_map, _current_state, effects| {
                        let arg_obj = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &value_to_js(ctx));
                        let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &Actor::event_to_js(evt));
                        let raised_val = if is_dynamic_event {
                            let func: js_sys::Function = event_val.clone().unchecked_into();
                            if let Ok(res) = func.call1(&JsValue::NULL, &arg_obj) {
                                js_to_value(&res)
                            } else {
                                Value::Null
                            }
                        } else {
                            js_to_value(&event_val)
                        };
                        if is_delayed {
                            let delay_ms = if delay_val.is_function() {
                                let d_fn: js_sys::Function = delay_val.clone().unchecked_into();
                                d_fn.call1(&JsValue::NULL, &arg_obj).ok().and_then(|v| {
                                    if let Some(n) = v.as_f64() {
                                        Some(n.max(0.0) as u64)
                                    } else if let Some(s) = v.as_string() {
                                        s.parse::<u64>().ok()
                                    } else {
                                        None
                                    }
                                }).unwrap_or(0)
                            } else if delay_val.is_string() {
                                let name = delay_val.as_string().unwrap_or_default();
                                if let Some(d_fn) = delays_map.get(&name) {
                                    d_fn(ctx, evt)
                                } else {
                                    name.parse::<u64>().unwrap_or(0)
                                }
                            } else if let Some(n) = delay_val.as_f64() {
                                n.max(0.0) as u64
                            } else {
                                0
                            };
                            effects.delayed_events.push(lavi::DelayedEvent {
                                id: id_val.clone(),
                                deadline: delay_ms,
                                event: raised_val,
                            });
                        } else {
                            effects.raised.push_back(raised_val);
                        }
                    }),
                );
                let new_obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
                let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("__raw_action"), item);
                action_descriptors.insert(id.clone(), new_obj.clone().into());
                return new_obj.into();
            }
            action_descriptors.insert("xstate.raise".to_string(), item.clone());
            return item.clone();
        }

        if type_str == "xstate.log" {
            *counter += 1;
            let id = format!("auto_action_log_{}_{}", if path.is_empty() { "root" } else { path }, counter);
            let val_or_expr = js_sys::Reflect::get(item, &JsValue::from_str("value"))
                .or_else(|_| {
                    let params = js_sys::Reflect::get(item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
                    js_sys::Reflect::get(&params, &JsValue::from_str("value"))
                })
                .unwrap_or(JsValue::UNDEFINED);
            let label = js_sys::Reflect::get(item, &JsValue::from_str("label"))
                .or_else(|_| {
                    let params = js_sys::Reflect::get(item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
                    js_sys::Reflect::get(&params, &JsValue::from_str("label"))
                })
                .ok()
                .and_then(|v| v.as_string());
            let log_holder = logger_holder.clone();
            let val_for_closure = val_or_expr.clone();
            let label_for_closure = label.clone();
            let closure = Rc::new(move |ctx_mut: &mut Value, evt: &Value, _params: &Option<Value>| {
                let resolved_val = if val_for_closure.is_function() {
                    let func: js_sys::Function = val_for_closure.clone().unchecked_into();
                    let arg_obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &value_to_js(ctx_mut));
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &Actor::event_to_js(evt));
                    func.call1(&JsValue::NULL, &arg_obj).unwrap_or(JsValue::UNDEFINED)
                } else {
                    val_for_closure.clone()
                };
                if let Some(ref l_fn) = *log_holder.borrow() {
                    if let Some(ref lbl) = label_for_closure {
                        let _ = l_fn.call2(&JsValue::NULL, &JsValue::from_str(lbl), &resolved_val);
                    } else {
                        let _ = l_fn.call1(&JsValue::NULL, &resolved_val);
                    }
                } else if let Some(ref lbl) = label_for_closure {
                    web_sys::console::log_2(&JsValue::from_str(lbl), &resolved_val);
                } else {
                    web_sys::console::log_1(&resolved_val);
                }
            });
            actions.insert(id.clone(), closure);
            let new_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("actionType"), &JsValue::from_str("xstate.log"));
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("__raw_action"), item);
            let params_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("value"), &val_or_expr);
            if let Some(ref lbl) = label {
                let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("label"), &JsValue::from_str(lbl));
            }
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("params"), &params_obj);
            action_descriptors.insert(id.clone(), new_obj.clone().into());
            return new_obj.into();
        }

        if type_str == "xstate.emit" {
            *counter += 1;
            let id = format!("auto_action_emit_{}_{}", if path.is_empty() { "root" } else { path }, counter);
            let params_val = js_sys::Reflect::get(item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
            let new_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("actionType"), &JsValue::from_str("xstate.emit"));
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("params"), &params_val);
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("__raw_action"), item);
            action_descriptors.insert(id.clone(), new_obj.clone().into());
            return new_obj.into();
        }


        if item.is_function() {
            *counter += 1;
            let id = format!("auto_action_{}_{}", if path.is_empty() { "root" } else { path }, counter);
            let func: js_sys::Function = item.clone().unchecked_into();
            let fn_name: String = func.name().into();
            let display_name = if fn_name.is_empty() || fn_name == "entry" || fn_name == "exit" || fn_name == "actions" {
                "(anonymous)".to_string()
            } else {
                fn_name
            };
            let closure = Rc::new(move |ctx_mut: &mut Value, evt: &Value, params: &Option<Value>| {
                let res = execute_js_action(&func, ctx_mut, evt, params);
                if res.is_object() || res.is_function() {
                    let t_val = js_sys::Reflect::get(&res, &JsValue::from_str("type")).unwrap_or(JsValue::UNDEFINED);
                    if t_val == JsValue::from_str("xstate.assign") {
                        let a_val = js_sys::Reflect::get(&res, &JsValue::from_str("assignment")).unwrap_or(JsValue::UNDEFINED);
                        if !a_val.is_undefined() {
                            evaluate_assignment_value(&a_val, ctx_mut, evt, params);
                        }
                    }
                }
            });
            actions.insert(id.clone(), closure);
            let new_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("displayName"), &JsValue::from_str(&display_name));
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("__raw_action"), item);
            action_descriptors.insert(id.clone(), new_obj.clone().into());
            return new_obj.into();
        }

        let params_val = js_sys::Reflect::get(item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
        if !type_str.is_empty() && params_val.is_function() && !type_str.starts_with("auto_action_") {
            *counter += 1;
            let id = format!("auto_action_{}_{}_{}", type_str, if path.is_empty() { "root" } else { path }, counter);
            let new_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("actionType"), &JsValue::from_str(&type_str));
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("params"), &params_val);
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("__raw_action"), item);
            action_descriptors.insert(id.clone(), new_obj.clone().into());
            return new_obj.into();
        }

        if !type_str.is_empty() {
            action_descriptors.insert(type_str.clone(), item.clone());
        }
    }
    item.clone()
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

    // Check if params is a dynamic function:
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

pub(crate) fn execute_js_action(func: &js_sys::Function, ctx_mut: &mut Value, evt: &Value, params: &Option<Value>) -> JsValue {
    let ctx_js = value_to_js(ctx_mut);
    let evt_js = Actor::event_to_js(evt);
    let arg_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
    let params_js = if let Some(p) = params {
        let p_js = value_to_js(p);
        let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("params"), &p_js);
        p_js
    } else {
        JsValue::UNDEFINED
    };
    match func.call2(&JsValue::NULL, &arg_obj, &params_js) {
        Ok(v) => v,
        Err(e) => {
            web_sys::console::log_2(&JsValue::from_str("Error executing JS action:"), &e);
            JsValue::UNDEFINED
        }
    }
}

pub(crate) fn evaluate_assignment_value(
    assignment: &JsValue,
    ctx_mut: &mut Value,
    evt: &Value,
    params: &Option<Value>,
) {
    let ctx_js = value_to_js(ctx_mut);
    let evt_js = Actor::event_to_js(evt);
    let params_js = if let Some(p) = params {
        value_to_js(p)
    } else {
        JsValue::UNDEFINED
    };
    let arg_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);

    if let Ok(assign_fn) = assignment.clone().dyn_into::<js_sys::Function>() {
        if let Ok(res) = assign_fn.call2(&JsValue::NULL, &arg_obj, &params_js) {
            if res.is_object() && !res.is_null() {
                let partial_value = js_to_value(&res);
                if let Some(partial_obj) = partial_value.as_object() {
                    if let Some(ctx_obj) = ctx_mut.as_object_mut() {
                        for (k, v) in partial_obj {
                            ctx_obj.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
        }
    } else if assignment.is_object() && !assignment.is_null() {
        let assign_obj = js_sys::Object::from(assignment.clone());
        let keys = js_sys::Object::keys(&assign_obj);
        if let Some(ctx_obj_mut) = ctx_mut.as_object_mut() {
            for k in 0..keys.length() {
                let key_js = keys.get(k);
                let key_str = key_js.as_string().unwrap_or_default();
                let val_js = js_sys::Reflect::get(&assign_obj, &key_js).unwrap_or(JsValue::UNDEFINED);
                let final_val = if let Ok(func) = val_js.clone().dyn_into::<js_sys::Function>() {
                    func.call2(&JsValue::NULL, &arg_obj, &params_js)
                        .unwrap_or(JsValue::UNDEFINED)
                } else {
                    val_js
                };
                let val_core = js_to_value(&final_val);
                ctx_obj_mut.insert(key_str, val_core);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_config(
    val: &JsValue, 
    guards: &mut IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>, 
    actions: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    actions_with_effects: &mut IndexMap<String, lavi::ActionWithEffectsFunction>,
    action_descriptors: &mut IndexMap<String, JsValue>,
    depth: usize,
    counter: &mut u32,
    path: &str,
    guards_holder: &Rc<RefCell<IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>>>,
    logger_holder: &Rc<RefCell<Option<js_sys::Function>>>,
) {
    if depth > 100 { return; }

    if js_sys::Array::is_array(val) {
        let arr: &js_sys::Array = val.unchecked_ref();
        for i in 0..arr.length() {
            let item = arr.get(i);
            if item.is_function() || (item.is_object() && !item.is_null()) {
                let processed = process_action_item(&item, actions, actions_with_effects, action_descriptors, counter, path, logger_holder);
                let _ = js_sys::Reflect::set(arr, &JsValue::from_f64(i as f64), &processed);
                if processed.is_object() && !processed.is_null() {
                    process_config(&processed, guards, actions, actions_with_effects, action_descriptors, depth + 1, counter, "", guards_holder, logger_holder);
                }
            }
        }
    } else if val.is_object() && !val.is_null() && !val.is_function() {
        let obj = js_sys::Object::from(val.clone());
        let keys = js_sys::Object::keys(&obj);
        for i in 0..keys.length() {
            let key = keys.get(i);
            let value = js_sys::Reflect::get(&obj, &key).unwrap_or(JsValue::UNDEFINED);
            if value.is_undefined() {
                 let _ = js_sys::Reflect::set(&obj, &key, &JsValue::NULL);
                 continue;
            }
            let key_str = key.as_string().unwrap_or_default();
            if key_str == "invoke" {
                process_invoke_list(&obj, actions, counter);
                continue;
            }
            if key_str == "context" || key_str == "output" || key_str == "input" || key_str == "types" {
                continue;
            }
            if key_str == "guard" || key_str == "cond" {
                let processed = process_guard_value(&value, guards, action_descriptors, counter, guards_holder);
                let _ = js_sys::Reflect::set(&obj, &key, &processed);
            } else if key_str == "entry" || key_str == "exit" || key_str == "actions" {
                if js_sys::Array::is_array(&value) {
                    let arr: &js_sys::Array = value.unchecked_ref();
                    for j in 0..arr.length() {
                        let it = arr.get(j);
                        let processed = process_action_item(&it, actions, actions_with_effects, action_descriptors, counter, path, logger_holder);
                        let _ = js_sys::Reflect::set(arr, &JsValue::from_f64(j as f64), &processed);
                    }
                } else {
                    let processed = process_action_item(&value, actions, actions_with_effects, action_descriptors, counter, path, logger_holder);
                    let _ = js_sys::Reflect::set(&obj, &key, &processed);
                }
            } else if key_str == "states" && value.is_object() && !value.is_null() {
                let states_obj = js_sys::Object::from(value.clone());
                let child_keys = js_sys::Object::keys(&states_obj);
                for j in 0..child_keys.length() {
                    let child_key = child_keys.get(j);
                    let child_key_str = child_key.as_string().unwrap_or_default();
                    let child_val = js_sys::Reflect::get(&states_obj, &child_key).unwrap_or(JsValue::UNDEFINED);
                    if child_val.is_object() && !child_val.is_null() {
                        let child_path = if path.is_empty() { child_key_str.clone() } else { format!("{}.{}", path, child_key_str) };
                        process_config(&child_val, guards, actions, actions_with_effects, action_descriptors, depth + 1, counter, &child_path, guards_holder, logger_holder);
                    }
                }
            } else if (value.is_object() || js_sys::Array::is_array(&value)) && !value.is_null() && !value.is_function() {
                process_config(&value, guards, actions, actions_with_effects, action_descriptors, depth + 1, counter, path, guards_holder, logger_holder);
            }
        }
    }
}


pub(crate) fn process_invoke_list(
    state_obj: &js_sys::Object,
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    counter: &mut u32
) {
    let invoke_key = JsValue::from_str("invoke");
    let val = js_sys::Reflect::get(state_obj, &invoke_key).unwrap_or(JsValue::UNDEFINED);

    if val.is_undefined() { return; }

    if js_sys::Array::is_array(&val) {
        let arr = js_sys::Array::from(&val);
        for i in 0..arr.length() {
            let item = arr.get(i);
            if let Some(new_item) = convert_inline_invoke(&item, actions_out, counter) {
                arr.set(i, new_item);
            }
        }
    } else if let Some(new_item) = convert_inline_invoke(&val, actions_out, counter) {
        let _ = js_sys::Reflect::set(state_obj, &invoke_key, &new_item);
    }
}

pub(crate) fn convert_inline_invoke(
    invoke_item: &JsValue,
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    counter: &mut u32
) -> Option<JsValue> {
    if !invoke_item.is_object() { return None; }
    let obj = js_sys::Object::from(invoke_item.clone());
    let src_key = JsValue::from_str("src");
    let src_val = js_sys::Reflect::get(&obj, &src_key).unwrap_or(JsValue::UNDEFINED);

    if src_val.is_function() {
        if let Ok(func) = src_val.clone().dyn_into::<js_sys::Function>() {
            *counter += 1;
            let new_src_name = format!("generated_invoke_{}", counter);
            let func_clone = func.clone();
            actions_out.insert(new_src_name.clone(), Rc::new(move |ctx_mut, evt, params| {
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
            }));

            let _ = js_sys::Reflect::set(&obj, &src_key, &JsValue::from_str(&new_src_name));
            return Some(obj.into());
        }
    }
    None
}

pub(crate) fn extract_unprocessed_fn_actions(
    state_obj: &js_sys::Object,
    prop_name: &str,
    path_label: &str,
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    counter: &mut u32,
) -> Vec<lavi::Action> {
    let mut prepended = Vec::new();
    let key = JsValue::from_str(prop_name);
    let val = js_sys::Reflect::get(state_obj, &key).unwrap_or(JsValue::UNDEFINED);
    if val.is_undefined() || val.is_null() {
        return prepended;
    }
    let mut funcs: Vec<js_sys::Function> = Vec::new();
    let track_key = JsValue::from_str("__xstate_track");
    if js_sys::Array::is_array(&val) {
        let arr: &js_sys::Array = val.unchecked_ref();
        for i in 0..arr.length() {
            let item = arr.get(i);
            if let Ok(func) = item.dyn_into::<js_sys::Function>() {
                if js_sys::Reflect::get(&func, &track_key).unwrap_or(JsValue::UNDEFINED) == JsValue::TRUE {
                    funcs.push(func);
                }
            }
        }
    } else if let Ok(func) = val.dyn_into::<js_sys::Function>() {
        if js_sys::Reflect::get(&func, &track_key).unwrap_or(JsValue::UNDEFINED) == JsValue::TRUE {
            funcs.push(func);
        }
    }
    for func in funcs {
        *counter += 1;
        let id = format!("__tracked_{}_{}_{}", prop_name, path_label, counter);
        let closure = Rc::new(move |ctx_mut: &mut Value, evt: &Value, params: &Option<Value>| {
            let _ = execute_js_action(&func, ctx_mut, evt, params);
        });
        actions_out.insert(id.clone(), closure);
        prepended.push(lavi::Action::Named(id));
    }
    prepended
}

pub(crate) fn resolve_pure_actions(
    raw_actions_val: &JsValue,
    ctx_val: &mut JsValue,
    evt_val: &JsValue,
    collected: &mut Vec<JsValue>,
) {
    if raw_actions_val.is_undefined() || raw_actions_val.is_null() {
        return;
    }
    let items = if js_sys::Array::is_array(raw_actions_val) {
        js_sys::Array::from(raw_actions_val)
    } else {
        let arr = js_sys::Array::new();
        arr.push(raw_actions_val);
        arr
    };

    for i in 0..items.length() {
        let item = items.get(i);
        if item.is_string() {
            let act_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&act_obj, &JsValue::from_str("type"), &item);
            collected.push(act_obj.into());
            continue;
        }

        let type_prop = js_sys::Reflect::get(&item, &JsValue::from_str("type")).unwrap_or(JsValue::UNDEFINED);
        let act_type_prop = js_sys::Reflect::get(&item, &JsValue::from_str("actionType")).unwrap_or(JsValue::UNDEFINED);
        let type_str = type_prop.as_string().unwrap_or_default();
        let act_type_str = act_type_prop.as_string().unwrap_or_default();

        let effective_type = if !act_type_str.is_empty() {
            act_type_str
        } else if type_str.starts_with("auto_action_enqueue_") {
            "xstate.enqueueActions".to_string()
        } else if type_str.starts_with("auto_action_emit_") {
            "xstate.emit".to_string()
        } else if type_str.starts_with("auto_action_log_") {
            "xstate.log".to_string()
        } else {
            type_str.clone()
        };

        if effective_type == "xstate.assign" {
            let assign_prop = js_sys::Reflect::get(&item, &JsValue::from_str("assignment")).unwrap_or(JsValue::UNDEFINED);
            let mut ctx_value = js_to_value(ctx_val);
            let evt_value = js_to_value(evt_val);
            evaluate_assignment_value(&assign_prop, &mut ctx_value, &evt_value, &None);
            *ctx_val = value_to_js(&ctx_value);
        } else if effective_type == "xstate.enqueueActions" {
            let collect_fn = js_sys::Reflect::get(&item, &JsValue::from_str("collect")).unwrap_or(JsValue::UNDEFINED);
            if let Ok(func) = collect_fn.dyn_into::<js_sys::Function>() {
                let enqueued_items = Rc::new(RefCell::new(Vec::new()));
                let enq_clone = enqueued_items.clone();
                let enq_closure = Closure::wrap(Box::new(move |act: JsValue| {
                    enq_clone.borrow_mut().push(act);
                }) as Box<dyn FnMut(JsValue)>);
                let helper_obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&helper_obj, &JsValue::from_str("context"), ctx_val);
                let _ = js_sys::Reflect::set(&helper_obj, &JsValue::from_str("event"), evt_val);
                let _ = js_sys::Reflect::set(&helper_obj, &JsValue::from_str("enqueue"), enq_closure.as_ref().unchecked_ref());
                let self_obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&helper_obj, &JsValue::from_str("self"), &self_obj);
                let _ = func.call1(&JsValue::NULL, &helper_obj);
                for enq_act in enqueued_items.borrow().iter() {
                    resolve_pure_actions(enq_act, ctx_val, evt_val, collected);
                }
            }
        } else if effective_type == "xstate.cancel" {
            let params_prop = js_sys::Reflect::get(&item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
            let act_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&act_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.cancel"));
            let _ = js_sys::Reflect::set(&act_obj, &JsValue::from_str("params"), &params_prop);
            collected.push(act_obj.into());
        } else if effective_type == "xstate.log" {
            let params_prop = js_sys::Reflect::get(&item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
            let resolved_params = js_sys::Object::new();
            if params_prop.is_object() {
                let val_expr = js_sys::Reflect::get(&params_prop, &JsValue::from_str("value")).unwrap_or(JsValue::UNDEFINED);
                let final_val = if val_expr.is_function() {
                    let fn_val = js_sys::Function::from(val_expr);
                    let arg_obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), ctx_val);
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), evt_val);
                    fn_val.call1(&JsValue::NULL, &arg_obj).unwrap_or(JsValue::UNDEFINED)
                } else {
                    val_expr
                };
                let _ = js_sys::Reflect::set(&resolved_params, &JsValue::from_str("value"), &final_val);
            }
            let act_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&act_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.log"));
            let _ = js_sys::Reflect::set(&act_obj, &JsValue::from_str("params"), &resolved_params);
            collected.push(act_obj.into());
        } else if effective_type == "xstate.emit" {
            let params_prop = js_sys::Reflect::get(&item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
            let resolved_params = js_sys::Object::new();
            if params_prop.is_object() {
                let evt_expr = js_sys::Reflect::get(&params_prop, &JsValue::from_str("event")).unwrap_or(JsValue::UNDEFINED);
                let final_evt = if evt_expr.is_function() {
                    let fn_val = js_sys::Function::from(evt_expr);
                    let arg_obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), ctx_val);
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), evt_val);
                    fn_val.call1(&JsValue::NULL, &arg_obj).unwrap_or(JsValue::UNDEFINED)
                } else {
                    evt_expr
                };
                let _ = js_sys::Reflect::set(&resolved_params, &JsValue::from_str("event"), &final_evt);
            }
            let act_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&act_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.emit"));
            let _ = js_sys::Reflect::set(&act_obj, &JsValue::from_str("params"), &resolved_params);
            collected.push(act_obj.into());
        } else if effective_type == "xstate.raise" {
            collected.push(item.clone());
        } else if !type_str.is_empty() {
            let act_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&act_obj, &JsValue::from_str("type"), &JsValue::from_str(&effective_type));
            let params_prop = js_sys::Reflect::get(&item, &JsValue::from_str("params")).unwrap_or(JsValue::UNDEFINED);
            if !params_prop.is_undefined() {
                if params_prop.is_function() {
                    let fn_params = js_sys::Function::from(params_prop);
                    let arg_obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), ctx_val);
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), evt_val);
                    if let Ok(evaluated) = fn_params.call1(&JsValue::NULL, &arg_obj) {
                        let _ = js_sys::Reflect::set(&act_obj, &JsValue::from_str("params"), &evaluated);
                    }
                } else {
                    let _ = js_sys::Reflect::set(&act_obj, &JsValue::from_str("params"), &params_prop);
                }
            }
            collected.push(act_obj.into());
        }
    }
}

pub(crate) fn format_and_resolve_action(
    act: &lavi::Action,
    action_descriptors: &IndexMap<String, JsValue>,
    ctx_val: &mut JsValue,
    evt_val: &JsValue,
    collected: &mut Vec<JsValue>,
) {
    match act {
        lavi::Action::DelayedRaise(dra) => {
            let obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.raise"));
            let params_obj = js_sys::Object::new();
            let delay_val = match &dra.delay {
                lavi::Delay::Literal(ms) => JsValue::from_f64(*ms as f64),
                lavi::Delay::Named(s) => JsValue::from_str(s),
            };
            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("delay"), &delay_val);
            let evt_val_inner = value_to_js(&dra.event);
            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("event"), &evt_val_inner);
            if let Some(ref id) = dra.id {
                let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("id"), &JsValue::from_str(id));
            }
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("params"), &params_obj);
            collected.push(obj.into());
        }
        lavi::Action::Raise(payload) => {
            let obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.raise"));
            let params_obj = js_sys::Object::new();
            let evt_val_inner = value_to_js(payload);
            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("event"), &evt_val_inner);
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("params"), &params_obj);
            collected.push(obj.into());
        }
        lavi::Action::Cancel(ca) => {
            let obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.cancel"));
            let params_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("sendId"), &JsValue::from_str(&ca.send_id));
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("params"), &params_obj);
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("sendId"), &JsValue::from_str(&ca.send_id));
            collected.push(obj.into());
        }
        lavi::Action::Log(la) => {
            let obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.log"));
            let params_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("value"), &value_to_js(&la.value));
            if let Some(ref lbl) = la.label {
                let _ = js_sys::Reflect::set(&params_obj, &JsValue::from_str("label"), &JsValue::from_str(lbl));
            }
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("params"), &params_obj);
            collected.push(obj.into());
        }
        lavi::Action::Assign(_) => {
            // Assignments modify context directly and are not emitted in actions array.
        }
        lavi::Action::Named(name) => {
            if let Some(desc) = action_descriptors.get(name) {
                resolve_pure_actions(desc, ctx_val, evt_val, collected);
            } else {
                let obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str(name));
                collected.push(obj.into());
            }
        }
        lavi::Action::Generic(ga) => {
            if let Some(desc) = action_descriptors.get(&ga.action_type) {
                resolve_pure_actions(desc, ctx_val, evt_val, collected);
            } else {
                let obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str(&ga.action_type));
                if let Some(ref p) = ga.params {
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("params"), &value_to_js(p));
                }
                collected.push(obj.into());
            }
        }
        lavi::Action::RaiseInvalid(msg) => {
            let obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.raise.invalid"));
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("error"), &JsValue::from_str(msg));
            collected.push(obj.into());
        }
    }
}
