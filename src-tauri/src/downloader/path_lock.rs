//! 跨任务下载路径互斥：不同版本任务共用 libraries/assets 目录，按目标路径独占，
//! 避免并发写同一文件导致 sha1 校验互相踩踏（只锁具体目标，不锁整个版本目录）。

use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

static ACTIVE_PATHS: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub struct PathLock {
    path: PathBuf,
}

impl PathLock {
    /// 独占占用一个路径；已被占用返回 `None`。
    pub fn try_acquire(path: &Path) -> Option<PathLock> {
        let mut active = ACTIVE_PATHS.lock();
        if !active.insert(path.to_path_buf()) {
            return None;
        }
        Some(PathLock {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for PathLock {
    fn drop(&mut self) {
        ACTIVE_PATHS.lock().remove(&self.path);
    }
}

/// 轮询等待获取路径锁：另一任务正占用同一目标路径时短暂等待其完成。
pub async fn wait_path_lock(path: &Path) -> PathLock {
    loop {
        if let Some(lock) = PathLock::try_acquire(path) {
            return lock;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_release_roundtrip() {
        let p = Path::new("C:/test/versions/26.2");
        {
            let _g = PathLock::try_acquire(p).expect("首次占用应成功");
            assert!(PathLock::try_acquire(p).is_none(), "同路径二次占用应失败");
        }
        assert!(PathLock::try_acquire(p).is_some(), "守卫 Drop 后占用应成功");
    }
}
