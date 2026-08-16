// ── OptiFine 安装：下载 OptiFine jar + 生成 minimal version.json ──

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::downloader::model::DownloaderConfig;
use crate::downloader::fetcher;

#[allow(clippy::too_many_arguments)]
pub async fn install_optifine(
    mc_version: &str,
    patch: &str,
    of_type: &str,
    dir_name: &str,
    vdir: &std::path::Path,
    config: &DownloaderConfig,
    bytes: &Arc<AtomicU64>,
    step_bytes: &Arc<AtomicU64>,
    step_total: &Arc<AtomicU64>,
) -> Result<(), String> {
    let config = config.clone();
    let source = config
        .file_primary_source()
        .ok_or("no active source")?
        .clone();

    let filename = format!("OptiFine_{}_{}_{}.jar", mc_version, of_type, patch);
    let jar_url = source.url(
        &source.optifine_download_url,
        &[("mc", mc_version), ("type", of_type), ("patch", patch)],
    );

    std::fs::create_dir_all(vdir).map_err(|e| format!("创建目录失败\n{}", e))?;
    let jar_path = vdir.join(&filename);
    // 字节级进度：增量累加进全局计数（收尾心跳显示速度）与 OptiFine 步骤计数；
    // total（content-length）写入步骤分母
    let file_done = Arc::new(AtomicU64::new(0));
    let progress = {
        let fd = file_done.clone();
        let b = bytes.clone();
        let sb = step_bytes.clone();
        let st = step_total.clone();
        move |downloaded: u64, total: u64| {
            let prev = fd.swap(downloaded, Ordering::Relaxed);
            let delta = downloaded.saturating_sub(prev);
            b.fetch_add(delta, Ordering::Relaxed);
            sb.fetch_add(delta, Ordering::Relaxed);
            if total > 0 {
                st.store(total, Ordering::Relaxed);
            }
        }
    };
    fetcher::fetch(&jar_url, &jar_path, Some(progress))
        .await
        .map_err(|e| format!("下载 OptiFine 失败\n{}", e))?;

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
        serde_json::to_string_pretty(&minimal).map_err(|e| format!("序列化失败\n{}", e))?,
    )
    .map_err(|e| format!("写入 version.json 失败\n{}", e))?;

    Ok(())
}
