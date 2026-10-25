use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::config::downloader::DownloadSource;
use crate::downloader::fetcher;
use crate::downloader::hash::file_sha1_matches;
use crate::downloader::path_lock::wait_path_lock;
use crate::downloader::progress::ProgressCounters;
use crate::game::version::model::{Artifact, ExtractRules, Library, OsRule, Rule};
use crate::util::safe_relative_path;

/// natives 代际：旧式（classifiers）vs 新式（库名后缀独立变体）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativesKind {
    Legacy,
    /// 新式：架构变体独立库，库名带 `:natives-{os}` 后缀
    Modern,
}

impl Library {
    /// 判定 natives 代际（普通库 None）：先查库名后缀（新式）再查 natives 字段（旧式），两代互斥。
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

/// rules 的 os.name（macOS 为 "osx"）。
#[cfg(target_os = "windows")]
pub const TARGET_OS: &str = "windows";
#[cfg(target_os = "linux")]
pub const TARGET_OS: &str = "linux";
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub const TARGET_OS: &str = "osx";

/// natives 库名的 OS 段：rules 用 `"osx"`，natives 后缀用 `"macos"`（两套命名）。
#[cfg(target_os = "macos")]
pub const TARGET_NATIVES_OS: &str = "macos";
#[cfg(not(target_os = "macos"))]
pub const TARGET_NATIVES_OS: &str = TARGET_OS;

/// natives 库名的架构段（`-arm64` / `-x86` / 无 = x64）。
#[cfg(target_arch = "aarch64")]
pub const TARGET_ARCH: &str = "arm64";
#[cfg(target_arch = "x86")]
pub const TARGET_ARCH: &str = "x86";
#[cfg(all(not(target_arch = "aarch64"), not(target_arch = "x86")))]
pub const TARGET_ARCH: &str = "x64";

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

/// 旧式：当前平台的 natives classifier key（查 `natives` 字段）。
fn legacy_natives_key(natives: &HashMap<String, String>) -> Option<&str> {
    natives.get(TARGET_OS).map(|s| s.as_str())
}

/// 新式：当前平台的 classifier key（natives-windows/-linux/-osx）。
pub fn modern_natives_key() -> Option<String> {
    match TARGET_OS {
        "windows" => Some("natives-windows".into()),
        "linux" => Some("natives-linux".into()),
        "osx" | "macos" => Some("natives-osx".into()),
        _ => None,
    }
}

/// 旧式：当前平台的 natives classifier artifact（natives map → downloads.classifiers）；下载侧与启动解压侧共用。
pub fn legacy_classifier_artifact(lib: &Library) -> Option<&Artifact> {
    let key = legacy_natives_key(lib.natives.as_ref()?)?;
    classifier_artifact(lib, key)
}

fn classifier_artifact<'a>(lib: &'a Library, key: &str) -> Option<&'a Artifact> {
    lib.downloads.as_ref()?.classifiers.as_ref()?.get(key)
}

/// 匹配当前平台的 natives 库名后缀（Mojang 命名）：x64 `natives-windows`/`-linux`/`-macos`，
/// arm64 `natives-windows-arm64`/`-macos-arm64`（旧版含 linux-arm64），x86 `natives-windows-x86`。
/// macOS 的 natives 名用 `macos`，rules 的 os name 用 `osx`（两套命名）。
pub fn modern_native_matches(lib_name: &str) -> bool {
    let Some(classifier) = lib_name.rsplit(':').next() else {
        return true;
    };
    let Some(rest) = classifier.strip_prefix("natives-") else {
        return true; // 非 native 库，交给 rules 处理
    };
    let (os_part, arch_part) = match rest.split_once('-') {
        Some((os, arch)) => (os, Some(arch)),
        None => (rest, None),
    };
    if os_part != TARGET_NATIVES_OS {
        return false;
    }
    // 架构精确匹配：arm64/x86 只收对应后缀，x64 不收后缀
    match TARGET_ARCH {
        "arm64" => arch_part == Some("arm64"),
        "x86" => arch_part == Some("x86"),
        _ => arch_part.is_none(),
    }
}

