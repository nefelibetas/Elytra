//! 启动命令（薄胶水 + 进程管理）：账号构造、spawn 游戏进程、导出脚本、环境校验。
//! 环境准备在 config/launcher/env.rs，离线皮肤在 config/launcher/skin.rs，命令构建在 config/launcher/args.rs，
//! 平台调节（GPU/内存/优先级）在 config/launcher/platform.rs。

use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

use crate::config::launcher::args::build_launch_command;
use crate::config::launcher::env;
use crate::config::launcher::model::{AuthMode, LauncherConfig};
use crate::config::launcher::platform;
use crate::config::launcher::skin;
use crate::constants;
use crate::downloader::model::VersionJson;
use crate::jdk::model::{JdkConfig, JdkSelection};

// ── 账号构造（启动前置）────────────────────────────

fn build_auth(
    auth_mode: &str,
    username: &str,
    access_token: &str,
    uuid: &str,
    xuid: &str,
) -> AuthMode {
    match auth_mode {
        "online" => AuthMode::Online {
            username: username.to_string(),
            access_token: access_token.to_string(),
            uuid: uuid.to_string(),
            xuid: xuid.to_string(),
        },
        _ => AuthMode::Offline {
            username: username.to_string(),
            uuid: String::new(), // 由 apply_offline_skin 按离线皮肤设置填充
        },
    }
}

/// 正版模式 xuid 缺失时从 Minecraft access token（JWT）解析兜底
/// （XSTS 的 xid 字段部分账号缺失，旧配置也可能未保存 xuid）。
fn fill_missing_xuid(auth: &mut AuthMode) {
    if let AuthMode::Online {
        xuid, access_token, ..
    } = auth
    {
        if xuid.is_empty() {
            *xuid = crate::auth::microsoft::extract_xuid_from_jwt(access_token);
        }
    }
}

/// 三个启动类命令（launch_game / export_launch_script / spawn_game）共享的前置：
/// 构造账号（补 xuid）→ 加载配置与版本 JSON。
#[allow(clippy::type_complexity)]
fn prepare_launch(
    version_dir_name: &str,
    auth_mode: &str,
    username: &str,
    access_token: &str,
    uuid: &str,
    xuid: &str,
) -> Result<(AuthMode, PathBuf, VersionJson, PathBuf, LauncherConfig, u32), String> {
    let mut auth = build_auth(auth_mode, username, access_token, uuid, xuid);
    fill_missing_xuid(&mut auth);
    let (java_path, version_json, mc_dir, launcher_config, java_major) =
        env::load_launch_config(version_dir_name)?;
    Ok((
        auth,
        java_path,
        version_json,
        mc_dir,
        launcher_config,
        java_major,
    ))
}

/// 启动前完整性与 natives 准备（check_launch_env 与 spawn_game 共用）。
fn verify_env(
    version_json: &VersionJson,
    mc_dir: &std::path::Path,
    version_dir_name: &str,
) -> Result<(), String> {
    env::pre_launch_check(version_json, mc_dir, version_dir_name)?;
    let natives_dir = mc_dir.join("natives").join(&version_json.id);
    crate::config::launcher::args::ensure_old_style_natives(
        version_json,
        &mc_dir.join("libraries"),
        &natives_dir,
    )?;
    Ok(())
}

// ── 进程管理 ───────────────────────────────────────

/// 读取子进程输出并追加写入日志文件，直到 EOF 或出错。
fn drain_to_file(reader: &mut impl std::io::Read, path: &std::path::Path, is_stderr: bool) {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok();
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if let Some(ref mut f) = file {
                    if is_stderr {
                        let _ = f.write_all(b"[stderr] ");
                    }
                    let _ = f.write_all(&buf[..n]);
                    let _ = f.write_all(b"\n");
                }
            }
        }
    }
}

/// 执行启动前命令（Windows: cmd /C；其他平台: sh -c）。失败时返回错误。
fn run_pre_launch_command(command: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let status = {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", command]);
        cmd.creation_flags(0x08000000);
        cmd.status().map_err(|e| e.to_string())?
    };
    #[cfg(not(target_os = "windows"))]
    let status = std::process::Command::new("sh")
        .args(["-c", command])
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("退出码 {}", status.code().unwrap_or(-1)))
    }
}

// ── 导出脚本 ───────────────────────────────────────

