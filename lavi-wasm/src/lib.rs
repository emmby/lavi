#![allow(clippy::type_complexity, clippy::unused_unit)]
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use lavi::{RuntimeMachine, StateType, Value};
use crate::utils::{js_to_value, value_to_js};
use lavi::{IndexMap, IndexMapExt};
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
    pub action_descriptors: IndexMap<String, JsValue>,
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct StateNode {
    pub(crate) machine: RuntimeMachine,
    pub(crate) path: String,
    pub(crate) guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    pub(crate) actions: IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    pub(crate) action_descriptors: IndexMap<String, JsValue>,
    pub(crate) config: JsValue,
    pub(crate) logger_holder: Rc<RefCell<Option<js_sys::Function>>>,
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
    pub fn description(&self) -> JsValue {
        if self.path.is_empty() {
            if let Some(ref d) = self.machine.definition.description {
                return JsValue::from_str(d);
            }
        } else if let Some(def) = self.machine.get_state_definition(&self.path) {
            if let Some(ref d) = def.description {
                return JsValue::from_str(d);
            }
        }
        JsValue::UNDEFINED
    }

    #[wasm_bindgen(getter)]
    pub fn root(&self) -> JsValue {
        let root_obj = js_sys::Object::new();
        let on_obj = js_sys::Object::new();
        for (event, transitions) in &self.machine.definition.on {
            let t_arr = js_sys::Array::new();
            for t in transitions {
                let t_obj = js_sys::Object::new();
                if let Some(ref desc) = t.description {
                    let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("description"), &JsValue::from_str(desc));
                }
                t_arr.push(&t_obj);
            }
            let _ = js_sys::Reflect::set(&on_obj, &JsValue::from_str(event), &t_arr);
        }
        let _ = js_sys::Reflect::set(&root_obj, &JsValue::from_str("on"), &on_obj);
        root_obj.into()
    }

    #[wasm_bindgen(getter)]
    pub fn definition(&self) -> JsValue {
        serialize_root_machine_definition(&self.machine, &self.config).into()
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
            action_descriptors: self.action_descriptors.clone(),
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
        let mut work_stack: Vec<&lavi::StateDefinition> =
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
}

pub(crate) fn compute_history_value_obj(
    machine: &RuntimeMachine,
    guards: &IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    actions: &IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    action_descriptors: &IndexMap<String, JsValue>,
    config: &JsValue,
    logger_holder: &Rc<RefCell<Option<js_sys::Function>>>,
) -> JsValue {
    let history_obj = js_sys::Object::new();
    let active_paths = machine.current_state.get_active_paths();
    for (id, path) in &machine.definition.id_map {
        if let Some(def) = machine.get_state_definition(path) {
            if matches!(def.state_type, lavi::StateType::History { .. }) {
                let is_deep = def.history == Some(lavi::HistoryType::Deep)
                    || matches!(def.state_type, lavi::StateType::History { deep: true });
                let parent_path = if let Some(idx) = path.rfind('.') {
                    &path[0..idx]
                } else {
                    ""
                };
                let prefix_len = if parent_path.is_empty() { 0 } else { parent_path.len() + 1 };

                let mut raw_paths: Vec<String> = active_paths
                    .iter()
                    .filter(|p| {
                        if parent_path.is_empty() {
                            !p.is_empty()
                        } else {
                            p.len() > prefix_len
                                && p.starts_with(parent_path)
                                && p.as_bytes().get(parent_path.len()) == Some(&b'.')
                        }
                    })
                    .cloned()
                    .collect();

                if raw_paths.is_empty() {
                    if let Some(stored) = machine.history_value.get(parent_path) {
                        let stored_paths = stored.get_active_paths();
                        let matching_stored: Vec<String> = stored_paths
                            .iter()
                            .filter(|p| {
                                if parent_path.is_empty() {
                                    !p.is_empty()
                                } else {
                                    p.len() > prefix_len
                                        && p.starts_with(parent_path)
                                        && p.as_bytes().get(parent_path.len()) == Some(&b'.')
                                }
                            })
                            .cloned()
                            .collect();
                        if !matching_stored.is_empty() {
                            raw_paths = matching_stored;
                        } else {
                            raw_paths = stored_paths
                                .into_iter()
                                .filter(|p| !p.is_empty())
                                .collect();
                        }
                    }
                }

                let mut targets_to_serialize: Vec<String> = Vec::new();
                for raw_path in raw_paths {
                    let target_path = if is_deep {
                        raw_path
                    } else if raw_path.len() > prefix_len {
                        let rest = &raw_path[prefix_len..];
                        let child = if let Some(end) = rest.find('.') {
                            &rest[..end]
                        } else {
                            rest
                        };
                        if parent_path.is_empty() {
                            child.to_string()
                        } else {
                            format!("{}.{}", parent_path, child)
                        }
                    } else {
                        raw_path
                    };
                    if !targets_to_serialize.contains(&target_path) {
                        targets_to_serialize.push(target_path);
                    }
                }

                if !machine.history_value.contains_key(parent_path)
                    && def.history_target.is_none()
                {
                    if let Some(p_def) = machine.get_state_definition(parent_path) {
                        if let Some(initial_key) = &p_def.initial {
                            let full_initial = if parent_path.is_empty() {
                                initial_key.clone()
                            } else {
                                format!("{}.{}", parent_path, initial_key)
                            };
                            if targets_to_serialize.len() == 1
                                && targets_to_serialize[0] == full_initial
                            {
                                targets_to_serialize.clear();
                            }
                        }
                    }
                }

                if !targets_to_serialize.is_empty() {
                    let entry_arr = js_sys::Array::new();
                    for target_path in targets_to_serialize {
                        let node = StateNode {
                            machine: machine.clone(),
                            path: target_path,
                            guards: guards.clone(),
                            actions: actions.clone(),
                            action_descriptors: action_descriptors.clone(),
                            config: config.clone(),
                            logger_holder: logger_holder.clone(),
                        };
                        let node_val: JsValue = node.into();
                        entry_arr.push(&node_val);
                    }
                    let _ = js_sys::Reflect::set(&history_obj, &JsValue::from_str(id), &entry_arr);
                }
            }
        }
    }
    history_obj.into()
}

#[wasm_bindgen]
impl StateNode {
    #[wasm_bindgen(js_name = resolveState)]
    pub fn resolve_state(&self, state_val_obj: JsValue) -> JsValue {
        let value_key = JsValue::from_str("value");
        let val = if state_val_obj.is_object() {
            js_sys::Reflect::get(&state_val_obj, &value_key).unwrap_or(state_val_obj.clone())
        } else {
            state_val_obj.clone()
        };
        let context_key = JsValue::from_str("context");
        let ctx = if state_val_obj.is_object() {
            js_sys::Reflect::get(&state_val_obj, &context_key).unwrap_or(JsValue::UNDEFINED)
        } else {
            JsValue::UNDEFINED
        };

        let resolved = if val.is_object() && !val.is_null() && !val.is_string() {
            let val_obj = js_sys::Object::from(val.clone());
            if matches!(self.machine.definition.state_type, StateType::Parallel) {
                let resolved_obj = js_sys::Object::new();
                for (r_name, r_def) in &self.machine.definition.states {
                    if matches!(r_def.state_type, StateType::History { .. }) || r_def.history.is_some() {
                        continue;
                    }
                    let r_key = JsValue::from_str(r_name);
                    let existing = js_sys::Reflect::get(&val_obj, &r_key).unwrap_or(JsValue::UNDEFINED);
                    if !existing.is_undefined() {
                        let _ = js_sys::Reflect::set(&resolved_obj, &r_key, &existing);
                    } else if r_def.states.is_empty() {
                        let _ = js_sys::Reflect::set(&resolved_obj, &r_key, &js_sys::Object::new());
                    } else if let Some(init) = &r_def.initial {
                        let _ = js_sys::Reflect::set(&resolved_obj, &r_key, &JsValue::from_str(init));
                    }
                }
                resolved_obj.into()
            } else {
                val.clone()
            }
        } else {
            let start_key = if let Some(s) = val.as_string() {
                s
            } else {
                self.machine.definition.initial.clone()
            };
            self.resolve_state_from_key(&start_key, &self.machine.definition.states)
        };

        let mut final_machine = self.machine.clone();
        if let Ok(json_str) = js_sys::JSON::stringify(&resolved) {
            if let Some(s) = json_str.as_string() {
                let _ = final_machine.restore_from_snapshot_json(&format!(r#"{{"value": {}}}"#, s));
            }
        }
        if ctx.is_object() && !ctx.is_null() {
            final_machine.context = js_to_value(&ctx);
        }

        let history_obj = compute_history_value_obj(
            &final_machine,
            &self.guards,
            &self.actions,
            &self.action_descriptors,
            &self.config,
            &self.logger_holder,
        );

        let output_val = if final_machine.is_done() {
            Actor::compute_output_js(&self.config, &final_machine, &if ctx.is_object() && !ctx.is_null() { ctx.clone() } else { js_sys::Object::new().into() })
        } else {
            JsValue::UNDEFINED
        };

        Snapshot {
            value: resolved,
            context: if ctx.is_object() && !ctx.is_null() { ctx } else { js_sys::Object::new().into() },
            status: if final_machine.is_done() { "done".to_string() } else { "active".to_string() },
            machine_ref: final_machine,
            guards: self.guards.clone(),
            history_value: history_obj,
            config: self.config.clone(),
            output: output_val,
            error: JsValue::UNDEFINED,
        }.into()
    }

    fn resolve_state_from_key(&self, key: &str, states: &IndexMap<String, lavi::StateDefinition>) -> JsValue {
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
         parse_implementations(&implementations, &mut new_machine.guards, &mut new_machine.actions, &mut new_machine.machine.actions_with_effects);
         parse_and_register_delays(&implementations, &mut new_machine.machine);

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
            action_descriptors: self.action_descriptors.clone(),
            config: self.config.clone(),
            logger_holder: self.logger_holder.clone(),
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
    states_def: &IndexMap<String, lavi::StateDefinition>,
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
        if let Some(desc) = &def.description {
            let _ = js_sys::Reflect::set(&state_node, &JsValue::from_str("description"), &JsValue::from_str(desc));
        }
        // initial
        if let Some(init) = &def.initial {
             let _ = js_sys::Reflect::set(&state_node, &JsValue::from_str("initial"), &JsValue::from_str(init));
        }
        // type
        let type_str = match def.state_type {
            lavi::StateType::Atomic => "atomic",
            lavi::StateType::Compound => "compound",
            lavi::StateType::Parallel => "parallel",
            lavi::StateType::Final => "final",
            lavi::StateType::History { .. } => "history",
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

fn serialize_state_node_definition(
    key: &str,
    parent_id: &str,
    order: usize,
    def: &lavi::StateDefinition,
    config: &JsValue,
) -> js_sys::Object {
    let obj = js_sys::Object::new();
    let state_id = if let Some(ref id) = def.id {
        id.clone()
    } else if parent_id.is_empty() || parent_id == "(machine)" {
        if key == "(machine)" {
            "(machine)".to_string()
        } else {
            format!("(machine).{}", key)
        }
    } else {
        format!("{}.{}", parent_id, key)
    };

    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("id"), &JsValue::from_str(&state_id));
    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("key"), &JsValue::from_str(key));
    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("order"), &JsValue::from_f64(order as f64));

    let type_str = match def.state_type {
        lavi::StateType::Atomic => "atomic",
        lavi::StateType::Compound => "compound",
        lavi::StateType::Parallel => "parallel",
        lavi::StateType::Final => "final",
        lavi::StateType::History { .. } => "history",
    };
    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str(type_str));

    if let Some(ref d) = def.description {
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("description"), &JsValue::from_str(d));
    }

