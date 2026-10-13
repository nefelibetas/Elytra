//! 批量文件下载（清单→一批文件）：多源 + 本地复用降级 + 逐文件进度 + 跨任务路径互斥。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::Semaphore;

use super::fetcher;
use super::path_lock::wait_path_lock;
use super::progress::ProgressCounters;
use super::reuse::copy_verified_local;

pub struct BatchItem {
    /// 多源候选 URL（按源顺序）。
    pub urls: Vec<String>,
    pub dest: PathBuf,
    /// 期望 sha1（空串跳过校验）。
    pub sha1: String,
    /// 本地复用期望大小（0 = 不校验）。
    pub size: u64,
    /// 本地复用源：命中则复制而非下载，失败降级网络。
    pub local: Option<PathBuf>,
    pub label: String,
}

/// 并发下载一批文件，返回错误列表（空 = 全部成功）；`max_errors` 达上限或 `cancel` 置位即停止派发。
pub async fn run(
    items: Vec<BatchItem>,
    concurrency: usize,
    pc: &ProgressCounters,
    cancel: Option<&Arc<AtomicBool>>,
    max_errors: Option<usize>,
) -> Vec<String> {
    let sem = Arc::new(Semaphore::new(concurrency));
    let failed = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    // 提前收场时补取消标记，避免空错误被当成成功
    let mut stopped_by_cancel = false;

    for item in items {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            stopped_by_cancel = true;
            break;
        }
        // 先取许可再派发：让出执行权，worker 得以累加失败数供 max_errors 判断
        let permit = match sem.clone().acquire_owned().await {
            Ok(p) => p,
            // 信号量不会关闭；防御性处理避免 panic 挂起整个下载任务
            Err(_) => break,
        };
        // 等待许可期间可能已被取消
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            stopped_by_cancel = true;
            break;
        }
        if max_errors.is_some_and(|max| failed.load(Ordering::Relaxed) >= max) {
            errors.push("失败过多，已停止".into());
            break;
        }
        let BatchItem {
            urls,
            dest,
            sha1,
            size,
            local,
            label,
        } = item;
        let ctr = pc.files.clone();
        let bc = pc.bytes.clone();
        let sb = pc.step_bytes.clone();
        let cancel = cancel.cloned();
        let failed = Arc::clone(&failed);
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            // 取锁前再查取消：避免已取消仍排队等锁
            if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Ok(());
            }
            // 跨任务互斥：防并发写同一目标文件
            let _file_lock = wait_path_lock(&dest).await;
            // 等到锁后可能已被取消：不再发请求
            if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Ok(());
            }
            // 字节级进度：增量累加（重试从 0 重新开始，增量不为负）
            let progress = ProgressCounters {
                bytes: bc.clone(),
                step_bytes: sb.clone(),
                ..Default::default()
            }
            .byte_callback();
            let result = if let Some(src) = local {
                match copy_verified_local(&src, &dest, size, Some(progress.clone())).await {
                    Ok(_) => Ok(()),
                    Err(e) => match fetcher::fetch_verified_c(
                        &urls,
                        &dest,
                        Some(progress),
                        &sha1,
                        cancel.as_deref(),
                    )
                    .await
                    {
                        Ok(_) => Ok(()),
                        Err(e2) => Err(format!("{}: {}（本地复用失败: {}）", label, e2, e)),
                    },
                }
            } else {
                // 逐源 sha1 校验：不匹配自动换源，全不匹配才报错
                match fetcher::fetch_verified_c(
                    &urls,
                    &dest,
                    Some(progress),
                    &sha1,
                    cancel.as_deref(),
                )
                .await
                {
                    Ok(_) => Ok(()),
                    Err(e) => Err(format!("{}: {}", label, e)),
                }
            };
            if result.is_ok() {
                ctr.fetch_add(1, Ordering::Relaxed);
            } else {
                // 实时失败计数：派发循环据此止损（取消提前返回不算失败）
                failed.fetch_add(1, Ordering::Relaxed);
            }
            result
        }));
    }

    if stopped_by_cancel {
        errors.push("任务已取消".into());
    }

    for handle in handles {
        // JoinError（worker panic）转任务错误：避免 panic 导致不发终态事件
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => errors.push(e),
            Err(e) => errors.push(format!("下载工作线程异常\n{e}")),
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::{run, BatchItem};
    use crate::downloader::progress::ProgressCounters;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    fn dead_url() -> String {
        "http://127.0.0.1:1/missing".to_string()
    }

    fn item(index: usize) -> BatchItem {
        let dest = std::env::temp_dir().join(format!(
            "elytra_batch_{index}_{}.bin",
            std::process::id()
        ));
        BatchItem {
            urls: vec![dead_url()],
            dest,
            sha1: String::new(),
            size: 0,
            local: None,
            label: format!("item{index}"),
        }
    }

    #[tokio::test]
    async fn cancel_stops_dispatch_immediately() {
        let cancel = Arc::new(AtomicBool::new(true));
        let pc = ProgressCounters::default();
        let items: Vec<BatchItem> = (0..50).map(item).collect();

        let errors = run(items, 4, &pc, Some(&cancel), Some(10)).await;

        assert_eq!(
            errors,
            vec!["任务已取消".to_string()],
            "取消后不应派发任何请求（死源本会产生失败项）"
        );
        assert_eq!(pc.files.load(Ordering::Relaxed), 0, "取消后不应有完成文件");
    }

    #[tokio::test]
    async fn max_errors_stops_dispatch() {
        let cancel = Arc::new(AtomicBool::new(false));
        let pc = ProgressCounters::default();
        let items: Vec<BatchItem> = (0..5).map(item).collect();

        let errors = run(items, 1, &pc, Some(&cancel), Some(1)).await;

        assert!(
            errors.iter().any(|e| e.contains("失败过多")),
            "达到上限应停止派发: {errors:?}"
        );
        assert_eq!(errors.len(), 2, "1 条真实失败 + 1 条止损提示: {errors:?}");
        assert_eq!(pc.files.load(Ordering::Relaxed), 0);
    }
}
