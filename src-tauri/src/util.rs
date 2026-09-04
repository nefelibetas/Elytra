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

/// 文件最后修改时间（unix 秒），读取失败回退 0（不存在/无权限/时钟异常均不致命）。
/// version.rs / mod_updates(identify,scan) 三处共用，消除重复的 metadata 时间链。
pub fn modified_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
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

/// 从版本目录名解析 ModLoader：(loader_key, loader_version)。
/// `26.2-fabric-0.16.9` → Some(("fabric", "0.16.9"))；vanilla → None；
/// 冲突重命名 `_n` 后缀先剥离。optifine 也识别（导出 mrpack 时忽略——mrpack
/// dependencies 无 OptiFine 标准键，文件本体已在 overrides 里）。
pub fn loader_from_dir(dir_name: &str) -> Option<(&str, &str)> {
    const LOADER_KEYS: &[&str] = &["fabric", "forge", "neoforge", "quilt", "optifine"];
    let base = dir_name.split('_').next().unwrap_or(dir_name);
    for key in LOADER_KEYS {
        let marker = format!("-{key}-");
        if let Some(idx) = base.find(&marker) {
            if idx > 0 {
                return Some((key, &base[idx + marker.len()..]));
            }
        }
    }
    None
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

/// 配置目录（Settings.toml / secrets.bin / tasks.db 的基准）。
/// - Windows/Linux：exe 旁 `Elytra/`（与历史布局一致）
/// - macOS：`~/Library/Application Support/Elytra`——current_exe 指向 app bundle 内部
///   （`Contents/MacOS/`），写入会破坏签名且 /Applications 下不可写，配置会丢失
pub fn config_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home)
                .join("Library/Application Support")
                .join(crate::constants::CONFIG_DIR);
        }
    }
    exe_dir().join(crate::constants::CONFIG_DIR)
}

/// 启动器可执行文件所在目录（配置文件与默认游戏目录的基准）。
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
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

/// 简单文件名/目录名判定（防路径穿越）：仅接受无路径分隔符、无 `..` 段的非空名。
/// 供版本/存档/资源/图标等「读路径由调用方拼接」的命令前置校验统一使用。
pub fn is_simple_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && !name.contains("..")
}

/// 目录条目枚举（文件或子目录，取决于 want_dir）：返回 (名称, 字节大小, 修改时间秒)。
/// 目录不存在/不可读返回空 Vec（调用方多为列表类命令，静默空列表语义）。
pub fn dir_entries(dir: &Path, want_dir: bool) -> Vec<(String, u64, u64)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir() == want_dir)
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some((
                e.file_name().to_string_lossy().to_string(),
                meta.len(),
                modified_secs(&meta),
            ))
        })
        .collect()
}

