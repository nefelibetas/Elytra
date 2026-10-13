//! 工具箱下载任务的执行体：分段/单连接下载 + 进度心跳 + 终态收口。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::manager;
use super::model::{StopReason, ToolDlStatus, ToolDlTask};
use crate::downloader::fetcher::FetchError;
use crate::downloader::segmented::{self, SegmentedOutcome, SegmentedRequest};
use crate::task::progress::SpeedSampler;
use crate::task::{emit_event, EventSink};
use crate::events;

/// 进度心跳间隔（与 `task` 域的 EmitLoop 同量级：够顺滑又不过量占 IPC）。
const TICK: Duration = Duration::from_millis(250);

/// 执行一个下载任务：下载过程中按 [`TICK`] 上报进度，结束时收口并派发终态。
pub async fn run(task: Arc<ToolDlTask>, sink: Arc<dyn EventSink>) {
    let url = task.url.clone();
    let dest = task.dest.clone();
    let progress_task = Arc::clone(&task);
    let progress = move |downloaded: u64, total: u64| {
        progress_task.downloaded.store(downloaded, Ordering::Relaxed);
        if total > 0 {
            progress_task.total.store(total, Ordering::Relaxed);
        }
    };

    // 探测一确定方案就写回任务：UI 在下载途中即显示真实并发度，而非等终态
    let plan_task = Arc::clone(&task);
    let on_plan = move |ranged: bool, segments: usize| {
        plan_task.ranged.store(ranged, Ordering::Relaxed);
        plan_task.segments_used.store(segments as u32, Ordering::Relaxed);
    };

    let download = segmented::download_segmented(
        SegmentedRequest {
            url: &url,
            dest: &dest,
            segments: task.segments as usize,
            // 暂停/失败后重新开始一律走断点续传（位图不匹配时内核自动作废）
            resume: true,
            cancel: Some(&task.stop),
            on_plan: Some(&on_plan),
        },
        Some(progress),
    );
    tokio::pin!(download);

    let mut sampler = SpeedSampler::new();
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let result = loop {
        tokio::select! {
            outcome = &mut download => break outcome,
            _ = ticker.tick() => {
                let (speed, _) = sampler.sample(
                    task.downloaded.load(Ordering::Relaxed),
                    task.total.load(Ordering::Relaxed),
                );
                task.speed.store(speed as u64, Ordering::Relaxed);
                emit_snapshot(&sink, &task);
            }
        }
    };

    finish(&task, &sink, result);
}

/// 终态收口：成功 / 暂停 / 取消 / 失败四路分流（暂停保留中间产物，取消清理）。
fn finish(
    task: &ToolDlTask,
    sink: &Arc<dyn EventSink>,
    result: Result<SegmentedOutcome, FetchError>,
) {
    match result {
        Ok(outcome) => {
            task.ranged.store(outcome.ranged, Ordering::Relaxed);
            task.segments_used
                .store(outcome.segments as u32, Ordering::Relaxed);
            task.downloaded.store(outcome.bytes, Ordering::Relaxed);
            if task.total.load(Ordering::Relaxed) == 0 {
                task.total.store(outcome.bytes, Ordering::Relaxed);
            }
            task.set_status(ToolDlStatus::Done);
        }
        Err(FetchError::Cancelled) => {
            if task.stop_reason() == StopReason::Pause {
                // 保留 .part 与位图：继续时从断点续传
                task.set_status(ToolDlStatus::Paused);
            } else {
                manager::cleanup_partials(&task.dest);
                task.set_status(ToolDlStatus::Cancelled);
            }
        }
        Err(e) => {
            *task.error.lock() = Some(e.to_string());
            task.set_status(ToolDlStatus::Error);
        }
    }
    task.speed.store(0, Ordering::Relaxed);
    emit_snapshot(sink, task);
}

/// 派发任务快照；已从注册表移除的任务不再派发（防前端重建已移除的卡片）。
pub(crate) fn emit_snapshot(sink: &Arc<dyn EventSink>, task: &ToolDlTask) {
    if !manager::contains(&task.id) {
        return;
    }
    emit_event(sink.as_ref(), events::TOOL_DL_PROGRESS, task.snapshot());
}
