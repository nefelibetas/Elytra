//! 分段（多线程）下载：单文件按 HTTP Range 切段并发写入同一文件，支持断点续传。
//! 仅当服务端返回 206 且总长可解析、总长 ≥ `MIN_SEGMENT_BYTES`、URL 非镜像源时启用，否则回退单连接 `fetch_with_c`；内容写 `<dest>.part`、元数据写 `<dest>.part.json`，全部段成功后 `rename` 到 `dest` 并删元数据，暂停/失败保留二者供续传。

use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt as _;
use parking_lot::Mutex;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncSeekExt as _, AsyncWriteExt as _};

use super::fetcher::{self, FetchError};

/// 每段最小字节数：低于此值不分段（段过碎时开销大于收益）。
const MIN_SEGMENT_BYTES: u64 = 1024 * 1024;
/// 段数硬上限：调用方配置再大也不超过（防连接数爆炸）。
const MAX_SEGMENTS: usize = 32;
/// 单文件下载时间预算（秒）：段数按此推导——单流吞吐（近海 CDN 受拥塞窗口×RTT 限制，
/// 实测 ~0.47MB/s 级）除该预算即所需并行度；25MB client.jar 约 5 段、50MB 约 8 段。
const TARGET_DOWNLOAD_SECS: u64 = 10;
/// 官方单流吞吐估计（bytes/s）：实测 466KB/s，取 512KB/s 为收敛圆整
/// （大值让段数略少——保守不外加连接）。分段仅在官方源启用，镜像保持单连接。
const OFFICIAL_SINGLE_STREAM_BPS: u64 = 512 * 1024;
/// 单文件段数上限：batch 并发下「同任务大文件 × 段数」的连接预算（官方源无 per-hash
/// 限流，但避免爆发连接数逼近我们对官方的经验并发上限）。
const MAX_FILE_SEGMENTS: usize = 8;

/// 官方大文件的请求段数：`ceil(total / (预算时间 × 单流吞吐))`，随文件大小生长
/// （返回 `< 2` 表示不值得分段，调用方回退单流）；实际切分仍由 `segment_count`
/// 按 `MIN_SEGMENT_BYTES` 二次收敛。
pub(crate) fn requested_segment_count(total: u64) -> usize {
	let budget_bytes = TARGET_DOWNLOAD_SECS.saturating_mul(OFFICIAL_SINGLE_STREAM_BPS);
	let k = (total as f64 / budget_bytes as f64).ceil() as usize;
	k.clamp(1, MAX_FILE_SEGMENTS)
}

/// 单次限速睡眠上限（秒）：保证暂停/取消能在一个小块间隔内被察觉。
const PACE_SLEEP_CAP: f64 = 0.25;
/// 位图落盘间隔（字节）：崩溃后最多丢这么多进度（暂停/取消时立即落盘）。
const META_FLUSH_BYTES: u64 = 2 * 1024 * 1024;

pub struct SegmentedRequest<'a> {
    pub url: &'a str,
    /// 最终落地路径（分段中间产物为其 `.part` / `.part.json` 兄弟文件）。
    pub dest: &'a Path,
    /// 期望并发段数；实际按总长收敛（每段 ≥ [`MIN_SEGMENT_BYTES`]，上限 [`MAX_SEGMENTS`]）。
    pub segments: usize,
    /// 断点续传：读 `<dest>.part.json` 跳过/接续已完成部分（URL/总长/切分不一致时忽略）。
    pub resume: bool,
    /// 取消令牌：每段每块检查，置位即停止（中间产物与进度位图保留）。
    pub cancel: Option<&'a AtomicBool>,
    /// 分段方案确定时回调（发起传输前）：`(是否分段, 实际段数)`，供调用方途中显示真实并发度。
    pub on_plan: Option<&'a (dyn Fn(bool, usize) + Send + Sync)>,
}

