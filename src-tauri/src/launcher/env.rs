//! 启动环境准备：加载配置、解析版本 JSON、完整性检查、JDK 匹配、游戏语言。
//! 命令层（commands/launch.rs）的启动前置逻辑集中于此。

use std::path::PathBuf;

use crate::commands::version::VersionServer;
use crate::config::downloader::DownloaderConfig;
use crate::constants;
use crate::downloader::hash::sha1_hex;
use crate::game::loader::expand_inheritance;
use crate::game::resource::assets::AssetsIndex;
use crate::game::resource::library::missing_libraries;
use crate::game::version::dir::version_server_section;
use crate::game::version::model::VersionJson;
use crate::jdk::model::{JdkInfo, JdkOptions, JdkScanCache, JdkSelection};
use crate::launcher::args::find_version_json;
use crate::launcher::model::LaunchOptions;
use crate::persist::{self, downloader_config, launch_options};
use crate::util::{java_bin_path, parse_java_major, sha1_sub_dir};

/// 按 JDK 选择配置解析实际使用的 JDK 版本字符串：
/// - Manual：在扫描缓存中按 path 匹配，返回其 version；
/// - Auto / 未匹配：None（调用方自行给兜底文案）。
pub fn selected_jdk_version(options: &JdkOptions, cache: &JdkScanCache) -> Option<String> {
    match &options.selection {
        JdkSelection::Manual { path } => cache
            .jdks
            .iter()
            .find(|e| &e.path == path)
            .map(|e| e.version.clone()),
        JdkSelection::Auto => None,
    }
}

/// 加载全部持久化配置 + 解析版本 JSON，供 launch/spawn 共用。
#[allow(clippy::type_complexity)]
pub fn load_launch_config(
    version_dir_name: &str,
) -> Result<
    (
        PathBuf,     // java_path
        VersionJson, // version_json
        PathBuf,     // mc_dir
        LaunchOptions,
        u32, // java_major
    ),
    String,
> {
    let dl_config: DownloaderConfig = downloader_config();
    let jdk_options: JdkOptions = persist::read(constants::SECT_JDK).unwrap_or_default();
    let jdk_cache: JdkScanCache = persist::read(constants::SECT_JDK_SCANNED).unwrap_or_default();
    let launch_option: LaunchOptions = launch_options();

    // 版本级服务器直连（版本设置里配的 --server/--port）优先，覆盖全局 LaunchOptions
    let version_server: VersionServer = version_server_section(version_dir_name)
        .ok()
        .and_then(|s| persist::read(&s).ok())
        .unwrap_or_default();
    let launch_option = if version_server.server.is_some() || version_server.port.is_some() {
        LaunchOptions {
            server: version_server.server.or(launch_option.server),
            port: version_server.port.or(launch_option.port),
            ..launch_option
        }
    } else {
        launch_option
    };

    // minecraft_dir 可能是相对路径（默认 ".minecraft"），统一转绝对，
    // 避免 classpath / natives 路径基于 java 进程 cwd 解析出错
    let mc_dir = dl_config.minecraft_dir_abs();

    // 读取版本 JSON（扫描目录，文件名不一定是目录名），并展开 inheritsFrom 继承
    let vdir = mc_dir.join("versions").join(version_dir_name);
    if !vdir.is_dir() {
        bail!("版本目录不存在\n{}", vdir.display());
    }
    let version_json = load_version_json_with_inheritance(&mc_dir, version_dir_name)?;

    let required = version_json
        .java_version
        .major_version
        .as_u64()
        .unwrap_or(0) as u32;
    let java_env = match jdk_options.selection {
        JdkSelection::Manual { path } => jdk_cache
            .jdks
            .iter()
            .find(|e| e.path == path)
            .cloned()
            .ok_or_else(|| "未找到指定的 JDK".to_string())?,
        JdkSelection::Auto => auto_pick_jdk(&jdk_cache.jdks, required).ok_or_else(|| {
            if required > 0 {
                format!("需要 Java {}+，请在设置中选择或刷新 JDK", required)
            } else {
                "未配置 JDK，请先在设置中选择 Java 路径".to_string()
            }
        })?,
    };
    // java_env.path 是 JAVA_HOME（目录），需要拼上 bin/java.exe
    let java_home = PathBuf::from(&java_env.path);
    if !java_home.exists() {
        bail!("JDK 路径不存在\n{}", java_home.display());
    }
    let java_path = java_bin_path(&java_home);
    if !java_path.exists() {
        bail!("未找到 Java 可执行文件\n{}", java_path.display());
    }

    // 解析 Java 主版本号
    let java_major = parse_java_major(&java_env.version)
        .ok_or_else(|| format!("无法解析 JDK 版本号\n{}", java_env.version))?;

    Ok((java_path, version_json, mc_dir, launch_option, java_major))
}

