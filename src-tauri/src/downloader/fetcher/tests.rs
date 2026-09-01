//! fetcher 单元/集成行为测试（随物理拆分集中迁移，断言一字未改）。
use super::core::{is_mirror_url, sort_routes_with_probes};
use super::redirect::redirect_allowed;
use super::routes::{
    mirror_candidates, record_route_failure, record_route_success, resolve_download_routes,
    route_health, DownloadRoute,
};
use super::*;
use crate::config::downloader::model::{DownloadSourceMode, ResourceClass};
use crate::downloader::hash::sha1_hex;
use crate::downloader::probe::ProbeResult;
use std::collections::HashMap;
use std::sync::Arc;

/// 就绪握手：accept 循环启动后通过 oneshot 通知调用方，杜绝 spawn 未调度时
/// 请求先行导致的连接竞态（此前偶发 "error sending request" 即此根因）。
async fn serve(body: Vec<u8>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = tx.send(());
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(x) => x,
                Err(_) => break,
            };
            let body = body.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(&body).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    let _ = rx.await; // 就绪：accept 循环已启动
    format!("http://{addr}/file.jar")
}

fn temp_dest(name: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("{}_{}.jar", name, std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

/// 取消 flag 预置时下载应立即返回 Cancelled 且不留文件
/// （此前下载循环不检查取消，任务取消要等 read_timeout/换源才生效）。
#[tokio::test]
async fn fetch_cancelled_returns_immediately() {
    let body = vec![b'x'; 256 * 1024];
    let url = serve(body).await;
    let dest = temp_dest("fetch_cancelled");
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(true)); // 预置已取消

    let t0 = std::time::Instant::now();
    let res = fetch_verified_c(&[url], &dest, None::<fn(u64, u64)>, "", Some(&cancel)).await;
    let elapsed = t0.elapsed();

    assert!(
        matches!(res, Err(FetchError::Cancelled)),
        "取消应返回 Cancelled，实际: {:?}",
        res
    );
    assert!(
        elapsed.as_secs() < 5,
        "取消应立即返回（{}s）",
        elapsed.as_secs_f64()
    );
    assert!(!dest.exists(), "取消后不应残留文件");
    let _ = std::fs::remove_file(&dest);
}

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
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
        crate::downloader::hash::file_sha1_matches(&dest, &sha1_hex(&good)),
        "落地内容应正确"
    );
    let _ = std::fs::remove_file(&dest);
}

#[tokio::test]
async fn fetch_verified_falls_back_to_second_source_on_checksum_mismatch() {
    let good = b"correct-content".to_vec();
    let wrong = serve(b"stale-mirror-content".to_vec()).await;
    let right = serve(good.clone()).await;
    // 防极端端口碰撞（内核 :0 分配理论极小概率相同）：相同则重建好源
    let right = if right == wrong {
        serve(good.clone()).await
    } else {
        right
    };
    let urls = vec![wrong, right];
    let dest = temp_dest("fetch_verified_recover");

    let res = fetch_verified(&urls, &dest, None::<fn(u64, u64)>, &sha1_hex(&good)).await;

    assert!(res.is_ok(), "应回退到第二源成功，实际: {:?}", res);
    assert!(
        crate::downloader::hash::file_sha1_matches(&dest, &sha1_hex(&good)),
        "落地文件内容应为正确字节"
    );
    let _ = std::fs::remove_file(&dest);
}

/// 全部源均返回坏内容：报 Checksum 且不留坏文件（不误报成功）。
#[tokio::test]
async fn fetch_verified_fails_when_all_sources_return_wrong_content() {
    let bad1 = serve(b"bad-bytes-1".to_vec()).await;
    let bad2 = serve(b"bad-bytes-2".to_vec()).await;
    let urls = vec![bad1, bad2];
    let dest = temp_dest("fetch_verified_allbad");

    let res = fetch_verified(
        &urls,
        &dest,
        None::<fn(u64, u64)>,
        &sha1_hex(b"expected-content"),
    )
    .await;

    assert!(
        matches!(res, Err(FetchError::Checksum)),
        "应报 Checksum 错误，实际: {:?}",
        res
    );
    assert!(!dest.exists(), "校验失败后坏文件应被删除");
}

/// 期望 sha1 为空串时跳过校验（file_sha1_matches 语义），坏内容也视为成功。
#[tokio::test]
async fn fetch_verified_skips_verification_when_sha1_empty() {
    let srv = serve(b"whatever-bytes".to_vec()).await;
    let urls = vec![srv];
    let dest = temp_dest("fetch_verified_emptysha1");

    let res = fetch_verified(&urls, &dest, None::<fn(u64, u64)>, "").await;

    assert!(res.is_ok(), "空 sha1 应跳过校验: {:?}", res);
    let _ = std::fs::remove_file(&dest);
}

