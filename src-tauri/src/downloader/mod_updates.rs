//! 本地 Mod 扫描与更新检查（Modrinth 官方 API）。
//! 更新链路：本地 mod jar → sha1 → Modrinth `/v2/version_file/{sha1}` 识别项目与当前版本
//! → `/v2/project/{id}/version`（按 MC 版本 + loader 过滤）取最新版 → 版本号不同即标记可更新。
//! 仅支持 Modrinth 源（sha1 识别项目）；CurseForge 无公开 hash 查询通道，命中不了静默跳过。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use futures_util::StreamExt;

use crate::downloader::fetcher;
use crate::downloader::hash::sha1_hex;

const MODRINTH_API: &str = "https://api.modrinth.com";
/// 交互查询超时（对齐 community.rs 的 QUERY_TIMEOUT_SECS，搜索类请求不该挂 5 分钟）
const QUERY_TIMEOUT_SECS: u64 = 10;
/// sha1 → 项目查询并发上限（Modrinth 限 300 req/min，个人 mods 量级安全）
const QUERY_CONCURRENCY: usize = 8;

/// 本地 mod 文件信息（mods 目录扫描结果）。
#[derive(Debug, Clone, Serialize)]
pub struct LocalModFile {
    pub file_name: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 最后修改时间（unix 秒）
    pub last_modified: u64,
}

/// 本地 mod 详细列表项：Modrinth 命中时带项目元数据（图标/标题/描述/下载量/当前版本），
/// 未命中或网络失败时仅本地信息（其余字段 None）。
#[derive(Debug, Clone, Serialize)]
pub struct ModFileDetail {
    pub file_name: String,
    pub size: u64,
    pub last_modified: u64,
    pub project_id: Option<String>,
    pub title: Option<String>,
    /// MC百科内置条目表提供的中文译名（slug 匹配；未收录为 None）
    pub title_zh: Option<String>,
    pub description: Option<String>,
    /// MCIM 翻译接口的中文简介（失败/未收录为 None）
    pub description_zh: Option<String>,
    pub icon_url: Option<String>,
    pub downloads: Option<u64>,
    pub current_version: Option<String>,
}

/// 详细列表结果：任一网络环节失败时 mods 仍含本地信息，network_error 供前端提示降级。
#[derive(Debug, Clone, Serialize)]
pub struct ModListResult {
    pub mods: Vec<ModFileDetail>,
    pub network_error: bool,
}

