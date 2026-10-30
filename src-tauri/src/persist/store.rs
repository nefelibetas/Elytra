use crate::constants::SETTINGS_FILE;
use crate::util::config_dir;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

/// Settings.toml 版本戳：长度 + mtime；外部修改/删除用于让解析缓存失效。
pub(crate) type FileStamp = (u64, Option<SystemTime>);

fn config_file() -> PathBuf {
    config_dir().join(SETTINGS_FILE)
}

/// 仅取 metadata，不读文件内容；配合解析结果缓存使用。
pub(crate) fn file_stamp() -> Result<Option<FileStamp>, String> {
    match fs::metadata(config_file()) {
        Ok(meta) => Ok(Some((meta.len(), meta.modified().ok()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(keyed_msg!("errors.config.read", "读取配置失败\n{e}")),
    }
}

#[cfg(test)]
static READ_RAW_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn read_raw_calls() -> usize {
    READ_RAW_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn read_raw() -> Result<Option<String>, String> {
    #[cfg(test)]
    READ_RAW_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    match fs::read_to_string(config_file()) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(keyed_msg!("errors.config.read", "读取配置失败\n{e}")),
    }
}

pub(crate) fn write_raw(content: &str) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(common_err_ctx_key!(
        "errors.file.createDir",
        "创建配置目录失败"
    ))?;
    let target = config_file();
    let tmp = dir.join(format!("{SETTINGS_FILE}.tmp"));
    fs::write(&tmp, content).map_err(common_err_ctx_key!("errors.config.write", "保存配置失败"))?;
    fs::rename(&tmp, &target).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        keyed_msg!("errors.config.write", "保存配置失败\n{e}")
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