    // invoke
    if config.is_object() {
        let inv_val = js_sys::Reflect::get(config, &JsValue::from_str("invoke")).unwrap_or(JsValue::UNDEFINED);
        if !inv_val.is_undefined() {
            let inv_arr = js_sys::Array::new();
            if js_sys::Array::is_array(&inv_val) {
                let raw_arr = js_sys::Array::from(&inv_val);
                for i in 0..raw_arr.length() {
                    let item = raw_arr.get(i);
                    if item.is_object() {
                        let inv_obj = js_sys::Object::new();
                        let src = js_sys::Reflect::get(&item, &JsValue::from_str("src")).unwrap_or(JsValue::UNDEFINED);
                        let id = js_sys::Reflect::get(&item, &JsValue::from_str("id")).unwrap_or(JsValue::UNDEFINED);
                        let t = js_sys::Reflect::get(&item, &JsValue::from_str("type")).unwrap_or(JsValue::UNDEFINED);
                        let t_final = if t.is_string() { t } else { JsValue::from_str("xstate.invoke") };
                        let _ = js_sys::Reflect::set(&inv_obj, &JsValue::from_str("type"), &t_final);
                        let _ = js_sys::Reflect::set(&inv_obj, &JsValue::from_str("id"), &id);
                        let _ = js_sys::Reflect::set(&inv_obj, &JsValue::from_str("src"), &src);
                        inv_arr.push(&inv_obj);
                    }
                }
            } else if inv_val.is_object() {
                let inv_obj = js_sys::Object::new();
                let src = js_sys::Reflect::get(&inv_val, &JsValue::from_str("src")).unwrap_or(JsValue::UNDEFINED);
                let id = js_sys::Reflect::get(&inv_val, &JsValue::from_str("id")).unwrap_or(JsValue::UNDEFINED);
                let _ = js_sys::Reflect::set(&inv_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.invoke"));
                let _ = js_sys::Reflect::set(&inv_obj, &JsValue::from_str("id"), &id);
                let _ = js_sys::Reflect::set(&inv_obj, &JsValue::from_str("src"), &src);
                inv_arr.push(&inv_obj);
            }
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("invoke"), &inv_arr);
        }
    }

    // entry & exit
    for action_key in &["entry", "exit"] {
        if config.is_object() {
            let act_val = js_sys::Reflect::get(config, &JsValue::from_str(action_key)).unwrap_or(JsValue::UNDEFINED);
            if !act_val.is_undefined() {
                let act_arr = js_sys::Array::new();
                let raw_items = if js_sys::Array::is_array(&act_val) {
                    js_sys::Array::from(&act_val)
                } else {
                    let a = js_sys::Array::new();
                    a.push(&act_val);
                    a
                };
                for i in 0..raw_items.length() {
                    let item = raw_items.get(i);
                    let a_obj = js_sys::Object::new();
                    if item.is_string() {
                        let _ = js_sys::Reflect::set(&a_obj, &JsValue::from_str("type"), &item);
                    } else if item.is_function() {
                        let func = js_sys::Function::from(item);
                        let fn_name: String = func.name().into();
                        let t_name = if fn_name.is_empty() { "(anonymous)" } else { &fn_name };
                        let _ = js_sys::Reflect::set(&a_obj, &JsValue::from_str("type"), &JsValue::from_str(t_name));
                    } else if item.is_object() {
                        let t = js_sys::Reflect::get(&item, &JsValue::from_str("type")).unwrap_or(JsValue::UNDEFINED);
                        let t_final = if t.is_string() { t } else { JsValue::from_str("xstate.action") };
                        let _ = js_sys::Reflect::set(&a_obj, &JsValue::from_str("type"), &t_final);
                    }
                    act_arr.push(&a_obj);
                }
                let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(action_key), &act_arr);
            }
        }
    }

    // on transitions
    let on_obj = js_sys::Object::new();
    if config.is_object() {
        let config_on = js_sys::Reflect::get(config, &JsValue::from_str("on")).unwrap_or(JsValue::UNDEFINED);
        if config_on.is_object() {
            let keys = js_sys::Object::keys(&js_sys::Object::from(config_on.clone()));
            for i in 0..keys.length() {
                let ev = keys.get(i);
                let tr_val = js_sys::Reflect::get(&config_on, &ev).unwrap_or(JsValue::UNDEFINED);
                let tr_arr = js_sys::Array::new();
                let raw_trs = if js_sys::Array::is_array(&tr_val) {
                    js_sys::Array::from(&tr_val)
                } else {
                    let a = js_sys::Array::new();
                    a.push(&tr_val);
                    a
                };
                for j in 0..raw_trs.length() {
                    let raw_t = raw_trs.get(j);
                    let t_obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("eventType"), &ev);
                    let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("source"), &JsValue::from_str(&state_id));
                    let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("actions"), &js_sys::Array::new());
                    let target_arr = js_sys::Array::new();
                    if raw_t.is_string() {
                        let t_str = raw_t.as_string().unwrap_or_default();
                        target_arr.push(&JsValue::from_str(&format!("(machine).{}", t_str)));
                    } else if raw_t.is_object() {
                        let t_target = js_sys::Reflect::get(&raw_t, &JsValue::from_str("target")).unwrap_or(JsValue::UNDEFINED);
                        if t_target.is_string() {
                            let t_str = t_target.as_string().unwrap_or_default();
                            target_arr.push(&JsValue::from_str(&format!("(machine).{}", t_str)));
                        } else if js_sys::Array::is_array(&t_target) {
                            let arr = js_sys::Array::from(&t_target);
                            for k in 0..arr.length() {
                                let el = arr.get(k);
                                if let Some(s) = el.as_string() {
                                    target_arr.push(&JsValue::from_str(&format!("(machine).{}", s)));
                                }
                            }
                        }
                    }
                    let _ = js_sys::Reflect::set(&t_obj, &JsValue::from_str("target"), &target_arr);
                    tr_arr.push(&t_obj);
                }
                let _ = js_sys::Reflect::set(&on_obj, &ev, &tr_arr);
            }
        }
    }
    // Schema requires "on" for atomic states!
    if type_str == "atomic" || js_sys::Object::keys(&on_obj).length() > 0 {
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("on"), &on_obj);
    }

    // Compound or Parallel: states
    if matches!(def.state_type, lavi::StateType::Compound | lavi::StateType::Parallel) || !def.states.is_empty() {
        let states_obj = js_sys::Object::new();
        let child_config_map = if config.is_object() {
            js_sys::Reflect::get(config, &JsValue::from_str("states")).unwrap_or(JsValue::UNDEFINED)
        } else {
            JsValue::UNDEFINED
        };
        for (i, (c_key, c_def)) in def.states.iter().enumerate() {
            let c_config = if child_config_map.is_object() {
                js_sys::Reflect::get(&child_config_map, &JsValue::from_str(c_key)).unwrap_or(JsValue::UNDEFINED)
            } else {
                JsValue::UNDEFINED
            };
            let c_node_obj = serialize_state_node_definition(c_key, &state_id, i, c_def, &c_config);
            let _ = js_sys::Reflect::set(&states_obj, &JsValue::from_str(c_key), &c_node_obj);
        }
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("states"), &states_obj);

        if let Some(ref init_str) = def.initial {
            let init_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&init_obj, &JsValue::from_str("actions"), &js_sys::Array::new());
            let _ = js_sys::Reflect::set(&init_obj, &JsValue::from_str("source"), &JsValue::from_str(&state_id));
            let targets = js_sys::Array::new();
            targets.push(&JsValue::from_str(&format!("{}.{}", state_id, init_str)));
            let _ = js_sys::Reflect::set(&init_obj, &JsValue::from_str("target"), &targets);
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("initial"), &init_obj);
        }
    }

    // History
    if matches!(def.state_type, lavi::StateType::History { .. }) || def.history.is_some() {
        let hist_type = match def.history {
            Some(lavi::HistoryType::Deep) => "deep",
            _ => "shallow",
        };
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("history"), &JsValue::from_str(hist_type));
    }

    // Final: data
    if matches!(def.state_type, lavi::StateType::Final) && config.is_object() {
        let data_val = js_sys::Reflect::get(config, &JsValue::from_str("data")).unwrap_or(JsValue::UNDEFINED);
        if data_val.is_object() {
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("data"), &data_val);
        }
    }


    obj
}

