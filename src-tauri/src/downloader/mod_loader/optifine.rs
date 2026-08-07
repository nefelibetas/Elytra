// ── OptiFine 安装：下载 OptiFine jar + 生成 minimal version.json ──

use crate::config::downloader::model::DownloaderConfig;
use crate::downloader::fetcher;

pub async fn install_optifine(
    mc_version: &str,
    patch: &str,
    of_type: &str,
    config: &DownloaderConfig,
) -> Result<(), String> {
    let config = config.clone();
    let source = config
        .file_primary_source()
        .ok_or("no active source")?
        .clone();
    let mc = config.minecraft_dir_abs();
    let dir_name = format!("{}-optifine-{}", mc_version, patch);
    let vdir = mc.join("versions").join(&dir_name);

    let filename = format!("OptiFine_{}_{}_{}.jar", mc_version, of_type, patch);
    let jar_url = source.url(
        &source.optifine_download_url,
        &[("mc", mc_version), ("type", of_type), ("patch", patch)],
    );

    std::fs::create_dir_all(&vdir).map_err(|e| format!("创建目录失败: {}", e))?;
    let jar_path = vdir.join(&filename);
    fetcher::fetch(&jar_url, &jar_path, None::<fn(u64, u64)>)
        .await
        .map_err(|e| format!("下载 OptiFine 失败: {}", e))?;

    let json_path = vdir.join(format!("{}.json", dir_name));
    let minimal = serde_json::json!({
        "id": dir_name,
        "mainClass": "net.minecraft.launchwrapper.Launch",
        "libraries": [{
            "name": format!("optifine:OptiFine:{}", patch),
            "downloads": {
                "artifact": {
                    "path": format!("{}/{}", dir_name, filename),
                    "url": format!("file:///{}", jar_path.display().to_string().replace("\\", "/")),
                    "sha1": "", "size": 0
                }
            }
        }],
        "inheritsFrom": mc_version,
        "type": "release"
    });
    std::fs::write(
        &json_path,
        serde_json::to_string_pretty(&minimal).map_err(|e| format!("序列化失败: {}", e))?,
    )
    .map_err(|e| format!("写入 version.json 失败: {}", e))?;

    Ok(())
}
