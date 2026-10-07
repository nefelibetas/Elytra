//! 跨域公共小工具（纯函数，无状态）。

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use base64::{engine, Engine as _};
use tokio::runtime::RuntimeFlavor;

#[cfg(target_os = "windows")]
use std::os::windows::io::AsRawHandle;

use crate::config::downloader::{DownloadSourceMode, DownloaderConfig};
use crate::constants;
use crate::downloader;
use crate::persist;

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

/// sha1 前两位（assets/objects 子目录名）。长度不足 2 时回退 `"00"`：
/// hash 来自远程索引或本地 JSON，裸切片 `&hash[..2]` 在畸形输入下会 panic
/// （字节切片还可能落在多字节边界）。
pub fn sha1_sub_dir(hash: &str) -> &str {
    hash.get(..2).unwrap_or("00")
}

/// 相对路径安全校验（路径穿越防护）：拒绝空路径、绝对路径、含 `..` 或空段的输入。
/// 返回校验后的相对路径，调用方自行 `join` 到目标根目录。
/// 用于整合包清单条目名、zip 成员名、JDK runtime 条目名等**外部输入**的落盘路径。
pub fn safe_relative_path(rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim();
    if rel.is_empty() || rel.starts_with('/') || rel.starts_with('\\') {
        return Err(format!("非法文件路径\n{rel:?}"));
    }
    if rel
        .split(['/', '\\'])
        .any(|seg| seg == ".." || seg.is_empty())
    {
        return Err(format!("文件路径含非法段\n{rel:?}"));
    }
    Ok(PathBuf::from(rel))
}

/// loader 标记 key 列表与顺序（与前端 utils/versionClassify 的 LOADER_KEYS 一致；
/// 各标记互不重叠，顺序不影响结果，仅保持同一事实源）。
const LOADER_KEYS: &[&str] = &["neoforge", "forge", "fabric", "quilt", "optifine"];

/// 冲突重命名 `{id}_n` 后缀剥离（与前端 classifyVersion 的 /_\d+$/ 语义一致）：
/// 只剥**尾部**一段 `_<数字>`（`26.2-forge-65.1.0_1` → `26.2-forge-65.1.0`；
/// `x_1_2` → `x_1`；`x_` 不剥）。
fn strip_conflict_suffix(name: &str) -> &str {
    if let Some(i) = name.rfind('_') {
        let tail = &name[i + 1..];
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            return &name[..i];
        }
    }
    name
}

/// 大小写不敏感查找 `-{key}-` loader 标记（标记前必须还有内容，idx > 0）。
/// ASCII 小写化不改变字节长度，返回的 idx 可直接回用于原串切片。
fn find_loader_marker(base: &str) -> Option<(usize, &'static str)> {
    let lower = base.to_ascii_lowercase();
    for key in LOADER_KEYS {
        let marker = format!("-{key}-");
        if let Some(idx) = lower.find(&marker) {
            if idx > 0 {
                return Some((idx, key));
            }
        }
    }
    None
}

/// 从版本目录名提取 MC 版本（与前端 utils/versionClassify 同规则）：
/// 找 `-{loader_key}-` 标记（大小写不敏感）取标记前部分；无标记（vanilla）取整个目录名；
/// 冲突重命名 `_n` 后缀按尾部 `_数字` 剥离一次。
/// **仅限展示与本地键**（natives 共享目录等）；对外表达真实 MC 版本应从版本目录内
/// json 解析（见 commands/version.rs::resolve_version_meta），目录名解析只是兜底。
pub fn mc_version_from_dir(dir_name: &str) -> &str {
    let base = strip_conflict_suffix(dir_name);
    match find_loader_marker(base) {
        Some((idx, _)) => &base[..idx],
        None => base,
    }
}

/// 从版本目录名解析 ModLoader：(loader_key, loader_version)。
/// `26.2-fabric-0.16.9` → Some(("fabric", "0.16.9"))；vanilla → None；
/// 标记匹配大小写不敏感（PCL2 等外部目录名可能大写）。optifine 也识别
/// （导出 mrpack 时忽略——dependencies 无 OptiFine 标准键，文件本体已在 overrides 里）。
pub fn loader_from_dir(dir_name: &str) -> Option<(&str, &str)> {
    let base = strip_conflict_suffix(dir_name);
    let (idx, key) = find_loader_marker(base)?;
    Some((key, &base[idx + key.len() + 2..]))
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

/// 配置目录（Settings.toml / secrets.bin / tasks.db / 日志的基准）。
/// - Windows：exe 旁 `Elytra/`（便携语义，与历史布局一致）
/// - macOS：`~/Library/Application Support/Elytra`——current_exe 指向 app bundle 内部
///   （`Contents/MacOS/`），写入会破坏签名且 /Applications 下不可写，配置必须外置
/// - Linux：`$XDG_DATA_HOME/Elytra`（缺省 `~/.local/share/Elytra`）——exe 位置由
///   包管理器/挂载器决定（/usr 只读、AppImage 挂载点每次不同），数据一律走用户目录
/// - dev/cargo test 进程（exe 位于 cargo target 下）走 exe 旁：测试隔离 + dev 行为不变
pub fn config_dir() -> PathBuf {
    #[cfg(not(target_os = "windows"))]
    {
        if is_dev_runtime() {
            return exe_dir().join(constants::CONFIG_DIR);
        }
        if let Some(root) = platform_data_root() {
            return root.join(constants::CONFIG_DIR);
        }
    }
    exe_dir().join(constants::CONFIG_DIR)
}

/// 平台用户数据根目录（macOS/Linux，config 与默认游戏目录的共同基准）。
/// - macOS：`~/Library/Application Support`
/// - Linux：`$XDG_DATA_HOME` 非空用之，否则 `~/.local/share`（XDG Base Directory）
#[cfg(target_os = "macos")]
fn platform_data_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
}