fn serialize_root_machine_definition(
    machine: &lavi::RuntimeMachine,
    config: &JsValue,
) -> js_sys::Object {
    let machine_id = if machine.definition.id.is_empty() {
        "(machine)".to_string()
    } else {
        machine.definition.id.clone()
    };
    let mut root_after = IndexMap::new();
    for (d, t) in &machine.definition.after {
        if let lavi::Delay::Literal(m) = d {
            root_after.insert(*m, t.clone());
        }
    }
    let mut root_def = lavi::StateDefinition::default();
    root_def.state_type = machine.definition.state_type.clone();
    root_def.initial = if machine.definition.initial.is_empty() { None } else { Some(machine.definition.initial.clone()) };
    root_def.states = machine.definition.states.clone();
    root_def.on = machine.definition.on.clone();
    root_def.after = root_after;
    root_def.entry = machine.definition.entry.clone();
    root_def.exit = machine.definition.exit.clone();
    root_def.description = machine.definition.description.clone();
    root_def.meta = machine.definition.meta.clone();
    root_def.output = machine.definition.output.clone();
    let obj = serialize_state_node_definition(&machine_id, "", 0, &root_def, config);
    if config.is_object() {
        let ver = js_sys::Reflect::get(config, &JsValue::from_str("version")).unwrap_or(JsValue::UNDEFINED);
        if !ver.is_undefined() {
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("version"), &ver);
        }
        let ctx = js_sys::Reflect::get(config, &JsValue::from_str("context")).unwrap_or(JsValue::UNDEFINED);
        if ctx.is_object() {
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("context"), &ctx);
        }
    }
    obj
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

         let mut guards = runner.guards.clone();
         let mut actions = runner.actions.clone();
         // Parse config
         parse_implementations(&self.config, &mut guards, &mut actions, &mut runtime_machine.actions_with_effects);
         parse_and_register_delays(&self.config, &mut runtime_machine);
         let machine = runtime_machine;

         Ok(StateNode {
             machine,
             path: "".to_string(),
             guards,
             actions,
             action_descriptors: runner.action_descriptors.clone(),
             config: val,
             logger_holder: runner.logger_holder.clone(),
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

fn check_initial_entry_guards(runner: &Actor) {
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
            logger: None,
            logger_holder: self.logger_holder.clone(),
            input_js: None,
            initialization_error: None,
            action_descriptors: self.action_descriptors.clone(),
        };
        runner.sync_tracked_entry_exit(&self.config);

        let override_cell = tick_override;
        runner.machine.register_clock(Rc::new(move || {
            override_cell.get().unwrap_or_else(|| js_sys::Date::now() as u64)
        }));
        runner.machine.register_warn_fn(Rc::new(|msg| {
            web_sys::console::warn_1(&JsValue::from_str(msg));
        }));

        configure_logger(&options, &mut runner);
        parse_and_register_delays(&options, &mut runner.machine);

        let input_val = if options.is_object() {
            js_sys::Reflect::get(&options, &JsValue::from_str("input")).unwrap_or(JsValue::UNDEFINED)
        } else {
            JsValue::UNDEFINED
        };
        if !input_val.is_undefined() {
            runner.input_js = Some(input_val.clone());
            runner.machine.input = Some(js_to_value(&input_val));
        }

        if self.config.is_object() {
             let context_key = JsValue::from_str("context");
             let context_val = js_sys::Reflect::get(&self.config, &context_key).unwrap_or(JsValue::UNDEFINED);
             if context_val.is_function() {
                 if let Ok(func) = context_val.dyn_into::<js_sys::Function>() {
                     let arg_obj = js_sys::Object::new();
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("input"), &input_val);
                     let self_obj = js_sys::Object::new();
                     let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &self_obj);
                     match func.call1(&JsValue::NULL, &arg_obj) {
                         Ok(res) => {
                             runner.machine.context = js_to_value(&res);
                         }
                         Err(err) => {
                             runner.initialization_error = Some(err);
                         }
                     }
                 }
             }
        }

        // Apply options
        if options.is_object() {
             let inspect_key = JsValue::from_str("inspect");
             let inspect_val = js_sys::Reflect::get(&options, &inspect_key).unwrap_or(JsValue::UNDEFINED);
             if inspect_val.is_function() {
                  let func: js_sys::Function = inspect_val.dyn_into().unwrap();
                  runner.set_inspector(func);
             }
             let context_opt = js_sys::Reflect::get(&options, &JsValue::from_str("context")).unwrap_or(JsValue::UNDEFINED);
             if !context_opt.is_undefined() {
                 runner.machine.context = js_to_value(&context_opt);
             }
             runner.restore_snapshot_option(&options)?;
             parse_implementations(&options, &mut runner.guards, &mut runner.actions, &mut runner.machine.actions_with_effects);
             runner.apply_clock_option(&options);
        }

        check_initial_entry_guards(&runner);
        runner.push_shared();
        Ok(runner)
    }
}

#[wasm_bindgen]
#[derive(Clone, Default)]
pub struct SimulatedClock {
    clock: lavi::SimulatedClock,
    actors: Rc<RefCell<Vec<Actor>>>,
}

