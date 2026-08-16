// ── ModLoader 引擎：公共纯函数 + 子模块 re-export ──
// fabric.rs（profile 式：Fabric/Quilt）、forge.rs（installer 式 Forge 共用流程 + 共享工具）、
// neoforge.rs（installer 式 NeoForge 专属）、optifine.rs、version_query.rs（版本列表查询）。

pub mod fabric;
pub mod forge;
pub mod neoforge;
pub mod optifine;
pub mod version_query;

pub use fabric::{install_fabric, install_quilt};
pub use forge::install_forge;
pub use neoforge::install_neoforge;
pub use optifine::{install_optifine, OptifineMode};

fn maven_path(name: &str) -> String {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return format!("{}.jar", name.replace(':', "_"));
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let group_path = group.replace('.', "/");
    format!("{group_path}/{artifact}/{version}/{artifact}-{version}.jar")
}

/// 合并 loader profile 到原版 JSON（Fabric/Quilt 官方 profile 接口）：
/// 追加 jvm/game 参数、替换 mainClass、追加 libraries（maven 库转 downloads.artifact）、
/// 删 inheritsFrom、保留原版 downloads（client.jar 下载信息不丢）。
pub(crate) fn merge_loader_profile(
    parent_text: &str,
    profile: &serde_json::Value,
    dir_name: &str,
    maven_url: &str,
) -> Result<serde_json::Value, String> {
    let mut merged: serde_json::Value =
        serde_json::from_str(parent_text).map_err(|e| format!("解析原版版本 JSON 失败\n{}", e))?;

    for key in ["jvm", "game"] {
        let loader_args = profile["arguments"][key].as_array();
        if let Some(loader_args) = loader_args {
            if !loader_args.is_empty() {
                if let Some(args) = merged["arguments"][key].as_array_mut() {
                    args.extend(loader_args.iter().cloned());
                } else {
                    merged["arguments"][key] = serde_json::Value::Array(loader_args.clone());
                }
            }
        }
    }

    merged["id"] = serde_json::Value::String(dir_name.to_string());
    if let Some(mc) = profile["mainClass"].as_str() {
        merged["mainClass"] = serde_json::Value::String(mc.to_string());
    }

    if let Some(loader_libs) = profile["libraries"].as_array() {
        if let Some(libs) = merged["libraries"].as_array_mut() {
            for lib in loader_libs {
                let name = match lib["name"].as_str() {
                    Some(n) if !n.is_empty() => n,
                    _ => continue,
                };
                let mp = maven_path(name);
                let base_url = lib["url"].as_str().unwrap_or(maven_url);
                let full_url = format!("{}/{}", base_url.trim_end_matches('/'), mp);
                let sha1 = lib["sha1"].as_str().unwrap_or("");
                let size = lib["size"].as_u64().unwrap_or(0);

                let mut new_lib = lib.clone();
                if let Some(obj) = new_lib.as_object_mut() {
                    obj.remove("md5");
                    obj.remove("sha256");
                    obj.remove("sha512");
                    obj.remove("url");
                    obj.remove("mainJar");
                }
                new_lib["downloads"] = serde_json::json!({
                    "artifact": {
                        "path": mp,
                        "url": full_url,
                        "sha1": sha1,
                        "size": size,
                    }
                });
                libs.push(new_lib);
            }
        }
    }

    if let Some(obj) = merged.as_object_mut() {
        obj.remove("inheritsFrom");
    }
    Ok(merged)
}

