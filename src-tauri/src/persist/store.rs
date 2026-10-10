use crate::constants::SETTINGS_FILE;
use crate::util::config_dir;
use std::fs;
use std::path::PathBuf;

fn config_file() -> PathBuf {
    config_dir().join(SETTINGS_FILE)
}

pub(crate) fn read_raw() -> Result<Option<String>, String> {
    match fs::read_to_string(config_file()) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("读取配置失败\n{e}")),
    }
}

pub(crate) fn write_raw(content: &str) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败\n{e}"))?;
    let target = config_file();
    let tmp = dir.join(format!("{SETTINGS_FILE}.tmp"));
    fs::write(&tmp, content).map_err(|e| format!("保存配置失败\n{e}"))?;
    fs::rename(&tmp, &target).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("保存配置失败\n{e}")
    })
}

pub(crate) fn backup_corrupt() -> Option<PathBuf> {
    let src = config_file();
    if !src.exists() {
        return None;
    }
    let dst = config_dir().join(format!("{}.bak", SETTINGS_FILE));
    fs::rename(&src, &dst).ok().map(|_| dst)
}
