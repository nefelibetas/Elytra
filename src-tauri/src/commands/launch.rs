//! 启动命令：装配/进程在 launcher/process.rs，命令构建 args.rs，环境 env.rs，皮肤 skin.rs，平台 platform.rs。

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::auth::account;
use crate::events;
use crate::jdk;
use crate::launcher::args::{build_launch_command, LaunchRequest};
use crate::launcher::auto_tasks;
use crate::launcher::compat::{self, JavaCompat};
use crate::launcher::env;
use crate::launcher::platform;
use crate::launcher::process;
use crate::launcher::skin;
use crate::task;
use crate::util;

// ── 启动装配 ──

// ── 进程管理 ──

// ── Tauri Commands ──────────────────────────────────

/// 启动前环境 + 完整性校验；通过时返回所用 JDK 信息，**存在 Java 兼容风险时返回 `@i18n:` 提示**（前端弹提示）。
#[tauri::command]
pub async fn check_launch_env(version_dir_name: String) -> Result<String, String> {
    let (java_path, version_json, mc_dir, launcher_config, java_major) =
        env::load_launch_config(&version_dir_name)?;

    process::ensure_game_ready(&version_json, &mc_dir, &version_dir_name)?;

    let java_env = env::selected_jdk_version(&jdk::read(), &jdk::read_cache())
        .unwrap_or_else(|| "自动选择".into());
    let jdk_label = java_path.display().to_string();
    match compat::check(
        &version_json,
        java_major,
        &jdk_label,
        launcher_config.disable_launch_wrapper,
    ) {
        JavaCompat::Warn(warning) => Ok(warning),
        _ => Ok(keyed_msg!(
            "errors.launch.envOk?java_env={java_env}",
            "环境校验通过（JDK: {java_env}）"
        )),
    }
}

/// 开始一次启动：清空上一次的取消标志与进程登记（前端在启动流程最前调用，此后到达的取消才作用于本次启动）。
#[tauri::command]
pub fn begin_launch() -> u64 {
    process::begin_launch()
}

/// 取消指定代际的启动：尚未拉起进程则阻止启动，已拉起则终止游戏进程。
#[tauri::command]
pub fn cancel_launch(generation: u64) {
    process::request_cancel(generation);
}

/// 启动游戏命令入参（字段名按前端 camelCase 约定）。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnGameRequest {
    pub version_dir_name: String,
    pub auth_mode: String,
    pub username: String,
    pub access_token: String,
    pub uuid: String,
    pub xuid: String,
    pub launch_generation: u64,
}

