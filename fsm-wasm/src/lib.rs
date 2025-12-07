use wasm_bindgen::prelude::*;
use fsm_core::RuntimeMachine;

#[wasm_bindgen]
pub struct FsmRunner {
    machine: RuntimeMachine,
}

#[wasm_bindgen]
impl FsmRunner {
    #[wasm_bindgen(constructor)]
    pub fn new(json_definition: &str) -> Result<FsmRunner, JsValue> {
        let machine = RuntimeMachine::from_json(json_definition)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse machine: {}", e)))?;
            
        Ok(FsmRunner { machine })
    }

    pub fn transition(&mut self, event: &str) -> Option<String> {
        // Guard Implementation for Demo
        use std::collections::HashMap;
        let mut guards: HashMap<String, Box<dyn Fn(&serde_json::Value) -> bool>> = HashMap::new();
        
        // Register "can_open" guard: checks if context.locked is FALSE
        guards.insert("can_open".to_string(), Box::new(|ctx| {
            if let Some(locked) = ctx.get("locked").and_then(|v| v.as_bool()) {
                !locked
            } else {
                true // Default to allowed if 'locked' not present
            }
        }));

        self.machine.transition(event, &guards)
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
