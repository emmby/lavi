
use serde::{Deserialize, Serialize};
use indexmap::IndexMap;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;
use alloc::string::ToString;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct MachineDefinitionDto {
    pub id: Option<String>,
    #[serde(default, rename = "type")]
    pub type_: Option<String>,
    pub initial: Option<serde_json::Value>,
    #[serde(default)]
    pub states: IndexMap<String, StateDefinitionDto>,
    #[serde(default, deserialize_with = "deserialize_map_vec_or_single")]
    pub on: IndexMap<String, Vec<TransitionDefinitionDto>>,
    #[serde(default, deserialize_with = "deserialize_after")]
    pub after: IndexMap<String, Vec<TransitionDefinitionDto>>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub entry: Vec<ActionDto>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub exit: Vec<ActionDto>,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub invoke: Vec<InvokeDefinitionDto>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum ActionDto {
    Name(String),
    Object {
        #[serde(rename = "type")]
        action_type: String,
        #[serde(default)]
        assignment: Option<serde_json::Value>,
        #[serde(default)]
        params: Option<serde_json::Value>,
    }
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
    pub target: Option<serde_json::Value>,
    #[serde(default, alias = "guard")]
    pub cond: Option<TransitionGuardDto>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub actions: Vec<ActionDto>,
    #[serde(default)]
    pub reenter: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum TransitionDefinitionDto {
    Target(String),
    Object(TransitionObjectDto),
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct InvokeDefinitionDto {
    pub src: serde_json::Value,
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct StateDefinitionDto {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "type")]
    pub type_: Option<String>,
    #[serde(default)]
    pub initial: Option<serde_json::Value>,
    #[serde(default)]
    pub states: IndexMap<String, StateDefinitionDto>,

    #[serde(default, deserialize_with = "deserialize_map_vec_or_single")]
    pub on: IndexMap<String, Vec<TransitionDefinitionDto>>,
    #[serde(default, deserialize_with = "deserialize_after")]
    pub after: IndexMap<String, Vec<TransitionDefinitionDto>>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub entry: Vec<ActionDto>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub exit: Vec<ActionDto>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub always: Vec<TransitionDefinitionDto>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub invoke: Vec<InvokeDefinitionDto>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub history: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub target: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_single")]
    pub actions: Vec<ActionDto>,
}

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
        Null,
    }

    match VecOrSingle::deserialize(deserializer)? {
        VecOrSingle::Vec(v) => Ok(v),
        VecOrSingle::Single(s) => Ok(vec![s]),
        VecOrSingle::Null => Ok(vec![]),
    }
}

fn deserialize_map_vec_or_single<'de, D, T>(deserializer: D) -> Result<IndexMap<String, Vec<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum MapVecOrSingle {
        Map(IndexMap<String, serde_json::Value>),
        Null,
    }

    match MapVecOrSingle::deserialize(deserializer)? {
        MapVecOrSingle::Map(m) => {
            let mut result = IndexMap::new();
            for (k, v) in m {
                let transitions = match v {
                    serde_json::Value::Array(arr) => {
                        arr.into_iter().map(|item| serde_json::from_value(item).unwrap()).collect()
                    }
                    serde_json::Value::String(s) => vec![serde_json::from_value(serde_json::Value::String(s)).unwrap()],
                    serde_json::Value::Object(_) => vec![serde_json::from_value(v).unwrap()],
                    _ => vec![],
                };
                result.insert(k, transitions);
            }
            Ok(result)
        }
        MapVecOrSingle::Null => Ok(IndexMap::new()),
    }
}

fn deserialize_after<'de, D>(deserializer: D) -> Result<IndexMap<String, Vec<TransitionDefinitionDto>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum AfterFormat {
        Map(IndexMap<String, serde_json::Value>),
        Array(Vec<serde_json::Value>),
        Null,
    }

    match AfterFormat::deserialize(deserializer)? {
        AfterFormat::Map(m) => {
            let mut result = IndexMap::new();
            for (k, v) in m {
                let transitions = match v {
                    serde_json::Value::Array(arr) => {
                        arr.into_iter().map(|item| serde_json::from_value(item).unwrap_or_else(|_| TransitionDefinitionDto::Target("".to_string()))).collect()
                    }
                    serde_json::Value::String(s) => vec![serde_json::from_value(serde_json::Value::String(s)).unwrap_or_else(|_| TransitionDefinitionDto::Target("".to_string()))],
                    serde_json::Value::Object(_) => vec![serde_json::from_value(v).unwrap_or_else(|_| TransitionDefinitionDto::Target("".to_string()))],
                    _ => vec![],
                };
                result.insert(k, transitions);
            }
            Ok(result)
        }
        AfterFormat::Array(arr) => {
            let mut result: IndexMap<String, Vec<TransitionDefinitionDto>> = IndexMap::new();
            for item in arr {
                if let serde_json::Value::Object(mut map) = item {
                    if let Some(delay_val) = map.remove("delay") {
                        let delay_str = match delay_val {
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::String(s) => s,
                            _ => "".to_string(),
                        };
                        if delay_str.is_empty() { continue; }
                        
                        let transition: TransitionDefinitionDto = serde_json::from_value(serde_json::Value::Object(map))
                            .unwrap_or_else(|_| TransitionDefinitionDto::Target("".to_string()));
                        result.entry(delay_str).or_default().push(transition);
                    }
                }
            }
            Ok(result)
        }
        AfterFormat::Null => Ok(IndexMap::new()),
    }
}
