use std::path::PathBuf;
use std::fs;

use crate::constants::{CONFIG_DIR, SETTINGS_FILE};

fn config_dir() -> PathBuf {
    crate::persist::exe_dir().join(CONFIG_DIR)
}

fn config_file() -> PathBuf {
    config_dir().join(SETTINGS_FILE)
}

pub fn read_raw() -> Result<String, String> {
    fs::read_to_string(config_file()).map_err(|e| format!("读取配置失败\n{e}"))
}

pub fn write_raw(content: &str) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败\n{e}"))?;
    fs::write(config_file(), content).map_err(|e| format!("保存配置失败\n{e}"))
}