/// 展开 inheritsFrom 继承（Forge/NeoForge 的 version.json 只有自身库 + 父指针）：
/// 以父 JSON 为基 → 子库拼到父库**之前** → 追加子参数 → 子关键字段覆盖（缺则保留父）
/// → 删 inheritsFrom。
/// 子 JSON 无 inheritsFrom 时视为完整独立 JSON，直接返回（仅改 id）。
pub(crate) fn expand_inheritance(
    parent_text: &str,
    mut child: serde_json::Value,
    dir_name: &str,
) -> Result<serde_json::Value, String> {
    child["id"] = serde_json::Value::String(dir_name.to_string());
    if child.get("inheritsFrom").is_none() {
        return Ok(child);
    }

    let mut merged: serde_json::Value =
        serde_json::from_str(parent_text).map_err(|e| format!("解析原版版本 JSON 失败\n{}", e))?;

    // libraries：子 + 父 拼接（子在前——继承式版本的通用惯例，官方启动器/HMCL 均子库
    // 前置；OptiFine 纯净版依赖此顺序：预补丁客户端 jar 必须先于 vanilla 库出现在
    // classpath，JVM 加载同名类时先命中 OptiFine 版本实现类覆盖。父在前会静默加载
    // 原版类，OptiFine 不生效）
    let child_libs = child["libraries"].take();
    if let Some(libs) = child_libs.as_array() {
        if let Some(parent_libs) = merged["libraries"].as_array_mut() {
            let mut combined = libs.clone();
            combined.append(parent_libs);
            merged["libraries"] = serde_json::Value::Array(combined);
        } else {
            merged["libraries"] = child_libs.clone();
        }
    }

    // arguments：父 + 子 拼接（jvm/game）
    for key in ["jvm", "game"] {
        let child_args = child["arguments"][key].take();
        if let Some(args) = child_args.as_array() {
            if let Some(parent_args) = merged["arguments"][key].as_array_mut() {
                parent_args.extend(args.iter().cloned());
            } else {
                merged["arguments"][key] = child_args.clone();
            }
        }
    }

    // 子覆盖（子无该字段则保留父的）：mainClass/minecraftArguments/assetIndex/downloads 等
    for key in [
        "mainClass",
        "minecraftArguments",
        "assets",
        "assetIndex",
        "downloads",
        "type",
        "javaVersion",
        "complianceLevel",
        "minimumLauncherVersion",
    ] {
        if let Some(v) = child.get(key) {
            merged[key] = v.clone();
        }
    }

    merged["id"] = serde_json::Value::String(dir_name.to_string());
    if let Some(obj) = merged.as_object_mut() {
        obj.remove("inheritsFrom");
    }
    Ok(merged)
}

/// 声明式处理器输出校验（Forge/NeoForge 安装幂等）：
/// 运行 Java 补丁工具**之前**判定"输出是否已是最新"，全部命中则跳过 JVM 调用——
/// 重装/修复场景不再白白跑 binarypatcher/installertools（数秒～数十秒 + 杀软误报风险）。
/// - 期望 sha1 已知（Forge：version.json 的 `forge:{ver}:client` 库 sha1）→ 存在且 sha1 匹配；
/// - 期望 sha1 未知（NeoForge patched jar 无公开 hash）→ 存在且体积 ≥1MB 粗校验（对齐旧行为，
///   部分写入的残留不会误判）。
pub(crate) fn processor_outputs_are_current(
    outputs: &[(&std::path::Path, Option<&str>)],
) -> bool {
    outputs
        .iter()
        .all(|(p, sha1)| match sha1 {
            Some(h) => crate::downloader::hash::file_sha1_matches(p, h),
            None => p.metadata().map(|m| m.len() >= 1_000_000).unwrap_or(false),
        })
}

/// 运行安装器 Java 工具并收集输出（stdout/stderr 完整返回），轮询取消标志：
/// cancel 置位时 kill 子进程——用户取消任务后 binarypatcher/installertools 不再白跑。
/// 使用 tokio::select! 同时等待进程退出与排空输出 pipe（避免大输出撑满 pipe 阻塞子进程）。
pub(crate) async fn run_installer_command(
    mut child: tokio::process::Child,
    cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    label: &str,
) -> Result<std::process::Output, String> {
    use std::sync::atomic::Ordering;
    use tokio::io::AsyncReadExt;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{label} stdout 不可用"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("{label} stderr 不可用"))?;
    let mut out_buf: Vec<u8> = Vec::new();
    let mut err_buf: Vec<u8> = Vec::new();

    let status = loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill().await;
        }
        tokio::select! {
            st = child.wait() => break st.map_err(|e| format!("运行{label}失败\n{e}"))?,
            n = stdout.read_buf(&mut out_buf) => { let _ = n; }
            n = stderr.read_buf(&mut err_buf) => { let _ = n; }
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {}
        }
    };
    // 进程退出后 flush pipe 剩余数据
    let _ = stdout.read_to_end(&mut out_buf).await;
    let _ = stderr.read_to_end(&mut err_buf).await;
    Ok(std::process::Output {
        status,
        stdout: out_buf,
        stderr: err_buf,
    })
}

