//! 同步上下文驱动异步任务（runtime 复用 / 兜底）。

use std::future::Future;
use std::sync::LazyLock;

use tokio::runtime::RuntimeFlavor;

/// 同步执行异步操作（低频阻塞可接受；secret-service 钥匙串 / task DB 复用）。
/// 有 tokio 上下文时复用现有 runtime：
/// - multi_thread runtime：worker 线程直接用 `Handle::block_on` 会 panic
///   （tokio 禁止 runtime 内再启动 runtime），须先 `block_in_place` 让出 worker；
///   外部线程（主线程/blocking 线程）下 `block_in_place` 是 no-op，行为不变。
/// - current_thread runtime：`block_in_place` 不可用，驱动线程直接 `block_on`。
///
/// 无 runtime 上下文（后台线程/独立进程/纯同步测试）时自维护兜底 runtime。
pub fn block_on<F: Future>(fut: F) -> F::Output {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        if handle.runtime_flavor() == RuntimeFlavor::MultiThread {
            tokio::task::block_in_place(move || handle.block_on(fut))
        } else {
            handle.block_on(fut)
        }
    } else {
        static RT: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .expect("tokio runtime")
        });
        RT.block_on(fut)
    }
}
