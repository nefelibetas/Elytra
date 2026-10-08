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

#[cfg(test)]
mod tests {
    use super::super::api::{fetch, fetch_verified};
    use super::super::core::test_support::{serve, temp_dest};
    use super::{redirect_allowed, reset_http_client, http};
    use crate::downloader::hash::{file_sha1_matches, sha1_hex};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// 重定向放行决策纯函数：黑洞子域拒绝、minio（bmclapi 下载存储）放行、
    /// 普通域放行。minio 放行是 OptiFine 安装的硬前提（下载 URL 302 → minio）。
    #[test]
    fn redirect_policy_minio_allowed() {
        assert!(!redirect_allowed("17.749333.xyz"), "实测黑洞子域必须拒绝");
        assert!(
            !redirect_allowed("cdn.749333.xyz"),
            "未知 749333 子域保持拒绝"
        );
        assert!(
            redirect_allowed("minio.749333.xyz"),
            "bmclapi 下载存储必须放行"
        );
        assert!(redirect_allowed("bmclapi2.bangbang93.com"));
        assert!(redirect_allowed("example.com"));
    }

    /// 真实 bmclapi OptiFine 下载（302 → minio.749333.xyz）：验证重定向放行后
    /// OptiFine 下载链路可用（黑名单按域名前缀一刀切时此测试必失败——
    /// minio 放行是 OptiFine 安装的硬前提）。网络测试，默认忽略。
    #[tokio::test]
    #[ignore]
    async fn bmclapi_optifine_real_download() {
        let dest = temp_dest("optifine_real");
        let res = fetch(
            "https://bmclapi2.bangbang93.com/optifine/1.20.1/HD_U/I6",
            &dest,
            None::<fn(u64, u64)>,
        )
        .await;
        assert!(res.is_ok(), "真实下载应成功: {:?}", res);
        let meta = std::fs::metadata(&dest).unwrap();
        assert!(
            meta.len() > 1_000_000,
            "OptiFine jar 应大于 1MB（实测 {} bytes）",
            meta.len()
        );
        let _ = std::fs::remove_file(&dest);
    }

    /// 302 到黑洞节点域（749333.xyz）应被重定向策略立即拒绝（快速失败，
    /// 而不是跟随到 TCP 黑洞挂满超时——国内网络实测卡在 asset index 的根因）。
    #[tokio::test]
    async fn redirect_to_blackhole_domain_rejected() {
        // 服务器 accept 3 次（fetch_with 有 3 轮重试），每轮都返回 302 → 黑洞域
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for _ in 0..3 {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let head = "HTTP/1.1 302 Found\r\nLocation: https://17.749333.xyz:443/download/x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = sock.write_all(head.as_bytes()).await;
            }
        });
        let url = format!("http://{addr}/f");
        let dest = temp_dest("redirect_blackhole");
        let t0 = std::time::Instant::now();
        let res = fetch_verified(&[url], &dest, None::<fn(u64, u64)>, "").await;
        let elapsed = t0.elapsed();
        eprintln!(
            "黑洞重定向总耗时: {:.2}s res={res:?}",
            elapsed.as_secs_f64()
        );
        assert!(res.is_err(), "黑洞重定向应失败，实际: {:?}", res);
        assert!(
            elapsed.as_secs() < 5,
            "应在超时前快速失败（{}s）",
            elapsed.as_secs_f64()
        );
        let _ = std::fs::remove_file(&dest);
    }

    /// 302 到正常域应正常跟随（重定向策略只拒绝黑洞域，不破坏 BMCLAPI 常规 302）。
    #[tokio::test]
    async fn redirect_to_normal_domain_followed() {
        let good = b"followed-content".to_vec();
        let final_url = serve(good.clone()).await;
        let target = final_url.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // 无限 accept：Auto 模式会先对域做 HEAD 探测（消耗一次连接），正式请求另有
        // 3 轮重试——只 accept 一次会被探测吃掉导致正式请求连接拒绝（确定性 flaky）。
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = tx.send(());
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let head = format!(
                    "HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                let _ = sock.write_all(head.as_bytes()).await;
            }
        });
        let _ = rx.await;
        let url = format!("http://{addr}/redirect");
        let dest = temp_dest("redirect_normal");
        let res = fetch_verified(&[url], &dest, None::<fn(u64, u64)>, &sha1_hex(&good)).await;
        assert!(res.is_ok(), "正常 302 应跟随成功: {:?}", res);
        assert!(
            file_sha1_matches(&dest, &sha1_hex(&good)),
            "落地内容应正确"
        );
        let _ = std::fs::remove_file(&dest);
    }

    /// network_mode 切换重建：reset 后下次 http() 构建新实例（旧 Arc 由持有者释放）。
    /// 构建 reqwest Client 不联网，测试在离线环境亦通过。
    #[test]
    fn http_client_reset_rebuilds_new_instance() {
        let a = http();
        reset_http_client();
        let b = http();
        assert!(
            !Arc::ptr_eq(&a, &b),
            "reset 后应重建新 client 实例（代理配置随 network_mode 切换生效）"
        );
        // 重建后全局实例稳定（再次调用复用同一实例）
        assert!(Arc::ptr_eq(&b, &http()));
    }
}
