//! 下载进度计数器（原子计数句柄）。原子由任务层（`task::progress::TaskEmitContext`）持有、clone 进来；须同源创建更新以避免计数错乱。

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// 下载进度计数器。步骤无细分或总量未知时 `step_bytes`/`step_total` 保持 0（进度回退文件数公式）。
#[derive(Clone, Default)]
pub struct ProgressCounters {
    /// 已完成文件数（无文件粒度的步骤恒 0）
    pub files: Arc<AtomicU32>,
    /// 累计下载字节（全任务总量，可共享）
    pub bytes: Arc<AtomicU64>,
    /// 本步骤累计字节（进度条分子，与 `bytes` 同步累加）
    pub step_bytes: Arc<AtomicU64>,
    /// 本步骤总字节（进度条分母，下载前 `store` 一次）
    pub step_total: Arc<AtomicU64>,
    /// 本步骤待下载文件数（批量步骤进度分母）。
    pub step_files_total: Arc<AtomicU32>,
}

impl ProgressCounters {
    /// 字节增量回调工厂（含步骤分母写入）：维护本地水位，增量累加进 `bytes`/`step_bytes`。
    pub fn step_progress(&self) -> impl Fn(u64, u64) + Clone + Send + Sync {
        let last = Arc::new(AtomicU64::new(0));
        let bytes = Arc::clone(&self.bytes);
        let step_bytes = Arc::clone(&self.step_bytes);
        let step_total = Arc::clone(&self.step_total);
        move |downloaded: u64, total: u64| {
            let prev = last.swap(downloaded, Ordering::Relaxed);
            let delta = downloaded.saturating_sub(prev);
            bytes.fetch_add(delta, Ordering::Relaxed);
            step_bytes.fetch_add(delta, Ordering::Relaxed);
            if total > 0 {
                step_total.store(total, Ordering::Relaxed);
            }
        }
    }

    /// 字节增量回调工厂：`step_progress` 的语义子集（step_total 恒不写入）。
    pub fn byte_callback(&self) -> impl Fn(u64, u64) + Clone + Send + Sync {
        self.step_progress()
    }

    /// 覆盖式回调工厂：写入字节绝对值（非增量），与 `step_progress` 语义不同。
    pub fn overwrite_callback(&self) -> impl Fn(u64, u64) + Clone + Send + Sync {
        let bytes = Arc::clone(&self.bytes);
        let step_total = Arc::clone(&self.step_total);
        move |downloaded: u64, total: u64| {
            bytes.store(downloaded, Ordering::Relaxed);
            if total > 0 {
                step_total.store(total, Ordering::Relaxed);
            }
        }
    }
}
