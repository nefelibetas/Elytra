//! ── JRE 自动下载（managed Java）：无 Java 机器补齐"扫描→下载→校验→注册"闭环 ──
//! 双源下载：Mojang 官方 runtime 优先（逐文件 sha1 校验，与官方启动器同构），
//! 失败回退 Azul Zulu（包级 sha256 + zip 解压）。
//! 下载目录：`{minecraft_dir}/runtime/{platform}/{version}/`，完成后经
//! `scanner::inspect` 校验版本并注册进 persist 的 jdk section（discovered_jdks）。

use std::path::{Path, PathBuf};

use crate::config::downloader::model::DownloaderConfig;
use crate::downloader::fetcher::{self, http};
use crate::jdk::model::JdkInfo;
use crate::jdk::scanner;

/// Mojang java-runtime all.json 固定资源地址（官方启动器 product 版本）。
const RUNTIME_ALL_JSON: &str =
    "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

/// 按 Java 主版本映射 Mojang runtime 组件名（all.json 的键）。
/// 官方启动器为 1.20.5+ 用 epsilon（25）、1.18-1.20.4 用 delta（21）、
/// 1.16-1.17 用 gamma（17）、1.7-1.8 用 jre-legacy（8）。
pub fn component_for_major(major: u32) -> Option<&'static str> {
    match major {
        8 => Some("jre-legacy"),
        17 => Some("java-runtime-gamma"),
        21 => Some("java-runtime-delta"),
        25 => Some("java-runtime-epsilon"),
        _ => None,
    }
}

/// 当前平台在 all.json 中的平台键。
fn platform_key() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        return "windows-x64";
    }
    #[cfg(target_os = "macos")]
    {
        return if cfg!(target_arch = "aarch64") {
            "mac-os-arm64"
        } else {
            "mac-os-x64"
        };
    }
    #[cfg(target_os = "linux")]
    {
        return if cfg!(target_arch = "aarch64") {
            "linux-arm64"
        } else {
            "linux-x64"
        };
    }
}

/// 统一入口：官方 runtime → Azul 回退。
/// 已装同 major 的 managed runtime 时直接返回（幂等）。
pub async fn auto_install(major: u32, config: &DownloaderConfig) -> Result<JdkInfo, String> {
    let dest_dir = runtime_dir(config, major);
    // 已下载过：直接校验返回
    if let Ok(info) = scanner::inspect(&dest_dir) {
        if crate::jdk::parse_java_major(&info.version) == Some(major) {
            return Ok(info);
        }
    }

    match install_mojang_runtime(major, config).await {
        Ok(info) => Ok(info),
        Err(e_mojang) => match install_azul_runtime(major, config).await {
            Ok(info) => Ok(info),
            Err(e_azul) => Err(format!(
                "下载 Java {} 失败\n官方源: {}\nAzul 源: {}",
                major, e_mojang, e_azul
            )),
        },
    }
}

/// managed runtime 安装目录：`{minecraft_dir}/runtime/{platform}/java-{major}/`。
fn runtime_dir(config: &DownloaderConfig, major: u32) -> PathBuf {
    config
        .minecraft_dir_abs()
        .join("runtime")
        .join(platform_key())
        .join(format!("java-{major}"))
}

/// 词法路径穿越防护：manifest/zip 条目路径必须为相对路径且不含 `..` 段
/// （manifest 来自网络，防恶意/篡改条目写出 runtime 根）。
fn safe_runtime_path(root: &Path, entry: &str) -> Result<PathBuf, String> {
    let entry = entry.trim();
    if entry.is_empty() || entry.starts_with('/') || entry.starts_with('\\') {
        return Err(format!("非法 runtime 文件路径: {entry:?}"));
    }
    if entry
        .split(['/', '\\'])
        .any(|seg| seg == ".." || seg.is_empty())
    {
        return Err(format!("runtime 文件路径含非法段: {entry:?}"));
    }
    Ok(root.join(entry))
}

