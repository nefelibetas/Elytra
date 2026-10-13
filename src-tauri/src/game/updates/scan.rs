//! 本地目录扫描：枚举 mods（jar）/ 资源目录（zip）顶层文件，按文件名排序，无网络请求。

use std::path::Path;

use serde::Serialize;

/// 本地 mod 文件信息。
#[derive(Debug, Clone, Serialize)]
pub struct LocalModFile {
    pub file_name: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 最后修改时间（unix 秒）
    pub last_modified: u64,
}

#[derive(Debug, Clone, Copy)]
pub enum ScanFilter {
    /// 仅 .jar（mods 目录）
    Jar,
    /// 仅 .zip（材质包/数据包/光影包；文件夹形式的包不识别）
    Zip,
}

/// 扫描目录下文件（按文件名排序，不含子目录）。
pub fn scan_dir_files(dir: &Path, filter: ScanFilter) -> Vec<LocalModFile> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|s| s.to_str());
        let keep = match filter {
            ScanFilter::Jar => ext == Some("jar"),
            ScanFilter::Zip => ext == Some("zip"),
        };
        if !keep {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let meta = match e.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        out.push(LocalModFile {
            file_name: file_name.to_string(),
            size: meta.len(),
            last_modified: crate::util::modified_secs(&meta),
        });
    }
    out.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    out
}

/// 扫描 mods 目录下的 .jar（按文件名排序）。
pub fn scan_mod_files(mods_dir: &Path) -> Vec<LocalModFile> {
    scan_dir_files(mods_dir, ScanFilter::Jar)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_mod_files_only_jars() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.jar"), b"a").unwrap();
        std::fs::write(dir.path().join("b.jar"), b"bb").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"readme").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("c.jar"), b"c").unwrap();

        let files = scan_mod_files(dir.path());
        let names: Vec<&str> = files.iter().map(|f| f.file_name.as_str()).collect();
        assert_eq!(names, vec!["a.jar", "b.jar"]);
        assert_eq!(files[0].size, 1);
        assert_eq!(files[1].size, 2);
    }

    #[test]
    fn scan_mod_files_missing_dir_empty() {
        let dir = tempfile::tempdir().unwrap();
        let none = dir.path().join("no-such-dir");
        assert!(scan_mod_files(&none).is_empty());
    }

    #[test]
    fn scan_dir_files_zip_filter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.zip"), b"a").unwrap();
        std::fs::write(dir.path().join("b.zip"), b"bb").unwrap();
        std::fs::write(dir.path().join("c.txt"), b"c").unwrap();
        std::fs::write(dir.path().join("d.jar"), b"d").unwrap();
        std::fs::create_dir_all(dir.path().join("folder")).unwrap();
        std::fs::write(dir.path().join("folder").join("e.zip"), b"e").unwrap();

        let files = scan_dir_files(dir.path(), ScanFilter::Zip);
        let names: Vec<&str> = files.iter().map(|f| f.file_name.as_str()).collect();
        assert_eq!(names, vec!["a.zip", "b.zip"]);
        assert_eq!(files[0].size, 1);
        assert_eq!(files[1].size, 2);
    }
}
