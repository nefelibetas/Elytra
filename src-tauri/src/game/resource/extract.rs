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

/// 平台 rules 语义的 OS 名（Mojang rules 的 os.name：macOS 为 "osx"）。
#[cfg(target_os = "windows")]
pub const TARGET_OS: &str = "windows";
#[cfg(target_os = "linux")]
pub const TARGET_OS: &str = "linux";
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub const TARGET_OS: &str = "osx";

/// natives 库名中的 OS 段（与 rules 的 os name 不同：Mojang 数据里
/// rules 用 `"osx"`，但 LWJGL natives 库名后缀用 `"macos"`——两套命名）。
#[cfg(target_os = "macos")]
pub const TARGET_NATIVES_OS: &str = "macos";
#[cfg(not(target_os = "macos"))]
pub const TARGET_NATIVES_OS: &str = TARGET_OS;

/// natives 库名中的架构段（`-arm64` / `-x86` / 无 = x64）。
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

/// 需要靠库名后缀区分。Mojang 数据实际命名（26.3 实测）：
///   x64：  `natives-windows` / `natives-linux` / `natives-macos`
///   arm64：`natives-windows-arm64` / `natives-macos-arm64`（及老版本 linux-arm64）
///   x86：  `natives-windows-x86`
/// 注意 macOS 的 natives 库名用 `macos`，而 rules 的 os name 用 `osx`（两套命名）。
pub fn modern_native_matches(lib_name: &str) -> bool {
    let Some(classifier) = lib_name.rsplit(':').next() else {
        return true;
    };
    let Some(rest) = classifier.strip_prefix("natives-") else {
        return true; // 非 native 库，交给 rules 处理
    };
    // rest 形如 "windows"、"windows-arm64"、"linux"、"macos"、"macos-arm64"、"windows-x86"
    let (os_part, arch_part) = match rest.split_once('-') {
        Some((os, arch)) => (os, Some(arch)),
        None => (rest, None),
    };
    if os_part != TARGET_NATIVES_OS {
        return false;
    }
    // 架构精确匹配：arm64 → 只收 `-arm64`；x86 → 只收 `-x86`；x64 → 不收任何后缀
    match TARGET_ARCH {
        "arm64" => arch_part == Some("arm64"),
        "x86" => arch_part == Some("x86"),
        _ => arch_part.is_none(),
    }
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
    pc: &ProgressCounters,
    cancel: Option<&Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();

    // 先单次遍历筛出候选：既作为进度分母（任务层心跳据此算该步骤百分比），
    // 也让"跳过条件"只有一处实现（此前只有执行循环，改条件时容易与分母漂移）。
    let mut pending: Vec<(&Library, &Artifact)> = Vec::new();
    for lib in libraries {
        // 仅旧式（natives 字段 + classifiers）参与；新式 natives 作为普通库由 should_download 覆盖
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
        // 每个源各构造一个 URL，下载时主源过慢/失败自动降级
        let urls: Vec<String> = sources
            .iter()
            .map(|s| s.mirror_url(&artifact.url))
            .collect();
        // 跨任务互斥：防多任务并发写同一 natives 文件互相踩踏
        let _file_lock = wait_path_lock(&dest).await;
        // 字节级进度：增量累加进全局与 natives 步骤计数（重试时进度从 0 重新开始，增量不会为负）
        let progress = pc.byte_callback();
        // 逐源 sha1 校验下载：某源返回内容不匹配自动换下一源重试（镜像缓存陈旧场景）。
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

/// 把单个 ZIP 条目解压到目标文件（ModLoader installer 流程共用）。
/// 打开 ZIP 归档（统一错误文案）。整合包/地图/压缩包读取共用。
pub fn open_zip(path: &Path, err_prefix: &str) -> Result<zip::ZipArchive<std::fs::File>, String> {
    let file = std::fs::File::open(path).map_err(common_err_ctx!("打开{}失败", err_prefix))?;
    zip::ZipArchive::new(file).map_err(common_err_ctx!("读取{}失败", err_prefix))
}
/// 把 ZIP 条目解压到目标文件（父目录自动创建）。
/// ModLoader installer 解 jar、整合包单成员解压共用。
pub fn extract_entry_to(entry: &mut zip::read::ZipFile, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(common_err_ctx!("创建目录失败"))?;
    }
    let mut out = std::fs::File::create(dest).map_err(common_err_ctx!("创建文件失败"))?;
    std::io::copy(entry, &mut out).map_err(common_err_ctx!("解压失败"))?;
    Ok(())
}

/// 安全解压 zip：逐条目经 `safe_runtime_path` 校验（拒绝绝对路径/`..` 段/空段），
/// 防 zip slip；每个文件条目检查 cancel（取消立即中止）；目录条目建目录。
/// worlds 地图存档安装流程使用（zip 来自第三方，路径穿越防护必须）。
pub(crate) fn extract_zip_safe(
    zip_path: &Path,
    dest_root: &Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(common_err_ctx!("打开存档文件失败"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(common_err_ctx!("读取存档文件失败"))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(common_err_ctx!("读取存档条目失败"))?;
        let name = entry.name().to_string();
        let out = dest_root.join(safe_relative_path(&name)?);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).ok();
            continue;
        }
        if cancel.load(Ordering::Relaxed) {
            return Err("任务已取消\n".into());
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(common_err_ctx!("创建目录失败"))?;
        }
        let mut file_out =
            std::fs::File::create(&out).map_err(common_err_ctx!("创建 {} 失败", out.display()))?;
        std::io::copy(&mut entry, &mut file_out)
            .map_err(common_err_ctx!("解压 {} 失败", out.display()))?;
    }
    Ok(())
}

