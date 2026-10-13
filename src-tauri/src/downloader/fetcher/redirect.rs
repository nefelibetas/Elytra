//! 重定向黑洞黑名单与安全跟随，以及全局共享 HTTP client 构建。

use std::sync::LazyLock;

use reqwest::Client;

use crate::constants::UA_DOWNLOAD;
use crate::downloader::proxy::apply_system_proxy;

/// 重定向放行判定：拒绝黑洞域（17.749333.xyz，国内部分网络 TCP 黑洞），放行 minio.749333.xyz（bmclapi 下载主存储，OptiFine 302 目标；不可达由 client 30s read_timeout 兜底）。
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
        // connect 黑洞防挂死：部分镜像 302 到不可达节点（749333.xyz 实测 TCP 黑洞），无连接超时会挂满 300s；
        // 2s 超时仅新建连接生效，不影响连接池复用
        .connect_timeout(std::time::Duration::from_secs(2))
        // 读黑洞防挂死：连接后 30s 无数据即掐断（此 client 服务清单/元数据等小请求，降级源黑洞会挂满 300s×源数）
        .read_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(300))
        // 302 到黑洞节点直接拒绝（重定向无 per-hop 超时控制，黑名单是唯一在超时前拦住的）
        .redirect(mirror_safe_redirect())
        .pool_max_idle_per_host(16);
    builder = apply_system_proxy(builder);
    builder.build().expect("reqwest::Client")
}

/// 单源请求 client（`MultiSource` 每源独立新建）：与全局 client 同一套 UA/重定向策略/系统代理，仅总时限由调用方给定；UA 非装饰：BMCLAPI 302 到的 CERNET/USTC 节点对无 UA 请求直接 403，缺它会令镜像候选换源全灭。
pub(super) fn route_client(total_timeout: Option<std::time::Duration>) -> reqwest::Result<Client> {
    let mut builder = Client::builder()
        .user_agent(UA_DOWNLOAD)
        .connect_timeout(std::time::Duration::from_secs(2))
        .read_timeout(std::time::Duration::from_secs(30))
        .redirect(mirror_safe_redirect());
    if let Some(t) = total_timeout {
        builder = builder.timeout(t);
    }
    apply_system_proxy(builder).build()
}

/// 全局 HTTP client（系统代理/超时/黑洞重定向防护）；返回 Arc 供 network_mode 切换整体重建，进行中下载持旧 Arc 不受影响。
pub fn http() -> std::sync::Arc<Client> {
    let mut guard = HTTP_CLIENT.lock();
    if guard.is_none() {
        *guard = Some(std::sync::Arc::new(build_client()));
    }
    guard.clone().expect("http client")
}

/// network_mode 切换后调用：置空全局 client，下次 `http()` 重建（旧 client 由引用计数释放）。
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

    /// 重定向放行：黑洞子域拒绝，minio（bmclapi 下载存储）与普通域放行；minio 放行是 OptiFine 安装的硬前提。
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

    /// 302 到黑洞域（749333.xyz）应被立即拒绝（快速失败，而非跟随到 TCP 黑洞挂满超时）。
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

    #[tokio::test]
    async fn redirect_to_normal_domain_followed() {
        let good = b"followed-content".to_vec();
        let final_url = serve(good.clone()).await;
        let target = final_url.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // 无限 accept：Auto 模式会先 HEAD 探测（消耗连接）+ 3 轮重试，只 accept 一次会 flaky
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

    #[test]
    fn http_client_reset_rebuilds_new_instance() {
        let a = http();
        reset_http_client();
        let b = http();
        assert!(
            !Arc::ptr_eq(&a, &b),
            "reset 后应重建新 client 实例（代理配置随 network_mode 切换生效）"
        );
        assert!(Arc::ptr_eq(&b, &http()));
    }
}
