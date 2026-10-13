//! JDK 下载/安装，按 jdk_source_mode 三态路由：
//! - auto（默认）：并发速率探测各源（每端实下前 2 MB 算有效吞吐，含代理真实表现）按速度排序安装，
//!   规避“HEAD 通过但下载卡死”；不可达源排最后。officialOnly：Mojang→Azul；mirrorOnly：Mojang→Adoptium 仅 TUNA（固定链）。
//!
//! 目录 `runtime/{platform}/java-{major}/`；完成后 scanner::inspect 校验并注册 jdk_scanned 缓存。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use sha2::Digest as _;

use crate::config::downloader::{DownloaderConfig, JdkSourceMode};
use crate::downloader::endpoints::JdkEndpoints;
use crate::downloader::fetcher::{self, http};
use crate::downloader::probe::{probe_speed, probe_url, SpeedProbeResult};
use crate::game::version::source::CacheBehaviour;
use crate::jdk::model::JdkInfo;
use crate::jdk::scanner;
use crate::util::{java_bin_path, parse_java_major, safe_relative_path};

/// 安装进度钩子（任务侧传入；均可为空，纯函数调用方直接不传）。
pub struct InstallProgress<'a> {
    /// 任务取消标志（下载循环每块检查，取消立即停止当前文件）
    pub cancel: Option<&'a AtomicBool>,
    /// 报告总文件数/总字节（manifest 解析后；Azul 路径不报，进度回退文件数）
    pub on_total: Option<&'a (dyn Fn(u32, u64) + Send + Sync)>,
    /// 字节增量（当前文件下载中；调用方自行持有 prev 做差分）
    pub on_bytes: Option<&'a (dyn Fn(u64) + Send + Sync)>,
    /// 单文件下载完成
    pub on_file_done: Option<&'a (dyn Fn() + Send + Sync)>,
}

/// 按 Java 主版本映射 Mojang runtime 组件名（all.json 键）：8→jre-legacy、17→gamma、21→delta、25→epsilon。
fn component_for_major(major: u32) -> Option<&'static str> {
    match major {
        8 => Some("jre-legacy"),
        17 => Some("java-runtime-gamma"),
        21 => Some("java-runtime-delta"),
        25 => Some("java-runtime-epsilon"),
        _ => None,
    }
}

fn platform_key() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows-x64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-os-arm64"
        } else {
            "mac-os-x64"
        }
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "aarch64") {
            "linux-arm64"
        } else {
            "linux-x64"
        }
    } else {
        "unsupported"
    }
}

/// 是否受支持的 managed runtime 目标（仅 64 位 x64/arm64；32 位架构 Minecraft 生态不支持，明确拒绝）。
fn is_supported_runtime_target() -> bool {
    cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
}

/// 统一入口：按 jdk_source_mode 三态选择回退链；已装同 major 时直接返回（幂等）。
pub async fn auto_install(
    major: u32,
    config: &DownloaderConfig,
    hook: &InstallProgress<'_>,
) -> Result<JdkInfo, String> {
    if !is_supported_runtime_target() {
        return Err("当前架构不受支持（仅支持 64 位 x64/arm64）".into());
    }
    let dest_dir = runtime_dir(config, major);
    // 已装同 major 则校验返回；Adoptium/Azul 顶层为 jdk-*/zulu*/ 子目录，
    // 需递归定位 java 根，否则每次启动都误判未装而重下整包
    if let Some(home) = find_java_home(&dest_dir) {
        if let Ok(info) = scanner::inspect(&home) {
            if parse_java_major(&info.version) == Some(major) {
                return Ok(info);
            }
        }
    }

    // Auto：先并发探测各源按速度排序；其余模式固定链（顺序敏感）。
    let chain: Vec<(JdkInstaller, &'static str)> = match config.jdk_source_mode {
        JdkSourceMode::Auto => build_speed_chain(major, config).await,
        JdkSourceMode::OfficialOnly => vec![
            (JdkInstaller::Mojang, "官方源"),
            (JdkInstaller::Azul, "Azul 源"),
        ],
        JdkSourceMode::MirrorOnly => vec![
            (JdkInstaller::Mojang, "官方源"),
            (JdkInstaller::Adoptium { tuna_only: true }, "国内镜像源"),
        ],
    };
    let mut errors = Vec::new();
    for (installer, source_name) in chain {
        match run_installer(&installer, major, config, hook).await {
            Ok(info) => return Ok(info),
            Err(e) => errors.push(format!("{source_name}: {e}")),
        }
    }
    Err(format!("下载 Java {} 失败\n{}", major, errors.join("\n")))
}

