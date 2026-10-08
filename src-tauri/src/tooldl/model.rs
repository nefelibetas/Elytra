//! 工具箱下载任务的状态模型：内部任务（原子 + 短锁）与对外快照。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// 任务状态（前端镜像同名小写字符串）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolDlStatus {
    /// 已入队，等待空闲槽位
    Queued,
    Downloading,
    /// 用户暂停（保留 `.part` 与位图，继续时断点续传）
    Paused,
    Done,
    Error,
    /// 用户取消（中间产物已清理）
    Cancelled,
}

/// 停止原因：决定中间产物保留（暂停）还是清理（取消）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    None,
    Pause,
    Cancel,
}

impl StopReason {
    pub fn as_u8(self) -> u8 {
        match self {
            StopReason::None => 0,
            StopReason::Pause => 1,
            StopReason::Cancel => 2,
        }
    }

    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => StopReason::Pause,
            2 => StopReason::Cancel,
            _ => StopReason::None,
        }
    }
}

/// 单个下载任务。
pub struct ToolDlTask {
    pub id: String,
    pub url: String,
    pub file_name: String,
    /// 最终落地路径（并发重名时已追加 `_n` 消解）
    pub dest: PathBuf,
    pub save_dir: PathBuf,
    /// 请求的分段数
    pub segments: u32,
    pub created_at: u64,
    pub downloaded: AtomicU64,
    pub total: AtomicU64,
    pub speed: AtomicU64,
    /// 本次（或上次）是否走了 Range 分段
    pub ranged: AtomicBool,
    /// 实际使用的分段数（1 = 单连接）
    pub segments_used: AtomicU32,
    pub status: Mutex<ToolDlStatus>,
    pub error: Mutex<Option<String>>,
    /// 下载循环停止标志（作为取消令牌交给 downloader）
    pub stop: AtomicBool,
    /// 停止原因（暂停 / 取消）
    pub stop_reason: AtomicU8,
}

impl ToolDlTask {
    /// 请求停止（幂等）：先记原因再置标志，执行体读到的原因一定与标志一致。
    pub fn request_stop(&self, reason: StopReason) {
        self.stop_reason.store(reason.as_u8(), Ordering::Relaxed);
        self.stop.store(true, Ordering::Relaxed);
    }

    /// 清除停止标志（继续 / 重试前调用）。
    pub fn clear_stop(&self) {
        self.stop_reason
            .store(StopReason::None.as_u8(), Ordering::Relaxed);
        self.stop.store(false, Ordering::Relaxed);
    }

    pub fn stop_reason(&self) -> StopReason {
        StopReason::from_u8(self.stop_reason.load(Ordering::Relaxed))
    }

    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    pub fn status(&self) -> ToolDlStatus {
        *self.status.lock()
    }

    pub fn set_status(&self, status: ToolDlStatus) {
        *self.status.lock() = status;
    }

    /// 对外快照（事件负载 / 列表返回；前端镜像此结构）。
    pub fn snapshot(&self) -> ToolDlSnapshot {
        ToolDlSnapshot {
            id: self.id.clone(),
            url: self.url.clone(),
            file_name: self.file_name.clone(),
            dest: self.dest.to_string_lossy().into_owned(),
            save_dir: self.save_dir.to_string_lossy().into_owned(),
            total: self.total.load(Ordering::Relaxed),
            downloaded: self.downloaded.load(Ordering::Relaxed),
            speed: self.speed.load(Ordering::Relaxed),
            ranged: self.ranged.load(Ordering::Relaxed),
            segments: self.segments_used.load(Ordering::Relaxed),
            status: self.status(),
            error: self.error.lock().clone(),
            created_at: self.created_at,
        }
    }
}

/// 对外快照（前端手写同名 interface）。
#[derive(Debug, Clone, Serialize)]
pub struct ToolDlSnapshot {
    pub id: String,
    pub url: String,
    pub file_name: String,
    /// 最终落地路径
    pub dest: String,
    pub save_dir: String,
    /// 总字节（0 = 未知）
    pub total: u64,
    pub downloaded: u64,
    /// 字节/秒（0 = 未在下载）
    pub speed: u64,
    /// 是否走 Range 分段
    pub ranged: bool,
    /// 实际分段数（1 = 单连接）
    pub segments: u32,
    pub status: ToolDlStatus,
    pub error: Option<String>,
    pub created_at: u64,
}