#[wasm_bindgen]
impl SimulatedClock {
    #[wasm_bindgen(constructor)]
    pub fn new() -> SimulatedClock {
        SimulatedClock {
            clock: lavi::SimulatedClock::new(),
            actors: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[wasm_bindgen(js_name = __registerActor)]
    pub fn register_actor(&mut self, actor: &Actor) {
        let mut actor_clone = actor.clone();
        actor_clone.pull_shared();
        if let Some(shared) = actor_clone.shared_state.borrow_mut().as_mut() {
            shared.is_simulated_clock = true;
        }
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
                .filter(|a| !a.is_stopped && !a.machine.is_done() && a.machine.status != lavi::MachineStatus::Done)
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
                if !a.is_stopped && !a.machine.is_done() && a.machine.status != lavi::MachineStatus::Done {
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
pub(crate) enum Subscriber {
    Fn(js_sys::Function),
    Observer {
        next: Option<js_sys::Function>,
        complete: Option<js_sys::Function>,
        #[allow(dead_code)]
        error: Option<js_sys::Function>,
    },
}

pub(crate) struct ActiveTimeout {
    pub handle: JsValue,
}

pub(crate) struct SharedActorState {
    pub machine: RuntimeMachine,
    pub inspector: Option<js_sys::Function>,
    pub subscribers: Vec<(u64, Subscriber)>,
    pub guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    pub actions: IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    pub is_stopped: bool,
    pub is_initialized: bool,
    pub is_in_tick: bool,
    pub reentrant_queue: std::collections::VecDeque<(String, Value)>,
    pub tick_override: Rc<core::cell::Cell<Option<u64>>>,
    pub host_clock: Option<JsValue>,
    pub is_simulated_clock: bool,
    pub active_timeout: Option<ActiveTimeout>,
    pub cached_output: Option<JsValue>,
    pub is_completed_emitted: bool,
    pub logger: Option<js_sys::Function>,
    pub logger_holder: Rc<RefCell<Option<js_sys::Function>>>,
    pub input_js: Option<JsValue>,
    pub initialization_error: Option<JsValue>,
    pub action_descriptors: IndexMap<String, JsValue>,
}

fn resolve_host_clock(custom_clock: Option<&JsValue>) -> Option<(js_sys::Function, js_sys::Function)> {
    if let Some(opts) = custom_clock {
        if opts.is_object() && !opts.is_null() {
            let st = js_sys::Reflect::get(opts, &JsValue::from_str("setTimeout")).unwrap_or(JsValue::UNDEFINED);
            let ct = js_sys::Reflect::get(opts, &JsValue::from_str("clearTimeout")).unwrap_or(JsValue::UNDEFINED);
            if let (Ok(st_fn), Ok(ct_fn)) = (st.dyn_into::<js_sys::Function>(), ct.dyn_into::<js_sys::Function>()) {
                return Some((st_fn, ct_fn));
            }
        }
    }
    let global = js_sys::global();
    let st = js_sys::Reflect::get(&global, &JsValue::from_str("setTimeout")).unwrap_or(JsValue::UNDEFINED);
    let ct = js_sys::Reflect::get(&global, &JsValue::from_str("clearTimeout")).unwrap_or(JsValue::UNDEFINED);
    if let (Ok(st_fn), Ok(ct_fn)) = (st.dyn_into::<js_sys::Function>(), ct.dyn_into::<js_sys::Function>()) {
        Some((st_fn, ct_fn))
    } else {
        None
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct Actor {
    pub(crate) machine: RuntimeMachine,
    pub(crate) inspector: Option<js_sys::Function>,
    pub(crate) guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    pub(crate) actions: IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    pub(crate) action_descriptors: IndexMap<String, JsValue>,
    pub(crate) config: JsValue,
    pub is_stopped: bool,
    pub(crate) is_initialized: bool,
    pub(crate) shared_state: Rc<RefCell<Option<SharedActorState>>>,
    pub(crate) tick_override: Rc<core::cell::Cell<Option<u64>>>,
    pub(crate) logger: Option<js_sys::Function>,
    pub(crate) logger_holder: Rc<RefCell<Option<js_sys::Function>>>,
    pub(crate) input_js: Option<JsValue>,
    pub(crate) initialization_error: Option<JsValue>,
}

fn evaluate_guard_entry(
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

fn parse_and_register_delays(options: &JsValue, machine: &mut RuntimeMachine) {
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

fn configure_logger(options: &JsValue, runner: &mut Actor) {
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
fn parse_implementations(
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

#[wasm_bindgen]
impl Actor {
    #[wasm_bindgen(constructor)]
    pub fn new(val: JsValue, options: JsValue) -> Result<Actor, JsValue> {
        let mut guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>> = IndexMap::new();
        let mut actions = IndexMap::new();
        let mut actions_with_effects = IndexMap::new();
        let mut action_descriptors = IndexMap::new();

        parse_implementations(&options, &mut guards, &mut actions, &mut actions_with_effects);

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
                                 action_descriptors: wasm_machine_ref.action_descriptors.clone(),
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
             action_descriptors = internal_state.action_descriptors;
             let mut m = internal_state.machine;
             m.actions_with_effects.extend(actions_with_effects);
             m
        } else if val.is_string() {
             let json_definition = val.as_string().unwrap();
             let def = lavi::create_machine(&json_definition)
                .map_err(|e| JsValue::from_str(&format!("Failed to parse machine: {}", e)))?;
             let mut m = RuntimeMachine::from_definition(def).map_err(|e| JsValue::from_str(&e))?;
             m.actions_with_effects.extend(actions_with_effects);
             m
        } else {
             let normalized_val = normalize_undefined(&val, 0);
             let (cleaned_val, extra_actions) = extract_inline_assignments(&normalized_val)?;
             let mut dummy_guards = IndexMap::new();
             let mut dummy_actions = IndexMap::new();
             let mut dummy_actions_with_effects = IndexMap::new();
             let mut dummy_action_descriptors = IndexMap::new();
             let mut counter = 0;
             let guards_holder = Rc::new(RefCell::new(IndexMap::new()));
             let dummy_logger_holder = Rc::new(RefCell::new(None));
             process_config(&cleaned_val, &mut dummy_guards, &mut dummy_actions, &mut dummy_actions_with_effects, &mut dummy_action_descriptors, 0, &mut counter, "", &guards_holder, &dummy_logger_holder);
             guards.extend(dummy_guards);
             guards_holder.borrow_mut().extend(guards.clone());
             actions.extend(dummy_actions);
             actions.extend(extra_actions);
             action_descriptors.extend(dummy_action_descriptors);

             let json_string = js_sys::JSON::stringify(&cleaned_val)
                .map_err(|_| JsValue::from_str("Failed to stringify machine definition"))?
                .as_string().unwrap_or_default();
             let machine_def = lavi::create_machine(&json_string)
                .map_err(|e| JsValue::from_str(&format!("Failed to parse definition: {}", e)))?;
             let mut m = RuntimeMachine::from_definition(machine_def).map_err(|e| JsValue::from_str(&e))?;
             m.actions_with_effects.extend(dummy_actions_with_effects);
             m.actions_with_effects.extend(actions_with_effects);
             m
        };

        parse_and_register_delays(&options, &mut machine);

        let tick_override = Rc::new(core::cell::Cell::new(None));
        let override_cell = tick_override.clone();
        machine.register_clock(Rc::new(move || {
            override_cell.get().unwrap_or_else(|| js_sys::Date::now() as u64)
        }));
        machine.register_warn_fn(Rc::new(|msg| {
            web_sys::console::warn_1(&JsValue::from_str(msg));
        }));

        let mut output_funcs = IndexMap::new();
        if val.is_object() {
            collect_output_resolvers(&val, "", &mut output_funcs);
        }
        for (p, func) in output_funcs {
            let f = func.clone();
            let resolver: lavi::OutputResolver = Rc::new(move |ctx: &Value, evt: &Value| {
                let ctx_js = value_to_js(ctx);
                let evt_js = value_to_js(evt);
                let arg_obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), &ctx_js);
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &evt_js);
                if let Ok(res) = f.call1(&JsValue::NULL, &arg_obj) {
                    crate::utils::js_to_value(&res)
                } else {
                    Value::Null
                }
            });
            machine.register_output_resolver(&p, resolver);
        }

        let mut actor = Actor { 
            machine, 
            inspector: None,
            guards,
            actions,
            action_descriptors,
            is_stopped: false,
            config: val.clone(),
            is_initialized: false,
            shared_state: Rc::new(RefCell::new(None)),
            tick_override,
            logger: None,
            logger_holder: Rc::new(RefCell::new(None)),
            input_js: None,
            initialization_error: None,
        };
        configure_logger(&options, &mut actor);
        actor.restore_snapshot_option(&options)?;
        actor.sync_tracked_entry_exit(&actor.config.clone());
        actor.apply_clock_option(&options);
        actor.push_shared();
        Ok(actor)
    }

    pub(crate) fn restore_snapshot_option(&mut self, options: &JsValue) -> Result<(), JsValue> {
        if !options.is_object() || options.is_null() {
            return Ok(());
        }

        let mut snapshot_val = JsValue::UNDEFINED;
        let snapshot_key = JsValue::from_str("snapshot");
        if let Ok(val) = js_sys::Reflect::get(options, &snapshot_key) {
            if !val.is_undefined() {
                snapshot_val = val;
            }
        }
        if snapshot_val.is_undefined() {
            let state_key = JsValue::from_str("state");
            if let Ok(val) = js_sys::Reflect::get(options, &state_key) {
                if !val.is_undefined() {
                    snapshot_val = val;
                }
            }
        }

        if !snapshot_val.is_undefined() && !snapshot_val.is_null() {
            let json_str = if snapshot_val.is_string() {
                snapshot_val.as_string().unwrap()
            } else {
                js_sys::JSON::stringify(&snapshot_val)?
                    .as_string()
                    .unwrap_or_default()
            };

            if !json_str.is_empty() && json_str != "{}" {
                self.machine
                    .restore_from_snapshot_json(&json_str)
                    .map_err(|e| JsValue::from_str(&format!("Failed to restore snapshot: {}", e)))?;

                self.is_initialized = true;
                if self.machine.status == lavi::MachineStatus::Stopped {
                    self.is_stopped = true;
                }
            }
        }
        Ok(())
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
        if let Some(shared) = borrow.as_mut() {
            shared.machine = self.machine.clone();
            shared.inspector = self.inspector.clone();
            shared.guards = self.guards.clone();
            shared.actions = self.actions.clone();
            shared.is_stopped = self.is_stopped;
            shared.is_initialized = self.is_initialized;
            shared.logger = self.logger.clone();
            shared.logger_holder = self.logger_holder.clone();
            shared.action_descriptors = self.action_descriptors.clone();
            if !shared.machine.is_done() && shared.machine.status != lavi::MachineStatus::Done {
                shared.cached_output = None;
                shared.is_completed_emitted = false;
            }
        } else {
            *borrow = Some(SharedActorState {
                machine: self.machine.clone(),
                inspector: self.inspector.clone(),
                subscribers: Vec::new(),
                guards: self.guards.clone(),
                actions: self.actions.clone(),
                is_stopped: self.is_stopped,
                is_initialized: self.is_initialized,
                is_in_tick: false,
                reentrant_queue: std::collections::VecDeque::new(),
                tick_override: self.tick_override.clone(),
                host_clock: None,
                is_simulated_clock: false,
                active_timeout: None,
                cached_output: None,
                is_completed_emitted: false,
                logger: self.logger.clone(),
                logger_holder: self.logger_holder.clone(),
                input_js: self.input_js.clone(),
                initialization_error: self.initialization_error.clone(),
                action_descriptors: self.action_descriptors.clone(),
            });
        }
    }

    pub(crate) fn emit_error(&self, err_val: &JsValue) {
        let subscribers = {
            let borrow = self.shared_state.borrow();
            borrow
                .as_ref()
                .map(|s| s.subscribers.clone())
                .unwrap_or_default()
        };
        for (_, sub) in &subscribers {
            if let Subscriber::Observer { error: Some(ref err_fn), .. } = sub {
                let _ = err_fn.call1(&JsValue::NULL, err_val);
            }
        }
    }

    pub(crate) fn set_in_tick(&self, in_tick: bool) {
        if let Some(shared) = self.shared_state.borrow_mut().as_mut() {
            shared.is_in_tick = in_tick;
        }
    }

    pub(crate) fn drain_reentrant_shared(&mut self) -> bool {
        let mut transitioned = false;
        loop {
            if self.is_stopped
                || self.machine.status == lavi::MachineStatus::Stopped
                || self.machine.status == lavi::MachineStatus::Done
                || self.machine.is_done()
            {
                if let Some(shared) = self.shared_state.borrow_mut().as_mut() {
                    shared.reentrant_queue.clear();
                }
                break;
            }
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

        self.push_shared();

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
                if let Some(shared) = self.shared_state.borrow_mut().as_mut() {
                    shared.is_simulated_clock = true;
                }
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

            let st = js_sys::Reflect::get(&clock_val, &JsValue::from_str("setTimeout"))
                .unwrap_or(JsValue::UNDEFINED);
            let ct = js_sys::Reflect::get(&clock_val, &JsValue::from_str("clearTimeout"))
                .unwrap_or(JsValue::UNDEFINED);
            if st.is_function() && ct.is_function() {
                if let Some(shared) = self.shared_state.borrow_mut().as_mut() {
                    shared.host_clock = Some(clock_val.clone());
                }
            }
        }
    }

    pub(crate) fn set_inspector(&mut self, callback: js_sys::Function) {
        self.inspector = Some(callback);
    }



    fn get_history_value_obj(&self, machine: &RuntimeMachine) -> JsValue {
        compute_history_value_obj(
            machine,
            &self.guards,
            &self.actions,
            &self.action_descriptors,
            &self.config,
            &self.logger_holder,
        )
    }

    #[wasm_bindgen(js_name = getSnapshot)]
    pub fn get_snapshot(&self) -> Snapshot {
        let init_err = {
            let borrow = self.shared_state.borrow();
            borrow
                .as_ref()
                .and_then(|s| s.initialization_error.clone())
                .or_else(|| self.initialization_error.clone())
        };
        if let Some(err) = init_err {
            return Snapshot {
                value: JsValue::UNDEFINED,
                context: JsValue::UNDEFINED,
                status: "error".to_string(),
                machine_ref: self.machine.clone(),
                guards: self.guards.clone(),
                history_value: js_sys::Object::new().into(),
                config: self.config.clone(),
                output: JsValue::UNDEFINED,
                error: err,
            };
        }

        let (mut active_machine, is_stopped, is_initialized, cached_out, guards) = {
            let shared_borrow = self.shared_state.borrow();
            if let Some(s) = shared_borrow.as_ref() {
                (s.machine.clone(), s.is_stopped, s.is_initialized, s.cached_output.clone(), s.guards.clone())
            } else {
                (self.machine.clone(), self.is_stopped, self.is_initialized, None, self.guards.clone())
            }
        };
        if !active_machine.is_started && !is_initialized {
            let mut snapshot_actions: IndexMap<String, lavi::ActionFunction> = IndexMap::new();
            snapshot_actions.insert(
                "__lavi_snapshot_mode__".to_string(),
                Rc::new(|_, _, _| {}),
            );
            active_machine.start(&guards, &snapshot_actions);
        }
        let machine_ref = &active_machine;

        let active_paths = machine_ref.current_state.get_active_paths();
        let value_val = Self::build_snapshot_state_value(machine_ref, &active_paths);
        let ctx_val = match &machine_ref.context {
            Value::Null => js_sys::Object::new().into(),
            other => value_to_js(other),
        };
        let history_obj = self.get_history_value_obj(machine_ref);
        let status = if machine_ref.is_done() {
            "done".to_string()
        } else if is_stopped
            || machine_ref.status() == lavi::MachineStatus::Stopped
        {
            "stopped".to_string()
        } else {
            "active".to_string()
        };
        let mut final_machine = machine_ref.clone();
        if is_stopped
            && !final_machine.is_done()
            && final_machine.status != lavi::MachineStatus::Stopped
        {
            final_machine.stop();
        }

        let output_val = if status == "done" {
            if let Some(c) = cached_out {
                c
            } else {
                let evaluated = Self::compute_output_js(&self.config, machine_ref, &ctx_val);
                if let Some(shared) = self.shared_state.borrow_mut().as_mut() {
                    shared.cached_output = Some(evaluated.clone());
                }
                evaluated
            }
        } else {
            JsValue::UNDEFINED
        };

        Snapshot {
            value: value_val,
            context: ctx_val,
            status,
            machine_ref: final_machine,
            guards: self.guards.clone(),
            history_value: history_obj,
            config: self.config.clone(),
            output: output_val,
            error: JsValue::UNDEFINED,
        }
    }

    pub(crate) fn compute_output_js(config: &JsValue, machine_ref: &RuntimeMachine, ctx_val: &JsValue) -> JsValue {
        if config.is_object() {
            let output_key = JsValue::from_str("output");
            let output_prop = js_sys::Reflect::get(config, &output_key).unwrap_or(JsValue::UNDEFINED);
            if !output_prop.is_undefined() {
                if let Ok(func) = output_prop.clone().dyn_into::<js_sys::Function>() {
                    let arg_obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("context"), ctx_val);
                    let done_evt = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(
                        &done_evt,
                        &JsValue::from_str("type"),
                        &JsValue::from_str(&format!("xstate.done.state.{}", machine_ref.definition.id)),
                    );
                    let leaf_out = machine_ref.get_output();
                    let out_js = match leaf_out {
                        Some(val) => value_to_js(&val),
                        None => JsValue::UNDEFINED,
                    };
                    let _ = js_sys::Reflect::set(&done_evt, &JsValue::from_str("output"), &out_js);
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("event"), &done_evt);
                    let self_obj = js_sys::Object::new();
                    let send_fn = js_sys::Function::new_no_args("");
                    let _ = js_sys::Reflect::set(&self_obj, &JsValue::from_str("send"), &send_fn);
                    let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &self_obj);

                    if let Ok(res) = func.call1(&JsValue::NULL, &arg_obj) {
                        return res;
                    } else {
                        return JsValue::UNDEFINED;
                    }
                } else {
                    return output_prop;
                }
            }
        }
        if let Some(out) = machine_ref.get_output() {
            value_to_js(&out)
        } else {
            JsValue::UNDEFINED
        }
    }

    pub(crate) fn event_to_js(evt: &Value) -> JsValue {
        let evt_js = value_to_js(evt);
        if let Value::Object(map) = evt {
            if let Some(Value::String(t)) = map.get("type") {
                if t.starts_with("xstate.done.state.") && !map.contains_key("output") {
                    let _ = js_sys::Reflect::set(&evt_js, &JsValue::from_str("output"), &JsValue::UNDEFINED);
                }
            }
        }
        evt_js
    }

    pub(crate) fn build_snapshot_state_value(machine: &RuntimeMachine, active_paths: &[String]) -> JsValue {
        if machine.definition.states.is_empty() {
            return js_sys::Object::new().into();
        }
        Self::build_state_value_for_node(machine, "", active_paths)
    }

    fn build_state_value_for_node(machine: &RuntimeMachine, state_path: &str, active_paths: &[String]) -> JsValue {
        let (is_parallel, states) = if state_path.is_empty() {
            (
                matches!(machine.definition.state_type, StateType::Parallel),
                &machine.definition.states,
            )
        } else if let Some(def) = machine.get_state_definition(state_path) {
            (
                matches!(def.state_type, StateType::Parallel),
                &def.states,
            )
        } else {
            return JsValue::NULL;
        };

        if is_parallel {
            let obj = js_sys::Object::new();
            for (r_name, r_def) in states {
                if matches!(r_def.state_type, StateType::History { .. }) || r_def.history.is_some() {
                    continue;
                }
                let child_path = if state_path.is_empty() {
                    r_name.clone()
                } else {
                    format!("{}.{}", state_path, r_name)
                };
                if r_def.states.is_empty() {
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(r_name), &js_sys::Object::new());
                } else {
                    let child_val = Self::build_state_value_for_node(machine, &child_path, active_paths);
                    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(r_name), &child_val);
                }
            }
            obj.into()
        } else {
            let prefix = if state_path.is_empty() {
                "".to_string()
            } else {
                format!("{}.", state_path)
            };
            for path in active_paths {
                let direct_child = if prefix.is_empty() {
                    path.split('.').next().unwrap_or("")
                } else if let Some(rest) = path.strip_prefix(&prefix) {
                    rest.split('.').next().unwrap_or("")
                } else {
                    continue;
                };
                if direct_child.is_empty() {
                    continue;
                }
                if let Some(child_def) = states.get(direct_child) {
                    let child_path = if prefix.is_empty() {
                        direct_child.to_string()
                    } else {
                        format!("{}{}", prefix, direct_child)
                    };
                    if child_def.states.is_empty() {
                        return JsValue::from_str(direct_child);
                    } else {
                        let sub_val = Self::build_state_value_for_node(machine, &child_path, active_paths);
                        let obj = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(direct_child), &sub_val);
                        return obj.into();
                    }
                }
            }
            JsValue::NULL
        }
    }

    #[wasm_bindgen]
    pub fn start(&self) -> Actor {
        if self.initialization_error.is_some() || {
            let borrow = self.shared_state.borrow();
            borrow.as_ref().and_then(|s| s.initialization_error.as_ref()).is_some()
        } {
            return self.clone();
        }

        let (mut machine, guards, actions, is_started, is_initialized) = {
            let borrow = self.shared_state.borrow();
            if let Some(shared) = borrow.as_ref() {
                (
                    shared.machine.clone(),
                    shared.guards.clone(),
                    shared.actions.clone(),
                    shared.machine.is_started,
                    shared.is_initialized,
                )
            } else {
                (
                    self.machine.clone(),
                    self.guards.clone(),
                    self.actions.clone(),
                    self.machine.is_started,
                    self.is_initialized,
                )
            }
        };

        if is_started {
            return self.clone();
        }

        self.set_in_tick(true);

        if !is_initialized {
            machine.start(&guards, &actions);
        } else {
            machine.is_started = true;
            if machine.status != lavi::MachineStatus::Done && !machine.is_done() {
                machine.revive_timers();
                let mut loop_count = 0;
                while loop_count < 100 {
                    let res = machine.transition("", &Value::Null, &guards, &actions);
                    if res.is_none() { break; }
                    loop_count += 1;
                }
            }
        }

        loop {
            let is_now_stopped = {
                let borrow = self.shared_state.borrow();
                borrow.as_ref().map(|s| s.is_stopped).unwrap_or(false)
            };
            if is_now_stopped || machine.status == lavi::MachineStatus::Stopped {
                machine.stop();
                if let Some(shared) = self.shared_state.borrow_mut().as_mut() {
                    shared.reentrant_queue.clear();
                    shared.is_stopped = true;
                }
                break;
            }
            if machine.is_done() || machine.status == lavi::MachineStatus::Done {
                if let Some(shared) = self.shared_state.borrow_mut().as_mut() {
                    shared.reentrant_queue.clear();
                }
                break;
            }
            let next_reentrant = {
                let mut borrow = self.shared_state.borrow_mut();
                borrow.as_mut().and_then(|s| s.reentrant_queue.pop_front())
            };
            let Some((re_name, re_payload)) = next_reentrant else {
                break;
            };
            let _ = machine.transition(&re_name, &re_payload, &guards, &actions);
        }

        {
            let mut borrow = self.shared_state.borrow_mut();
            if let Some(shared) = borrow.as_mut() {
                shared.machine = machine;
                shared.is_initialized = true;
                shared.is_in_tick = false;
            }
        }

        let last_err = {
            let mut borrow = self.shared_state.borrow_mut();
            borrow.as_mut().and_then(|s| s.machine.last_error.take())
        };
        if let Some(err_msg) = last_err {
            let js_err = js_sys::Error::new(&err_msg);
            self.emit_error(&js_err.into());
        }

        // Emit initial snapshot
        self.emit_snapshot();

        // Schedule host timer for any active deadline
        Actor::reschedule_host_timer(&self.shared_state);

        self.clone()
    }

    fn emit_snapshot(&self) {
        let snapshot = self.get_snapshot();
        let status_str = snapshot.status();
        let should_complete = status_str == "done";
        let is_completed = if should_complete {
            let mut borrow = self.shared_state.borrow_mut();
            if let Some(shared) = borrow.as_mut() {
                if !shared.is_completed_emitted {
                    shared.is_completed_emitted = true;
                    true
                } else {
                    false
                }
            } else {
                true
            }
        } else {
            false
        };
        let obj: JsValue = snapshot.into();
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.snapshot"));
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("status"), &JsValue::from_str(&status_str));

        let (subscribers, inspector) = {
            let borrow = self.shared_state.borrow();
            if let Some(shared) = borrow.as_ref() {
                (shared.subscribers.clone(), shared.inspector.clone())
            } else {
                (Vec::new(), self.inspector.clone())
            }
        };

        if let Some(insp) = &inspector {
            let _ = insp.call1(&JsValue::NULL, &obj);
        }

        for (_, sub) in &subscribers {
            match sub {
                Subscriber::Fn(func) => {
                    let _ = func.call1(&JsValue::NULL, &obj);
                }
                Subscriber::Observer { next, complete, .. } => {
                    if let Some(next_fn) = next {
                        let _ = next_fn.call1(&JsValue::NULL, &obj);
                    }
                    if is_completed {
                        if let Some(complete_fn) = complete {
                            let _ = complete_fn.call0(&JsValue::NULL);
                        }
                    }
                }
            }
        }
    }