/// 回退链上的安装器（顺序由 jdk_source_mode 决定）。
enum JdkInstaller {
    /// Mojang 官方 runtime（manifest 获取与文件下载跟随文件源模式走镜像）
    Mojang,
    /// Adoptium：TUNA 镜像优先；`tuna_only`（mirrorOnly）时官方 link 不参与下载候选
    Adoptium { tuna_only: bool },
    /// Azul Zulu（包级 sha256）
    Azul,
}

async fn run_installer(
    installer: &JdkInstaller,
    major: u32,
    config: &DownloaderConfig,
    hook: &InstallProgress<'_>,
) -> Result<JdkInfo, String> {
    match installer {
        JdkInstaller::Mojang => install_mojang_runtime(major, config, hook).await,
        JdkInstaller::Adoptium { tuna_only } => {
            install_adoptium_runtime(major, config, hook, *tuna_only).await
        }
        JdkInstaller::Azul => install_azul_runtime(major, config, hook).await,
    }
}

/// 各源模式的回退链（纯安装器列表，不含标签）；Auto 探测后据此排序。
fn installer_chain(mode: JdkSourceMode) -> Vec<JdkInstaller> {
    match mode {
        JdkSourceMode::OfficialOnly => vec![JdkInstaller::Mojang, JdkInstaller::Azul],
        JdkSourceMode::MirrorOnly => vec![
            JdkInstaller::Mojang,
            JdkInstaller::Adoptium { tuna_only: true },
        ],
        JdkSourceMode::Auto => vec![
            JdkInstaller::Mojang,
            JdkInstaller::Adoptium { tuna_only: false },
            JdkInstaller::Azul,
        ],
    }
}

async fn speed_probe_url_for(
    installer: &JdkInstaller,
    major: u32,
    config: &DownloaderConfig,
) -> Option<String> {
    match installer {
        JdkInstaller::Mojang => {
            let mirror_urls: Vec<String> = config
                .file_sources_list()
                .iter()
                .map(|s| s.mirror_url(JdkEndpoints::RUNTIME_ALL_JSON))
                .collect();
            let all_text = fetch_all_json_content_sync(major, config).await?;
            let all: serde_json::Value = all_text.parse().ok()?;
            let pk = platform_key();
            let comp = component_for_major(major)?;
            let files: Vec<&str> = all
                .get(comp)?
                .get(pk)?
                .get("files")?
                .as_array()?
                .iter()
                .filter_map(|f| f.get("id")?.as_str())
                .collect();
            let target = files
                .iter()
                .copied()
                .find(|id| id.ends_with(".jar") || id.ends_with(".json"))
                .or_else(|| files.first().copied())?;
            let base: &str = mirror_urls
                .first()
                .map(|s| s.as_str())
                .unwrap_or(JdkEndpoints::RUNTIME_ALL_JSON);
            let base = base.rsplit_once('/').map(|(b, _)| format!("{}/", b))?;
            Some(format!("{}{}", base, target))
        }
        JdkInstaller::Adoptium { .. } => {
            let (os, arch) = adoptium_platform();
            let api_url = format!(
                "{}/v3/assets/latest/{}/hotspot?os={}&architecture={}&image_type=jdk",
                JdkEndpoints::ADOPTIUM_API_BASE,
                major,
                os,
                arch
            );
            let resp = http().get(&api_url).send().await.ok()?;
            if !resp.status().is_success() {
                return None;
            }
            let assets: serde_json::Value = resp.json().await.ok()?;
            let asset = assets.as_array()?.iter().find(|x| {
                x["binary"]["os"].as_str() == Some(os)
                    && x["binary"]["architecture"].as_str() == Some(arch)
            })?;
            let name = asset["binary"]["package"]["name"].as_str()?.to_string();
            Some(tuna_package_url(major, arch, os, &name))
        }
        JdkInstaller::Azul => {
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
                "{}/metadata/v1/zulu/packages/?java_version={major}&os={os}&arch={arch}&archive_type=zip&java_package_type=jdk&javafx_bundled=false&release_status=ga&latest=true",
                JdkEndpoints::AZUL_API_BASE
            );
            let resp = http().get(&api_url).send().await.ok()?;
            if !resp.status().is_success() {
                return None;
            }
            let packages: serde_json::Value = resp.json().await.ok()?;
            packages
                .as_array()?
                .first()?
                .get("download_url")?
                .as_str()
                .map(String::from)
        }
    }
}