/// 启动 Minecraft 游戏进程。返回子进程 PID。
#[tauri::command]
pub async fn spawn_game(app: AppHandle, request: SpawnGameRequest) -> Result<u32, String> {
    let SpawnGameRequest {
        version_dir_name,
        auth_mode,
        username,
        access_token,
        uuid,
        xuid,
        launch_generation,
    } = request;
    // 启动流程开始即已收到取消请求（如重启前一次启动）：不再拉起进程
    if process::cancel_requested(launch_generation) {
        bail_key!("errors.task.cancelled", "启动已取消");
    }
    // online 模式改用后端有效 token（临近过期自动刷新；前端缓存 token 约 24h 失效）；离线/第三方沿用传入值。
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

    skin::apply_offline_skin(&mut auth);

    process::ensure_game_ready(&version_json, &mc_dir, &version_dir_name)?;

    // 构造 --user-properties（仅 1.19.2 以下生效；Alex 注入 slim 纹理，Steve 默认）
    let user_properties = skin::build_user_properties(&auth);

    // 语言自动设中文（1.13+ 由 options.txt 的 lang 控制；-Duser.language 仅旧版 Locale 生效）
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

    let game_dir = mc_dir.join("versions").join(&version_dir_name);
    let config_dir = util::config_dir();
    let failures = auto_tasks::runner::run_auto_tasks(
        &app,
        &auto_tasks::ctx::AutoTaskCtx {
            mc_dir: &mc_dir,
            game_dir: &game_dir,
            version_id: &version_dir_name,
            config_dir: &config_dir,
            exit_code: None,
            crash_report: None,
        },
        auto_tasks::model::AutoTaskPhase::PreLaunch,
        &launcher_config.auto_tasks,
    );
    if !failures.is_empty()
        && launcher_config.auto_tasks.pre_on_failure == auto_tasks::model::AutoTaskFailure::Abort
    {
        let detail = failures
            .iter()
            .map(|f| format!("[{}] {}", f.action, f.message))
            .collect::<Vec<_>>()
            .join("\n");
        bail_key!("errors.launch.failed", "启动前自动任务执行失败\n{detail}");
    }

    // 启动前清理启动器工作集腾内存（仅 Windows）
    if launcher_config.pre_launch_memory_optimize {
        platform::optimize_launcher_memory();
    }

    platform::apply_gpu_preference(launcher_config.use_high_performance_gpu);

    // 取消请求可能在安装 JDK / 检查文件 / 启动前自动任务期间到达：此时不再拉起游戏进程
    if process::cancel_requested(launch_generation) {
        bail_key!("errors.task.cancelled", "启动已取消");
    }

    // cwd = 版本隔离的 gameDir（与官方启动器一致）；装配在 launcher/process.rs，此处仅注入事件派发（队列线程，监听线程不等 webview 消费）。
    let log_path = mc_dir.join("logs").join("elytra-launch.log");
    let app_window = app.clone();
    let app_exit = app.clone();
    let label_window = version_dir_name.clone();
    let label_exit = version_dir_name.clone();
    let auto_tasks_exit = launcher_config.auto_tasks.clone();
    let mc_dir_exit = mc_dir.clone();
    let game_dir_exit = game_dir.clone();
    let version_exit = version_dir_name.clone();
    let clear_generation = launch_generation;
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
        move |pid, exit_code, crash_report, diagnosis| {
            // 注销启动 pid：退出后到达的取消不得按此 pid 误杀其他进程
            process::clear_launched(clear_generation, pid);
            // 退出码随事件送出：前端据非 0 判崩溃；diagnosis 为推断出的失败结论（可空）
            task::emit_event(
                &app_exit,
                events::GAME_EXITED,
                (pid, label_exit, exit_code, crash_report.clone(), diagnosis),
            );
            // 崩溃先跑 on_crash，再总是跑 post_exit；spawn_blocking 避免阻塞监听线程（备份可能数十秒）。
            tauri::async_runtime::spawn_blocking(move || {
                let config_dir = util::config_dir();
                let ctx = auto_tasks::ctx::AutoTaskCtx {
                    mc_dir: &mc_dir_exit,
                    game_dir: &game_dir_exit,
                    version_id: &version_exit,
                    config_dir: &config_dir,
                    exit_code,
                    crash_report: crash_report.as_deref(),
                };
                if crash_report.is_some() {
                    auto_tasks::runner::run_auto_tasks(
                        &app_exit,
                        &ctx,
                        auto_tasks::model::AutoTaskPhase::OnCrash,
                        &auto_tasks_exit,
                    );
                }
                auto_tasks::runner::run_auto_tasks(
                    &app_exit,
                    &ctx,
                    auto_tasks::model::AutoTaskPhase::PostExit,
                    &auto_tasks_exit,
                );
            });
        },
    )?;

    // 登记 pid 供取消终止；若取消在 spawn 与登记之间到达，登记时即终止并如实报错
    if process::note_launched(launch_generation, pid) {
        bail_key!("errors.task.cancelled", "启动已取消");
    }

    Ok(pid)
}

/// 读取游戏日志尾部（≤200 行）：优先版本目录 logs/latest.log，空则回退 <game_dir>/logs/elytra-launch.log。
#[tauri::command]
pub async fn get_game_log(game_dir: String, version_name: String) -> String {
    process::read_game_log(std::path::Path::new(&game_dir), &version_name)
}