/// 扫描 mods 目录下的 .jar 文件（按文件名排序）。
pub fn scan_mod_files(mods_dir: &Path) -> Vec<LocalModFile> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("jar") {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let meta = match e.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        out.push(LocalModFile {
            file_name: file_name.to_string(),
            size: meta.len(),
            last_modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
    }
    out.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    out
}

/// 识别单个 mod：sha1 → Modrinth version_file，返回 (project_id, current_version)。
/// Ok(Some(..)) 命中；Ok(None) 不在 Modrinth（HTTP 404）；Err 网络/服务错误。
async fn identify_mod(dir: PathBuf, file_name: String) -> Result<Option<(String, String)>, ()> {
    let sha1 = file_sha1(dir.join(&file_name)).await.ok_or(())?;
    let url = format!(
        "{}/v2/version_file/{}?algorithm=sha1",
        MODRINTH_API, sha1
    );
    match get_json(&url).await {
        Ok(cur) => {
            let project_id = cur["project_id"].as_str().map(str::to_string);
            let current_version = cur["version_number"].as_str().map(str::to_string);
            Ok(match (project_id, current_version) {
                (Some(pid), Some(ver)) => Some((pid, ver)),
                _ => None,
            })
        }
        Err(e) => {
            // HTTP 404 = 该文件不在 Modrinth（正常未命中）；其余视为网络/服务错误
            if e.contains("HTTP 404") {
                Ok(None)
            } else {
                Err(())
            }
        }
    }
}

/// 扫描 + Modrinth 元数据识别（sha1 → version_file 逐文件 → /v2/projects 批量详情）。
/// 网络环节失败不阻塞：降级为纯本地列表并置 network_error。
pub async fn list_mods_detailed(mods_dir: &Path) -> ModListResult {
    let files = scan_mod_files(mods_dir);
    if files.is_empty() {
        return ModListResult {
            mods: Vec::new(),
            network_error: false,
        };
    }

    let dir = mods_dir.to_path_buf();
    let inputs: Vec<(String, PathBuf)> = files
        .iter()
        .map(|f| (f.file_name.clone(), dir.clone()))
        .collect();
    let identified: Vec<Result<Option<(String, String)>, ()>> =
        futures_util::stream::iter(inputs.into_iter().map(|(file_name, dir)| async move {
            identify_mod(dir, file_name).await
        }))
        .buffer_unordered(QUERY_CONCURRENCY)
        .collect()
        .await;

    // 网络/服务错误（含 version_file 查询失败）→ 置降级标记；404 未命中是合法降级
    let network_error = identified.iter().any(|r| r.is_err());

    let hits: Vec<Option<(String, String)>> = identified
        .into_iter()
        .map(|r| r.unwrap_or(None))
        .collect();

    // 项目元数据：project_id → (title, slug, description, icon_url, downloads)
    // 批量详情请求失败也算网络错误（命中项因此缺元数据）
    let meta = fetch_project_meta(&hits).await;
    let network_error = network_error || (meta.is_none() && hits.iter().any(Option::is_some));

    // 中文简介（MCIM 翻译，复用 community 通道；失败/未收录静默降级为英文原文）
    let ids: Vec<String> = hits
        .iter()
        .flatten()
        .map(|(pid, _)| pid.clone())
        .collect();
    let translated = crate::downloader::community::fetch_translations("modrinth", &ids).await;

    let mods = files
        .into_iter()
        .zip(hits)
        .map(|(f, hit)| match hit {
            Some((project_id, current_version)) => {
                let p = meta.as_ref().and_then(|m| m.get(&project_id));
                // MC百科内置表按 slug 查中文译名（与社区搜索结果同源）
                let title_zh = p
                    .and_then(|v| v["slug"].as_str())
                    .and_then(|slug| {
                        crate::downloader::wiki_entry::WIKI
                            .lookup("modrinth", slug)
                            .and_then(|e| e.chinese_raw.as_deref())
                            .and_then(crate::downloader::wiki_entry::chinese_name)
                    });
                ModFileDetail {
                    file_name: f.file_name,
                    size: f.size,
                    last_modified: f.last_modified,
                    project_id: Some(project_id.clone()),
                    title: p.and_then(|v| v["title"].as_str().map(str::to_string)),
                    title_zh,
                    description: p
                        .and_then(|v| v["description"].as_str().map(str::to_string)),
                    description_zh: translated.get(&project_id).cloned(),
                    icon_url: p.and_then(|v| v["icon_url"].as_str().map(str::to_string)),
                    downloads: p.and_then(|v| v["downloads"].as_u64()),
                    current_version: Some(current_version),
                }
            }
            None => ModFileDetail {
                file_name: f.file_name,
                size: f.size,
                last_modified: f.last_modified,
                project_id: None,
                title: None,
                title_zh: None,
                description: None,
                description_zh: None,
                icon_url: None,
                downloads: None,
                current_version: None,
            },
        })
        .collect();

    ModListResult { mods, network_error }
}

/// 批量拉取项目元数据（GET /v2/projects?ids=[...]）；空 ids 或请求失败返回 None。
async fn fetch_project_meta(
    identified: &[Option<(String, String)>],
) -> Option<HashMap<String, serde_json::Value>> {
    let ids: Vec<&String> = identified
        .iter()
        .flatten()
        .map(|(pid, _)| pid)
        .collect();
    if ids.is_empty() {
        return None;
    }
    let ids_json = serde_json::json!(ids).to_string();
    let url = reqwest::Url::parse_with_params(
        &format!("{}/v2/projects", MODRINTH_API),
        &[("ids", ids_json)],
    )
    .ok()?;
    let data = get_json(url.as_str()).await.ok()?;
    let mut map = HashMap::new();
    if let Some(arr) = data.as_array() {
        for v in arr {
            if let Some(id) = v["id"].as_str() {
                map.insert(id.to_string(), v.clone());
            }
        }
    }
    Some(map)
}

/// 单个 mod 的更新信息（check_updates 命中 Modrinth 且存在新版本时产出）。
#[derive(Debug, Clone, Serialize)]
pub struct ModUpdate {
    pub file_name: String,
    pub project_id: String,
    pub current_version: String,
    pub latest_version: String,
    /// 最新版本文件的直链（cdn.modrinth.com）
    pub file_url: String,
    /// 最新版本文件的 sha1（下载校验用）
    pub file_sha1: String,
}

/// JSON 交互请求（10s 超时）：直接 GET 解析，不做重试/换源（更新检查失败即返回）。
async fn get_json(url: &str) -> Result<serde_json::Value, String> {
    let resp = fetcher::http()
        .get(url)
        .timeout(std::time::Duration::from_secs(QUERY_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| format!("请求失败\n{}", e))?;
    if !resp.status().is_success() {
        return Err(format!("请求失败\nHTTP {}", resp.status()));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("解析 JSON 失败\n{}", e))
}

/// 读文件并计算 sha1（文件 IO 放 spawn_blocking，避免阻塞 async executor）。
async fn file_sha1(path: PathBuf) -> Option<String> {
    tokio::task::spawn_blocking(move || {
        std::fs::read(&path).ok().map(|d| sha1_hex(&d))
    })
    .await
    .ok()
    .flatten()
}

/// 查询项目在兼容 MC 版本 + loader 下的最新版本（Modrinth 返回按 date_published 降序，
/// 过滤后取首个即最新）。返回 (version_number, file_url, file_sha1)。
async fn latest_version(
    project_id: &str,
    mc_version: &str,
    loader: &str,
) -> Option<(String, String, String)> {
    let game_versions = serde_json::json!([mc_version]).to_string();
    let loaders = serde_json::json!([loader]).to_string();
    let url = reqwest::Url::parse_with_params(
        &format!("{}/v2/project/{}/version", MODRINTH_API, project_id),
        &[
            ("game_versions", game_versions),
            ("loaders", loaders),
        ],
    )
    .ok()?;
    let data = get_json(url.as_str()).await.ok()?;
    let arr = data.as_array()?;
    let first = arr.iter().find(|v| {
        v["version_number"].as_str().is_some()
            && v["files"].as_array().map(|f| !f.is_empty()).unwrap_or(false)
    })?;
    let version_number = first["version_number"].as_str()?.to_string();
    let file = first["files"].as_array()?.first()?;
    let file_url = file["url"].as_str()?.to_string();
    let file_sha1 = file["hashes"]["sha1"].as_str()?.to_string();
    Some((version_number, file_url, file_sha1))
}

/// 删除 mods 目录下的单个 mod 文件（Mod 管理删除按钮）。
/// 仅接受纯文件名（防路径穿越），不存在报错。
pub fn delete_mod_file(mods_dir: &Path, file_name: &str) -> Result<(), String> {
    if file_name.contains('/')
        || file_name.contains('\\')
        || file_name.contains("..")
        || file_name.is_empty()
    {
        return Err("非法文件名".to_string());
    }
    let path = mods_dir.join(file_name);
    if !path.exists() {
        return Err(format!("文件不存在\n{file_name}"));
    }
    std::fs::remove_file(&path).map_err(|e| format!("删除失败\n{e}"))?;
    Ok(())
}

/// 检查一批本地 mod 是否有可用更新（Modrinth 源）。
/// 命中项目但无新版本 / 不在 Modrinth 的 mod 不进入结果。
pub async fn check_updates(
    mods_dir: &Path,
    files: &[LocalModFile],
    mc_version: &str,
    loader: &str,
) -> Vec<ModUpdate> {
    let dir = mods_dir.to_path_buf();
    let mc = mc_version.to_string();
    let ld = loader.to_string();
    // 先提取拥有所有权的输入：闭包参数借用 slice 时 async block 的返回类型
    // 会被推断绑定到该引用生命周期（HRTB 报错）；mc/ld 也必须随 tuple move，
    // 否则被多个 async block 捕获冲突（E0507）
    let inputs: Vec<(String, PathBuf, String, String)> = files
        .iter()
        .map(|m| (m.file_name.clone(), dir.clone(), mc.clone(), ld.clone()))
        .collect();

    let results = futures_util::stream::iter(
        inputs.into_iter().map(|(file_name, dir, mc, ld)| async move {
            let sha1 = file_sha1(dir.join(&file_name)).await?;
            let url = format!(
                "{}/v2/version_file/{}?algorithm=sha1",
                MODRINTH_API, sha1
            );
            let cur = get_json(&url).await.ok()?;
            let project_id = cur["project_id"].as_str()?.to_string();
            let current_version = cur["version_number"].as_str()?.to_string();
            if current_version.is_empty() {
                return None;
            }
            let (latest_version, file_url, file_sha1) =
                latest_version(&project_id, &mc, &ld).await?;
            if latest_version == current_version {
                return None;
            }
            Some(ModUpdate {
                file_name,
                project_id,
                current_version,
                latest_version,
                file_url,
                file_sha1,
            })
        }
    ),
)
    .buffer_unordered(QUERY_CONCURRENCY)
    .collect::<Vec<_>>()
    .await;

    results.into_iter().flatten().collect()
}

/// 下载更新文件并替换 mods 目录中的旧文件。
/// 先下载到同目录临时文件（fetch_verified 逐源校验 sha1，失败不触碰旧文件），
/// 校验通过后删旧文件再原子改名。
pub async fn update_mod_file(
    mods_dir: &Path,
    file_name: &str,
    file_url: &str,
    file_sha1: &str,
) -> Result<(), String> {
    let target = mods_dir.join(file_name);
    let tmp = mods_dir.join(format!(".elytra-update-{file_name}"));
    // 清理上次残留的临时文件（幂等重试）
    let _ = std::fs::remove_file(&tmp);

    fetcher::fetch_verified(
        &[file_url.to_string()],
        &tmp,
        None::<fn(u64, u64)>,
        file_sha1,
    )
    .await
    .map_err(|e| format!("下载更新失败\n{e}"))?;

    // 替换：先删旧文件再改名（Windows 上 rename 覆盖已存在目标会失败）
    if target.exists() {
        std::fs::remove_file(&target).map_err(|e| format!("删除旧文件失败\n{e}"))?;
    }
    std::fs::rename(&tmp, &target).map_err(|e| format!("替换文件失败\n{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_mod_files_only_jars() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.jar"), b"a").unwrap();
        std::fs::write(dir.path().join("b.jar"), b"bb").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"readme").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("c.jar"), b"c").unwrap();

        let files = scan_mod_files(dir.path());
        let names: Vec<&str> = files.iter().map(|f| f.file_name.as_str()).collect();
        // 仅顶层 .jar、按文件名排序、不含子目录
        assert_eq!(names, vec!["a.jar", "b.jar"]);
        assert_eq!(files[0].size, 1);
        assert_eq!(files[1].size, 2);
    }

    #[test]
    fn scan_mod_files_missing_dir_empty() {
        let dir = tempfile::tempdir().unwrap();
        let none = dir.path().join("no-such-dir");
        assert!(scan_mod_files(&none).is_empty());
    }

    #[test]
    fn delete_mod_file_removes_and_guards() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("a.jar");
        std::fs::write(&jar, b"x").unwrap();

        // 正常删除
        delete_mod_file(dir.path(), "a.jar").unwrap();
        assert!(!jar.exists());

        // 重复删除报错
        assert!(delete_mod_file(dir.path(), "a.jar").is_err(), "文件不存在应报错");

        // 路径穿越拒绝
        std::fs::write(dir.path().join("b.jar"), b"y").unwrap();
        assert!(delete_mod_file(dir.path(), "../b.jar").is_err(), "穿越路径拒绝");
        assert!(delete_mod_file(dir.path(), "sub\\b.jar").is_err(), "反斜杠拒绝");
        assert!(dir.path().join("b.jar").exists(), "拒绝后文件保留");
    }
}
