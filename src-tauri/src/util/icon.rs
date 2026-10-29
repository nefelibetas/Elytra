//! 图片魔数嗅探 / base64 编码 / 图标 IPC 载荷。

use std::path::Path;

use base64::{engine, Engine as _};

/// 魔数嗅探受支持图片格式（png/jpeg/webp/gif）；前端 data URL 渲染与图标落盘共用同一判定集。
pub fn is_supported_image(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return true; // png
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return true; // jpeg
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return true; // webp
    }
    bytes.starts_with(b"GIF8") // gif
}

/// 魔数嗅探图片 mime 类型（与 is_supported_image 同一判定集；未知返回 None）。
pub fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("image/gif");
    }
    None
}

/// 字节 → base64（图标 IPC 用：JSON 数字数组膨胀 3~5 倍，base64 仅 +33%）。
pub fn to_base64(bytes: &[u8]) -> String {
    engine::general_purpose::STANDARD.encode(bytes)
}

/// 图标 IPC 载荷：mime 由魔数嗅探，data 为 base64（替代 JSON 数字数组——后者每字节膨胀 3~5 倍）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct IconData {
    pub mime: String,
    pub data: String,
}

/// 读取图片文件为图标载荷；超限/空文件/非支持格式返回 None（fail-closed）。
pub fn icon_payload(path: &Path, max_bytes: u64) -> Option<IconData> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > max_bytes {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let mime = image_mime(&bytes)?;
    Some(IconData {
        mime: mime.to_string(),
        data: to_base64(&bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_mime_sniffs_supported_formats() {
        assert_eq!(image_mime(b"\x89PNG\r\n\x1a\n"), Some("image/png"));
        assert_eq!(image_mime(&[0xff, 0xd8, 0xff, 0xe0]), Some("image/jpeg"));
        let webp = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec();
        assert_eq!(image_mime(&webp), Some("image/webp"));
        assert_eq!(image_mime(b"GIF89a"), Some("image/gif"));
        assert_eq!(image_mime(b"<html>"), None);
    }

    #[test]
    fn to_base64_matches_known_vectors() {
        assert_eq!(to_base64(b""), "");
        assert_eq!(to_base64(b"f"), "Zg==");
        assert_eq!(to_base64(b"fo"), "Zm8=");
        assert_eq!(to_base64(b"foo"), "Zm9v");
        assert_eq!(to_base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn icon_payload_reads_png_and_rejects_invalid_files() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("icon.png");
        let bytes = b"\x89PNG\r\n\x1a\npayload";
        std::fs::write(&png, bytes).unwrap();

        let icon = icon_payload(&png, 1024).expect("应读取 PNG 图标");
        assert_eq!(icon.mime, "image/png");
        assert_eq!(icon.data, to_base64(bytes));

        assert!(icon_payload(&png, 4).is_none(), "超过 max_bytes 应拒绝");

        let empty = dir.path().join("empty.png");
        std::fs::write(&empty, b"").unwrap();
        assert!(icon_payload(&empty, 1024).is_none(), "空文件应拒绝");

        let text = dir.path().join("text.png");
        std::fs::write(&text, b"<html>").unwrap();
        assert!(icon_payload(&text, 1024).is_none(), "非图片应拒绝");

        assert!(
            icon_payload(&dir.path().join("missing.png"), 1024).is_none(),
            "缺失文件应返回 None"
        );
    }
}
