//! 批量文件下载批处理：多源下载 + 本地复用降级 + 逐文件进度 + 跨任务路径互斥。
//! 供资源（assets）与支持库（libraries）等「一个清单变一批文件」的场景共用。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::Semaphore;

use super::fetcher;
use super::path_lock::wait_path_lock;
use super::progress::ProgressCounters;
use super::reuse::copy_verified_local;

/// 单个待下载文件。
pub struct BatchItem {
    /// 多源候选 URL（调用方按源顺序构造）。
    pub urls: Vec<String>,
    pub dest: PathBuf,
    /// 期望 sha1（空串跳过校验）。
    pub sha1: String,
    /// 本地复用复制时的期望大小（0 = 不校验大小）。
    pub size: u64,
    /// 本地复用源（命中则复制而非下载；复制失败自动降级为网络下载）。
    pub local: Option<PathBuf>,
    /// 错误信息前缀（sha1 或目标路径可读形式）。
    pub label: String,
}

/// 并发下载一批文件，返回错误列表（空 = 全部成功）。
/// `max_errors` 有值时达到上限即停止派发（清单庞大且源整体不可达时的止损）：
/// 上限按 worker 实时累加的失败数判断，而非全部 await 完的汇总。
/// `cancel` 置位后立即停止派发（不再为本批剩余条目创建任务/请求）。
pub async fn run(
    items: Vec<BatchItem>,
    concurrency: usize,
    pc: &ProgressCounters,
    cancel: Option<&Arc<AtomicBool>>,
    max_errors: Option<usize>,
) -> Vec<String> {
    let sem = Arc::new(Semaphore::new(concurrency));
    // 失败数由 worker 实时累加：不再等到全部 await 完成才可见，止损因此能真正生效
    let failed = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    // 派发阶段因取消而提前收场（批次不完整）：汇总里补一条取消标记，避免「空错误」
    // 被调用方（如整合包步骤）当成下载成功而继续后续步骤。
    let mut stopped_by_cancel = false;

    for item in items {
        // 取消后立即停止派发：大清单（数千条）不再逐条创建任务、排队等信号量。
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            stopped_by_cancel = true;
            break;
        }
        // 先取并发许可再派发：`acquire_owned` 会让出执行权，已派发的 worker 得以推进并
        // 累加失败数，下面的 max_errors 判断因此能看到最新值（此前先 spawn 全部、
        // 派发阶段看到的失败数恒为 0，止损形同虚设）。
        let permit = match sem.clone().acquire_owned().await {
            Ok(p) => p,
            // 信号量不会关闭；防御性处理，避免 panic 让整个下载任务静默挂起
            Err(_) => break,
        };
        // 等待许可期间可能已被取消：此时停止派发，不再新增任务。
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
            // 取锁前再查一次取消：避免已取消后仍排在路径锁队列里等锁。
            if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Ok(());
            }
            // 跨任务互斥：不同任务共用同一目标目录，防并发写同一文件互相踩踏
            let _file_lock = wait_path_lock(&dest).await;
            // 等到锁后可能已被取消：不再发起网络请求（fetch 本身也可取消，双保险）。
            if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Ok(());
            }
            // 字节级进度：增量累加进全局计数（重试从 0 重新开始，增量不会为负）
            let progress = ProgressCounters {
                bytes: bc.clone(),
                step_bytes: sb.clone(),
                ..Default::default()
            }
            .byte_callback();
            // 复用分支：本地复制；失败降级为网络下载，不致命
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
                // 逐源 sha1 校验下载：某源内容不匹配（镜像缓存陈旧）自动换下一源重试，
                // 全部源均不匹配才报错；校验失败时 fetch_verified 内部已删除坏文件。
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
                // 实时失败计数：派发循环据此止损（取消提前返回的 worker 不算失败）
                failed.fetch_add(1, Ordering::Relaxed);
            }
            result
        }));
    }

    if stopped_by_cancel {
        errors.push("任务已取消".into());
    }

    for handle in handles {
        // JoinError（worker panic）转任务错误：unwrap 会让整个下载任务 panic、
        // 不发终态事件，前端卡片永久挂起
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

    /// 不可达的多源 URL：连接立即被拒，用于构造确定性失败（无外部网络依赖）。
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

    /// 取消标志已置位：派发循环入口即 break，不创建任何 worker、不发起任何请求。
    #[tokio::test]
    async fn cancel_stops_dispatch_immediately() {
        let cancel = Arc::new(AtomicBool::new(true));
        let pc = ProgressCounters::default();
        let items: Vec<BatchItem> = (0..50).map(item).collect();

        let errors = run(items, 4, &pc, Some(&cancel), Some(10)).await;

        // 仅在派发入口收场：只有取消标记，没有任何 worker 的真实失败
        assert_eq!(
            errors,
            vec!["任务已取消".to_string()],
            "取消后不应派发任何请求（死源本会产生失败项）"
        );
        assert_eq!(pc.files.load(Ordering::Relaxed), 0, "取消后不应有完成文件");
    }

    /// max_errors 止损：失败数由 worker 实时累加，达到上限即停止派发。
    /// concurrency=1 时派发需等待许可归还，故失败计数在下次派发前可见。
    #[tokio::test]
    async fn max_errors_stops_dispatch() {
        let cancel = Arc::new(AtomicBool::new(false));
        let pc = ProgressCounters::default();
        let items: Vec<BatchItem> = (0..5).map(item).collect();

        let errors = run(items, 1, &pc, Some(&cancel), Some(1)).await;

        // 仅 1 个 worker 有机会失败（第 2 个条目在派发前已达上限被拦下）
        assert!(
            errors.iter().any(|e| e.contains("失败过多")),
            "达到上限应停止派发: {errors:?}"
        );
        assert_eq!(errors.len(), 2, "1 条真实失败 + 1 条止损提示: {errors:?}");
        assert_eq!(pc.files.load(Ordering::Relaxed), 0);
    }
}
