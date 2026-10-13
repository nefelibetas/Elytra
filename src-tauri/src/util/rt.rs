//! 同步上下文驱动异步任务（runtime 复用 / 兜底）。

use std::future::Future;
use std::sync::LazyLock;

use tokio::runtime::RuntimeFlavor;

/// 同步执行异步操作（低频阻塞可接受；secret-service 钥匙串 / task DB 复用）。有 tokio 上下文则复用现有 runtime：
/// multi_thread 下 worker 直接 `Handle::block_on` 会 panic（runtime 内不能再启动 runtime），须 `block_in_place`
/// 让出 worker（外部线程下 no-op）；current_thread 不可用则直接 `block_on`；无上下文时自维护兜底 runtime（2 worker）。
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
