//! 工具箱「多线程下载器」端到端测试：入队 → 并发调度 → 分段下载 → 进度事件 → 终态。
//! 默认用例走本地 HTTP 服务（可控 206/200 与节流速率，不依赖外网），驱动 `tooldl` 域真实调度与
//! 下载链路，事件出口注入记录型实现（不触碰 IPC）。`real_network_segmented_download` 走真实 CDN：
//! `cargo test --test tool_download -- --ignored --nocapture`

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

use elytra_lib::task::EventSink;
use elytra_lib::tooldl::{self, ToolDlSnapshot, ToolDlStatus};

/// 测试串行闸：`tooldl` 的注册表 / 并发上限 / 槽位计数都是进程级全局，用例之间必须互斥
/// （否则 clear_finished 会清掉别人的任务）。
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// 事件出口：记录 payload 供断言（不触达任何 IPC）。
#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<serde_json::Value>>,
}

impl EventSink for RecordingSink {
    fn send(&self, _event: &'static str, payload: serde_json::Value) {
        self.events.lock().push(payload);
    }
}

impl RecordingSink {
    fn saw_status(&self, want: &str) -> bool {
        self.events
            .lock()
            .iter()
            .any(|v| v["status"].as_str() == Some(want))
    }

    fn last_status(&self) -> Option<String> {
        self.events
            .lock()
            .last()
            .and_then(|v| v["status"].as_str())
            .map(str::to_string)
    }
}

/// 本地 HTTP 服务：`support_range` 为真时按 `Range` 回 206 分片，否则 200 全量；
/// `pace` 为 Some 时按块节流（制造可观测的下载中窗口）。返回 URL 与累计请求数。
async fn serve(
    body: Vec<u8>,
    support_range: bool,
    pace: Option<(usize, Duration)>,
) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("绑定本地端口");
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
                let Some(text) = read_head(&mut sock).await else {
                    return;
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
                if sock.write_all(head.as_bytes()).await.is_err() {
                    return;
                }
                match pace {
                    Some((chunk, delay)) => {
                        for piece in payload.chunks(chunk) {
                            if sock.write_all(piece).await.is_err() {
                                return;
                            }
                            tokio::time::sleep(delay).await;
                        }
                    }
                    None => {
                        let _ = sock.write_all(&payload).await;
                    }
                }
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

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tooldl_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建临时目录");
    dir
}

fn sample_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

fn find(id: &str) -> ToolDlSnapshot {
    tooldl::list()
        .into_iter()
        .find(|t| t.id == id)
        .unwrap_or_else(|| panic!("任务 {id} 不在注册表中"))
}

