use crate::config::jdk::model::JavaConfig;
use crate::config::jdk::scanner;

pub fn read() -> JavaConfig {
    crate::persist::read("jdk").unwrap_or_default()
}

pub fn save(config: &JavaConfig) -> Result<(), String> {
    crate::persist::save("jdk", config).map_err(|e| e.to_string())
}

/// 启动时调用：已有持久化结果则直接复用，否则后台扫描并保存。
pub fn ensure_scanned() {
    let config = read();
    if !config.discovered_jdks.is_empty() {
        return;
    }
    let found = scanner::auto_scan();
    if found.is_empty() {
        return;
    }
    let mut cfg = config;
    cfg.discovered_jdks = found;
    let _ = save(&cfg);
}

/// 合并新扫描到的 JDK 到持久化配置（按 path 去重）。
pub fn merge_scanned(found: Vec<crate::config::jdk::model::JavaEnv>) {
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
