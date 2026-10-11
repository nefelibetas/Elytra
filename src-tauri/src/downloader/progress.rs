//! 下载进度计数器（原子计数句柄）。
//!
//! 下载/安装函数通过 `ProgressCounters` 上报进度；实际原子由任务层
//! （`task::progress::TaskEmitContext`）持有，组装时从任务上下文字段 clone 进来。
//! 四个原子必须同源创建与更新——整合为结构体后，"文件数/字节/步骤分母"
//! 不再可能被漏传或换源导致计数错乱（此前靠参数顺序隐式绑定）。

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// 下载进度计数器：完成文件数 / 累计字节 / 本步骤字节 / 本步骤总字节。
///
/// - `bytes` 可跨函数共享（多步骤并行时累计任务总字节，如 libraries 与 assets 并发）；
/// - 步骤无细分或总量未知时，`step_bytes` / `step_total` 保持 0（进度回退文件数公式）。
#[derive(Clone, Default)]
pub struct ProgressCounters {
    /// 已完成文件数（无文件粒度的步骤恒 0）
    pub files: Arc<AtomicU32>,
    /// 累计下载字节（全任务总量，可共享）
    pub bytes: Arc<AtomicU64>,
    /// 本步骤累计字节（进度条步骤分子；与 `bytes` 同步累加）
    pub step_bytes: Arc<AtomicU64>,
    /// 本步骤总字节（进度条步骤分母；下载前 `store` 一次）
    pub step_total: Arc<AtomicU64>,
    /// 本步骤**待下载文件数**（批量步骤的进度分母）。
    ///
    /// 与 `step_total` 分开的原因：后者是**逐文件**语义（每次回调写入当前文件的
    /// content-length），批量步骤用它当分母会得到"下载进度 / 单个文件大小"的
    /// 无意义比值。批量辅助函数在起跑前写入一次待下载条目数，心跳据此算步骤百分比。
    pub step_files_total: Arc<AtomicU32>,
}

impl ProgressCounters {
    /// 字节增量回调工厂（含步骤分母写入）：维护 worker 本地水位，把每次回调的
    /// 增量累加进 `bytes` / `step_bytes`；`total > 0` 时把 content-length 写入
    /// `step_total`（步骤分母）。替代各任务层手写的 "swap-prev + saturating_sub" 闭包。
    /// 返回值 `(closure)` 可直接传给各 fetch 变体（`fetch_verified_c` 等）的进度参数。
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

    /// 字节增量回调工厂（写 `bytes` / `step_bytes`，不动步骤分母）。
    /// `step_progress` 的语义子集（step_total 恒不写入）。
    pub fn byte_callback(&self) -> impl Fn(u64, u64) + Clone + Send + Sync {
        self.step_progress()
    }

    /// 覆盖式回调工厂：把当前下载字节绝对值写入 `bytes`，`total > 0` 时写入
    /// `step_total`。用于单文件进度被任务层按"当前文件绝对值"消费的场景
    /// （client.jar 下载：EmitLoop 读 jar_bytes/jar_total 做 jar_total 回退），
    /// 与 `step_progress` 的增量累加语义不同。
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
