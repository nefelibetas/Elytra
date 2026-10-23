//! 重定向跟随与全局共享 HTTP client 构建。

use std::sync::LazyLock;

use reqwest::Client;

use crate::constants::UA_DOWNLOAD;
use crate::downloader::proxy::apply_system_proxy;

/// 重定向跟随上限：B 回 2 跳（主控 → 节点 → MinIO 预签名）。
pub(super) fn mirror_safe_redirect() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::limited(5)
}

/// UA 非装饰：BMCLAPI 302 到的 CERNET/USTC 节点对无 UA 请求直接 403，缺它会令镜像候选换源全灭。
fn build_client() -> Client {
    let mut builder = Client::builder()
        .user_agent(UA_DOWNLOAD)
        // connect 黑洞防挂死：部分镜像 302 到不可达节点（749333.xyz 实测 TCP 黑洞），无连接超时会挂满 300s；
        // 2s 超时仅新建连接生效，不影响连接池复用
        .connect_timeout(std::time::Duration::from_secs(2))
        // 读黑洞防挂死：连接后 30s 无数据即掐断（此 client 服务清单/元数据等小请求，降级源黑洞会挂满 300s×源数）
        .read_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(300))
        // 限制重定向跳数，防止循环；不可达节点由 connect/read timeout 兜底
        .redirect(mirror_safe_redirect())
        .pool_max_idle_per_host(16);
    builder = apply_system_proxy(builder);
    builder.build().expect("reqwest::Client")
}

/// 全局 HTTP client（系统代理、超时与重定向跳数限制）；返回 Arc 供 network_mode 切换整体重建，进行中下载持旧 Arc 不受影响。
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
    use super::{http, reset_http_client};
    use crate::downloader::hash::{file_sha1_matches, sha1_hex};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// 重定向跟随：策略为 limited(5)，本地 302 应被跟随；不依赖外部 DNS。
    #[tokio::test]
    async fn redirect_policy_all_allowed() {
        let final_url = serve(b"redirected".to_vec()).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let target = final_url.clone();
        tokio::spawn(async move {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let head = format!(
                "HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            let _ = sock.write_all(head.as_bytes()).await;
        });
        let client = super::build_client();
        let resp = client
            .get(format!("http://{addr}/f"))
            .send()
            .await
            .expect("跟随重定向应到达");
        assert!(resp.status().is_success(), "应 200，实际 {}", resp.status());
        assert_eq!(resp.text().await.unwrap(), "redirected");
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
        assert!(file_sha1_matches(&dest, &sha1_hex(&good)), "落地内容应正确");
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
