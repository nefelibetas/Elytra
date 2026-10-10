//! 启动命令（薄胶水）：装配与进程管理在 launcher/process.rs、命令构建 launcher/args.rs、
//! 环境准备 launcher/env.rs、离线皮肤 launcher/skin.rs、平台调节 launcher/platform.rs；
//! 启动脚本导出在 commands/launch_script.rs（渲染在 launcher/script.rs）、
//! 启动选项在 commands/launch_options.rs、离线账号历史在 commands/offline_account.rs。

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::auth::account;
use crate::events;
use crate::jdk;
use crate::launcher::args::{build_launch_command, LaunchRequest};
use crate::launcher::env;
use crate::launcher::platform;
use crate::launcher::process;
use crate::launcher::skin;
use crate::task;
use crate::util;

// ── 启动装配（build_auth / fill_missing_xuid / prepare_launch 已下沉 launcher/process.rs）──

// ── 进程管理（实现下沉 launcher/process.rs）────────────────────

// ── Tauri Commands ──────────────────────────────────

/// 启动前环境 + 完整性校验，供前端分阶段显示启动流程。
/// 通过时返回使用的 JDK 信息，失败时返回具体原因。
#[tauri::command]
pub async fn check_launch_env(version_dir_name: String) -> Result<String, String> {
    let (_java_path, version_json, mc_dir, _launcher_config, _java_major) =
        env::load_launch_config(&version_dir_name)?;

    // 游戏完整性 + 旧式 natives 解压准备
    process::ensure_game_ready(&version_json, &mc_dir, &version_dir_name)?;

    let java_env = env::selected_jdk_version(&jdk::read(), &jdk::read_cache())
        .unwrap_or_else(|| "自动选择".into());
    Ok(format!("环境校验通过（JDK: {}）", java_env))
}

/// 启动 Minecraft 游戏进程。返回子进程 PID。
#[tauri::command]
pub async fn spawn_game(
    app: AppHandle,
    version_dir_name: String,
    auth_mode: String,
    username: String,
    access_token: String,
    uuid: String,
    xuid: String,
) -> Result<u32, String> {
    // 正版（online）模式：改用后端取的有效 token（临近过期自动静默刷新）——
    // 前端缓存的是登录当时的 token，MC token 仅约 24h 有效，过期后启动会「会话无效」。
    // 离线（空串）与第三方（authlib-injector 自有 token）沿用调用方传入值。
    let access_token = if auth_mode == "online" {
        account::valid_access_token().await?
    } else {
        access_token
    };
    let (mut auth, java_path, version_json, mc_dir, launcher_config, java_major) =
        process::prepare_launch(
            &version_dir_name,
            &auth_mode,
            &username,
            &access_token,
            &uuid,
            &xuid,
        )
        .await?;

    // 离线皮肤（PCL 机制）：固定随机（用户名哈希决定 Steve/Alex），UUID 匹配模型
    skin::apply_offline_skin(&mut auth);

    // 启动前完整性检测 + 旧式 natives 解压准备
    process::ensure_game_ready(&version_json, &mc_dir, &version_dir_name)?;

    // 离线皮肤：构造 --user-properties 参数（1.19.2 以下版本生效；Alex 注入 slim 纹理，Steve 默认）
    let user_properties = skin::build_user_properties(&auth);

    // 游戏语言：自动设置为中文（1.13+ 语言由 gameDir/options.txt 的 lang 字段控制，
    // -Duser.language 仅对旧版 Locale 机制生效）
    if launcher_config.game_language == "auto_zh_CN" {
        env::apply_game_language(&mc_dir.join("versions").join(&version_dir_name));
    }

    let cmd = build_launch_command(
        &java_path,
        &version_json,
        &LaunchRequest {
            version_dir_name: &version_dir_name,
            mc_dir: &mc_dir,
            config: &launcher_config,
            auth: &auth,
            java_major_version: java_major,
            user_properties: &user_properties,
        },
    );

    // 启动前执行命令（非空时运行；失败则中止启动）
    if !launcher_config.pre_launch_cmd.trim().is_empty() {
        process::run_pre_launch_command(&launcher_config.pre_launch_cmd)
            .map_err(|e| format!("启动前命令执行失败\n{e}"))?;
    }

    // 启动前内存优化：清理启动器自身工作集，为游戏腾出内存（仅 Windows）
    if launcher_config.pre_launch_memory_optimize {
        platform::optimize_launcher_memory();
    }

    // 高性能显卡：写入 Windows DirectX 用户 GPU 偏好注册表
    platform::apply_gpu_preference(launcher_config.use_high_performance_gpu);

    // 启动子进程：cwd = 版本隔离的 gameDir，与官方启动器一致——
    // 游戏相对路径行为正确，crash-reports/logs 落在版本目录而非启动器目录。
    // 装配（cwd/stdio/优先级/日志排空/后台监听）全在 launcher/process.rs（域层），
    // 此处只注入事件派发——队列线程派发（与 task-progress 同机制），
    // 游戏监听线程不因 webview 未消费而阻塞。
    let game_dir = mc_dir.join("versions").join(&version_dir_name);
    let log_path = mc_dir.join("logs").join("elytra-launch.log");
    let app_window = app.clone();
    let app_exit = app.clone();
    let label_window = version_dir_name.clone();
    let label_exit = version_dir_name.clone();
    let pid = process::spawn_and_watch(
        process::SpawnRequest {
            args: &cmd.args,
            game_dir: &game_dir,
            log_path: &log_path,
            process_priority: launcher_config.process_priority,
        },
        move |pid| {
            task::emit_event(
                &app_window,
                events::GAME_WINDOW_VISIBLE,
                (pid, label_window.clone()),
            );
        },
        move |pid, exit_code, crash_report| {
            // 退出码随事件送出：前端据此区分正常退出与崩溃（非 0 → 错误级提示）
            task::emit_event(
                &app_exit,
                events::GAME_EXITED,
                (pid, label_exit, exit_code, crash_report),
            );
        },
    )?;

    Ok(pid)
}

