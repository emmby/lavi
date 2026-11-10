use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use lavi::{IndexMap, IndexMapExt, RuntimeMachine, StateType, Value};

use crate::actions::{
    configure_logger, format_and_resolve_action, parse_and_register_delays, parse_implementations,
};
use crate::actor::{check_initial_entry_guards, create_actor, Actor, ActorInner, Snapshot};
use crate::value_bridge::{js_to_value, value_to_js};

thread_local! {
    static CAPTURED_STATE_NODE: RefCell<Option<StateNode>> = const { RefCell::new(None) };
}

pub(crate) fn try_clone_state_node(val: &JsValue) -> Option<StateNode> {
    if !val.is_object() || val.is_null() {
        return None;
    }
    let func_val = js_sys::Reflect::get(val, &JsValue::from_str("__capture_state_node")).ok()?;
    let func = func_val.dyn_into::<js_sys::Function>().ok()?;
    let _ = func.call0(val).ok()?;
    CAPTURED_STATE_NODE.with(|slot| slot.borrow_mut().take())
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct StateNode {
    pub(crate) machine: RuntimeMachine,
    pub(crate) path: String,
    pub(crate) guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    pub(crate) guards_holder: Rc<RefCell<IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>>>,
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

    #[wasm_bindgen(js_name = __capture_state_node)]
    pub fn __capture_state_node(&self) {
        CAPTURED_STATE_NODE.with(|slot| {
            *slot.borrow_mut() = Some(self.clone());
        });
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
                            guards_holder: Rc::new(RefCell::new(guards.clone())),
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
         parse_implementations(
             &implementations,
             &mut new_machine.guards,
             &mut new_machine.actions,
             &mut new_machine.machine.actions_with_effects,
             &mut new_machine.action_descriptors,
             &new_machine.logger_holder,
         );
         new_machine
             .guards_holder
             .borrow_mut()
             .extend(new_machine.guards.clone());
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
            guards_holder: self.guards_holder.clone(),
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
pub(crate) fn map_state_map(
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

pub(crate) fn serialize_state_node_definition(
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

pub(crate) fn serialize_root_machine_definition(
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
         let (
             mut runtime_machine,
             mut guards,
             guards_holder,
             mut actions,
             mut action_descriptors,
             logger_holder,
         ) = {
             let inner = runner.inner.borrow();
             (
                 inner.machine.clone(),
                 inner.guards.clone(),
                 inner.guards_holder.clone(),
                 inner.actions.clone(),
                 inner.action_descriptors.clone(),
                 inner.logger_holder.clone(),
             )
         };
         let schemas_key = JsValue::from_str("schemas");
         let schemas_val = js_sys::Reflect::get(&self.config, &schemas_key).unwrap_or(JsValue::UNDEFINED);
         if !schemas_val.is_undefined() {
             let schemas = js_to_value(&schemas_val);
             if schemas != Value::Null {
                 // Overwrite types with schemas (or merge if needed, but usually one source)
                 Rc::make_mut(&mut runtime_machine.definition).types = Some(schemas);
             }
         }

         // Parse config
         parse_implementations(
             &self.config,
             &mut guards,
             &mut actions,
             &mut runtime_machine.actions_with_effects,
             &mut action_descriptors,
             &logger_holder,
         );
         guards_holder.borrow_mut().extend(guards.clone());
         parse_and_register_delays(&self.config, &mut runtime_machine);
         let machine = runtime_machine;

         Ok(StateNode {
             machine,
             path: "".to_string(),
             guards,
             guards_holder,
             actions,
             action_descriptors,
             config: val,
             logger_holder,
         })
    }
}

#[wasm_bindgen(js_name = setup)]
pub fn setup_wasm_machine(config: JsValue) -> SetupObject {
    SetupObject { config }
}


#[wasm_bindgen]
impl StateNode {
    #[wasm_bindgen(js_name = createActor)]
    pub fn create_actor_method(&self, options: JsValue) -> Result<Actor, JsValue> {
         let tick_override = Rc::new(core::cell::Cell::new(None));
         let session_id = crate::actor::next_session_id();
         let custom_id = if options.is_object() && !options.is_null() {
             js_sys::Reflect::get(&options, &JsValue::from_str("id"))
                 .ok()
                 .and_then(|v| v.as_string())
         } else {
             None
         };
         let mut inner = ActorInner {
            machine: self.machine.clone(),
            inspector: None,
            inspectors: Vec::new(),
            subscribers: Vec::new(),
            guards: self.guards.clone(),
            guards_holder: self.guards_holder.clone(),
            actions: self.actions.clone(),
            action_descriptors: self.action_descriptors.clone(),
            config: self.config.clone(),
            is_stopped: false,
            is_initialized: false,
            is_in_tick: false,
            tick_override: tick_override.clone(),
            host_clock: None,
            is_simulated_clock: false,
            active_timeout: None,
            cached_output: None,
            is_completed_emitted: false,
            logger: None,
            logger_holder: self.logger_holder.clone(),
            input_js: None,
            initialization_error: None,
            session_id,
            custom_id,
        };
        inner.sync_tracked_entry_exit(&self.config);

        let override_cell = tick_override;
        inner.machine.register_clock(Rc::new(move || {
            override_cell.get().unwrap_or_else(|| js_sys::Date::now() as u64)
        }));
        inner.machine.register_warn_fn(Rc::new(|msg| {
            web_sys::console::warn_1(&JsValue::from_str(msg));
        }));

        configure_logger(&options, &mut inner);
        parse_and_register_delays(&options, &mut inner.machine);

        let input_val = if options.is_object() {
            js_sys::Reflect::get(&options, &JsValue::from_str("input")).unwrap_or(JsValue::UNDEFINED)
        } else {
            JsValue::UNDEFINED
        };
        if !input_val.is_undefined() {
            inner.input_js = Some(input_val.clone());
            inner.machine.input = Some(js_to_value(&input_val));
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
                             inner.machine.context = js_to_value(&res);
                         }
                         Err(err) => {
                             inner.initialization_error = Some(err);
                         }
                     }
                 }
             }
        }

        let mut initial_inspect_val = JsValue::UNDEFINED;
        // Apply options
        if options.is_object() {
             let inspect_key = JsValue::from_str("inspect");
             let inspect_val = js_sys::Reflect::get(&options, &inspect_key).unwrap_or(JsValue::UNDEFINED);
             if !inspect_val.is_undefined() && !inspect_val.is_null() {
                  initial_inspect_val = inspect_val;
             }
             let context_opt = js_sys::Reflect::get(&options, &JsValue::from_str("context")).unwrap_or(JsValue::UNDEFINED);
             if !context_opt.is_undefined() {
                 inner.machine.context = js_to_value(&context_opt);
             }
             inner.restore_snapshot_option(&options)?;
             parse_implementations(
                 &options,
                 &mut inner.guards,
                 &mut inner.actions,
                 &mut inner.machine.actions_with_effects,
                 &mut inner.action_descriptors,
                 &inner.logger_holder,
             );
             inner.guards_holder.borrow_mut().extend(inner.guards.clone());
        }

        check_initial_entry_guards(&inner);
        let runner = Actor {
            inner: Rc::new(RefCell::new(inner)),
        };
        if options.is_object() {
            runner.apply_clock_option(&options);
        }
        if !initial_inspect_val.is_undefined() {
            let _ = runner.add_inspector(initial_inspect_val, true);
        }
        Ok(runner)
    }
}

