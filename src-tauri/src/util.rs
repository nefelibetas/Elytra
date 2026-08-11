// ── 跨域公共小工具（纯函数，无状态）──

/// URL 路径最后一段（文件名）：`https://a/b/c.jar` → `Some("c.jar")`；空尾段/空串 → None。
pub fn url_file_name(url: &str) -> Option<&str> {
    url.rsplit('/').next().filter(|s| !s.is_empty())
}

/// 从版本目录名提取 MC 版本（与前端 utils/versionClassify 一致）：
/// 找 `-{loader_key}-` 标记取标记前部分；无标记（vanilla）取整个目录名；
/// 冲突重命名 `{id}_n` 后缀一并剥离（`26.2-forge-65.1.0_1` → `26.2`）。
/// natives 目录按 MC 版本共享——同一 MC 的 vanilla/ModLoader 版本 native 库一致。
pub fn mc_version_from_dir(dir_name: &str) -> &str {
    const LOADER_KEYS: &[&str] = &["fabric", "forge", "neoforge", "quilt", "optifine"];
    // 冲突重命名 {id}_n 后缀（与前端 classifyVersion 的 /_\d+$/ 等价）
    let base = dir_name.split('_').next().unwrap_or(dir_name);
    for key in LOADER_KEYS {
        let marker = format!("-{key}-");
        if let Some(idx) = base.find(&marker) {
            if idx > 0 {
                return &base[..idx];
            }
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_file_name_extracts_last_segment() {
        assert_eq!(
            url_file_name("https://cdn.example.com/mods/fabric-api-0.102.0.jar"),
            Some("fabric-api-0.102.0.jar")
        );
        assert_eq!(url_file_name("a/b/"), None);
        assert_eq!(url_file_name(""), None);
    }

    #[test]
    fn mc_version_from_dir_all_naming_forms() {
        assert_eq!(mc_version_from_dir("26.2"), "26.2");
        assert_eq!(mc_version_from_dir("26.2-forge-65.1.0"), "26.2");
        assert_eq!(mc_version_from_dir("26.2-neoforge-26.2.0.53-beta"), "26.2");
        assert_eq!(mc_version_from_dir("1.20.1-pre1-forge-40.1.0"), "1.20.1-pre1");
        assert_eq!(mc_version_from_dir("1.18.2-fabric-0.16.11"), "1.18.2");
        assert_eq!(mc_version_from_dir("1.12.2"), "1.12.2");
        assert_eq!(mc_version_from_dir("26.2-forge-65.1.0_1"), "26.2");
    }
}
