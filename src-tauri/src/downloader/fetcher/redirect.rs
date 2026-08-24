//! 重定向黑洞黑名单与安全跟随，以及全局共享 HTTP client 构建。

use std::sync::LazyLock;

use reqwest::Client;

use crate::constants::UA_DOWNLOAD;
use crate::downloader::proxy::apply_system_proxy;

/// 重定向目标是否放行：跟随重定向，但拒绝实测黑洞节点域（17.749333.xyz——
/// BMCLAPI 对象存储节点，国内部分网络 TCP 黑洞，连上不传数据）。
/// 放行 minio.749333.xyz（bmclapi 文件下载主存储：OptiFine 下载 URL 实测
/// 302 → minio，按域名前缀一刀切会把真实下载误杀，安装必失败；
/// 无代理黑洞网络下 minio 不可达由全局 client 的 30s read_timeout 兜底，不永久挂死）。
pub(super) fn redirect_allowed(host: &str) -> bool {
    !host.contains("749333") || host.starts_with("minio.")
}

pub(super) fn mirror_safe_redirect() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let host = attempt.url().host_str().unwrap_or("").to_string();
        if !redirect_allowed(&host) {
            eprintln!("[fetch] 拒绝重定向到黑洞节点: {host}");
            attempt.error("重定向到不可达镜像节点（749333.xyz）")
        } else {
            attempt.follow()
        }
    })
}

fn build_client() -> Client {
    let mut builder = Client::builder()
        .user_agent(UA_DOWNLOAD)
        // connect 黑洞防挂死：部分镜像 302 到不可达对象存储节点（749333.xyz 实测 TCP 黑洞），
        // 全局 client 无连接超时会挂满 300s；2s 连接超时只在新建连接时生效，不影响连接池复用
        .connect_timeout(std::time::Duration::from_secs(2))
        // 读黑洞防挂死：连接建立后 30s 无数据（镜像假死）即掐断。此 client 服务
        // 版本清单/资源索引/元数据等小请求（fetch_with_fallback 降级源只靠总超时，
        // 无 read_timeout 时降级源黑洞会挂满 300s×源数，前端表现为进度永远 0%）
        .read_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(300))
        // 302 到黑洞节点时直接拒绝（reqwest 跟随重定向时无 per-hop 超时控制，
        // 黑名单是唯一在超时前拦住的方式）
        .redirect(mirror_safe_redirect())
        .pool_max_idle_per_host(16);
    builder = apply_system_proxy(builder);
    builder.build().expect("reqwest::Client")
}

/// 全局 HTTP client（带系统代理/超时/黑洞重定向防护；首次调用按当前
/// network_mode 构建代理配置）。返回 Arc 以便 network_mode 切换时整体重建：
/// `reset_http_client` 置空后下次调用按新配置重建；进行中下载持旧 Arc 引用
/// 不受影响。探测（probe.rs）与下载全链路共用此 client，代理行为天然一致。
pub fn http() -> std::sync::Arc<Client> {
    let mut guard = HTTP_CLIENT.lock();
    if guard.is_none() {
        *guard = Some(std::sync::Arc::new(build_client()));
    }
    guard.clone().expect("http client")
}

/// network_mode（Auto/System/Direct）切换后调用：置空全局 client，
/// 下次 `http()` 按新配置重建。旧 client 由持有者引用计数自然释放。
pub fn reset_http_client() {
    HTTP_CLIENT.lock().take();
}

static HTTP_CLIENT: LazyLock<parking_lot::Mutex<Option<std::sync::Arc<Client>>>> =
    LazyLock::new(|| parking_lot::Mutex::new(None));
