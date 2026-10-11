//! Installer **processors 引擎**：按 installer 自己声明的 `install_profile.json` 执行安装。
//!
//! # 为什么需要它
//!
//! 不同代的 Forge/NeoForge 补丁链完全不同：
//! - NeoForge 26.x：`installertools` 单个 `PROCESS_MINECRAFT_JAR` 任务；
//! - NeoForge 21.x：`installertools`/`jarsplitter`/`AutoRenamingTool` 一串处理器，
//!   最后 `binarypatcher` 打补丁，且工具版本（`installertools:2.1.2` 等）由 installer 声明。
//!
//! 写死其中一代的配方，在另一代上必然失败（实测：拿 21.1.248 的 `data/client.lzma`
//! 喂给写死的 `installertools:4.0.12` → `Invalid bundle signature`；而 2.1.2 里根本
//! 没有 `PROCESS_MINECRAFT_JAR` 这个 task）。因此这里只认 profile 声明：
//! `data`（占位符）→ `libraries`（工具构件与 URL/sha1）→ `processors`（顺序与参数）。
//!
//! # 已实现的 profile 语义
//!
//! - data 值三种形态：`'literal'`（字面量）、`/rel`（installer 内条目或绝对路径）、
//!   `[group:artifact:version[:classifier][@ext]]`（maven 本地布局路径）
//! - data 值可写成 `{ "client": ..., "server": ... }`，按 `side` 取
//! - 处理器参数里出现 `[coord@ext]` 表示**输入构件**：下载（优先 profile.libraries
//!   声明的 URL/sha1，其次按 maven 布局拼接 + 镜像回退），必要时从构件内解出 `@ext` 成员
//! - 处理器 `sides` 不含 `client` 时跳过；主类取处理器 JAR 的 `META-INF/MANIFEST.MF`
//! - 每个处理器独立 classpath（`classpath` 项 + `jar`），逐个 `java -cp … <main> <args…>`

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::Deserialize;

use super::forge::resolve_java;
use super::run_installer_command;
use crate::config::downloader::{DownloadSource, DownloaderConfig};
use crate::downloader::fetcher;

/// `install_profile.json` 的执行相关视图（其余字段忽略）。
#[derive(Debug, Deserialize)]
pub struct InstallProfile {
    #[serde(default)]
    pub data: HashMap<String, DataValue>,
    #[serde(default)]
    pub libraries: Vec<ProfileLibrary>,
    #[serde(default)]
    pub processors: Vec<Processor>,
}

/// data 值：字面量字符串，或按 client/server 分侧。
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum DataValue {
    Plain(String),
    Sided {
        client: Option<String>,
        server: Option<String>,
    },
}

impl DataValue {
    fn for_side(&self, side: &str) -> Option<&str> {
        match self {
            DataValue::Plain(s) => Some(s.as_str()),
            DataValue::Sided { client, server } => {
                if side == "server" {
                    server.as_deref()
                } else {
                    client.as_deref()
                }
            }
        }
    }
}

/// profile 声明的构件（工具 jar / 依赖），带官方 URL 与 sha1 时优先照搬。
#[derive(Debug, Deserialize)]
pub struct ProfileLibrary {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<ProfileDownloads>,
}

#[derive(Debug, Deserialize)]
pub struct ProfileDownloads {
    #[serde(default)]
    pub artifact: Option<ProfileArtifact>,
}

#[derive(Debug, Deserialize)]
pub struct ProfileArtifact {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
}

/// 单个处理器：主 JAR + classpath + 参数（+ 可选 sides 限定）。
#[derive(Debug, Deserialize)]
pub struct Processor {
    pub jar: String,
    #[serde(default)]
    pub classpath: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub sides: Option<Vec<String>>,
}

/// 执行上下文（路径 + 下载配置 + 取消）。
pub struct ProcessorCtx<'a> {
    /// installer jar 路径（`{INSTALLER}`，也用于 `/data/...` 类条目解包）
    pub installer: &'a Path,
    /// 游戏根目录（`{ROOT}`）
    pub mc_dir: &'a Path,
    /// 原版 client jar（`{MINECRAFT_JAR}`）
    pub vanilla_jar: &'a Path,
    /// `libraries/`（构件按 maven 布局落盘于此）
    pub lib_dir: &'a Path,
    /// 解包/中间产物目录（`@ext` 解出的成员、installer 内条目）
    pub work_dir: &'a Path,
    pub config: &'a DownloaderConfig,
    pub cancel: &'a Arc<AtomicBool>,
}