/// 渲染启动脚本内容（Windows: .bat；macOS/Linux: .sh）。
/// 格式对齐 PCL 导出脚本：chcp 65001 + title + cd 版本目录 + java 命令行 + pause。
fn render_launch_script(
    args: &[String],
    mc_dir: &std::path::Path,
    version_dir_name: &str,
) -> String {
    #[cfg(target_os = "windows")]
    {
        let mut out = String::new();
        out.push_str("chcp 65001>nul\r\n");
        out.push_str("@echo off\r\n");
        out.push_str(&format!("title 启动 - {version_dir_name}\r\n"));
        out.push_str("echo 游戏正在启动，请稍候。\r\n");
        let vdir = mc_dir.join("versions").join(version_dir_name);
        out.push_str(&format!("cd /D \"{}\"\r\n\r\n", vdir.display()));
        for a in args {
            out.push('"');
            out.push_str(&a.replace('%', "%%"));
            out.push_str("\" ");
        }
        out.push_str("\r\necho 游戏已退出。\r\n");
        out.push_str("pause\r\n");
        out
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut out = String::from("#!/usr/bin/env sh\n");
        out.push_str(&format!("# 启动 - {version_dir_name}\n"));
        out.push_str(&format!(
            "cd \"{}\"\n",
            mc_dir.join("versions").join(version_dir_name).display()
        ));
        out.push_str("exec ");
        for a in args {
            out.push('\'');
            out.push_str(&a.replace('\'', "'\\''"));
            out.push_str("' ");
        }
        out.push('\n');
        out
    }
}

// ── Tauri Commands ──────────────────────────────────

/// 启动前环境 + 完整性校验，供前端分阶段显示启动流程。
/// 通过时返回使用的 JDK 信息，失败时返回具体原因。
#[tauri::command]
pub async fn check_launch_env(version_dir_name: String) -> Result<String, String> {
    let (_java_path, version_json, mc_dir, _launcher_config, _java_major) =
        env::load_launch_config(&version_dir_name)?;

    // 游戏完整性 + 旧式 natives 解压准备
    verify_env(&version_json, &mc_dir, &version_dir_name)?;

    let java_env = crate::persist::read::<JdkConfig>(constants::SECT_JDK)
        .ok()
        .and_then(|c| match c.jdk_selection {
            JdkSelection::Manual { path } => c
                .discovered_jdks
                .iter()
                .find(|e| e.path == path)
                .map(|e| e.version.clone()),
            JdkSelection::Auto => None,
        })
        .unwrap_or_else(|| "自动选择".into());
    Ok(format!("环境校验通过（JDK: {}）", java_env))
}

/// 系统内存信息（MB）：总内存 + 当前可用内存。
#[tauri::command]
pub fn get_memory_info() -> (u64, u64) {
    #[cfg(target_os = "windows")]
    unsafe {
        #[repr(C)]
        #[allow(clippy::upper_case_acronyms)]
        struct MEMORYSTATUSEX {
            dw_length: u32,
            dw_memory_load: u32,
            ull_total_phys: u64,
            ull_avail_phys: u64,
            ull_total_page_file: u64,
            ull_avail_page_file: u64,
            ull_total_virtual: u64,
            ull_avail_virtual: u64,
            ull_avail_extended_virtual: u64,
        }
        #[link(name = "Kernel32")]
        extern "system" {
            fn GlobalMemoryStatusEx(lp_buffer: *mut MEMORYSTATUSEX) -> i32;
        }
        let mut st = MEMORYSTATUSEX {
            dw_length: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            dw_memory_load: 0,
            ull_total_phys: 0,
            ull_avail_phys: 0,
            ull_total_page_file: 0,
            ull_avail_page_file: 0,
            ull_total_virtual: 0,
            ull_avail_virtual: 0,
            ull_avail_extended_virtual: 0,
        };
        if GlobalMemoryStatusEx(&mut st) != 0 {
            return (
                st.ull_total_phys / 1024 / 1024,
                st.ull_avail_phys / 1024 / 1024,
            );
        }
        (16384, 8192)
    }
    #[cfg(target_os = "linux")]
    {
        // /proc/meminfo: MemTotal / MemAvailable（kB）——MemAvailable 是真实可用（含可回收缓存）
        let content = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let mut total_kb = 0u64;
        let mut avail_kb = 0u64;
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                total_kb = meminfo_kb(rest);
            } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
                avail_kb = meminfo_kb(rest);
            }
        }
        (total_kb / 1024, avail_kb / 1024)
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        (16384, 8192)
    }
}

