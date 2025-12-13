use wasm_bindgen::prelude::*;
use fsm_core::{RuntimeMachine, MachineDefinition};
use std::collections::HashMap;
use std::rc::Rc;

#[wasm_bindgen]
pub struct WasmMachine {
     machine: RuntimeMachine,
     guards: HashMap<String, Rc<dyn Fn(&serde_json::Value, &serde_json::Value) -> bool>>,
     actions: HashMap<String, Rc<dyn Fn(&mut serde_json::Value, &serde_json::Value) -> ()>>,
}

#[wasm_bindgen]
pub struct SetupObject {
    config: JsValue,
}

#[wasm_bindgen]
impl SetupObject {
    #[wasm_bindgen(js_name = createMachine)]
    pub fn create_wasm_machine(&self, val: JsValue) -> Result<WasmMachine, JsValue> {
         // Create the inner machine using FsmRunner constructor logic
         let runner = FsmRunner::new(val)?;
         let machine = runner.machine.clone();

         let mut guards = HashMap::new();
         let mut actions = HashMap::new();
         
         // Parse config
         parse_implementations(&self.config, &mut guards, &mut actions);

         Ok(WasmMachine {
             machine,
             guards,
             actions
         })
    }
}

#[wasm_bindgen(js_name = setup)]
pub fn setup_wasm_machine(config: JsValue) -> SetupObject {
    SetupObject { config }
}

#[wasm_bindgen]
pub fn assign(assignment: JsValue) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.assign"));
    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("assignment"), &assignment);
    obj.into()
}

#[wasm_bindgen]
impl WasmMachine {
    // Hidden / internal use mainly, but exposed to JS
    pub fn spawn_runner(&self, options: JsValue) -> Result<FsmRunner, JsValue> {
         let mut runner = FsmRunner { 
            machine: self.machine.clone(), 
            inspector: None,
            guards: self.guards.clone(),
            actions: self.actions.clone(),
        };

        // Apply options
        if options.is_object() {
             let inspect_key = JsValue::from_str("inspect");
             let inspect_val = js_sys::Reflect::get(&options, &inspect_key).unwrap_or(JsValue::UNDEFINED);
             if inspect_val.is_function() {
                  let func: js_sys::Function = inspect_val.dyn_into().unwrap();
                  runner.set_inspector(func);
             }
             
             // Overrides
             parse_implementations(&options, &mut runner.guards, &mut runner.actions);
        }
        
        Ok(runner)
    }
}

#[wasm_bindgen]
pub struct FsmRunner {
    machine: RuntimeMachine,
    inspector: Option<js_sys::Function>,
    guards: HashMap<String, Rc<dyn Fn(&serde_json::Value, &serde_json::Value) -> bool>>,
    actions: HashMap<String, Rc<dyn Fn(&mut serde_json::Value, &serde_json::Value) -> ()>>,
}

