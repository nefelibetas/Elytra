//! 本地运行时复用（LocalRuntimeSource）：换启动器迁移时探测可复用根，逐文件 size + sha1 双校验后本地复制（免重下数百 MB），未命中走网络。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::hash::file_sha1_matches_streaming;

/// 可复用运行时根（.minecraft 形态：assets/ 与 libraries/ 目录同时存在）。
#[derive(Debug, Clone)]
pub struct LocalRuntimeSource {
    root: PathBuf,
}

impl LocalRuntimeSource {
    /// 从目标版本目录向上探测最近的复用根：首个同时含 `assets/` 与 `libraries/` 的目录即可复用根。
    pub fn discover(start: &Path) -> Option<LocalRuntimeSource> {
        start
            .ancestors()
            .find(|d| is_local_runtime_root(d))
            .map(|root| LocalRuntimeSource {
                root: root.to_path_buf(),
            })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 复用根下对应 .minecraft 相对路径的文件。
    pub fn local_path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

/// 判定目录是否为可复用运行时根：`assets/` 与 `libraries/` 目录同时存在。
pub fn is_local_runtime_root(dir: &Path) -> bool {
    dir.join("assets").is_dir() && dir.join("libraries").is_dir()
}

/// 逐产物复用判定：无期望 sha1 / 本地缺失 / size 不符 → `None`，流式 sha1 匹配 → `Some(源路径)`；目标自身匹配由调用方先判。
pub fn classify_local_artifact(
    source: &LocalRuntimeSource,
    rel: &str,
    expected_size: Option<u64>,
    expected_sha1: Option<&str>,
) -> Option<PathBuf> {
    let expected_sha1 = expected_sha1?;
    if expected_sha1.is_empty() {
        return None;
    }
    let local = source.local_path(rel);
    let meta = local.metadata().ok()?;
    if let Some(size) = expected_size {
        if meta.len() != size {
            return None;
        }
    }
    if file_sha1_matches_streaming(&local, expected_sha1) {
        Some(local)
    } else {
        None
    }
}

/// 验证后复制：流式复制本地文件到目标，进度回调与下载回调同语义；失败清理残留（由调用方降级为网络下载）。
pub async fn copy_verified_local<F: Fn(u64, u64)>(
    src: &Path,
    dest: &Path,
    total: u64,
    progress: Option<F>,
) -> Result<u64, String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(common_err_ctx_key!("errors.file.createDir", "创建目录失败"))?;
    }
    let mut src_f = std::fs::File::open(src).map_err(common_err_ctx_key!(
        "errors.file.read",
        "打开本地复用源失败"
    ))?;
    let mut dst_f = std::fs::File::create(dest)
        .map_err(common_err_ctx_key!("errors.file.write", "创建目标文件失败"))?;
    let mut copied: u64 = 0;
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = match src_f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                let _ = std::fs::remove_file(dest);
                bail_key!("errors.file.read", "读取本地复用源失败\n{e}");
            }
        };
        if let Err(e) = dst_f.write_all(&buf[..n]) {
            let _ = std::fs::remove_file(dest);
            bail_key!("errors.file.write", "写入目标文件失败\n{e}");
        }
        copied += n as u64;
        if let Some(cb) = &progress {
            cb(copied, total);
        }
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempdir_shim::TempDir, PathBuf) {
        let root = std::env::temp_dir().join(format!("elytra_test_reuse_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("assets/objects/ab")).unwrap();
        std::fs::create_dir_all(root.join("libraries/net/test")).unwrap();
        (tempdir_shim::TempDir(root.clone()), root)
    }

    #[test]
    fn discover_finds_nearest_runtime_root() {
        let (_guard, root) = fixture();
        let vdir = root.join("versions").join("1.20.4");
        assert!(!is_local_runtime_root(&vdir));
        assert!(is_local_runtime_root(&root));
        let found = LocalRuntimeSource::discover(&vdir).expect("应向上找到复用根");
        assert_eq!(found.root(), root);
    }

    #[test]
    fn discover_returns_none_when_no_root() {
        let empty = std::env::temp_dir().join("elytra_test_no_root");
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(empty.join("versions/x")).unwrap();
        assert!(
            LocalRuntimeSource::discover(&empty.join("versions/x")).is_none(),
            "无 assets+libraries 时不应找到复用根"
        );
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn classify_requires_sha1_and_verifies_content() {
        let (_guard, root) = fixture();
        let src = LocalRuntimeSource { root: root.clone() };
        let rel = "libraries/net/test/a.jar";
        let content = b"hello reuse";
        std::fs::write(root.join(rel), content).unwrap();
        let sha = super::super::hash::sha1_hex(content);

        let hit = classify_local_artifact(&src, rel, Some(content.len() as u64), Some(&sha));
        assert_eq!(hit.as_deref(), Some(root.join(rel).as_path()));
        assert!(
            classify_local_artifact(&src, rel, Some(999), Some(&sha)).is_none(),
            "size 不匹配不应复用"
        );
        assert!(
            classify_local_artifact(&src, rel, Some(content.len() as u64), Some("deadbeef"))
                .is_none(),
            "sha1 不匹配不应复用"
        );
        assert!(classify_local_artifact(&src, rel, None, None).is_none());
        assert!(
            classify_local_artifact(&src, "libraries/net/test/missing.jar", None, Some(&sha))
                .is_none()
        );
    }

    #[tokio::test]
    async fn copy_verified_local_copies_and_reports_progress() {
        let (_guard, root) = fixture();
        let src = root.join("libraries/net/test/a.jar");
        std::fs::write(&src, vec![0xABu8; 3000]).unwrap();
        let dest = root.join("libraries/net/test/copied.jar");
        let last = std::cell::Cell::new(0u64);
        let copied = copy_verified_local(&src, &dest, 3000, Some(|d, _| last.set(d))).await;
        assert_eq!(copied.expect("复制应成功"), 3000);
        assert_eq!(last.get(), 3000, "进度回调应上报最终字节");
        assert_eq!(std::fs::read(&dest).unwrap(), vec![0xABu8; 3000]);
    }

    /// 极简临时目录守卫（避免引入 tempfile 依赖）。
    mod tempdir_shim {
        pub struct TempDir(pub std::path::PathBuf);
        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