/// 混合失败（一源网络错误 + 一源内容不匹配）应报 Network 且信息包含每个源各自原因，
/// 不被最后一个源的错误覆盖。
#[tokio::test]
async fn fetch_from_sources_reports_every_source_error() {
    // 未监听端口 → 连接失败（网络错误）；坏内容源 → Checksum
    let dead = "http://127.0.0.1:1/file.jar".to_string();
    let bad = serve(b"stale-content".to_vec()).await;
    let urls = vec![dead, bad];
    let dest = temp_dest("fetch_from_sources_mixed");

    let res = fetch_verified(
        &urls,
        &dest,
        None::<fn(u64, u64)>,
        &sha1_hex(b"expected-content"),
    )
    .await;

    let msg = match res {
        Err(FetchError::Network(m)) => m,
        other => panic!("应报 Network 错误，实际: {:?}", other),
    };
    assert!(
        msg.contains("127.0.0.1"),
        "信息应包含第一个源的失败原因，实际: {msg}"
    );
    assert!(
        msg.contains("sha1 校验失败") || msg.contains("Checksum") || msg.contains("校验"),
        "信息应包含内容校验失败的原因，实际: {msg}"
    );
    assert!(!dest.exists(), "失败后不应留文件");
}

/// 镜像源判定：仅 BMCLAPI/MCIM 域名受限，官方源不受限。
#[test]
fn is_mirror_url_detects_mirror_domains() {
    assert!(is_mirror_url(
        "https://bmclapi2.bangbang93.com/libraries/x.jar"
    ));
    assert!(is_mirror_url("https://mod.mcimirror.top/data/x.jar"));
    assert!(!is_mirror_url("https://libraries.minecraft.net/x.jar"));
    assert!(!is_mirror_url("https://piston-data.mojang.com/x.jar"));
    assert!(!is_mirror_url("https://resources.download.minecraft.net/x"));
    // 高并发不稳的官方 maven 同样限流
    assert!(is_mirror_url(
        "https://maven.quiltmc.org/repository/release/org/quiltmc/x.jar"
    ));
    assert!(is_mirror_url(
        "https://maven.neoforged.net/releases/net/neoforged/x.jar"
    ));
    assert!(is_mirror_url(
        "https://maven.minecraftforge.net/net/minecraftforge/x.jar"
    ));
}

/// 镜像候选生成：mojang/library/loader 域 → BMCLAPI 镜像（单一表在 downloader/model.rs）。
#[test]
fn mirror_candidates_generate_bmclapi_urls() {
    assert_eq!(
        mirror_candidates("https://libraries.minecraft.net/com/x/y.jar"),
        vec!["https://bmclapi2.bangbang93.com/libraries/com/x/y.jar"]
    );
    assert_eq!(
        mirror_candidates("https://resources.download.minecraft.net/ab/hash"),
        vec!["https://bmclapi2.bangbang93.com/assets/ab/hash"]
    );
    assert_eq!(
        mirror_candidates("https://maven.neoforged.net/releases/net/neoforged/x/1.0/x-1.0.jar"),
        vec!["https://bmclapi2.bangbang93.com/maven/releases/net/neoforged/x/1.0/x-1.0.jar"]
    );
    assert_eq!(
        mirror_candidates("https://meta.fabricmc.net/v2/versions/loader/1.20.4"),
        vec!["https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/1.20.4"]
    );
    // 未知域不生成
    assert!(mirror_candidates("https://example.com/x.jar").is_empty());
}

/// 路由解析：去重保序 + 未含镜像时自动追加候选。
#[test]
fn resolve_routes_dedupes_and_appends_mirror_candidate() {
    let urls = vec![
        "https://libraries.minecraft.net/com/x/y.jar".to_string(),
        "https://libraries.minecraft.net/com/x/y.jar".to_string(), // 重复
    ];
    let routes = resolve_download_routes(&urls, DownloadSourceMode::Auto);
    assert_eq!(routes.len(), 2, "去重后原始 1 + 镜像候选 1");
    assert_eq!(routes[0].url, "https://libraries.minecraft.net/com/x/y.jar");
    assert_eq!(
        routes[1].url,
        "https://bmclapi2.bangbang93.com/libraries/com/x/y.jar"
    );
    assert!(!routes[0].is_mirror);
    assert!(routes[1].is_mirror);
    // 调用方已含镜像（手工双源）→ 不再重复追加
    let urls2 = vec![
        "https://maven.fabricmc.net/a.jar".to_string(),
        "https://bmclapi2.bangbang93.com/maven/a.jar".to_string(),
    ];
    let routes2 = resolve_download_routes(&urls2, DownloadSourceMode::Auto);
    assert_eq!(routes2.len(), 2);
}