#[derive(Debug)]
pub struct SegmentedOutcome {
    pub bytes: u64,
    pub speed: u64,
    /// 是否真正走了 Range 分段（false = 回退单连接）。
    pub ranged: bool,
    /// 实际并发段数（回退时为 1）。
    pub segments: usize,
}

/// 分段中间产物位图（`.part.json`）：`progress[i]` 为第 i 段已写入的字节数。
#[derive(Serialize, Deserialize)]
struct PartMeta {
    url: String,
    total: u64,
    segments: usize,
    progress: Vec<u64>,
}

/// 分段下载入口：探测 → 分段并发 → 合并；不可分段时回退单连接。
pub async fn download_segmented<F>(
    req: SegmentedRequest<'_>,
    progress_cb: Option<F>,
) -> Result<SegmentedOutcome, FetchError>
where
    F: Fn(u64, u64) + Send + Sync,
{
    let SegmentedRequest {
        url,
        dest,
        segments,
        resume,
        cancel,
        on_plan,
    } = req;
    let started = Instant::now();
    let client = fetcher::http();

    // 与其它下载路径共用目标时排队（防并发写同一文件）
    let _path_guard = super::path_lock::wait_path_lock(dest).await;

    let plan = match probe_total(client.as_ref(), url).await {
        Some(total) if !fetcher::is_mirror_url(url) => segment_count(total, segments),
        _ => None,
    };
    let Some((total, count)) = plan else {
        // 回退同样上报方案，否则调用方途中会误显示为分段
        if let Some(cb) = on_plan {
            cb(false, 1);
        }
        return single_connection(client.as_ref(), url, dest, progress_cb, cancel, started).await;
    };
    if let Some(cb) = on_plan {
        cb(true, count);
    }

    // 多段共享同一回调：借出 trait object（不 clone，不要求 'static）
    let progress_ref: Option<&(dyn Fn(u64, u64) + Send + Sync)> = progress_cb
        .as_ref()
        .map(|f| f as &(dyn Fn(u64, u64) + Send + Sync));

    transfer_with_ranges(RangeTransfer {
        client,
        url,
        dest,
        total,
        count,
        resume,
        callback: progress_ref,
        cancel,
    })
    .await
}

/// 核心分段传输参数（字段多，收敛为结构体避免 `too_many_arguments`）。
pub(crate) struct RangeTransfer<'a> {
    pub client: Arc<Client>,
    pub url: &'a str,
    pub dest: &'a Path,
    pub total: u64,
    pub count: usize,
    pub resume: bool,
    /// 多段共享的进度回调（并发驱动）：要求 Send/Sync
    pub callback: Option<&'a (dyn Fn(u64, u64) + Send + Sync)>,
    pub cancel: Option<&'a AtomicBool>,
}

