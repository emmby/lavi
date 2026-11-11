use core::cell::RefCell;
use lavi::{IndexMap, IndexMapExt};
use lavi::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

thread_local! {
    pub(crate) static LIVE_ACTOR_REFS: RefCell<IndexMap<String, JsValue>> = RefCell::new(IndexMap::new());
}

pub fn register_live_actor_ref(session_id: &str, actor: &JsValue) {
    if !session_id.is_empty() {
        LIVE_ACTOR_REFS.with(|m| m.borrow_mut().insert(session_id.to_string(), actor.clone()));
    }
}

pub fn unregister_live_actor_ref(id: &str) {
    if !id.is_empty() {
        LIVE_ACTOR_REFS.with(|m| {
            m.borrow_mut().shift_remove(id);
        });
    }
}

pub fn js_to_value(val: &JsValue) -> Value {
    if val.is_null() || val.is_undefined() || val.is_function() {
        Value::Null
    } else if let Some(b) = val.as_bool() {
        Value::Bool(b)
    } else if let Some(n) = val.as_f64() {
        Value::Number(n)
    } else if let Some(s) = val.as_string() {
        Value::String(s)
    } else if js_sys::Array::is_array(val) {
        let arr: &js_sys::Array = val.unchecked_ref();
        let len = arr.length();
        let mut vec = Vec::with_capacity(len as usize);
        for i in 0..len {
            let elem = arr.get(i);
            vec.push(js_to_value(&elem));
        }
        Value::Array(vec)
    } else if val.is_object() {
        let send_fn = js_sys::Reflect::get(val, &JsValue::from_str("send")).unwrap_or(JsValue::UNDEFINED);
        if send_fn.is_function() {
            let sid = js_sys::Reflect::get(val, &JsValue::from_str("sessionId"))
                .ok()
                .and_then(|v| v.as_string())
                .or_else(|| {
                    js_sys::Reflect::get(val, &JsValue::from_str("id"))
                        .ok()
                        .and_then(|v| v.as_string())
                });
            if let Some(id_str) = sid {
                LIVE_ACTOR_REFS.with(|m| m.borrow_mut().insert(id_str.clone(), val.clone()));
                let mut map = IndexMap::with_capacity(1);
                map.insert("__xstate_actor_ref__".to_string(), Value::String(id_str));
                return Value::Object(map);
            }
        }

        let obj = js_sys::Object::from(val.clone());
        let keys = js_sys::Object::keys(&obj);
        let len = keys.length();
        let mut map = IndexMap::with_capacity(len as usize);
        for i in 0..len {
            let key_js = keys.get(i);
            let key_str = key_js.as_string().unwrap_or_default();
            if let Ok(v) = js_sys::Reflect::get(&obj, &key_js) {
                if v.is_undefined() || v.is_function() {
                    continue;
                }
                map.insert(key_str, js_to_value(&v));
            }
        }
        Value::Object(map)
    } else {
        Value::Null
    }
}

pub fn value_to_js(val: &Value) -> JsValue {
    match val {
        Value::Null => JsValue::NULL,
        Value::Bool(b) => JsValue::from_bool(*b),
        Value::Number(n) => JsValue::from_f64(*n),
        Value::String(s) => JsValue::from_str(s),
        Value::Array(arr) => {
            let js_arr = js_sys::Array::new_with_length(arr.len() as u32);
            for (i, item) in arr.iter().enumerate() {
                js_arr.set(i as u32, value_to_js(item));
            }
            js_arr.into()
        }
        Value::Object(map) => {
            if map.len() == 1 {
                if let Some(Value::String(sid)) = map.get("__xstate_actor_ref__") {
                    if let Some(act) = LIVE_ACTOR_REFS.with(|m| m.borrow().get(sid).cloned()) {
                        return act;
                    }
                    if let Some(current_actor) = crate::actions::CURRENT_ACTOR.with(|c| c.borrow().clone()) {
                        if let Ok(sys) = js_sys::Reflect::get(&current_actor, &JsValue::from_str("system")) {
                            if let Ok(get_fn) = js_sys::Reflect::get(&sys, &JsValue::from_str("get")) {
                                if let Ok(f) = get_fn.dyn_into::<js_sys::Function>() {
                                    if let Ok(act) = f.call1(&sys, &JsValue::from_str(sid)) {
                                        if !act.is_undefined() && !act.is_null() {
                                            return act;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let js_obj = js_sys::Object::new();
            for (k, v) in map {
                let _ = js_sys::Reflect::set(&js_obj, &JsValue::from_str(k), &value_to_js(v));
            }
            js_obj.into()
        }
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_js_to_value_and_value_to_js_roundtrip() {
        let mut map = IndexMap::new();
        map.insert("num".to_string(), Value::Number(42.0));
        map.insert("str".to_string(), Value::String("hello".to_string()));
        map.insert("bool".to_string(), Value::Bool(true));
        map.insert("null".to_string(), Value::Null);
        map.insert("arr".to_string(), Value::Array(vec![Value::Number(1.0), Value::Number(2.0)]));
        let val = Value::Object(map);

        let js = value_to_js(&val);
        let back = js_to_value(&js);
        assert_eq!(val, back);
    }
}
