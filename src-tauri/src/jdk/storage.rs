use crate::jdk::{error::JdkError, JavaConfig};
use std::{fs, path::PathBuf};

fn config_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".elytra")
}

fn config_file() -> PathBuf {
    config_dir().join("jdk.json")
}

pub fn read() -> JavaConfig {
    let path = config_file();
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        _ => return JavaConfig::default(),
    };
    serde_json::from_str(&content).unwrap_or_default()
}

pub fn save(config: &JavaConfig) -> Result<(), JdkError> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| JdkError::IoError(e.to_string()))?;
    let json =
        serde_json::to_string_pretty(&config).map_err(|e| JdkError::IoError(e.to_string()))?;
    fs::write(config_file(), &json).map_err(|e| JdkError::IoError(e.to_string()))?;
    Ok(())
}
