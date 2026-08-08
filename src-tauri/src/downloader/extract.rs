use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use crate::config::downloader::model::DownloadSource;
use crate::downloader::fetcher;
use crate::downloader::hash::file_sha1_matches;
use crate::downloader::model::{Artifact, ExtractRules, Library, OsRule, Rule};

/// natives 代际：区分旧式（classifiers 解压）与新式（库名后缀独立变体）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativesKind {
    /// 旧式：`natives` 字段 + `downloads.classifiers`（lwjgl 2.x 及 LWJGL 3 早期）
    Legacy,
    /// 新式：架构变体独立库，库名带 `:natives-{os}` 后缀（1.13+）
    Modern,
}

impl Library {
    /// 统一入口：判定 natives 代际；普通库返回 None。
    /// 先查库名后缀（新式），再查 natives 字段（旧式），两代互斥。
    pub fn natives_kind(&self) -> Option<NativesKind> {
        if self
            .name
            .rsplit(':')
            .next()
            .is_some_and(|c| c.starts_with("natives-"))
        {
            Some(NativesKind::Modern)
        } else if self.natives.is_some() {
            Some(NativesKind::Legacy)
        } else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
pub const TARGET_OS: &str = "windows";
#[cfg(target_os = "linux")]
pub const TARGET_OS: &str = "linux";
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub const TARGET_OS: &str = "osx";

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

/// 旧式：查表取当前平台的 natives classifier key（lwjgl 2.x 的 `natives` 字段映射）。
fn legacy_natives_key(natives: &HashMap<String, String>) -> Option<&str> {
    natives.get(TARGET_OS).map(|s| s.as_str())
}

/// 新式：当前平台的约定 classifier key（"natives-windows" / "natives-linux" / "natives-osx"）。
pub fn modern_natives_key() -> Option<String> {
    match TARGET_OS {
        "windows" => Some("natives-windows".into()),
        "linux" => Some("natives-linux".into()),
        "osx" | "macos" => Some("natives-osx".into()),
        _ => None,
    }
}

/// 旧式：取某库当前平台的 natives classifier artifact（查 natives map → downloads.classifiers）。
/// 下载侧（process_natives）与启动解压侧（args.rs ensure_old_style_natives）共用。
pub fn legacy_classifier_artifact(lib: &Library) -> Option<&Artifact> {
    let key = legacy_natives_key(lib.natives.as_ref()?)?;
    classifier_artifact(lib, key)
}

/// 取某库 `downloads.classifiers` 中指定 classifier key 的 artifact。
fn classifier_artifact<'a>(lib: &'a Library, key: &str) -> Option<&'a Artifact> {
    lib.downloads.as_ref()?.classifiers.as_ref()?.get(key)
}