/// 读取版本 JSON 并展开 inheritsFrom 继承。
/// HMCL/官方安装器等生成的 ModLoader 版本 json 常为继承式（只含自身库 +
/// inheritsFrom 指向原版，父库在父版本 json 里）——直接解析会缺父库导致
/// 启动失败。Elytra 自装版本（merge_loader_profile）已是完整 json 无继承，
/// 原样返回；父版本 json 缺失时明确报错（引导补装原版）。
fn load_version_json_with_inheritance(
    mc_dir: &std::path::Path,
    version_dir_name: &str,
) -> Result<VersionJson, String> {
    let vdir = mc_dir.join("versions").join(version_dir_name);
    let vj_path = find_version_json(&vdir, version_dir_name)
        .ok_or_else(|| format!("版本目录下未找到版本 JSON\n{}", vdir.display()))?;
    let vj_text =
        std::fs::read_to_string(&vj_path).map_err(common_err_ctx!("读取版本 JSON 失败"))?;
    let mut vj_value: serde_json::Value =
        serde_json::from_str(&vj_text).map_err(common_err_ctx!("解析版本 JSON 失败"))?;

    // 展开继承链：子库 + 父库合并、子关键字段覆盖、删 inheritsFrom
    while let Some(parent_id) = vj_value.get("inheritsFrom").and_then(|v| v.as_str()) {
        let parent_path = mc_dir
            .join("versions")
            .join(parent_id)
            .join(format!("{parent_id}.json"));
        let parent_text = std::fs::read_to_string(&parent_path).map_err(|e| {
            format!(
                "读取父版本 JSON（{parent_id}）失败——该版本为继承式安装，请先用 Elytra 补装原版 {parent_id}\n{}",
                e
            )
        })?;
        vj_value = expand_inheritance(&parent_text, vj_value, version_dir_name)?;
    }

    serde_json::from_value(vj_value).map_err(common_err_ctx!("解析版本 JSON 失败"))
}