/// 解析 /proc/meminfo 行尾的 `123456 kB`。
#[cfg(target_os = "linux")]
fn meminfo_kb(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap_or(0)
}

/// 读取启动选项配置（launcher section）。
#[tauri::command]
pub fn get_launch_options() -> LauncherConfig {
    crate::persist::read::<LauncherConfig>(constants::SECT_LAUNCHER).unwrap_or_default()
}

/// 保存启动选项配置（launcher section）。
#[tauri::command]
pub fn save_launch_options(config: LauncherConfig) -> Result<(), String> {
    crate::persist::save(constants::SECT_LAUNCHER, &config).map_err(|e| e.to_string())
}

/// 读取最后一次使用的离线用户名（与正版隔离）。
#[tauri::command]
pub fn get_last_offline_username() -> String {
    crate::persist::read::<String>("last_offline_username").unwrap_or_default()
}

/// 保存最后一次使用的离线用户名。
#[tauri::command]
pub fn save_last_offline_username(username: String) -> Result<(), String> {
    crate::persist::save("last_offline_username", &username).map_err(|e| e.to_string())
}

/// 导出启动脚本：按 PCL 风格生成平台对应脚本（Windows .bat / macOS/Linux .sh）写入指定路径。
/// 返回写入的文件路径。
#[tauri::command]
pub async fn export_launch_script(
    save_path: String,
    version_dir_name: String,
    auth_mode: String,
    username: String,
    access_token: String,
    uuid: String,
    xuid: String,
) -> Result<String, String> {
    let (auth, java_path, version_json, mc_dir, launcher_config, java_major) = prepare_launch(
        &version_dir_name,
        &auth_mode,
        &username,
        &access_token,
        &uuid,
        &xuid,
    )?;
    let cmd = build_launch_command(
        &java_path,
        &version_json,
        &version_dir_name,
        &mc_dir,
        &launcher_config,
        &auth,
        java_major,
        "",
    );

    let script = render_launch_script(&cmd.args, &mc_dir, &version_dir_name);
    std::fs::write(&save_path, script).map_err(|e| format!("写入脚本失败：{e}"))?;
    Ok(save_path)
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
    let (mut auth, java_path, version_json, mc_dir, launcher_config, java_major) = prepare_launch(
        &version_dir_name,
        &auth_mode,
        &username,
        &access_token,
        &uuid,
        &xuid,
    )?;

    // 离线皮肤配置：同一次启动只读一次（apply_offline_skin 与 build_user_properties 共用）
    let skin_cfg: crate::config::offline_skin::OfflineSkinConfig =
        crate::persist::read(constants::SECT_OFFLINE_SKIN).unwrap_or_default();

    // 离线皮肤（PCL 机制）：UUID 匹配模型（Steve/Alex/Custom）或正版玩家真实 UUID（Player）；
    // Custom 额外构建替换默认纹理的资源包
    skin::apply_offline_skin(
        &mut auth,
        &version_json,
        &mc_dir,
        &version_dir_name,
        &skin_cfg,
    )
    .await;

    // 启动前完整性检测 + 旧式 natives 解压准备
    verify_env(&version_json, &mc_dir, &version_dir_name)?;

    // 离线皮肤：构造 --user-properties 参数（1.19.2 以下版本生效；
    // Random/Steve/Alex 无需注入（游戏默认纹理一致），Player/Custom 注入皮肤）
    let user_properties = skin::build_user_properties(&auth, &skin_cfg).await;

    // 游戏语言：自动设置为中文（1.13+ 语言由 gameDir/options.txt 的 lang 字段控制，
    // -Duser.language 仅对旧版 Locale 机制生效）
    if launcher_config.game_language == "auto_zh_CN" {
        env::apply_game_language(&mc_dir.join("versions").join(&version_dir_name));
    }

    let cmd = build_launch_command(
        &java_path,
        &version_json,
        &version_dir_name,
        &mc_dir,
        &launcher_config,
        &auth,
        java_major,
        &user_properties,
    );

    // 启动前执行命令（非空时运行；失败则中止启动）
    if !launcher_config.pre_launch_cmd.trim().is_empty() {
        run_pre_launch_command(&launcher_config.pre_launch_cmd)
            .map_err(|e| format!("启动前命令执行失败：{e}"))?;
    }

    // 启动前内存优化：清理启动器自身工作集，为游戏腾出内存（仅 Windows）
    if launcher_config.pre_launch_memory_optimize {
        platform::optimize_launcher_memory();
    }

    // 高性能显卡：写入 Windows DirectX 用户 GPU 偏好注册表
    platform::apply_gpu_preference(launcher_config.use_high_performance_gpu);

    // 启动子进程：cwd = 版本隔离的 gameDir，与官方启动器一致——
    // 游戏相对路径行为正确，crash-reports/logs 落在版本目录而非启动器目录
    let game_dir = mc_dir.join("versions").join(&version_dir_name);
    let mut command = std::process::Command::new(&cmd.args[0]);
    command
        .args(&cmd.args[1..])
        .current_dir(&game_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("启动游戏失败: {}", e))?;

    let pid = child.id();

    // 进程优先级（Windows: SetPriorityClass；进程刚启动即设置）
    platform::set_process_priority(pid, launcher_config.process_priority);

    // 捕获子进程 stdout/stderr 写入日志文件（避免 GBK 乱码污染终端，同时保留诊断信息）
    let log_path = mc_dir.join("logs").join("elytra-launch.log");
    if let Some(mut so) = child.stdout.take() {
        let lp = log_path.clone();
        std::thread::spawn(move || drain_to_file(&mut so, &lp, false));
    }
    if let Some(mut se) = child.stderr.take() {
        let lp = log_path.clone();
        std::thread::spawn(move || drain_to_file(&mut se, &lp, true));
    }

    // 后台线程等待进程退出后通知前端
    let app_handle = app.clone();
    let version_label = version_dir_name.clone();
    let crash_dir = game_dir.join("crash-reports");
    std::thread::spawn(move || {
        let status = child.wait();
        // 退出码随事件送出：前端据此区分正常退出与崩溃（非 0 → 错误级提示）
        let exit_code = status.ok().and_then(|s| s.code());
        // 崩溃时定位最新 crash report 文件，随事件交给前端展示
        let crash_report = if !matches!(exit_code, Some(0)) {
            latest_crash_report(&crash_dir)
        } else {
            None
        };
        let _ = app_handle.emit(
            crate::events::GAME_EXITED,
            (pid, version_label, exit_code, crash_report),
        );
    });

    Ok(pid)
}

