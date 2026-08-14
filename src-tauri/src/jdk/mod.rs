pub mod download;
pub mod model;
pub mod scanner;

use std::path::{Path, PathBuf};

use crate::constants::SECT_JDK;
use crate::persist;

pub fn read() -> model::JdkConfig {
    persist::read(SECT_JDK).unwrap_or_default()
}

pub fn save(config: &model::JdkConfig) -> Result<(), String> {
    persist::save(SECT_JDK, config).map_err(|e| e.to_string())
}

/// 启动时后台扫描：已有缓存则复用；扫描为空则不动配置。
/// 只在 lib.rs 的 spawn 线程里调，命令里禁止调用。
pub fn init() {
    let mut cfg = read();
    if !cfg.discovered_jdks.is_empty() {
        return;
    }
    let found = scanner::auto_scan();
    if found.is_empty() {
        return;
    }
    cfg.discovered_jdks = found;
    let _ = save(&cfg);
}

/// JAVA_HOME 下 java 可执行文件的路径（bin/java(.exe)）。
/// 扫描与启动共用，避免平台判定散落两处。
pub fn java_bin_path(home: &Path) -> PathBuf {
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    home.join("bin").join(exe)
}

/// 从版本字符串解析主版本号。
/// 支持格式："17.0.8" → 17, "21" → 21, "25-ea" → 25, "1.8.0_202" → 8
pub fn parse_java_major(version: &str) -> Option<u32> {
    // "1.x" 格式（Java 8 及更早，如 "1.8.0_202" → 8）
    if let Some(v) = version.strip_prefix("1.") {
        return v.split('.').next()?.parse::<u32>().ok();
    }
    // 新格式："17.0.8" → 17
    version
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|s| s.parse::<u32>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_java_major() {
        assert_eq!(parse_java_major("1.8.0_202"), Some(8));
        assert_eq!(parse_java_major("1.7.0"), Some(7));
        assert_eq!(parse_java_major("1.6.0"), Some(6));
        assert_eq!(parse_java_major("17.0.8"), Some(17));
        assert_eq!(parse_java_major("21"), Some(21));
        assert_eq!(parse_java_major("25-ea"), Some(25));
        assert_eq!(parse_java_major("11.0.2"), Some(11));
    }
}
