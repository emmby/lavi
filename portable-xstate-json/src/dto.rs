use serde::{Deserialize, Serialize};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;

// --- DTO Definition (Schema) ---
// These structs exactly mirror the JSON input

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MachineDefinitionDto {
    pub id: String,
    pub initial: String,
    pub states: BTreeMap<String, StateDefinitionDto>,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum ActionDto {
    Named(String),
    Assign(AssignActionDto),
    Generic(GenericActionDto),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AssignActionDto {
    #[serde(rename = "type")]
    pub action_type: String, // Expect "xstate.assign"
    pub assignment: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GenericActionDto {
    #[serde(rename = "type")]
    pub action_type: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GuardObjectDto {
    #[serde(rename = "type")]
    pub guard_type: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum TransitionGuardDto {
    Name(String),
    Object(GuardObjectDto),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TransitionDefinitionDto {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "guard")]
    pub cond: Option<TransitionGuardDto>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub actions: Vec<ActionDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StateDefinitionDto {
    #[serde(default)]
    pub initial: Option<String>,
    #[serde(default)]
    pub states: BTreeMap<String, StateDefinitionDto>,

    #[serde(default, deserialize_with = "deserialize_map_vec_or_single")]
    pub on: BTreeMap<String, Vec<TransitionDefinitionDto>>, // Event -> List of Transitions
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub entry: Vec<ActionDto>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub exit: Vec<ActionDto>,
}

// Helpers
fn deserialize_vec_or_single<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VecOrSingle<T> {
        Vec(Vec<T>),
        Single(T),
    }

    match VecOrSingle::deserialize(deserializer)? {
        VecOrSingle::Vec(v) => Ok(v),
        VecOrSingle::Single(s) => Ok(vec![s]),
    }
}

fn deserialize_map_vec_or_single<'de, D, T>(deserializer: D) -> Result<BTreeMap<String, Vec<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VecOrSingle<T> {
        Vec(Vec<T>),
        Single(T),
    }

    let map: BTreeMap<String, VecOrSingle<T>> = BTreeMap::deserialize(deserializer)?;
    
    let mut result = BTreeMap::new();
    for (key, value) in map {
        let vec = match value {
            VecOrSingle::Vec(v) => v,
            VecOrSingle::Single(s) => vec![s],
        };
        result.insert(key, vec);
    }
    Ok(result)
}