/// 单选模式（OfficialOnly/ChinaOnly）保持调用方顺序（无健康度竞争）。
#[test]
fn resolve_routes_single_mode_keeps_order() {
    // OfficialOnly：调用方传官方 URL → 保持（自动追加镜像候选，顺序官方在前）
    let urls = vec!["https://libraries.minecraft.net/com/x/y.jar".to_string()];
    let routes = resolve_download_routes(&urls, DownloadSourceMode::OfficialOnly);
    assert!(
        !routes[0].is_mirror,
        "OfficialOnly 官方先行，实际: {}",
        routes[0].url
    );
    // ChinaOnly：调用方只传镜像 URL → 保持单一镜像
    let mirror = vec!["https://bmclapi2.bangbang93.com/libraries/com/x/y.jar".to_string()];
    let routes2 = resolve_download_routes(&mirror, DownloadSourceMode::ChinaOnly);
    assert_eq!(routes2.len(), 1, "ChinaOnly 单源不追加候选");
    assert!(
        routes2[0].is_mirror,
        "ChinaOnly 保持镜像，实际: {}",
        routes2[0].url
    );
}

/// 健康度：连续失败 >=3 进入冷却（Auto 排序中冷却路由排最末）。
#[test]
fn route_health_cooldown_after_consecutive_failures() {
    let url = "https://example.com/bad-route.jar";
    let mut h = route_health(url);
    assert_eq!(h.failures, 0);
    assert!(!h.in_cooldown());
    record_route_failure(url);
    record_route_failure(url);
    record_route_failure(url);
    h = route_health(url);
    assert_eq!(h.failures, 3);
    assert!(h.in_cooldown(), "连续 3 次失败应进入 30s 冷却");
    // 成功重置失败数并清除冷却
    record_route_success(url, 120.0);
    h = route_health(url);
    assert_eq!(h.failures, 0);
    assert!(!h.in_cooldown());
    assert_eq!(h.samples, 1);
    // TTFB EWMA 更新（α=0.25）
    record_route_success(url, 200.0);
    h = route_health(url);
    assert!(
        (h.ttfb_ewma_ms - 140.0).abs() < 0.01,
        "EWMA 应收敛到 140，实际 {}",
        h.ttfb_ewma_ms
    );
}

/// Auto 模式：冷却中的路由排最末；Loader 镜像冷启动优先。
#[test]
fn resolve_routes_auto_sorts_by_health() {
    let official = "https://maven.neoforged.net/releases/x.jar".to_string();
    let mirror = "https://bmclapi2.bangbang93.com/maven/releases/x.jar".to_string();
    let urls = vec![official.clone(), mirror.clone()];
    // 冷启动（无样本）：Loader 镜像优先
    let routes = resolve_download_routes(&urls, DownloadSourceMode::Auto);
    assert!(
        routes[0].is_mirror,
        "Auto + Loader 冷启动镜像优先: {}",
        routes[0].url
    );
    // 镜像连续失败进入冷却 → 官方排前
    record_route_failure(&mirror);
    record_route_failure(&mirror);
    record_route_failure(&mirror);
    let routes2 = resolve_download_routes(&urls, DownloadSourceMode::Auto);
    assert!(
        !routes2[0].is_mirror,
        "冷却中的镜像应排最末，实际: {}",
        routes2[0].url
    );
}

