use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::parser::value_to_state_value;
use crate::runtime::RuntimeMachine;
use crate::types::*;

impl RuntimeMachine {
    pub fn compute_persisted_history_value(&self) -> Value {
        let mut history_map = IndexMap::new();
        let active_paths = self.current_state.get_active_paths();

        for (id, path) in &self.definition.id_map {
            if let Some(def) = self.get_state_definition(path) {
                if matches!(def.state_type, StateType::History { .. }) {
                    let is_deep = def.history == Some(HistoryType::Deep)
                        || matches!(def.state_type, StateType::History { deep: true });
                    let parent_path = if let Some(idx) = path.rfind('.') {
                        &path[..idx]
                    } else {
                        ""
                    };
                    let prefix_len = if parent_path.is_empty() {
                        0
                    } else {
                        parent_path.len() + 1
                    };

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
                        if let Some(stored) = self.history_value.get(parent_path) {
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
                                alloc::format!("{}.{}", parent_path, child)
                            }
                        } else {
                            raw_path
                        };
                        if !targets_to_serialize.contains(&target_path) {
                            targets_to_serialize.push(target_path);
                        }
                    }

                    if !self.history_value.contains_key(parent_path)
                        && def.history_target.is_none()
                    {
                        if let Some(p_def) = self.get_state_definition(parent_path) {
                            if let Some(initial_key) = &p_def.initial {
                                let full_initial = if parent_path.is_empty() {
                                    initial_key.clone()
                                } else {
                                    alloc::format!("{}.{}", parent_path, initial_key)
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
                        let mut entries = Vec::with_capacity(targets_to_serialize.len());
                        for target_path in targets_to_serialize {
                            let target_id = self
                                .definition
                                .id_map
                                .iter()
                                .find(|(k, v)| {
                                    v.as_str() == target_path.as_str() && !k.starts_with('(')
                                })
                                .map(|(k, _)| k.clone())
                                .unwrap_or_else(|| {
                                    self.definition
                                        .id_map
                                        .iter()
                                        .find(|(_, v)| v.as_str() == target_path.as_str())
                                        .map(|(k, _)| k.clone())
                                        .unwrap_or_else(|| {
                                            alloc::format!("{}.{}", self.definition.id, target_path)
                                        })
                                });

                            let mut entry_obj = IndexMap::new();
                            entry_obj.insert("id".to_string(), Value::String(target_id));
                            entries.push(Value::Object(entry_obj));
                        }
                        history_map.insert(id.clone(), Value::Array(entries));
                    }
                }
            }
        }

        Value::Object(history_map)
    }

    pub fn get_persisted_snapshot_value(&self) -> Value {
        let mut map = IndexMap::new();
        map.insert("status".to_string(), Value::String(self.status().as_str().to_string()));
        map.insert("value".to_string(), self.current_state.to_value());
        map.insert("context".to_string(), self.context.clone());
        map.insert("historyValue".to_string(), self.compute_persisted_history_value());
        map.insert(
            "tags".to_string(),
            Value::Array(self.get_tags().into_iter().map(Value::String).collect()),
        );
        map.insert("children".to_string(), Value::Object(IndexMap::new()));
        Value::Object(map)
    }

    pub fn get_persisted_snapshot_json(&self) -> alloc::string::String {
        self.get_persisted_snapshot_value().to_json_string()
    }

    pub fn get_snapshot_json(&self) -> alloc::string::String {
        self.get_persisted_snapshot_json()
    }

    /// Parse a `Value::Object` (e.g. from `historyValue` or `history`) into an `IndexMap<String, StateValue>`.
    /// Resolves target state IDs back to parent paths via `definition.id_map` and merges parallel regions iteratively.
    pub fn parse_history_value(&self, hist_val: &Value) -> IndexMap<String, StateValue> {
        let mut history_value = IndexMap::new();
        if let Some(hist_map) = hist_val.as_object() {
            let mut paths_by_parent: IndexMap<String, Vec<String>> = IndexMap::new();

            for (k, v) in hist_map {
                let parent_path = if let Some(history_node_path) = self.definition.id_map.get(k) {
                    if let Some(idx) = history_node_path.rfind('.') {
                        history_node_path[0..idx].to_string()
                    } else {
                        alloc::string::String::new()
                    }
                } else if let Some(idx) = k.rfind('.') {
                    k[0..idx].to_string()
                } else {
                    k.clone()
                };

                match v {
                    Value::Array(arr) => {
                        for item in arr.iter() {
                            if let Some(id_val) = item
                                .as_object()
                                .and_then(|o| o.get("id"))
                                .and_then(|i| i.as_string())
                            {
                                if let Some(path) = self.definition.id_map.get(id_val) {
                                    let entry = paths_by_parent
                                        .entry(parent_path.clone())
                                        .or_default();
                                    if !entry.contains(path) {
                                        entry.push(path.clone());
                                    }
                                } else if let Some(warn) = &self.warn_fn {
                                    warn(&alloc::format!(
                                        "Could not resolve StateNode for id: {}",
                                        id_val
                                    ));
                                }
                            }
                        }
                    }
                    Value::String(s) => {
                        if !s.is_empty() {
                            let entry = paths_by_parent.entry(parent_path).or_default();
                            if !entry.contains(s) {
                                entry.push(s.clone());
                            }
                        }
                    }
                    Value::Object(_) => {
                        history_value.insert(parent_path, value_to_state_value(v.clone()));
                    }
                    _ => {}
                }
            }

            for (parent_path, paths) in paths_by_parent {
                let leaf_paths: Vec<String> = paths
                    .iter()
                    .filter(|p| {
                        let prefix = alloc::format!("{}.", p);
                        !paths
                            .iter()
                            .any(|other| other.len() > p.len() && other.starts_with(&prefix))
                    })
                    .cloned()
                    .collect();

                let mut merged: Option<StateValue> = None;
                for p in leaf_paths {
                    let mut dummy_actions = Vec::new();
                    let sv =
                        Self::resolve_initial_state(&self.definition, &p, &mut dummy_actions)
                            .unwrap_or_else(|| value_to_state_value(Value::String(p.clone())));
                    merged = Some(match merged {
                        None => sv,
                        Some(acc) => Self::merge_state_value(&acc, &sv, "", &p),
                    });
                }
                if let Some(state_val) = merged {
                    history_value.insert(parent_path, state_val);
                }
            }
        }
        history_value
    }

    pub fn restore_from_snapshot_value_owned(&mut self, val: Value) -> Result<(), &'static str> {
        let mut map = match val.into_object() {
            Some(m) => m,
            None => return Err("Snapshot must be a JSON object"),
        };

        // 1. Status
        if let Some(status_val) = map.swap_remove("status") {
            if let Some(s) = status_val.as_string() {
                match s.as_str() {
                    "stopped" => self.status = MachineStatus::Stopped,
                    "done" => self.status = MachineStatus::Done,
                    "active" => self.status = MachineStatus::Active,
                    _ => {}
                }
            }
        }

        // 2. Value
        if let Some(state_val) = map.swap_remove("value") {
            if !matches!(state_val, Value::Null) {
                self.current_state = value_to_state_value(state_val);
                self.is_initialized = true;
            }
        }

        // 3. Context
        if let Some(ctx_val) = map.swap_remove("context") {
            if !matches!(ctx_val, Value::Null) {
                self.context = ctx_val;
            }
        }

        // 4. History Value
        let history_entry = map.swap_remove("historyValue").or_else(|| map.swap_remove("history"));
        if let Some(h_val) = history_entry {
            self.history_value = self.parse_history_value(&h_val);
        }

        self.revive_timers();
        self.refresh_status();
        if self.status != MachineStatus::Stopped {
            self.is_started = true;
        }
        Ok(())

    }

    pub fn restore_from_snapshot_value(&mut self, val: &Value) -> Result<(), &'static str> {
        self.restore_from_snapshot_value_owned(val.clone())
    }

    pub fn restore_from_snapshot_json(&mut self, json_str: &str) -> Result<(), &'static str> {
        let val = Value::from_json(json_str).map_err(|_| "Invalid JSON for snapshot")?;
        self.restore_from_snapshot_value_owned(val)
    }
}
