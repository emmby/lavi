use wasm_bindgen::prelude::*;
use fsm_core::{RuntimeMachine, MachineDefinition};
use std::collections::HashMap;

#[wasm_bindgen]
pub struct FsmRunner {
    machine: RuntimeMachine,
    inspector: Option<js_sys::Function>,
    guards: HashMap<String, Box<dyn Fn(&serde_json::Value, &serde_json::Value) -> bool>>,
    actions: HashMap<String, Box<dyn Fn(&mut serde_json::Value, &serde_json::Value) -> ()>>,
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
             let json_string = js_sys::JSON::stringify(&val)
                .map_err(|_| JsValue::from_str("Failed to stringify machine definition"))?
                .as_string()
                .ok_or(JsValue::from_str("Machine definition JSON is not a string"))?;

             let definition: MachineDefinition = serde_json::from_str(&json_string)
                .map_err(|e| JsValue::from_str(&format!("Failed to parse definition: {}", e)))?;

             let current_state = definition.initial.clone();
             let context = definition.context.clone().unwrap_or(serde_json::Value::Null);
             RuntimeMachine {
                definition,
                current_state,
                context
             }
        };
            
        Ok(FsmRunner { 
            machine, 
            inspector: None,
            guards: HashMap::new(),
            actions: HashMap::new(),
        })
    }

    pub fn set_inspector(&mut self, callback: js_sys::Function) {
        self.inspector = Some(callback);
    }

    pub fn start(&self) {
        if let Some(inspector) = &self.inspector {
             let obj = js_sys::Object::new();
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.snapshot"));
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("status"), &JsValue::from_str("active"));
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("value"), &JsValue::from_str(self.machine.state()));
             
             // Fix: Use JSON parse to avoid Map serialization
             let ctx_json = serde_json::to_string(&self.machine.context).unwrap_or("{}".to_string());
             let ctx_val = js_sys::JSON::parse(&ctx_json).unwrap_or(JsValue::NULL);
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("context"), &ctx_val);

             // Parity Fields
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("tags"), &js_sys::Array::new());
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("historyValue"), &js_sys::Object::new());
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("children"), &js_sys::Object::new());

             let _ = inspector.call1(&JsValue::NULL, &obj);
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

        if let Some(inspector) = &self.inspector {
             let obj = js_sys::Object::new();
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.event"));
             
             // Fix: Use JSON parse to avoid Map serialization
             let evt_json = serde_json::to_string(&event_payload).unwrap_or("{}".to_string());
             let evt_val = js_sys::JSON::parse(&evt_json).unwrap_or(JsValue::NULL);
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("event"), &evt_val);

             let _ = inspector.call1(&JsValue::NULL, &obj);
        }
        
        let result = self.machine.transition(&event_name, &event_payload, &self.guards, &self.actions);

        // 2. Inspect Snapshot
        if let Some(inspector) = &self.inspector {
             let obj = js_sys::Object::new();
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.snapshot"));
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("status"), &JsValue::from_str("active"));
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("value"), &JsValue::from_str(self.machine.state()));
             
             // Fix: Use JSON parse to avoid Map serialization
             let ctx_json = serde_json::to_string(&self.machine.context).unwrap_or("{}".to_string());
             let ctx_val = js_sys::JSON::parse(&ctx_json).unwrap_or(JsValue::NULL);
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("context"), &ctx_val);

             // Parity Fields
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("tags"), &js_sys::Array::new());
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("historyValue"), &js_sys::Object::new());
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("children"), &js_sys::Object::new());

             let _ = inspector.call1(&JsValue::NULL, &obj);
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
    
    pub fn send(&mut self, event: JsValue) -> Option<String> {
        self.transition(event)
    }

    pub fn subscribe(&mut self, callback: js_sys::Function) {
        self.set_inspector(callback);
    }

    pub fn get_snapshot(&self) -> JsValue {
         let obj = js_sys::Object::new();
         let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.snapshot"));
         let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("status"), &JsValue::from_str("active"));
         let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("value"), &JsValue::from_str(self.machine.state()));
         
         let ctx_json = serde_json::to_string(&self.machine.context).unwrap_or("{}".to_string());
         let ctx_val = js_sys::JSON::parse(&ctx_json).unwrap_or(JsValue::NULL);
         let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("context"), &ctx_val);

         let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("tags"), &js_sys::Array::new());
         let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("historyValue"), &js_sys::Object::new());
         let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("children"), &js_sys::Object::new());

         obj.into()
    }
}

#[wasm_bindgen(js_name = createActor)]
pub fn create_actor(machine_def: JsValue, options: JsValue) -> Result<FsmRunner, JsValue> {
    let mut runner = FsmRunner::new(machine_def)?;
    
    if options.is_object() {
        let inspect_key = JsValue::from_str("inspect");
        let inspect_val = js_sys::Reflect::get(&options, &inspect_key).unwrap_or(JsValue::UNDEFINED);
        if inspect_val.is_function() {
             let func: js_sys::Function = inspect_val.dyn_into().unwrap();
             runner.set_inspector(func);
        }
    }

    Ok(runner)
}