/// 下载匹配当前 OS 的 native classifier JAR 到 `libraries/`：旧式/新式都只下 JAR，
/// 不在 `.minecraft` 建 natives 目录（旧式启动时解压到系统临时目录，新式由 LWJGL 运行时自解压）。
pub async fn process_natives(
    libraries: &[Library],
    libraries_dir: &Path,
    sources: &[&DownloadSource],
    pc: &ProgressCounters,
    cancel: Option<&Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    // 单次遍历筛候选：兼作进度分母，跳过条件只有一处实现（避免与分母漂移）。
    let mut pending: Vec<(&Library, &Artifact)> = Vec::new();
    for lib in libraries {
        // 仅旧式参与；新式 natives 作为普通库由 should_download 覆盖
        let Some(artifact) = legacy_classifier_artifact(lib) else {
            continue;
        };
        let dest = libraries_dir.join(&artifact.path);
        if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) {
            continue;
        }
        pending.push((lib, artifact));
    }
    pc.step_files_total
        .store(pending.len() as u32, Ordering::Relaxed);

    for (lib, artifact) in pending {
        let dest = libraries_dir.join(&artifact.path);
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                errors.push(format!("创建目录失败\n{}: {}", lib.name, e));
                continue;
            }
        }
        // 每源一个 URL，主源过慢/失败自动降级
        let urls: Vec<String> = sources
            .iter()
            .map(|s| s.mirror_url(&artifact.url))
            .collect();
        // 跨任务互斥：防并发写同一 natives 文件
        let _file_lock = wait_path_lock(&dest).await;
        // 字节级进度增量累加（重试从 0 重启，增量不为负）
        let progress = pc.byte_callback();
        // 逐源 sha1 校验，不匹配自动换下一源（镜像缓存陈旧）。
        match fetcher::fetch_verified_c(
            &urls,
            &dest,
            Some(progress),
            &artifact.sha1,
            cancel.map(|c| c.as_ref()),
        )
        .await
        {
            Ok(_dl) => {
                pc.files.fetch_add(1, Ordering::Relaxed);
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

/// 打开 ZIP 归档（统一错误文案）；整合包/地图/压缩包读取共用。
pub fn open_zip(path: &Path, err_prefix: &str) -> Result<zip::ZipArchive<std::fs::File>, String> {
    let file = std::fs::File::open(path).map_err(common_err_ctx_key!(
        "errors.file.read",
        "打开{}失败",
        err_prefix
    ))?;
    zip::ZipArchive::new(file).map_err(common_err_ctx_key!(
        "errors.file.read",
        "读取{}失败",
        err_prefix
    ))
}
/// 把 ZIP 条目解压到目标文件（父目录自动创建）；ModLoader installer 解 jar、整合包单成员共用。
pub fn extract_entry_to(entry: &mut zip::read::ZipFile, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(common_err_ctx_key!("errors.file.createDir", "创建目录失败"))?;
    }
    let mut out = std::fs::File::create(dest)
        .map_err(common_err_ctx_key!("errors.file.write", "创建文件失败"))?;
    std::io::copy(entry, &mut out)
        .map_err(common_err_ctx_key!("errors.file.extract", "解压失败"))?;
    Ok(())
}

/// 安全解压 zip（worlds 安装用，zip 来自第三方）：逐条目经 `safe_runtime_path` 校验
/// （拒绝对路径/`..`/空段，防 zip slip）；每文件条目检查 cancel（取消立即中止），目录条目建目录。
pub(crate) fn extract_zip_safe(
    zip_path: &Path,
    dest_root: &Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    let file = std::fs::File::open(zip_path)
        .map_err(common_err_ctx_key!("errors.file.read", "打开存档文件失败"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(common_err_ctx_key!("errors.file.read", "读取存档文件失败"))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(common_err_ctx_key!("errors.file.read", "读取存档条目失败"))?;
        let name = entry.name().to_string();
        let out = dest_root.join(safe_relative_path(&name)?);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).ok();
            continue;
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(keyed_msg!("errors.task.cancelled", "任务已取消\n"));
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)
                .map_err(common_err_ctx_key!("errors.file.createDir", "创建目录失败"))?;
        }
        let mut file_out = std::fs::File::create(&out).map_err(common_err_ctx_key!(
            "errors.file.write",
            "创建 {} 失败",
            out.display()
        ))?;
        std::io::copy(&mut entry, &mut file_out).map_err(common_err_ctx_key!(
            "errors.file.extract",
            "解压 {} 失败",
            out.display()
        ))?;
    }
    Ok(())
}

