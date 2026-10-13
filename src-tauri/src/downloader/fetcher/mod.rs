//! HTTP 下载层：把 URL 变成校验过的字节。
//! - [`api`] 公开入口/`DownloadResult`/`FetchError`；[`core`] 多源核心；[`routes`] 镜像路由；[`redirect`] 重定向/client。

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