/// 并行拉取 all.json 内容（带超时），供 Mojang 测速使用。
async fn fetch_all_json_content(url: &str) -> Option<String> {
    let resp = match tokio::time::timeout(Duration::from_secs(4), http().get(url).send()).await {
        Ok(Ok(r)) => r,
        _ => return None,
    };
    if !resp.status().is_success() {
        return None;
    }
    resp.text().await.ok()
}

/// 按顺序尝试所有 all.json 候选（官方 + 镜像），返回第一个格式正确的结果。
async fn fetch_all_json_content_sync(major: u32, config: &DownloaderConfig) -> Option<String> {
    let mirror_urls: Vec<String> = config
        .file_sources_list()
        .iter()
        .map(|s| s.mirror_url(JdkEndpoints::RUNTIME_ALL_JSON))
        .collect();
    let urls: Vec<String> = [JdkEndpoints::RUNTIME_ALL_JSON.to_string()]
        .into_iter()
        .chain(mirror_urls)
        .collect();
    for url in &urls {
        if let Some(text) = fetch_all_json_content(url).await {
            if let Ok(all) = text.parse::<serde_json::Value>() {
                let pk = platform_key();
                let comp = component_for_major(major);
                if let Some(comp) = comp {
                    if let Some(pk_val) = all.get(comp).and_then(|v| v.get(pk)) {
                        if pk_val.get("files").is_some() {
                            return Some(text);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Auto 模式：并发速率探测各源（每端实下前 2 MB 算有效吞吐）按速率降序排序；全不可达保持默认序。
async fn build_speed_chain(
    major: u32,
    config: &DownloaderConfig,
) -> Vec<(JdkInstaller, &'static str)> {
    let candidates = installer_chain(JdkSourceMode::Auto);

    struct CandidateWithSpeed {
        installer: JdkInstaller,
        speed: SpeedProbeResult,
        label: &'static str,
    }

    let labels: [&'_ str; 3] = ["官方源", "Adoptium 源", "Azul 源"];
    let mut handles = Vec::new();

    for (i, installer) in candidates.into_iter().enumerate() {
        let major_clone = major;
        let config_clone = config.clone();
        handles.push(tokio::task::spawn(async move {
            let url = speed_probe_url_for(&installer, major_clone, &config_clone).await;
            let speed = match url {
                Some(u) => probe_speed(&u).await,
                None => SpeedProbeResult {
                    ok: false,
                    speed_bps: 0,
                },
            };
            CandidateWithSpeed {
                installer,
                speed,
                label: labels[i],
            }
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(c) = handle.await {
            results.push(c);
        }
    }

    // 按速度降序（相同则保持稳定顺序），不可达排后
    results.sort_by(|a, b| {
        b.speed
            .speed_bps
            .cmp(&a.speed.speed_bps)
            .then_with(|| a.speed.ok.cmp(&b.speed.ok))
    });

    results
        .into_iter()
        .map(|c| (c.installer, c.label))
        .collect()
}

/// managed runtime 目录 `{minecraft_dir}/runtime/{platform}/java-{major}/`；pub(crate)：task/runner.rs 用于路径互斥。
pub(crate) fn runtime_dir(config: &DownloaderConfig, major: u32) -> PathBuf {
    config
        .minecraft_dir_abs()
        .join("runtime")
        .join(platform_key())
        .join(format!("java-{major}"))
}

async fn install_mojang_runtime(
    major: u32,
    config: &DownloaderConfig,
    hook: &InstallProgress<'_>,
) -> Result<JdkInfo, String> {
    let component = component_for_major(major)
        .ok_or_else(|| format!("Java {major} 无对应 Mojang runtime 组件（仅支持 8/17/21/25）"))?;
    let platform = platform_key();

    // 1. 拉取 all.json（多源回退 + TTL 缓存：BMCLAPI 镜像 piston-meta）
    let all_text = config
        .fetch_with_fallback(config.game_source_mode, CacheBehaviour::DefaultTtl, |s| {
            s.mirror_url(JdkEndpoints::RUNTIME_ALL_JSON)
        })
        .await
        .map_err(common_err_ctx!("获取 Java runtime 索引失败"))?;
    let all: serde_json::Value =
        serde_json::from_str(&all_text).map_err(common_err_ctx!("解析 runtime 索引失败"))?;

    // 2. 取平台→组件第一条 manifest（组件键在第二层；顶层同名键为空占位，勿取）
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
    let manifest_path = temp_file(&format!("runtime-manifest-{major}.json"));
    fetcher::fetch_verified(
        &manifest_urls,
        &manifest_path,
        None::<fn(u64, u64)>,
        &manifest_sha1,
    )
    .await
    .map_err(common_err_ctx!("下载 runtime manifest 失败"))?;
    let manifest_text =
        std::fs::read_to_string(&manifest_path).map_err(common_err_ctx!("读取 manifest 失败"))?;
    let _ = std::fs::remove_file(&manifest_path);
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).map_err(common_err_ctx!("解析 manifest 失败"))?;

    // 4. 统计文件条目/总字节（进度基线），逐文件下载（目录建目录、文件 sha1 校验、link 跳过）
    let dest_root = runtime_dir(config, major);
    let files = manifest["files"]
        .as_object()
        .ok_or("runtime manifest 缺 files")?;
    let mut total_files = 0u32;
    let mut total_bytes = 0u64;
    for (_, meta) in files {
        if meta["type"].as_str() != Some("file") {
            continue;
        }
        if meta["downloads"]["raw"].as_object().is_none() {
            continue;
        }
        total_files += 1;
        total_bytes += meta["downloads"]["raw"]["size"].as_u64().unwrap_or(0);
    }
    if let Some(f) = hook.on_total {
        f(total_files, total_bytes);
    }
    for (path, meta) in files {
        let ftype = meta["type"].as_str().unwrap_or("file");
        let dest = dest_root.join(safe_relative_path(path)?);
        match ftype {
            "directory" => {
                std::fs::create_dir_all(&dest).map_err(common_err_ctx!("创建目录失败"))?;
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
                // 字节回调走 delta_callback 差分助手（闭包需 Fn，fetch 内部按块调用）
                let cb = delta_callback(hook.on_bytes);
                fetcher::fetch_verified_c(&urls, &dest, Some(cb), sha1, hook.cancel)
                    .await
                    .map_err(common_err_ctx!("下载 {} 失败", path))?;
                if let Some(f) = hook.on_file_done {
                    f();
                }
            }
            "link" => {
                // symlink 条目主要在 unix 平台，跳过（实体文件均已逐一下载）
            }
            other => {
                bail!("未知 runtime 文件类型\n{other:?}（{path}）");
            }
        }
    }

    // 5. 校验：inspect 版本 + major 匹配
    let info = scanner::inspect(&dest_root).map_err(common_err_ctx!("安装后校验失败"))?;
    if parse_java_major(&info.version) != Some(major) {
        bail!(
            "安装的 Java 版本异常（需要 {}，实际 {}）",
            major, info.version
        );
    }
    Ok(info)
}

/// Adoptium（Temurin）回退：官方 API 查最新 GA 包（含 sha256）→ TUNA 优先、失败回退官方 link
/// → sha256 校验 → 解压（zip/tar.gz）→ inspect。`tuna_only`（mirrorOnly）：候选仅 TUNA。
async fn install_adoptium_runtime(
    major: u32,
    config: &DownloaderConfig,
    hook: &InstallProgress<'_>,
    tuna_only: bool,
) -> Result<JdkInfo, String> {
    // 1. 元数据：Adoptium API 最新 GA 资产（响应含全平台，按 os/arch 过滤）
    let (os, arch) = adoptium_platform();
    let api_url = format!(
        "{}/v3/assets/latest/{major}/hotspot?os={os}&architecture={arch}&image_type=jdk",
        JdkEndpoints::ADOPTIUM_API_BASE
    );
    let resp = http()
        .get(&api_url)
        .send()
        .await
        .map_err(common_err_ctx!("查询 Adoptium 包失败"))?;
    if !resp.status().is_success() {
        bail!("Adoptium API 返回 {}", resp.status());
    }
    let assets: serde_json::Value = resp
        .json()
        .await
        .map_err(common_err_ctx!("解析 Adoptium 响应失败"))?;
    let asset = assets
        .as_array()
        .and_then(|a| {
            a.iter().find(|x| {
                x["binary"]["os"].as_str() == Some(os)
                    && x["binary"]["architecture"].as_str() == Some(arch)
            })
        })
        .ok_or("Adoptium 无匹配包")?;
    let package = asset["binary"]["package"]
        .as_object()
        .ok_or("Adoptium 包缺 binary.package")?;
    let name = package
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Adoptium 包缺文件名")?
        .to_string();
    let sha256 = package
        .get("checksum")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 2. TUNA 优先（文件名与官方一致，直接构造 URL）；Auto 先探测 TTFB，快者优先、慢者降级兜底
    let tuna_url = tuna_package_url(major, arch, os, &name);
    let mut urls = vec![tuna_url.clone()];
    if !tuna_only {
        if let Some(link) = package.get("link").and_then(|v| v.as_str()) {
            let tuna_probe = probe_url(&tuna_url).await;
            let official_probe = probe_url(link).await;
            if prefer_official_over_tuna(
                official_probe.ok,
                official_probe.ttfb_ms,
                tuna_probe.ttfb_ms,
            ) {
                urls = vec![link.to_string(), tuna_url];
            } else {
                urls.push(link.to_string());
            }
        }
    }

    // 3. 下载（sha1 传空，整包 sha256 校验与 Azul 路径同构）→ 校验 → 解压 → inspect
    let ext = if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        "tar.gz"
    } else {
        "zip"
    };
    let pkg_path = temp_file(&format!("adoptium-java-{major}.{ext}"));
    let cb = delta_callback(hook.on_bytes);
    fetcher::fetch_verified_c(&urls, &pkg_path, Some(cb), "", hook.cancel)
        .await
        .map_err(common_err_ctx!("下载 Adoptium 包失败"))?;
    if let Some(f) = hook.on_file_done {
        f();
    }
    if !sha256.is_empty() {
        let actual = file_sha256(&pkg_path);
        if actual != sha256 {
            let _ = std::fs::remove_file(&pkg_path);
            return Err("Adoptium 包 sha256 校验失败".into());
        }
    }

    let dest_root = runtime_dir(config, major);
    std::fs::create_dir_all(&dest_root).map_err(common_err_ctx!("创建目录失败"))?;
    extract_archive(&pkg_path, &dest_root)?;
    let _ = std::fs::remove_file(&pkg_path);

    let home = find_java_home(&dest_root).ok_or("解压后未找到 JDK 根目录")?;
    let info = scanner::inspect(&home).map_err(common_err_ctx!("校验 Adoptium JDK 失败"))?;
    if parse_java_major(&info.version) != Some(major) {
        bail!(
            "Adoptium 安装版本异常（需要 {}，实际 {}）",
            major, info.version
        );
    }
    Ok(info)
}

/// Adoptium API 查询参数 / TUNA 目录的平台键（与 Azul 命名不同：mac 而非 macos）。
fn adoptium_platform() -> (&'static str, &'static str) {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    };
    (os, arch)
}

fn tuna_package_url(major: u32, arch: &str, os: &str, name: &str) -> String {
    format!(
        "{}/{major}/jdk/{arch}/{os}/{name}",
        JdkEndpoints::TUNA_ADOPTIUM_BASE
    )
}

/// 源层排序（纯逻辑便于单测）：官方可用且快于 TUNA → 官方优先，否则 TUNA 优先；探测失败不剔除候选。
fn prefer_official_over_tuna(official_ok: bool, official_ttfb_ms: u64, tuna_ttfb_ms: u64) -> bool {
    official_ok && official_ttfb_ms < tuna_ttfb_ms
}

/// 解压 JDK 包到 dest_root：.zip 用 zip crate；.tar.gz/.tgz 用 tar + flate2
/// （Adoptium 的 mac/linux 只提供 tar.gz）。所有条目过 safe_runtime_path 防路径穿越。
fn extract_archive(package: &Path, dest_root: &Path) -> Result<(), String> {
    let name = package.to_string_lossy().to_lowercase();
    if name.ends_with(".zip") {
        extract_zip(package, dest_root)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(package, dest_root)
    } else {
        Err(format!("不支持的 JDK 包格式\n{name}"))
    }
}

/// zip 解压（Mojang/Azul/Adoptium 共用的包级解压；zip 内顶层目录保留）。
fn extract_zip(package: &Path, dest_root: &Path) -> Result<(), String> {
    let file = std::fs::File::open(package).map_err(common_err_ctx!("打开 JDK 包失败"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(common_err_ctx!("读取 JDK 包失败"))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(common_err_ctx!("读取条目失败"))?;
        let name = entry.name().to_string();
        let out = dest_root.join(safe_relative_path(&name)?);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).ok();
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let mut file_out =
            std::fs::File::create(&out).map_err(common_err_ctx!("创建 {} 失败", out.display()))?;
        std::io::copy(&mut entry, &mut file_out)
            .map_err(common_err_ctx!("解压 {} 失败", out.display()))?;
    }
    Ok(())
}

/// tar.gz 解压（Adoptium mac/linux）：条目剥 `./` 后过 safe_runtime_path，符号链接等特殊条目跳过。
fn extract_tar_gz(package: &Path, dest_root: &Path) -> Result<(), String> {
    let file = std::fs::File::open(package).map_err(common_err_ctx!("打开 JDK 包失败"))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    let entries = archive
        .entries()
        .map_err(common_err_ctx!("读取 JDK 包失败"))?;
    for entry in entries {
        let mut entry = entry.map_err(common_err_ctx!("读取条目失败"))?;
        let mut name = entry
            .path()
            .map_err(common_err_ctx!("读取条目路径失败"))?
            .to_string_lossy()
            .into_owned();
        while let Some(stripped) = name.strip_prefix("./") {
            name = stripped.to_string();
        }
        let out = dest_root.join(safe_relative_path(&name)?);
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&out).ok();
            continue;
        }
        if !entry.header().entry_type().is_file() {
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let mut file_out =
            std::fs::File::create(&out).map_err(common_err_ctx!("创建 {} 失败", out.display()))?;
        std::io::copy(&mut entry, &mut file_out)
            .map_err(common_err_ctx!("解压 {} 失败", out.display()))?;
    }
    Ok(())
}

/// Azul Zulu 回退：metadata API 查 zip 包 → sha256 校验 → 解压 → inspect。
async fn install_azul_runtime(
    major: u32,
    config: &DownloaderConfig,
    hook: &InstallProgress<'_>,
) -> Result<JdkInfo, String> {
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
        "{}/metadata/v1/zulu/packages/?java_version={major}&os={os}&arch={arch}&archive_type=zip&java_package_type=jdk&javafx_bundled=false&release_status=ga&latest=true",
        JdkEndpoints::AZUL_API_BASE
    );
    let resp = http()
        .get(&api_url)
        .send()
        .await
        .map_err(common_err_ctx!("查询 Azul 包失败"))?;
    if !resp.status().is_success() {
        bail!("Azul API 返回 {}", resp.status());
    }
    let packages: serde_json::Value = resp
        .json()
        .await
        .map_err(common_err_ctx!("解析 Azul 响应失败"))?;
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

    // 下载 zip → sha256 校验（单包：字节增量回调 + 完成后文件计数）
    let zip_path = temp_file(&format!("azul-java-{major}.zip"));
    let urls: Vec<String> = vec![download_url.clone()];
    // 字节回调同 Mojang 路径，统一走 delta_callback 差分助手
    let cb = delta_callback(hook.on_bytes);
    fetcher::fetch_verified_c(&urls, &zip_path, Some(cb), "", hook.cancel)
        .await
        .map_err(common_err_ctx!("下载 Azul 包失败"))?;
    if let Some(f) = hook.on_file_done {
        f();
    }
    if !sha256.is_empty() {
        let actual = file_sha256(&zip_path);
        if actual != sha256 {
            let _ = std::fs::remove_file(&zip_path);
            return Err("Azul 包 sha256 校验失败".into());
        }
    }

    // 解压到 runtime 目录（zip 内顶层目录保留，如 zulu17.x-ca-jdk17.x-win_x64/）
    let dest_root = runtime_dir(config, major);
    std::fs::create_dir_all(&dest_root).map_err(common_err_ctx!("创建目录失败"))?;
    extract_archive(&zip_path, &dest_root)?;
    let _ = std::fs::remove_file(&zip_path);

    let home = find_java_home(&dest_root).ok_or("解压后未找到 JDK 根目录")?;
    let info = scanner::inspect(&home).map_err(common_err_ctx!("校验 Azul JDK 失败"))?;
    if parse_java_major(&info.version) != Some(major) {
        bail!(
            "Azul 安装版本异常（需要 {}，实际 {}）",
            major, info.version
        );
    }
    Ok(info)
}

/// 递归查找含 `bin/java(.exe)` 的 JDK 根目录（Azul zip 顶层目录名不确定）。
fn find_java_home(dir: &Path) -> Option<PathBuf> {
    let bin = java_bin_path(dir);
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

/// 字节增量回调工厂：闭包自持水位原子，swap-prev 差分增量转给 `on_bytes`；
/// 不复用 byte_callback（JDK 链路无步骤计数，消费端不同构）。
fn delta_callback<'a>(on_bytes: Option<&'a (dyn Fn(u64) + Send + Sync)>) -> impl Fn(u64, u64) + 'a {
    let prev = AtomicU64::new(0);
    move |downloaded: u64, _total: u64| {
        let p = prev.swap(downloaded, Ordering::Relaxed);
        if let Some(f) = on_bytes {
            f(downloaded.saturating_sub(p));
        }
    }
}

/// 文件 sha256（Azul 包校验），1MB 分块流式读；读取失败返回空串（fail-closed → 校验失败）。
fn file_sha256(path: &Path) -> String {
    let Ok(file) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut reader = std::io::BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(_) => return String::new(),
        }
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
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
    fn installer_chain_matches_source_mode() {
        assert_eq!(installer_chain(JdkSourceMode::Auto).len(), 3);
        assert_eq!(installer_chain(JdkSourceMode::OfficialOnly).len(), 2);
        assert_eq!(installer_chain(JdkSourceMode::MirrorOnly).len(), 2);
        // MirrorOnly 的 Adoptium 必须 tuna_only（官方 link 不参与下载候选）
        assert!(matches!(
            installer_chain(JdkSourceMode::MirrorOnly)[1],
            JdkInstaller::Adoptium { tuna_only: true }
        ));
    }

    #[test]
    fn adoptium_tuna_url_constructed_from_package_name() {
        assert_eq!(
            tuna_package_url(
                21,
                "x64",
                "windows",
                "OpenJDK21U-jdk_x64_windows_hotspot_21.0.12.1_1.zip"
            ),
            "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/21/jdk/x64/windows/OpenJDK21U-jdk_x64_windows_hotspot_21.0.12.1_1.zip"
        );
        assert_eq!(
            tuna_package_url(
                17,
                "aarch64",
                "mac",
                "OpenJDK17U-jdk_aarch64_mac_hotspot_17.0.13_11.tar.gz"
            ),
            "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/17/jdk/aarch64/mac/OpenJDK17U-jdk_aarch64_mac_hotspot_17.0.13_11.tar.gz"
        );
    }

    #[test]
    fn find_java_home_finds_nested_bin() {
        let tmp = std::env::temp_dir().join(format!("elytra_jh_{}", std::process::id()));
        let home = tmp.join("zulu17-ca-jdk17.0.11-win_x64");
        std::fs::create_dir_all(home.join("bin")).unwrap();
        let bin = java_bin_path(&home);
        std::fs::write(&bin, b"x").unwrap();
        let found = find_java_home(&tmp);
        assert_eq!(
            found.as_deref(),
            Some(home.as_path()),
            "应递归定位 zulu 子目录内的 java 根"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn extract_tar_gz_handles_nested_layout_and_finds_home() {
        let tmp = std::env::temp_dir().join(format!("elytra_tgz_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let pkg = tmp.join("jdk.tar.gz");
        {
            let file = std::fs::File::create(&pkg).unwrap();
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(enc);
            let entry_rel = java_bin_path(Path::new("jdk-21.0.12.1+1"))
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = b"fake java";
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, &entry_rel, &bytes[..])
                .unwrap();
            builder.finish().unwrap();
        }
        let dest = tmp.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        extract_archive(&pkg, &dest).unwrap();
        let found = find_java_home(&dest);
        assert_eq!(
            found.as_deref(),
            Some(dest.join("jdk-21.0.12.1+1").as_path()),
            "应递归定位嵌套 JDK 根"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }
}