#[tauri::command]
pub fn open_launcher_log_dir(app: AppHandle) -> Result<(), String> {
    let dir = util::config_dir();
    let path = dir.to_string_lossy().to_string();
    if !dir.exists() {
        bail_key!("errors.dir.missing", "目录不存在\n{path}");
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(common_err_ctx_key!("errors.unknown", "打开文件夹失败"))
}

/// 读取启动器日志尾部（≤200 行）：exe 旁 Elytra/launcher.log；文件不存在/不可读返回空串（不报错）。
#[tauri::command]
pub async fn get_launcher_log() -> String {
    let path = util::config_dir().join("launcher.log");
    // 异步读：同步 std::fs 会阻塞 tokio worker
    tokio::fs::read_to_string(path)
        .await
        .map(|text| util::tail_lines(&text))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::version::model::VersionJson;

    #[test]
    fn spawn_game_request_deserializes_camel_case() {
        let req: SpawnGameRequest = serde_json::from_value(serde_json::json!({
            "versionDirName": "26.3-fabric-0.20.0",
            "authMode": "offline",
            "username": "Player",
            "accessToken": "",
            "uuid": "",
            "xuid": "",
            "launchGeneration": 7
        }))
        .unwrap();
        assert_eq!(req.version_dir_name, "26.3-fabric-0.20.0");
        assert_eq!(req.launch_generation, 7);
    }
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
            std::path::Path::new("java/bin/java.exe"),
            &vj,
            &LaunchRequest {
                version_dir_name: "1.18.2",
                mc_dir: std::path::Path::new("Minecraft/.minecraft"),
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
        let args = build_cmd(&LaunchOptions::default());
        let gi = args.iter().position(|a| a == "--gameDir").unwrap();
        assert_eq!(
            args[gi + 1].replace('\\', "/"),
            "Minecraft/.minecraft/versions/1.18.2"
        );
    }

    #[test]
    fn test_server_direct_args() {
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
            std::path::Path::new("java/bin/java.exe"),
            &vj,
            &LaunchRequest {
                version_dir_name: "1.18.2",
                mc_dir: std::path::Path::new("Minecraft/.minecraft"),
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
        vj.arguments = None; // 走旧格式 minecraftArguments 分支
        vj.minecraft_arguments = Some(
            "--username ${auth_player_name} --version ${version_name} --accessToken ${auth_access_token} --user-properties ${user_properties} --user-type ${user_type}"
                .into(),
        );
        let cfg = LaunchOptions::default();
        let props = r#"{"textures":[{"name":"textures","value":"AQID","signature":""}]}"#;
        let cmd = build_launch_command(
            std::path::Path::new("java/bin/java.exe"),
            &vj,
            &LaunchRequest {
                version_dir_name: "1.18.2",
                mc_dir: std::path::Path::new("Minecraft/.minecraft"),
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
            std::path::Path::new("java/bin/java.exe"),
            &vj,
            &LaunchRequest {
                version_dir_name: "1.8.9",
                mc_dir: std::path::Path::new("Minecraft/.minecraft"),
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
        assert!(args.contains(&"net.minecraft.launchwrapper.Launch".to_string()));

        let cfg = LaunchOptions {
            disable_launch_wrapper: true,
            ..Default::default()
        };
        let args = build_launch_command(
            std::path::Path::new("java/bin/java.exe"),
            &vj,
            &LaunchRequest {
                version_dir_name: "1.8.9",
                mc_dir: std::path::Path::new("Minecraft/.minecraft"),
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

        let cfg = LaunchOptions {
            jvm_args: vec!["-javaagent:lwjgl-unsafe-agent.jar".into()],
            ..Default::default()
        };
        let args = build_cmd(&cfg);
        assert!(args.iter().any(|a| a.contains("lwjgl-unsafe-agent")));
    }

    /// PCL2 实例：json libraries 缺 fabric-loader 时 classpath 扫 libraries 补 fabricmc 与 asm，
    /// asm 多版本取最高（9.10.1 而非 9.2）。
    #[test]
    fn test_pcl_isolated_fabric_appends_loader_libs() {
        let tmp = std::env::temp_dir().join(format!("elytra_pcl_{}", std::process::id()));
        let lib = tmp.join("libraries");
        let mk_jar = |path: &std::path::Path| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"PK").unwrap();
        };
        mk_jar(&lib.join("net/fabricmc/fabric-loader/0.19.3/fabric-loader-0.19.3.jar"));
        mk_jar(&lib.join("net/fabricmc/sponge-mixin/0.17.3/sponge-mixin-0.17.3.jar"));
        mk_jar(&lib.join("org/ow2/asm/asm/9.10.1/asm-9.10.1.jar"));
        mk_jar(&lib.join("org/ow2/asm/asm/9.2/asm-9.2.jar"));

        let mut vj = minimal_version_json();
        vj.main_class = "net.fabricmc.loader.impl.launch.knot.KnotClient".into();
        let cfg = LaunchOptions::default();
        let args = build_launch_command(
            std::path::Path::new("java/bin/java.exe"),
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

        let args = build_cmd(&LaunchOptions::default());
        assert!(
            !args.iter().any(|a| a.starts_with("-Xms")),
            "默认不应有 -Xms: {args:?}"
        );
    }
}