/// 将 JAR 解压到 `out_dir`，跳过 `extract.exclude`；公开给启动命令（旧式 natives 运行时解压到临时目录）。
pub fn extract_jar(
    jar_path: &Path,
    out_dir: &Path,
    extract: Option<&ExtractRules>,
) -> Result<(), String> {
    let file = std::fs::File::open(jar_path)
        .map_err(common_err_ctx_key!("errors.file.read", "打开 JAR 失败"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(common_err_ctx_key!("errors.file.read", "读取 JAR 失败"))?;

    let excludes: Vec<&str> = extract
        .map(|e| e.exclude.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(common_err_ctx_key!("errors.file.read", "读取 ZIP 条目失败"))?;

        let name = entry.name().to_string();

        if excludes.iter().any(|e| name.starts_with(e) || name == *e) {
            continue;
        }

        if entry.is_dir() {
            continue;
        }

        let target = out_dir.join(&name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(common_err_ctx_key!(
                "errors.file.createDir",
                "创建目录失败\n{}",
                parent.display()
            ))?;
        }

        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf).map_err(common_err_ctx_key!(
            "errors.file.read",
            "读取失败\n{}",
            name
        ))?;

        std::fs::write(&target, &buf).map_err(common_err_ctx_key!(
            "errors.file.write",
            "写入失败\n{}",
            target.display()
        ))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use std::sync::atomic::AtomicBool;

    #[cfg(target_os = "windows")]
    use crate::game::version::model::LibraryDownloads;

    #[test]
    fn test_modern_native_matches() {
        let os = TARGET_NATIVES_OS;
        let mine = format!("org.lwjgl:lwjgl-freetype:3.4.1:natives-{os}");
        assert_eq!(
            modern_native_matches(&mine),
            TARGET_ARCH == "x64",
            "当前 OS 无后缀 natives 应仅在 x64 匹配"
        );

        let mine_arm64 = format!("org.lwjgl:lwjgl-freetype:3.4.1:natives-{os}-arm64");
        assert_eq!(
            modern_native_matches(&mine_arm64),
            TARGET_ARCH == "arm64",
            "当前 OS arm64 natives 应仅在 arm64 匹配"
        );

        let mine_x86 = format!("org.lwjgl:lwjgl-freetype:3.4.1:natives-{os}-x86");
        assert!(!modern_native_matches(&mine_x86), "当前平台不收 x86 变体");

        for other in ["windows", "linux", "macos"]
            .into_iter()
            .filter(|o| *o != os)
        {
            let lib = format!("org.lwjgl:lwjgl-freetype:3.4.1:natives-{other}");
            assert!(!modern_native_matches(&lib), "其他 OS 不应匹配: {other}");
            let lib_arm64 = format!("org.lwjgl:lwjgl-freetype:3.4.1:natives-{other}-arm64");
            assert!(!modern_native_matches(&lib_arm64), "其他 OS arm64 不应匹配");
        }

        assert!(modern_native_matches("org.lwjgl:lwjgl-freetype:3.4.1"));
        assert!(modern_native_matches("com.google.guava:guava:33.0.0-jre"));

        assert!(modern_native_matches("some-library"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_legacy_classifier_artifact_windows() {
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
        let legacy = Library {
            name: "org.lwjgl.lwjgl:lwjgl-platform:2.9.4-nightly-20150209".into(),
            downloads: None,
            rules: None,
            natives: Some(HashMap::from([(
                "windows".into(),
                "natives-windows".into(),
            )])),
            extract: None,
        };
        assert_eq!(legacy.natives_kind(), Some(NativesKind::Legacy));

        let modern = Library {
            name: "org.lwjgl:lwjgl:3.4.1:natives-windows".into(),
            downloads: None,
            rules: None,
            natives: None,
            extract: None,
        };
        assert_eq!(modern.natives_kind(), Some(NativesKind::Modern));

        let plain = Library {
            name: "com.azure:azure-json:1.4.0".into(),
            downloads: None,
            rules: None,
            natives: None,
            extract: None,
        };
        assert_eq!(plain.natives_kind(), None);
    }

    /// 构造测试 zip：entries 为 (条目名, 内容)，全部作文件写入。
    fn make_zip(zip_path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
    }

    /// zip slip：`../` 条目必须被拒，且不落目标根之外。
    #[test]
    fn extract_zip_safe_rejects_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("evil.zip");
        make_zip(
            &zip_path,
            &[("../evil.txt", b"boom"), ("level.dat", b"data")],
        );
        let dest = tmp.path().join("dest");
        let cancel = AtomicBool::new(false);
        let err = extract_zip_safe(&zip_path, &dest, &cancel).unwrap_err();
        assert!(err.contains("非法"), "应报路径非法，实际: {err}");
        assert!(
            !tmp.path().join("evil.txt").exists(),
            "zip slip 不得写出目标根之外"
        );
        assert!(
            !dest.join("evil.txt").exists(),
            "目标根内也不得出现穿越条目"
        );
    }

    #[test]
    fn extract_zip_safe_unpacks_normal_world() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("world.zip");
        make_zip(
            &zip_path,
            &[
                ("level.dat", b"level"),
                ("region/r.0.0.mca", b"region"),
                ("icon.png", b"png"),
            ],
        );
        let dest = tmp.path().join("dest");
        let cancel = AtomicBool::new(false);
        extract_zip_safe(&zip_path, &dest, &cancel).unwrap();
        assert_eq!(std::fs::read(dest.join("level.dat")).unwrap(), b"level");
        assert_eq!(
            std::fs::read(dest.join("region/r.0.0.mca")).unwrap(),
            b"region"
        );
        assert_eq!(std::fs::read(dest.join("icon.png")).unwrap(), b"png");
    }

    /// 取消在文件条目前生效：立即中止不续写。
    #[test]
    fn extract_zip_safe_honors_cancel() {
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("world.zip");
        make_zip(
            &zip_path,
            &[("a.dat", b"a"), ("b.dat", b"b"), ("c.dat", b"c")],
        );
        let dest = tmp.path().join("dest");
        let cancel = AtomicBool::new(false);
        cancel.store(true, Ordering::Relaxed);
        let err = extract_zip_safe(&zip_path, &dest, &cancel).unwrap_err();
        assert!(err.contains("取消"), "应返回取消错误，实际: {err}");
    }
}