/// ModLoader 安装的阶段事件（runner 钩子：推进步骤状态/置完成标志）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderEvent {
    /// 合并 JSON 已写入
    ConfigDone,
    /// 本体（fabric libs / installer→jar）已完成
    LibsDone,
}

/// 阶段钩子：安装过程中在对应事件点同步调用（非 async，直接闭包）。
/// Send + Sync：钩子闭包会穿过 async task 边界（跨线程）。
pub type LoaderHook<'a> = &'a (dyn Fn(LoaderEvent) + Send + Sync);

#[cfg(test)]
mod tests {
    use super::*;

    const PARENT: &str = r#"{
        "id": "26.2",
        "mainClass": "net.minecraft.client.main.Main",
        "arguments": { "jvm": ["-Xmx2G"] },
        "libraries": [],
        "downloads": {
            "client": { "url": "https://example.com/client.jar", "sha1": "abc", "size": 1 }
        },
        "type": "release"
    }"#;

    #[test]
    fn processor_outputs_are_current_sha1_or_size() {
        let dir = std::env::temp_dir().join("elytra_test_processor_outputs");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("out.jar");
        std::fs::write(&p, b"data").unwrap();
        // sha1 已知：匹配 → 最新（跳过运行）；不匹配 → 需重跑
        let sha = crate::downloader::hash::sha1_hex(b"data");
        assert!(processor_outputs_are_current(&[(&p, Some(&sha))]));
        assert!(
            !processor_outputs_are_current(&[(&p, Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"))]),
            "sha1 不匹配必须判定需重跑"
        );
        // 无 sha1 来源：体积阈值粗校验（≥1MB 视为就绪，对齐 NeoForge 旧行为）
        assert!(
            !processor_outputs_are_current(&[(&p, None)]),
            "不足 1MB 不应判定就绪"
        );
        std::fs::write(&p, vec![0u8; 1_100_000]).unwrap();
        assert!(processor_outputs_are_current(&[(&p, None)]));
        // 文件缺失 → 需重跑（两种校验都失败）
        let missing = dir.join("missing.jar");
        assert!(!processor_outputs_are_current(&[(&missing, Some(&sha))]));
        assert!(!processor_outputs_are_current(&[(&missing, None)]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn maven_path_full_coordinate() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.15.11"),
            "net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar"
        );
    }

    #[test]
    fn maven_path_short_coordinate() {
        assert_eq!(maven_path("a:b"), "a_b.jar");
    }

    #[test]
    fn merge_loader_profile_combines_all_fields() {
        let profile = serde_json::json!({
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": { "jvm": ["-Dfabric.side=client"] },
            "libraries": [
                {
                    "name": "net.fabricmc:fabric-loader:0.15.11",
                    "url": "https://maven.fabricmc.net/",
                    "sha1": "d34d",
                    "size": 5000
                },
                {
                    "name": "net.fabricmc:tiny-mappings-parser:0.3.0",
                    "sha1": "", "size": 0
                }
            ]
        });
        let merged = merge_loader_profile(
            PARENT,
            &profile,
            "26.2-fabric-0.15.11",
            "https://libraries.minecraft.net",
        )
        .expect("merge 应成功");

        assert_eq!(merged["id"], "26.2-fabric-0.15.11");
        assert_eq!(
            merged["mainClass"],
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        // jvm 参数追加：原版 + loader 的
        let jvm = merged["arguments"]["jvm"].as_array().expect("jvm 为数组");
        assert_eq!(jvm.len(), 2);
        assert_eq!(jvm[1], "-Dfabric.side=client");
        // inheritsFrom 被删除
        assert!(merged.get("inheritsFrom").is_none());
        // 原版 downloads 保留（jar 下载信息不丢）
        assert!(merged["downloads"]["client"].is_object());
        // maven 库转换
        let libs = merged["libraries"].as_array().expect("libraries 为数组");
        assert_eq!(libs.len(), 2);
        let artifact = &libs[0]["downloads"]["artifact"];
        assert_eq!(
            artifact["path"],
            "net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar"
        );
        assert_eq!(artifact["url"], "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar");
        assert_eq!(artifact["sha1"], "d34d");
        assert_eq!(artifact["size"], 5000);
        // 无 url 的库回落到 maven_url；md5/url 等字段被清除
        assert!(libs[0].get("url").is_none());
        assert!(libs[0].get("md5").is_none());
        assert_eq!(
            libs[1]["downloads"]["artifact"]["url"],
            "https://libraries.minecraft.net/net/fabricmc/tiny-mappings-parser/0.3.0/tiny-mappings-parser-0.3.0.jar"
        );
    }

    #[test]
    fn merge_loader_profile_invalid_parent_returns_err() {
        let profile = serde_json::json!({"mainClass": "x"});
        let r = merge_loader_profile("not json", &profile, "id", "maven");
        assert!(r.is_err(), "原版 JSON 损坏时应返回 Err");
    }

    /// Forge 风格子 JSON：仅自身库 + inheritsFrom 父指针。
    fn forge_child() -> serde_json::Value {
        serde_json::json!({
            "id": "1.20.1-forge-47.1.0",
            "inheritsFrom": "1.20.1",
            "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "minecraftArguments": "--username ${auth_player_name}",
            "arguments": {
                "jvm": ["-Dforge.logging.console.level=debug"],
                "game": ["--fml.forgeVersion", "47.1.0"]
            },
            "libraries": [{ "name": "net.minecraftforge:forge:47.1.0" }]
        })
    }

    #[test]
    fn expand_inheritance_merges_forge_json() {
        let merged =
            expand_inheritance(PARENT, forge_child(), "1.20.1-forge-47.1.0").expect("展开应成功");
        assert_eq!(merged["id"], "1.20.1-forge-47.1.0");
        assert!(merged.get("inheritsFrom").is_none());
        // mainClass/minecraftArguments 子覆盖
        assert_eq!(
            merged["mainClass"],
            "cpw.mods.bootstraplauncher.BootstrapLauncher"
        );
        assert_eq!(
            merged["minecraftArguments"],
            "--username ${auth_player_name}"
        );
        // libraries：父（空）+ 子 1 个
        let libs = merged["libraries"].as_array().expect("libraries 为数组");
        assert_eq!(libs.len(), 1);
        assert_eq!(libs[0]["name"], "net.minecraftforge:forge:47.1.0");
        // arguments：jvm 父1+子1；game 父无 → 子覆盖
        let jvm = merged["arguments"]["jvm"].as_array().expect("jvm 为数组");
        assert_eq!(jvm.len(), 2);
        assert_eq!(jvm[0], "-Xmx2G");
        assert_eq!(jvm[1], "-Dforge.logging.console.level=debug");
        let game = merged["arguments"]["game"].as_array().expect("game 为数组");
        assert_eq!(game.len(), 2);
        // 父字段继承：downloads/type
        assert!(merged["downloads"]["client"].is_object());
        assert_eq!(merged["type"], "release");
    }

    #[test]
    fn expand_inheritance_appends_game_args_to_parent() {
        let parent = r#"{
            "id": "1.20.1",
            "mainClass": "net.minecraft.client.main.Main",
            "arguments": { "jvm": ["-Xmx2G"], "game": ["--version", "${version_name}"] },
            "libraries": [],
            "downloads": { "client": { "url": "x", "sha1": "y", "size": 1 } },
            "type": "release"
        }"#;
        let merged =
            expand_inheritance(parent, forge_child(), "1.20.1-forge-47.1.0").expect("展开应成功");
        let game = merged["arguments"]["game"].as_array().expect("game 为数组");
        assert_eq!(game.len(), 4);
        assert_eq!(game[0], "--version");
        assert_eq!(game[2], "--fml.forgeVersion");
    }

    #[test]
    fn expand_inheritance_without_parent_returns_child() {
        let standalone = serde_json::json!({
            "id": "x",
            "mainClass": "net.minecraft.client.main.Main",
            "libraries": []
        });
        let merged = expand_inheritance("unused", standalone.clone(), "26.2-fabric-0.15.11")
            .expect("无 inheritsFrom 时应直接返回");
        assert_eq!(merged["id"], "26.2-fabric-0.15.11");
        assert_eq!(merged["mainClass"], "net.minecraft.client.main.Main");
    }

    #[test]
    fn expand_inheritance_invalid_parent_returns_err() {
        let r = expand_inheritance("not json", forge_child(), "id");
        assert!(r.is_err(), "父 JSON 损坏时应返回 Err");
    }
}
