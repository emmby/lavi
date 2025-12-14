use portable_xstate_core::Value;
use serde_json::{Value as JsonValue, Map};


pub fn json_to_value(v: JsonValue) -> Value {
    match v {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Bool(b),
        JsonValue::Number(n) => {
            if let Some(f) = n.as_f64() {
                Value::Number(f)
            } else if let Some(i) = n.as_i64() {
                Value::Number(i as f64)
            } else {
                 Value::Number(0.0)
            }
        },
        JsonValue::String(s) => Value::String(s),
        JsonValue::Array(arr) => Value::Array(arr.into_iter().map(json_to_value).collect()),
        JsonValue::Object(obj) => Value::Object(obj.into_iter().map(|(k, v)| (k, json_to_value(v))).collect()),
    }
}

pub fn value_to_json(v: &Value) -> JsonValue {
    match v {
        Value::Null => JsonValue::Null,
        Value::Bool(b) => JsonValue::Bool(*b),
        Value::Number(n) => {
            // Check if it's integer-like for nicer JSON? 
            // For now just generic number
            if n.fract() == 0.0 {
                 if let Some(n_u64) = serde_json::Number::from_f64(*n) {
                      JsonValue::Number(n_u64)
                 } else {
                     // Fallback
                     JsonValue::Number(serde_json::Number::from_f64(*n).unwrap_or(serde_json::Number::from(0)))
                 }
            } else {
                 JsonValue::Number(serde_json::Number::from_f64(*n).unwrap_or(serde_json::Number::from(0)))
            }
        },
        Value::String(s) => JsonValue::String(s.clone()),
        Value::Array(arr) => JsonValue::Array(arr.iter().map(value_to_json).collect()),
        Value::Object(obj) => {
            let mut map = Map::new();
            for (k, v) in obj {
                map.insert(k.clone(), value_to_json(v));
            }
            JsonValue::Object(map)
        }
    }
}