/// 返回目录下最新的 `crash-*.txt` 文件路径（崩溃报告）。
fn latest_crash_report(dir: &std::path::Path) -> Option<String> {
    use std::time::SystemTime;
    let mut latest: Option<(SystemTime, String)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let modified = path.metadata().ok()?.modified().ok()?;
        if latest.as_ref().is_none_or(|(t, _)| modified > *t) {
            latest = Some((modified, path.display().to_string()));
        }
    }
    latest.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_launch_script_bat() {
        #[cfg(target_os = "windows")]
        {
            let args = vec![
                r"C:\java\bin\java.exe".to_string(),
                "-Xmx2G".to_string(),
                "net.minecraft.client.main.Main".to_string(),
            ];
            let s = render_launch_script(
                &args,
                std::path::Path::new(r"D:\Minecraft\.minecraft"),
                "26.2",
            );
            assert!(s.starts_with("chcp 65001>nul\r\n"));
            assert!(s.contains("title 启动 - 26.2\r\n"));
            assert!(s.contains(r#"cd /D "D:\Minecraft\.minecraft\versions\26.2""#));
            assert!(
                s.contains(r#""C:\java\bin\java.exe" "-Xmx2G" "net.minecraft.client.main.Main" "#)
            );
            assert!(s.ends_with("pause\r\n"));
        }
    }

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

    fn build_cmd(config: &LauncherConfig) -> Vec<String> {
        let vj = minimal_version_json();
        let cmd = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            "1.18.2",
            std::path::Path::new("D:/Minecraft/.minecraft"),
            config,
            &AuthMode::Offline {
                username: "tester".into(),
                uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into(),
            },
            17,
            "",
        );
        cmd.args
    }

    #[test]
    fn test_window_size_default_no_width_args() {
        let args = build_cmd(&LauncherConfig::default());
        assert!(
            !args.iter().any(|a| a == "--width" || a == "--height"),
            "默认窗口大小不应传 --width/--height: {args:?}"
        );
    }

    #[test]
    fn test_window_size_custom_passes_width_height() {
        let mut cfg = LauncherConfig::default();
        cfg.window_size = crate::config::launcher::model::WindowSize::Custom(1920, 1080);
        let args = build_cmd(&cfg);
        assert!(args.contains(&"--width".into()));
        assert!(args.contains(&"1920".into()));
        assert!(args.contains(&"--height".into()));
        assert!(args.contains(&"1080".into()));
    }

    #[test]
    fn test_game_dir_is_version_dir() {
        // 版本隔离（固定行为）：游戏目录 = 版本目录
        let args = build_cmd(&LauncherConfig::default());
        let gi = args.iter().position(|a| a == "--gameDir").unwrap();
        assert_eq!(
            args[gi + 1].replace('\\', "/"),
            "D:/Minecraft/.minecraft/versions/1.18.2"
        );
    }

    #[test]
    fn test_user_properties_appended_new_format() {
        // 新格式（1.17+ json 无 token）：主动追加 camelCase 参数
        let vj = minimal_version_json();
        let cfg = LauncherConfig::default();
        let props = r#"{"textures":[{"name":"textures","value":"AQID","signature":""}]}"#;
        let cmd = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            "1.18.2",
            std::path::Path::new("D:/Minecraft/.minecraft"),
            &cfg,
            &AuthMode::Offline {
                username: "tester".into(),
                uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into(),
            },
            17,
            props,
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
        let cfg = LauncherConfig::default();
        let props = r#"{"textures":[{"name":"textures","value":"AQID","signature":""}]}"#;
        let cmd = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            "1.18.2",
            std::path::Path::new("D:/Minecraft/.minecraft"),
            &cfg,
            &AuthMode::Offline {
                username: "tester".into(),
                uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into(),
            },
            17,
            props,
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
        let mut cfg = LauncherConfig::default();
        cfg.window_title = "我的世界".into();
        let args = build_cmd(&cfg);
        assert!(args.contains(&"-Dminecraft.window.title=我的世界".into()));
        // 空 = 不传
        let args = build_cmd(&LauncherConfig::default());
        assert!(!args
            .iter()
            .any(|a| a.starts_with("-Dminecraft.window.title")));
    }

    #[test]
    fn test_disable_launch_wrapper_main_class() {
        let mut vj = minimal_version_json();
        vj.main_class = "net.minecraft.launchwrapper.Launch".into();
        let cfg = LauncherConfig::default();
        let args = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            "1.8.9",
            std::path::Path::new("D:/Minecraft/.minecraft"),
            &cfg,
            &AuthMode::Offline {
                username: "t".into(),
                uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into(),
            },
            8,
            "",
        )
        .args;
        // 默认：主类为 launchwrapper
        assert!(args.contains(&"net.minecraft.launchwrapper.Launch".to_string()));

        // 禁用后：直接用客户端主类
        let mut cfg = LauncherConfig::default();
        cfg.disable_launch_wrapper = true;
        let args = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            "1.8.9",
            std::path::Path::new("D:/Minecraft/.minecraft"),
            &cfg,
            &AuthMode::Offline {
                username: "t".into(),
                uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into(),
            },
            8,
            "",
        )
        .args;
        assert!(args.contains(&"net.minecraft.client.main.Main".to_string()));
        assert!(!args.contains(&"net.minecraft.launchwrapper.Launch".to_string()));
    }

    #[test]
    fn test_disable_lwjgl_unsafe_agent_filters_javaagent() {
        let mut cfg = LauncherConfig::default();
        cfg.jvm_args = vec![
            "-javaagent:lwjgl-unsafe-agent.jar".into(),
            "-XX:-OmitStackTraceInFastThrow".into(),
        ];
        cfg.disable_lwjgl_unsafe_agent = true;
        let args = build_cmd(&cfg);
        assert!(!args.iter().any(|a| a.contains("lwjgl-unsafe-agent")));
        assert!(args.contains(&"-XX:-OmitStackTraceInFastThrow".to_string()));

        // 默认不禁用：agent 参数保留
        let mut cfg = LauncherConfig::default();
        cfg.jvm_args = vec!["-javaagent:lwjgl-unsafe-agent.jar".into()];
        let args = build_cmd(&cfg);
        assert!(args.iter().any(|a| a.contains("lwjgl-unsafe-agent")));
    }
}