#[wasm_bindgen(js_name = createMachine)]
pub fn create_machine(config: JsValue, options: Option<JsValue>) -> Result<StateNode, JsValue> {
    let options_val = options.unwrap_or(JsValue::UNDEFINED);
    let runner = Actor::new(config.clone(), options_val)?;
    let inner = runner.inner.borrow();

    Ok(StateNode {
        machine: inner.machine.clone(),
        path: "".to_string(),
        guards: inner.guards.clone(),
        guards_holder: inner.guards_holder.clone(),
        actions: inner.actions.clone(),
        action_descriptors: inner.action_descriptors.clone(),
        config,
        logger_holder: inner.logger_holder.clone(),
    })
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

pub(crate) struct MachineSession {
    pub machine: RuntimeMachine,
    pub guards: IndexMap<String, Rc<dyn Fn(&Value, &Value, &Option<Value>) -> bool>>,
    #[allow(dead_code)]
    pub actions: IndexMap<String, Rc<dyn Fn(&mut Value, &Value, &Option<Value>) -> ()>>,
    pub action_descriptors: IndexMap<String, JsValue>,
    pub config: JsValue,
}

pub(crate) fn extract_machine_session(val: &JsValue) -> Result<MachineSession, JsValue> {
    if let Some(mut state_node) = try_clone_state_node(val) {
        state_node.machine.guards.extend(state_node.guards.clone());
        state_node.machine.actions.extend(state_node.actions.clone());
        return Ok(MachineSession {
            machine: state_node.machine,
            guards: state_node.guards,
            actions: state_node.actions,
            action_descriptors: state_node.action_descriptors,
            config: state_node.config,
        });
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
