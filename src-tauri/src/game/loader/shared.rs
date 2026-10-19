//! ModLoader 安装的公共纯函数：profile 合并 / 继承展开 / 幂等判定 / 安装器进程管理（fabric/forge/neoforge/optifine 共用）。

use std::process::Output;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::process::Child;

fn maven_path(name: &str) -> String {
    // 坐标形态 `group:artifact:version[:classifier][@ext]`（classifier 与扩展名都必须进路径，否则 native 分类器会指向错误文件名）
    let (coord, ext) = match name.split_once('@') {
        Some((c, e)) if !e.is_empty() => (c, e),
        _ => (name, "jar"),
    };
    let parts: Vec<&str> = coord.split(':').collect();
    if parts.len() < 3 {
        return format!("{}.{ext}", coord.replace(':', "_"));
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let group_path = group.replace('.', "/");
    match parts.get(3).copied().filter(|c| !c.is_empty()) {
        Some(classifier) => {
            format!("{group_path}/{artifact}/{version}/{artifact}-{version}-{classifier}.{ext}")
        }
        None => format!("{group_path}/{artifact}/{version}/{artifact}-{version}.{ext}"),
    }
}

/// 老式库条目补全下载信息：spec-0 安装器（Forge 1.7.10~1.11.2）的 `versionInfo.libraries` 只有 `{name, url}`、
/// 无 `downloads.artifact`；classpath 与缺库检查从 artifact.path 取路径，缺了静默丢失（装得下但起不来）。按坐标补出
/// path/url/sha1/size；`url` 为空的老式条目（如 `net.minecraft:launchwrapper:1.12`）按 Mojang libraries 兜底。
fn normalize_legacy_libraries(libs: &mut [serde_json::Value]) {
    for lib in libs.iter_mut() {
        // 已有 artifact 或 classifiers（老式 natives 只有 classifiers、无普通构件，按坐标合成会得到 CDN 上不存在的普通 jar → 404）→ 都不动
        let has_download_info = lib
            .get("downloads")
            .and_then(|d| d.as_object())
            .is_some_and(|d| {
                d.get("artifact").is_some_and(|a| !a.is_null()) || d.contains_key("classifiers")
            });
        if has_download_info || lib.get("natives").is_some() {
            continue;
        }
        let Some(name) = lib.get("name").and_then(|n| n.as_str()).map(str::to_string) else {
            continue;
        };
        let path = maven_path(&name);
        let base = lib
            .get("url")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        let url = if base.is_empty() {
            format!(
                "{}/{}",
                crate::downloader::endpoints::MOJANG_LIBRARIES,
                path
            )
        } else {
            format!("{base}/{path}")
        };
        let sha1 = lib
            .get("sha1")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let size = lib.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        lib["downloads"] = serde_json::json!({
            "artifact": { "path": path, "url": url, "sha1": sha1, "size": size }
        });
    }
}

/// 合并 loader profile 到原版 JSON（Fabric/Quilt 官方 profile 接口）：追加 jvm/game 参数、替换 mainClass、追加 libraries（maven 库转 downloads.artifact）、删 inheritsFrom、保留原版 downloads。
pub(crate) fn merge_loader_profile(
    parent_text: &str,
    profile: &serde_json::Value,
    dir_name: &str,
    maven_url: &str,
) -> Result<serde_json::Value, String> {
    let mut merged: serde_json::Value =
        serde_json::from_str(parent_text).map_err(common_err_ctx!("解析原版版本 JSON 失败"))?;

    for key in ["jvm", "game"] {
        let loader_args = profile["arguments"][key].as_array();
        if let Some(loader_args) = loader_args {
            if !loader_args.is_empty() {
                if let Some(args) = merged
                    .get_mut("arguments")
                    .and_then(|a| a.get_mut(key))
                    .and_then(|v| v.as_array_mut())
                {
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

    // loader（子级）库合成下载信息后前置，vanilla 同键条目丢弃：classpath 里 loader 版本须先被 JVM 命中（asm/mixin 与 vanilla 重叠），且避免同坐标两份 jar 让缺库计数双计。
    let mut loader_out: Vec<serde_json::Value> = Vec::new();
    if let Some(loader_libs) = profile["libraries"].as_array() {
        {
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
                loader_out.push(new_lib);
            }
        }
    }
    if !loader_out.is_empty() {
        let loader_keys: std::collections::HashSet<String> = loader_out
            .iter()
            .filter_map(|l| l["name"].as_str())
            .map(library_key)
            .collect();
        let mut combined = loader_out;
        if let Some(parent_libs) = merged.get("libraries").and_then(|v| v.as_array()) {
            for l in parent_libs {
                let key = l["name"].as_str().map(library_key).unwrap_or_default();
                if !loader_keys.contains(&key) {
                    combined.push(l.clone());
                }
            }
        }
        normalize_legacy_libraries(&mut combined);
        merged["libraries"] = serde_json::Value::Array(combined);
    }
    unify_game_arg_form(
        &mut merged,
        profile["minecraftArguments"].as_str().unwrap_or("").trim(),
        profile["arguments"]["game"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false),
    );

    if let Some(obj) = merged.as_object_mut() {
        obj.remove("inheritsFrom");
    }
    Ok(merged)
}

/// 库去重键 `group:artifact[:classifier]`：版本不参与（同坐标不同版本子级优先）；classifier 必须参与，否则 native 分类器会被当成主 jar 同名条目顶掉。
fn library_key(name: &str) -> String {
    let name = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 2 {
        return name.to_string();
    }
    match parts.get(3).filter(|c| !c.is_empty()) {
        Some(classifier) => format!("{}:{}:{}", parts[0], parts[1], classifier),
        None => format!("{}:{}", parts[0], parts[1]),
    }
}

/// 注意：`serde_json` 字符串索引**读时插入 `null`**（`v["missing"]` 会写进 `"missing": null`），故本文件探测一律走 `get`/`get_mut`，只有写入才用索引——
/// 否则老版本（1.7.10 等）合并出的 JSON 会多出 `"arguments": null`，读取端反序列化直接报 `invalid type: null, expected struct Arguments`。
/// 游戏参数两形态：`arguments.game`（1.13+ 数组）与 `minecraftArguments`（旧式模板串）。`build_game_args` 并存时"新格式优先"取 `arguments.game`，故"子级只有旧式串 + 父级有 arguments.game"时子级整串会被静默丢弃。
/// 合并阶段消歧：子级只有旧式串 → 丢继承的 arguments.game；只有新式数组 → 丢继承的旧式串；都声明/都没声明 → 原样保留。标志须在 `.take()` 前取出（take 会置空子级字段）。
fn unify_game_arg_form(
    merged: &mut serde_json::Value,
    child_legacy_args: &str,
    child_has_game_args: bool,
) {
    let legacy_only = !child_legacy_args.is_empty() && !child_has_game_args;
    let modern_only = child_has_game_args && child_legacy_args.is_empty();
    if legacy_only {
        if let Some(obj) = merged.get_mut("arguments").and_then(|v| v.as_object_mut()) {
            obj.remove("game");
        }
        merged["minecraftArguments"] = serde_json::Value::String(child_legacy_args.to_string());
    } else if modern_only {
        if let Some(obj) = merged.as_object_mut() {
            obj.remove("minecraftArguments");
        }
    }
}

/// 展开 inheritsFrom 继承（version.json 只有自身库 + 父指针）：以父 JSON 为基 → 子库拼到父库前 → 追加子参数 →
/// 子关键字段覆盖（缺则保留父）→ 删 inheritsFrom；子无 inheritsFrom 时视为完整独立 JSON，直接返回（仅改 id）。
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
        serde_json::from_str(parent_text).map_err(common_err_ctx!("解析原版版本 JSON 失败"))?;
    let parent_inherits = merged.get("inheritsFrom").cloned();

    // libraries：子 + 父 拼接（子在前，官方启动器/HMCL 惯例）。OptiFabric/纯净版 OptiFine 依赖此顺序：预补丁客户端 jar 须先于 vanilla 库进 classpath，JVM 同名类先命中 OptiFine 实现才能覆盖
    let child_libs = child
        .get_mut("libraries")
        .map(|v| v.take())
        .unwrap_or(serde_json::Value::Null);
    // 父级库也要规范化：老式条目（只有 name/url、无 downloads.artifact）可能只在父级，缺 path 会静默丢出 classpath
    let mut combined: Vec<serde_json::Value> = merged
        .get("libraries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if let Some(libs) = child_libs.as_array() {
        // 子级库在前，父级同键条目丢弃（同坐标不同版本子级优先；避免两份 jar 同进 classpath 与缺库计数）
        let child_keys: std::collections::HashSet<String> = libs
            .iter()
            .filter_map(|l| l["name"].as_str())
            .map(library_key)
            .collect();
        let mut out: Vec<serde_json::Value> = libs.clone();
        for l in &combined {
            let key = l["name"].as_str().map(library_key).unwrap_or_default();
            if !child_keys.contains(&key) {
                out.push(l.clone());
            }
        }
        combined = out;
    }
    normalize_legacy_libraries(&mut combined);
    merged["libraries"] = serde_json::Value::Array(combined);

    // 参数形态标志要在下面 `.take()` 之前取（take 会把子级字段置空）
    let child_legacy_args = child
        .get("minecraftArguments")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let child_has_game_args = child
        .get("arguments")
        .and_then(|a| a.get("game"))
        .and_then(|v| v.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false);

    // arguments：父 + 子 拼接（jvm/game）
    for key in ["jvm", "game"] {
        let child_args = child
            .get_mut("arguments")
            .and_then(|a| a.get_mut(key))
            .map(|v| v.take());
        if let Some(args) = child_args.as_ref().and_then(|v| v.as_array()) {
            if let Some(parent_args) = merged
                .get_mut("arguments")
                .and_then(|a| a.get_mut(key))
                .and_then(|v| v.as_array_mut())
            {
                parent_args.extend(args.iter().cloned());
            } else {
                // 确实要写入时才用索引赋值（会按需创建 arguments 对象）
                merged["arguments"][key] = serde_json::Value::Array(args.clone());
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

    unify_game_arg_form(&mut merged, &child_legacy_args, child_has_game_args);

    merged["id"] = serde_json::Value::String(dir_name.to_string());
    // 保留父级自己的 inheritsFrom：调用方（`launcher::env`）靠它继续向上展开多级链（整合包 → Forge → vanilla）；直接 remove 会让多级继承退化成单级，祖父版本整体丢失。父级无继承时才删。
    match parent_inherits {
        Some(v) => merged["inheritsFrom"] = v,
        None => {
            if let Some(obj) = merged.as_object_mut() {
                obj.remove("inheritsFrom");
            }
        }
    }
    Ok(merged)
}

/// 运行安装器 Java 工具并收集输出（stdout/stderr 完整返回）：cancel 置位时 kill 子进程；用 tokio::select!
/// 同时等待进程退出与排空输出 pipe（避免大输出撑满 pipe 阻塞子进程）。
pub(crate) async fn run_installer_command(
    mut child: Child,
    cancel: &Arc<AtomicBool>,
    label: &str,
) -> Result<Output, String> {
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
            st = child.wait() => break st.map_err(common_err_ctx!("运行{}失败", label))?,
            n = stdout.read_buf(&mut out_buf) => { let _ = n; }
            n = stderr.read_buf(&mut err_buf) => { let _ = n; }
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {}
        }
    };
    // 进程退出后 flush pipe 剩余数据
    let _ = stdout.read_to_end(&mut out_buf).await;
    let _ = stderr.read_to_end(&mut err_buf).await;
    Ok(Output {
        status,
        stdout: out_buf,
        stderr: err_buf,
    })
}

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
    fn maven_path_full_coordinate() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.15.11"),
            "net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar"
        );
    }

    /// 老式条目（无 downloads.artifact）必须补出路径，否则 classpath 静默丢失 loader 本体。
    #[test]
    fn legacy_library_entries_get_artifact() {
        // 父级同时含：老式无下载信息条目（launchwrapper）、老式 natives-only 条目（只有 classifiers）
        let parent = r#"{"id":"p","libraries":[
            {"name":"net.minecraft:launchwrapper:1.12","url":"https://libraries.minecraft.net/"},
            {"name":"org.lwjgl.lwjgl:lwjgl-platform:2.9.1","natives":{"windows":"natives-windows"},
             "downloads":{"classifiers":{"natives-windows":{"path":"org/lwjgl/lwjgl/lwjgl-platform/2.9.1/lwjgl-platform-2.9.1-natives-windows.jar","sha1":"","size":1,"url":"https://libraries.minecraft.net/org/lwjgl/lwjgl/lwjgl-platform/2.9.1/lwjgl-platform-2.9.1-natives-windows.jar"}}}}
        ]}"#;
        let child = serde_json::json!({
            "id": "c", "inheritsFrom": "p", "mainClass": "net.minecraft.launchwrapper.Launch"
        });
        let out = expand_inheritance(parent, child, "1.7.10-forge-x").expect("展开失败");
        let libs = out["libraries"].as_array().unwrap();
        let lw = libs
            .iter()
            .find(|l| l["name"].as_str().unwrap_or("").contains("launchwrapper"))
            .expect("缺 launchwrapper");
        assert_eq!(
            lw["downloads"]["artifact"]["path"],
            "net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar"
        );
        assert!(lw["downloads"]["artifact"]["url"]
            .as_str()
            .unwrap()
            .ends_with("/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar"));
        // natives-only 条目不得被合成普通构件（否则会去下 CDN 上不存在的 -platform 普通 jar → 404）
        let plat = libs
            .iter()
            .find(|l| l["name"].as_str().unwrap_or("").contains("lwjgl-platform"))
            .expect("缺 lwjgl-platform");
        assert!(
            plat["downloads"].get("artifact").is_none(),
            "natives-only 条目不应被合成 artifact"
        );
        assert!(plat["downloads"]["classifiers"]["natives-windows"]["path"]
            .as_str()
            .unwrap()
            .ends_with("-natives-windows.jar"));
    }

    #[test]
    fn maven_path_classifier_and_ext() {
        assert_eq!(
            maven_path("net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10"),
            "net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10.jar"
        );
        assert_eq!(
            maven_path("net.minecraft:lwjgl:2.9.4:natives-windows"),
            "net/minecraft/lwjgl/2.9.4/lwjgl-2.9.4-natives-windows.jar"
        );
        assert_eq!(
            maven_path("de.oceanlabs.mcp:mcp_config:1.12.2@zip"),
            "de/oceanlabs/mcp/mcp_config/1.12.2/mcp_config-1.12.2.zip"
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

    /// 多级继承（A→B→C）：连续展开后祖先的库/参数必须在结果里，且合并后保留父级 inheritsFrom 供继续展开。
    #[test]
    fn expand_inheritance_survives_multi_level_chain() {
        let grandparent = r#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main",
            "libraries":[{"name":"com.example:gp:1"}],"arguments":{"game":["--gp"]}}"#;
        let parent = r#"{"id":"1.20.1-forge","inheritsFrom":"1.20.1",
            "mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
            "libraries":[{"name":"com.example:parent:1"}],"arguments":{"game":["--parent"]}}"#;
        let child = r#"{"id":"pack-1.20.1","inheritsFrom":"1.20.1-forge",
            "libraries":[{"name":"com.example:child:1"}],"arguments":{"game":["--child"]}}"#;

        let child: serde_json::Value = serde_json::from_str(child).unwrap();
        let lvl1 = expand_inheritance(parent, child, "pack-1.20.1").unwrap();
        assert_eq!(
            lvl1["inheritsFrom"], "1.20.1",
            "合并后必须保留父级自己的 inheritsFrom，调用方才能继续向上展开"
        );

        let lvl2 = expand_inheritance(grandparent, lvl1, "pack-1.20.1").unwrap();
        let libs: Vec<String> = lvl2["libraries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            libs,
            vec![
                "com.example:child:1",
                "com.example:parent:1",
                "com.example:gp:1"
            ],
            "三级库都应保留，且子级在前（classpath 优先级）"
        );
        let args: Vec<String> = lvl2["arguments"]["game"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            args,
            vec!["--gp", "--parent", "--child"],
            "参数应为 祖先→子 顺序"
        );
        assert_eq!(
            lvl2["mainClass"], "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "子级 mainClass 必须覆盖父级"
        );
        assert!(
            lvl2.get("inheritsFrom").is_none(),
            "链尾应清掉 inheritsFrom"
        );
        assert_eq!(lvl2["id"], "pack-1.20.1");
    }

    /// 库去重：同坐标（group:artifact[:classifier]）**子级/loader 优先**，父级同键条目丢弃。
    #[test]
    fn libraries_dedup_child_wins() {
        let parent =
            r#"{"id":"1.20.1","libraries":[{"name":"org.ow2.asm:asm:9.3"},{"name":"keep:me:1"}]}"#;
        let child =
            r#"{"id":"v","inheritsFrom":"1.20.1","libraries":[{"name":"org.ow2.asm:asm:9.10.1"}]}"#;
        let merged = expand_inheritance(parent, serde_json::from_str(child).unwrap(), "v").unwrap();
        let names: Vec<String> = merged["libraries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["org.ow2.asm:asm:9.10.1", "keep:me:1"],
            "子级版本保留、父级同坐标丢弃、无关键照旧"
        );
    }

    /// classifier 必须参与去重键：主 jar 与 native 分类器不能互相顶掉。
    #[test]
    fn library_key_includes_classifier() {
        assert_eq!(library_key("org.lwjgl:lwjgl:3.3.1"), "org.lwjgl:lwjgl");
        assert_eq!(
            library_key("org.lwjgl:lwjgl:3.3.1:natives-windows"),
            "org.lwjgl:lwjgl:natives-windows"
        );
        assert_eq!(library_key("g:a:1@zip"), "g:a");
    }

    /// 游戏参数形态二选一（子级旧式 vs 继承来的新式）。
    #[test]
    fn child_legacy_args_win_over_inherited_modern_args() {
        let parent = r#"{"id":"1.20.1","arguments":{"game":["--parent-game"]}}"#;
        let child = r#"{"id":"v","inheritsFrom":"1.20.1","minecraftArguments":"--child ${auth_player_name}"}"#;
        let merged = expand_inheritance(parent, serde_json::from_str(child).unwrap(), "v").unwrap();
        assert!(
            merged["arguments"].get("game").is_none(),
            "子级声明旧式参数时，继承来的 arguments.game 必须丢弃（否则子级参数永远用不上）"
        );
        assert!(merged["minecraftArguments"]
            .as_str()
            .unwrap()
            .contains("--child"));
    }

    /// 反向：子级声明新式 arguments.game 时，继承来的旧式串必须丢弃。
    #[test]
    fn child_modern_args_win_over_inherited_legacy_string() {
        let parent = r#"{"id":"1.20.1","minecraftArguments":"--parent-legacy"}"#;
        let child = r#"{"id":"v","inheritsFrom":"1.20.1","arguments":{"game":["--child-game"]}}"#;
        let merged = expand_inheritance(parent, serde_json::from_str(child).unwrap(), "v").unwrap();
        assert!(
            merged.get("minecraftArguments").is_none(),
            "子级声明新式参数时，继承来的旧式串必须丢弃"
        );
        assert_eq!(merged["arguments"]["game"][0], "--child-game");
    }
}
