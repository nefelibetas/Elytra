pub mod download;
pub mod model;
pub mod scanner;

/// 支持自动安装的 Java 主版本（须与 download::component_for_major 的映射键一致）。
pub const SUPPORTED_JAVA_MAJORS: [u32; 4] = [8, 17, 21, 25];

use crate::constants::SECT_JDK;
use crate::persist;

// 读取toml中配置
pub fn read() -> model::JdkConfig {
    persist::read(SECT_JDK).unwrap_or_default()
}

// 保存配置到tmol
pub fn save(config: &model::JdkConfig) -> Result<(), String> {
    persist::save(SECT_JDK, config).map_err(|e| format!("保存配置失败\n{e}"))
}

/// 将扫描/安装发现的 JDK 去重注册进 discovered_jdks（启动器自动扫描与下载安装共用）。
pub fn register_scanned(found: Vec<model::JdkInfo>) {
    if found.is_empty() {
        return;
    }
    let mut cfg = read();
    for env in found {
        if !cfg.discovered_jdks.iter().any(|e| e.path == env.path) {
            cfg.discovered_jdks.push(env);
        }
    }
    let _ = save(&cfg);
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
