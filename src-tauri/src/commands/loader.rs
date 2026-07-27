use serde::Deserialize;

#[derive(Deserialize)]
struct ForgeVersion {
    version: String,
}

#[derive(Deserialize)]
struct FabricEntry {
    loader: FabricLoader,
}

#[derive(Deserialize)]
struct FabricLoader {
    version: String,
}

#[derive(Deserialize)]
struct OptiFineEntry {
    patch: String,
    filename: String,
}

#[derive(Deserialize)]
struct NeoForgeEntry {
    version: String,
}

#[tauri::command]
pub async fn get_forge_versions(mc_version: String) -> Result<Vec<String>, String> {
    let url = format!("https://bmclapi2.bangbang93.com/forge/minecraft/{}", mc_version);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("请求 Forge 版本列表失败: {}", e))?;
    let data: Vec<ForgeVersion> = resp
        .json()
        .await
        .map_err(|e| format!("解析 Forge 版本列表失败: {}", e))?;
    let mut versions: Vec<String> = data.into_iter().map(|v| v.version).collect();
    versions.reverse();
    Ok(versions)
}

#[tauri::command]
pub async fn get_fabric_versions(mc_version: String) -> Result<Vec<String>, String> {
    let url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{}",
        mc_version
    );
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("请求 Fabric 版本列表失败: {}", e))?;
    let data: Vec<FabricEntry> = resp
        .json()
        .await
        .map_err(|e| format!("解析 Fabric 版本列表失败: {}", e))?;
    Ok(data.into_iter().map(|e| e.loader.version).collect())
}

#[tauri::command]
pub async fn get_optifine_versions(mc_version: String) -> Result<Vec<String>, String> {
    let url = format!("https://bmclapi2.bangbang93.com/optifine/{}", mc_version);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("请求 OptiFine 版本列表失败: {}", e))?;
    let data: Vec<OptiFineEntry> = resp
        .json()
        .await
        .map_err(|e| format!("解析 OptiFine 版本列表失败: {}", e))?;
    let versions: Vec<String> = data
        .into_iter()
        .filter(|e| !e.filename.starts_with("preview_"))
        .map(|e| e.patch)
        .collect();
    Ok(versions)
}

#[tauri::command]
pub async fn get_neoforge_versions(mc_version: String) -> Result<Vec<String>, String> {
    let url = format!("https://bmclapi2.bangbang93.com/neoforge/list/{}", mc_version);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("请求 NeoForge 版本列表失败: {}", e))?;
    let data: Vec<NeoForgeEntry> = resp
        .json()
        .await
        .map_err(|e| format!("解析 NeoForge 版本列表失败: {}", e))?;
    let prefix = format!("{}-", mc_version);
    let versions: Vec<String> = data
        .into_iter()
        .filter_map(|e| {
            let v = e.version;
            if v.starts_with(&prefix) {
                Some(v[prefix.len()..].to_string())
            } else if v == mc_version {
                None
            } else {
                Some(v)
            }
        })
        .collect();
    Ok(versions)
}
