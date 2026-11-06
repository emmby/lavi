#![allow(clippy::type_complexity, clippy::unused_unit)]
use wasm_bindgen::prelude::*;
use portable_xstate_rust::{RuntimeMachine, Value};
use crate::utils::{js_to_value, value_to_js};
use indexmap::IndexMap;
// use std::collections::BTreeMap; // Keep BTreeMap for internal maps if not critical, or move them too.
// Actually Actor uses BTreeMap for guards/actions. XState impls order matters?
// Probably better to switch guards/actions to IndexMap too for consistency.
use std::cell::RefCell;
use std::rc::Rc;

pub mod utils;

pub(crate) struct InternalState {
    pub machine: RuntimeMachine,
    pub guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    pub actions: IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct StateNode {
    pub(crate) machine: RuntimeMachine,
    pub(crate) path: String,
    pub(crate) guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    pub(crate) actions: IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    pub(crate) config: JsValue,
}

#[wasm_bindgen]
impl StateNode {
    #[wasm_bindgen(getter)]
    pub fn path(&self) -> js_sys::Array {
        let arr = js_sys::Array::new();
        if self.path.is_empty() { return arr; }
        for part in self.path.split('.') {
            arr.push(&JsValue::from_str(part));
        }
        arr
    }

    #[wasm_bindgen(getter)]
    pub fn key(&self) -> String {
        if let Some(idx) = self.path.rfind('.') {
            self.path[idx+1..].to_string()
        } else if self.path.is_empty() {
            self.machine.definition.id.clone()
        } else {
            self.path.clone()
        }
    }

    #[wasm_bindgen]
    pub fn get_config(&self) -> JsValue {
        self.config.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn config(&self) -> JsValue {
        self.config.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        if let Some(def) = self.machine.get_state_definition(&self.path) {
            if let Some(id) = &def.id {
                return id.clone();
            }
        }
        if self.path.is_empty() { 
            self.machine.definition.id.clone() 
        } else { 
            format!("{}.{}", self.machine.definition.id, self.path) 
        }
    }

    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json(&self) -> JsValue {
        let obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("id"), &JsValue::from_str(&self.id()));
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("path"), &self.path());
        obj.into()
    }


    #[wasm_bindgen(getter)]
    pub fn version(&self) -> Option<String> {
        if self.config.is_object() {
            let v = js_sys::Reflect::get(&self.config, &JsValue::from_str("version")).unwrap_or(JsValue::UNDEFINED);
            v.as_string()
        } else {
            None
        }
    }

    #[wasm_bindgen(getter)]
    pub fn types(&self) -> JsValue {
         if let Some(types_val) = &self.machine.definition.types {
             return value_to_js(types_val);
         }
         JsValue::UNDEFINED
    }

    #[wasm_bindgen(getter)]
    pub fn schemas(&self) -> JsValue {
        self.types()
    }

    #[wasm_bindgen(js_name = _get_internal_state)]
    pub fn _get_internal_state(&self) -> f64 {
        let state = InternalState {
            machine: self.machine.clone(),
            guards: self.guards.clone(),
            actions: self.actions.clone(),
        };
        Box::into_raw(Box::new(state)) as usize as f64
    }

    #[wasm_bindgen(js_name = is_wasm_machine)]
    pub fn is_wasm_machine(&self) -> bool {
        true
    }

    #[wasm_bindgen(getter)]
    pub fn events(&self) -> Vec<String> {
        let mut events_vec = Vec::new();
        let mut work_stack: Vec<&portable_xstate_rust::StateDefinition> =
            self.machine.definition.states.values().rev().collect();

        while let Some(state) = work_stack.pop() {
            // Iterate keys in definition order (IndexMap)
            for (key, transitions) in &state.on {
                if transitions.is_empty() {
                    continue;
                }
                if !events_vec.contains(key) {
                    events_vec.push(key.clone());
                }
            }
            for substate in state.states.values().rev() {
                work_stack.push(substate);
            }
        }

        // Do NOT sort. XState definition order.
        events_vec
    }

    #[wasm_bindgen(js_name = resolveState)]
    pub fn resolve_state(&self, state_val_obj: JsValue) -> JsValue {
        let value_key = JsValue::from_str("value");
        let val = if state_val_obj.is_object() {
            js_sys::Reflect::get(&state_val_obj, &value_key).unwrap_or(state_val_obj.clone())
        } else {
            state_val_obj.clone()
        };

        // Determine start state key
        let start_key = if let Some(s) = val.as_string() {
            s
        } else {
             // If val is not a string, it might be an object (compound value). 
             // We should inspect it. But for now fallback to initial.
             self.machine.definition.initial.clone()
        };

        let resolved = self.resolve_state_from_key(&start_key, &self.machine.definition.states);
        let state_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&state_obj, &JsValue::from_str("value"), &resolved);
        // Debug
        // let _ = web_sys::console::log_2(&JsValue::from_str("Resolved Value:"), &resolved);

        // Check if final
        // To do this, we need the definition of the Resolved state(s).
        // Since resolved value can be complex, it's hard to map back to single definition.
        // But for this test, value is "bar", definition "bar" has type "final".
        let is_done = self.is_state_done(&val, &self.machine.definition.states);
        if is_done {
             let _ = js_sys::Reflect::set(&state_obj, &JsValue::from_str("status"), &JsValue::from_str("done"));
        }

        // Expose history value for testing? 
        // Although the test checks `snapshot.historyValue`.
        // So we should expose it.
        let history_obj = js_sys::Object::new();
        for (k, v) in &self.machine.history_value {
             let path_str = v.get_active_paths().first().cloned().unwrap_or_default();
             let _ = js_sys::Reflect::set(&history_obj, &JsValue::from_str(k), &JsValue::from_str(&path_str));
        }
        let _ = js_sys::Reflect::set(&state_obj, &JsValue::from_str("historyValue"), &history_obj);

        state_obj.into()
    }

    fn is_state_done(&self, val: &JsValue, states: &IndexMap<String, portable_xstate_rust::StateDefinition>) -> bool {
         if let Some(key) = val.as_string() {
              if let Some(def) = states.get(&key) {
                   return def.type_.as_deref() == Some("final");
              }
         } else if val.is_object() {
              // Compound/Parallel value { key: child_val }
              let obj = js_sys::Object::from(val.clone());
              let keys = js_sys::Object::keys(&obj);
              // If empty object?
              if keys.length() == 0 { return false; }

              // For Compound state (one key): Parent is done if child is done + matches final state of parent?
              // No, parent is done if it is in a final substate.
              // Logic check:
              //   If resolving state value, we have { foo: 'bar' }.
              //   Is 'foo' done? 'foo' is compound. Child 'bar' is active.
              //   If 'bar' is final, then 'foo' is done.
              //   If Parallel: All regions must be done.
              let mut all_done = true;
              for i in 0..keys.length() {
                  let k = keys.get(i).as_string().unwrap_or_default();
                  let v = js_sys::Reflect::get(&obj, &JsValue::from_str(&k)).unwrap();
                  if let Some(def) = states.get(&k) {
                       // Recurse
                       if !self.is_state_done(&v, &def.states) {
                           all_done = false;
                           break;
                       }
                  } else {
                       all_done = false;
                  }
              }
              return all_done;
         }
         false
    }

    fn resolve_state_from_key(&self, key: &str, states: &IndexMap<String, portable_xstate_rust::StateDefinition>) -> JsValue {
        if let Some(def) = states.get(key) {
             if !def.states.is_empty() {
                 if let Some(initial) = &def.initial {
                      // Compound: Resolve initial state within THIS state's children
                      let child_res = self.resolve_state_from_key(initial, &def.states);
                      let obj = js_sys::Object::new();
                      let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(key), &child_res);
                      return obj.into();
                 } else {
                      // Parallel: Resolve ALL children
                      let parallel_val = js_sys::Object::new();
                      for child_key in def.states.keys() {
                           // Resolve each child relative to this state's children map
                           let r = self.resolve_state_from_key(child_key, &def.states);
                           if r.is_object() {
                               let r_obj = js_sys::Object::from(r);
                               // Recursive result is { child_key: ... }. Merge it.
                               // We want to merge all { region:val } objects into one parent object.
                               // Iterate keys of result (should be just one usually, but merge anyway)
                               let keys = js_sys::Object::keys(&r_obj);
                               for i in 0..keys.length() {
                                   let k = keys.get(i);
                                   let v = js_sys::Reflect::get(&r_obj, &k).unwrap_or(JsValue::UNDEFINED);
                                   let _ = js_sys::Reflect::set(&parallel_val, &k, &v);
                               }
                           }
                      }
                      let obj = js_sys::Object::new();
                      let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(key), &parallel_val);
                      return obj.into();
                 }
             }
        }
        JsValue::from_str(key)
    }

    #[wasm_bindgen]
    pub fn provide(&self, implementations: JsValue, context: Option<JsValue>) -> Result<StateNode, JsValue> {
         let mut new_machine = self.clone_machine(); 
         // Parse implementations and merge
         parse_implementations(&implementations, &mut new_machine.guards, &mut new_machine.actions);

         // Handle context override (second argument)
         if let Some(ctx_val) = context {
             if !ctx_val.is_undefined() && !ctx_val.is_null() {
                 new_machine.machine.context = js_to_value(&ctx_val);
             }
         }
         Ok(new_machine)
    }

    fn clone_machine(&self) -> StateNode {
        StateNode {
            machine: self.machine.clone(),
            path: self.path.clone(),
            guards: self.guards.clone(),
            actions: self.actions.clone(),
            config: self.config.clone(),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn states(&self) -> JsValue {
        let machine_id = &self.machine.definition.id;
        let config_states = if self.config.is_object() {
             js_sys::Reflect::get(&self.config, &JsValue::from_str("states")).unwrap_or(JsValue::UNDEFINED)
        } else {
             JsValue::UNDEFINED
        };

        map_state_map(&self.machine.definition.states, &config_states, machine_id)
    }

    #[wasm_bindgen(js_name = mergeActions)]
    pub fn merge_actions(&mut self, other: &StateNode) {
        self.actions.extend(other.actions.clone());
    }
    #[wasm_bindgen(js_name = registerAction)]
    pub fn register_action(&mut self, name: String, callback: js_sys::Function) {
        self.actions.insert(name, Rc::new(move |_ctx, _evt, _params| {
             let _ = callback.call0(&JsValue::NULL);
        }));
    }
}

// Helper to map a map of states to a JS object of StateNodes
fn map_state_map(
    states_def: &IndexMap<String, portable_xstate_rust::StateDefinition>,
    states_config: &JsValue,
    parent_id: &str
) -> JsValue {
    let obj = js_sys::Object::new();
    let config_map = if states_config.is_object() {
        js_sys::Object::from(states_config.clone())
    } else {
        js_sys::Object::new() 
    };

    for (key, def) in states_def {
        let _state_id = if parent_id.is_empty() || parent_id == "(machine)" {
            key.clone() // Top level? Or machine.id.key? 
            // XState 4: machine.id + "." + key? 
            // Actually id logic is complex. Let's assume parent_id . key 
            // Unless the state definition has an ID (which core doesn't store yet).
            // For now: parent_id.key
            // Wait, if parent_id is the machine ID, it is typically "machineId.stateKey".
        } else {
            format!("{}.{}", parent_id, key)
        };
        // Fix for root machine ID:
        let _state_id = if parent_id == "(machine)" {
             // If machine has no ID, XState might treat it differently. 
             // But let's stick to simple path for now.
             key.clone()
        } else {
             format!("{}.{}", parent_id, key)
        };

        let state_config = js_sys::Reflect::get(&config_map, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED);

        // Calculate ID: Check explicit ID in config, else parent.key
        let explicit_id = if state_config.is_object() {
             let id_val = js_sys::Reflect::get(&state_config, &JsValue::from_str("id")).unwrap_or(JsValue::UNDEFINED);
             id_val.as_string()
        } else {
             None
        };

        let state_id = if let Some(eid) = explicit_id {
            eid
        } else if parent_id.is_empty() || parent_id == "(machine)" {
            key.clone() 
        } else {
            format!("{}.{}", parent_id, key)
        };

        let state_node = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&state_node, &JsValue::from_str("key"), &JsValue::from_str(key));
        let _ = js_sys::Reflect::set(&state_node, &JsValue::from_str("id"), &JsValue::from_str(&state_id));
        let _ = js_sys::Reflect::set(&state_node, &JsValue::from_str("config"), &state_config);
        // initial
        if let Some(init) = &def.initial {
             let _ = js_sys::Reflect::set(&state_node, &JsValue::from_str("initial"), &JsValue::from_str(init));
        }
        // type
        let type_str = if !def.states.is_empty() {
             "compound" // or parallel? Core doesn't verify parallel yet.
        } else {
             "atomic" // or history/final
        };
        let _ = js_sys::Reflect::set(&state_node, &JsValue::from_str("type"), &JsValue::from_str(type_str));

        // states
        let child_states_config = if state_config.is_object() {
             js_sys::Reflect::get(&state_config, &JsValue::from_str("states")).unwrap_or(JsValue::UNDEFINED)
        } else {
             JsValue::UNDEFINED
        };
        let child_states = map_state_map(&def.states, &child_states_config, &state_id);
        let _ = js_sys::Reflect::set(&state_node, &JsValue::from_str("states"), &child_states);

        // on (events) -> Array of keys? XState V5 StateNode.on is just the transition config object (deprecated?) 
        // Actually Machine.on is events. StateNode.on is transition definitions?
        // Wait, the test checked `transitions.keys()`. 
        // StateNode.transitions is a Map<string, TransitionDefinition[]>.
        let transitions_map = js_sys::Map::new();
        for (event, transitions) in &def.on {
             // We need to map Vec<TransitionDefinition> to JS objects
             let t_arr = js_sys::Array::new();
             for t in transitions {
                  let t_obj = js_sys::Object::new();
                  if let Some(target) = &t.target {
                       let t_arr_js = js_sys::Array::new();
                       for tx in target {
                           t_arr_js.push(&JsValue::from_str(tx));
                       }
                       let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("target"), &t_arr_js);
                  }
                  // actions, guards... (simplified for now)
                  t_arr.push(&t_obj);
             }
             transitions_map.set(&JsValue::from_str(event), &t_arr);
        }
        let _ = js_sys::Reflect::set(&state_node, &JsValue::from_str("transitions"), &transitions_map);

        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(key), &state_node);
    }
    obj.into()
}

