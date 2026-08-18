//! 下载进度计数器（原子计数句柄）。
//!
//! 下载/安装函数通过 `ProgressCounters` 上报进度；实际原子由任务层
//! （`task::progress::TaskEmitContext`）持有，组装时从任务上下文字段 clone 进来。
//! 四个原子必须同源创建与更新——整合为结构体后，"文件数/字节/步骤分母"
//! 不再可能被漏传或换源导致计数错乱（此前靠参数顺序隐式绑定）。

use std::sync::atomic::{AtomicU32, AtomicU64};
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
}

impl ProgressCounters {
    /// 全量构造（任务层组装时逐字段 clone；未用字段可 `..Default::default()` 补零）。
    pub fn new(
        files: Arc<AtomicU32>,
        bytes: Arc<AtomicU64>,
        step_bytes: Arc<AtomicU64>,
        step_total: Arc<AtomicU64>,
    ) -> Self {
        Self {
            files,
            bytes,
            step_bytes,
            step_total,
        }
    }
}
