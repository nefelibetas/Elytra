use std::path::{Path, PathBuf};
// ── 跨域公共小工具（纯函数，无状态）──

use std::future::Future;
use std::sync::LazyLock;

/// 同步执行异步操作（低频阻塞可接受；secret-service 钥匙串 / task DB 复用）。
/// 有 tokio 上下文时复用现有 runtime：
/// - multi_thread runtime：worker 线程直接用 `Handle::block_on` 会 panic
///   （tokio 禁止 runtime 内再启动 runtime），须先 `block_in_place` 让出 worker；
///   外部线程（主线程/blocking 线程）下 `block_in_place` 是 no-op，行为不变。
/// - current_thread runtime：`block_in_place` 不可用，驱动线程直接 `block_on`。
///
/// 无 runtime 上下文（后台线程/独立进程/纯同步测试）时自维护兜底 runtime。
pub fn block_on<F: Future>(fut: F) -> F::Output {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        use tokio::runtime::RuntimeFlavor;
        if handle.runtime_flavor() == RuntimeFlavor::MultiThread {
            tokio::task::block_in_place(move || handle.block_on(fut))
        } else {
            handle.block_on(fut)
        }
    } else {
        static RT: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .expect("tokio runtime")
        });
        RT.block_on(fut)
    }
}

/// URL 路径最后一段（文件名）：`https://a/b/c.jar` → `Some("c.jar")`；空尾段/空串 → None。
pub fn url_file_name(url: &str) -> Option<&str> {
    url.rsplit('/').next().filter(|s| !s.is_empty())
}

/// 从版本目录名提取 MC 版本（与前端 utils/versionClassify 一致）：
/// 找 `-{loader_key}-` 标记取标记前部分；无标记（vanilla）取整个目录名；
/// 冲突重命名 `{id}_n` 后缀一并剥离（`26.2-forge-65.1.0_1` → `26.2`）。
/// natives 目录按 MC 版本共享——同一 MC 的 vanilla/ModLoader 版本 native 库一致。
pub fn mc_version_from_dir(dir_name: &str) -> &str {
    const LOADER_KEYS: &[&str] = &["fabric", "forge", "neoforge", "quilt", "optifine"];
    // 冲突重命名 {id}_n 后缀（与前端 classifyVersion 的 /_\d+$/ 等价）
    let base = dir_name.split('_').next().unwrap_or(dir_name);
    for key in LOADER_KEYS {
        let marker = format!("-{key}-");
        if let Some(idx) = base.find(&marker) {
            if idx > 0 {
                return &base[..idx];
            }
        }
    }
    base
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

/// JAVA_HOME 下 java 可执行文件的路径（bin/java(.exe)）。
/// 扫描与启动共用，避免平台判定散落两处。
pub fn java_bin_path(home: &Path) -> PathBuf {
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    home.join("bin").join(exe)
}

/// 启动器可执行文件所在目录（配置文件与默认游戏目录的基准）。
pub fn exe_dir() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_file_name_extracts_last_segment() {
        assert_eq!(
            url_file_name("https://cdn.example.com/mods/fabric-api-0.102.0.jar"),
            Some("fabric-api-0.102.0.jar")
        );
        assert_eq!(url_file_name("a/b/"), None);
        assert_eq!(url_file_name(""), None);
    }

    #[test]
    fn mc_version_from_dir_all_naming_forms() {
        assert_eq!(mc_version_from_dir("26.2"), "26.2");
        assert_eq!(mc_version_from_dir("26.2-forge-65.1.0"), "26.2");
        assert_eq!(mc_version_from_dir("26.2-neoforge-26.2.0.53-beta"), "26.2");
        assert_eq!(
            mc_version_from_dir("1.20.1-pre1-forge-40.1.0"),
            "1.20.1-pre1"
        );
        assert_eq!(mc_version_from_dir("1.18.2-fabric-0.16.11"), "1.18.2");
        assert_eq!(mc_version_from_dir("1.12.2"), "1.12.2");
        assert_eq!(mc_version_from_dir("26.2-forge-65.1.0_1"), "26.2");
    }

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