/// 解析清单文本。
pub fn parse_profile(json: &str) -> Result<InstallProfile, String> {
    serde_json::from_str(json).map_err(|e| format!("解析 install_profile.json 失败\n{e}"))
}

/// 从 installer jar 读 `install_profile.json` 文本（生产走已打开的 archive，测试直接给路径）。
pub fn read_profile_from_installer(installer: &Path) -> Result<String, String> {
    let file = std::fs::File::open(installer).map_err(|e| format!("打开安装程序失败\n{e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("安装程序不是 zip\n{e}"))?;
    let mut entry = archive
        .by_name("install_profile.json")
        .map_err(|_| "安装程序缺少 install_profile.json".to_string())?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut entry, &mut text)
        .map_err(|e| format!("读取 install_profile.json 失败\n{e}"))?;
    Ok(text)
}

/// 由已解析的 JSON 值构造 profile（Forge 分支拿到的就是 `serde_json::Value`）。
pub fn parse_profile_value(value: &serde_json::Value) -> Result<InstallProfile, String> {
    serde_json::from_value(value.clone()).map_err(|e| format!("解析 install_profile 失败\n{e}"))
}

/// 产物是否可信：存在、≥1MB 且能被当作 zip 打开。
/// 半成品/截断文件不算（否则中途失败的产物会被当成"已完成"永久跳过）。
pub fn intact_archive(p: &Path) -> bool {
    if !p.metadata().map(|m| m.len() >= 1_000_000).unwrap_or(false) {
        return false;
    }
    std::fs::File::open(p)
        .ok()
        .and_then(|f| zip::ZipArchive::new(f).ok())
        .is_some()
}

/// 取某侧可用的 data 值。
fn data_value<'a>(profile: &'a InstallProfile, key: &str, side: &str) -> Option<&'a str> {
    profile.data.get(key)?.for_side(side)
}

/// `[group:artifact:version[:classifier][@ext]]` → maven 本地布局相对路径。
///
/// 例：`net.neoforged:neoform:1.21.1-20240808.144430:mappings@txt`
/// → `net/neoforged/neoform/1.21.1-20240808.144430/neoform-1.21.1-20240808.144430-mappings.txt`
fn coord_rel_path(coord: &str, ext: Option<&str>) -> Option<String> {
    let (coord, ext_in_coord) = match coord.split_once('@') {
        Some((c, e)) => (c, Some(e)),
        None => (coord, None),
    };
    let ext = ext.or(ext_in_coord).unwrap_or("jar");
    let parts: Vec<&str> = coord.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let classifier = parts.get(3).copied().filter(|c| !c.is_empty());
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Some(format!(
        "{}/{artifact}/{version}/{file}",
        group.replace('.', "/")
    ))
}

/// 从 installer jar 解出内部条目到 `dest`（zip 内名不含前导 `/`）。
fn extract_installer_entry(installer: &Path, entry: &str, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建临时目录失败\n{e}"))?;
    }
    let file = std::fs::File::open(installer).map_err(|e| format!("打开安装程序失败\n{e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("安装程序不是 zip\n{e}"))?;
    let mut member = archive
        .by_name(entry)
        .map_err(|_| format!("安装程序内没有 {entry}"))?;
    let mut out = std::fs::File::create(dest).map_err(|e| format!("创建文件失败\n{e}"))?;
    std::io::copy(&mut member, &mut out).map_err(|e| format!("解包 {entry} 失败\n{e}"))?;
    Ok(())
}

/// 参数/占位符解析结果：本地路径（写成字符串传参）或字面量。
#[derive(Debug, PartialEq, Eq)]
enum Resolved {
    Path(PathBuf),
    Literal(String),
}

impl Resolved {
    fn into_arg(self) -> String {
        match self {
            Resolved::Path(p) => p.display().to_string(),
            Resolved::Literal(s) => s,
        }
    }
}

/// data 值 → 路径或字面量（不联网；`[coord@ext]` 只算路径，构件按需另下）。
fn resolve_data(raw: &str, ctx: &ProcessorCtx<'_>) -> Resolved {
    let raw = raw.trim();
    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        return Resolved::Literal(raw[1..raw.len() - 1].to_string());
    }
    if let Some(inner) = raw.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        if let Some(rel) = coord_rel_path(inner, None) {
            return Resolved::Path(ctx.lib_dir.join(rel));
        }
    }
    if let Some(entry) = raw.strip_prefix('/') {
        // installer 内条目（如 /data/client.lzma）：解包到 work_dir 后返回该路径；
        // 解不出（不是 installer 内部条目）时按原始路径理解（绝对路径语义）
        let cached = ctx.work_dir.join(entry.replace('/', "_"));
        if cached.is_file() || extract_installer_entry(ctx.installer, entry, &cached).is_ok() {
            return Resolved::Path(cached);
        }
        return Resolved::Path(PathBuf::from(raw));
    }
    Resolved::Literal(raw.to_string())
}

