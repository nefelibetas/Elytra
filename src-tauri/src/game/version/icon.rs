//! 版本图标优先级规则：用户自定义 > 整合包项目图。

use std::path::{Path, PathBuf};

/// 用户自定义图标文件名（设置页设置的图，优先级最高）。
pub const CUSTOM_ICON_NAME: &str = "version-icon.png";
/// 整合包项目图文件名（安装时写入，优先级次之）。
pub const PACK_ICON_NAME: &str = "pack-icon.png";

/// 图标优先级：用户自定义（version-icon.png）> 整合包项目图（pack-icon.png）。
/// 两者都不存在返回 None。
pub fn version_icon_path(vdir: &Path) -> Option<PathBuf> {
    for name in [CUSTOM_ICON_NAME, PACK_ICON_NAME] {
        let p = vdir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_icon_takes_priority_over_pack() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(PACK_ICON_NAME), b"pack").unwrap();
        std::fs::write(dir.path().join(CUSTOM_ICON_NAME), b"custom").unwrap();
        assert_eq!(
            version_icon_path(dir.path()),
            Some(dir.path().join(CUSTOM_ICON_NAME)),
            "两张图都在时应选自定义图"
        );
    }

    #[test]
    fn pack_icon_used_when_custom_missing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(PACK_ICON_NAME), b"pack").unwrap();
        assert_eq!(
            version_icon_path(dir.path()),
            Some(dir.path().join(PACK_ICON_NAME)),
            "无自定义图时回退整合包图"
        );

        let empty = tempfile::tempdir().unwrap();
        assert!(version_icon_path(empty.path()).is_none(), "都缺失返回 None");
    }
}
