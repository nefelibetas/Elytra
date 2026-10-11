//! 批量文件下载批处理：多源下载 + 本地复用降级 + 逐文件进度 + 跨任务路径互斥。
//! 供资源（assets）与支持库（libraries）等「一个清单变一批文件」的场景共用。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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
/// `max_errors` 有值时达到上限即停止派发（清单庞大且源整体不可达时的止损）。
pub async fn run(
    items: Vec<BatchItem>,
    concurrency: usize,
    pc: &ProgressCounters,
    cancel: Option<&Arc<AtomicBool>>,
    max_errors: Option<usize>,
) -> Vec<String> {
    let sem = Arc::new(Semaphore::new(concurrency));
    let mut handles = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for item in items {
        if max_errors.is_some_and(|max| errors.len() >= max) {
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
        let permit = sem.clone();
        let ctr = pc.files.clone();
        let bc = pc.bytes.clone();
        let sb = pc.step_bytes.clone();
        let cancel = cancel.cloned();
        handles.push(tokio::spawn(async move {
            let _permit = permit
                .acquire_owned()
                .await
                .expect("下载信号量不会关闭");
            // 跨任务互斥：不同任务共用同一目标目录，防并发写同一文件互相踩踏
            let _file_lock = wait_path_lock(&dest).await;
            // 字节级进度：增量累加进全局计数（重试从 0 重新开始，增量不会为负）
            let progress = ProgressCounters {
                bytes: bc.clone(),
                step_bytes: sb.clone(),
                ..Default::default()
            }
            .byte_callback();
            // 复用分支：本地复制；失败降级为网络下载，不致命
            if let Some(src) = local {
                match copy_verified_local(&src, &dest, size, Some(progress.clone())).await {
                    Ok(_) => {
                        ctr.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    }
                    Err(e) => match fetcher::fetch_verified_c(
                        &urls,
                        &dest,
                        Some(progress),
                        &sha1,
                        cancel.as_deref(),
                    )
                    .await
                    {
                        Ok(_) => {
                            ctr.fetch_add(1, Ordering::Relaxed);
                            Ok(())
                        }
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
                    Ok(_) => {
                        ctr.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    }
                    Err(e) => Err(format!("{}: {}", label, e)),
                }
            }
        }));
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
