//! 版本图标命令：读/写/还原 versions/{dir} 下的图标。
//! 优先级：用户自定义（version-icon.png）> 整合包项目图（pack-icon.png）；目录名守卫防路径穿越。

use std::path::PathBuf;

use serde::Serialize;

use crate::game::version::dir::version_dir_path;
use crate::game::version::icon::{version_icon_path, CUSTOM_ICON_NAME};
use crate::util;

/// 版本图标读取上限（平台 logo / 用户图片一般 <256KB，留余量）。
const MAX_PACK_ICON_BYTES: u64 = 1024 * 1024;

/// 校验版本目录名：仅接受简单目录名，防路径穿越。
fn ensure_simple_dir_name(name: &str) -> Result<(), String> {
    if !util::is_simple_name(name) {
        bail_key!("errors.version.dirNameInvalid", "版本目录名不合法\n{name}");
    }
    Ok(())
}

/// 读取版本图标载荷（自定义 > 整合包；都没有返回 None，UI 回退 loader 图标）。
#[tauri::command]
pub async fn get_version_icon(version_dir_name: String) -> Option<util::IconData> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_simple_dir_name(&version_dir_name).ok()?;
        let vdir = version_dir_path(&version_dir_name).ok()?;
        util::icon_payload(&version_icon_path(&vdir)?, MAX_PACK_ICON_BYTES)
    })
    .await
    .unwrap_or_default()
}

/// 图标来源："custom"=用户自定义 / "pack"=整合包项目图 / "default"=无（loader 图标）。
#[derive(Debug, Clone, Serialize)]
pub struct VersionIconInfo {
    pub source: String,
}

#[tauri::command]
pub fn get_version_icon_source(version_dir_name: String) -> VersionIconInfo {
    let source = ensure_simple_dir_name(&version_dir_name)
        .ok()
        .and_then(|_| version_dir_path(&version_dir_name).ok())
        .and_then(|vdir| version_icon_path(&vdir))
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .map(|n| {
            if n == CUSTOM_ICON_NAME {
                "custom"
            } else {
                "pack"
            }
        })
        .unwrap_or("default");
    VersionIconInfo {
        source: source.to_string(),
    }
}

/// 设置自定义版本图标：把选中图片复制为 versions/{dir}/version-icon.png；仅接受 png/jpeg/webp/gif（魔数校验），≤1MB，覆盖旧图。
#[tauri::command]
pub async fn set_version_icon(version_dir_name: String, source_path: String) -> Result<(), String> {
    ensure_simple_dir_name(&version_dir_name)?;
    let src = PathBuf::from(&source_path);
    let meta =
        std::fs::metadata(&src).map_err(common_err_ctx_key!("errors.file.read", "读取图片失败"))?;
    if !meta.is_file() {
        return Err(keyed_msg!("errors.version.notAFile", "所选路径不是文件"));
    }
    if meta.len() == 0 || meta.len() > MAX_PACK_ICON_BYTES {
        bail_key!(
            "errors.version.iconTooLarge",
            "图片大小需在 1MB 以内\n当前 {} 字节",
            meta.len()
        );
    }
    let bytes =
        std::fs::read(&src).map_err(common_err_ctx_key!("errors.file.read", "读取图片失败"))?;
    if !util::is_supported_image(&bytes) {
        return Err(keyed_msg!(
            "errors.version.iconFormat",
            "不支持的图片格式（仅支持 png / jpg / webp / gif）"
        ));
    }
    let vdir = version_dir_path(&version_dir_name)?;
    tokio::task::spawn_blocking(move || {
        std::fs::write(vdir.join("version-icon.png"), &bytes)
            .map_err(common_err_ctx_key!("errors.file.write", "写入图标失败"))
    })
    .await
    .map_err(common_err_ctx_key!("errors.unknown", "写入任务失败"))?
}

/// 还原默认图标：删自定义图标（version-icon.png），整合包项目图不受影响。
#[tauri::command]
pub async fn reset_version_icon(version_dir_name: String) -> Result<(), String> {
    ensure_simple_dir_name(&version_dir_name)?;
    let vdir = version_dir_path(&version_dir_name)?;
    let target = vdir.join("version-icon.png");
    if !target.exists() {
        return Ok(()); // 幂等：本就无自定义图
    }
    tokio::task::spawn_blocking(move || {
        std::fs::remove_file(target)
            .map_err(common_err_ctx_key!("errors.file.delete", "删除图标失败"))
    })
    .await
    .map_err(common_err_ctx_key!("errors.unknown", "删除任务失败"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_image_sniffs_magic_bytes() {
        // 判定来自 util::is_supported_image，这里守住命令层的格式白名单行为
        assert!(util::is_supported_image(b"\x89PNG\r\n\x1a\nrest"));
        assert!(util::is_supported_image(&[0xff, 0xd8, 0xff, 0xe0]));
        let mut webp = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec();
        assert!(util::is_supported_image(&webp));
        webp[8] = b'X'; // 破坏 WEBP 标记
        assert!(!util::is_supported_image(&webp));
        assert!(util::is_supported_image(b"GIF89a"));
        assert!(!util::is_supported_image(b"<html>"));
        assert!(!util::is_supported_image(b""));
    }

    #[test]
    fn ensure_simple_dir_name_blocks_traversal() {
        assert!(ensure_simple_dir_name("1.20.1-fabric-0.16.9").is_ok());
        assert!(ensure_simple_dir_name("Pack_1").is_ok());
        assert!(
            ensure_simple_dir_name("a..b").is_err(),
            "含 .. 子串一律拒绝（对齐 get_world_icon 先例）"
        );
        assert!(ensure_simple_dir_name("a\\b").is_err());
        assert!(ensure_simple_dir_name("..").is_err());
    }
}
