mod store;

use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug)]
pub enum PersistError {
    NotFound,
    Io(String),
    Serde(String),
}

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistError::NotFound => write!(f, "config section not found"),
            PersistError::Io(e) => write!(f, "IO error: {}", e),
            PersistError::Serde(e) => write!(f, "serialization error: {}", e),
        }
    }
}

pub fn read<T: DeserializeOwned>(key: &str) -> Result<T, PersistError> {
    let content = store::read_raw().map_err(PersistError::Io)?;
    if content.is_empty() {
        return Err(PersistError::NotFound);
    }
    let root: toml::Table =
        toml::from_str(&content).map_err(|e| PersistError::Serde(e.to_string()))?;
    let value = root.get(key).ok_or(PersistError::NotFound)?;
    let section =
        toml::to_string(value).map_err(|e| PersistError::Serde(e.to_string()))?;
    toml::from_str(&section).map_err(|e| PersistError::Serde(e.to_string()))
}

pub fn save<T: Serialize>(key: &str, data: &T) -> Result<(), PersistError> {
    let content = store::read_raw().unwrap_or_default();
    let mut root: toml::Table =
        toml::from_str(&content).unwrap_or_default();

    let value =
        toml::Value::try_from(data).map_err(|e| PersistError::Serde(e.to_string()))?;
    root.insert(key.to_string(), value);

    let output =
        toml::to_string_pretty(&root).map_err(|e| PersistError::Serde(e.to_string()))?;
    store::write_raw(&output).map_err(PersistError::Io)
}