/// 平台用户数据根目录（macOS/Linux，config 与默认游戏目录的共同基准）。
#[cfg(all(unix, not(target_os = "macos")))]
fn platform_data_root() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        if !x.is_empty() {
            return Some(PathBuf::from(x));
        }
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
}

/// exe 是否位于 cargo target/debug|release 下（dev 与 cargo test 进程）。
/// 此类进程配置走 exe 旁：单元测试进程在 `target/debug/deps/` 下，配置天然落在
/// 该隔离区（不污染真实用户数据目录），`pnpm tauri dev` 行为也与历史一致。
#[cfg(not(target_os = "windows"))]
fn is_dev_runtime() -> bool {
    std::env::current_exe()
        .ok()
        .map(|p| path_under_cargo_target(&p))
        .unwrap_or(false)
}

/// 路径是否形如 `…/target/{debug|release}/…`（纯函数，便于单测）。
#[cfg(any(not(target_os = "windows"), test))]
fn path_under_cargo_target(p: &Path) -> bool {
    let comps: Vec<&str> = p
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    comps
        .windows(2)
        .any(|w| w[0] == "target" && matches!(w[1], "debug" | "release"))
}

/// 启动器可执行文件所在目录。Windows 便携语义锚点（exe 旁数据/游戏目录）；
/// macOS/Linux 的数据布局不依赖 exe 位置（见 config_dir / default_game_dir_base）。
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .or_else(|| {
            // current_exe 解析失败（极端环境）：回退用户主目录，不依赖进程 cwd
            std::env::var_os("HOME").map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 取文本尾部最多 200 行（lines().rev().take(200) 后反转，保留原顺序）。
pub fn tail_lines(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().rev().take(200).collect();
    lines.reverse();
    lines.join("\n")
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
    // 循环为无限 RangeFrom（命中即 return），此处仅为满足类型检查
    unreachable!()
}

/// 社区文件下载候选：Modrinth 官方 CDN → MCIM 换域名镜像。
/// 按 community_source_mode 组织：OfficialOnly 仅官方 / MirrorOnly 仅镜像 /
/// Auto 下载前探测两源 TTFB（downloader/probe.rs，TTL 10min），快者优先、
/// 慢者兜底（探测失败不剔除，仅排序——下载期另有重试降级）。
/// 非 cdn.modrinth.com URL 仅返回官方一源。社区资源文件下载统一经此构造多源。
pub async fn community_download_urls(url: &str) -> Vec<String> {
    let official = url.to_string();
    let mirrored = official.replace(
        downloader::endpoints::CommunityEndpoints::MODRINTH_CDN_BASE,
        downloader::endpoints::CommunityEndpoints::MODRINTH_CDN_MIRROR_BASE,
    );
    if mirrored == official {
        return vec![official];
    }
    let cfg: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    match cfg.community_source_mode {
        DownloadSourceMode::OfficialOnly => vec![official],
        DownloadSourceMode::MirrorOnly => vec![mirrored],
        DownloadSourceMode::Auto => {
            let o = downloader::probe::probe_url(&official).await;
            let m = downloader::probe::probe_url(&mirrored).await;
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
/// - macOS/Linux：应用数据目录（`<数据根>/Elytra`）——默认 .minecraft 由启动器自持，
///   与官方/其它启动器默认位置隔离；需要共享的目录在游戏目录列表里显式添加
pub fn default_game_dir_base() -> PathBuf {
    #[cfg(target_os = "windows")]
    return exe_dir();
    // macOS/Linux：dev/cargo test 进程走 exe 旁（与 config_dir 同规则，测试隔离）；
    // 发布形态 = 应用数据目录（默认游戏目录 = <数据根>/Elytra/.minecraft）
    #[cfg(not(target_os = "windows"))]
    {
        if is_dev_runtime() {
            exe_dir()
        } else if let Some(root) = platform_data_root() {
            root.join(constants::CONFIG_DIR)
        } else if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home)
        } else {
            exe_dir()
        }
    }
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
    fn sha1_sub_dir_never_panics_on_malformed_input() {
        assert_eq!(sha1_sub_dir("abcdef0123456789"), "ab");
        // 长度不足与多字节边界都回退 "00"（裸切片会 panic）
        assert_eq!(sha1_sub_dir(""), "00");
        assert_eq!(sha1_sub_dir("a"), "00");
        assert_eq!(sha1_sub_dir("中文"), "00");
    }

    #[test]
    fn safe_relative_path_blocks_traversal() {
        // 合法相对路径：整合包清单条目、zip 成员、JDK runtime 条目
        assert!(safe_relative_path("mods/a.jar").is_ok());
        assert!(safe_relative_path("config/nested/b.json").is_ok());
        assert!(safe_relative_path("bin/java.exe").is_ok());
        // 绝对路径（两种分隔符）、空路径、`..` 与空段
        assert!(safe_relative_path("/abs").is_err());
        assert!(safe_relative_path("\\abs").is_err());
        assert!(safe_relative_path("").is_err());
        assert!(safe_relative_path("../evil").is_err());
        assert!(safe_relative_path("bin/../../evil").is_err());
        assert!(safe_relative_path("a//b").is_err());
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
        // 与前端 classifyVersion 对齐：标记大小写不敏感（PCL2 等外部目录名大写）
        assert_eq!(mc_version_from_dir("1.20.1-Fabric-0.14.22"), "1.20.1");
        assert_eq!(mc_version_from_dir("1.20.1-FABRIC-0.14.22"), "1.20.1");
        // 尾部 `_数字` 只剥一段（与前端 /_\d+$/ 一致）
        assert_eq!(mc_version_from_dir("1.20.1_1_2"), "1.20.1_1");
        // 任意名无标记：整串兜底（vanilla 语义，仅供展示/本地键）
        assert_eq!(mc_version_from_dir("[abcd]"), "[abcd]");
        assert_eq!(mc_version_from_dir("我的整合包-fabric-1.0"), "我的整合包");
    }

    /// loader 解析与前端同规则（大小写不敏感 + 尾部冲突后缀）；解析器本身信任目录名，
    /// 对外真实语义由版本 json 权威层兜底（commands/version.rs::resolve_version_meta）。
    #[test]
    fn loader_from_dir_parity_forms() {
        assert_eq!(
            loader_from_dir("26.2-fabric-0.16.9"),
            Some(("fabric", "0.16.9"))
        );
        assert_eq!(
            loader_from_dir("1.20.1-Fabric-0.14.22"),
            Some(("fabric", "0.14.22"))
        );
        assert_eq!(
            loader_from_dir("26.2-forge-65.1.0_1"),
            Some(("forge", "65.1.0"))
        );
        assert_eq!(loader_from_dir("26.2"), None);
        assert_eq!(loader_from_dir("[abcd]"), None);
        // 伪标记示例：解析器原样给出，json 权威层会覆盖为真实 loader
        assert_eq!(loader_from_dir("X-fabric-Y"), Some(("fabric", "Y")));
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

    /// cargo 产物路径判定：target/{debug|release} 段命中（deps 测试进程在 debug 下）。
    #[test]
    fn path_under_cargo_target_detects_dev_runtime() {
        assert!(path_under_cargo_target(Path::new(
            "src-tauri/target/debug/Elytra.exe"
        )));
        assert!(path_under_cargo_target(Path::new(
            "src-tauri/target/debug/deps/Elytra-abc.exe"
        )));
        assert!(path_under_cargo_target(Path::new(
            "home/u/elytra/target/release/elytra"
        )));
        assert!(!path_under_cargo_target(Path::new("/usr/bin/elytra")));
        assert!(!path_under_cargo_target(Path::new("target/debuggy/x")));
        assert!(!path_under_cargo_target(Path::new(
            "/tmp/.mount_elytraX/usr/bin/elytra"
        )));
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

    #[test]
    fn infer_loader_variants() {
        assert_eq!(
            infer_loader_from_json(
                r#"{"mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient"}"#
            ),
            "fabric"
        );
        assert_eq!(
            infer_loader_from_json(r#"{"libraries":[{"name":"net.minecraftforge:forge:47.1.0"}]}"#),
            "forge"
        );
        assert_eq!(
            infer_loader_from_json(r#"{"libraries":[{"name":"net.neoforged:neoforge:21.0.0"}]}"#),
            "neoforge"
        );
        // neoforged 含 forge 子串：neoforge 必须在 forge 前匹配
        assert_eq!(
            infer_loader_from_json(r#"{"mainClass":"net.neoforged.bootstrap.BootstrapLauncher"}"#),
            "neoforge"
        );
        assert_eq!(
            infer_loader_from_json(r#"{"mainClass":"net.minecraft.client.main.Main"}"#),
            "vanilla"
        );
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
    let path = config_dir().join("launcher.log");
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    #[cfg(target_os = "windows")]
    {
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

/// 从版本 json 文本推断 loader（轻量字符串检查，无需完整解析）：
/// 特征名匹配优先级 fabric → neoforge → forge → optifine（neoforge 在 forge 前，
/// 避免 "neoforged" 子串误匹配）；无特征视为原版。
pub fn infer_loader_from_json(text: &str) -> String {
    if text.contains("net.fabricmc") {
        "fabric".into()
    } else if text.contains("net.neoforged") {
        "neoforge".into()
    } else if text.contains("net.minecraftforge") {
        "forge".into()
    } else if text.contains("optifine") {
        "optifine".into()
    } else {
        "vanilla".into()
    }
}
