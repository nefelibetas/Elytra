//! 更新检查与文件管理：按 MC 版本 + loader 过滤最新版本、校验下载替换、删除单个 mod 文件。

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::Serialize;

use super::identify::file_sha1;
use super::scan::LocalModFile;
use super::QUERY_CONCURRENCY;
use crate::downloader::community::{api_routes, api_urls, get_json_multi, ApiPlatform};
use crate::downloader::fetcher;

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

/// 查询项目在兼容 MC 版本 + loader 下的最新版本（Modrinth 返回按 date_published 降序，
/// 过滤后取首个即最新）。返回 (version_number, file_url, file_sha1)。
async fn latest_version(
    project_id: &str,
    mc_version: &str,
    loader: &str,
) -> Option<(String, String, String)> {
    let game_versions = serde_json::json!([mc_version]).to_string();
    let loaders = serde_json::json!([loader]).to_string();
    let urls: Vec<String> = api_urls(
        ApiPlatform::Modrinth,
        &format!("/v2/project/{project_id}/version"),
        &[
            ("game_versions", game_versions.clone()),
            ("loaders", loaders.clone()),
        ],
    )
    .ok()?;
    let data = get_json_multi(&urls, |u| {
        crate::downloader::fetcher::http().get(u.to_string())
    })
    .await
    .ok()?;
    let arr = data.as_array()?;
    let first = arr.iter().find(|v| {
        v["version_number"].as_str().is_some()
            && v["files"]
                .as_array()
                .map(|f| !f.is_empty())
                .unwrap_or(false)
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

    let results = futures_util::stream::iter(inputs.into_iter().map(
        |(file_name, dir, mc, ld)| async move {
            let sha1 = file_sha1(dir.join(&file_name)).await?;
            let urls = api_routes(
                ApiPlatform::Modrinth,
                &format!("/v2/version_file/{sha1}?algorithm=sha1"),
            );
            let cur = get_json_multi(&urls, |u| {
                crate::downloader::fetcher::http().get(u.to_string())
            })
            .await
            .ok()?;
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
        },
    ))
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
    fn delete_mod_file_removes_and_guards() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("a.jar");
        std::fs::write(&jar, b"x").unwrap();

        // 正常删除
        delete_mod_file(dir.path(), "a.jar").unwrap();
        assert!(!jar.exists());

        // 重复删除报错
        assert!(
            delete_mod_file(dir.path(), "a.jar").is_err(),
            "文件不存在应报错"
        );

        // 路径穿越拒绝
        std::fs::write(dir.path().join("b.jar"), b"y").unwrap();
        assert!(
            delete_mod_file(dir.path(), "../b.jar").is_err(),
            "穿越路径拒绝"
        );
        assert!(
            delete_mod_file(dir.path(), "sub\\b.jar").is_err(),
            "反斜杠拒绝"
        );
        assert!(dir.path().join("b.jar").exists(), "拒绝后文件保留");
    }
}