/// 新式：判断库名中的 native classifier 是否匹配当前 OS + 架构。
///
/// 现代版本中每个架构变体是独立 library，且 rules 只写 `allow windows`，
/// 需要靠库名后缀区分：`natives-windows`（x64）/ `natives-windows-x86` / `natives-windows-arm64`。
/// x64 Windows 只接受无架构后缀的 `natives-windows`。
pub fn modern_native_matches(lib_name: &str) -> bool {
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
    counter: Arc<AtomicU32>,
    bytes_counter: Arc<AtomicU64>,
    step_bytes: Arc<AtomicU64>,
) -> Result<(), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    for lib in libraries {
        // 仅旧式（natives 字段 + classifiers）参与；新式 natives 作为普通库由 should_download 覆盖
        let Some(artifact) = legacy_classifier_artifact(lib) else {
            continue;
        };

        let dest = libraries_dir.join(&artifact.path);
        if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) {
            continue;
        }
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                errors.push(format!("{}: 创建目录失败: {}", lib.name, e));
                continue;
            }
        }
        // 每个源各构造一个 URL，下载时主源过慢/失败自动降级
        let urls: Vec<String> = sources
            .iter()
            .map(|s| s.mirror_url(&artifact.url))
            .collect();
        // 字节级进度：增量累加进全局与 natives 步骤计数（重试时进度从 0 重新开始，增量不会为负）
        let file_done = Arc::new(AtomicU64::new(0));
        let progress = {
            let fd = file_done.clone();
            let bc = bytes_counter.clone();
            let sb = step_bytes.clone();
            move |downloaded: u64, _total: u64| {
                let prev = fd.swap(downloaded, Ordering::Relaxed);
                let delta = downloaded.saturating_sub(prev);
                bc.fetch_add(delta, Ordering::Relaxed);
                sb.fetch_add(delta, Ordering::Relaxed);
            }
        };
        match fetcher::fetch_fallback(&urls, &dest, Some(progress)).await {
            Ok(_dl) => {
                // 下载后 sha1 校验（镜像源可能返回坏内容）：不匹配删除并报错
                if !file_sha1_matches(&dest, &artifact.sha1) {
                    let _ = std::fs::remove_file(&dest);
                    errors.push(format!("{}: sha1 校验失败", lib.name));
                    continue;
                }
                counter.fetch_add(1, Ordering::Relaxed);
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
    let file = std::fs::File::open(jar_path).map_err(|e| format!("打开 JAR 失败: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取 JAR 失败: {}", e))?;

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
    fn test_modern_native_matches() {
        // Windows x64：只接受无后缀的 natives-windows
        assert!(modern_native_matches(
            "org.lwjgl:lwjgl-freetype:3.4.1:natives-windows"
        ));
        assert!(!modern_native_matches(
            "org.lwjgl:lwjgl-freetype:3.4.1:natives-windows-x86"
        ));
        assert!(!modern_native_matches(
            "org.lwjgl:lwjgl-freetype:3.4.1:natives-windows-arm64"
        ));

        // 非当前 OS
        assert!(!modern_native_matches(
            "org.lwjgl:lwjgl-freetype:3.4.1:natives-linux"
        ));
        assert!(!modern_native_matches(
            "org.lwjgl:lwjgl-freetype:3.4.1:natives-macos"
        ));

        // 普通库（无 natives 后缀）不受影响
        assert!(modern_native_matches("org.lwjgl:lwjgl-freetype:3.4.1"));
        assert!(modern_native_matches("com.google.guava:guava:33.0.0-jre"));

        // 无冒号分隔的库名
        assert!(modern_native_matches("some-library"));
    }

    #[test]
    fn test_legacy_classifier_artifact_windows() {
        use crate::downloader::model::LibraryDownloads;
        use std::collections::HashMap;

        let lib = Library {
            name: "org.lwjgl.lwjgl:lwjgl-platform:2.9.4".into(),
            downloads: Some(LibraryDownloads {
                artifact: None,
                classifiers: Some(HashMap::from([(
                    "natives-windows".to_string(),
                    Artifact {
                        path: "org/lwjgl/lwjgl-platform-2.9.4-natives-windows.jar".into(),
                        url: "https://libraries.minecraft.net/...".into(),
                        sha1: "abc".into(),
                        size: 613748,
                    },
                )])),
            }),
            rules: None,
            natives: Some(HashMap::from([
                ("windows".into(), "natives-windows".into()),
                ("osx".into(), "natives-osx".into()),
                ("linux".into(), "natives-linux".into()),
            ])),
            extract: None,
        };
        let artifact = legacy_classifier_artifact(&lib).expect("Windows 应命中 natives-windows");
        assert_eq!(
            artifact.path,
            "org/lwjgl/lwjgl-platform-2.9.4-natives-windows.jar"
        );
    }

    #[test]
    fn test_natives_kind_legacy_modern_plain() {
        use std::collections::HashMap;

        // 旧式：natives 字段（1.12.2 lwjgl-platform 2.9.4 形态）
        let legacy = Library {
            name: "org.lwjgl.lwjgl:lwjgl-platform:2.9.4-nightly-20150209".into(),
            downloads: None,
            rules: None,
            natives: Some(HashMap::from([("windows".into(), "natives-windows".into())])),
            extract: None,
        };
        assert_eq!(legacy.natives_kind(), Some(NativesKind::Legacy));

        // 新式：库名后缀（26.2 lwjgl 3.4.1 形态）
        let modern = Library {
            name: "org.lwjgl:lwjgl:3.4.1:natives-windows".into(),
            downloads: None,
            rules: None,
            natives: None,
            extract: None,
        };
        assert_eq!(modern.natives_kind(), Some(NativesKind::Modern));

        // 普通库：无 natives 字段、无后缀
        let plain = Library {
            name: "com.azure:azure-json:1.4.0".into(),
            downloads: None,
            rules: None,
            natives: None,
            extract: None,
        };
        assert_eq!(plain.natives_kind(), None);
    }
}