/// 读取游戏日志尾部（最多 200 行）：优先版本目录 logs/latest.log，为空回退
/// 启动器捕获日志 <game_dir>/logs/elytra-launch.log；读取逻辑在 launcher/process.rs。
#[tauri::command]
pub async fn get_game_log(game_dir: String, version_name: String) -> String {
    process::read_game_log(std::path::Path::new(&game_dir), &version_name)
}

/// 打开启动器日志所在目录（exe 旁 Elytra/，launcher.log 所在地）。
#[tauri::command]
pub fn open_launcher_log_dir(app: AppHandle) -> Result<(), String> {
    let dir = util::config_dir();
    let path = dir.to_string_lossy().to_string();
    // 与 gamedirs::open_folder 同机制，但直接调 plugin 而非调另一个命令（命令间不互调）
    if !dir.exists() {
        return Err(format!("目录不存在\n{path}"));
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| format!("打开文件夹失败\n{e}"))
}

/// 读取启动器日志尾部（最多 200 行）：exe 旁 Elytra/launcher.log（stdout/stderr 重定向落盘）。
/// 文件不存在/不可读返回空字符串（不报错）。
#[tauri::command]
pub async fn get_launcher_log() -> String {
    let path = util::config_dir().join("launcher.log");
    // 异步读：命令是 async，同步 std::fs 会阻塞 tokio worker（文件不大但没必要）
    tokio::fs::read_to_string(path)
        .await
        .map(|text| util::tail_lines(&text))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::version::model::VersionJson;
    use crate::launcher::model::{AuthMode, LaunchOptions, MemoryMode, WindowSize};

    /// 构造一个最小可用的版本 JSON（旧格式 minecraftArguments，保底参数路径）。
    fn minimal_version_json() -> VersionJson {
        serde_json::from_str(
            r#"{
                "id": "1.18.2",
                "mainClass": "net.minecraft.client.main.Main",
                "assets": "1.18",
                "assetIndex": { "id": "1.18", "url": "x", "sha1": "y", "size": 1, "totalSize": 1 },
                "downloads": { "client": { "url": "x", "sha1": "y", "size": 1 } },
                "arguments": {
                  "game": [
                    "--gameDir", "${game_directory}",
                    "--assetsDir", "${assets_root}",
                    "--assetIndex", "${assets_index_name}",
                    { "rules": [ { "action": "allow", "features": { "has_custom_resolution": true } } ],
                      "value": [ "--width", "${resolution_width}", "--height", "${resolution_height}" ] }
                  ],
                  "jvm": []
                },
                "libraries": [],
                "complianceLevel": 0,
                "minimumLauncherVersion": 21,
                "javaVersion": { "majorVersion": 17 },
                "releaseTime": "2021-11-30T00:00:00Z",
                "time": "2021-11-30T00:00:00Z",
                "type": "release"
            }"#,
        )
        .unwrap()
    }

    fn build_cmd(config: &LaunchOptions) -> Vec<String> {
        let vj = minimal_version_json();
        let cmd = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            &LaunchRequest {
                version_dir_name: "1.18.2",
                mc_dir: std::path::Path::new("D:/Minecraft/.minecraft"),
                config,
                auth: &AuthMode::Offline {
                    username: "tester".into(),
                    uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into(),
                },
                java_major_version: 17,
                user_properties: "",
            },
        );
        cmd.args
    }

    #[test]
    fn test_window_size_default_no_width_args() {
        let args = build_cmd(&LaunchOptions::default());
        assert!(
            !args.iter().any(|a| a == "--width" || a == "--height"),
            "默认窗口大小不应传 --width/--height: {args:?}"
        );
    }

    #[test]
    fn test_window_size_custom_passes_width_height() {
        let cfg = LaunchOptions {
            window_size: WindowSize::Custom(1920, 1080),
            ..Default::default()
        };
        let args = build_cmd(&cfg);
        assert!(args.contains(&"--width".into()));
        assert!(args.contains(&"1920".into()));
        assert!(args.contains(&"--height".into()));
        assert!(args.contains(&"1080".into()));
    }

    #[test]
    fn test_game_dir_is_version_dir() {
        // 版本隔离（固定行为）：游戏目录 = 版本目录
        let args = build_cmd(&LaunchOptions::default());
        let gi = args.iter().position(|a| a == "--gameDir").unwrap();
        assert_eq!(
            args[gi + 1].replace('\\', "/"),
            "D:/Minecraft/.minecraft/versions/1.18.2"
        );
    }

    #[test]
    fn test_server_direct_args() {
        // 服务器直连：--server/--port 追加到游戏参数末尾；未配置时不出现
        let cfg = LaunchOptions {
            server: Some("1.2.3.4".into()),
            port: Some(25565),
            ..Default::default()
        };
        let args = build_cmd(&cfg);
        assert!(args.contains(&"--server".into()));
        assert!(args.contains(&"1.2.3.4".into()));
        assert!(args.contains(&"--port".into()));
        assert!(args.contains(&"25565".into()));

        let args = build_cmd(&LaunchOptions::default());
        assert!(
            !args.iter().any(|a| a == "--server" || a == "--port"),
            "未配置服务器直连不应传 --server/--port: {args:?}"
        );
    }

    #[test]
    fn test_user_properties_appended_new_format() {
        // 新格式（1.17+ json 无 token）：主动追加 camelCase 参数
        let vj = minimal_version_json();
        let cfg = LaunchOptions::default();
        let props = r#"{"textures":[{"name":"textures","value":"AQID","signature":""}]}"#;
        let cmd = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            &LaunchRequest {
                version_dir_name: "1.18.2",
                mc_dir: std::path::Path::new("D:/Minecraft/.minecraft"),
                config: &cfg,
                auth: &AuthMode::Offline {
                    username: "tester".into(),
                    uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into(),
                },
                java_major_version: 17,
                user_properties: props,
            },
        );
        let joined = cmd.args.join(" ");
        assert!(
            joined.contains("--userProperties"),
            "应追加 --userProperties: {joined}"
        );
        assert!(
            joined.contains("--profileProperties"),
            "应追加 --profileProperties: {joined}"
        );
        assert!(joined.contains(props), "值应注入: {joined}");
        assert!(
            !joined.contains("--user-properties"),
            "不应注入连字符版: {joined}"
        );
    }

    #[test]
    fn test_user_properties_substituted() {
        // 旧式 minecraftArguments（1.19.2 以下）应注入 --user-properties 与 legacy user_type
        let mut vj = minimal_version_json();
        vj.arguments.game = None; // 走旧格式 minecraftArguments 分支
        vj.minecraft_arguments = Some(
            "--username ${auth_player_name} --version ${version_name} --accessToken ${auth_access_token} --user-properties ${user_properties} --user-type ${user_type}"
                .into(),
        );
        let cfg = LaunchOptions::default();
        let props = r#"{"textures":[{"name":"textures","value":"AQID","signature":""}]}"#;
        let cmd = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            &LaunchRequest {
                version_dir_name: "1.18.2",
                mc_dir: std::path::Path::new("D:/Minecraft/.minecraft"),
                config: &cfg,
                auth: &AuthMode::Offline {
                    username: "tester".into(),
                    uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into(),
                },
                java_major_version: 17,
                user_properties: props,
            },
        );
        let joined = cmd.args.join(" ");
        assert!(joined.contains(props), "应注入 user properties: {joined}");
        assert!(
            !joined.contains("${user_properties}"),
            "模板残留未替换: {joined}"
        );
        assert!(
            joined.contains("--user-type legacy"),
            "离线应为 legacy: {joined}"
        );
        // 旧格式模板应按空白拆分为独立 argv（否则 joptsimple 报 missing accessToken/version）
        assert!(
            cmd.args.contains(&"--accessToken".into()),
            "--accessToken 应为独立参数: {joined}"
        );
        assert!(
            cmd.args.contains(&"--version".into()),
            "--version 应为独立参数: {joined}"
        );
    }

    #[test]
    fn test_window_title_prop() {
        let cfg = LaunchOptions {
            window_title: "我的世界".into(),
            ..Default::default()
        };
        let args = build_cmd(&cfg);
        assert!(args.contains(&"-Dminecraft.window.title=我的世界".into()));
        // 空 = 不传
        let args = build_cmd(&LaunchOptions::default());
        assert!(!args
            .iter()
            .any(|a| a.starts_with("-Dminecraft.window.title")));
    }

    #[test]
    fn test_disable_launch_wrapper_main_class() {
        let mut vj = minimal_version_json();
        vj.main_class = "net.minecraft.launchwrapper.Launch".into();
        let cfg = LaunchOptions::default();
        let args = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            &LaunchRequest {
                version_dir_name: "1.8.9",
                mc_dir: std::path::Path::new("D:/Minecraft/.minecraft"),
                config: &cfg,
                auth: &AuthMode::Offline {
                    username: "t".into(),
                    uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into(),
                },
                java_major_version: 8,
                user_properties: "",
            },
        )
        .args;
        // 默认：主类为 launchwrapper
        assert!(args.contains(&"net.minecraft.launchwrapper.Launch".to_string()));

        // 禁用后：直接用客户端主类
        let cfg = LaunchOptions {
            disable_launch_wrapper: true,
            ..Default::default()
        };
        let args = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            &LaunchRequest {
                version_dir_name: "1.8.9",
                mc_dir: std::path::Path::new("D:/Minecraft/.minecraft"),
                config: &cfg,
                auth: &AuthMode::Offline {
                    username: "t".into(),
                    uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into(),
                },
                java_major_version: 8,
                user_properties: "",
            },
        )
        .args;
        assert!(args.contains(&"net.minecraft.client.main.Main".to_string()));
        assert!(!args.contains(&"net.minecraft.launchwrapper.Launch".to_string()));
    }

    #[test]
    fn test_disable_lwjgl_unsafe_agent_filters_javaagent() {
        let cfg = LaunchOptions {
            jvm_args: vec![
                "-javaagent:lwjgl-unsafe-agent.jar".into(),
                "-XX:-OmitStackTraceInFastThrow".into(),
            ],
            disable_lwjgl_unsafe_agent: true,
            ..Default::default()
        };
        let args = build_cmd(&cfg);
        assert!(!args.iter().any(|a| a.contains("lwjgl-unsafe-agent")));
        assert!(args.contains(&"-XX:-OmitStackTraceInFastThrow".to_string()));

        // 默认不禁用：agent 参数保留
        let cfg = LaunchOptions {
            jvm_args: vec!["-javaagent:lwjgl-unsafe-agent.jar".into()],
            ..Default::default()
        };
        let args = build_cmd(&cfg);
        assert!(args.iter().any(|a| a.contains("lwjgl-unsafe-agent")));
    }

    /// PCL2 版本隔离实例兜底：json 的 libraries 无 fabric-loader（PCL2 组件机制），
    /// mainClass 是 KnotClient——classpath 应扫描 libraries 目录补 fabricmc 与 asm，
    /// asm 多版本时取最高版本（9.10.1 而非 9.2）。
    #[test]
    fn test_pcl_isolated_fabric_appends_loader_libs() {
        let tmp = std::env::temp_dir().join(format!("elytra_pcl_{}", std::process::id()));
        let lib = tmp.join("libraries");
        let mk_jar = |path: &std::path::Path| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"PK").unwrap(); // 空 zip 头即可，build_classpath 不读内容
        };
        mk_jar(&lib.join("net/fabricmc/fabric-loader/0.19.3/fabric-loader-0.19.3.jar"));
        mk_jar(&lib.join("net/fabricmc/sponge-mixin/0.17.3/sponge-mixin-0.17.3.jar"));
        mk_jar(&lib.join("org/ow2/asm/asm/9.10.1/asm-9.10.1.jar"));
        mk_jar(&lib.join("org/ow2/asm/asm/9.2/asm-9.2.jar"));

        let mut vj = minimal_version_json();
        vj.main_class = "net.fabricmc.loader.impl.launch.knot.KnotClient".into();
        let cfg = LaunchOptions::default();
        let args = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            &LaunchRequest {
                version_dir_name: "1.18.2",
                mc_dir: &tmp,
                config: &cfg,
                auth: &AuthMode::Offline {
                    username: "t".into(),
                    uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into(),
                },
                java_major_version: 17,
                user_properties: "",
            },
        )
        .args;
        let cp_idx = args
            .iter()
            .position(|a| a == "-cp")
            .expect("-cp 参数应存在");
        let cp = &args[cp_idx + 1];
        assert!(
            cp.contains("fabric-loader-0.19.3.jar") && cp.contains("sponge-mixin-0.17.3.jar"),
            "classpath 应补 fabricmc 组件: {cp}"
        );
        assert!(
            cp.contains("asm-9.10.1.jar") && !cp.contains("asm-9.2.jar"),
            "asm 多版本应取最高版本: {cp}"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_min_memory_xms_injection() {
        // Custom + min_memory → 注入 -Xms（clamp 到 -Xmx 内）
        let cfg = LaunchOptions {
            memory: MemoryMode::Custom(4096),
            min_memory: Some(2048),
            ..Default::default()
        };
        let args = build_cmd(&cfg);
        assert!(
            args.iter().any(|a| a == "-Xmx4096m"),
            "Custom 应有 -Xmx4096m: {args:?}"
        );
        assert!(
            args.iter().any(|a| a == "-Xms2048m"),
            "Custom+min 应有 -Xms2048m: {args:?}"
        );

        // min_memory > Xmx → clamp 到 Xmx
        let cfg = LaunchOptions {
            memory: MemoryMode::Custom(1024),
            min_memory: Some(8192),
            ..Default::default()
        };
        let args = build_cmd(&cfg);
        assert!(
            args.iter().any(|a| a == "-Xms1024m"),
            "超上限应 clamp 到 -Xmx: {args:?}"
        );

        // Auto 模式忽略 min_memory（-Xmx 动态推测，固定 -Xms 可能超上限）
        let cfg = LaunchOptions {
            memory: MemoryMode::Auto,
            min_memory: Some(2048),
            ..Default::default()
        };
        let args = build_cmd(&cfg);
        assert!(
            !args.iter().any(|a| a.starts_with("-Xms")),
            "Auto 模式不应注入 -Xms: {args:?}"
        );

        // 默认（无 min_memory）不注入 -Xms
        let args = build_cmd(&LaunchOptions::default());
        assert!(
            !args.iter().any(|a| a.starts_with("-Xms")),
            "默认不应有 -Xms: {args:?}"
        );
    }
}