/// 核心分段传输：`total`/`count` 由调用方预探测给出，各段 `Range` 并发直写 `<dest>.part`
/// 并汇入位图，全部完成后 rename 到 dest；不持 path_lock（调用方负责互斥——fetch 核心分支
/// 的 batch worker 已持锁，download_segmented 自持；嵌套占用同路径会自旋死锁）。
pub(crate) async fn transfer_with_ranges(
    RangeTransfer {
        client,
        url,
        dest,
        total,
        count,
        resume,
        callback,
        cancel,
    }: RangeTransfer<'_>,
) -> Result<SegmentedOutcome, FetchError> {
    let started = Instant::now();

    let ranges = segment_ranges(total, count);
    let (part, meta_path) = partial_paths(dest);
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| FetchError::Io(format!("创建目录失败\n{e}")))?;
    }

    // 仅在 URL / 总长 / 划分数完全一致时才复用旧位图
    let mut progress = vec![0u64; count];
    if resume {
        if let Some(meta) = read_meta(&meta_path) {
            if meta.url == url && meta.total == total && meta.segments == count {
                progress = meta.progress;
            }
        }
    }

    // 预分配到总长：各段 seek 到自身偏移直写，无需拼接
    {
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            // 保留既有 .part 内容供续传复用，总长由 set_len 校正
            .truncate(false)
            .open(&part)
            .await
            .map_err(|e| FetchError::Io(format!("创建文件失败\n{e}")))?;
        file.set_len(total)
            .await
            .map_err(|e| FetchError::Io(format!("预分配文件失败\n{e}")))?;
    }

    // 本次会话起点的已落盘字节：counter 含此偏移用于进度绝对值，Pace 记录基线供限速扣除。
    let done_base: u64 = progress.iter().sum();
    let counter = Arc::new(AtomicU64::new(done_base));
    let pace = Arc::new(Pace::new(done_base));
    let writer = PartWriter {
        slots: Mutex::new(progress.clone()),
        path: meta_path.clone(),
        url: url.to_string(),
        total,
        segments: count,
    };

    let jobs = ranges
        .iter()
        .enumerate()
        .filter_map(|(index, &(start, end))| {
            let span = end - start + 1;
            let already = progress[index].min(span);
            if already >= span {
                // 该段此前已完整落盘，无需重下
                return None;
            }
            let client = Arc::clone(&client);
            let part = part.clone();
            let counter = Arc::clone(&counter);
            let pace = Arc::clone(&pace);
            let writer = &writer;
            let url = url.to_string();
            Some(async move {
                fetch_segment(SegmentJob {
                    client: client.as_ref(),
                    url: &url,
                    part: &part,
                    index,
                    start,
                    end,
                    already,
                    total,
                    counter: &counter,
                    pace: &pace,
                    writer,
                    cancel,
                    progress: callback,
                })
                .await
            })
        });

    let mut cancelled = false;
    for result in futures_util::future::join_all(jobs).await {
        match result {
            Ok(()) => {}
            Err(FetchError::Cancelled) => cancelled = true,
            // 任一段失败：保留 .part 与位图，下次 resume 从断点继续
            Err(e) => return Err(e),
        }
    }
    if cancelled {
        return Err(FetchError::Cancelled);
    }

    // 全部段就位：改名到最终路径并清位图
    let _ = std::fs::remove_file(&meta_path);
    std::fs::rename(&part, dest).map_err(|e| FetchError::Io(format!("完成写入失败\n{e}")))?;

    let elapsed = started.elapsed().as_secs_f64();
    Ok(SegmentedOutcome {
        bytes: total,
        speed: if elapsed > 0.0 {
            (total as f64 / elapsed) as u64
        } else {
            0
        },
        ranged: true,
        segments: count,
    })
}

/// 分段中间产物路径（`.part` 数据 + `.part.json` 位图）；调用方取消时清理，暂停/失败保留以支持续传。
pub fn partial_paths(dest: &Path) -> (PathBuf, PathBuf) {
    (append_suffix(dest, ".part"), append_suffix(dest, ".part.json"))
}

/// 位图写入器：记录某段已完成字节并落盘（`.part.json`）。
struct PartWriter {
    slots: Mutex<Vec<u64>>,
    path: PathBuf,
    url: String,
    total: u64,
    segments: usize,
}

impl PartWriter {
    /// 记录第 `index` 段已完成 `done` 字节并落盘（写失败仅丢进度记录，不影响本次下载）。
    fn record(&self, index: usize, done: u64) {
        let snapshot = {
            let mut slots = self.slots.lock();
            slots[index] = done;
            PartMeta {
                url: self.url.clone(),
                total: self.total,
                segments: self.segments,
                progress: slots.clone(),
            }
        };
        let _ = write_meta(&self.path, &snapshot);
    }
}

/// 单段下载上下文（字段多，收敛为结构体避免 `too_many_arguments`）。
struct SegmentJob<'a> {
    client: &'a Client,
    url: &'a str,
    part: &'a Path,
    index: usize,
    /// 段起始偏移（含）
    start: u64,
    /// 段结束偏移（含）
    end: u64,
    /// 本段此前已写入的字节数（续传起点）
    already: u64,
    total: u64,
    counter: &'a AtomicU64,
    pace: &'a Pace,
    writer: &'a PartWriter,
    cancel: Option<&'a AtomicBool>,
    progress: Option<&'a (dyn Fn(u64, u64) + Send + Sync)>,
}