/// 解析单个处理器参数：`{KEY}` 走 data，`[coord@ext]` 走输入构件（下载+按需解出成员）。
async fn resolve_arg(arg: &str, profile: &InstallProfile, ctx: &ProcessorCtx<'_>) -> Result<String, String> {
    if let Some(key) = arg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        return Ok(match key {
            "ROOT" => ctx.mc_dir.display().to_string(),
            "INSTALLER" => ctx.installer.display().to_string(),
            "MINECRAFT_JAR" => ctx.vanilla_jar.display().to_string(),
            "SIDE" => "client".to_string(),
            _ => {
                let raw = data_value(profile, key, "client")
                    .ok_or_else(|| format!("install_profile 缺少 data 项 {key}"))?;
                resolve_data(raw, ctx).into_arg()
            }
        });
    }
    if let Some(inner) = arg.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let path = ensure_artifact(inner, profile, ctx).await?;
        return Ok(path.display().to_string());
    }
    Ok(arg.to_string())
}

/// 确保构件存在于 `libraries/`（按 maven 布局），返回本地路径。
///
/// URL 优先取 profile.libraries 里同名（忽略 `@ext`）条目的官方地址与 sha1；
/// 否则按 `coord` 布局拼接官方 maven 基址，逐源镜像回退。
async fn ensure_artifact(
    coord: &str,
    profile: &InstallProfile,
    ctx: &ProcessorCtx<'_>,
) -> Result<PathBuf, String> {
    let (coord_base, ext) = match coord.split_once('@') {
        Some((c, e)) => (c, Some(e)),
        None => (coord, None),
    };
    let rel = coord_rel_path(coord, None)
        .ok_or_else(|| format!("构件坐标格式异常\n{coord}"))?;
    let dest = ctx.lib_dir.join(&rel);

    // 优先用官方声明的 path/url/sha1（名字里的 @ext 后缀要忽略后再比）
    let declared = profile.libraries.iter().find(|l| {
        let name = l.name.split('@').next().unwrap_or(&l.name);
        name == coord_base
    });
    let artifact = declared.and_then(|l| {
        l.downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
    });
    let dest = match artifact.and_then(|a| a.path.as_deref()) {
        Some(p) => ctx.lib_dir.join(p),
        None => dest,
    };

    let sha1 = artifact.and_then(|a| a.sha1.clone()).unwrap_or_default();
    if !sha1.is_empty() && crate::downloader::hash::file_sha1_matches(&dest, &sha1) {
        return finish_extract(dest, ext, ctx);
    }
    if sha1.is_empty() && dest.is_file() {
        return finish_extract(dest, ext, ctx);
    }

    let sources: Vec<&DownloadSource> = ctx.config.file_sources_list();
    let mut urls: Vec<String> = Vec::new();
    if let Some(u) = artifact.and_then(|a| a.url.clone()) {
        urls.push(u.clone());
        for s in &sources {
            let m = s.mirror_url(&u);
            if m != u && !urls.contains(&m) {
                urls.push(m);
            }
        }
    }
    if urls.is_empty() {
        // 官方声明缺失：按 maven 基址拼（NeoForge/Forge 的 maven 由源列表给出）
        for s in &sources {
            let base = s.maven_url.trim_end_matches('/');
            urls.push(format!("{base}/{rel}"));
        }
    }
    if urls.is_empty() {
        return Err(format!("无法确定构件下载地址\n{coord}"));
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建目录失败\n{}", e))?;
    }
    if sha1.is_empty() {
        fetcher::fetch_fallback(&urls, &dest, None::<fn(u64, u64)>)
            .await
            .map_err(|e| format!("下载构件失败（{coord}）\n{e}"))?;
    } else {
        fetcher::fetch_verified(&urls, &dest, None::<fn(u64, u64)>, &sha1)
            .await
            .map_err(|e| format!("下载构件失败（{coord}）\n{e}"))?;
    }
    finish_extract(dest, ext, ctx)
}