/// 轮询等待任务满足条件（避免固定 sleep 造成的抖动）。
async fn wait_for(
    id: &str,
    timeout: Duration,
    pred: impl Fn(&ToolDlSnapshot) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(task) = tooldl::list().into_iter().find(|t| t.id == id) {
            if pred(&task) {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    false
}

/// 完整链路：入队 → 分段下载 → 完成态 + 内容一致 + 进度事件齐备。
#[tokio::test(flavor = "multi_thread")]
async fn enqueue_segmented_download_completes_with_progress_events() {
    let _guard = TEST_LOCK.lock();
    let body = sample_bytes(8 * 1024 * 1024);
    let (url, hits) = serve(body.clone(), true, None).await;
    let dir = temp_dir("ok");
    let sink = Arc::new(RecordingSink::default());

    tooldl::set_max_files(2);
    let ids = tooldl::enqueue(sink.clone(), vec![url], dir.to_string_lossy().to_string(), 4).expect("入队");
    assert_eq!(ids.len(), 1);
    let id = &ids[0];

    assert!(
        wait_for(id, Duration::from_secs(60), |t| t.status == ToolDlStatus::Done).await,
        "任务应在超时内完成，当前状态 {:?}",
        tooldl::list().into_iter().find(|t| t.id == *id).map(|t| t.status)
    );

    let snap = find(id);
    assert!(snap.ranged, "服务端支持 Range 时应走分段");
    assert_eq!(snap.segments, 4, "应按请求切 4 段");
    assert_eq!(snap.downloaded, body.len() as u64, "完成字节数应等于文件长度");
    assert_eq!(
        std::fs::read(&snap.dest).expect("读回下载文件"),
        body,
        "分段并发写出的内容必须与源逐字节一致"
    );
    assert!(
        hits.load(Ordering::SeqCst) >= 5,
        "应发生 1 次探测 + 4 次分段请求，实际 {}",
        hits.load(Ordering::SeqCst)
    );

    assert!(sink.saw_status("downloading"), "应派发过下载中快照");
    assert_eq!(
        sink.last_status().as_deref(),
        Some("done"),
        "末次事件应为完成态"
    );

    tooldl::clear_finished();
    let _ = std::fs::remove_dir_all(&dir);
}

/// 暂停保留断点，继续后从断点续传并最终内容完整。
#[tokio::test(flavor = "multi_thread")]
async fn pause_keeps_partial_then_resume_finishes() {
    let _guard = TEST_LOCK.lock();
    let body = sample_bytes(8 * 1024 * 1024);
    let (url, _) = serve(body.clone(), true, Some((32 * 1024, Duration::from_millis(20)))).await;
    let dir = temp_dir("pause");
    let sink = Arc::new(RecordingSink::default());

    tooldl::set_max_files(2);
    let id = tooldl::enqueue(sink.clone(), vec![url], dir.to_string_lossy().to_string(), 4)
        .expect("入队")
        .remove(0);

    assert!(
        wait_for(&id, Duration::from_secs(30), |t| {
            t.status == ToolDlStatus::Downloading && t.downloaded > 0
        })
        .await,
        "应进入下载且已有进度"
    );

    tooldl::pause(&id).expect("暂停请求");
    assert!(
        wait_for(&id, Duration::from_secs(30), |t| t.status == ToolDlStatus::Paused).await,
        "应进入暂停态，当前 {:?}",
        find(&id).status
    );

    let paused = find(&id);
    assert!(paused.downloaded > 0, "暂停时应有已完成部分");
    assert!(
        paused.downloaded < body.len() as u64,
        "暂停时不应已下完（节流服务保证有窗口）"
    );
    let part = PathBuf::from(format!("{}.part", paused.dest));
    let meta = PathBuf::from(format!("{}.part.json", paused.dest));
    assert!(part.exists(), "暂停必须保留 .part 供断点续传");
    assert!(meta.exists(), "暂停必须保留位图");

    tooldl::resume(sink.clone(), &id).expect("继续");
    assert!(
        wait_for(&id, Duration::from_secs(180), |t| t.status == ToolDlStatus::Done).await,
        "续传应完成，当前 {:?}",
        find(&id).status
    );

    let done = find(&id);
    assert_eq!(
        std::fs::read(&done.dest).expect("读回文件"),
        body,
        "续传后内容必须完整"
    );
    assert!(!part.exists(), "完成后 .part 应被改名清理");
    assert!(!meta.exists(), "完成后位图应被清理");

    tooldl::clear_finished();
    let _ = std::fs::remove_dir_all(&dir);
}

/// 取消清理半成品与中间产物，不留残留。
#[tokio::test(flavor = "multi_thread")]
async fn cancel_cleans_partial_files() {
    let _guard = TEST_LOCK.lock();
    let body = sample_bytes(8 * 1024 * 1024);
    let (url, _) = serve(body, true, Some((32 * 1024, Duration::from_millis(20)))).await;
    let dir = temp_dir("cancel");
    let sink = Arc::new(RecordingSink::default());

    tooldl::set_max_files(2);
    let id = tooldl::enqueue(sink, vec![url], dir.to_string_lossy().to_string(), 4)
        .expect("入队")
        .remove(0);

    assert!(
        wait_for(&id, Duration::from_secs(30), |t| {
            t.status == ToolDlStatus::Downloading && t.downloaded > 0
        })
        .await,
        "应进入下载且已有进度"
    );

    tooldl::cancel(&id).expect("取消");
    assert!(
        wait_for(&id, Duration::from_secs(30), |t| t.status == ToolDlStatus::Cancelled).await,
        "应进入取消态，当前 {:?}",
        find(&id).status
    );

    let snap = find(&id);
    assert!(
        !PathBuf::from(format!("{}.part", snap.dest)).exists(),
        "取消必须清理 .part"
    );
    assert!(
        !PathBuf::from(format!("{}.part.json", snap.dest)).exists(),
        "取消必须清理位图"
    );
    assert!(!PathBuf::from(&snap.dest).exists(), "取消不应留下目标文件");

    tooldl::clear_finished();
    let _ = std::fs::remove_dir_all(&dir);
}

/// 服务端不支持 Range：整条链路回退单连接，内容仍完整。
#[tokio::test(flavor = "multi_thread")]
async fn falls_back_to_single_connection() {
    let _guard = TEST_LOCK.lock();
    let body = sample_bytes(256 * 1024);
    let (url, _) = serve(body.clone(), false, None).await;
    let dir = temp_dir("fallback");
    let sink = Arc::new(RecordingSink::default());

    tooldl::set_max_files(2);
    let id = tooldl::enqueue(sink, vec![url], dir.to_string_lossy().to_string(), 8)
        .expect("入队")
        .remove(0);

    assert!(
        wait_for(&id, Duration::from_secs(60), |t| t.status == ToolDlStatus::Done).await,
        "回退下载应完成"
    );

    let snap = find(&id);
    assert!(!snap.ranged, "不支持 Range 应回退单连接");
    assert_eq!(snap.segments, 1, "回退路径固定单连接");
    assert_eq!(std::fs::read(&snap.dest).expect("读回文件"), body);

    tooldl::clear_finished();
    let _ = std::fs::remove_dir_all(&dir);
}

/// 真实 CDN 冒烟：验证真实服务端的 Range 行为与分段落地（需手动 `--ignored`）。
#[tokio::test(flavor = "multi_thread")]
#[ignore = "真实网络：cargo test --test tool_download -- --ignored --nocapture"]
async fn real_network_segmented_download() {
    let _guard = TEST_LOCK.lock();
    // OVH 公共测速文件（长期在线、支持 Range）
    let url = "https://proof.ovh.net/files/10Mb.dat".to_string();
    let dir = temp_dir("real");
    let sink = Arc::new(RecordingSink::default());

    tooldl::set_max_files(1);
    let id = tooldl::enqueue(sink, vec![url], dir.to_string_lossy().to_string(), 8)
        .expect("入队")
        .remove(0);

    assert!(
        wait_for(&id, Duration::from_secs(300), |t| t.status == ToolDlStatus::Done).await,
        "真实下载应完成，当前 {:?}",
        find(&id).status
    );

    let snap = find(&id);
    println!(
        "真实下载结果：ranged={} segments={} bytes={} speed={} B/s",
        snap.ranged, snap.segments, snap.downloaded, snap.speed
    );
    assert!(snap.ranged, "10Mb.dat 实测支持 Range，应走分段");
    assert!(
        snap.segments > 1,
        "应实际切了多段，实际 {}",
        snap.segments
    );
    let meta = std::fs::metadata(&snap.dest).expect("目标文件");
    assert_eq!(meta.len(), 10 * 1024 * 1024, "下载结果大小应为 10 MiB");

    tooldl::clear_finished();
    let _ = std::fs::remove_dir_all(&dir);
}

/// 下载**途中**就必须显示真实并发度：`ranged` / `segments_used` 曾只在终态收口里写入，
/// 导致下载中一直显示「单连接」，完成才跳成「8 线程分段」。
#[tokio::test(flavor = "multi_thread")]
async fn segment_plan_visible_while_downloading() {
    let _guard = TEST_LOCK.lock();
    let body = sample_bytes(8 * 1024 * 1024);
    let (url, _) = serve(body, true, Some((32 * 1024, Duration::from_millis(20)))).await;
    let dir = temp_dir("plan");
    let sink = Arc::new(RecordingSink::default());

    tooldl::set_max_files(2);
    let id = tooldl::enqueue(sink, vec![url], dir.to_string_lossy().to_string(), 4)
        .expect("入队")
        .remove(0);

    assert!(
        wait_for(&id, Duration::from_secs(30), |t| {
            t.status == ToolDlStatus::Downloading && t.downloaded > 0
        })
        .await,
        "应进入下载且已有进度"
    );

    let mid = find(&id);
    assert_eq!(
        mid.status,
        ToolDlStatus::Downloading,
        "取快照时任务应仍在下载中（否则本用例失去意义）"
    );
    assert!(
        mid.ranged,
        "下载途中就必须已标记为分段，实际 snapshot = {mid:?}"
    );
    assert_eq!(mid.segments, 4, "下载途中应显示实际段数（而非请求值/占位值）");

    tooldl::cancel(&id).expect("取消");
    assert!(
        wait_for(&id, Duration::from_secs(30), |t| t.status == ToolDlStatus::Cancelled).await,
        "取消应生效"
    );

    tooldl::clear_finished();
    let _ = std::fs::remove_dir_all(&dir);
}
