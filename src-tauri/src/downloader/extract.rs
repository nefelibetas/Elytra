use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use crate::config::downloader::model::DownloadSource;
use crate::downloader::fetcher;
use crate::downloader::model::{Artifact, ExtractRules, Library, OsRule, Rule};

/// 当前目标 OS（与 matches_rules 保持一致）。
/// 支持 Windows / Linux；macOS 仅保证编译（native classifier 惯例为 osx），不做适配。
#[cfg(target_os = "windows")]
pub const TARGET_OS: &str = "windows";
#[cfg(target_os = "linux")]
pub const TARGET_OS: &str = "linux";
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub const TARGET_OS: &str = "osx";

/// 将 Library 的 Rule 列表匹配当前 OS（allow/disallow + os 匹配）。
/// 下载过滤（downloader/library.rs）与启动 classpath 过滤（launcher/args.rs）共用，
/// 修改必须同时考虑两侧语义。
pub fn matches_rules(rules: &[Rule]) -> bool {
    let mut allowed = false;
    for rule in rules {
        let os_match = match &rule.os {
            Some(OsRule { name: Some(n) }) => n == TARGET_OS,
            _ => true,
        };
        match rule.action.as_str() {
            "allow" if os_match => allowed = true,
            "disallow" if os_match => return false,
            _ => {}
        }
    }
    allowed
}

/// 返回当前 OS 对应的 classifier key。
///
/// - 旧式（`natives` 字段存在）：从 map 中查，如 `"windows" → "natives-windows"`
/// - 新式（无 `natives`）：按约定 `"natives-windows"` / `"natives-linux"` / `"natives-osx"`
pub fn os_classifier_key(natives: Option<&HashMap<String, String>>) -> Option<&str> {
    if let Some(map) = natives {
        map.get(TARGET_OS).map(|s| s.as_str())
    } else {
        match TARGET_OS {
            "windows" => Some("natives-windows"),
            "linux" => Some("natives-linux"),
            "osx" | "macos" => Some("natives-osx"),
            _ => None,
        }
    }
}

/// 取某库 `downloads.classifiers` 中指定 classifier key 的 artifact。
/// 下载侧（process_natives）与启动解压侧（launcher/args.rs ensure_old_style_natives）共用，
/// 避免两侧各自穿过 Option 链取 classifier 的样板重复。
pub fn classifier_artifact<'a>(lib: &'a Library, key: &str) -> Option<&'a Artifact> {
    lib.downloads.as_ref()?.classifiers.as_ref()?.get(key)
}

/// 判断库名中的 native classifier 是否匹配当前 OS + 架构。
///
/// 现代版本中每个架构变体是独立 library，且 rules 只写 `allow windows`，
/// 需要靠库名后缀区分：`natives-windows`（x64）/ `natives-windows-x86` / `natives-windows-arm64`。
/// x64 Windows 只接受无架构后缀的 `natives-windows`。
pub fn native_classifier_matches(lib_name: &str) -> bool {
    let Some(classifier) = lib_name.rsplit(':').next() else {
        return true;
    };
    let Some(rest) = classifier.strip_prefix("natives-") else {
        return true; // 非 native 库，交给 rules 处理
    };
    // rest 形如 "windows"、"windows-arm64"、"linux"、"osx-arm64"
    let (os_part, arch_part) = match rest.split_once('-') {
        Some((os, arch)) => (os, Some(arch)),
        None => (rest, None),
    };
    if os_part != TARGET_OS {
        return false;
    }
    // Windows x64：只接受无后缀的 natives-windows
    if TARGET_OS == "windows" {
        return arch_part.is_none();
    }
    arch_part.is_none()
}

/// 下载所有匹配当前 OS 的 native classifier JAR 到 `libraries/`。
///
/// 旧式（有 `natives` 字段，lwjgl 2.x）和新式（无 `natives`，LWJGL 3.x）
/// 都只下载 JAR 到 libraries/，不在 `.minecraft` 里创建 natives 目录——
/// 与 PCL2/官方启动器一致：旧式 natives 在启动时解压到系统临时目录，
/// 新式由 LWJGL 运行时自行解压。
pub async fn process_natives(
    libraries: &[Library],
    libraries_dir: &Path,
    sources: &[&DownloadSource],
    counter: Option<Arc<AtomicU32>>,
    bytes_counter: Option<Arc<AtomicU64>>,
) -> Result<(), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    for lib in libraries {
        let Some(classifier_key) = os_classifier_key(lib.natives.as_ref()) else { continue };
        let Some(artifact) = classifier_artifact(lib, classifier_key) else { continue };

        let dest = libraries_dir.join(&artifact.path);
        if dest.exists() {
            continue;
        }
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                errors.push(format!("{}: 创建目录失败: {}", lib.name, e));
                continue;
            }
        }
        // 每个源各构造一个 URL，下载时主源过慢/失败自动降级
        let urls: Vec<String> = sources.iter().map(|s| s.mirror_url(&artifact.url)).collect();
        match fetcher::fetch_fallback(&urls, &dest, None::<fn(u64, u64)>).await {
            Ok(dl) => {
                if let Some(c) = counter.as_ref() {
                    c.fetch_add(1, Ordering::Relaxed);
                }
                if let Some(b) = bytes_counter.as_ref() {
                    b.fetch_add(dl.bytes, Ordering::SeqCst);
                }
            }
            Err(e) => errors.push(format!("{}: {}", lib.name, e)),
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// 将 JAR（ZIP）解压到 `out_dir`，跳过 `extract.exclude` 中的路径。
/// 公开给启动命令构建使用（旧式 natives 运行时解压到临时目录）。
pub fn extract_jar(
    jar_path: &Path,
    out_dir: &Path,
    extract: Option<&ExtractRules>,
) -> Result<(), String> {
    let file =
        std::fs::File::open(jar_path).map_err(|e| format!("打开 JAR 失败: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("读取 JAR 失败: {}", e))?;

    let excludes: Vec<&str> = extract
        .map(|e| e.exclude.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取 ZIP 条目失败: {}", e))?;

        let name = entry.name().to_string();

        // 跳过排除路径
        if excludes.iter().any(|e| name.starts_with(e) || name == *e) {
            continue;
        }

        // 跳过目录（由文件创建时自动生成）
        if entry.is_dir() {
            continue;
        }

        let target = out_dir.join(&name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建目录 {} 失败: {}", parent.display(), e))?;
        }

        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("读取 {} 失败: {}", name, e))?;

        std::fs::write(&target, &buf)
            .map_err(|e| format!("写入 {} 失败: {}", target.display(), e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_classifier_matches() {
        // Windows x64：只接受无后缀的 natives-windows
        assert!(native_classifier_matches("org.lwjgl:lwjgl-freetype:3.4.1:natives-windows"));
        assert!(!native_classifier_matches("org.lwjgl:lwjgl-freetype:3.4.1:natives-windows-x86"));
        assert!(!native_classifier_matches("org.lwjgl:lwjgl-freetype:3.4.1:natives-windows-arm64"));

        // 非当前 OS
        assert!(!native_classifier_matches("org.lwjgl:lwjgl-freetype:3.4.1:natives-linux"));
        assert!(!native_classifier_matches("org.lwjgl:lwjgl-freetype:3.4.1:natives-macos"));

        // 普通库（无 natives 后缀）不受影响
        assert!(native_classifier_matches("org.lwjgl:lwjgl-freetype:3.4.1"));
        assert!(native_classifier_matches("com.google.guava:guava:33.0.0-jre"));

        // 无冒号分隔的库名
        assert!(native_classifier_matches("some-library"));
    }
}