#[wasm_bindgen]
pub struct SetupObject {
    config: JsValue,
}

#[wasm_bindgen]
impl SetupObject {
    #[wasm_bindgen(js_name = createMachine)]
    pub fn create_wasm_machine(&self, val: JsValue) -> Result<StateNode, JsValue> {
         // Create the inner machine using Actor constructor logic
         let runner = Actor::new(val.clone(), JsValue::UNDEFINED)?;
         // Clone the runtime machine to modify it (inject schemas)
         let mut runtime_machine = runner.machine.clone();
         let schemas_key = JsValue::from_str("schemas");
         let schemas_val = js_sys::Reflect::get(&self.config, &schemas_key).unwrap_or(JsValue::UNDEFINED);
         if !schemas_val.is_undefined() {
             let schemas = js_to_value(&schemas_val);
             if schemas != Value::Null {
                 // Overwrite types with schemas (or merge if needed, but usually one source)
                 runtime_machine.definition.types = Some(schemas);
             }
         }
         let machine = runtime_machine;

         let mut guards = runner.guards.clone();
         let mut actions = runner.actions.clone();
         // Parse config
         parse_implementations(&self.config, &mut guards, &mut actions);

         Ok(StateNode {
             machine,
             path: "".to_string(),
             guards,
             actions,
             config: val,
         })
    }
}

