use wasm_bindgen::prelude::*;
use portable_xstate_core::{RuntimeMachine, Value};
use crate::utils::{json_to_value, value_to_json};
use std::collections::BTreeMap;
use std::rc::Rc;

mod utils;

#[wasm_bindgen]
pub struct WasmMachine {
     machine: RuntimeMachine,
     guards: BTreeMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
     actions: BTreeMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
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
         let runner = FsmRunner::new(val, JsValue::UNDEFINED)?;
         let machine = runner.machine.clone();

         let mut guards = runner.guards.clone();
         let mut actions = runner.actions.clone();
         
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
    // Create a function named "assign" that returns undefined, matching xstate
    let func_constructor = js_sys::Function::new_no_args("return function assign() {};");
    let func = func_constructor.call0(&JsValue::NULL).unwrap();

    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("type"), &JsValue::from_str("xstate.assign"));
    let _ = js_sys::Reflect::set(&func, &JsValue::from_str("assignment"), &assignment);
    
    func.into()
}

#[wasm_bindgen]
impl WasmMachine {
    #[wasm_bindgen(js_name = createActor)]
    pub fn create_actor_method(&self, options: JsValue) -> Result<FsmRunner, JsValue> {
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
    guards: BTreeMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    actions: BTreeMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
}

// Helper to parse "guards" and "actions" from a JS object (either implementations object or options object)
fn parse_implementations(
    impl_obj: &JsValue,
    guards_out: &mut BTreeMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    actions_out: &mut BTreeMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
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
                 guards_out.insert(key_str, Rc::new(move |ctx, evt, params| {
                     // Convert Core Value -> JS Value via JSON
                     let ctx_json = value_to_json(ctx);
                     let ctx_str = serde_json::to_string(&ctx_json).unwrap_or("null".to_string());
                     let ctx_js = js_sys::JSON::parse(&ctx_str).unwrap_or(JsValue::NULL);
                     
                     let evt_json = value_to_json(evt);
                     let evt_str = serde_json::to_string(&evt_json).unwrap_or("null".to_string());
                     let evt_js = js_sys::JSON::parse(&evt_str).unwrap_or(JsValue::NULL);

                     let params_js = if let Some(p) = params {
                         let p_json = value_to_json(p);
                         let p_str = serde_json::to_string(&p_json).unwrap_or("null".to_string());
                         js_sys::JSON::parse(&p_str).unwrap_or(JsValue::NULL)
                     } else {
                         JsValue::UNDEFINED
                     };
                     
                     let arg_obj = js_sys::Object::new();
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
 
                     let res = guard_fn.call2(&JsValue::NULL, &arg_obj, &params_js).unwrap_or(JsValue::FALSE);
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
                 actions_out.insert(key_str, Rc::new(move |ctx_mut, evt, _params| {
                     let ctx_json = value_to_json(ctx_mut);
                     let ctx_str = serde_json::to_string(&ctx_json).unwrap_or("null".to_string());
                     let ctx_js = js_sys::JSON::parse(&ctx_str).unwrap_or(JsValue::NULL);
                     
                     let evt_json = value_to_json(evt);
                     let evt_str = serde_json::to_string(&evt_json).unwrap_or("null".to_string());
                     let evt_js = js_sys::JSON::parse(&evt_str).unwrap_or(JsValue::NULL);
                     
                     let arg_obj = js_sys::Object::new();
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);

                     if let Ok(res) = assign_fn.call1(&JsValue::NULL, &arg_obj) {
                         if res.is_object() {
                               let partial: serde_json::Value = serde_wasm_bindgen::from_value(res).unwrap_or(serde_json::Value::Null);
                               // Merge back into ctx_mut (Core Value)
                               let partial_value = json_to_value(partial);
                               if let Some(partial_obj) = partial_value.as_object() {
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
                 let partial_json: serde_json::Value = serde_wasm_bindgen::from_value(assign_val).unwrap_or(serde_json::Value::Null);
                 let partial_value = json_to_value(partial_json);

                 if let Some(partial_obj) = partial_value.as_object() {
                     let partial_obj = partial_obj.clone();
                     actions_out.insert(key_str, Rc::new(move |ctx_mut, _evt, _params| {
                         if let Some(ctx_obj) = ctx_mut.as_object_mut() {
                             for (k, v) in &partial_obj {
                                 ctx_obj.insert(k.clone(), v.clone());
                             }
                         }
                     }));
                     continue; // handled
                 }
             }

             // Fallback: Treat as function (Generic Action)
             if let Ok(action_fn) = action_item.dyn_into::<js_sys::Function>() {
                 actions_out.insert(key_str, Rc::new(move |ctx_mut, evt, params| {
                     let ctx_json = value_to_json(ctx_mut);
                     let ctx_str = serde_json::to_string(&ctx_json).unwrap_or("null".to_string());
                     let ctx_js = js_sys::JSON::parse(&ctx_str).unwrap_or(JsValue::NULL);
                     
                     let evt_json = value_to_json(evt);
                     let evt_str = serde_json::to_string(&evt_json).unwrap_or("null".to_string());
                     let evt_js = js_sys::JSON::parse(&evt_str).unwrap_or(JsValue::NULL);
                     
                     let arg_obj = js_sys::Object::new();
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);

                     let params_js = if let Some(p) = params {
                         let p_json = value_to_json(p);
                         let p_str = serde_json::to_string(&p_json).unwrap_or("null".to_string());
                         js_sys::JSON::parse(&p_str).unwrap_or(JsValue::NULL)
                     } else {
                         JsValue::UNDEFINED
                     };

                     // Call action with (args, params)
                     let _ = action_fn.call2(&JsValue::NULL, &arg_obj, &params_js);
                 }));
             }
         }
    }
}

// Helper function to extract inline assignments
fn extract_inline_assignments(val: &JsValue) -> Result<(JsValue, BTreeMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>), JsValue> {
    let mut actions_out = BTreeMap::new();
    let mut action_counter = 0;

    if !val.is_object() {
         return Ok((val.clone(), actions_out));
    }
    let definition = js_sys::Object::from(val.clone());

    let states_key = JsValue::from_str("states");
    let states_val = js_sys::Reflect::get(&definition, &states_key).unwrap_or(JsValue::UNDEFINED);

    if states_val.is_object() {
        let states_obj = js_sys::Object::from(states_val);
        let state_keys = js_sys::Object::keys(&states_obj);

        for i in 0..state_keys.length() {
            let state_key = state_keys.get(i);
            
            let state_node = js_sys::Reflect::get(&states_obj, &state_key).unwrap_or(JsValue::UNDEFINED);
            if state_node.is_object() {
                let state_obj = js_sys::Object::from(state_node);
                
                // Process entry
                process_action_list(&state_obj, "entry", &mut actions_out, &mut action_counter);
                
                // Process exit
                process_action_list(&state_obj, "exit", &mut actions_out, &mut action_counter);

                // Process on -> transitions -> actions
                let on_key = JsValue::from_str("on");
                let on_val = js_sys::Reflect::get(&state_obj, &on_key).unwrap_or(JsValue::UNDEFINED);
                if on_val.is_object() {
                     let on_obj = js_sys::Object::from(on_val);
                     let event_keys = js_sys::Object::keys(&on_obj);
                     for j in 0..event_keys.length() {
                         let evt_key = event_keys.get(j);
                         let transitions_val = js_sys::Reflect::get(&on_obj, &evt_key).unwrap_or(JsValue::UNDEFINED);
                         
                         // Transitions can be array or object
                         if js_sys::Array::is_array(&transitions_val) {
                             let arr = js_sys::Array::from(&transitions_val);
                             for k in 0..arr.length() {
                                 let t_val = arr.get(k);
                                 if t_val.is_object() {
                                     let t_obj = js_sys::Object::from(t_val);
                                     process_action_list(&t_obj, "actions", &mut actions_out, &mut action_counter);
                                 }
                             }
                         } else if transitions_val.is_object() {
                             let t_obj = js_sys::Object::from(transitions_val);
                             process_action_list(&t_obj, "actions", &mut actions_out, &mut action_counter);
                         }
                     }
                }
            }
        }
    }

    Ok((val.clone(), actions_out))
}

fn process_action_list(
    parent_obj: &js_sys::Object, 
    prop_name: &str, 
    actions_out: &mut BTreeMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    counter: &mut u32
) {
    let prop_key = JsValue::from_str(prop_name);
    let val = js_sys::Reflect::get(parent_obj, &prop_key).unwrap_or(JsValue::UNDEFINED);

    if val.is_undefined() { return; }

    if js_sys::Array::is_array(&val) {
        let arr: &js_sys::Array = val.unchecked_ref();
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

fn convert_inline_action(
    action_item: &JsValue,
    actions_out: &mut BTreeMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    counter: &mut u32
) -> Option<JsValue> {
     let is_obj = action_item.is_object();
     let is_func = action_item.is_function();
     
     if !is_obj && !is_func { return None; }
     
     // Check if it's an assignment object like { type: 'xstate.assign', assignment: ... }
     let type_key = JsValue::from_str("type");
     let type_val = js_sys::Reflect::get(action_item, &type_key).unwrap_or(JsValue::UNDEFINED);
     let type_str = type_val.as_string().unwrap_or_default();
     
     if type_str == "xstate.assign" {
         let assign_key = JsValue::from_str("assignment");
         let assign_val = js_sys::Reflect::get(action_item, &assign_key).unwrap_or(JsValue::UNDEFINED);
         
         if !assign_val.is_undefined() {
             // Lift ALL assignments (function or object) to preserve functions
             *counter += 1;
             let new_action_name = format!("generated_assign_{}", counter);
             
             // Create closure wrapper
             actions_out.insert(new_action_name.clone(), Rc::new(move |ctx_mut, evt, _params| {
                  // Common Setup
                  let ctx_json = value_to_json(ctx_mut);
                  let ctx_str = serde_json::to_string(&ctx_json).unwrap_or("null".to_string());
                  let ctx_js = js_sys::JSON::parse(&ctx_str).unwrap_or(JsValue::NULL);
                  
                  let evt_json = value_to_json(evt);
                  let evt_str = serde_json::to_string(&evt_json).unwrap_or("null".to_string());
                  let evt_js = js_sys::JSON::parse(&evt_str).unwrap_or(JsValue::NULL);
                  
                  let arg_obj = js_sys::Object::new();
                  let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                  let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);

                  // Logic
                  if let Ok(assign_fn) = assign_val.clone().dyn_into::<js_sys::Function>() {
                      // Case 1: assignment is a function directly
                      if let Ok(res) = assign_fn.call1(&JsValue::NULL, &arg_obj) {
                          if res.is_object() {
                               let partial: serde_json::Value = serde_wasm_bindgen::from_value(res).unwrap_or(serde_json::Value::Null);
                               let partial_core = json_to_value(partial);
                               if let Some(partial_obj) = partial_core.as_object() {
                                   if let Some(ctx_obj) = ctx_mut.as_object_mut() {
                                       for (k, v) in partial_obj {
                                           ctx_obj.insert(k.clone(), v.clone());
                                       }
                                   }
                               }
                          }
                     }
                 } else if assign_val.is_object() {
                     // Case 2: assignment is an object { key: value_or_fn }
                     let assign_obj = js_sys::Object::from(assign_val.clone());
                     let keys = js_sys::Object::keys(&assign_obj);
                     
                     if let Some(ctx_obj) = ctx_mut.as_object_mut() {
                         for i in 0..keys.length() {
                             let key_js = keys.get(i);
                             let val_or_fn = js_sys::Reflect::get(&assign_obj, &key_js).unwrap_or(JsValue::UNDEFINED);
                             
                             let final_val_js = if let Ok(func) = val_or_fn.clone().dyn_into::<js_sys::Function>() {
                                 match func.call1(&JsValue::NULL, &arg_obj) {
                                     Ok(v) => v,
                                     Err(e) => {
                                         web_sys::console::log_2(&JsValue::from_str("Error executing assign function:"), &e);
                                         JsValue::UNDEFINED
                                     }
                                 }
                             } else {
                                 val_or_fn
                             };
                             
                             // Convert JS value to Value (via intermediate JSON if needed)
                             let final_val_json: serde_json::Value = serde_wasm_bindgen::from_value(final_val_js).unwrap_or(serde_json::Value::Null);
                             let final_val = json_to_value(final_val_json);
                             
                             let key_str = key_js.as_string().unwrap_or_default();
                             
                             // Update context
                             ctx_obj.insert(key_str, final_val);
                         }
                     }
                 }
             }));
             
             // Return replacement object: { type: "generated_assign_N", params: null }
             let new_obj = js_sys::Object::new();
             let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&new_action_name));
             return Some(new_obj.into());
         }
     }
     
     None
}

#[wasm_bindgen]
impl FsmRunner {
    #[wasm_bindgen(constructor)]
    pub fn new(val: JsValue, options: JsValue) -> Result<FsmRunner, JsValue> {
        let mut guards: BTreeMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>> = BTreeMap::new();
        let mut actions = BTreeMap::new();

        // Parse options if provided
        parse_implementations(&options, &mut guards, &mut actions);

        let machine = if val.is_string() {
             let json_definition = val.as_string().unwrap();
             let def = portable_xstate_json::create_machine(&json_definition)
                .map_err(|e| JsValue::from_str(&format!("Failed to parse machine: {}", e)))?;
             RuntimeMachine::new(def)
        } else {
             // If object, pre-process to "lift" inline implementations (like assign functions)
             let (cleaned_val, extra_actions) = extract_inline_assignments(&val)?;
             actions.extend(extra_actions);
 
             let json_string = js_sys::JSON::stringify(&cleaned_val)
                .map_err(|_| JsValue::from_str("Failed to stringify machine definition"))?
                .as_string()
                .ok_or(JsValue::from_str("Machine definition JSON is not a string"))?;

             let definition = portable_xstate_json::create_machine(&json_string)
                .map_err(|e| JsValue::from_str(&format!("Failed to parse definition: {}", e)))?;

             RuntimeMachine::new(definition)
        };
            
        Ok(FsmRunner { 
            machine, 
            inspector: None,
            guards,
            actions,
        })
    }


    pub(crate) fn set_inspector(&mut self, callback: js_sys::Function) {
        self.inspector = Some(callback);
    }

    pub fn start(&mut self) {
        // 1. Execute entry actions for the initial state
        let current_state = self.machine.state();
        let actions_map = self.actions.clone();
        
        let entry_actions = if let Some(state_def) = self.machine.get_state_definition(&current_state) {
            state_def.entry.clone()
        } else {
            Vec::new()
        };

        for action in entry_actions {
            let action_type = match &action {
                portable_xstate_core::Action::Named(s) => s.clone(),
                portable_xstate_core::Action::Assign(a) => a.action_type.clone(),
                portable_xstate_core::Action::Generic(g) => g.action_type.clone(),
            };

            if let Some(impl_fn) = actions_map.get(&action_type) {
                 let params = match &action {
                     portable_xstate_core::Action::Generic(g) => &g.params,
                     _ => &None
                 };

                 // Create init event
                 // Pure Rust value!
                 let init_event_value = Value::Object(BTreeMap::from([("type".to_string(), Value::String("xstate.init".to_string()))]));
                 
                 // Execute action
                 (impl_fn)(&mut self.machine.context, &init_event_value, params);
            }
        }

        // 2. Inspector Snapshot
        if let Some(inspector) = &self.inspector {
             let obj = self.get_snapshot();
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.snapshot"));
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("status"), &JsValue::from_str("active"));
             let _ = inspector.call1(&JsValue::NULL, &obj);
        }
    }

    #[wasm_bindgen(js_name = getSnapshot)]
    pub fn get_snapshot(&self) -> JsValue {
        let obj = js_sys::Object::new();
        
        // Value: Convert dot-path to object or string
        let state_val = self.path_to_value_object(&self.machine.state());
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("value"), &state_val);

        // Context
        // Convert Core Value -> JS Value via JSON (re-use utils)
        let ctx_json = value_to_json(&self.machine.context);
        let ctx_str = serde_json::to_string(&ctx_json).unwrap_or("null".to_string());
        let ctx_val = js_sys::JSON::parse(&ctx_str).unwrap_or(JsValue::NULL);
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("context"), &ctx_val);

        // Parity Fields
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("tags"), &js_sys::Array::new());
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("historyValue"), &js_sys::Object::new());
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("children"), &js_sys::Object::new());
        
        obj.into()
    }

    fn path_to_value_object(&self, path: &str) -> JsValue {
        if !path.contains('.') {
            return JsValue::from_str(path);
        }
        
        // "active.one" -> { active: "one" }
        // "active.nested.three" -> { active: { nested: "three" } }
        let parts: Vec<&str> = path.split('.').collect();
        self.recursive_build_obj(&parts)
    }

    fn recursive_build_obj(&self, parts: &[&str]) -> JsValue {
        if parts.len() == 1 {
            return JsValue::from_str(parts[0]);
        }
        
        let obj = js_sys::Object::new();
        let key = parts[0];
        let val = self.recursive_build_obj(&parts[1..]);
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(key), &val);
        
        obj.into()
    }

    pub fn send(&mut self, event: JsValue) {
        let (event_name, event_payload) = if event.is_string() {
            let s = event.as_string().unwrap();
            let payload = Value::Object(BTreeMap::from([("type".to_string(), Value::String(s.clone()))]));
            (s, payload)
        } else {
             // Assume object
             let payload_json: serde_json::Value = serde_wasm_bindgen::from_value(event.clone()).unwrap_or(serde_json::Value::Null);
             let payload = json_to_value(payload_json);
             
             let name = payload.get("type").and_then(|t| t.as_str()).unwrap_or("UNKNOWN").to_string();
             (name, payload)
        };

        if let Some(inspector) = &self.inspector {
             let obj = js_sys::Object::new();
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.event"));
             
             let evt_json = value_to_json(&event_payload); 
             let evt_str = serde_json::to_string(&evt_json).unwrap_or("{}".to_string());
             let evt_val = js_sys::JSON::parse(&evt_str).unwrap_or(JsValue::NULL);
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("event"), &evt_val);

             let _ = inspector.call1(&JsValue::NULL, &obj);
        }
        
        let _ = self.machine.transition(&event_name, &event_payload, &self.guards, &self.actions);

        // 2. Inspect Snapshot
        if let Some(inspector) = &self.inspector {
             let obj = self.get_snapshot();
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.snapshot"));
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("status"), &JsValue::from_str("active"));
             let _ = inspector.call1(&JsValue::NULL, &obj);
        }
    }

    #[wasm_bindgen(js_name = subscribe)]
    pub fn subscribe(&mut self, callback: js_sys::Function) {
        self.set_inspector(callback);
    }
}

#[wasm_bindgen(js_name = createActor)]
pub fn create_actor(machine_def: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
    // 1. Check if machine_def is a WasmMachine (via createActor method)
    if machine_def.is_object() {
        let spawn_fn = js_sys::Reflect::get(&machine_def, &JsValue::from_str("createActor")).unwrap_or(JsValue::UNDEFINED);
        if let Ok(func) = spawn_fn.dyn_into::<js_sys::Function>() {
             // It is a WasmMachine (or compatible object)
             let runner_val = func.call1(&machine_def, &options)?;
             return Ok(runner_val);
        }
    }

    // 2. Fallback: Create new FsmRunner
    let mut runner = FsmRunner::new(machine_def, options.clone())?;
    
    if options.is_object() {
        let inspect_key = JsValue::from_str("inspect");
        let inspect_val = js_sys::Reflect::get(&options, &inspect_key).unwrap_or(JsValue::UNDEFINED);
        if inspect_val.is_function() {
             let func: js_sys::Function = inspect_val.dyn_into().unwrap();
             runner.set_inspector(func);
        }

        parse_implementations(&options, &mut runner.guards, &mut runner.actions);
    }

    // Convert FsmRunner to JsValue
    Ok(runner.into())
}