    pub fn send(&self, event: JsValue) -> Result<JsValue, JsValue> {
        let (event_name, event_payload) = if event.is_string() {
            let s = event.as_string().unwrap();
            let payload = Value::Object(lavi::indexmap! { "type".to_string() => Value::String(s.clone()) });
            (s, payload)
        } else {
             // Assume object
             let payload = js_to_value(&event);
             let name = payload.as_object().and_then(|m| m.get("type")).and_then(|t| t.as_string()).map(|s| s.as_str()).unwrap_or("UNKNOWN").to_string();
             (name, payload)
        };

        let inspector = {
            let borrow = self.shared_state.borrow();
            borrow.as_ref().and_then(|s| s.inspector.clone()).or_else(|| self.inspector.clone())
        };
        if let Some(inspector) = &inspector {
             let obj = js_sys::Object::new();
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str("@xstate.event"));
             let evt_val = value_to_js(&event_payload);
             let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("event"), &evt_val);

             let _ = inspector.call1(&JsValue::NULL, &obj);
        }

        let (is_started, is_stopped, is_done, is_in_tick) = {
            let borrow = self.shared_state.borrow();
            if let Some(shared) = borrow.as_ref() {
                (
                    shared.machine.is_started,
                    shared.is_stopped || shared.machine.status == lavi::MachineStatus::Stopped,
                    shared.machine.is_done(),
                    shared.is_in_tick,
                )
            } else {
                (
                    self.machine.is_started,
                    self.is_stopped || self.machine.status == lavi::MachineStatus::Stopped,
                    self.machine.is_done(),
                    false,
                )
            }
        };

        if !is_started || is_stopped || is_done {
            return Ok(JsValue::UNDEFINED);
        }

        if is_in_tick {
            if let Some(shared) = self.shared_state.borrow_mut().as_mut() {
                shared.reentrant_queue.push_back((event_name, event_payload));
            }
            return Ok(JsValue::UNDEFINED);
        }

        self.set_in_tick(true);

        let (mut machine, guards, actions) = {
            let borrow = self.shared_state.borrow();
            if let Some(shared) = borrow.as_ref() {
                (shared.machine.clone(), shared.guards.clone(), shared.actions.clone())
            } else {
                (self.machine.clone(), self.guards.clone(), self.actions.clone())
            }
        };

        let _ = machine.transition(&event_name, &event_payload, &guards, &actions);

        loop {
            let is_now_stopped = {
                let borrow = self.shared_state.borrow();
                borrow.as_ref().map(|s| s.is_stopped).unwrap_or(false)
            };
            if is_now_stopped || machine.status == lavi::MachineStatus::Stopped {
                machine.stop();
                if let Some(shared) = self.shared_state.borrow_mut().as_mut() {
                    shared.reentrant_queue.clear();
                    shared.is_stopped = true;
                }
                break;
            }
            if machine.is_done() {
                if let Some(shared) = self.shared_state.borrow_mut().as_mut() {
                    shared.reentrant_queue.clear();
                }
                break;
            }
            let next_reentrant = {
                let mut borrow = self.shared_state.borrow_mut();
                borrow.as_mut().and_then(|s| s.reentrant_queue.pop_front())
            };
            let Some((re_name, re_payload)) = next_reentrant else {
                break;
            };
            let _ = machine.transition(&re_name, &re_payload, &guards, &actions);
        }

        {
            let mut borrow = self.shared_state.borrow_mut();
            if let Some(shared) = borrow.as_mut() {
                shared.machine = machine;
                shared.is_in_tick = false;
            }
        }

        let last_err = {
            let mut borrow = self.shared_state.borrow_mut();
            borrow.as_mut().and_then(|s| s.machine.last_error.take())
        };
        if let Some(err_msg) = last_err {
            let js_err = js_sys::Error::new(&err_msg);
            self.emit_error(&js_err.into());
        }

        // 2. Inspect Snapshot
        self.emit_snapshot();

        // Host timer scheduling
        Actor::reschedule_host_timer(&self.shared_state);
        Ok(JsValue::UNDEFINED)
    }

    #[wasm_bindgen(js_name = subscribe)]
    pub fn subscribe_js(&self, observer_or_fn: JsValue) -> Result<JsValue, JsValue> {
        self.subscribe_internal(observer_or_fn)
    }

    fn subscribe_internal(&self, observer_or_fn: JsValue) -> Result<JsValue, JsValue> {
        let sub = if observer_or_fn.is_function() {
            Subscriber::Fn(observer_or_fn.unchecked_into())
        } else if observer_or_fn.is_object() && !observer_or_fn.is_null() {
            let obj = js_sys::Object::from(observer_or_fn);
            let next_fn = js_sys::Reflect::get(&obj, &JsValue::from_str("next"))
                .ok()
                .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
            let complete_fn = js_sys::Reflect::get(&obj, &JsValue::from_str("complete"))
                .ok()
                .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
            let error_fn = js_sys::Reflect::get(&obj, &JsValue::from_str("error"))
                .ok()
                .and_then(|v| v.dyn_into::<js_sys::Function>().ok());
            Subscriber::Observer {
                next: next_fn,
                complete: complete_fn,
                error: error_fn,
            }
        } else {
            return Err(JsValue::from_str("subscribe requires a function or observer"));
        };

        static NEXT_SUB_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);
        let sub_id = NEXT_SUB_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

        let is_done = {
            let borrow = self.shared_state.borrow();
            borrow
                .as_ref()
                .map(|s| s.machine.is_done() || s.machine.status == lavi::MachineStatus::Done)
                .unwrap_or(false)
        };

        if is_done {
            if let Subscriber::Observer { complete: Some(ref comp), .. } = sub {
                let _ = comp.call0(&JsValue::NULL);
            }
        }

        {
            let mut borrow = self.shared_state.borrow_mut();
            if let Some(shared) = borrow.as_mut() {
                shared.subscribers.push((sub_id, sub));
            }
        }

        let shared_weak = Rc::downgrade(&self.shared_state);
        let unsub_closure = Closure::wrap(Box::new(move || {
            if let Some(shared_rc) = shared_weak.upgrade() {
                if let Ok(mut borrow) = shared_rc.try_borrow_mut() {
                    if let Some(shared) = borrow.as_mut() {
                        shared.subscribers.retain(|(id, _)| *id != sub_id);
                    }
                }
            }
        }) as Box<dyn FnMut()>);

        let unsub_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &unsub_obj,
            &JsValue::from_str("unsubscribe"),
            &unsub_closure.into_js_value(),
        );

        Ok(unsub_obj.into())
    }

    #[wasm_bindgen]
    pub fn stop(&self) -> Actor {
        let mut borrow = self.shared_state.borrow_mut();
        if let Some(shared) = borrow.as_mut() {
            if shared.machine.status == lavi::MachineStatus::Done || shared.machine.is_done() {
                return self.clone();
            }
            shared.is_stopped = true;
            shared.machine.stop();
            if let Some((_, clear_timeout)) = resolve_host_clock(shared.host_clock.as_ref()) {
                if let Some(active) = shared.active_timeout.take() {
                    let _ = clear_timeout.call1(&JsValue::NULL, &active.handle);
                }
            }
        }
        self.clone()
    }

    #[wasm_bindgen(js_name = getPersistedSnapshot)]
    pub fn get_persisted_snapshot(&self) -> Result<JsValue, JsValue> {
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
        } else if is_stopped
            && !active_machine.is_done()
            && active_machine.status != lavi::MachineStatus::Done
            && active_machine.status != lavi::MachineStatus::Stopped
        {
            let mut m = active_machine.clone();
            m.stop();
            preview = m;
            &preview
        } else {
            active_machine
        };

        let json_str = machine_ref.get_persisted_snapshot_json();
        js_sys::JSON::parse(&json_str)
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
        if active_machine.is_done() || active_machine.status == lavi::MachineStatus::Done {
            return None;
        }
        active_machine.next_timer_deadline().map(|d| d as f64)
    }

    #[wasm_bindgen]
    pub fn tick(&mut self) -> Result<(), JsValue> {
        self.pull_shared();
        if !self.machine.is_started
            || self.is_stopped
            || self.machine.status == lavi::MachineStatus::Stopped
            || self.machine.status == lavi::MachineStatus::Done
            || self.machine.is_done()
        {
            return Ok(());
        }
        let target_ms = if let Some(override_now) = self.tick_override.get() {
            override_now
        } else if let Some(clock_fn) = &self.machine.clock_millis_fn {
            clock_fn()
        } else {
            js_sys::Date::now() as u64
        };
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
        Actor::reschedule_host_timer(&self.shared_state);
        Ok(())
    }

    pub(crate) fn reschedule_host_timer(shared_rc: &Rc<RefCell<Option<SharedActorState>>>) {
        let mut borrow = shared_rc.borrow_mut();
        let Some(shared) = borrow.as_mut() else {
            return;
        };

        if shared.is_simulated_clock {
            return;
        }

        let Some((set_timeout, clear_timeout)) = resolve_host_clock(shared.host_clock.as_ref()) else {
            return;
        };

        // Cancel existing timer handle if any
        if let Some(active) = shared.active_timeout.take() {
            let _ = clear_timeout.call1(&JsValue::NULL, &active.handle);
        }

        // If stopped or done, do not schedule
        if shared.is_stopped
            || shared.machine.status == lavi::MachineStatus::Stopped
            || shared.machine.status == lavi::MachineStatus::Done
            || shared.machine.is_done()
        {
            return;
        }

        // Check if there is an active deadline
        let Some(deadline) = shared.machine.next_timer_deadline() else {
            return;
        };

        let now = shared
            .machine
            .clock_millis_fn
            .as_ref()
            .map(|c| c())
            .unwrap_or_else(|| js_sys::Date::now() as u64);

        let delay_ms = deadline.saturating_sub(now) as f64;

        let shared_weak = Rc::downgrade(shared_rc);
        let func = Closure::once_into_js(move || {
            if let Some(shared_upgrade) = shared_weak.upgrade() {
                Actor::on_host_timeout(&shared_upgrade);
            }
        });
        let delay_val = JsValue::from_f64(delay_ms);

        if let Ok(handle) = set_timeout.call2(&JsValue::NULL, &func, &delay_val) {
            shared.active_timeout = Some(ActiveTimeout {
                handle,
            });
        }
    }

    fn on_host_timeout(shared_rc: &Rc<RefCell<Option<SharedActorState>>>) {
        // Step 1: Pre-clear active_timeout handle so exiting source state doesn't cancel it
        {
            let mut borrow = shared_rc.borrow_mut();
            if let Some(shared) = borrow.as_mut() {
                shared.active_timeout = None;
            }
        }

        // Step 2: Create temp actor and tick
        let mut temp_actor = {
            let borrow = shared_rc.borrow();
            let Some(shared) = borrow.as_ref() else {
                return;
            };
            if shared.is_stopped
                || shared.machine.status == lavi::MachineStatus::Stopped
                || shared.machine.status == lavi::MachineStatus::Done
                || shared.machine.is_done()
            {
                return;
            }
            Actor {
                machine: shared.machine.clone(),
                inspector: shared.inspector.clone(),
                guards: shared.guards.clone(),
                actions: shared.actions.clone(),
                is_stopped: shared.is_stopped,
                config: JsValue::NULL,
                is_initialized: shared.is_initialized,
                shared_state: shared_rc.clone(),
                tick_override: shared.tick_override.clone(),
                logger: shared.logger.clone(),
                logger_holder: shared.logger_holder.clone(),
                input_js: shared.input_js.clone(),
                initialization_error: shared.initialization_error.clone(),
                action_descriptors: shared.action_descriptors.clone(),
            }
        };

        let next_deadline = {
            let borrow = shared_rc.borrow();
            borrow.as_ref().and_then(|s| s.machine.next_timer_deadline())
        };
        if let Some(dl) = next_deadline {
            temp_actor.tick_override.set(Some(dl));
        }

        let _ = temp_actor.tick();

        let last_err = {
            let mut borrow = shared_rc.borrow_mut();
            borrow.as_mut().and_then(|s| s.machine.last_error.take())
        };
        if let Some(err_msg) = last_err {
            let js_err = js_sys::Error::new(&err_msg);
            temp_actor.emit_error(&js_err.into());
        }

        // Step 3: Reschedule for next deadline if any
        Actor::reschedule_host_timer(shared_rc);
    }
}