#[wasm_bindgen(js_name = setup)]
pub fn setup_wasm_machine(config: JsValue) -> SetupObject {
    SetupObject { config }
}

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
        portable_xstate_rust::matches_event_descriptor_with_warn(&event_type, descriptor, |msg| {
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
impl StateNode {
    #[wasm_bindgen(js_name = createActor)]
    pub fn create_actor_method(&self, options: JsValue) -> Result<Actor, JsValue> {
         let tick_override = Rc::new(core::cell::Cell::new(None));
         let mut runner = Actor { 
            machine: self.machine.clone(), 
            inspector: None,
            guards: self.guards.clone(),
            actions: self.actions.clone(),
            is_stopped: false,
            config: self.config.clone(),
            is_initialized: false,
            shared_state: Rc::new(RefCell::new(None)),
            tick_override: tick_override.clone(),
        };
        runner.sync_tracked_entry_exit(&self.config);

        let override_cell = tick_override;
        runner.machine.register_clock(Rc::new(move || {
            override_cell.get().unwrap_or_else(|| js_sys::Date::now() as u64)
        }));
        runner.machine.register_warn_fn(Rc::new(|msg| {
            web_sys::console::warn_1(&JsValue::from_str(msg));
        }));

        // Apply options
        if options.is_object() {
             let inspect_key = JsValue::from_str("inspect");
             let inspect_val = js_sys::Reflect::get(&options, &inspect_key).unwrap_or(JsValue::UNDEFINED);
             if inspect_val.is_function() {
                  let func: js_sys::Function = inspect_val.dyn_into().unwrap();
                  runner.set_inspector(func);
             }
             // Restore snapshot
             parse_implementations(&options, &mut runner.guards, &mut runner.actions); 
             let mut snapshot_val = JsValue::UNDEFINED;
             let snapshot_key = JsValue::from_str("snapshot");
             if let Ok(val) = js_sys::Reflect::get(&options, &snapshot_key) {
                 if !val.is_undefined() { snapshot_val = val; }
             }
             if snapshot_val.is_undefined() {
                 let state_key = JsValue::from_str("state");
                 if let Ok(val) = js_sys::Reflect::get(&options, &state_key) {
                     snapshot_val = val; 
                 }
             }

             if snapshot_val.is_object() && !snapshot_val.is_null() {
                 let snap_obj = js_sys::Object::from(snapshot_val.clone());
                 web_sys::console::log_2(&JsValue::from_str("StateNode restoring snapshot:"), &snap_obj);
                 // Value
                 let value_key = JsValue::from_str("value");
                 if let Ok(v) = js_sys::Reflect::get(&snap_obj, &value_key) {
                     if !v.is_undefined() {
                         web_sys::console::log_2(&JsValue::from_str("StateNode restoring value:"), &v);
                         runner.machine.current_state = portable_xstate_rust::value_to_state_value(js_to_value(&v));
                         runner.is_initialized = true;
                         web_sys::console::log_1(&JsValue::from_str("StateNode initialized from snapshot"));
                     }
                 }

                 // Context
                 let context_key = JsValue::from_str("context");
                 if let Ok(c) = js_sys::Reflect::get(&snap_obj, &context_key) {
                     if !c.is_undefined() {
                         runner.machine.context = js_to_value(&c);
                     }
                 }

                 // History Value
                 let history_key = JsValue::from_str("historyValue");
                 let mut history_found = false;
                 web_sys::console::log_2(&JsValue::from_str("Looking for history key 2:"), &history_key);
                 if let Ok(h) = js_sys::Reflect::get(&snap_obj, &history_key) {
                     web_sys::console::log_2(&JsValue::from_str("Found history val 2:"), &h);
                     if !h.is_undefined() && !h.is_null() && h.is_object() {
                        let hist_map = js_sys::Object::from(h);
                        web_sys::console::log_1(&JsValue::from_str("Getting keys..."));
                        let keys = js_sys::Object::keys(&hist_map);
                        web_sys::console::log_2(&JsValue::from_str("StateNode history keys length:"), &JsValue::from(keys.length()));
                        for i in 0..keys.length() {
                             let k = keys.get(i).as_string().unwrap_or_default();
                             web_sys::console::log_2(&JsValue::from_str("Process Key:"), &JsValue::from_str(&k));
                             // Resolve ID (k) to Parent Path for Core
                             let parent_path = if let Some(history_path) = runner.machine.definition.id_map.get(&k) {
                                 if let Some(idx) = history_path.rfind('.') {
                                     history_path[0..idx].to_string()
                                 } else {
                                     "".to_string()
                                 }
                             } else {
                                 // Fallback: assume k is path ? Or log warning? Usually ID should resolve.
                                 // If snapshot is from Core, maybe it uses paths?
                                 // But WASM get_snapshot uses IDs.
                                 // Maybe log warning for debugging.
                                 web_sys::console::warn_1(&JsValue::from_str(&format!("Could not resolve History ID to Path: {}", k)));
                                 k.clone()
                             };

                             if let Ok(v) = js_sys::Reflect::get(&hist_map, &JsValue::from_str(&k)) {
                                 if js_sys::Array::is_array(&v) {
                                      let arr: &js_sys::Array = v.unchecked_ref();
                                      let mut longest_path: Option<String> = None;
                                      for j in 0..arr.length() {
                                          let item = arr.get(j);
                                          // lightweight ID extraction
                                          let id_key = JsValue::from_str("id");
                                          if let Ok(id_val) = js_sys::Reflect::get(&item, &id_key) {
                                               if let Some(id_str) = id_val.as_string() {
                                                    if let Some(path) = runner.machine.definition.id_map.get(&id_str) {
                                                        web_sys::console::log_1(&JsValue::from_str(&format!("Resolved path for id {}: {}", id_str, path)));
                                                        match &longest_path {
                                                            None => longest_path = Some(path.clone()),
                                                            Some(curr) => { if path.len() > curr.len() { longest_path = Some(path.clone()); } }
                                                        }
                                                    } else {
                                                        web_sys::console::warn_1(&JsValue::from_str(&format!("Could not resolve StateNode for id: {}", id_str)));
                                                    }
                                               }
                                          }
                                      }
                                      if let Some(p) = longest_path {
                                          runner.machine.history_value.insert(parent_path, portable_xstate_rust::StateValue::Atomic(p));
                                      }
                                 } else if v.is_string() {
                                      runner.machine.history_value.insert(parent_path, portable_xstate_rust::StateValue::Atomic(v.as_string().unwrap()));
                                 } else {
                                      // Complex compound object
                                      runner.machine.history_value.insert(parent_path, portable_xstate_rust::value_to_state_value(js_to_value(&v)));
                                 }
                             }
                        }
                        history_found = true;
                     }
                 }
                 if !history_found {
                     let h_v5_key = JsValue::from_str("history");
                     if let Ok(h) = js_sys::Reflect::get(&snap_obj, &h_v5_key) {
                          if !h.is_undefined() && !h.is_null() {
                              let h_val = js_to_value(&h);
                              if let Some(map) = h_val.into_object() {
                                  for (k, v) in map {
                                      runner.machine.history_value.insert(k, portable_xstate_rust::value_to_state_value(v));
                                  }
                              }
                          }
                     }
                 }
             }

             // Overrides
             parse_implementations(&options, &mut runner.guards, &mut runner.actions);
             web_sys::console::log_1(&JsValue::from_str(&format!("Restored History Value: {:?}", runner.machine.history_value)));

             runner.apply_clock_option(&options);
        }

        runner.push_shared();
        Ok(runner)
    }
}

#[wasm_bindgen]
#[derive(Clone, Default)]
pub struct SimulatedClock {
    clock: portable_xstate_rust::SimulatedClock,
    actors: Rc<RefCell<Vec<Actor>>>,
}

#[wasm_bindgen]
impl SimulatedClock {
    #[wasm_bindgen(constructor)]
    pub fn new() -> SimulatedClock {
        SimulatedClock {
            clock: portable_xstate_rust::SimulatedClock::new(),
            actors: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[wasm_bindgen(js_name = __registerActor)]
    pub fn register_actor(&mut self, actor: &Actor) {
        let mut actor_clone = actor.clone();
        actor_clone.pull_shared();
        let tick_override = actor_clone.tick_override.clone();
        let core_clock = self.clock.clone();
        actor_clone.machine.register_clock(Rc::new(move || {
            tick_override.get().unwrap_or_else(|| core_clock.now())
        }));
        actor_clone.push_shared();
        self.actors.borrow_mut().push(actor_clone);
    }

    #[wasm_bindgen]
    pub fn now(&self) -> f64 {
        self.clock.now() as f64
    }

    #[wasm_bindgen]
    pub fn set(&mut self, timestamp: f64) {
        let target_ms = if timestamp.is_finite() && timestamp >= 0.0 {
            timestamp as u64
        } else {
            0
        };

        let mut last_deadline: Option<u64> = None;
        let mut same_deadline_steps = 0usize;
        let mut total_steps = 0usize;
        let mut transitioned_indices = Vec::new();

        loop {
            let mut actors = self.actors.borrow().clone();
            for a in &mut actors {
                a.pull_shared();
            }

            let next_deadline_opt = actors
                .iter()
                .filter(|a| !a.is_stopped)
                .filter_map(|a| a.machine.next_timer_deadline())
                .min();

            let Some(next_deadline) = next_deadline_opt else {
                break;
            };
            if next_deadline > target_ms {
                break;
            }
            if last_deadline == Some(next_deadline) {
                same_deadline_steps += 1;
                if same_deadline_steps >= 100 {
                    break;
                }
            } else {
                last_deadline = Some(next_deadline);
                same_deadline_steps = 0;
            }
            total_steps += 1;
            if total_steps > 1_000_000 {
                break;
            }

            self.clock.set_now_raw(next_deadline);

            for (idx, a) in actors.iter_mut().enumerate() {
                if !a.is_stopped {
                    a.set_in_tick(true);
                    let mut did_transition = a.machine.tick(&a.guards, &a.actions).is_some();
                    if a.drain_reentrant_shared() {
                        did_transition = true;
                    }
                    a.set_in_tick(false);
                    a.push_shared();
                    if did_transition && !transitioned_indices.contains(&idx) {
                        transitioned_indices.push(idx);
                    }
                }
            }
            *self.actors.borrow_mut() = actors;
        }

        self.clock.set_now_raw(target_ms);

        let actors_snapshot = self.actors.borrow().clone();
        for idx in transitioned_indices {
            if let Some(a) = actors_snapshot.get(idx) {
                a.emit_snapshot();
            }
        }
    }

    #[wasm_bindgen]
    pub fn increment(&mut self, ms: f64) {
        let dt_ms = if ms.is_finite() && ms >= 0.0 {
            ms as u64
        } else {
            0
        };
        let target = self.clock.now().saturating_add(dt_ms);
        self.set(target as f64);
    }
}

#[derive(Clone)]
pub(crate) struct SharedActorState {
    pub machine: RuntimeMachine,
    pub inspector: Option<js_sys::Function>,
    pub guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    pub actions: IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    pub is_stopped: bool,
    pub is_initialized: bool,
    pub is_in_tick: bool,
    pub reentrant_queue: std::collections::VecDeque<(String, Value)>,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct Actor {
    pub(crate) machine: RuntimeMachine,
    pub(crate) inspector: Option<js_sys::Function>,
    pub(crate) guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    pub(crate) actions: IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    pub(crate) config: JsValue,
    pub is_stopped: bool,
    pub(crate) is_initialized: bool,
    pub(crate) shared_state: Rc<RefCell<Option<SharedActorState>>>,
    pub(crate) tick_override: Rc<core::cell::Cell<Option<u64>>>,
}

// Helper to parse "guards" and "actions" from a JS object (either implementations object or options object)
fn parse_implementations(
    impl_obj: &JsValue,
    guards_out: &mut IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
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
                 let _id = key_str.clone();
                 guards_out.insert(key_str.clone(), Rc::new(move |ctx, evt, params| {
                     let ctx_js = value_to_js(ctx);
                     let evt_js = value_to_js(evt);
                     let params_js = if let Some(p) = params {
                         value_to_js(p)
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
                     let ctx_js = value_to_js(ctx_mut);
                     let evt_js = value_to_js(evt);
                     let arg_obj = js_sys::Object::new();
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);

                     if let Ok(res) = assign_fn.call1(&JsValue::NULL, &arg_obj) {
                         if res.is_object() {  
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
                 }));
                 continue;
             }

             // Case 2: Static Assignment (assignment is an object)
             if assign_val.is_object() {
                 let partial_value = js_to_value(&assign_val);

                     if let Some(partial_obj) = partial_value.as_object() {
                         let partial_obj = partial_obj.clone();
                         actions_out.insert(key_str, Rc::new(move |ctx_mut: &mut Value, _evt: &Value, _params: &Option<Value>| {
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
                 actions_out.insert(key_str, Rc::new(move |ctx_mut: &mut Value, evt: &Value, params: &Option<Value>| {
                     let ctx_js = value_to_js(ctx_mut);
                     let evt_js = value_to_js(evt);
                     let params_js = if let Some(p) = params {
                         value_to_js(p)
                     } else {
                         JsValue::UNDEFINED
                     };

                     let arg_obj = js_sys::Object::new();
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("params"), &params_js);

                      // Call action with ({ context, event, params })
                      if let Err(e) = action_fn.call1(&JsValue::NULL, &arg_obj) {
                          web_sys::console::error_2(&JsValue::from_str("Error executing generic action:"), &e);
                     }
                 }));
             }
         }
    }
}

// Helper function to extract inline assignments
fn extract_inline_assignments(val: &JsValue) -> Result<(JsValue, IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>), JsValue> {
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
             if let Ok(res) = func.call0(&JsValue::NULL) {
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

fn extract_recursive(obj: &js_sys::Object, actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>, counter: &mut u32) {
    // Process transitions in 'on'
    let on_key = JsValue::from_str("on");
    let on_val = js_sys::Reflect::get(obj, &on_key).unwrap_or(JsValue::UNDEFINED);
    if on_val.is_object() {
         let on_obj = js_sys::Object::from(on_val);
         let event_keys = js_sys::Object::keys(&on_obj);
         for j in 0..event_keys.length() {
             let evt_key = event_keys.get(j);
             let transitions_val = js_sys::Reflect::get(&on_obj, &evt_key).unwrap_or(JsValue::UNDEFINED);
             if js_sys::Array::is_array(&transitions_val) {
                 let arr = js_sys::Array::from(&transitions_val);
                 for k in 0..arr.length() {
                     let t_val = arr.get(k);
                     if t_val.is_object() {
                         process_action_list(&js_sys::Object::from(t_val), "actions", actions_out, counter);
                     }
                 }
             } else if transitions_val.is_object() {
                 process_action_list(&js_sys::Object::from(transitions_val), "actions", actions_out, counter);
             }
         }
    }

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

fn process_action_list(
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

fn convert_inline_action(
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
     if type_str == "xstate.assign" {
         let assign_key = JsValue::from_str("assignment");
         let assign_val = js_sys::Reflect::get(action_item, &assign_key).unwrap_or(JsValue::UNDEFINED);
         if !assign_val.is_undefined() {
             *counter += 1;
             let new_action_name = format!("generated_assign_{}", counter);
             let assign_val_clone = assign_val.clone();
             actions_out.insert(new_action_name.clone(), Rc::new(move |ctx_mut, evt, _params| {
                  let ctx_js = value_to_js(ctx_mut);
                  let evt_js = value_to_js(evt);
                  let arg_obj = js_sys::Object::new();
                  let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                  let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
                  if let Ok(assign_fn) = assign_val_clone.clone().dyn_into::<js_sys::Function>() {
                      if let Ok(res) = assign_fn.call1(&JsValue::NULL, &arg_obj) {
                          if res.is_object() {  
                               let partial_core = js_to_value(&res);
                               if let Some(partial_obj) = partial_core.as_object() {
                                   if let Some(ctx_obj) = ctx_mut.as_object_mut() { 
                                       for (k, v) in partial_obj { ctx_obj.insert(k.clone(), v.clone()); }
                                   }
                               }
                          }
                      }
                  } else if assign_val_clone.is_object() { 
                      let assign_obj = js_sys::Object::from(assign_val_clone.clone());
                      let keys = js_sys::Object::keys(&assign_obj);
                      if let Some(ctx_obj) = ctx_mut.as_object_mut() { 
                          for i in 0..keys.length() {
                              let key_js = keys.get(i);
                              let val_or_fn = js_sys::Reflect::get(&assign_obj, &key_js).unwrap_or(JsValue::UNDEFINED);
                              let final_val_js = if let Ok(func) = val_or_fn.clone().dyn_into::<js_sys::Function>() {
                                  match func.call1(&JsValue::NULL, &arg_obj) {
                                      Ok(v) => v,
                                      Err(e) => { web_sys::console::log_2(&JsValue::from_str("Error executing assign function:"), &e); JsValue::UNDEFINED }
                                  }
                              } else { val_or_fn };
                              let key_str = key_js.as_string().unwrap_or_default();
                              let final_val = js_to_value(&final_val_js);
                              ctx_obj.insert(key_str, final_val);
                          }
                      }
                  }
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
                  let evt_js = value_to_js(evt);
                  let arg_obj = js_sys::Object::new();
                  let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                  let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
                  if let Some(p) = params {
                       let p_js = value_to_js(p);
                       let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("params"), &p_js);
                  }
                  match func_clone.call1(&JsValue::NULL, &arg_obj) {
                      Ok(res) => {
                           if res.is_object() {  
                                let type_key = JsValue::from_str("type");
                                let type_val = js_sys::Reflect::get(&res, &type_key).unwrap_or(JsValue::UNDEFINED);
                                if type_val == JsValue::from_str("xstate.assign") {  
                                     let assign_key = JsValue::from_str("assignment");
                                     let assignment = js_sys::Reflect::get(&res, &assign_key).unwrap_or(JsValue::UNDEFINED);
                                     if assignment.is_object() {
                                          let assign_obj = js_sys::Object::from(assignment);
                                          evaluate_assignment(&assign_obj, ctx_mut, evt);
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

#[wasm_bindgen]
impl Actor {
    #[wasm_bindgen(constructor)]
    pub fn new(val: JsValue, options: JsValue) -> Result<Actor, JsValue> {
        let mut guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>> = IndexMap::new();
        let mut actions = IndexMap::new();

        parse_implementations(&options, &mut guards, &mut actions);

        let machine_data: Option<InternalState> = if val.is_object() {
             let ptr_key = JsValue::from_str("__wbg_ptr");
             let ptr_check = js_sys::Reflect::get(&val, &ptr_key);
             if let Ok(ptr_val) = ptr_check {
                 if !ptr_val.is_undefined() {
                     let ptr = ptr_val.as_f64().unwrap_or(0.0) as usize as *mut StateNode; 
                     if !ptr.is_null() {
                         unsafe {
                             let wasm_machine_ref = &*ptr;
                             let state = InternalState {
                                 machine: wasm_machine_ref.machine.clone(),
                                 guards: wasm_machine_ref.guards.clone(),
                                 actions: wasm_machine_ref.actions.clone(),
                             };
                             Some(state)
                         }
                     } else { None }
                 } else { None }
             } else { None }
        } else { None };

        let mut machine = if let Some(mut internal_state) = machine_data {
             internal_state.guards.extend(guards);
             guards = internal_state.guards;
             internal_state.actions.extend(actions);
             actions = internal_state.actions;
             internal_state.machine
        } else if val.is_string() {
             let json_definition = val.as_string().unwrap();
             let def = portable_xstate_rust::create_machine(&json_definition)
                .map_err(|e| JsValue::from_str(&format!("Failed to parse machine: {}", e)))?;
             RuntimeMachine::from_definition(def).map_err(|e| JsValue::from_str(&e))?
        } else {
             let normalized_val = normalize_undefined(&val, 0);
             let (cleaned_val, extra_actions) = extract_inline_assignments(&normalized_val)?;
             let mut dummy_guards = IndexMap::new();
             let mut dummy_actions = IndexMap::new();
             let mut counter = 0;
             process_config(&cleaned_val, &mut dummy_guards, &mut dummy_actions, 0, &mut counter, "");
             actions.extend(dummy_actions);
             actions.extend(extra_actions);

             let json_string = js_sys::JSON::stringify(&cleaned_val)
                .map_err(|_| JsValue::from_str("Failed to stringify machine definition"))?
                .as_string().unwrap_or_default();
             let machine_def = portable_xstate_rust::create_machine(&json_string)
                .map_err(|e| JsValue::from_str(&format!("Failed to parse definition: {}", e)))?;
             RuntimeMachine::from_definition(machine_def).map_err(|e| JsValue::from_str(&e))?
        };

        let tick_override = Rc::new(core::cell::Cell::new(None));
        let override_cell = tick_override.clone();
        machine.register_clock(Rc::new(move || {
            override_cell.get().unwrap_or_else(|| js_sys::Date::now() as u64)
        }));
        machine.register_warn_fn(Rc::new(|msg| {
            web_sys::console::warn_1(&JsValue::from_str(msg));
        }));

        let mut actor = Actor { 
            machine, 
            inspector: None,
            guards,
            actions,
            is_stopped: false,
            config: val.clone(),
            is_initialized: false,
            shared_state: Rc::new(RefCell::new(None)),
            tick_override,
        };


        if options.is_object() {
            let mut state_val_opt = None;
            let snapshot_key = JsValue::from_str("snapshot");
            if let Ok(val) = js_sys::Reflect::get(&options, &snapshot_key) {
                if !val.is_undefined() { state_val_opt = Some(val); }
            }
            if state_val_opt.is_none() {
                let state_key = JsValue::from_str("state");
                if let Ok(val) = js_sys::Reflect::get(&options, &state_key) {
                   if !val.is_undefined() { state_val_opt = Some(val); }
                }
            }

            if let Some(state_val) = state_val_opt {
                if !state_val.is_undefined() && !state_val.is_null() {
                    let v_key = JsValue::from_str("value");
                    if let Ok(v) = js_sys::Reflect::get(&state_val, &v_key) {
                        if !v.is_undefined() {
                             actor.machine.current_state = portable_xstate_rust::value_to_state_value(js_to_value(&v));
                             actor.is_initialized = true;
                        }
                    }
                    let c_key = JsValue::from_str("context");
                    if let Ok(c) = js_sys::Reflect::get(&state_val, &c_key) {
                        if !c.is_undefined() {
                             actor.machine.context = js_to_value(&c);
                        }
                    }
                    
                     let _h_key = JsValue::from_str("historyValue"); // avoid unused var warning if logic changes
                     let mut history_found = false;
                     let h_key_lookup = JsValue::from_str("historyValue");
                     if let Ok(h) = js_sys::Reflect::get(&state_val, &h_key_lookup) {
                         if !h.is_undefined() && !h.is_null() && h.is_object() {
                             let hist_map = js_sys::Object::from(h);
                             let keys = js_sys::Object::keys(&hist_map);
                             for i in 0..keys.length() {
                                 let k = keys.get(i).as_string().unwrap_or_default();
                                 let parent_path = if let Some(history_path) = actor.machine.definition.id_map.get(&k) {
                                     if let Some(idx) = history_path.rfind('.') {
                                         history_path[0..idx].to_string()
                                     } else {
                                         "".to_string()
                                     }
                                 } else {
                                     k.clone()
                                 };

                                 if let Ok(v) = js_sys::Reflect::get(&hist_map, &JsValue::from_str(&k)) {
                                     if js_sys::Array::is_array(&v) {
                                         let arr: &js_sys::Array = v.unchecked_ref();
                                         let mut longest_path: Option<String> = None;
                                         for j in 0..arr.length() {
                                              let item = arr.get(j);
                                              let id_key = JsValue::from_str("id");
                                              if let Ok(id_val) = js_sys::Reflect::get(&item, &id_key) {
                                                   if let Some(id_str) = id_val.as_string() {
                                                        if let Some(path) = actor.machine.definition.id_map.get(&id_str) {
                                                            match &longest_path {
                                                                None => longest_path = Some(path.clone()),
                                                                Some(curr) => { if path.len() > curr.len() { longest_path = Some(path.clone()); } }
                                                            }
                                                        } else {
                                                            web_sys::console::warn_1(&JsValue::from_str(&format!("Could not resolve StateNode for id: {}", id_str)));
                                                        }
                                                   }
                                              }
                                         }
                                         if let Some(p) = longest_path {
                                             actor.machine.history_value.insert(parent_path, portable_xstate_rust::StateValue::Atomic(p));
                                         }
                                     } else if v.is_string() {
                                         actor.machine.history_value.insert(parent_path, portable_xstate_rust::StateValue::Atomic(v.as_string().unwrap()));
                                     } else {
                                         actor.machine.history_value.insert(parent_path, portable_xstate_rust::value_to_state_value(js_to_value(&v)));
                                     }
                                 }
                             }
                             history_found = true;
                         }
                     }
                      if !history_found {
                        // Try "history" (XState v5)
                        let h_v5_key = JsValue::from_str("history");
                        if let Ok(h) = js_sys::Reflect::get(&state_val, &h_v5_key) {
                             if !h.is_undefined() && !h.is_null() && h.is_object() {
                                 let h_val = js_to_value(&h);
                                 if let Some(map) = h_val.into_object() {
                                     for (k, v) in map {
                                         actor.machine.history_value.insert(k, portable_xstate_rust::value_to_state_value(v));
                                     }
                                 }
                             }
                        }
                    }
                }
            }
        }
        actor.sync_tracked_entry_exit(&actor.config.clone());
        actor.apply_clock_option(&options);
        actor.push_shared();
        Ok(actor)
    }

    pub(crate) fn sync_tracked_entry_exit(&mut self, config: &JsValue) {
        if !config.is_object() || config.is_null() {
            return;
        }
        let mut counter = 0u32;
        let mut stack: Vec<(JsValue, String)> = vec![(config.clone(), String::new())];
        while let Some((node_val, path)) = stack.pop() {
            if !node_val.is_object() || node_val.is_null() {
                continue;
            }
            let node_obj = js_sys::Object::from(node_val);
            let path_label = if path.is_empty() { "__root__" } else { &path };
            let mut entry_actions = extract_unprocessed_fn_actions(
                &node_obj,
                "entry",
                path_label,
                &mut self.actions,
                &mut counter,
            );
            let mut exit_actions = extract_unprocessed_fn_actions(
                &node_obj,
                "exit",
                path_label,
                &mut self.actions,
                &mut counter,
            );
            if path.is_empty() {
                if !entry_actions.is_empty() {
                    entry_actions.append(&mut self.machine.definition.entry);
                    self.machine.definition.entry = entry_actions;
                }
                if !exit_actions.is_empty() {
                    exit_actions.append(&mut self.machine.definition.exit);
                    self.machine.definition.exit = exit_actions;
                }
            } else if !entry_actions.is_empty() || !exit_actions.is_empty() {
                if let Some(def) = get_state_def_mut(&mut self.machine.definition.states, &path) {
                    if !entry_actions.is_empty() {
                        entry_actions.append(&mut def.entry);
                        def.entry = entry_actions;
                    }
                    if !exit_actions.is_empty() {
                        exit_actions.append(&mut def.exit);
                        def.exit = exit_actions;
                    }
                }
            }
            let states_val = js_sys::Reflect::get(&node_obj, &JsValue::from_str("states"))
                .unwrap_or(JsValue::UNDEFINED);
            if states_val.is_object() && !states_val.is_null() {
                let states_obj = js_sys::Object::from(states_val);
                let keys = js_sys::Object::keys(&states_obj);
                for i in 0..keys.length() {
                    let k = keys.get(i);
                    if let Some(k_str) = k.as_string() {
                        let child_val = js_sys::Reflect::get(&states_obj, &k)
                            .unwrap_or(JsValue::UNDEFINED);
                        let child_path = if path.is_empty() {
                            k_str
                        } else {
                            format!("{}.{}", path, k_str)
                        };
                        stack.push((child_val, child_path));
                    }
                }
            }
        }
    }

    pub(crate) fn pull_shared(&mut self) {
        if let Some(shared) = self.shared_state.borrow().as_ref() {
            self.machine = shared.machine.clone();
            self.inspector = shared.inspector.clone();
            self.guards = shared.guards.clone();
            self.actions = shared.actions.clone();
            self.is_stopped = shared.is_stopped;
            self.is_initialized = shared.is_initialized || self.machine.is_started;
        }
    }

    pub(crate) fn push_shared(&self) {
        let mut borrow = self.shared_state.borrow_mut();
        let (is_in_tick, reentrant_queue) = borrow
            .as_ref()
            .map(|s| (s.is_in_tick, s.reentrant_queue.clone()))
            .unwrap_or((false, std::collections::VecDeque::new()));
        *borrow = Some(SharedActorState {
            machine: self.machine.clone(),
            inspector: self.inspector.clone(),
            guards: self.guards.clone(),
            actions: self.actions.clone(),
            is_stopped: self.is_stopped,
            is_initialized: self.is_initialized,
            is_in_tick,
            reentrant_queue,
        });
    }

    pub(crate) fn set_in_tick(&self, in_tick: bool) {
        if let Some(shared) = self.shared_state.borrow_mut().as_mut() {
            shared.is_in_tick = in_tick;
        }
    }

    pub(crate) fn drain_reentrant_shared(&mut self) -> bool {
        let mut transitioned = false;
        loop {
            let next_evt = self
                .shared_state
                .borrow_mut()
                .as_mut()
                .and_then(|s| s.reentrant_queue.pop_front());
            let Some((ev_name, ev_payload)) = next_evt else {
                break;
            };
            if self
                .machine
                .transition(&ev_name, &ev_payload, &self.guards, &self.actions)
                .is_some()
            {
                transitioned = true;
            }
        }
        transitioned
    }

    pub(crate) fn apply_clock_option(&mut self, options: &JsValue) {
        if !options.is_object() || options.is_null() {
            return;
        }
        let clock_key = JsValue::from_str("clock");
        let clock_val = js_sys::Reflect::get(options, &clock_key).unwrap_or(JsValue::UNDEFINED);
        if clock_val.is_undefined() || clock_val.is_null() {
            return;
        }

        let tick_override = self.tick_override.clone();
        if let Ok(func) = clock_val.clone().dyn_into::<js_sys::Function>() {
            self.machine.register_clock(Rc::new(move || {
                if let Some(override_now) = tick_override.get() {
                    return override_now;
                }
                func.call0(&JsValue::NULL)
                    .ok()
                    .and_then(|v| v.as_f64())
                    .map(|v| v.max(0.0) as u64)
                    .unwrap_or(0)
            }));
            return;
        }

        if clock_val.is_object() {
            let attach = js_sys::Reflect::get(&clock_val, &JsValue::from_str("__registerActor"))
                .unwrap_or(JsValue::UNDEFINED);
            if let Ok(attach_fn) = attach.dyn_into::<js_sys::Function>() {
                self.push_shared();
                let actor_js: JsValue = self.clone().into();
                let _ = attach_fn.call1(&clock_val, &actor_js);
                self.pull_shared();
                return;
            }

            let now_prop = js_sys::Reflect::get(&clock_val, &JsValue::from_str("now"))
                .unwrap_or(JsValue::UNDEFINED);
            if let Ok(now_fn) = now_prop.dyn_into::<js_sys::Function>() {
                let clock_this = clock_val.clone();
                self.machine.register_clock(Rc::new(move || {
                    if let Some(override_now) = tick_override.get() {
                        return override_now;
                    }
                    now_fn
                        .call0(&clock_this)
                        .ok()
                        .and_then(|v| v.as_f64())
                        .map(|v| v.max(0.0) as u64)
                        .unwrap_or(0)
                }));
            }
        }
    }

    pub(crate) fn set_inspector(&mut self, callback: js_sys::Function) {
        self.inspector = Some(callback);
    }



    fn get_history_value_obj(&self) -> JsValue {
         let history_obj = js_sys::Object::new();
         let active_paths = self.machine.current_state.get_active_paths();
         for (id, path) in &self.machine.definition.id_map {
             if let Some(def) = self.machine.get_state_definition(path) {
                 if def.type_ == Some("history".to_string()) {
                     let parent_path = if let Some(idx) = path.rfind('.') {
                         &path[0..idx]
                     } else {
                         "" 
                     };
                     let mut target_to_serialize: Option<String> = None;
                     let matching_active_path = active_paths.iter().find(|p| {
                         if parent_path.is_empty() {
                             !p.is_empty()
                         } else {
                             *p == parent_path || p.starts_with(&format!("{}.", parent_path))
                         }
                     });
                     if let Some(active_path) = matching_active_path {
                          let prefix_len = if parent_path.is_empty() { 0 } else { parent_path.len() + 1 };
                          if active_path.len() > prefix_len {
                               let rest = &active_path[prefix_len..];
                               if let Some(end) = rest.find('.') {
                                   let child = &rest[0..end];
                                   target_to_serialize = Some(if parent_path.is_empty() { child.to_string() } else { format!("{}.{}", parent_path, child) });
                               } else {
                                   target_to_serialize = Some(if parent_path.is_empty() { rest.to_string() } else { format!("{}.{}", parent_path, rest) });
                               }
                          }
                     }
                     if target_to_serialize.is_none() {
                         if let Some(stored) = self.machine.history_value.get(parent_path) {
                             let stored_paths = stored.get_active_paths();
                             target_to_serialize = stored_paths
                                 .iter()
                                 .find(|p| {
                                     if parent_path.is_empty() {
                                         !p.is_empty()
                                     } else {
                                         *p == parent_path || p.starts_with(&format!("{}.", parent_path))
                                     }
                                 })
                                 .or_else(|| stored_paths.first())
                                 .cloned();
                         }
                     }
                     if let Some(target) = &target_to_serialize {
                         if let Some(p_def) = self.machine.get_state_definition(parent_path) {
                             if let Some(initial_key) = &p_def.initial {
                                  let full_initial = if parent_path.is_empty() { initial_key.clone() } else { format!("{}.{}", parent_path, initial_key) };
                                  if *target == full_initial { target_to_serialize = None; }
                             }
                         }
                     }
                     if let Some(target_path) = target_to_serialize {
                          let entry_arr = js_sys::Array::new();
                          let node = StateNode {
                              machine: self.machine.clone(),
                              path: target_path.clone(),
                              guards: self.guards.clone(),
                              actions: self.actions.clone(),
                              config: self.config.clone(),
                          };
                          let node_val: JsValue = node.into();
                          entry_arr.push(&node_val);
                          let _ = js_sys::Reflect::set(&history_obj, &JsValue::from_str(id), &entry_arr);
                     }
                 }
             }
         }
         history_obj.into()
    }

    #[wasm_bindgen(js_name = getSnapshot)]
    pub fn get_snapshot(&self) -> Snapshot {
        let shared_borrow = self.shared_state.borrow();
        let active_machine = shared_borrow
            .as_ref()
            .map(|s| &s.machine)
            .unwrap_or(&self.machine);
        let is_stopped = shared_borrow
            .as_ref()
            .map(|s| s.is_stopped)
            .unwrap_or(self.is_stopped);
        let is_initialized = shared_borrow
            .as_ref()
            .map(|s| s.is_initialized)
            .unwrap_or(self.is_initialized);
        let preview;
        let machine_ref = if !active_machine.is_started && !is_initialized {
            let mut m = active_machine.clone();
            m.start(&self.guards, &IndexMap::new());
            preview = m;
            &preview
        } else {
            active_machine
        };

        let active_paths = machine_ref.current_state.get_active_paths();
        let value_val = if active_paths.is_empty() && machine_ref.definition.states.is_empty() {
            js_sys::Object::new().into()
        } else if active_paths.len() == 1 {
            self.path_to_value_object(&active_paths[0])
        } else {
            // Parallel states: build and merge multiple paths
            let root_obj = js_sys::Object::new();
            for path in &active_paths {
                self.merge_path_into_obj(&root_obj, path);
            }
            root_obj.into()
        };
        let ctx_val = match &machine_ref.context {
            Value::Null => js_sys::Object::new().into(),
            other => value_to_js(other),
        };
        let history_obj = self.get_history_value_obj();
        let status = if is_stopped
            || machine_ref.status() == portable_xstate_rust::MachineStatus::Stopped
        {
            "stopped".to_string()
        } else if machine_ref.is_done() {
            "done".to_string()
        } else {
            "active".to_string()
        };

        Snapshot {
            value: value_val,
            context: ctx_val,
            status,
            machine_ref: machine_ref.clone(),
            guards: self.guards.clone(),
            history_value: history_obj,
            config: self.config.clone(),
        }
    }

    fn merge_path_into_obj(&self, obj: &js_sys::Object, path: &str) {
        let parts: Vec<&str> = path.split('.').collect();
        self.recursive_merge(obj, &parts);
    }

    fn recursive_merge(&self, parent: &js_sys::Object, parts: &[&str]) {
        if parts.is_empty() { return; }
        let key = parts[0];
        if parts.len() == 2 {
            let _ = js_sys::Reflect::set(parent, &JsValue::from_str(key), &JsValue::from_str(parts[1]));
            return;
        }
        if parts.len() == 1 {
            let _ = js_sys::Reflect::set(parent, &JsValue::from_str(key), &JsValue::from_str(key));
            return;
        }

        let existing = js_sys::Reflect::get(parent, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED);
        let target_obj = if existing.is_object() && !existing.is_null() && !existing.is_string() {
            js_sys::Object::from(existing)
        } else {
            let new_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(parent, &JsValue::from_str(key), &new_obj);
            new_obj
        };

        self.recursive_merge(&target_obj, &parts[1..]);
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

    pub fn start(&mut self) -> Actor {
        self.pull_shared();
        if self.machine.is_started {
            return self.clone();
        }

        if !self.is_initialized {
            self.machine.start(&self.guards, &self.actions);
            self.is_initialized = true;
        } else {
            self.machine.is_started = true;
            self.machine.revive_timers();
            self.handle_always_transition();
        }
        self.push_shared();

        // Emit initial snapshot
        self.emit_snapshot();

        self.clone()
    }

    fn emit_snapshot(&self) {
        if let Some(inspector) = &self.inspector {
             let snapshot = self.get_snapshot();
             let obj: JsValue = snapshot.into();
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.snapshot"));
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("status"), &JsValue::from_str("active"));
             let _ = inspector.call1(&JsValue::NULL, &obj);
        }
    }



    fn handle_always_transition(&mut self) {
        let mut loop_count = 0;
        while loop_count < 100 {
            let res = self.machine.transition("", &Value::Null, &self.guards, &self.actions);
            if res.is_none() { break; }
            loop_count += 1;
        }
    }

    pub fn send(&mut self, event: JsValue) -> Result<JsValue, JsValue> {
        self.pull_shared();
        
        let (event_name, event_payload) = if event.is_string() {
            let s = event.as_string().unwrap();
            let payload = Value::Object(IndexMap::from([("type".to_string(), Value::String(s.clone()))]));
            (s, payload)
        } else {
             // Assume object
             let payload = js_to_value(&event);
             let name = payload.as_object().and_then(|m| m.get("type")).and_then(|t| t.as_string()).map(|s| s.as_str()).unwrap_or("UNKNOWN").to_string();
             (name, payload)
        };

        if let Some(inspector) = &self.inspector {
             let obj = js_sys::Object::new();
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.event"));
             let evt_val = value_to_js(&event_payload);
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("event"), &evt_val);

             let _ = inspector.call1(&JsValue::NULL, &obj);
        }
        if self.shared_state.borrow().as_ref().map(|s| s.is_in_tick).unwrap_or(false) {
            if let Some(shared) = self.shared_state.borrow_mut().as_mut() {
                shared.reentrant_queue.push_back((event_name, event_payload));
            }
            return Ok(JsValue::UNDEFINED);
        }

        if !self.machine.is_started { return Ok(JsValue::UNDEFINED); }
        let _ = self.machine.transition(&event_name, &event_payload, &self.guards, &self.actions);
        self.push_shared();

        // 2. Inspect Snapshot
        // web_sys::console::error_2(&JsValue::from_str(&format!("AFTER TRANSITION: current_state={} history_value={:?}", self.machine.current_state, self.machine.history_value)), &JsValue::NULL);

        // 2. Inspect Snapshot
        self.emit_snapshot();
        Ok(JsValue::UNDEFINED)
    }

    #[wasm_bindgen(js_name = subscribe)]
    pub fn subscribe(&mut self, callback: js_sys::Function) {
        self.pull_shared();
        self.set_inspector(callback);
        self.push_shared();
    }

    #[wasm_bindgen]
    pub fn stop(&mut self) -> Actor {
        self.pull_shared();
        self.is_stopped = true;
        self.machine.stop();
        self.push_shared();
        self.clone()
    }

    #[wasm_bindgen(js_name = getPersistedSnapshot)]
    pub fn get_persisted_snapshot(&self) -> JsValue {
        self.get_snapshot().into()
    }

    #[wasm_bindgen(js_name = getNextDeadline)]
    pub fn get_next_deadline(&self) -> Option<f64> {
        let shared_borrow = self.shared_state.borrow();
        if shared_borrow.as_ref().map(|s| s.is_stopped).unwrap_or(self.is_stopped) {
            return None;
        }
        let active_machine = shared_borrow
            .as_ref()
            .map(|s| &s.machine)
            .unwrap_or(&self.machine);
        active_machine.next_timer_deadline().map(|d| d as f64)
    }

    #[wasm_bindgen]
    pub fn tick(&mut self) -> Result<(), JsValue> {
        self.pull_shared();
        if !self.machine.is_started || self.is_stopped {
            return Ok(());
        }
        let target_ms = self.machine.clock_millis_fn.as_ref().map(|c| c()).unwrap_or(0);
        let mut transitioned = false;
        let mut last_deadline: Option<u64> = None;
        let mut same_deadline_steps = 0usize;
        let mut total_steps = 0usize;

        while let Some(deadline) = self.machine.next_timer_deadline() {
            if deadline > target_ms {
                break;
            }
            if last_deadline == Some(deadline) {
                same_deadline_steps += 1;
                if same_deadline_steps >= 100 {
                    break;
                }
            } else {
                last_deadline = Some(deadline);
                same_deadline_steps = 0;
            }
            total_steps += 1;
            if total_steps > 1_000_000 {
                break;
            }

            self.tick_override.set(Some(deadline));
            self.set_in_tick(true);
            let mut did_transition = self.machine.tick(&self.guards, &self.actions).is_some();
            if self.drain_reentrant_shared() {
                did_transition = true;
            }
            self.set_in_tick(false);
            self.tick_override.set(None);

            if did_transition {
                transitioned = true;
            }
        }
        self.tick_override.set(None);
        self.push_shared();
        if transitioned {
            self.emit_snapshot();
        }
        Ok(())
    }
}

fn is_valid_state_value_js(val: &JsValue) -> bool {
    if val.is_string() {
        return true;
    }
    if val.is_object() && !val.is_null() && !js_sys::Array::is_array(val) && !val.is_function() {
        let obj = js_sys::Object::from(val.clone());
        let keys = js_sys::Object::keys(&obj);
        for i in 0..keys.length() {
            let k = keys.get(i);
            let child = js_sys::Reflect::get(&obj, &k).unwrap_or(JsValue::UNDEFINED);
            if !is_valid_state_value_js(&child) {
                return false;
            }
        }
        return true;
    }
    false
}

#[wasm_bindgen]
pub struct Snapshot {
    value: JsValue,
    context: JsValue,
    status: String,
    machine_ref: RuntimeMachine, // Cloned for now
    pub(crate) guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    history_value: JsValue,
    #[allow(dead_code)]
    pub(crate) config: JsValue,
}

#[wasm_bindgen]
impl Snapshot {
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> JsValue {
        self.value.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn context(&self) -> JsValue {
        self.context.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn status(&self) -> String {
        self.status.clone()
    }

    #[wasm_bindgen(getter, js_name = historyValue)]
    pub fn history_value(&self) -> JsValue {
        self.history_value.clone()
    }

    #[wasm_bindgen]
    pub fn matches(&self, test_value: JsValue) -> bool {
        if let Some(path) = test_value.as_string() {
            self.machine_ref.current_state.matches(&path)
        } else if is_valid_state_value_js(&test_value) {
            let val = js_to_value(&test_value);
            let partial = portable_xstate_rust::value_to_state_value(val);
            self.machine_ref.current_state.matches_state_value(&partial)
        } else {
            false
        }
    }

    #[wasm_bindgen]
    pub fn can(&self, event: JsValue) -> bool {
        let (event_name, event_payload) = if event.is_string() {
            let s = event.as_string().unwrap();
            let payload = Value::Object(IndexMap::from([(
                "type".to_string(),
                Value::String(s.clone()),
            )]));
            (s, payload)
        } else {
            let payload = js_to_value(&event);
            let name = payload
                .as_object()
                .and_then(|m| m.get("type"))
                .and_then(|t| t.as_string())
                .unwrap_or(&"UNKNOWN".to_string())
                .clone();
            (name, payload)
        };
        self.machine_ref
            .can_with_guards(&event_name, &event_payload, &self.guards)
    }
    #[wasm_bindgen(js_name = hasTag)]
    pub fn has_tag(&self, tag: &str) -> bool {
        self.machine_ref.has_tag(tag)
    }
    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json(&self) -> JsValue {
         let obj = js_sys::Object::new();
         let _ = js_sys::Reflect::set(&obj, &"value".into(), &self.value);
         let _ = js_sys::Reflect::set(&obj, &"context".into(), &self.context);
         let _ = js_sys::Reflect::set(&obj, &"status".into(), &self.status.clone().into());
         // Add tags as empty array if not present (XState compat)
         let tags = js_sys::Array::new();
         // TODO: Populate tags properly? 
         // For verification test, XState tags are [], and we don't have them in snapshot struct yet?
         // verification.rs log shows XState has tags.
         // Let's add them if we can, or empty array.
         let _ = js_sys::Reflect::set(&obj, &"tags".into(), &tags);
         // Children, etc? verification.rs filters them out or compares mismatch?
         // Mismatch log: XState: {"children": Object {}, "context": ..., "historyValue": Object {}, "status": "active", "tags": [], "value": "a"}
         // Verification filters 'machine' but not 'children'?
         // Oh wait, verification comparison loop filters "type" from Wasm and "machine" from XState.
         // It compares the rest.
         // So we should try to match structure.
         let _ = js_sys::Reflect::set(&obj, &"children".into(), &js_sys::Object::new());
         // Transform history_value to match XState structure: { key: [StateNode, ...] }
         // Since we don't have StateNode instances, we use objects with matching IDs.
         // Use stored history value (already transformed in get_snapshot)
         let _ = js_sys::Reflect::set(&obj, &"historyValue".into(), &self.history_value);
         obj.into()
    }
}


#[wasm_bindgen(js_name = createActor)]
pub fn create_actor(machine_def: JsValue, options: Option<JsValue>) -> Result<JsValue, JsValue> {
    let options = options.unwrap_or(JsValue::UNDEFINED);

    // 1. Check for createActor factory method (StateNode or deferred actor logic)
    if machine_def.is_object() {
         let spawn_fn = js_sys::Reflect::get(&machine_def, &JsValue::from_str("createActor")).unwrap_or(JsValue::UNDEFINED);
         if let Ok(func) = spawn_fn.dyn_into::<js_sys::Function>() {
              // Call createActor(options)
              let runner_val = func.call1(&machine_def, &options)?;
              return Ok(runner_val);
         }
    }

    // 2. Treat as Machine Config Object (POJO)
    // Process config to extract inline guards and remove undefineds
    let config = machine_def.clone();
    let mut inline_guards = IndexMap::new();
    let mut inline_actions = IndexMap::new();
    let mut counter = 0;
    process_config(&config, &mut inline_guards, &mut inline_actions, 0, &mut counter, "");
    let mut runner = Actor::new(config, options.clone())?;
    runner.guards.extend(inline_guards);
    runner.actions.extend(inline_actions);

    // Apply common options (inspect, implementations, snapshot)
    if options.is_object() {
        let inspect_key = JsValue::from_str("inspect");
        let inspect_val = js_sys::Reflect::get(&options, &inspect_key).unwrap_or(JsValue::UNDEFINED);
        if inspect_val.is_function() {
             let func: js_sys::Function = inspect_val.dyn_into().unwrap();
             runner.set_inspector(func);
        }

        parse_implementations(&options, &mut runner.guards, &mut runner.actions);
        // Snapshot restoration
        let snap_key = JsValue::from_str("snapshot");
        let snap_val = js_sys::Reflect::get(&options, &snap_key).unwrap_or(JsValue::UNDEFINED);
        if snap_val.is_object() {
             // Try to read value and context
             let val_key = JsValue::from_str("value");
             let ctx_key = JsValue::from_str("context");
             let val = js_sys::Reflect::get(&snap_val, &val_key).unwrap_or(JsValue::UNDEFINED);
             let ctx = js_sys::Reflect::get(&snap_val, &ctx_key).unwrap_or(JsValue::UNDEFINED);
             let status_key = JsValue::from_str("status");
             let status = js_sys::Reflect::get(&snap_val, &status_key).unwrap_or(JsValue::UNDEFINED);
             if let Some(s) = status.as_string() {
                 if s == "done" || s == "stopped" {
                      runner.is_stopped = true;
                 }
             }

             if !val.is_undefined() {
                  runner.machine.current_state = portable_xstate_rust::value_to_state_value(js_to_value(&val));
                  runner.is_initialized = true;
             }
             if !ctx.is_undefined() {
                  runner.machine.context = js_to_value(&ctx);
             }

             // History Value restoration for createActor
             let history_key = JsValue::from_str("historyValue");
             let history_val = js_sys::Reflect::get(&snap_val, &history_key).unwrap_or(JsValue::UNDEFINED);
             if history_val.is_object() && !history_val.is_null() {
                  let hist_map = js_sys::Object::from(history_val);
                  let keys = js_sys::Object::keys(&hist_map);
                  for i in 0..keys.length() {
                      let k = keys.get(i).as_string().unwrap_or_default();
                      let v = js_sys::Reflect::get(&hist_map, &JsValue::from_str(&k)).unwrap_or(JsValue::UNDEFINED);
                      if let Some(s) = v.as_string() {
                          runner.machine.history_value.insert(k, portable_xstate_rust::StateValue::Atomic(s));
                      } else if js_sys::Array::is_array(&v) {
                          let arr: &js_sys::Array = v.unchecked_ref();
                          let mut longest_path: Option<String> = None;
                          for i in 0..arr.length() {
                              let item = arr.get(i);
                              let id_key = JsValue::from_str("id");
                              if let Ok(id_val) = js_sys::Reflect::get(&item, &id_key) {
                                  if let Some(id_str) = id_val.as_string() {
                                      if let Some(path) = runner.machine.definition.id_map.get(&id_str) {
                                           match &longest_path {
                                               None => longest_path = Some(path.clone()),
                                               Some(current) => {
                                                   if path.len() > current.len() {
                                                       longest_path = Some(path.clone());
                                                   }
                                               }
                                           }
                                      } else {
                                          web_sys::console::warn_1(&JsValue::from_str(&format!("Could not resolve StateNode for id: {}", id_str)));
                                      }
                                  }
                              }
                          }
                          if let Some(p) = longest_path {
                              // k is the History Node ID.
                              // We need to store it under the Parent Path.
                              // Resolve k -> Path -> Parent Path.
                              if let Some(history_node_path) = runner.machine.definition.id_map.get(&k) {
                                  // Strip last segment to get parent
                                  if let Some(idx) = history_node_path.rfind('.') {
                                      let parent_path = &history_node_path[0..idx];
                                      runner.machine.history_value.insert(parent_path.to_string(), portable_xstate_rust::StateValue::Atomic(p.clone()));
                                  } else {
                                      // History node at root?
                                      runner.machine.history_value.insert("".to_string(), portable_xstate_rust::StateValue::Atomic(p));
                                  }
                              } else {
                                  web_sys::console::warn_1(&JsValue::from_str(&format!("Could not resolve History Node ID during hydration: {}", k)));
                              }
                          }
                      }
                  }
             }
        }
    }

    runner.push_shared();

    Ok(runner.into())
}

#[wasm_bindgen(js_name = createMachine)]
pub fn create_machine(config: JsValue, options: Option<JsValue>) -> Result<StateNode, JsValue> {
    let options_val = options.unwrap_or(JsValue::UNDEFINED);
    let config_processed = config.clone();
    let mut inline_guards = IndexMap::new();
    let mut inline_actions = IndexMap::new();
    let mut counter = 0;
    process_config(&config_processed, &mut inline_guards, &mut inline_actions, 0, &mut counter, "");
    let mut runner = Actor::new(config_processed, options_val)?;
    runner.guards.extend(inline_guards);
    runner.actions.extend(inline_actions);
    Ok(StateNode {
        machine: runner.machine,
        path: "".to_string(),
        guards: runner.guards,
        actions: runner.actions,
        config: config.clone(),
    })
}
fn evaluate_js_guard(func: &js_sys::Function, context: &Value, event: &Value, _guard_params: &Option<Value>) -> bool {
    let ctx_js = value_to_js(context);
    let evt_js = value_to_js(event);
    let arg_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
    if let Ok(res) = func.call1(&JsValue::NULL, &arg_obj) {
        res.as_bool().unwrap_or(false)
    } else {
        false
    }
}

fn execute_js_action(func: &js_sys::Function, ctx_mut: &mut Value, evt: &Value, params: &Option<Value>) -> JsValue {
    let ctx_js = value_to_js(ctx_mut);
    let evt_js = value_to_js(evt);
    let arg_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
    if let Some(p) = params {
        let p_js = value_to_js(p);
        let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("params"), &p_js);
    }
    match func.call1(&JsValue::NULL, &arg_obj) {
        Ok(v) => v,
        Err(e) => {
            web_sys::console::log_2(&JsValue::from_str("Error executing JS action:"), &e);
            JsValue::UNDEFINED
        }
    }
}

fn evaluate_assignment(assignment: &js_sys::Object, ctx_mut: &mut Value, evt: &Value) { 
    let ctx_js = value_to_js(ctx_mut);
    let evt_js = value_to_js(evt);
    let arg_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
    let keys = js_sys::Object::keys(assignment);
    if let Some(ctx_obj_mut) = ctx_mut.as_object_mut() {
        for k in 0..keys.length() {
            let key_js = keys.get(k);
            let key_str = key_js.as_string().unwrap_or_default();
            let val_js = js_sys::Reflect::get(assignment, &key_js).unwrap_or(JsValue::UNDEFINED);
            let final_val = if let Ok(func) = val_js.clone().dyn_into::<js_sys::Function>() {
                if let Ok(res) = func.call1(&JsValue::NULL, &arg_obj) {
                    res
                } else {
                    JsValue::UNDEFINED
                }
            } else {
                val_js
            };
            let val_core = js_to_value(&final_val);
            ctx_obj_mut.insert(key_str, val_core);
        }
    }
}

fn process_config(
    val: &JsValue, 
    guards: &mut IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>, 
    actions: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    depth: usize,
    counter: &mut u32,
    path: &str
) {
    if depth > 100 { return; }

    // Removed inject_tracking_action

    if js_sys::Array::is_array(val) {
        let arr: &js_sys::Array = val.unchecked_ref();
        for i in 0..arr.length() {
            let item = arr.get(i);
            if item.is_function() {
                 *counter += 1;
                 let id = format!("auto_action_{}_{}", if path.is_empty() { "root" } else { path }, counter);
                 let func: js_sys::Function = item.clone().dyn_into().unwrap();
                 let type_key = JsValue::from_str("type");
                 let func_type_val = js_sys::Reflect::get(&item, &type_key).unwrap_or(JsValue::UNDEFINED);
                 if func_type_val == JsValue::from_str("xstate.assign") {
                      let assign_key = JsValue::from_str("assignment");
                      let mut assignment = js_sys::Reflect::get(&item, &assign_key).unwrap_or(JsValue::UNDEFINED);
                      if assignment.is_undefined() {
                           assignment = js_sys::Reflect::get(&func, &assign_key).unwrap_or(JsValue::UNDEFINED);
                      }
                      let closure = Rc::new(move |ctx_mut: &mut Value, evt: &Value, _params: &Option<Value>| {
                           if assignment.is_object() {
                                let assign_obj = js_sys::Object::from(assignment.clone());
                                evaluate_assignment(&assign_obj, ctx_mut, evt);
                           }
                      });
                      actions.insert(id.clone(), closure);
                      let new_obj = js_sys::Object::new();
                      let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
                      let _ = js_sys::Reflect::set(arr, &JsValue::from_f64(i as f64), &new_obj);
                      continue;
                 }
                 let closure = Rc::new(move |ctx_mut: &mut Value, evt: &Value, _params: &Option<Value>| {
                      let res = execute_js_action(&func, ctx_mut, evt, _params); 
                      if res.is_object() { 
                           let type_key = JsValue::from_str("type");
                           let type_val = js_sys::Reflect::get(&res, &type_key).unwrap_or(JsValue::UNDEFINED);
                           if type_val == JsValue::from_str("xstate.assign") { 
                                let assign_key = JsValue::from_str("assignment");
                                let mut assignment = js_sys::Reflect::get(&res, &assign_key).unwrap_or(JsValue::UNDEFINED);
                                 if assignment.is_undefined() {
                                     assignment = js_sys::Reflect::get(&func, &assign_key).unwrap_or(JsValue::UNDEFINED); 
                                }

                                if assignment.is_object() {
                                     let assign_obj = js_sys::Object::from(assignment);
                                     evaluate_assignment(&assign_obj, ctx_mut, evt);
                                }
                           }
                      }
                 });
                 actions.insert(id.clone(), closure);
                 let new_obj = js_sys::Object::new();
                 let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
                 let _ = js_sys::Reflect::set(arr, &JsValue::from_f64(i as f64), &new_obj);
            } else if item.is_object() && !item.is_null() {
                // Check if it's an assign action
                let type_key = JsValue::from_str("type");
                let type_val = js_sys::Reflect::get(&item, &type_key).unwrap_or(JsValue::UNDEFINED);
                if type_val == JsValue::from_str("xstate.assign") {    
                     let assign_key = JsValue::from_str("assignment");
                     let assignment = js_sys::Reflect::get(&item, &assign_key).unwrap_or(JsValue::UNDEFINED);
                     if assignment.is_object() && !assignment.is_function() {
                         let assign_obj = js_sys::Object::from(assignment.clone());
                         let keys = js_sys::Object::keys(&assign_obj);
                         let mut has_funcs = false;
                         for k in 0..keys.length() {
                             let k_key = keys.get(k);
                             let k_val = js_sys::Reflect::get(&assign_obj, &k_key).unwrap_or(JsValue::UNDEFINED);
                             if k_val.is_function() { has_funcs = true; break; }
                         }
                         if has_funcs {
                             *counter += 1;
                             let id = format!("auto_action_{}_{}", if path.is_empty() { "root" } else { path }, counter);
                             let assignment_clone = assign_obj.clone();
                             let closure = Rc::new(move |ctx_mut: &mut Value, evt: &Value, _params: &Option<Value>| {
                                  evaluate_assignment(&assignment_clone, ctx_mut, evt);
                             });
                             actions.insert(id.clone(), closure);
                             let new_action_obj = js_sys::Object::new();
                             let _ = js_sys::Reflect::set(&new_action_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
                             let _ = js_sys::Reflect::set(arr, &JsValue::from_f64(i as f64), &new_action_obj);
                             continue;
                         }
                     }
                }
                process_config(&item, guards, actions, depth + 1, counter, "");
            }
        }
    } else if (val.is_object() || val.is_function()) && !val.is_null() {
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
            }
            if (key_str == "guard" || key_str == "cond") && value.is_function() {
                 let id = format!("auto_guard_{}", guards.len());
                 let func: js_sys::Function = value.dyn_into().unwrap();
                 let closure = Rc::new(move |context: &Value, event: &Value, _guard_params: &Option<Value>| -> bool {
                      evaluate_js_guard(&func, context, event, _guard_params)
                 });
                 guards.insert(id.clone(), closure);
                 let replace_obj = js_sys::Object::new();
                 let _ = js_sys::Reflect::set(&replace_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
                 let _ = js_sys::Reflect::set(&obj, &key, &replace_obj);
            } else if key_str == "states" && value.is_object() && !value.is_null() {
                 let states_obj = js_sys::Object::from(value.clone());
                 let child_keys = js_sys::Object::keys(&states_obj);
                 for j in 0..child_keys.length() {
                     let child_key = child_keys.get(j);
                     let child_key_str = child_key.as_string().unwrap_or_default();
                     let child_val = js_sys::Reflect::get(&states_obj, &child_key).unwrap_or(JsValue::UNDEFINED);
                     if child_val.is_object() && !child_val.is_null() {
                         let child_path = if path.is_empty() { child_key_str.clone() } else { format!("{}.{}", path, child_key_str) };
                         process_config(&child_val, guards, actions, depth + 1, counter, &child_path);
                     }
                 }
            } else if value.is_object() && !value.is_null() {
                // Check if it's a function that should be an auto_action
                if value.is_function() {
                    let type_key = JsValue::from_str("type");
                    let type_val = js_sys::Reflect::get(&value, &type_key).unwrap_or(JsValue::UNDEFINED);
                    
                    if type_val == JsValue::from_str("xstate.assign") {
                         // It's a function-object from assign()
                         *counter += 1;
                         let id = format!("auto_action_{}_{}", if path.is_empty() { "root" } else { path }, counter);
                         let func: js_sys::Function = value.clone().dyn_into().unwrap();
                         let assign_key = JsValue::from_str("assignment");
                         let mut assignment = js_sys::Reflect::get(&value, &assign_key).unwrap_or(JsValue::UNDEFINED);
                         if assignment.is_undefined() {
                              assignment = js_sys::Reflect::get(&func, &assign_key).unwrap_or(JsValue::UNDEFINED);
                         }
                         
                         let closure = Rc::new(move |ctx_mut: &mut Value, evt: &Value, _params: &Option<Value>| {
                             if assignment.is_object() {
                                 let assign_obj = js_sys::Object::from(assignment.clone());
                                 evaluate_assignment(&assign_obj, ctx_mut, evt);
                             }
                         });
                         actions.insert(id.clone(), closure);
                         let new_obj = js_sys::Object::new();
                         let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
                         let _ = js_sys::Reflect::set(&obj, &key, &new_obj);
                         continue;
                    } else if key_str != "guard" && key_str != "cond" {
                         // It's a plain function used as an action (not a guard)
                         *counter += 1;
                         let id = format!("auto_action_{}_{}", if path.is_empty() { "root" } else { path }, counter);
                         let func: js_sys::Function = value.clone().dyn_into().unwrap();
                         let closure = Rc::new(move |ctx_mut: &mut Value, evt: &Value, _params: &Option<Value>| {
                              let res = execute_js_action(&func, ctx_mut, evt, _params);
                              if res.is_object() {
                                   let t_key = JsValue::from_str("type");
                                   let t_val = js_sys::Reflect::get(&res, &t_key).unwrap_or(JsValue::UNDEFINED);
                                   if t_val == JsValue::from_str("xstate.assign") {
                                        let a_key = JsValue::from_str("assignment");
                                        let mut a_val = js_sys::Reflect::get(&res, &a_key).unwrap_or(JsValue::UNDEFINED);
                                        if a_val.is_undefined() {
                                             a_val = js_sys::Reflect::get(&func, &a_key).unwrap_or(JsValue::UNDEFINED);
                                        }
                                        if a_val.is_object() {
                                             let a_obj = js_sys::Object::from(a_val);
                                             evaluate_assignment(&a_obj, ctx_mut, evt);
                                        }
                                   }
                              }
                         });
                         actions.insert(id.clone(), closure);
                         let new_obj = js_sys::Object::new();
                         let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
                         let _ = js_sys::Reflect::set(&obj, &key, &new_obj);
                         continue;
                    }
                }

                // Check if it's an assign action object
                let type_key = JsValue::from_str("type");
                let type_val = js_sys::Reflect::get(&value, &type_key).unwrap_or(JsValue::UNDEFINED);
                let mut processed = false;
                if type_val == JsValue::from_str("xstate.assign") {    
                     let assign_key = JsValue::from_str("assignment");
                     let assignment = js_sys::Reflect::get(&value, &assign_key).unwrap_or(JsValue::UNDEFINED);
                     if assignment.is_object() && !assignment.is_function() {
                         let assign_obj = js_sys::Object::from(assignment.clone());
                         let keys = js_sys::Object::keys(&assign_obj);
                         let mut has_funcs = false;
                         for k in 0..keys.length() {
                             let k_key = keys.get(k);
                             let k_val = js_sys::Reflect::get(&assign_obj, &k_key).unwrap_or(JsValue::UNDEFINED);
                             if k_val.is_function() { has_funcs = true; break; }
                         }
                         if has_funcs {
                             *counter += 1;
                             let id = format!("auto_action_{}_{}", if path.is_empty() { "root" } else { path }, counter);
                             let assignment_clone = assign_obj.clone();
                             let closure = Rc::new(move |ctx_mut: &mut Value, evt: &Value, _params: &Option<Value>| {
                                  evaluate_assignment(&assignment_clone, ctx_mut, evt);
                             });
                             actions.insert(id.clone(), closure);
                             let new_action_obj = js_sys::Object::new();
                             let _ = js_sys::Reflect::set(&new_action_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
                             let _ = js_sys::Reflect::set(&obj, &key, &new_action_obj);
                             processed = true;
                         }
                     }
                }
                if !processed {
                    process_config(&value, guards, actions, depth + 1, counter, "");
                }
            }
        }
    }
}


fn process_invoke_list(
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

fn convert_inline_invoke(
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

fn get_state_def_mut<'a>(
    mut states: &'a mut IndexMap<String, portable_xstate_rust::StateDefinition>,
    path: &str,
) -> Option<&'a mut portable_xstate_rust::StateDefinition> {
    let mut parts = path.split('.').peekable();
    while let Some(part) = parts.next() {
        let def = states.get_mut(part)?;
        if parts.peek().is_none() {
            return Some(def);
        }
        states = &mut def.states;
    }
    None
}

fn extract_unprocessed_fn_actions(
    state_obj: &js_sys::Object,
    prop_name: &str,
    path_label: &str,
    actions_out: &mut IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    counter: &mut u32,
) -> Vec<portable_xstate_rust::Action> {
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
        prepended.push(portable_xstate_rust::Action::Named(id));
    }
    prepended
}

fn normalize_undefined(val: &JsValue, depth: usize) -> JsValue {
    if depth > 100 { return val.clone(); }
    if val.is_object() && !val.is_null() {
        if js_sys::Array::is_array(val) {
            let arr = js_sys::Array::from(val);
            let res_arr = js_sys::Array::new();
            for i in 0..arr.length() {
                let item = arr.get(i);
                res_arr.push(&normalize_undefined(&item, depth + 1));
            }
            return res_arr.into();
        }
        if val.is_function() { return val.clone(); }
        let obj = js_sys::Object::from(val.clone());
        let keys = js_sys::Object::keys(&obj);
        let res_obj = js_sys::Object::new();
        for i in 0..keys.length() {
            let key = keys.get(i);
            let v = js_sys::Reflect::get(&obj, &key).unwrap_or(JsValue::UNDEFINED);
            if v.is_undefined() {
                // Convert undefined to null to preserve keys in JSON/Serde
                let _ = js_sys::Reflect::set(&res_obj, &key, &JsValue::NULL);
            } else {
                let _ = js_sys::Reflect::set(&res_obj, &key, &normalize_undefined(&v, depth + 1));
            }
        }
        return res_obj.into();
    }
    val.clone()
}
