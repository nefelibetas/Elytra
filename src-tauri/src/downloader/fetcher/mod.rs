//! HTTP 下载层：把 URL 变成校验过的字节。
//! 实现划分：
//! - [`api`]：公开入口与 `DownloadResult` / `FetchError`；
//! - [`core`]：多源下载核心（`FetchOptions` / `download_core` / `fetch_with_retries`）；
//! - [`routes`]：镜像路由表引用、健康度排序与候选 URL 生成；
//! - [`redirect`]：重定向黑洞黑名单、安全跟随与全局 client。

mod api;
mod core;
mod redirect;
mod routes;

pub use api::{
    fetch, fetch_bounded, fetch_bounded_c, fetch_fallback, fetch_fallback_c, fetch_verified,
    fetch_verified_c, fetch_with, fetch_with_c, DownloadResult, FetchError,
};
pub use core::set_speed_limit;
pub(crate) use core::{is_mirror_url, speed_limit};
pub use redirect::{http, reset_http_client};