/// 下载单个段：`Range` 请求（从断点起）→ seek 到段偏移顺序写 → 限速/取消检查；完成/取消/周期节点均落盘本段进度。
async fn fetch_segment(job: SegmentJob<'_>) -> Result<(), FetchError> {
    let SegmentJob {
        client,
        url,
        part,
        index,
        start,
        end,
        already,
        total,
        counter,
        pace,
        writer,
        cancel,
        progress,
    } = job;
    let request_start = start + already;
    let expected = end - request_start + 1;

    let response = client
        .get(url)
        .header(RANGE, format!("bytes={request_start}-{end}"))
        .send()
        .await
        .map_err(|e| FetchError::Network(format!("请求失败\n{e}")))?;
    // 探测已确认服务端支持 Range；此处再收 200 说明中间层（CDN/代理）吃掉了 Range 头
    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(FetchError::Network(format!(
            "服务器未按分段响应\nHTTP {}",
            response.status()
        )));
    }
    // 206 的 body 长度必须等于请求范围，否则该段尾部会留下零填充（静默损坏）
    if let Some(len) = response.content_length() {
        if len != expected {
            return Err(FetchError::Network(format!(
                "分段响应长度不符\n期望 {expected} 字节，服务端声明 {len} 字节"
            )));
        }
    }

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(part)
        .await
        .map_err(|e| FetchError::Io(format!("打开文件失败\n{e}")))?;
    file.seek(SeekFrom::Start(request_start))
        .await
        .map_err(|e| FetchError::Io(format!("定位写入位置失败\n{e}")))?;

    let mut stream = response.bytes_stream();
    let mut written: u64 = 0;
    let mut last_flush: u64 = 0;
    while let Some(chunk) = stream.next().await {
        // 取消立即生效：当前块读完即停（进度落盘，保留中间产物）
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            writer.record(index, already + written);
            return Err(FetchError::Cancelled);
        }
        let chunk = chunk.map_err(|e| FetchError::Network(format!("请求失败\n{e}")))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| FetchError::Io(format!("写入文件失败\n{e}")))?;
        written = written.saturating_add(chunk.len() as u64);
        let global = counter.fetch_add(chunk.len() as u64, Ordering::Relaxed) + chunk.len() as u64;
        if let Some(cb) = progress {
            cb(global, total);
        }
        // 周期落盘：崩溃后最多丢 META_FLUSH_BYTES 的进度记录
        if written - last_flush >= META_FLUSH_BYTES {
            last_flush = written;
            writer.record(index, already + written);
        }
        pace.wait(global).await;
    }
    file.flush()
        .await
        .map_err(|e| FetchError::Io(format!("写入文件失败\n{e}")))?;

    if written != expected {
        return Err(FetchError::Network(format!(
            "分段数据不完整\n期望 {expected} 字节，实收 {written} 字节"
        )));
    }
    writer.record(index, already + written);
    Ok(())
}

/// 全局限速状态：所有段共享同一时间基准，按「全局累计字节 ÷ 限速」推算目标时刻并补睡；`base` 为会话起点已落盘字节（续传偏移），目标按会话字节推算，否则续传首块即被 PACE_SLEEP_CAP 钉在低速。
struct Pace {
    started: Instant,
    base: u64,
}

impl Pace {
    fn new(base: u64) -> Self {
        Self {
            started: Instant::now(),
            base,
        }
    }

    /// 按本次会话已传字节推算的目标时刻（秒）：`global_bytes` 含续传偏移，先减去基线。
    fn target_secs(&self, global_bytes: u64, limit: u64) -> f64 {
        global_bytes.saturating_sub(self.base) as f64 / limit as f64
    }