/// 文件/目录名净化：Windows 非法字符（`<>:"/\|?*`）与控制字符替换 `_`，
/// 去首尾空白与尾点；结果为空或命中 Windows 保留名（CON/NUL/COM1…）时回退 `fallback`。
pub fn sanitize_fs_name(name: &str, fallback: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
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

/// 社区文件下载候选：Modrinth 官方 CDN → MCIM 换域名镜像。
/// 按 community_source_mode 组织：OfficialOnly 仅官方 / ChinaOnly 仅镜像 /
/// Auto 下载前探测两源 TTFB（downloader/probe.rs，TTL 10min），快者优先、
/// 慢者兜底（探测失败不剔除，仅排序——下载期另有重试降级）。
/// 非 cdn.modrinth.com URL 仅返回官方一源。社区资源文件下载统一经此构造多源。
pub async fn community_download_urls(url: &str) -> Vec<String> {
    let official = url.to_string();
    let mirrored = official.replace(
        crate::downloader::endpoints::CommunityEndpoints::MODRINTH_CDN_BASE,
        crate::downloader::endpoints::CommunityEndpoints::MODRINTH_CDN_MIRROR_BASE,
    );
    if mirrored == official {
        return vec![official];
    }
    use crate::config::downloader::model::{DownloadSourceMode, DownloaderConfig};
    let cfg: DownloaderConfig =
        crate::persist::read(crate::constants::SECT_DOWNLOADER).unwrap_or_default();
    match cfg.community_source_mode {
        DownloadSourceMode::OfficialOnly => vec![official],
        DownloadSourceMode::ChinaOnly => vec![mirrored],
        DownloadSourceMode::Auto => {
            let o = crate::downloader::probe::probe_url(&official).await;
            let m = crate::downloader::probe::probe_url(&mirrored).await;
            if prefer_mirror(m.ok, m.ttfb_ms, o.ttfb_ms) {
                vec![mirrored, official]
            } else {
                vec![official, mirrored]
            }
        }
    }
}

/// 镜像优先决策（纯逻辑，便于单测）：镜像探测可用且快于官方 → 镜像优先；
/// 镜像失败/更慢 → 官方优先（现状默认）。探测失败不剔除候选，仅排序。
fn prefer_mirror(mirror_ok: bool, mirror_ttfb_ms: u64, official_ttfb_ms: u64) -> bool {
    mirror_ok && mirror_ttfb_ms < official_ttfb_ms
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

/// 默认游戏目录（.minecraft）的基准目录。
/// - Windows：exe 旁（便携语义，与历史一致）
/// - macOS：`~/Library/Application Support`——app bundle 内部只读且破坏签名，游戏数据必须外置
/// - Linux：exe 旁可写则 exe 旁（便携 tar.gz 场景），否则 `~`（deb/AppImage 打包场景，/usr、挂载点只读）
pub fn default_game_dir_base() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join("Library/Application Support");
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Linux/其他 Unix：exe 旁可写则便携，否则（打包只读）回退用户主目录
        if exe_dir_is_writable() {
            return exe_dir();
        }
    }
    // Windows：便携语义，exe 旁（与历史一致）
    #[cfg(target_os = "windows")]
    return exe_dir();
    // macOS 无 HOME 兜底 / Linux exe 目录不可写：退回用户主目录
    #[cfg(not(target_os = "windows"))]
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(exe_dir)
}

/// exe 所在目录是否可写（便携 vs 打包判断，仅 Unix）。
/// 不实际创建文件：读目录 metadata 的权限位（owner/group/other 任一有写位即视为可写）。
#[cfg(all(unix, not(target_os = "macos")))]
fn exe_dir_is_writable() -> bool {
    use std::os::unix::fs::PermissionsExt;
    let dir = exe_dir();
    let Ok(meta) = std::fs::metadata(&dir) else {
        return false;
    };
    meta.is_dir() && meta.permissions().mode() & 0o222 != 0
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

    /// 社区文件源排序决策：镜像快于官方 → 镜像优先；镜像失败/更慢 → 官方优先。
    #[test]
    fn prefer_mirror_ttfb_decision() {
        // 镜像更快 → 镜像优先
        assert!(prefer_mirror(true, 80, 400));
        // 镜像探测失败 → 官方（镜像候选保留作兜底，仅排序不剔除）
        assert!(!prefer_mirror(false, 0, 400));
        // 镜像更慢 → 官方
        assert!(!prefer_mirror(true, 500, 100));
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
/// 启动器日志落盘：把进程 stdout/stderr 重定向到 exe 旁 Elytra/launcher.log（追加）。
/// 启动器为 GUI 进程（Windows 无控制台），eprintln 输出无人可见，落盘后设置页可查看。
/// 注意：file 句柄必须保活（mem::forget），否则 Drop 关闭句柄后被系统复用，重定向失效。
pub fn init_file_logging() {
    let path = exe_dir()
        .join(crate::constants::CONFIG_DIR)
        .join("launcher.log");
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::io::AsRawHandle;
        let handle = file.as_raw_handle();
        unsafe {
            extern "system" {
                fn SetStdHandle(n_std_handle: u32, h_handle: *mut std::ffi::c_void) -> i32;
            }
            const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
            const STD_ERROR_HANDLE: u32 = -12i32 as u32;
            SetStdHandle(STD_OUTPUT_HANDLE, handle);
            SetStdHandle(STD_ERROR_HANDLE, handle);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // POSIX：dup2 到 stdout/stderr（std::os::unix::io::AsRawFd + libc::dup2）
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = file.as_raw_fd();
            unsafe {
                libc::dup2(fd, 1);
                libc::dup2(fd, 2);
            }
        }
    }
    std::mem::forget(file);
}
