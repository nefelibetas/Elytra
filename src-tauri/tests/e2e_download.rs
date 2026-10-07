//! `run_task` 端到端测试（真实网络 + 真实下载编排）。
//!
//! ## 为什么现在能测
//!
//! 此前不可测：`run_task` 的参数是 `tauri::AppHandle`（生产 runtime 为 `Wry`），
//! 而 `tauri::test::mock_app()` 返回 `MockRuntime`，两者类型不兼容；在集成测试里
//! 用 `tauri::Builder::build(generate_context!())` 构造真实 App 又会因缺少 build
//! script 生成的资源而在运行时报 `STATUS_ENTRYPOINT_NOT_FOUND`。
//!
//! 现在任务层的事件出口抽象为 [`EventSink`]（`AppHandle<R>` 是它的 blanket impl），
//! 测试注入记录型实现即可驱动完整流程，且不触碰任何 IPC。
//!
//! ## 运行
//!
//! 均需真实网络，默认 `#[ignore]`：
//! ```text
//! cargo test --test e2e_download -- --ignored --nocapture
//! ```
//! - `run_task_cancel_path`：秒级（只拉一次版本清单 + 本地扫描后即取消）
//! - `run_task_full_download`：分钟级（真实下载一整个版本，数百 MB）

use std::sync::atomic::Ordering;
use std::sync::Arc;

use parking_lot::Mutex;

use elytra_lib::config::downloader::DownloaderConfig;
use elytra_lib::game::version::manifest::fetch_version_list;
use elytra_lib::task::model::{DownloadTask, TaskStatus};
use elytra_lib::task::runner::run_task;
use elytra_lib::task::{self, EventSink};

/// 事件出口的空实现：只记录事件名，不触达任何 IPC。
#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<&'static str>>,
}

impl EventSink for RecordingSink {
    fn send(&self, event: &'static str, _payload: serde_json::Value) {
        self.events.lock().push(event);
    }
}

impl RecordingSink {
    fn saw(&self, event: &str) -> bool {
        self.events.lock().iter().any(|e| *e == event)
    }
}

/// 从官方清单取指定版本的 JSON URL（避免硬编码会随版本更新失效的 packages 路径）。
async fn version_url_for(version_id: &str) -> String {
    let cfg = DownloaderConfig::default();
    let manifest = fetch_version_list(&cfg).await.expect("拉取版本清单失败");
    manifest
        .versions
        .iter()
        .find(|v| v.id == version_id)
        .unwrap_or_else(|| panic!("清单中找不到版本 {version_id}"))
        .url
        .clone()
}

/// 构造原版（无 ModLoader / 无 OptiFine / 无 Fabric API）下载任务。
fn vanilla_task(version_id: &str, version_url: String) -> DownloadTask {
    DownloadTask {
        version_id: version_id.to_string(),
        version_url,
        mc_version: version_id.to_string(),
        loader: None,
        optifine_patch: None,
        version_dir_name: Some(version_id.to_string()),
        fabric_api: None,
    }
}

/// 取消路径端到端：阶段 A（真实拉取版本 JSON）与阶段 B（本地扫描）真实执行，
/// 阶段 C 前置位取消 → 断言走 `Cancelled` 收口、任务注册被移除、进度事件已发出。
///
/// runtime 必须 multi_thread：任务收口会经 `store::update_status` 走 `util::block_on`
/// （`block_in_place` 分支），current_thread 下会 panic —— 与生产（tauri runtime）一致。
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn run_task_cancel_path() {
    let rec = Arc::new(RecordingSink::default());
    let sink: Arc<dyn EventSink> = rec.clone();

    let task = vanilla_task("1.20.1", version_url_for("1.20.1").await);
    let (task_id, cancel, progress) = task::global().create("e2e-cancel", &["测试步骤"]);

    // 阶段 C 前置位取消：A/B 仍会真实执行（验证其编排），下载前即收口
    cancel.store(true, Ordering::SeqCst);

    run_task(sink, &cancel, &progress, &task).await;

    assert_eq!(
        progress.lock().status,
        TaskStatus::Cancelled,
        "取消令牌预置时应走 Cancelled 收口"
    );
    assert!(
        !task::global().exists(&task_id),
        "终态收口应移除任务注册"
    );
    assert!(
        rec.saw("task-progress"),
        "应至少发出一次进度事件（阶段 A/B 的 set_phase/step_done）"
    );
}

/// 成功路径端到端：真实下载一整个原版版本（数百 MB，分钟级）。
/// 断言终态为 `Done`、任务注册已移除，且期间发出过进度与（可能的）步骤事件。
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn run_task_full_download() {
    let rec = Arc::new(RecordingSink::default());
    let sink: Arc<dyn EventSink> = rec.clone();

    let version_id = "1.20.1";
    let task = vanilla_task(version_id, version_url_for(version_id).await);
    let (task_id, cancel, progress) = task::global().create("e2e-full", &["版本 JSON"]);

    run_task(sink, &cancel, &progress, &task).await;

    let final_status = progress.lock().status;
    assert_eq!(
        final_status,
        TaskStatus::Done,
        "完整下载应成功收口（实际 {final_status:?}，error={:?}）",
        progress.lock().error
    );
    assert!(
        !task::global().exists(&task_id),
        "终态收口应移除任务注册"
    );
    assert!(rec.saw("task-progress"), "应发出进度事件");
}
