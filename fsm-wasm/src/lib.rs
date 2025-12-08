use wasm_bindgen::prelude::*;
use fsm_core::{RuntimeMachine, MachineDefinition};

#[wasm_bindgen]
pub struct FsmRunner {
    machine: RuntimeMachine,
    inspector: Option<js_sys::Function>,
}

#[wasm_bindgen]
impl FsmRunner {
    #[wasm_bindgen(constructor)]
    pub fn new(val: JsValue) -> Result<FsmRunner, JsValue> {
        let machine = if val.is_string() {
             let json_definition = val.as_string().unwrap();
             RuntimeMachine::from_json(&json_definition)
                .map_err(|e| JsValue::from_str(&format!("Failed to parse machine: {}", e)))?
        } else {
             let definition: MachineDefinition = serde_wasm_bindgen::from_value(val)?;
             let current_state = definition.initial.clone();
             let context = definition.context.clone().unwrap_or(serde_json::Value::Null);
             RuntimeMachine {
                definition,
                current_state,
                context
             }
        };
            
        Ok(FsmRunner { machine, inspector: None })
    }

    pub fn set_inspector(&mut self, callback: js_sys::Function) {
        self.inspector = Some(callback);
    }

    pub fn start(&self) {
        if let Some(inspector) = &self.inspector {
             let snap_obj = serde_json::json!({
                 "type": "@xstate.snapshot",
                 "context": self.machine.context,
                 "value": self.machine.state(),
                 "status": "active"
             });
             if let Ok(s) = snap_obj.to_string().parse::<String>() {
                 let _ = inspector.call1(&JsValue::NULL, &JsValue::from_str(&s));
             }
        }
    }

    pub fn transition(&mut self, event: JsValue) -> Option<String> {
        let (event_name, event_payload) = if event.is_string() {
            let s = event.as_string().unwrap();
            (s.clone(), serde_json::json!({ "type": s }))
        } else {
             // Assume object
             let payload: serde_json::Value = serde_wasm_bindgen::from_value(event.clone()).unwrap_or(serde_json::Value::Null);
             let name = payload.get("type").and_then(|t| t.as_str()).unwrap_or("UNKNOWN").to_string();
             (name, payload)
        };

        // 1. Inspect Event
        if let Some(inspector) = &self.inspector {
             let evt_obj = serde_json::json!({
                 "type": "@xstate.event",
                 "event": event_payload
             });
             if let Ok(s) = evt_obj.to_string().parse::<String>() {
                 let _ = inspector.call1(&JsValue::NULL, &JsValue::from_str(&s));
             }
        }

        // Guard Implementation for Demo
        use std::collections::HashMap;
        let mut guards: HashMap<String, Box<dyn Fn(&serde_json::Value, &serde_json::Value) -> bool>> = HashMap::new();
        
        // Register "can_open" guard: checks if context.locked is FALSE
        guards.insert("can_open".to_string(), Box::new(|ctx, _evt| {
            if let Some(locked) = ctx.get("locked").and_then(|v| v.as_bool()) {
                !locked
            } else {
                true // Default to allowed if 'locked' not present
            }
        }));

        // Actions Implementation for Demo
        let mut actions: HashMap<String, Box<dyn Fn(&mut serde_json::Value, &serde_json::Value) -> ()>> = HashMap::new();
        
        // Handle "unlock" action - triggered by UNLOCK event
        actions.insert("unlock".to_string(), Box::new(|ctx, _evt| {
            if let Some(obj) = ctx.as_object_mut() {
                obj.insert("locked".to_string(), serde_json::Value::Bool(false));
            }
        }));

        // Handle "lock" action - triggered by LOCK event
        actions.insert("lock".to_string(), Box::new(|ctx, _evt| {
            if let Some(obj) = ctx.as_object_mut() {
                obj.insert("locked".to_string(), serde_json::Value::Bool(true));
            }
        }));

        actions.insert("increment".to_string(), Box::new(|ctx, _evt| {
             let new_val = if let Some(count) = ctx.get("count").and_then(|v| v.as_i64()) {
                count + 1
            } else {
                1
            };
            if let Some(obj) = ctx.as_object_mut() {
                obj.insert("count".to_string(), serde_json::Value::Number(new_val.into()));
            }
        }));

        actions.insert("reset".to_string(), Box::new(|ctx, _evt| {
            if let Some(obj) = ctx.as_object_mut() {
                obj.insert("count".to_string(), serde_json::Value::Number(0.into()));
            }
        }));
        
        actions.insert("log_transition".to_string(), Box::new(|ctx, _evt| {
            if let Some(obj) = ctx.as_object_mut() {
                 obj.insert("last_transition".to_string(), serde_json::Value::String("occurred".to_string()));
            }
        }));

        actions.insert("set_exited_a".to_string(), Box::new(|ctx, _evt| {
            if let Some(obj) = ctx.as_object_mut() {
                obj.insert("exited_a".to_string(), serde_json::Value::Bool(true));
            }
        }));

        actions.insert("set_entered_b".to_string(), Box::new(|ctx, _evt| {
             if let Some(obj) = ctx.as_object_mut() {
                obj.insert("entered_b".to_string(), serde_json::Value::Bool(true));
            }
        }));

        let result = self.machine.transition(&event_name, &event_payload, &guards, &actions);

        // 2. Inspect Snapshot
        if let Some(inspector) = &self.inspector {
             let snap_obj = serde_json::json!({
                 "type": "@xstate.snapshot",
                 "context": self.machine.context,
                 "value": self.machine.state(),
                 "status": "active"
             });
             if let Ok(s) = snap_obj.to_string().parse::<String>() {
                 let _ = inspector.call1(&JsValue::NULL, &JsValue::from_str(&s));
             }
        }

        result
    }

    pub fn set_context_bool(&mut self, key: &str, value: bool) {
        if let Some(obj) = self.machine.context.as_object_mut() {
            obj.insert(key.to_string(), serde_json::Value::Bool(value));
        } else {
            // context might be null or not an object, promote to object?
            // for now assume initialized as object
            let mut map = serde_json::Map::new();
            map.insert(key.to_string(), serde_json::Value::Bool(value));
            self.machine.context = serde_json::Value::Object(map); 
        }
    }

    pub fn current_state(&self) -> String {
        self.machine.state().to_string()
    }
    
    pub fn context_json(&self) -> String {
        self.machine.context.to_string()
    }
}