// Helper to parse "guards" and "actions" from a JS object (either implementations object or options object)
fn parse_implementations(
    impl_obj: &JsValue,
    guards_out: &mut HashMap<String, Rc<dyn Fn(&serde_json::Value, &serde_json::Value) -> bool>>,
    actions_out: &mut HashMap<String, Rc<dyn Fn(&mut serde_json::Value, &serde_json::Value) -> ()>>,
) {
    if !impl_obj.is_object() {
        return;
    }
    let obj = js_sys::Object::from(impl_obj.clone());

    // --- Guards ---
    let guards_key = JsValue::from_str("guards");
    let guards_val = js_sys::Reflect::get(&obj, &guards_key).unwrap_or(JsValue::UNDEFINED);
    if guards_val.is_object() {
         let guards_map = js_sys::Object::from(guards_val);
         let keys = js_sys::Object::keys(&guards_map);
         for i in 0..keys.length() {
             let key_js = keys.get(i);
             let key_str = key_js.as_string().unwrap_or_default();
             
             let guard_fn_js = js_sys::Reflect::get(&guards_map, &key_js).unwrap_or(JsValue::UNDEFINED);
             
             if let Ok(guard_fn) = guard_fn_js.dyn_into::<js_sys::Function>() {
                 guards_out.insert(key_str, Rc::new(move |ctx, evt| {
                     // Use JSON round-trip to ensure plain JS Objects (not Maps)
                     let ctx_json = serde_json::to_string(ctx).unwrap_or("{}".to_string());
                     let ctx_js = js_sys::JSON::parse(&ctx_json).unwrap_or(JsValue::NULL);
                     
                     let evt_json = serde_json::to_string(evt).unwrap_or("{}".to_string());
                     let evt_js = js_sys::JSON::parse(&evt_json).unwrap_or(JsValue::NULL);
                     
                     let arg_obj = js_sys::Object::new();
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);

                     let res = guard_fn.call1(&JsValue::NULL, &arg_obj).unwrap_or(JsValue::FALSE);
                     res.as_bool().unwrap_or(false)
                 }));
             }
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
             
             // Check for static assignment (xstate 5 assign)
             let assign_key = JsValue::from_str("assignment");
             let assign_val = js_sys::Reflect::get(&action_item, &assign_key).unwrap_or(JsValue::UNDEFINED);
             
             // Case 1: Dynamic Assignment (assignment is a function)
             if let Ok(assign_fn) = assign_val.clone().dyn_into::<js_sys::Function>() {
                 actions_out.insert(key_str, Rc::new(move |ctx_mut, evt| {
                     // Use JSON round-trip
                     let ctx_json = serde_json::to_string(ctx_mut).unwrap_or("{}".to_string());
                     let ctx_js = js_sys::JSON::parse(&ctx_json).unwrap_or(JsValue::NULL);
                     
                     let evt_json = serde_json::to_string(evt).unwrap_or("{}".to_string());
                     let evt_js = js_sys::JSON::parse(&evt_json).unwrap_or(JsValue::NULL);
                     
                     let arg_obj = js_sys::Object::new();
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);

                     if let Ok(res) = assign_fn.call1(&JsValue::NULL, &arg_obj) {
                         if res.is_object() {
                              let partial: serde_json::Value = serde_wasm_bindgen::from_value(res).unwrap_or(serde_json::Value::Null);
                              if let Some(partial_obj) = partial.as_object() {
                                  if let Some(ctx_obj) = ctx_mut.as_object_mut() {
                                      for (k, v) in partial_obj {
                                          ctx_obj.insert(k.clone(), v.clone());
                                      }
                                  }
                              }
                         }
                     }
                 }));
                 continue;
             }

             // Case 2: Static Assignment (assignment is an object)
             if assign_val.is_object() {
                 let partial: serde_json::Value = serde_wasm_bindgen::from_value(assign_val).unwrap_or(serde_json::Value::Null);
                 
                 if let Some(partial_obj) = partial.as_object() {
                     let partial_obj = partial_obj.clone();
                     actions_out.insert(key_str, Rc::new(move |ctx_mut, _evt| {
                         if let Some(ctx_obj) = ctx_mut.as_object_mut() {
                             for (k, v) in &partial_obj {
                                 ctx_obj.insert(k.clone(), v.clone());
                             }
                         }
                     }));
                     continue; // handled
                 }
             }

             // Fallback: Treat as function
             if let Ok(action_fn) = action_item.dyn_into::<js_sys::Function>() {
                 actions_out.insert(key_str, Rc::new(move |ctx_mut, evt| {
                     // Use JSON round-trip
                     let ctx_json = serde_json::to_string(ctx_mut).unwrap_or("{}".to_string());
                     let ctx_js = js_sys::JSON::parse(&ctx_json).unwrap_or(JsValue::NULL);
                     
                     let evt_json = serde_json::to_string(evt).unwrap_or("{}".to_string());
                     let evt_js = js_sys::JSON::parse(&evt_json).unwrap_or(JsValue::NULL);
                     
                     let arg_obj = js_sys::Object::new();
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);

                     if let Ok(res) = action_fn.call1(&JsValue::NULL, &arg_obj) {
                         if res.is_object() {
                              let partial: serde_json::Value = serde_wasm_bindgen::from_value(res).unwrap_or(serde_json::Value::Null);
                              if let Some(partial_obj) = partial.as_object() {
                                  if let Some(ctx_obj) = ctx_mut.as_object_mut() {
                                      for (k, v) in partial_obj {
                                          ctx_obj.insert(k.clone(), v.clone());
                                      }
                                  }
                              }
                         }
                     }
                 }));
             }
         }
    }
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
pub fn create_actor(machine_def: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
    // 1. Check if machine_def is a WasmMachine (via spawn_runner method)
    if machine_def.is_object() {
        let spawn_fn = js_sys::Reflect::get(&machine_def, &JsValue::from_str("spawn_runner")).unwrap_or(JsValue::UNDEFINED);
        if let Ok(func) = spawn_fn.dyn_into::<js_sys::Function>() {
             // It is a WasmMachine (or compatible object)
             let runner_val = func.call1(&machine_def, &options)?;
             return Ok(runner_val);
        }
    }

    // 2. Fallback: Create new FsmRunner
    let mut runner = FsmRunner::new(machine_def)?;
    
    if options.is_object() {
        let inspect_key = JsValue::from_str("inspect");
        let inspect_val = js_sys::Reflect::get(&options, &inspect_key).unwrap_or(JsValue::UNDEFINED);
        if inspect_val.is_function() {
             let func: js_sys::Function = inspect_val.dyn_into().unwrap();
             runner.set_inspector(func);
        }
    }

    // Convert FsmRunner to JsValue
    Ok(runner.into())
}