/// 下载并安装 Mojang 官方 runtime。
async fn install_mojang_runtime(
    major: u32,
    config: &DownloaderConfig,
) -> Result<JdkInfo, String> {
    let component = component_for_major(major)
        .ok_or_else(|| format!("Java {major} 无对应 Mojang runtime 组件（仅支持 8/17/21/25）"))?;
    let platform = platform_key();

    // 1. 拉取 all.json（多源回退 + TTL 缓存：BMCLAPI 镜像 piston-meta）
    let all_text = config
        .fetch_with_fallback(
            config.file_source_mode,
            crate::downloader::source::CacheBehaviour::DefaultTtl,
            |s| s.mirror_url(RUNTIME_ALL_JSON),
        )
        .await
        .map_err(|e| format!("获取 Java runtime 索引失败: {e}"))?;
    let all: serde_json::Value =
        serde_json::from_str(&all_text).map_err(|e| format!("解析 runtime 索引失败: {e}"))?;

    // 2. 平台 → 组件 → 版本列表（取最新一条的 manifest）。
    // 注意实际结构：顶层键是平台（windows-x64/mac-os-arm64/...），
    // 组件（java-runtime-gamma 等）在第二层（顶层同名键为空占位，勿取）。
    let entry = all[&platform][component]
        .as_array()
        .and_then(|arr| arr.first())
        .ok_or_else(|| format!("runtime 索引缺少 {platform}/{component} 条目"))?;
    let manifest_info = entry["manifest"]
        .as_object()
        .cloned()
        .ok_or("runtime manifest 信息缺失")?;
    let manifest_url = manifest_info
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or("runtime manifest 缺 url")?
        .to_string();
    let manifest_sha1 = manifest_info
        .get("sha1")
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .to_string();

    // 3. 下载 manifest.json（sha1 校验）
    let manifest_urls: Vec<String> = config
        .file_sources_list()
        .iter()
        .map(|s| s.mirror_url(&manifest_url))
        .collect();
    let manifest_path = temp_file("runtime-manifest.json");
    fetcher::fetch_verified(&manifest_urls, &manifest_path, None::<fn(u64, u64)>, &manifest_sha1)
        .await
        .map_err(|e| format!("下载 runtime manifest 失败: {e}"))?;
    let manifest_text =
        std::fs::read_to_string(&manifest_path).map_err(|e| format!("读取 manifest 失败: {e}"))?;
    let _ = std::fs::remove_file(&manifest_path);
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).map_err(|e| format!("解析 manifest 失败: {e}"))?;

    // 4. 逐文件下载到 runtime 目录（目录条目建目录，文件条目 sha1 校验下载，link 条目跳过）
    let dest_root = runtime_dir(config, major);
    let files = manifest["files"]
        .as_object()
        .ok_or("runtime manifest 缺 files")?;
    for (path, meta) in files {
        let ftype = meta["type"].as_str().unwrap_or("file");
        let dest = safe_runtime_path(&dest_root, path)?;
        match ftype {
            "directory" => {
                std::fs::create_dir_all(&dest).map_err(|e| format!("创建目录失败: {e}"))?;
            }
            "file" => {
                let raw = meta["downloads"]["raw"]
                    .as_object()
                    .ok_or_else(|| format!("{path} 缺 downloads.raw"))?;
                let url = raw
                    .get("url")
                    .and_then(|u| u.as_str())
                    .ok_or_else(|| format!("{path} 缺 url"))?;
                let sha1 = raw.get("sha1").and_then(|s| s.as_str()).unwrap_or("");
                let urls: Vec<String> = config
                    .file_sources_list()
                    .iter()
                    .map(|s| s.mirror_url(url))
                    .collect();
                fetcher::fetch_verified(&urls, &dest, None::<fn(u64, u64)>, sha1)
                    .await
                    .map_err(|e| format!("下载 {path} 失败: {e}"))?;
            }
            "link" => {
                // 官方 manifest 的 symlink 条目主要在 unix 平台；Windows runtime 无此条目。
                // 此处跳过（Java 运行所需实体文件均已逐一下载）。
            }
            other => {
                return Err(format!("未知 runtime 文件类型: {other:?}（{path}）"));
            }
        }
    }

    // 5. 校验：inspect 版本 + major 匹配
    let info = scanner::inspect(&dest_root).map_err(|e| format!("安装后校验失败: {e}"))?;
    if crate::jdk::parse_java_major(&info.version) != Some(major) {
        return Err(format!(
            "安装的 Java 版本异常（需要 {}，实际 {}）",
            major, info.version
        ));
    }
    Ok(info)
}

