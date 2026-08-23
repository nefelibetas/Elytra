use std::fs;
use std::path::PathBuf;

use crate::constants::{CONFIG_DIR, SETTINGS_FILE};
use crate::util::exe_dir;

fn config_dir() -> PathBuf {
    exe_dir().join(CONFIG_DIR)
}

fn config_file() -> PathBuf {
    config_dir().join(SETTINGS_FILE)
}

pub(crate) fn read_raw() -> Result<String, String> {
    fs::read_to_string(config_file()).map_err(|e| format!("读取配置失败\n{e}"))
}

pub(crate) fn write_raw(content: &str) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败\n{e}"))?;
    let target = config_file();
    // 原子写：先写临时文件再 rename 替换——直接 fs::write 中断（断电/崩溃）
    // 会留下半截 TOML，下次读取解析失败被 unwrap_or_default 静默清空全部配置
    let tmp = dir.join(format!("{SETTINGS_FILE}.tmp"));
    fs::write(&tmp, content).map_err(|e| format!("保存配置失败\n{e}"))?;
    fs::rename(&tmp, &target).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("保存配置失败\n{e}")
    })
}