impl Actor {
    pub fn subscribe<T: Into<JsValue>>(&self, observer_or_fn: T) -> JsValue {
        self.subscribe_internal(observer_or_fn.into())
            .unwrap_or(JsValue::UNDEFINED)
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
    pub(crate) output: JsValue,
    pub(crate) error: JsValue,
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
        if self.status == "error" {
            return false;
        }
        if let Some(path) = test_value.as_string() {
            self.machine_ref.current_state.matches(&path)
        } else if is_valid_state_value_js(&test_value) {
            let val = js_to_value(&test_value);
            let partial = lavi::value_to_state_value(val);
            self.machine_ref.current_state.matches_state_value(&partial)
        } else {
            false
        }
    }

    #[wasm_bindgen]
    pub fn can(&self, event: JsValue) -> bool {
        if self.status == "error" {
            return false;
        }
        let (event_name, event_payload) = if event.is_string() {
            let s = event.as_string().unwrap();
            let payload = Value::Object(lavi::indexmap! {
                "type".to_string() => Value::String(s.clone()),
            });
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
        if self.status == "error" {
            return false;
        }
        self.machine_ref.has_tag(tag)
    }

    #[wasm_bindgen(getter)]
    pub fn tags(&self) -> js_sys::Set {
        if self.status == "error" {
            return js_sys::Set::new(&JsValue::UNDEFINED);
        }
        let set = js_sys::Set::new(&JsValue::UNDEFINED);
        for tag in self.machine_ref.get_tags() {
            set.add(&JsValue::from_str(&tag));
        }
        set
    }

    #[wasm_bindgen(js_name = getMeta)]
    pub fn get_meta(&self) -> JsValue {
        let meta_val = self.machine_ref.get_meta();
        value_to_js(&meta_val)
    }

    #[wasm_bindgen(getter)]
    pub fn error(&self) -> JsValue {
        self.error.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn output(&self) -> JsValue {
        self.output.clone()
    }

    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json(&self) -> JsValue {
        let json_str = self.machine_ref.get_persisted_snapshot_json();
        js_sys::JSON::parse(&json_str).unwrap_or(JsValue::NULL)
    }
}


fn evaluate_guard_in_check(
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
fn execute_enqueue_actions_collect(
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
                    let delay_ms = if delay_val.is_function() {
                        let d_fn: js_sys::Function = delay_val.unchecked_into();
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
                        if let Some(d_fn) = delays_for_enqueue.get(&name) {
                            d_fn(&ctx_for_enqueue.borrow(), &event_for_enqueue)
                        } else {
                            name.parse::<u64>().unwrap_or(0)
                        }
                    } else if let Some(n) = delay_val.as_f64() {
                        n.max(0.0) as u64
                    } else {
                        0
                    };
                    delayed_for_enqueue.borrow_mut().push(lavi::DelayedEvent {
                        id: id_val,
                        deadline: delay_ms,
                        event: raised_val,
                    });
                } else {
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
                    cancel_for_enqueue.borrow_mut().push(send_id);
                }
            }
        } else if let Some(act) = actions_for_enqueue.get(&t_str) {
            if !is_snapshot_mode {
                let params_val = js_sys::Reflect::get(&action_arg, &JsValue::from_str("params")).ok().map(|p| js_to_value(&p));
                let mut ctx_borrow = ctx_for_enqueue.borrow_mut();
                act(&mut ctx_borrow, &event_for_enqueue, &params_val);
            }
        } else if action_arg.is_string() && !is_snapshot_mode {
            let name = action_arg.as_string().unwrap_or_default();
            if let Some(act) = actions_for_enqueue.get(&name) {
                let mut ctx_borrow = ctx_for_enqueue.borrow_mut();
                act(&mut ctx_borrow, &event_for_enqueue, &None);
            }
        } else if action_arg.is_function() && !is_snapshot_mode {
            let func: js_sys::Function = action_arg.unchecked_into();
            let mut ctx_borrow = ctx_for_enqueue.borrow_mut();
            let res = execute_js_action(&func, &mut ctx_borrow, &event_for_enqueue, &None);
            if res.is_object() || res.is_function() {
                let t_val = js_sys::Reflect::get(&res, &JsValue::from_str("type")).unwrap_or(JsValue::UNDEFINED);
                if t_val == JsValue::from_str("xstate.assign") {
                    let a_val = js_sys::Reflect::get(&res, &JsValue::from_str("assignment")).unwrap_or(JsValue::UNDEFINED);
                    if !a_val.is_undefined() {
                        evaluate_assignment_value(&a_val, &mut ctx_borrow, &event_for_enqueue, &None);
                    }
                }
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
    let raise_closure = Closure::wrap(Box::new(move |evt_arg: JsValue| {
        if !is_snapshot_mode {
            effects_for_raise.borrow_mut().push(js_to_value(&evt_arg));
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
}

fn process_action_item(
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
            action_descriptors.insert("xstate.cancel".to_string(), new_obj.clone().into());
            return new_obj.into();
        }

        if type_str == "xstate.raise.invalid" {
            let new_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("type"), &JsValue::from_str("xstate.raise.invalid"));
            let err_val = js_sys::Reflect::get(item, &JsValue::from_str("error")).unwrap_or(JsValue::UNDEFINED);
            let _ = js_sys::Reflect::set(&new_obj, &JsValue::from_str("error"), &err_val);
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
            action_descriptors.insert(id.clone(), new_obj.clone().into());
            return new_obj.into();
        }


        if item.is_function() {
            *counter += 1;
            let id = format!("auto_action_{}_{}", if path.is_empty() { "root" } else { path }, counter);
            let func: js_sys::Function = item.clone().unchecked_into();
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
            action_descriptors.insert(id.clone(), new_obj.clone().into());
            return new_obj.into();
        }

        if !type_str.is_empty() {
            action_descriptors.insert(type_str.clone(), item.clone());
        }
    }
    item.clone()
}

fn process_guard_value(
    guard_val: &JsValue,
    guards: &mut IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
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
                let processed_child = process_guard_value(&child, guards, counter, guards_holder);
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
            process_guard_value(&inner, guards, counter, guards_holder)
        } else {
            let guards_key = JsValue::from_str("guards");
            let g_val = js_sys::Reflect::get(guard_val, &guards_key).unwrap_or(JsValue::UNDEFINED);
            if js_sys::Array::is_array(&g_val) {
                let arr = js_sys::Array::from(&g_val);
                if arr.length() > 0 {
                    let child = arr.get(0);
                    process_guard_value(&child, guards, counter, guards_holder)
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
        let replace_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&replace_obj, &JsValue::from_str("type"), &JsValue::from_str(&id));
        return replace_obj.into();
    }

    guard_val.clone()
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
    let config = machine_def.clone();
    let mut inline_guards = IndexMap::new();
    let mut inline_actions = IndexMap::new();
    let mut inline_actions_with_effects = IndexMap::new();
    let mut inline_action_descriptors = IndexMap::new();
    let mut counter = 0;
    let guards_holder = Rc::new(RefCell::new(IndexMap::new()));
    let logger_holder = Rc::new(RefCell::new(None));
    process_config(
        &config,
        &mut inline_guards,
        &mut inline_actions,
        &mut inline_actions_with_effects,
        &mut inline_action_descriptors,
        0,
        &mut counter,
        "",
        &guards_holder,
        &logger_holder,
    );
    let mut runner = Actor::new(config.clone(), options.clone())?;
    runner.logger_holder = logger_holder.clone();
    runner.guards.extend(inline_guards);
    runner.actions.extend(inline_actions);
    runner.action_descriptors.extend(inline_action_descriptors);
    runner.machine.actions_with_effects.extend(inline_actions_with_effects);

    configure_logger(&options, &mut runner);
    parse_and_register_delays(&options, &mut runner.machine);

    let input_val = if options.is_object() {
        js_sys::Reflect::get(&options, &JsValue::from_str("input")).unwrap_or(JsValue::UNDEFINED)
    } else {
        JsValue::UNDEFINED
    };
    if !input_val.is_undefined() {
        runner.input_js = Some(input_val.clone());
        runner.machine.input = Some(js_to_value(&input_val));
    }

    if config.is_object() {
        let context_key = JsValue::from_str("context");
        let context_val = js_sys::Reflect::get(&config, &context_key).unwrap_or(JsValue::UNDEFINED);
        if context_val.is_function() {
            if let Ok(func) = context_val.dyn_into::<js_sys::Function>() {
                let arg_obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("input"), &input_val);
                let self_obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &self_obj);
                match func.call1(&JsValue::NULL, &arg_obj) {
                    Ok(res) => {
                        runner.machine.context = js_to_value(&res);
                    }
                    Err(err) => {
                        runner.initialization_error = Some(err);
                    }
                }
            }
        }
    }

    // Apply common options (inspect, implementations, snapshot)
    if options.is_object() {
        let inspect_key = JsValue::from_str("inspect");
        let inspect_val = js_sys::Reflect::get(&options, &inspect_key).unwrap_or(JsValue::UNDEFINED);
        if inspect_val.is_function() {
             let func: js_sys::Function = inspect_val.dyn_into().unwrap();
             runner.set_inspector(func);
        }

        let context_opt = js_sys::Reflect::get(&options, &JsValue::from_str("context")).unwrap_or(JsValue::UNDEFINED);
        if !context_opt.is_undefined() {
            runner.machine.context = js_to_value(&context_opt);
        }

        parse_implementations(&options, &mut runner.guards, &mut runner.actions, &mut runner.machine.actions_with_effects);
    }

    guards_holder.borrow_mut().extend(runner.guards.clone());

    check_initial_entry_guards(&runner);
    runner.push_shared();

    Ok(runner.into())
}

#[wasm_bindgen(js_name = createMachine)]
pub fn create_machine(config: JsValue, options: Option<JsValue>) -> Result<StateNode, JsValue> {
    let options_val = options.unwrap_or(JsValue::UNDEFINED);
    let config_processed = config.clone();
    let mut inline_guards = IndexMap::new();
    let mut inline_actions = IndexMap::new();
    let mut inline_actions_with_effects = IndexMap::new();
    let mut inline_action_descriptors = IndexMap::new();
    let mut counter = 0;
    let guards_holder = Rc::new(RefCell::new(IndexMap::new()));
    let logger_holder = Rc::new(RefCell::new(None));
    process_config(
        &config_processed,
        &mut inline_guards,
        &mut inline_actions,
        &mut inline_actions_with_effects,
        &mut inline_action_descriptors,
        0,
        &mut counter,
        "",
        &guards_holder,
        &logger_holder,
    );
    let mut runner = Actor::new(config_processed, options_val.clone())?;
    runner.logger_holder = logger_holder.clone();
    runner.guards.extend(inline_guards);
    runner.actions.extend(inline_actions);
    runner.action_descriptors.extend(inline_action_descriptors.clone());
    runner.machine.actions_with_effects.extend(inline_actions_with_effects);

    configure_logger(&options_val, &mut runner);
    parse_and_register_delays(&options_val, &mut runner.machine);
    parse_implementations(&options_val, &mut runner.guards, &mut runner.actions, &mut runner.machine.actions_with_effects);

    guards_holder.borrow_mut().extend(runner.guards.clone());

    Ok(StateNode {
        machine: runner.machine,
        path: "".to_string(),
        guards: runner.guards,
        actions: runner.actions,
        action_descriptors: inline_action_descriptors,
        config: config.clone(),
        logger_holder: logger_holder.clone(),
    })
}

fn evaluate_js_guard(func: &js_sys::Function, context: &Value, event: &Value, guard_params: &Option<Value>) -> bool {
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

fn execute_js_action(func: &js_sys::Function, ctx_mut: &mut Value, evt: &Value, params: &Option<Value>) -> JsValue {
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

fn evaluate_assignment_value(
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
fn process_config(
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
                let processed = process_guard_value(&value, guards, counter, guards_holder);
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
    mut states: &'a mut IndexMap<String, lavi::StateDefinition>,
    path: &str,
) -> Option<&'a mut lavi::StateDefinition> {
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

fn collect_output_resolvers(val: &JsValue, path: &str, resolvers: &mut IndexMap<String, js_sys::Function>) {
    if !val.is_object() || val.is_null() {
        return;
    }
    let obj = js_sys::Object::from(val.clone());
    let output_key = JsValue::from_str("output");
    let out_val = js_sys::Reflect::get(&obj, &output_key).unwrap_or(JsValue::UNDEFINED);
    if !path.is_empty() {
        if let Ok(func) = out_val.dyn_into::<js_sys::Function>() {
            resolvers.insert(path.to_string(), func);
        }
    }
    let states_key = JsValue::from_str("states");
    let states_val = js_sys::Reflect::get(&obj, &states_key).unwrap_or(JsValue::UNDEFINED);
    if states_val.is_object() && !states_val.is_null() {
        let states_obj = js_sys::Object::from(states_val);
        let keys = js_sys::Object::keys(&states_obj);
        for i in 0..keys.length() {
            let k = keys.get(i);
            let k_str = k.as_string().unwrap_or_default();
            let child_val = js_sys::Reflect::get(&states_obj, &k).unwrap_or(JsValue::UNDEFINED);
            let child_path = if path.is_empty() { k_str } else { format!("{}.{}", path, k_str) };
            collect_output_resolvers(&child_val, &child_path, resolvers);
        }
    }
}

#[wasm_bindgen(js_name = getNextSnapshot)]
pub fn get_next_snapshot(machine: JsValue, snapshot: JsValue, event: JsValue) -> Result<JsValue, JsValue> {
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&options, &JsValue::from_str("snapshot"), &snapshot);
    let actor_val = create_actor(machine, Some(options.into()))?;
    let actor_obj = js_sys::Object::from(actor_val);
    let start_fn = js_sys::Reflect::get(&actor_obj, &JsValue::from_str("start"))?;
    if let Ok(func) = start_fn.dyn_into::<js_sys::Function>() {
        let _ = func.call0(&actor_obj)?;
    }
    let send_fn = js_sys::Reflect::get(&actor_obj, &JsValue::from_str("send"))?;
    if let Ok(func) = send_fn.dyn_into::<js_sys::Function>() {
        let _ = func.call1(&actor_obj, &event)?;
    }
    let get_snapshot_fn = js_sys::Reflect::get(&actor_obj, &JsValue::from_str("getSnapshot"))?;
    if let Ok(func) = get_snapshot_fn.dyn_into::<js_sys::Function>() {
        func.call0(&actor_obj)
    } else {
        Ok(JsValue::UNDEFINED)
    }
}

fn resolve_pure_actions(
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

pub(crate) struct MachineSession {
    pub machine: RuntimeMachine,
    pub guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    #[allow(dead_code)]
    pub actions: IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    pub action_descriptors: IndexMap<String, JsValue>,
    pub config: JsValue,
}

fn extract_machine_session(val: &JsValue) -> Result<MachineSession, JsValue> {
    if val.is_object() {
        if let Ok(func_val) = js_sys::Reflect::get(val, &JsValue::from_str("_get_internal_state")) {
            if func_val.is_function() {
                let func = func_val.dyn_into::<js_sys::Function>()?;
                let ptr_num_val = func.call0(val)?;
                let num = js_sys::Number::from(ptr_num_val);
                let ptr = num.value_of() as usize;
                if ptr != 0 {
                    let mut internal = unsafe { Box::from_raw(ptr as *mut InternalState) };
                    internal.machine.guards.extend(internal.guards.clone());
                    internal.machine.actions.extend(internal.actions.clone());
                    let config = js_sys::Reflect::get(val, &JsValue::from_str("config")).unwrap_or(JsValue::UNDEFINED);
                    return Ok(MachineSession {
                        machine: internal.machine,
                        guards: internal.guards,
                        actions: internal.actions,
                        action_descriptors: internal.action_descriptors,
                        config,
                    });
                }
            }
        }
    }
    let node = create_machine(val.clone(), None)?;
    let mut m = node.machine.clone();
    m.guards.extend(node.guards.clone());
    m.actions.extend(node.actions.clone());
    Ok(MachineSession {
        machine: m,
        guards: node.guards.clone(),
        actions: node.actions.clone(),
        action_descriptors: node.action_descriptors.clone(),
        config: node.config.clone(),
    })
}

fn format_and_resolve_action(
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

#[wasm_bindgen(js_name = initialTransition)]
pub fn initial_transition(machine_val: JsValue, options: Option<JsValue>) -> Result<JsValue, JsValue> {
    let options = options.unwrap_or(JsValue::UNDEFINED);
    let session = extract_machine_session(&machine_val)?;

    let input_js = if options.is_object() {
        js_sys::Reflect::get(&options, &JsValue::from_str("input")).unwrap_or(JsValue::UNDEFINED)
    } else {
        JsValue::UNDEFINED
    };
    let input_val = if !input_js.is_undefined() {
        Some(js_to_value(&input_js))
    } else {
        None
    };

    let mut machine_to_run = session.machine.clone();
    if session.config.is_object() {
        let context_key = JsValue::from_str("context");
        let context_val = js_sys::Reflect::get(&session.config, &context_key).unwrap_or(JsValue::UNDEFINED);
        if context_val.is_function() {
            if let Ok(func) = context_val.dyn_into::<js_sys::Function>() {
                let arg_obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("input"), &input_js);
                let self_obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&arg_obj, &JsValue::from_str("self"), &self_obj);
                if let Ok(res) = func.call1(&JsValue::NULL, &arg_obj) {
                    machine_to_run.context = js_to_value(&res);
                }
            }
        }
    }

    let (next_state_val, next_ctx, actions_vec) = machine_to_run.initial_transition_pure(input_val.as_ref());

    let mut ctx_js = value_to_js(&next_ctx);
    let mut actions_js = Vec::new();
    let init_evt = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&init_evt, &JsValue::from_str("type"), &JsValue::from_str("xstate.init"));
    let init_evt_js: JsValue = init_evt.into();

    for act in &actions_vec {
        format_and_resolve_action(
            act,
            &session.action_descriptors,
            &mut ctx_js,
            &init_evt_js,
            &mut actions_js,
        );
    }

    let mut final_machine = machine_to_run;
    final_machine.current_state = next_state_val.clone();
    final_machine.context = js_to_value(&ctx_js);
    final_machine.refresh_status();

    let dummy_logger = Rc::new(RefCell::new(None));
    let history_obj = compute_history_value_obj(
        &final_machine,
        &session.guards,
        &session.actions,
        &session.action_descriptors,
        &session.config,
        &dummy_logger,
    );

    let output_val = if final_machine.is_done() {
        Actor::compute_output_js(&session.config, &final_machine, &ctx_js)
    } else {
        JsValue::UNDEFINED
    };

    let active_paths = final_machine.current_state.get_active_paths();
    let state_val_js = Actor::build_snapshot_state_value(&final_machine, &active_paths);

    let snapshot = Snapshot {
        value: state_val_js,
        context: ctx_js,
        status: if final_machine.is_done() { "done".to_string() } else { "active".to_string() },
        machine_ref: final_machine,
        guards: session.guards.clone(),
        history_value: history_obj,
        config: session.config.clone(),
        output: output_val,
        error: JsValue::UNDEFINED,
    };

    let result = js_sys::Array::new();
    result.push(&snapshot.into());
    let actions_arr = js_sys::Array::new();
    for a in actions_js {
        actions_arr.push(&a);
    }
    result.push(&actions_arr);
    Ok(result.into())
}

#[wasm_bindgen]
pub fn transition(machine_val: JsValue, state_val: JsValue, event_val: JsValue) -> Result<JsValue, JsValue> {
    let session = extract_machine_session(&machine_val)?;

    let (current_state_val, current_ctx_val, incoming_history) = if state_val.is_object() {
        let val_prop = js_sys::Reflect::get(&state_val, &JsValue::from_str("value")).unwrap_or(JsValue::UNDEFINED);
        let ctx_prop = js_sys::Reflect::get(&state_val, &JsValue::from_str("context")).unwrap_or(JsValue::UNDEFINED);
        let hist_prop = js_sys::Reflect::get(&state_val, &JsValue::from_str("historyValue"))
            .ok()
            .filter(|v| !v.is_undefined() && !v.is_null())
            .or_else(|| {
                js_sys::Reflect::get(&state_val, &JsValue::from_str("history"))
                    .ok()
                    .filter(|v| !v.is_undefined() && !v.is_null())
            })
            .unwrap_or(JsValue::UNDEFINED);

        let st_val = if val_prop.is_string() {
            let s = val_prop.as_string().unwrap_or_default();
            lavi::value_to_state_value(Value::String(s))
        } else if val_prop.is_object() {
            let v = js_to_value(&val_prop);
            let parsed = lavi::value_to_state_value(v);
            if matches!(session.machine.definition.state_type, StateType::Parallel) {
                match &parsed {
                    lavi::StateValue::Parallel(_) => parsed,
                    lavi::StateValue::Compound(k, v) => {
                        let mut m = IndexMap::new();
                        m.insert(k.clone(), lavi::StateValue::Compound(k.clone(), v.clone()));
                        lavi::StateValue::Parallel(m)
                    }
                    lavi::StateValue::Atomic(k) => {
                        let mut m = IndexMap::new();
                        m.insert(k.clone(), lavi::StateValue::Atomic(k.clone()));
                        lavi::StateValue::Parallel(m)
                    }
                }
            } else {
                parsed
            }
        } else {
            session.machine.current_state.clone()
        };
        let c_val = if ctx_prop.is_object() && !ctx_prop.is_null() {
            js_to_value(&ctx_prop)
        } else {
            session.machine.context.clone()
        };

        let hist_val = if hist_prop.is_object() && !hist_prop.is_null() {
            if let Ok(json_js) = js_sys::JSON::stringify(&hist_prop) {
                if let Some(json_str) = json_js.as_string() {
                    lavi::Value::from_json(&json_str).unwrap_or(Value::Null)
                } else {
                    js_to_value(&hist_prop)
                }
            } else {
                js_to_value(&hist_prop)
            }
        } else {
            Value::Null
        };
        let parsed_history = session.machine.parse_history_value(&hist_val);

        (st_val, c_val, parsed_history)
    } else if let Some(s) = state_val.as_string() {
        (lavi::value_to_state_value(Value::String(s)), session.machine.context.clone(), IndexMap::new())
    } else {
        (session.machine.current_state.clone(), session.machine.context.clone(), IndexMap::new())
    };

    let (evt_type, evt_payload) = if event_val.is_string() {
        let s = event_val.as_string().unwrap_or_default();
        let mut map = IndexMap::new();
        map.insert("type".to_string(), Value::String(s.clone()));
        (s, Value::Object(map))
    } else if event_val.is_object() {
        let t_prop = js_sys::Reflect::get(&event_val, &JsValue::from_str("type")).unwrap_or(JsValue::UNDEFINED);
        let s = t_prop.as_string().unwrap_or_default();
        let payload = js_to_value(&event_val);
        (s, payload)
    } else {
        (String::new(), Value::Object(IndexMap::new()))
    };

    let (next_state_val, next_ctx, actions_vec, updated_history) = session.machine.transition_pure_with_context_and_history(
        &current_state_val,
        &current_ctx_val,
        Some(&incoming_history),
        &evt_type,
        &evt_payload,
    );

    let mut ctx_js = value_to_js(&next_ctx);
    let mut actions_js = Vec::new();
    let evt_js = value_to_js(&evt_payload);

    for act in &actions_vec {
        format_and_resolve_action(
            act,
            &session.action_descriptors,
            &mut ctx_js,
            &evt_js,
            &mut actions_js,
        );
    }

    let mut final_machine = session.machine.clone();
    final_machine.current_state = next_state_val.clone();
    final_machine.context = js_to_value(&ctx_js);
    final_machine.history_value = updated_history;
    final_machine.refresh_status();

    let dummy_logger = Rc::new(RefCell::new(None));
    let history_obj = compute_history_value_obj(
        &final_machine,
        &session.guards,
        &session.actions,
        &session.action_descriptors,
        &session.config,
        &dummy_logger,
    );

    let output_val = if final_machine.is_done() {
        Actor::compute_output_js(&session.config, &final_machine, &ctx_js)
    } else {
        JsValue::UNDEFINED
    };

    let active_paths = final_machine.current_state.get_active_paths();
    let state_val_js = Actor::build_snapshot_state_value(&final_machine, &active_paths);

    let snapshot = Snapshot {
        value: state_val_js,
        context: ctx_js,
        status: if final_machine.is_done() { "done".to_string() } else { "active".to_string() },
        machine_ref: final_machine,
        guards: session.guards.clone(),
        history_value: history_obj,
        config: session.config.clone(),
        output: output_val,
        error: JsValue::UNDEFINED,
    };

    let result = js_sys::Array::new();
    result.push(&snapshot.into());
    let actions_arr = js_sys::Array::new();
    for a in actions_js {
        actions_arr.push(&a);
    }
    result.push(&actions_arr);
    Ok(result.into())
}