/// 镜像源（URL 含 bmclapi）请求并发必须被限制在 MIRROR_CONCURRENCY 以内：
/// 20 个并发任务同时下载镜像文件，服务器观测到的同时在处理的连接数 ≤ 8。
#[tokio::test]
async fn mirror_requests_capped_at_mirror_concurrency() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (a2, m2) = (Arc::clone(&active), Arc::clone(&max_active));
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(x) => x,
                Err(_) => break,
            };
            let (a3, m3) = (Arc::clone(&a2), Arc::clone(&m2));
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let cur = a3.fetch_add(1, Ordering::SeqCst) + 1;
                m3.fetch_max(cur, Ordering::SeqCst);
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                // 模拟下载耗时，给并发统计留出窗口
                tokio::time::sleep(Duration::from_millis(80)).await;
                let body = b"mirror-content";
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(body).await;
                let _ = sock.shutdown().await;
                a3.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });

    // URL 以 userinfo 携带 bmclapi 标记（is_mirror_url 判定为镜像），
    // host 仍是 127.0.0.1 保证可解析且直连本地服务器
    let url = format!("http://bmclapi@127.0.0.1:{}/file.jar", addr.port());
    let mut handles = Vec::new();
    for i in 0..20 {
        let u = url.clone();
        let d = temp_dest(&format!("mirror_cap_{i}"));
        handles.push(tokio::spawn(async move {
            let _ = fetch_verified(std::slice::from_ref(&u), &d, None::<fn(u64, u64)>, "").await;
            let _ = std::fs::remove_file(&d);
        }));
    }
    for h in handles {
        let _ = h.await;
    }

    let max = max_active.load(Ordering::SeqCst);
    assert!(
        max <= crate::constants::MIRROR_CONCURRENCY,
        "镜像并发应 ≤ {}，实际峰值 {}",
        crate::constants::MIRROR_CONCURRENCY,
        max
    );
    assert!(max > 1, "应存在真实并发（>1），实际 {}", max);
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
/// 冷启动预探测排序纯逻辑：Loader 镜像永远先行（官方 maven 限流语义）；
/// 非 Loader 探测快者先行；探测失败/无样本排后（不剔除）。
#[test]
fn sort_routes_with_probes_orders_fastest_first() {
    let mk = |url: &str| DownloadRoute {
        url: url.into(),
        is_mirror: url.contains("bmclapi"),
    };
    let ok = |ms: u64| ProbeResult {
        ok: true,
        ttfb_ms: ms,
    };
    let fail = ProbeResult {
        ok: false,
        ttfb_ms: 999,
    };

    // 非 Loader：官方快 → 官方前
    let mut r = vec![
        mk("https://libraries.minecraft.net/a.jar"),
        mk("https://bmclapi2.bangbang93.com/libraries/a.jar"),
    ];
    sort_routes_with_probes(
        &mut r,
        &HashMap::from([
            ("https://libraries.minecraft.net/a.jar".to_string(), ok(50)),
            (
                "https://bmclapi2.bangbang93.com/libraries/a.jar".to_string(),
                ok(400),
            ),
        ]),
        ResourceClass::Library,
    );
    assert!(
        r[0].url.contains("minecraft.net"),
        "官方快应先行: {}",
        r[0].url
    );

    // 非 Loader：官方慢/失败 → 镜像前
    let mut r = vec![
        mk("https://libraries.minecraft.net/a.jar"),
        mk("https://bmclapi2.bangbang93.com/libraries/a.jar"),
    ];
    sort_routes_with_probes(
        &mut r,
        &HashMap::from([
            ("https://libraries.minecraft.net/a.jar".to_string(), fail),
            (
                "https://bmclapi2.bangbang93.com/libraries/a.jar".to_string(),
                ok(80),
            ),
        ]),
        ResourceClass::Library,
    );
    assert!(
        r[0].url.contains("bmclapi"),
        "官方探测失败应镜像先行: {}",
        r[0].url
    );

    // Loader 类：镜像永远先行（即使官方探测更快）——官方 maven 限流语义
    let mut r = vec![
        mk("https://maven.minecraftforge.net/x.jar"),
        mk("https://bmclapi2.bangbang93.com/maven/x.jar"),
    ];
    sort_routes_with_probes(
        &mut r,
        &HashMap::from([
            ("https://maven.minecraftforge.net/x.jar".to_string(), ok(10)),
            (
                "https://bmclapi2.bangbang93.com/maven/x.jar".to_string(),
                ok(500),
            ),
        ]),
        ResourceClass::Loader,
    );
    assert!(
        r[0].url.contains("bmclapi"),
        "Loader 镜像先行语义不可被探测破坏: {}",
        r[0].url
    );

    // 无探测样本视同失败排后（不剔除候选）
    let mut r = vec![
        mk("https://libraries.minecraft.net/b.jar"),
        mk("https://bmclapi2.bangbang93.com/libraries/b.jar"),
    ];
    sort_routes_with_probes(
        &mut r,
        &HashMap::from([(
            "https://bmclapi2.bangbang93.com/libraries/b.jar".to_string(),
            ok(60),
        )]),
        ResourceClass::Library,
    );
    assert!(
        r[0].url.contains("bmclapi"),
        "无样本的官方应排后（镜像探测成功在前）: {}",
        r[0].url
    );
}