/// 将 JAR（ZIP）解压到 `out_dir`，跳过 `extract.exclude` 中的路径。
/// 公开给启动命令构建使用（旧式 natives 运行时解压到临时目录）。
pub fn extract_jar(
    jar_path: &Path,
    out_dir: &Path,
    extract: Option<&ExtractRules>,
) -> Result<(), String> {
    let file = std::fs::File::open(jar_path).map_err(common_err_ctx!("打开 JAR 失败"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(common_err_ctx!("读取 JAR 失败"))?;

    let excludes: Vec<&str> = extract
        .map(|e| e.exclude.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(common_err_ctx!("读取 ZIP 条目失败"))?;

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
                .map_err(|e| format!("创建目录失败\n{}: {}", parent.display(), e))?;
        }

        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("读取失败\n{}: {}", name, e))?;

        std::fs::write(&target, &buf)
            .map_err(|e| format!("写入失败\n{}: {}", target.display(), e))?;
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
        // 平台无关：用当前平台常量构造，验证 OS + 架构精确匹配。
        // 当前 OS 的 x64 natives（无后缀）→ 匹配（若当前是 x64）
        // 当前 OS 的 arm64/x86 变体 → 仅当前架构匹配
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

        // 其他 OS 一律不匹配
        for other in ["windows", "linux", "macos"]
            .into_iter()
            .filter(|o| *o != os)
        {
            let lib = format!("org.lwjgl:lwjgl-freetype:3.4.1:natives-{other}");
            assert!(!modern_native_matches(&lib), "其他 OS 不应匹配: {other}");
            let lib_arm64 = format!("org.lwjgl:lwjgl-freetype:3.4.1:natives-{other}-arm64");
            assert!(!modern_native_matches(&lib_arm64), "其他 OS arm64 不应匹配");
        }

        // 普通库（无 natives 后缀）不受影响
        assert!(modern_native_matches("org.lwjgl:lwjgl-freetype:3.4.1"));
        assert!(modern_native_matches("com.google.guava:guava:33.0.0-jre"));

        // 无冒号分隔的库名
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
        // 旧式：natives 字段（1.12.2 lwjgl-platform 2.9.4 形态）
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

    /// 构造测试 zip：entries 为 (条目名, 内容) 列表，全部作为文件写入。
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

    /// zip slip 防护：`../` 条目必须被拒绝，且目标根之外不落任何文件。
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

    /// 正常 zip（多目录 + 文件）解压成功、目录结构正确。
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

    /// 取消标记在文件条目解压前生效：立即中止且不继续写出。
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
        // 取消标记由外部在解压前设置：首个文件条目前检查即中止
        cancel.store(true, Ordering::Relaxed);
        let err = extract_zip_safe(&zip_path, &dest, &cancel).unwrap_err();
        assert!(err.contains("取消"), "应返回取消错误，实际: {err}");
    }
}
