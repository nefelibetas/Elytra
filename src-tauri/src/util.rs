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

/// FNV-1a 64 位散列（跨进程稳定，非加密）：离线皮肤哈希 / 缓存键派生共用。
pub fn fnv1a64(data: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// 文件/目录名净化：Windows 非法字符（`<>:"/\|?*`）与控制字符替换 `_`，
/// 去首尾空白与尾点；结果为空或命中 Windows 保留名（CON/NUL/COM1…）时回退 `fallback`。
pub fn sanitize_fs_name(name: &str, fallback: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
                || c.is_control()
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    let no_tail_dot = trimmed.strip_suffix('.').unwrap_or(trimmed);
    if no_tail_dot.is_empty() || no_tail_dot.chars().all(|c| c == '_') {
        return fallback.to_string();
    }
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(no_tail_dot)) {
        return fallback.to_string();
    }
    no_tail_dot.to_string()
}

/// 子目录名冲突消解：`parent/{base}` 已存在则追加 `_{n}`。
pub fn unique_child_dir(parent: &Path, base: &str) -> String {
    if !parent.join(base).exists() {
        return base.to_string();
    }
    for n in 1.. {
        let candidate = format!("{base}_{n}");
        if !parent.join(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!()
}

/// Modrinth CDN 的 MCIM 国内镜像候选列表（官方优先）；
/// 非 cdn.modrinth.com URL 仅返回官方一源。社区资源下载统一经此构造多源。
pub fn mcim_mirror_urls(url: &str) -> Vec<String> {
    let official = url.to_string();
    let mirrored = official.replace("https://cdn.modrinth.com", "https://mod.mcimirror.top");
    if mirrored == official {
        vec![official]
    } else {
        vec![official, mirrored]
    }
}

/// 魔数嗅探受支持的图片格式（png/jpeg/webp/gif）——前端 data URL 渲染与
/// 自定义图标落盘共用同一判定，两侧语义必须一致。
pub fn is_supported_image(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return true; // png
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return true; // jpeg
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return true; // webp
    }
    bytes.starts_with(b"GIF8") // gif
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

/// 魔数嗅探图片 mime 类型（与 is_supported_image 同一判定集；未知返回 None）。
pub fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("image/gif");
    }
    None
}

/// 字节 → base64（图标 IPC 用：JSON 数字数组膨胀 3~5 倍，base64 仅 +33%）。
pub fn to_base64(bytes: &[u8]) -> String {
    use base64::{engine, Engine as _};
    engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests_icon {
    use super::*;

    #[test]
    fn image_mime_sniffs_supported_formats() {
        assert_eq!(image_mime(b"\x89PNG\r\n\x1a\n"), Some("image/png"));
        assert_eq!(image_mime(&[0xff, 0xd8, 0xff, 0xe0]), Some("image/jpeg"));
        let webp = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec();
        assert_eq!(image_mime(&webp), Some("image/webp"));
        assert_eq!(image_mime(b"GIF89a"), Some("image/gif"));
        assert_eq!(image_mime(b"<html>"), None);
    }

    #[test]
    fn to_base64_matches_known_vectors() {
        assert_eq!(to_base64(b""), "");
        assert_eq!(to_base64(b"f"), "Zg==");
        assert_eq!(to_base64(b"fo"), "Zm8=");
        assert_eq!(to_base64(b"foo"), "Zm9v");
        assert_eq!(to_base64(b"foobar"), "Zm9vYmFy");
    }
}

/// 图标 IPC 载荷：mime 由魔数嗅探，data 为 base64。
/// （替代 JSON 数字数组——后者每个字节膨胀 3~5 倍，大图标拖慢 IPC。）
#[derive(Debug, Clone, serde::Serialize)]
pub struct IconData {
    pub mime: String,
    pub data: String,
}

/// 读取图片文件为图标载荷；超限/空文件/非支持格式返回 None（fail-closed）。
pub fn icon_payload(path: &Path, max_bytes: u64) -> Option<IconData> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > max_bytes {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let mime = image_mime(&bytes)?;
    Some(IconData {
        mime: mime.to_string(),
        data: to_base64(&bytes),
    })
}
