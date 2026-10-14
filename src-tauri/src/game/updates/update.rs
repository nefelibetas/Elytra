//! 更新检查与文件管理：按 MC 版本 + loader 取最新版、校验下载替换、删除/启停 mod。

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::Serialize;

use super::identify::file_sha1;
use super::scan::LocalModFile;
use super::identify::QUERY_CONCURRENCY;
use crate::game::community::{api_routes, api_urls, get_json_multi, ApiPlatform};
use crate::downloader::fetcher;

/// 单个 mod 的更新信息（命中 Modrinth 且有新版本时产出）。
#[derive(Debug, Clone, Serialize)]
pub struct ModUpdate {
    pub file_name: String,
    pub project_id: String,
    pub current_version: String,
    pub latest_version: String,
    /// 最新版本文件的直链（cdn.modrinth.com）
    pub file_url: String,
    /// 最新版本文件 sha1（下载校验用）
    pub file_sha1: String,
}

/// 查询兼容 MC 版本 + loader 的最新版本（按 date_published 降序，过滤后首个即最新）；
/// 返回 (version_number, file_url, file_sha1)。
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
        fetcher::http().get(u.to_string())
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

/// 删除 mods 目录下单个文件；仅接受纯文件名（防路径穿越），不存在报错。
pub fn delete_mod_file(mods_dir: &Path, file_name: &str) -> Result<(), String> {
    if !crate::util::is_simple_name(file_name) {
        return Err("非法文件名".to_string());
    }
    let path = mods_dir.join(file_name);
    // 禁用项在磁盘上是 `{file_name}.disabled`（启用态与它由 toggle_mod_file 互相重命名）
    let target = if path.exists() {
        path
    } else {
        let disabled = mods_dir.join(format!("{file_name}.disabled"));
        if !disabled.exists() {
            bail!("文件不存在\n{file_name}");
        }
        disabled
    };
    std::fs::remove_file(&target).map_err(common_err_ctx!("删除失败"))?;
    Ok(())
}

/// 删除资源目录（resourcepacks/datapacks/shaderpacks）下单个文件；kind 白名单 + 纯文件名（防穿越），不存在报错。
pub async fn delete_resource_file(
    version_dir: &Path,
    kind: &str,
    file_name: &str,
) -> Result<(), String> {
    if !matches!(kind, "resourcepacks" | "datapacks" | "shaderpacks") {
        return Err("未知资源类型".to_string());
    }
    if !crate::util::is_simple_name(file_name) {
        return Err("非法文件名".to_string());
    }
    let path = version_dir.join(kind).join(file_name);
    if !path.is_file() {
        bail!("文件不存在\n{file_name}");
    }
    tokio::fs::remove_file(&path)
        .await
        .map_err(common_err_ctx!("删除失败"))
}

/// 切换 mod 启用/禁用：`name.jar` → `name.jar.disabled`（扫描按扩展名自动隐藏），反向即启用。
/// 仅接受纯文件名（防穿越），不存在报错；返回 true=已禁用，false=已启用。
pub fn toggle_mod_file(mods_dir: &Path, file_name: &str) -> Result<bool, String> {
    if !crate::util::is_simple_name(file_name) {
        return Err("非法文件名".to_string());
    }
    let path = mods_dir.join(file_name);
    if path.exists() && file_name.ends_with(".jar") {
        let disabled = mods_dir.join(format!("{file_name}.disabled"));
        std::fs::rename(&path, &disabled).map_err(common_err_ctx!("禁用失败"))?;
        return Ok(true);
    }
    let disabled = mods_dir.join(format!("{file_name}.disabled"));
    if disabled.exists() {
        std::fs::rename(&disabled, &path).map_err(common_err_ctx!("启用失败"))?;
        return Ok(false);
    }
    Err(format!("Mod 文件不存在\n{file_name}"))
}

/// 检查一批本地 mod 更新（Modrinth）；命中但无新版本/不在 Modrinth 的不进结果。
pub async fn check_updates(
    mods_dir: &Path,
    files: &[LocalModFile],
    mc_version: &str,
    loader: &str,
) -> Vec<ModUpdate> {
    let dir = mods_dir.to_path_buf();
    let mc = mc_version.to_string();
    let ld = loader.to_string();
    // 提取拥有所有权的输入：借用 slice 时 async block 返回类型会绑定该引用生命周期（HRTB 报错）；
    // mc/ld 必须随 tuple move，否则被多个 async block 捕获冲突（E0507）
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
                fetcher::http().get(u.to_string())
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

/// 下载更新替换旧文件：先下到同目录临时文件（逐源校验 sha1，失败不触碰旧文件），通过后删旧再改名。
pub async fn update_mod_file(
    mods_dir: &Path,
    file_name: &str,
    file_url: &str,
    file_sha1: &str,
) -> Result<(), String> {
    // 同 delete/toggle：文件名不得含分隔符或 `..`，否则 join 会越界写删。
    if !crate::util::is_simple_name(file_name) {
        bail!("非法的文件名");
    }
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
    .map_err(common_err_ctx!("下载更新失败"))?;

    // 先删旧再改名（Windows 上 rename 覆盖已存在目标会失败）
    if target.exists() {
        std::fs::remove_file(&target).map_err(common_err_ctx!("删除旧文件失败"))?;
    }
    std::fs::rename(&tmp, &target).map_err(common_err_ctx!("替换文件失败"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_mod_file_disables_and_enables() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("a.jar");
        std::fs::write(&jar, b"x").unwrap();

        assert!(toggle_mod_file(dir.path(), "a.jar").unwrap());
        assert!(!jar.exists());
        assert!(
            dir.path().join("a.jar.disabled").exists(),
            "禁用应改名 .disabled"
        );

        // 重复禁用 → 走启用分支
        assert!(
            !toggle_mod_file(dir.path(), "a.jar").unwrap(),
            "应回退为启用"
        );
        assert!(jar.exists());
        assert!(!dir.path().join("a.jar.disabled").exists());

        assert!(
            toggle_mod_file(dir.path(), "missing.jar").is_err(),
            "不存在应报错"
        );

        assert!(
            toggle_mod_file(dir.path(), "../b.jar").is_err(),
            "穿越路径拒绝"
        );
    }

    #[test]
    fn delete_mod_file_removes_and_guards() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("a.jar");
        std::fs::write(&jar, b"x").unwrap();

        delete_mod_file(dir.path(), "a.jar").unwrap();
        assert!(!jar.exists());

        assert!(
            delete_mod_file(dir.path(), "a.jar").is_err(),
            "文件不存在应报错"
        );

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

    /// 禁用项在磁盘上是 `{name}.disabled`：传 UI 展示的原始文件名也应能删掉（否则禁用项无法删除）。
    #[test]
    fn delete_mod_file_removes_disabled_variant() {
        let dir = tempfile::tempdir().unwrap();
        let disabled = dir.path().join("a.jar.disabled");
        std::fs::write(&disabled, b"x").unwrap();

        delete_mod_file(dir.path(), "a.jar").unwrap();
        assert!(!disabled.exists(), "传原名应删掉 .disabled 变体");

        assert!(
            delete_mod_file(dir.path(), "a.jar").is_err(),
            "两种形态都不存在时应报错"
        );
    }
}
