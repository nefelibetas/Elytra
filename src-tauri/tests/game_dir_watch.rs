//! 游戏目录监听端到端：真实文件系统事件驱动（临时游戏目录 + 记录型事件出口）。
//! 监听是进程级单例且依赖全局配置，故只用一个用例覆盖完整链路。

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use elytra_lib::config::game_dirs;
use elytra_lib::game::version::watch;
use elytra_lib::task::EventSink;

/// 记录型事件出口（替代 `AppHandle`）。
struct Recorder {
    tx: Sender<(String, serde_json::Value)>,
}

impl EventSink for Recorder {
    fn send(&self, event: &'static str, payload: serde_json::Value) {
        let _ = self.tx.send((event.to_string(), payload));
    }
}

/// 建临时游戏目录并起监听（进程级单例只初始化一次）。
fn harness() -> (PathBuf, &'static Mutex<Receiver<(String, serde_json::Value)>>) {
    static MC_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        let dir = tempfile::tempdir().unwrap().keep();
        std::fs::create_dir_all(dir.join("versions")).unwrap();
        game_dirs::set_active(&dir.to_string_lossy()).unwrap();
        dir
    });
    static RX: LazyLock<Mutex<Receiver<(String, serde_json::Value)>>> = LazyLock::new(|| {
        let (tx, rx) = channel();
        watch::init_with_sink(Arc::new(Recorder { tx }));
        watch::retarget();
        Mutex::new(rx)
    });
    let dir = MC_DIR.clone();
    LazyLock::force(&RX);
    (dir, &RX)
}

/// 丢弃已到达的事件（就绪前的噪声）。
fn drain(rx: &Mutex<Receiver<(String, serde_json::Value)>>) {
    let guard = rx.lock();
    while guard.try_recv().is_ok() {}
}

/// 等指定事件；超时则打印已收到的事件名（诊断用）。
fn wait_event(
    rx: &Mutex<Receiver<(String, serde_json::Value)>>,
    event: &str,
    timeout: Duration,
) -> serde_json::Value {
    let deadline = Instant::now() + timeout;
    let mut seen: Vec<String> = Vec::new();
    loop {
        let remain = deadline.saturating_duration_since(Instant::now());
        let got = rx.lock().recv_timeout(remain);
        match got {
            Ok((name, payload)) if name == event => return payload,
            Ok((name, _)) => seen.push(name),
            Err(RecvTimeoutError::Timeout) => {
                panic!("等待事件 {event} 超时；已收到：{seen:?}")
            }
            Err(RecvTimeoutError::Disconnected) => panic!("事件出口已断开"),
        }
    }
}

/// 解析 payload 里版本条目的名字。
fn names_of(payload: &serde_json::Value) -> Vec<String> {
    payload["versions"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|v| v["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn watch_pushes_versions_and_resource_events() {
    let (mc_dir, rx) = harness();
    // 等监听目标应用（retarget 经通道异步生效）
    std::thread::sleep(Duration::from_millis(800));
    drain(rx);

    // 外部新增版本目录 → 列表事件（payload 即最新全量列表）
    let version_dir = mc_dir.join("versions").join("1.21.4");
    std::fs::create_dir_all(&version_dir).unwrap();
    let payload = wait_event(rx, "versions-changed", Duration::from_secs(15));
    assert!(
        names_of(&payload).iter().any(|n| n == "1.21.4"),
        "payload: {payload}"
    );

    // 聚焦该版本后，其一级资源子目录变化 → 资源事件
    watch::set_focus(Some("1.21.4".to_string()));
    std::thread::sleep(Duration::from_millis(800));
    drain(rx);
    std::fs::create_dir_all(version_dir.join("mods")).unwrap();
    let payload = wait_event(rx, "version-dir-changed", Duration::from_secs(15));
    assert_eq!(payload["version"], "1.21.4");
    let kinds: Vec<&str> = payload["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|k| k.as_str())
        .collect();
    assert!(kinds.contains(&"mods"), "payload: {payload}");

    // 外部删除版本目录 → 同样刷新列表
    drain(rx);
    std::fs::remove_dir_all(&version_dir).unwrap();
    let payload = wait_event(rx, "versions-changed", Duration::from_secs(15));
    assert!(
        !names_of(&payload).iter().any(|n| n == "1.21.4"),
        "payload: {payload}"
    );

    watch::set_focus(None);
}
