use serde::{Deserialize, Serialize};
use indexmap::IndexMap;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;

// --- DTO Definition (Schema) ---
// These structs exactly mirror the JSON input

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MachineDefinitionDto {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub initial: Option<String>,
    #[serde(default)]
    pub states: IndexMap<String, StateDefinitionDto>,
    #[serde(default, deserialize_with = "deserialize_map_vec_or_single")]
    pub on: IndexMap<String, Vec<TransitionDefinitionDto>>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub entry: Vec<ActionDto>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub exit: Vec<ActionDto>,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    #[serde(default)]
    pub types: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum ActionDto {
    Named(String),
    Assign(AssignActionDto),
    Generic(GenericActionDto),
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct AssignActionDto {
    #[serde(rename = "type")]
    pub action_type: String, // Expect "xstate.assign"
    pub assignment: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct GenericActionDto {
    #[serde(rename = "type")]
    pub action_type: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct GuardObjectDto {
    #[serde(rename = "type")]
    pub guard_type: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum TransitionGuardDto {
    Name(String),
    Object(GuardObjectDto),
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct TransitionObjectDto {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "guard")]
    pub cond: Option<TransitionGuardDto>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub actions: Vec<ActionDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum TransitionDefinitionDto {
    Target(String),
    Object(TransitionObjectDto),
}


#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct StateDefinitionDto {
    #[serde(default, rename = "type")]
    pub type_: Option<String>,
    #[serde(default)]
    pub initial: Option<String>,
    #[serde(default)]
    pub states: IndexMap<String, StateDefinitionDto>,

    #[serde(default, deserialize_with = "deserialize_map_vec_or_single")]
    pub on: IndexMap<String, Vec<TransitionDefinitionDto>>, // Event -> List of Transitions
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub entry: Vec<ActionDto>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub exit: Vec<ActionDto>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub always: Vec<TransitionDefinitionDto>,
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

fn deserialize_map_vec_or_single<'de, D, T>(deserializer: D) -> Result<IndexMap<String, Vec<T>>, D::Error>
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

    let map: IndexMap<String, Option<VecOrSingle<T>>> = IndexMap::deserialize(deserializer)?;
    
    let mut result = IndexMap::new();
    for (key, value) in map {
        let vec = match value {
            Some(VecOrSingle::Vec(v)) => v,
            Some(VecOrSingle::Single(s)) => vec![s],
            None => vec![],
        };
        result.insert(key, vec);
    }
    Ok(result)
}