/// 启动前完整性检测：校验 client JAR 和所有必需 library 是否已下载。
pub fn pre_launch_check(
    version_json: &VersionJson,
    mc_dir: &std::path::Path,
    version_dir_name: &str,
) -> Result<(), String> {
    let mut missing: Vec<String> = Vec::new();

    // 1. client JAR（存在但 0 字节视为损坏）
    let vdir = mc_dir.join("versions").join(version_dir_name);
    let jar_ok = |p: &std::path::Path| p.metadata().map(|m| m.len() > 0).unwrap_or(false);
    // 与 classpath 用同一套解析规则（否则"检查通过但启动用错 jar"）
    if super::args::resolve_client_jar(&vdir, version_dir_name)
        .filter(|p| jar_ok(p))
        .is_none()
    {
        missing.push(format!(
            "客户端 JAR: {}",
            vdir.join(format!("{}.jar", version_dir_name)).display()
        ));
    }

    // 2. libraries（复用下载侧过滤 + sha1 校验：缺失/损坏都算）
    let lib_dir = mc_dir.join("libraries");
    let missing_libs = missing_libraries(&version_json.libraries, &lib_dir);
    if !missing_libs.is_empty() {
        let mut msg = format!("缺失支持库 {} 个（含 sha1 校验）", missing_libs.len());
        for p in missing_libs.iter().take(20) {
            msg.push_str(&format!("\n  {}", p));
        }
        if missing_libs.len() > 20 {
            msg.push_str(&format!("\n  … 其余 {} 个", missing_libs.len() - 20));
        }
        missing.push(msg);
    }

    // 3. assets index（存在 + sha1 校验）+ 资源对象扫描。
    // 此前仅检查索引文件是否存在——索引损坏（镜像返回错误页/半截文件）或资源对象
    // 缺失/被截断（BMCLAPI minio 存储异常时返回 567/429 错误页覆盖对象）都会"通过"
    // 完整性检查，游戏启动后表现为无声音/无语言文件/贴图缺失。此处补齐两层校验。
    let assets_dir = mc_dir.join("assets");
    let index_path = assets_dir
        .join("indexes")
        .join(format!("{}.json", version_json.asset_index.id));
    let index_sha1 = version_json.asset_index.sha1.as_str();
    match std::fs::read(&index_path) {
        Err(_) => {
            missing.push(format!("资源索引: {}", index_path.display()));
        }
        Ok(index_bytes) => {
            // 索引 sha1 校验（version_json 提供期望值时；空则跳过校验，仅确认可解析）
            if !index_sha1.is_empty() && sha1_hex(&index_bytes) != index_sha1 {
                missing.push(format!(
                    "资源索引校验失败（sha1 不匹配，文件已损坏）: {}",
                    index_path.display()
                ));
            } else {
                // 索引可信 → 扫描资源对象（objects/{sub}/{hash}）。
                // 对象数可达数千，逐个 sha1 会让每次启动多花数秒；改为存在 + 文件大小校验
                // （metadata 极廉价）：缺失/被截断/错误页覆盖（大小不符）都能可靠捕获，
                // 正好覆盖镜像存储异常这一实际故障场景。
                match serde_json::from_slice::<AssetsIndex>(&index_bytes) {
                    Err(e) => {
                        missing.push(format!("资源索引解析失败（文件已损坏）: {e}"));
                    }
                    Ok(index) => {
                        let objects_dir = assets_dir.join("objects");
                        let mut bad_objects: Vec<String> = Vec::new();
                        for (name, obj) in &index.objects {
                            let sub = sha1_sub_dir(&obj.hash);
                            let path = objects_dir.join(sub).join(&obj.hash);
                            let ok = std::fs::metadata(&path)
                                .map(|m| m.len() == obj.size)
                                .unwrap_or(false);
                            if !ok {
                                bad_objects.push(format!("{} ({}/{})", name, sub, obj.hash));
                            }
                        }
                        if !bad_objects.is_empty() {
                            let mut msg =
                                format!("缺失/损坏资源文件 {} 个（大小校验）", bad_objects.len());
                            for b in bad_objects.iter().take(20) {
                                msg.push_str(&format!("\n  {}", b));
                            }
                            if bad_objects.len() > 20 {
                                msg.push_str(&format!("\n  … 其余 {} 个", bad_objects.len() - 20));
                            }
                            missing.push(msg);
                        }
                    }
                }
            }
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        // 第一行为大类短句（dialog 正文显示），文件列表进复制区
        Err(format!(
            "游戏文件不完整\n缺少以下文件：\n{}",
            missing.join("\n")
        ))
    }
}

/// options.txt 行 upsert：替换以 `key:` 开头的行，不存在则追加（文件不存在时创建）。
/// apply_game_language（lang）与 skin.rs（resourcePacks）共用。
pub fn upsert_option(options_path: &std::path::Path, key: &str, value: &str) {
    let line = format!("{key}:{value}");
    let content = match std::fs::read_to_string(options_path) {
        Ok(c) => c,
        Err(_) => {
            // 文件不存在：预写，游戏首次启动沿用
            if let Some(parent) = options_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(options_path, format!("{line}\n"));
            return;
        }
    };

    let mut found = false;
    let mut out = String::new();
    for l in content.lines() {
        if l.starts_with(&format!("{key}:")) {
            out.push_str(&line);
            out.push('\n');
            found = true;
        } else {
            out.push_str(l);
            out.push('\n');
        }
    }
    if !found {
        out.push_str(&line);
        out.push('\n');
    }
    let _ = std::fs::write(options_path, out);
}

/// 游戏语言：自动设置为中文。
/// 1.13+ 游戏启动时读取 gameDir/options.txt 的 `lang` 字段，
/// 存在则替换，否则追加（文件不存在时创建，游戏启动后保留该值）。
pub fn apply_game_language(game_dir: &std::path::Path) {
    upsert_option(&game_dir.join("options.txt"), "lang", "zh_cn");
}

/// 从已扫描的 JDK 列表中自动选择最合适的版本。
/// 优先精确匹配需求版本；否则选最低但满足需求的版本；再不行选最高版本。
fn auto_pick_jdk(discovered: &[JdkInfo], required: u32) -> Option<JdkInfo> {
    let mut candidates: Vec<(u32, &JdkInfo)> = discovered
        .iter()
        .filter_map(|env| parse_java_major(&env.version).map(|m| (m, env)))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    // 精确匹配
    if let Some(env) = candidates.iter().find(|(m, _)| *m == required) {
        return Some(env.1.clone());
    }
    // 最低但满足需求
    let mut enough: Vec<_> = candidates.iter().filter(|(m, _)| *m >= required).collect();
    if !enough.is_empty() {
        enough.sort_by_key(|(m, _)| *m);
        return Some(enough[0].1.clone());
    }
    // 都不满足 → 用最高版本（列表可能为空，返回 None 由调用方兜底）
    candidates.sort_by_key(|(m, _)| *m);
    candidates.last().map(|(_, v)| (*v).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_pick_jdk() {
        let list = vec![
            JdkInfo {
                path: "/jdk17".into(),
                version: "17.0.8".into(),
                vendor: "OpenJDK".into(),
                bits: 64,
            },
            JdkInfo {
                path: "/jdk8".into(),
                version: "1.8.0_202".into(),
                vendor: "Oracle".into(),
                bits: 64,
            },
            JdkInfo {
                path: "/jdk21".into(),
                version: "21".into(),
                vendor: "Temurin".into(),
                bits: 64,
            },
        ];
        // 精确匹配
        assert_eq!(auto_pick_jdk(&list, 21).unwrap().path, "/jdk21");
        // 无精确匹配时选最低但满足的
        assert_eq!(auto_pick_jdk(&list, 17).unwrap().path, "/jdk17");
        assert_eq!(auto_pick_jdk(&list, 11).unwrap().path, "/jdk17");
        // 都不满足时用最高版本
        assert_eq!(auto_pick_jdk(&list, 22).unwrap().path, "/jdk21");
        // 空列表
        assert!(auto_pick_jdk(&[], 8).is_none());
    }

    /// 继承式版本（HMCL/官方安装器风格）：子 json 只有自身库 + inheritsFrom 父指针，
    /// 展开后应合并父库、子关键字段覆盖、父版本缺失时明确报错。
    #[test]
    fn version_json_expands_inheritance() {
        let tmp = std::env::temp_dir().join(format!("elytra_inh_{}", std::process::id()));
        let parent = tmp.join("versions/1.20.1");
        let child = tmp.join("versions/1.20.1-forge-47.1.0");
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(
            parent.join("1.20.1.json"),
            r#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main","assets":"1.20","assetIndex":{"id":"1.20","url":"x","sha1":"y","size":1,"totalSize":1},"downloads":{"client":{"url":"x","sha1":"y","size":1}},"arguments":{"game":[],"jvm":[]},"complianceLevel":0,"minimumLauncherVersion":21,"javaVersion":{"majorVersion":17},"releaseTime":"2021-11-30T00:00:00Z","time":"2021-11-30T00:00:00Z","type":"release","libraries":[{"name":"a:b:1","downloads":{"artifact":{"path":"a/b/1/a-1.jar","url":"x","sha1":"y","size":1}}}]}"#,
        )
        .unwrap();
        std::fs::write(
            child.join("1.20.1-forge-47.1.0.json"),
            r#"{"id":"1.20.1-forge-47.1.0","inheritsFrom":"1.20.1","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","complianceLevel":0,"libraries":[{"name":"net.minecraftforge:forge:47.1.0","downloads":{"artifact":{"path":"net/minecraftforge/forge/47.1.0/forge-47.1.0.jar","url":"x","sha1":"y","size":1}}}]}"#,
        )
        .unwrap();

        let vj = load_version_json_with_inheritance(&tmp, "1.20.1-forge-47.1.0").unwrap();
        assert_eq!(vj.libraries.len(), 2, "父库 + 子库应合并");
        assert_eq!(
            vj.libraries[0].name, "net.minecraftforge:forge:47.1.0",
            "子库应在父库之前（classpath 顺序：子版本的补丁/加载器库先加载，OptiFine 预补丁类覆盖依赖）"
        );
        assert_eq!(
            vj.main_class, "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "子 mainClass 应覆盖父"
        );

        // 父版本缺失：明确报错
        std::fs::remove_dir_all(&parent).unwrap();
        let err = load_version_json_with_inheritance(&tmp, "1.20.1-forge-47.1.0").unwrap_err();
        assert!(err.contains("父版本 JSON"), "应提示父版本缺失: {err}");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// find_version_json：PCL2 版本隔离实例目录混有游戏数据 json（字典序在前），
    /// 应跳过它们取到真正的版本 json（优先同名；否则含 mainClass 的）。
    #[test]
    fn find_version_json_picks_real_version_json() {
        let tmp = std::env::temp_dir().join(format!("elytra_vj_{}", std::process::id()));
        let dir = tmp.join("versions/愚者");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("TrashSlotSaveState.json"), r#"{"save":1}"#).unwrap();
        std::fs::write(dir.join("patchouli_data.json"), r#"{"book":1}"#).unwrap();
        std::fs::write(
            dir.join("愚者.json"),
            r#"{"id":"1.20.1-forge","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","libraries":[]}"#,
        )
        .unwrap();
        let found = find_version_json(&dir, "愚者");
        let name = found
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(name, "愚者.json", "应跳过游戏数据 json 取到版本 json");

        // 标准布局：同名 json 优先
        let dir2 = tmp.join("versions/1.20.1");
        std::fs::create_dir_all(&dir2).unwrap();
        std::fs::write(
            dir2.join("1.20.1.json"),
            r#"{"mainClass":"net.minecraft.client.main.Main"}"#,
        )
        .unwrap();
        std::fs::write(dir2.join("extra.json"), r#"{"x":1}"#).unwrap();
        let found2 = find_version_json(&dir2, "1.20.1");
        assert_eq!(found2.unwrap().file_name().unwrap(), "1.20.1.json");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// pre_launch_check 资源完整性：
    ///  ① 索引 + 对象齐全（大小匹配）→ 通过；
    ///  ② 对象被截断（大小不符）→ 报"缺失/损坏资源文件"；
    ///  ③ 索引本身 sha1 不匹配（损坏）→ 报"资源索引校验失败"。
    /// 覆盖此前"仅检查索引文件存在"导致镜像坏内容/缺失对象漏检的场景。
    #[test]
    fn pre_launch_check_detects_asset_corruption() {
        let tmp = std::env::temp_dir().join(format!("elytra_assetchk_{}", std::process::id()));
        let mc = &tmp;
        let vdir = mc.join("versions/1.20.1");
        std::fs::create_dir_all(&vdir).unwrap();
        // 客户端 JAR（非 0 字节）
        std::fs::write(vdir.join("1.20.1.jar"), b"CLIENTJAR").unwrap();

        // 资源对象：一个 "hello world" 内容，hash 即其 sha1
        let obj_body = b"hello world";
        let obj_hash = sha1_hex(obj_body);
        let obj_size = obj_body.len() as u64;
        let objects_dir = mc.join("assets/objects").join(&obj_hash[..2]);
        std::fs::create_dir_all(&objects_dir).unwrap();
        std::fs::write(objects_dir.join(&obj_hash), obj_body).unwrap();

        // 资源索引 JSON（引用上面的对象），并计算其 sha1
        let index_json =
            format!(r#"{{"objects":{{"minecraft/x":{{"hash":"{obj_hash}","size":{obj_size}}}}}}}"#);
        let index_sha1 = sha1_hex(index_json.as_bytes());
        let index_dir = mc.join("assets/indexes");
        std::fs::create_dir_all(&index_dir).unwrap();
        std::fs::write(index_dir.join("5.json"), &index_json).unwrap();

        // 版本 JSON：assetIndex 指向 id=5、上面算出的 sha1；无 libraries、client 走目录扫描兜底
        let vj: VersionJson = serde_json::from_str(&format!(
            r#"{{
                "id":"1.20.1","mainClass":"net.minecraft.client.main.Main","assets":"5",
                "assetIndex":{{"id":"5","url":"x","sha1":"{index_sha1}","size":{},"totalSize":{obj_size}}},
                "downloads":{{"client":{{"url":"x","sha1":"y","size":1}}}},
                "arguments":{{"game":[],"jvm":[]}},"libraries":[],
                "complianceLevel":0,"minimumLauncherVersion":21,"javaVersion":{{"majorVersion":17}},
                "releaseTime":"2023-01-01T00:00:00Z","time":"2023-01-01T00:00:00Z","type":"release"
            }}"#,
            index_json.len()
        ))
        .unwrap();

        // ① 全齐 → 通过
        assert!(
            pre_launch_check(&vj, mc, "1.20.1").is_ok(),
            "索引 + 对象齐全应通过完整性检查"
        );

        // ② 对象被截断（大小不符）→ 应报缺失/损坏
        std::fs::write(objects_dir.join(&obj_hash), b"short").unwrap();
        let err = pre_launch_check(&vj, mc, "1.20.1").unwrap_err();
        assert!(
            err.contains("缺失/损坏资源文件"),
            "对象大小不符应被捕获: {err}"
        );
        // 恢复对象
        std::fs::write(objects_dir.join(&obj_hash), obj_body).unwrap();

        // ③ 索引损坏（内容改动 → sha1 不匹配）→ 应报索引校验失败
        std::fs::write(index_dir.join("5.json"), format!("{index_json} ")).unwrap();
        let err = pre_launch_check(&vj, mc, "1.20.1").unwrap_err();
        assert!(
            err.contains("资源索引校验失败"),
            "索引 sha1 不匹配应被捕获: {err}"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
