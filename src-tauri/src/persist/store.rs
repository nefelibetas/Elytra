use std::path::PathBuf;
use std::fs;

fn config_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Elytra")
}

fn config_file() -> PathBuf {
    config_dir().join("Settings.toml")
}

pub fn read_raw() -> Result<String, String> {
    fs::read_to_string(config_file()).map_err(|e| e.to_string())
}

pub fn write_raw(content: &str) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(config_file(), content).map_err(|e| e.to_string())
}
