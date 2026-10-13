//! JDK 配置（`jdk` section）与扫描缓存（`jdk_scanned` section）的读写，集中在 jdk/ 内。

use crate::constants::{SECT_JDK, SECT_JDK_SCANNED};
use crate::persist;

use super::model::{JdkInfo, JdkOptions, JdkScanCache};
use super::scanner;

/// 支持自动安装的 Java 主版本（须与 download::component_for_major 的映射键一致）。
pub const SUPPORTED_JAVA_MAJORS: [u32; 4] = [8, 17, 21, 25];

pub fn read() -> JdkOptions {
    persist::read(SECT_JDK).unwrap_or_default()
}

pub fn save(options: &JdkOptions) -> Result<(), String> {
    persist::save(SECT_JDK, options).map_err(common_err_ctx!("保存配置失败"))
}

pub fn read_cache() -> JdkScanCache {
    persist::read(SECT_JDK_SCANNED).unwrap_or_default()
}

pub fn save_cache(cache: &JdkScanCache) -> Result<(), String> {
    persist::save(SECT_JDK_SCANNED, cache).map_err(common_err_ctx!("保存配置失败"))
}

/// 将扫描/安装发现的 JDK 去重注册进缓存（启动器自动扫描与下载安装共用）。
pub fn register_scanned(found: Vec<JdkInfo>) {
    if found.is_empty() {
        return;
    }
    let mut cache = read_cache();
    for env in found {
        if !cache.jdks.iter().any(|e| e.path == env.path) {
            cache.jdks.push(env);
        }
    }
    if let Err(e) = save_cache(&cache) {
        eprintln!("[jdk] 扫描结果持久化失败: {e}");
    }
}

/// 启动后台扫描：已有缓存则复用，扫描为空则不动缓存；只在 lib.rs spawn 线程调，命令里禁止。
pub fn init() {
    let mut cache = read_cache();
    if !cache.jdks.is_empty() {
        return;
    }
    let found = scanner::auto_scan();
    if found.is_empty() {
        return;
    }
    cache.jdks = found;
    if let Err(e) = save_cache(&cache) {
        eprintln!("[jdk] 扫描结果持久化失败: {e}");
    }
}
