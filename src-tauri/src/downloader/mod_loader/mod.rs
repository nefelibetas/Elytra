// ── ModLoader 引擎：公共纯函数 + 子模块 re-export ──
// fabric.rs（profile 式：Fabric/Quilt）、forge.rs（installer 式：Forge/NeoForge）、optifine.rs。

pub mod fabric;
pub mod forge;
pub mod optifine;

pub use fabric::{install_fabric, install_quilt};
pub use forge::{install_forge, install_neoforge};
pub use optifine::install_optifine;

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
        serde_json::from_str(parent_text).map_err(|e| format!("解析原版版本 JSON 失败: {}", e))?;

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
/// 以父 JSON 为基 → 追加子库/参数 → 子关键字段覆盖（缺则保留父）→ 删 inheritsFrom。
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
        serde_json::from_str(parent_text).map_err(|e| format!("解析原版版本 JSON 失败: {}", e))?;

    // libraries：父 + 子 拼接（父在前）
    let child_libs = child["libraries"].take();
    if let Some(libs) = child_libs.as_array() {
        if let Some(parent_libs) = merged["libraries"].as_array_mut() {
            parent_libs.extend(libs.iter().cloned());
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