    async fn wait(&self, global_bytes: u64) {
        let limit = fetcher::speed_limit();
        if limit == 0 {
            return;
        }
        let target = self.target_secs(global_bytes, limit);
        let elapsed = self.started.elapsed().as_secs_f64();
        if elapsed < target {
            let sleep = (target - elapsed).min(PACE_SLEEP_CAP);
            tokio::time::sleep(Duration::from_secs_f64(sleep)).await;
        }
    }
}

/// 探测是否支持 Range 并取总长：`GET Range: bytes=0-0` 返回 206 时从 `Content-Range` 取总长；其余（200/4xx/网络错）返回 `None`，交调用方回退单连接。
async fn probe_total(client: &Client, url: &str) -> Option<u64> {
    let response = client.get(url).header(RANGE, "bytes=0-0").send().await.ok()?;
    if response.status() != StatusCode::PARTIAL_CONTENT {
        return None;
    }
    let value = response.headers().get(CONTENT_RANGE)?.to_str().ok()?;
    value
        .rsplit('/')
        .next()?
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
}

/// 回退路径：单连接流式（保留 3 次重试 + 失败清残留 + 每块取消检查）。
async fn single_connection<F>(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: Option<F>,
    cancel: Option<&AtomicBool>,
    started: Instant,
) -> Result<SegmentedOutcome, FetchError>
where
    F: Fn(u64, u64) + Send + Sync,
{
    let dl = fetcher::fetch_with_c(client, url, dest, progress, None, cancel).await?;
    let elapsed = started.elapsed().as_secs_f64();
    Ok(SegmentedOutcome {
        bytes: dl.bytes,
        speed: if elapsed > 0.0 {
            (dl.bytes as f64 / elapsed) as u64
        } else {
            dl.speed
        },
        ranged: false,
        segments: 1,
    })
}

/// 按总长与期望段数决定实际切分：每段不低于 [`MIN_SEGMENT_BYTES`]，段数不足 2 时返回 `None`（回退单连接）。
fn segment_count(total: u64, requested: usize) -> Option<(u64, usize)> {
    let by_size = (total / MIN_SEGMENT_BYTES) as usize;
    let count = requested.clamp(1, MAX_SEGMENTS).min(by_size);
    (count >= 2).then_some((total, count))
}