/// 若声明了 `@ext` 且与构件自身后缀不同，从构件（zip/jar）里解出该后缀的成员。
fn finish_extract(dest: PathBuf, ext: Option<&str>, ctx: &ProcessorCtx<'_>) -> Result<PathBuf, String> {
    let Some(ext) = ext else {
        return Ok(dest);
    };
    let own = dest.extension().and_then(|e| e.to_str()).unwrap_or("");
    // `@zip`/`@jar` 与构件自身后缀一致时无需解包（neoform 这类构件本身就是 zip）
    if own.eq_ignore_ascii_case(ext) {
        return Ok(dest);
    }
    let out = ctx.work_dir.join(format!(
        "{}.{}",
        dest.file_name().and_then(|n| n.to_str()).unwrap_or("artifact"),
        ext
    ));
    if out.is_file() {
        return Ok(out);
    }
    let file = std::fs::File::open(&dest).map_err(|e| format!("打开构件失败\n{}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("构件不是 zip（无法解出 .{ext}）\n{e}"))?;
    let suffix = format!(".{ext}");
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取构件条目失败\n{e}"))?;
        if entry.is_dir() || !entry.name().to_ascii_lowercase().ends_with(&suffix) {
            continue;
        }
        let mut out_file = std::fs::File::create(&out).map_err(|e| format!("创建文件失败\n{e}"))?;
        std::io::copy(&mut entry, &mut out_file).map_err(|e| format!("解出成员失败\n{e}"))?;
        return Ok(out);
    }
    bail!("构件内没有 .{ext} 成员\n{}", dest.display())
}

/// 读处理器 JAR 的主类（MANIFEST 的 `Main-Class`）。
fn main_class_of(jar: &Path) -> Result<String, String> {
    let file = std::fs::File::open(jar).map_err(|e| format!("打开处理器失败\n{}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("处理器不是 zip\n{e}"))?;
    let mut manifest = archive
        .by_name("META-INF/MANIFEST.MF")
        .map_err(|e| format!("处理器缺少 MANIFEST\n{e}"))?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut manifest, &mut text)
        .map_err(|e| format!("读取 MANIFEST 失败\n{e}"))?;
    // MANIFEST 每行可能折行（续行以空格开头），这里只取主类所在行
    for line in text.lines() {
        let line = line.trim_end();
        if let Some(v) = line.strip_prefix("Main-Class:") {
            return Ok(v.trim().to_string());
        }
    }
    bail!("处理器 MANIFEST 未声明 Main-Class\n{}", jar.display())
}

/// 按 profile 声明依次执行 **client 侧**处理器。
///
/// 幂等由调用方负责（例如 NeoForge 用 `{PATCHED}` 产物是否存在判断是否整体跳过）。
pub async fn run_client_processors(
    profile: &InstallProfile,
    ctx: &ProcessorCtx<'_>,
) -> Result<(), String> {
    if profile.processors.is_empty() {
        bail!("install_profile 未声明 processors（该版本可能只支持官方 installer 图形安装）");
    }
    std::fs::create_dir_all(ctx.work_dir).map_err(|e| format!("创建临时目录失败\n{e}"))?;
    let sep = if cfg!(windows) { ";" } else { ":" };
    let java = resolve_java()?;

    for (i, proc) in profile.processors.iter().enumerate() {
        if let Some(sides) = &proc.sides {
            if !sides.iter().any(|s| s == "client") {
                continue;
            }
        }
        // 主 JAR 单独解析：Main-Class 必须取自**处理器自己的 jar**，
        // 而不是 classpath 的末项（classpath 里还挂着传递依赖，末项常是 commons-*）。
        let main_jar = ensure_artifact(&proc.jar, profile, ctx).await?;
        let main = main_class_of(&main_jar)?;
        // classpath：声明项 + 主 JAR（去重、保序）
        let mut coords: Vec<&str> = proc.classpath.iter().map(String::as_str).collect();
        coords.push(proc.jar.as_str());
        let mut jars: Vec<PathBuf> = Vec::new();
        for c in coords {
            let p = ensure_artifact(c, profile, ctx).await?;
            if !jars.contains(&p) {
                jars.push(p);
            }
        }
        let classpath = jars
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(sep);
        let mut args: Vec<String> = Vec::with_capacity(proc.args.len());
        for a in &proc.args {
            args.push(resolve_arg(a, profile, ctx).await?);
        }

        let mut cmd = tokio::process::Command::new(&java);
        cmd.arg("-cp")
            .arg(&classpath)
            .arg(&main)
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let child = cmd
            .spawn()
            .map_err(|e| format!("启动安装器处理器失败（{}）\n{e}", proc.jar))?;
        let label = format!("安装器处理器 {}", main.rsplit('.').next().unwrap_or(&main));
        let output = run_installer_command(child, ctx.cancel, &label).await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let tail: Vec<&str> = stderr.lines().rev().take(15).collect();
            let tail = tail.iter().rev().cloned().collect::<Vec<_>>().join("\n");
            bail!(
                "ModLoader 安装失败\n步骤 {}/{}（{}）退出码 {:?}\n{}",
                i + 1,
                profile.processors.len(),
                proc.jar,
                output.status.code().unwrap_or(-1),
                tail
            );
        }
    }
    Ok(())
}

/// data 项（如 `PATCHED`）在本地对应的产物路径——用于幂等判断与启动期定位。
///
/// 只依赖 `libraries/` 与游戏目录，不需要先拿到 vanilla jar（幂等判断要在
/// 等待 vanilla 下载之前就能做完）。
pub fn output_path(
    profile: &InstallProfile,
    key: &str,
    lib_dir: &Path,
    mc_dir: &Path,
) -> Result<PathBuf, String> {
    let raw = data_value(profile, key, "client")
        .ok_or_else(|| format!("install_profile 缺少 data 项 {key}"))?;
    let raw = raw.trim();
    if let Some(inner) = raw.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        if let Some(rel) = coord_rel_path(inner, None) {
            return Ok(lib_dir.join(rel));
        }
    }
    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        return Ok(PathBuf::from(&raw[1..raw.len() - 1]));
    }
    if let Some(rel) = raw.strip_prefix('/') {
        return Ok(mc_dir.join(rel));
    }
    Ok(PathBuf::from(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coord_rel_path_matches_maven_layout() {
        assert_eq!(
            coord_rel_path("net.neoforged:neoform:1.21.1-20240808.144430:mappings@txt", None)
                .as_deref(),
            Some("net/neoforged/neoform/1.21.1-20240808.144430/neoform-1.21.1-20240808.144430-mappings.txt")
        );
        assert_eq!(
            coord_rel_path("net.neoforged.installertools:binarypatcher:2.1.2:fatjar@jar", None)
                .as_deref(),
            Some("net/neoforged/installertools/binarypatcher/2.1.2/binarypatcher-2.1.2-fatjar.jar")
        );
        // 无分类器 → 默认 .jar
        assert_eq!(
            coord_rel_path("cpw.mods:bootstraplauncher:2.0.2", None).as_deref(),
            Some("cpw/mods/bootstraplauncher/2.0.2/bootstraplauncher-2.0.2.jar")
        );
        assert!(coord_rel_path("bad-coord", None).is_none());
    }

    #[test]
    fn parse_profile_reads_sided_data_and_processors() {
        let json = r#"{
          "data": {
            "SIDE_LITERAL": "'1.21.1-x'",
            "BINPATCH": { "client": "/data/client.lzma", "server": "/data/server.lzma" }
          },
          "libraries": [
            { "name": "net.neoforged.installertools:binarypatcher:2.1.2:fatjar",
              "downloads": { "artifact": {
                "path": "net/neoforged/installertools/binarypatcher/2.1.2/binarypatcher-2.1.2-fatjar.jar",
                "url": "https://maven.neoforged.net/releases/x.jar",
                "sha1": "759b63ef393ed80418ec1ea4d233cd6152d02637" } } }
          ],
          "processors": [
            { "jar": "a:b:1", "classpath": ["a:b:1"], "args": ["--x", "{BINPATCH}"], "sides": ["server"] }
          ]
        }"#;
        let p = parse_profile(json).unwrap();
        assert_eq!(p.libraries.len(), 1);
        assert_eq!(p.processors.len(), 1);
        assert_eq!(data_value(&p, "SIDE_LITERAL", "client"), Some("'1.21.1-x'"));
        assert_eq!(data_value(&p, "BINPATCH", "client"), Some("/data/client.lzma"));
        assert_eq!(data_value(&p, "BINPATCH", "server"), Some("/data/server.lzma"));
    }

    #[test]
    fn resolve_data_handles_literal_installer_entry_and_coord() {
        let ctx = ProcessorCtx {
            installer: Path::new("C:/x/installer.jar"),
            mc_dir: Path::new("C:/mc"),
            vanilla_jar: Path::new("C:/mc/versions/v/v.jar"),
            lib_dir: Path::new("C:/mc/libraries"),
            work_dir: Path::new("C:/mc/.elytra/tmp"),
            config: &DownloaderConfig::default(),
            cancel: &Arc::new(AtomicBool::new(false)),
        };
        // 字面量（去引号）
        assert_eq!(
            resolve_data("'1.21.1-x'", &ctx),
            Resolved::Literal("1.21.1-x".into())
        );
        // installer 内条目（未解包时保持原样，由解包流程补上）
        assert_eq!(
            resolve_data("/data/client.lzma", &ctx),
            Resolved::Path(PathBuf::from("/data/client.lzma"))
        );
        // 构件坐标 → maven 本地布局（产物路径，不下载）
        assert_eq!(
            resolve_data("[net.neoforged:neoforge:21.1.248:client]", &ctx),
            Resolved::Path(PathBuf::from(
                "C:/mc/libraries/net/neoforged/neoforge/21.1.248/neoforge-21.1.248-client.jar"
            ))
        );
    }

    /// `{KEY}`/内置占位符解析（不触网）。
    #[tokio::test]
    async fn resolve_arg_handles_placeholders_without_network() {
        let profile = parse_profile(
            r#"{"data":{"MCP_VERSION":"'1.21.1-x'"},"libraries":[],"processors":[]}"#,
        )
        .unwrap();
        let ctx = ProcessorCtx {
            installer: Path::new("C:/x/installer.jar"),
            mc_dir: Path::new("C:/mc"),
            vanilla_jar: Path::new("C:/mc/versions/v/v.jar"),
            lib_dir: Path::new("C:/mc/libraries"),
            work_dir: Path::new("C:/mc/.elytra/tmp"),
            config: &DownloaderConfig::default(),
            cancel: &Arc::new(AtomicBool::new(false)),
        };
        assert_eq!(resolve_arg("{SIDE}", &profile, &ctx).await.unwrap(), "client");
        assert_eq!(resolve_arg("--version", &profile, &ctx).await.unwrap(), "--version");
        assert_eq!(
            resolve_arg("{MCP_VERSION}", &profile, &ctx).await.unwrap(),
            "1.21.1-x"
        );
        assert!(resolve_arg("{NOPE}", &profile, &ctx).await.is_err(), "缺 data 项应报错");
    }

    /// NeoForge 21.x 安装链端到端（**真实网络 + 真实 JDK**，默认 `#[ignore]`）。
    ///
    /// ```text
    /// JAVA_HOME=<jdk> cargo test --lib -- --ignored neoforge_21_chain_end_to_end --nocapture
    /// ```
    ///
    /// 覆盖：下载 installer → 解析其声明的 processors → 下载全部工具构件（sha1 校验）→
    /// 下载原版 client.jar → 按声明串行跑完整条链 → 校验 `{PATCHED}` 产物。
    /// 这是修 `Invalid bundle signature`（写死 `installertools 4.0.12` 配方）的验证。
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn neoforge_21_chain_end_to_end() {
        use crate::config::downloader::DownloaderConfig;
        use crate::downloader::{fetcher, hash};
        use crate::game::version::manifest::fetch_version_list;
        use crate::game::version::meta::fetch_version_json_text;

        const NEOFORGE_VERSION: &str = "21.1.248";
        const MC_VERSION: &str = "1.21.1";

        let cfg = DownloaderConfig::default();
        let root = std::env::temp_dir().join("elytra_e2e_neoforge");
        let _ = std::fs::remove_dir_all(&root);
        let mc_dir = root.join(".minecraft");
        let lib_dir = mc_dir.join("libraries");
        let work_dir = mc_dir.join(".elytra").join("tmp").join("loader");
        std::fs::create_dir_all(&lib_dir).expect("创建 libraries 失败");

        // 1) installer
        let installer = root.join(format!("neoforge-{NEOFORGE_VERSION}-installer.jar"));
        let source = cfg.file_primary_source().expect("无可用下载源");
        let url = source.url(&source.neoforge_installer_url, &[("ver", NEOFORGE_VERSION)]);
        fetcher::fetch_fallback(&[url], &installer, None::<fn(u64, u64)>)
            .await
            .expect("下载 installer 失败");

        let profile = parse_profile(
            &read_profile_from_installer(&installer).expect("读 install_profile 失败"),
        )
        .expect("解析 install_profile 失败");
        assert!(
            !profile.processors.is_empty(),
            "该 installer 应声明 processors（这正是被写死配方绕过的部分）"
        );
        eprintln!("installer 声明的处理器数：{}", profile.processors.len());

        // 2) vanilla client.jar（{MINECRAFT_JAR} 输入）
        let dir_name = format!("neoforge-{NEOFORGE_VERSION}");
        let vanilla = mc_dir
            .join("versions")
            .join(&dir_name)
            .join(format!("{dir_name}.jar"));
        std::fs::create_dir_all(vanilla.parent().unwrap()).unwrap();
        let manifest = fetch_version_list(&cfg).await.expect("拉取版本清单失败");
        let vurl = manifest
            .versions
            .iter()
            .find(|v| v.id == MC_VERSION)
            .expect("清单中找不到 1.21.1")
            .url
            .clone();
        let vjson = fetch_version_json_text(&vurl, &cfg)
            .await
            .expect("拉取版本 JSON 失败");
        let v: serde_json::Value = serde_json::from_str(&vjson).expect("解析版本 JSON 失败");
        let client = &v["downloads"]["client"];
        let (curl, csha1) = (
            client["url"].as_str().expect("缺少 client.url").to_string(),
            client["sha1"].as_str().expect("缺少 client.sha1").to_string(),
        );
        if !hash::file_sha1_matches(&vanilla, &csha1) {
            fetcher::fetch_verified(&[curl], &vanilla, None::<fn(u64, u64)>, &csha1)
                .await
                .expect("下载原版 client.jar 失败");
        }

        // 3) 按声明跑整条链
        let patched =
            output_path(&profile, "PATCHED", &lib_dir, &mc_dir).expect("profile 缺少 {PATCHED}");
        assert!(!patched.exists(), "临时目录应已清空，产物不该预先存在");

        let cancel = Arc::new(AtomicBool::new(false));
        let pctx = ProcessorCtx {
            installer: &installer,
            mc_dir: &mc_dir,
            vanilla_jar: &vanilla,
            lib_dir: &lib_dir,
            work_dir: &work_dir,
            config: &cfg,
            cancel: &cancel,
        };
        run_client_processors(&profile, &pctx)
            .await
            .expect("处理器链执行失败");

        // 4) 产物必须存在、够大、且是完整 zip/jar
        let size = patched
            .metadata()
            .unwrap_or_else(|e| panic!("未生成 PATCHED 产物（{}）：{e}", patched.display()))
            .len();
        assert!(size > 1_000_000, "产物过小：{size}");
        zip::ZipArchive::new(std::fs::File::open(&patched).expect("打开产物失败"))
            .expect("产物应是完整 zip/jar");
        eprintln!("✅ 已生成 {}（{size} 字节）", patched.display());
    }

    /// Forge 链端到端（**真实网络 + 真实 JDK**，默认 `#[ignore]`）。
    ///
    /// ```text
    /// JAVA_HOME=<jdk> cargo test --lib -- --ignored forge_chain_end_to_end --nocapture
    /// ```
    ///
    /// 覆盖：Forge 分支切到 processors 引擎后，能按 installer 声明（1.20.1-47.1.0 声明的是
    /// binarypatcher **1.1.1**/srgutils 0.4.x，而旧代码写死 1.3.4/0.6.6）跑完整条链并产出
    /// `{PATCHED}`（`forge-<ver>-client.jar`）。
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn forge_chain_end_to_end() {
        use crate::config::downloader::DownloaderConfig;
        use crate::downloader::{fetcher, hash};
        use crate::game::version::manifest::fetch_version_list;
        use crate::game::version::meta::fetch_version_json_text;

        const MC_VERSION: &str = "1.20.1";
        const FORGE_VERSION: &str = "47.1.0";

        let cfg = DownloaderConfig::default();
        let root = std::env::temp_dir().join("elytra_e2e_forge");
        let _ = std::fs::remove_dir_all(&root);
        let mc_dir = root.join(".minecraft");
        let lib_dir = mc_dir.join("libraries");
        let work_dir = mc_dir.join(".elytra").join("tmp").join("loader");
        std::fs::create_dir_all(&lib_dir).expect("创建 libraries 失败");

        // 1) installer（官方 maven）
        let installer = root.join(format!("forge-{MC_VERSION}-{FORGE_VERSION}-installer.jar"));
        let source = cfg.file_primary_source().expect("无可用下载源");
        let url = source.url(
            &source.forge_installer_url,
            &[("mc", MC_VERSION), ("ver", FORGE_VERSION)],
        );
        fetcher::fetch_fallback(&[url], &installer, None::<fn(u64, u64)>)
            .await
            .expect("下载 Forge installer 失败");

        let profile = parse_profile(
            &read_profile_from_installer(&installer).expect("读 install_profile 失败"),
        )
        .expect("解析 install_profile 失败");
        assert!(!profile.processors.is_empty(), "该 installer 应声明 processors");
        eprintln!("Forge installer 声明的处理器数：{}", profile.processors.len());

        // 2) vanilla client.jar
        let dir_name = format!("{MC_VERSION}-forge-{FORGE_VERSION}");
        let vanilla = mc_dir
            .join("versions")
            .join(&dir_name)
            .join(format!("{dir_name}.jar"));
        std::fs::create_dir_all(vanilla.parent().unwrap()).unwrap();
        let manifest = fetch_version_list(&cfg).await.expect("拉取版本清单失败");
        let vurl = manifest
            .versions
            .iter()
            .find(|v| v.id == MC_VERSION)
            .expect("清单中找不到 1.20.1")
            .url
            .clone();
        let vjson = fetch_version_json_text(&vurl, &cfg).await.expect("拉取版本 JSON 失败");
        let v: serde_json::Value = serde_json::from_str(&vjson).expect("解析版本 JSON 失败");
        let client = &v["downloads"]["client"];
        let (curl, csha1) = (
            client["url"].as_str().expect("缺少 client.url").to_string(),
            client["sha1"].as_str().expect("缺少 client.sha1").to_string(),
        );
        if !hash::file_sha1_matches(&vanilla, &csha1) {
            fetcher::fetch_verified(&[curl], &vanilla, None::<fn(u64, u64)>, &csha1)
                .await
                .expect("下载原版 client.jar 失败");
        }

        // 3) 跑声明链
        let patched =
            output_path(&profile, "PATCHED", &lib_dir, &mc_dir).expect("profile 缺少 {PATCHED}");
        eprintln!("期望产物：{}", patched.display());
        let cancel = Arc::new(AtomicBool::new(false));
        let pctx = ProcessorCtx {
            installer: &installer,
            mc_dir: &mc_dir,
            vanilla_jar: &vanilla,
            lib_dir: &lib_dir,
            work_dir: &work_dir,
            config: &cfg,
            cancel: &cancel,
        };
        run_client_processors(&profile, &pctx)
            .await
            .expect("Forge 处理器链执行失败");

        let size = patched
            .metadata()
            .unwrap_or_else(|e| panic!("未生成 PATCHED 产物（{}）：{e}", patched.display()))
            .len();
        assert!(size > 1_000_000, "产物过小：{size}");
        zip::ZipArchive::new(std::fs::File::open(&patched).expect("打开产物失败"))
            .expect("产物应是完整 zip/jar");
        eprintln!("✅ 已生成 {}（{size} 字节）", patched.display());
    }
}
