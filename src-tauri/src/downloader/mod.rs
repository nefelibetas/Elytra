//! 下载传输层：把 URL 变成校验过的字节（资源特化场景见 `crate::game`）。

pub mod batch;
pub mod endpoints;
pub mod fetcher;
pub mod hash;
pub mod path_lock;
pub mod probe;
pub mod progress;
pub mod proxy;
pub mod reuse;
pub mod segmented;
