//! 路径 / 文件名解析与安全校验、目录枚举。

use std::path::{Path, PathBuf};

use super::time::modified_secs;

/// URL 路径最后一段（文件名）：`https://a/b/c.jar` → `Some("c.jar")`；空尾段/空串 → None。
pub fn url_file_name(url: &str) -> Option<&str> {
    url.rsplit('/').next().filter(|s| !s.is_empty())
}

/// sha1 前两位（assets/objects 子目录名）。长度不足 2 时回退 `"00"`：
/// hash 来自远程索引或本地 JSON，裸切片 `&hash[..2]` 在畸形输入下会 panic
/// （字节切片还可能落在多字节边界）。
pub fn sha1_sub_dir(hash: &str) -> &str {
    hash.get(..2).unwrap_or("00")
}

/// 相对路径安全校验（路径穿越防护）：拒绝空路径、绝对路径、含 `..` 或空段的输入。
/// 返回校验后的相对路径，调用方自行 `join` 到目标根目录。
/// 用于整合包清单条目名、zip 成员名、JDK runtime 条目名等**外部输入**的落盘路径。
pub fn safe_relative_path(rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim();
    if rel.is_empty() || rel.starts_with('/') || rel.starts_with('\\') {
        return Err(format!("非法文件路径\n{rel:?}"));
    }
    if rel
        .split(['/', '\\'])
        .any(|seg| seg == ".." || seg.is_empty())
    {
        return Err(format!("文件路径含非法段\n{rel:?}"));
    }
    Ok(PathBuf::from(rel))
}

/// JAVA_HOME 下 java 可执行文件的路径（bin/java(.exe)）。
/// 扫描与启动共用，避免平台判定散落两处。
pub fn java_bin_path(home: &Path) -> PathBuf {
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    home.join("bin").join(exe)
}

/// 简单文件名/目录名判定（防路径穿越）：仅接受无路径分隔符、无 `..` 段的非空名。
/// 供版本/存档/资源/图标等「读路径由调用方拼接」的命令前置校验统一使用。
pub fn is_simple_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && !name.contains("..")
}

/// 目录条目枚举（文件或子目录，取决于 want_dir）：返回 (名称, 字节大小, 修改时间秒)。
/// 目录不存在/不可读返回空 Vec（调用方多为列表类命令，静默空列表语义）。
pub fn dir_entries(dir: &Path, want_dir: bool) -> Vec<(String, u64, u64)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir() == want_dir)
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some((
                e.file_name().to_string_lossy().to_string(),
                meta.len(),
                modified_secs(&meta),
            ))
        })
        .collect()
}

/// 文件/目录名净化：Windows 非法字符（`<>:"/\|?*`）与控制字符替换 `_`，
/// 去首尾空白与尾点；结果为空或命中 Windows 保留名（CON/NUL/COM1…）时回退 `fallback`。
pub fn sanitize_fs_name(name: &str, fallback: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    let no_tail_dot = trimmed.strip_suffix('.').unwrap_or(trimmed);
    if no_tail_dot.is_empty() || no_tail_dot.chars().all(|c| c == '_') {
        return fallback.to_string();
    }
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(no_tail_dot)) {
        return fallback.to_string();
    }
    no_tail_dot.to_string()
}

/// 子目录名冲突消解：`parent/{base}` 已存在则追加 `_{n}`。
pub fn unique_child_dir(parent: &Path, base: &str) -> String {
    if !parent.join(base).exists() {
        return base.to_string();
    }
    for n in 1.. {
        let candidate = format!("{base}_{n}");
        if !parent.join(&candidate).exists() {
            return candidate;
        }
    }
    // 循环为无限 RangeFrom（命中即 return），此处仅为满足类型检查
    unreachable!()
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
    fn sha1_sub_dir_never_panics_on_malformed_input() {
        assert_eq!(sha1_sub_dir("abcdef0123456789"), "ab");
        // 长度不足与多字节边界都回退 "00"（裸切片会 panic）
        assert_eq!(sha1_sub_dir(""), "00");
        assert_eq!(sha1_sub_dir("a"), "00");
        assert_eq!(sha1_sub_dir("中文"), "00");
    }

    #[test]
    fn safe_relative_path_blocks_traversal() {
        // 合法相对路径：整合包清单条目、zip 成员、JDK runtime 条目
        assert!(safe_relative_path("mods/a.jar").is_ok());
        assert!(safe_relative_path("config/nested/b.json").is_ok());
        assert!(safe_relative_path("bin/java.exe").is_ok());
        // 绝对路径（两种分隔符）、空路径、`..` 与空段
        assert!(safe_relative_path("/abs").is_err());
        assert!(safe_relative_path("\\abs").is_err());
        assert!(safe_relative_path("").is_err());
        assert!(safe_relative_path("../evil").is_err());
        assert!(safe_relative_path("bin/../../evil").is_err());
        assert!(safe_relative_path("a//b").is_err());
    }
}