/// 均匀切分 `[0, total)` 为 `count` 段：第 i 段 `[i*total/count, (i+1)*total/count - 1]`，各段长度差 ≤ 1 且恰好覆盖全文件。
fn segment_ranges(total: u64, count: usize) -> Vec<(u64, u64)> {
    let count = count as u64;
    (0..count)
        .map(|i| (i * total / count, (i + 1) * total / count - 1))
        .collect()
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

fn read_meta(path: &Path) -> Option<PartMeta> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_meta(path: &Path, meta: &PartMeta) -> std::io::Result<()> {
    let raw = serde_json::to_string(meta)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tokio::io::AsyncReadExt as _;
    use tokio::net::TcpListener;

    async fn serve(body: Vec<u8>, support_range: bool) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = ready_tx.send(());
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let body = body.clone();
                let hits = Arc::clone(&counter);
                tokio::spawn(async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    let text = match read_head(&mut sock).await {
                        Some(t) => t,
                        None => return,
                    };
                    let total = body.len() as u64;
                    let (head, payload) = match parse_range(&text, total) {
                        Some((start, end)) if support_range => (
                            format!(
                                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\n\
                                 Content-Range: bytes {start}-{end}/{total}\r\n\
                                 Accept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                                end - start + 1
                            ),
                            body[start as usize..=end as usize].to_vec(),
                        ),
                        _ => (
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\n{}\
                                 Connection: close\r\n\r\n",
                                if support_range {
                                    "Accept-Ranges: bytes\r\n"
                                } else {
                                    ""
                                }
                            ),
                            body.clone(),
                        ),
                    };
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(&payload).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        let _ = ready_rx.await;
        (format!("http://{addr}/file.bin"), hits)
    }

    async fn read_head(sock: &mut tokio::net::TcpStream) -> Option<String> {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let n = sock.read(&mut chunk).await.ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                return Some(String::from_utf8_lossy(&buf).into_owned());
            }
        }
    }

    fn parse_range(head: &str, total: u64) -> Option<(u64, u64)> {
        let line = head
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("range:"))?;
        let spec = line.split_once('=')?.1.trim();
        let (raw_start, raw_end) = spec.split_once('-')?;
        let start: u64 = raw_start.trim().parse().ok()?;
        let end = if raw_end.trim().is_empty() {
            total.saturating_sub(1)
        } else {
            raw_end.trim().parse::<u64>().ok()?.min(total - 1)
        };
        (start <= end).then_some((start, end))
    }

    fn temp_dest(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("{}_{}.bin", name, std::process::id()));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(append_suffix(&path, ".part"));
        let _ = std::fs::remove_file(append_suffix(&path, ".part.json"));
        path
    }

    fn sample_body(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn segment_ranges_cover_file_exactly() {
        let ranges = segment_ranges(1000, 8);
        assert_eq!(ranges.len(), 8);
        assert_eq!(ranges[0].0, 0, "首段应从 0 开始");
        assert_eq!(ranges[7].1, 999, "末段应到 total-1");
        for pair in ranges.windows(2) {
            assert_eq!(pair[0].1 + 1, pair[1].0, "相邻段必须首尾相接");
        }
        let sum: u64 = ranges.iter().map(|(a, b)| b - a + 1).sum();
        assert_eq!(sum, 1000, "各段长度之和应等于总长");
    }

    #[test]
    fn segment_count_rejects_small_files() {
        assert!(segment_count(MIN_SEGMENT_BYTES - 1, 16).is_none());
        assert!(segment_count(MIN_SEGMENT_BYTES, 16).is_none());
        assert!(segment_count(MIN_SEGMENT_BYTES * 2, 1).is_none());
        assert_eq!(
            segment_count(MIN_SEGMENT_BYTES * 3, 16),
            Some((MIN_SEGMENT_BYTES * 3, 3))
        );
        assert_eq!(
            segment_count(MIN_SEGMENT_BYTES * 100, 999),
            Some((MIN_SEGMENT_BYTES * 100, MAX_SEGMENTS))
        );
    }

    /// 段数随文件大小生长（时间预算 × 单流吞吐推导）：< 预算不分段、大文件收敛到上限。
    #[test]
    fn requested_segment_count_scales_with_size() {
        let budget = TARGET_DOWNLOAD_SECS.saturating_mul(OFFICIAL_SINGLE_STREAM_BPS);
        // 低于预算：1 段（调用方据此回退单流）
        assert_eq!(requested_segment_count(budget - 1), 1);
        assert_eq!(requested_segment_count(budget), 1);
        // 2×/5× 预算：2/5 段
        assert_eq!(requested_segment_count(budget * 2), 2);
        assert_eq!(requested_segment_count(budget * 5), 5);
        // 远超预算：收敛到上限 8
        assert_eq!(requested_segment_count(budget * 100), 8);
    }

    #[test]
    fn part_meta_roundtrip() {
        let dest = temp_dest("seg_meta");
        let (_, meta_path) = partial_paths(&dest);
        let meta = PartMeta {
            url: "https://example.com/a.bin".into(),
            total: 4096,
            segments: 3,
            progress: vec![1024, 0, 2048],
        };
        write_meta(&meta_path, &meta).expect("写入位图");
        let back = read_meta(&meta_path).expect("读回位图");
        assert_eq!(back.url, meta.url);
        assert_eq!(back.total, meta.total);
        assert_eq!(back.segments, meta.segments);
        assert_eq!(back.progress, meta.progress);
        let _ = std::fs::remove_file(&meta_path);
    }

    #[tokio::test]
    async fn downloads_via_ranges_from_local_server() {
        let body = sample_body(8 * 1024 * 1024);
        let (url, hits) = serve(body.clone(), true).await;
        let dest = temp_dest("seg_ranged");
        let out = download_segmented(
            SegmentedRequest {
                url: &url,
                dest: &dest,
                segments: 4,
                resume: false,
                cancel: None,
                on_plan: None,
            },
            None::<fn(u64, u64)>,
        )
        .await
        .expect("分段下载应成功");

        assert!(out.ranged, "应走分段路径");
        assert_eq!(out.segments, 4, "应按请求切 4 段");
        assert_eq!(out.bytes, body.len() as u64);
        assert_eq!(
            std::fs::read(&dest).expect("读回文件"),
            body,
            "分段并发写内容必须一致"
        );
        assert!(
            hits.load(Ordering::SeqCst) >= 5,
            "应至少 5 次请求（1 次探测 + 4 段），实际 {}",
            hits.load(Ordering::SeqCst)
        );
        let _ = std::fs::remove_file(&dest);
    }

    #[tokio::test]
    async fn falls_back_when_server_ignores_range() {
        let body = sample_body(64 * 1024);
        let (url, hits) = serve(body.clone(), false).await;
        let dest = temp_dest("seg_fallback");
        let out = download_segmented(
            SegmentedRequest {
                url: &url,
                dest: &dest,
                segments: 8,
                resume: false,
                cancel: None,
                on_plan: None,
            },
            None::<fn(u64, u64)>,
        )
        .await
        .expect("回退下载应成功");

        assert!(!out.ranged, "不支持 Range 应回退单连接");
        assert_eq!(out.segments, 1);
        assert_eq!(std::fs::read(&dest).expect("读回文件"), body);
        assert_eq!(hits.load(Ordering::SeqCst), 2, "1 次探测 + 1 次完整下载");
        let _ = std::fs::remove_file(&dest);
    }

    #[tokio::test]
    async fn resume_skips_completed_segments() {
        let body = sample_body(8 * 1024 * 1024);
        let (url, hits) = serve(body.clone(), true).await;
        let dest = temp_dest("seg_resume");
        let (part, meta_path) = partial_paths(&dest);
        let span = body.len() as u64 / 4;

        std::fs::write(&part, &body).expect("写入 .part");
        write_meta(
            &meta_path,
            &PartMeta {
                url: url.clone(),
                total: body.len() as u64,
                segments: 4,
                progress: vec![span, 0, span, 0],
            },
        )
        .expect("写入位图");

        let out = download_segmented(
            SegmentedRequest {
                url: &url,
                dest: &dest,
                segments: 4,
                resume: true,
                cancel: None,
                on_plan: None,
            },
            None::<fn(u64, u64)>,
        )
        .await
        .expect("续传应成功");

        assert!(out.ranged);
        assert_eq!(
            std::fs::read(&dest).expect("读回文件"),
            body,
            "续传后内容完整"
        );
        assert_eq!(
            hits.load(Ordering::SeqCst),
            3,
            "1 次探测 + 仅 2 个未完成段"
        );
        let _ = std::fs::remove_file(&dest);
    }

    #[tokio::test]
    async fn resume_continues_inside_segment() {
        let body = sample_body(8 * 1024 * 1024);
        let (url, hits) = serve(body.clone(), true).await;
        let dest = temp_dest("seg_resume_inner");
        let (part, meta_path) = partial_paths(&dest);
        let total = body.len() as u64;
        let span = total / 4;
        let half = span / 2;

        std::fs::write(&part, &body).expect("写入 .part");
        write_meta(
            &meta_path,
            &PartMeta {
                url: url.clone(),
                total,
                segments: 4,
                progress: vec![half, 0, half, 0],
            },
        )
        .expect("写入位图");

        let out = download_segmented(
            SegmentedRequest {
                url: &url,
                dest: &dest,
                segments: 4,
                resume: true,
                cancel: None,
                on_plan: None,
            },
            None::<fn(u64, u64)>,
        )
        .await
        .expect("段内续传应成功");

        assert_eq!(out.bytes, total);
        assert_eq!(std::fs::read(&dest).expect("读回文件"), body, "段内续传后内容完整");
        assert_eq!(
            hits.load(Ordering::SeqCst),
            5,
            "1 次探测 + 4 段（两段从半途请求，两段全量）"
        );
        let _ = std::fs::remove_file(&dest);
    }

    #[tokio::test]
    async fn resume_ignores_stale_meta() {
        let body = sample_body(8 * 1024 * 1024);
        let (url, hits) = serve(body.clone(), true).await;
        let dest = temp_dest("seg_stale");
        let (part, meta_path) = partial_paths(&dest);
        let span = body.len() as u64 / 4;
        std::fs::write(&part, &body).expect("写入 .part");
        write_meta(
            &meta_path,
            &PartMeta {
                url: "https://other.example.com/other.bin".into(),
                total: body.len() as u64,
                segments: 4,
                progress: vec![span, span, span, 0],
            },
        )
        .expect("写入位图");

        download_segmented(
            SegmentedRequest {
                url: &url,
                dest: &dest,
                segments: 4,
                resume: true,
                cancel: None,
                on_plan: None,
            },
            None::<fn(u64, u64)>,
        )
        .await
        .expect("下载应成功");

        assert_eq!(
            hits.load(Ordering::SeqCst),
            5,
            "位图 URL 不匹配应作废（1 探测 + 4 段全下）"
        );
        let _ = std::fs::remove_file(&dest);
        let _ = std::fs::remove_file(&meta_path);
    }

    /// 方案回调须在传输开始前上报真实并发度（分段与回退两路）：否则途中只能显示初始值。
    #[tokio::test]
    async fn on_plan_reports_decided_shape() {
        let body = sample_body(8 * 1024 * 1024);
        let (url, _) = serve(body, true).await;
        let dest = temp_dest("seg_plan");
        let seen = Arc::new(Mutex::new(Vec::<(bool, usize)>::new()));
        let sink = Arc::clone(&seen);
        let on_plan = move |ranged: bool, count: usize| sink.lock().push((ranged, count));
        download_segmented(
            SegmentedRequest {
                url: &url,
                dest: &dest,
                segments: 4,
                resume: false,
                cancel: None,
                on_plan: Some(&on_plan),
            },
            None::<fn(u64, u64)>,
        )
        .await
        .expect("分段下载应成功");
        assert_eq!(
            seen.lock().as_slice(),
            &[(true, 4)],
            "分段路径应上报 (true, 4)"
        );

        let body = sample_body(64 * 1024);
        let (url, _) = serve(body, false).await;
        let dest2 = temp_dest("seg_plan_fallback");
        let seen2 = Arc::new(Mutex::new(Vec::<(bool, usize)>::new()));
        let sink2 = Arc::clone(&seen2);
        let on_plan2 = move |ranged: bool, count: usize| sink2.lock().push((ranged, count));
        download_segmented(
            SegmentedRequest {
                url: &url,
                dest: &dest2,
                segments: 8,
                resume: false,
                cancel: None,
                on_plan: Some(&on_plan2),
            },
            None::<fn(u64, u64)>,
        )
        .await
        .expect("回退下载应成功");
        assert_eq!(
            seen2.lock().as_slice(),
            &[(false, 1)],
            "回退路径应上报 (false, 1)"
        );

        let _ = std::fs::remove_file(&dest);
        let _ = std::fs::remove_file(&dest2);
    }

    /// 续传限速：目标按「本次会话已传字节」推算（误用累计值会配合 PACE_SLEEP_CAP 钉在低速）；断言基线被扣除。
    #[test]
    fn pace_target_excludes_resume_offset() {
        let limit = 1024 * 1024; // 1 MiB/s
        let base = 4 * 1024 * 1024; // 续传起点：已落盘 4 MiB
        let pace = Pace::new(base);
        let t = pace.target_secs(base + 128 * 1024, limit);
        assert!((t - 0.125).abs() < 1e-9, "应按会话字节计，实际 {t}");
    }
}