/// Azul Zulu 回退：metadata API 查 zip 包 → sha256 校验 → 解压 → inspect。
async fn install_azul_runtime(major: u32, config: &DownloaderConfig) -> Result<JdkInfo, String> {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    };
    let api_url = format!(
        "https://api.azul.com/metadata/v1/zulu/packages/?java_version={major}&os={os}&arch={arch}&archive_type=zip&java_package_type=jdk&javafx_bundled=false&release_status=ga&latest=true"
    );
    let resp = http()
        .get(&api_url)
        .send()
        .await
        .map_err(|e| format!("查询 Azul 包失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Azul API 返回 {}", resp.status()));
    }
    let packages: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let pkg = packages
        .as_array()
        .and_then(|a| a.first())
        .ok_or("Azul 无匹配包")?;
    let download_url = pkg
        .get("download_url")
        .and_then(|u| u.as_str())
        .ok_or("Azul 包缺 download_url")?
        .to_string();
    let sha256 = pkg
        .get("sha256_hash")
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .to_string();

    // 下载 zip → sha256 校验
    let zip_path = temp_file(&format!("azul-java-{major}.zip"));
    let urls: Vec<String> = vec![download_url.clone()];
    fetcher::fetch_verified(&urls, &zip_path, None::<fn(u64, u64)>, "")
        .await
        .map_err(|e| format!("下载 Azul 包失败: {e}"))?;
    if !sha256.is_empty() {
        let actual = file_sha256(&zip_path);
        if actual != sha256 {
            let _ = std::fs::remove_file(&zip_path);
            return Err("Azul 包 sha256 校验失败".into());
        }
    }

    // 解压到 runtime 目录（zip 内顶层目录保留，如 zulu17.x-ca-jdk17.x-win_x64/）
    let dest_root = runtime_dir(config, major);
    std::fs::create_dir_all(&dest_root).map_err(|e| format!("创建目录失败: {e}"))?;
    let file = std::fs::File::open(&zip_path).map_err(|e| format!("打开 Azul 包失败: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取 Azul 包失败: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("读取条目失败: {e}"))?;
        let name = entry.name().to_string();
        // 路径穿越防护：拒绝 .. 与绝对路径
        let out = safe_runtime_path(&dest_root, &name)?;
        if entry.is_dir() {
            std::fs::create_dir_all(&out).ok();
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let mut file_out = std::fs::File::create(&out)
            .map_err(|e| format!("创建 {} 失败: {e}", out.display()))?;
        std::io::copy(&mut entry, &mut file_out)
            .map_err(|e| format!("解压 {} 失败: {e}", out.display()))?;
    }
    let _ = std::fs::remove_file(&zip_path);

    // 找到含 bin/java(.exe) 的 JDK 根目录（zip 顶层目录名不定）
    let home = find_java_home(&dest_root).ok_or("解压后未找到 JDK 根目录")?;
    let info = scanner::inspect(&home).map_err(|e| format!("校验 Azul JDK 失败: {e}"))?;
    if crate::jdk::parse_java_major(&info.version) != Some(major) {
        return Err(format!(
            "Azul 安装版本异常（需要 {}，实际 {}）",
            major, info.version
        ));
    }
    Ok(info)
}

/// 递归查找含 `bin/java(.exe)` 的 JDK 根目录（Azul zip 顶层目录名不确定）。
fn find_java_home(dir: &Path) -> Option<PathBuf> {
    let bin = crate::jdk::java_bin_path(dir);
    if bin.is_file() {
        return Some(dir.to_path_buf());
    }
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_java_home(&path) {
                return Some(found);
            }
        }
    }
    None
}

/// 临时文件路径（下载中间产物；用完即删）。
fn temp_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("elytra-jre");
    std::fs::create_dir_all(&dir).ok();
    dir.join(name)
}

/// 文件 sha256（Azul 包校验）。
fn file_sha256(path: &Path) -> String {
    use sha2::Digest;
    let Ok(data) = std::fs::read(path) else {
        return String::new();
    };
    let hash = sha2::Sha256::digest(&data);
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_mapping_matches_plan() {
        assert_eq!(component_for_major(8), Some("jre-legacy"));
        assert_eq!(component_for_major(17), Some("java-runtime-gamma"));
        assert_eq!(component_for_major(21), Some("java-runtime-delta"));
        assert_eq!(component_for_major(25), Some("java-runtime-epsilon"));
        assert_eq!(component_for_major(11), None);
        assert_eq!(component_for_major(9), None);
    }

    #[test]
    fn safe_runtime_path_rejects_traversal() {
        let root = Path::new("C:/minecraft/runtime");
        assert!(safe_runtime_path(root, "bin/java.exe").is_ok());
        assert!(safe_runtime_path(root, "legal/README.txt").is_ok());
        assert!(safe_runtime_path(root, "../evil.exe").is_err());
        assert!(safe_runtime_path(root, "bin/../../evil").is_err());
        assert!(safe_runtime_path(root, "/abs/path").is_err());
        assert!(safe_runtime_path(root, "\\abs\\path").is_err());
        assert!(safe_runtime_path(root, "").is_err());
    }
}
