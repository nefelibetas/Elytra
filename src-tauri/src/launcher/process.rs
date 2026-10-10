//! 游戏进程生命周期（launcher 域，原 commands/launch.rs 内嵌逻辑下沉）：
//! 子进程 spawn（Windows Job Object 脱钩重试）、stdout/stderr 落盘、可见窗口轮询、
//! 退出监听与崩溃报告定位。命令层（commands/launch.rs）只做编排与事件派发，
//! 本模块不依赖 tauri：退出/窗口回调由调用方注入。

#[cfg(target_os = "windows")]
use std::ffi::c_void;
use std::io::Write as _;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::auth::external::{ensure_jar, read};
use crate::auth::microsoft::extract_xuid_from_jwt;
use crate::game::version::model::VersionJson;
use crate::launcher::args::{ensure_old_style_natives, mc_version_of};
use crate::launcher::env::{load_launch_config, pre_launch_check};
use crate::launcher::model::{AuthMode, LaunchOptions, ProcessPriority};

/// 启动游戏子进程（cwd/stdio 由调用方在 command 上预设）。
/// Windows：游戏进程应与启动器生命周期解耦，否则关闭启动器（或其终端）会连游戏一起关闭：
/// 游戏作为启动器的子进程，会自动加入启动器所在的 Job Object
/// （如 VS Code 终端建的 KILL_ON_JOB_CLOSE job），job 关闭时游戏被连带杀死。
/// 用 CREATE_BREAKAWAY_FROM_JOB 让游戏脱离该 job；
/// 但宿主 job 不允许 breakaway 时，带该标志的 CreateProcess 会直接失败
/// ERROR_ACCESS_DENIED(5)（实测复现），此时回退为不带该标志重试——
/// 游戏留在 job 中（关终端仍会被连带关闭，Windows 硬限制），但能正常启动。
#[cfg(target_os = "windows")]
pub fn spawn_game_child(
    command: &mut std::process::Command,
) -> Result<std::process::Child, String> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let mut flags = CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB;
    loop {
        command.creation_flags(flags);
        match command.spawn() {
            Ok(c) => return Ok(c),
            Err(e) if e.raw_os_error() == Some(5) && flags & CREATE_BREAKAWAY_FROM_JOB != 0 => {
                flags &= !CREATE_BREAKAWAY_FROM_JOB; // 最多重试一次
            }
            Err(e) => bail!("启动游戏失败\n{e}"),
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn spawn_game_child(
    command: &mut std::process::Command,
) -> Result<std::process::Child, String> {
    command.spawn().map_err(common_err_ctx!("启动游戏失败"))
}

/// 读取子进程输出并追加写入日志文件，直到 EOF 或出错。
pub fn drain_output(reader: &mut impl std::io::Read, path: &Path, is_stderr: bool) {
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
                if let Some(f) = &mut file {
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
pub fn run_pre_launch_command(command: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let status = {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", command]);
        cmd.creation_flags(0x08000000);
        cmd.status().map_err(common_err_ctx!("启动进程失败"))?
    };
    #[cfg(not(target_os = "windows"))]
    let status = std::process::Command::new("sh")
        .args(["-c", command])
        .status()
        .map_err(common_err_ctx!("启动进程失败"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("退出码 {}", status.code().unwrap_or(-1)))
    }
}

/// 启动前完整性与 natives 准备（check_launch_env 与 spawn_game 共用）。
pub fn ensure_game_ready(
    version_json: &VersionJson,
    mc_dir: &Path,
    version_dir_name: &str,
) -> Result<(), String> {
    pre_launch_check(version_json, mc_dir, version_dir_name)?;
    let natives_dir = mc_dir
        .join("natives")
        .join(mc_version_of(version_dir_name));
    ensure_old_style_natives(
        version_json,
        &mc_dir.join("libraries"),
        &natives_dir,
    )?;
    Ok(())
}

/// 该进程是否存在可见顶层窗口（Minecraft 主窗口）。
/// Windows: EnumWindows 按 pid 匹配 + IsWindowVisible 判定；
/// 其他平台无此能力，直接视为可见（保持旧行为，前端另有 90s 超时兜底）。
#[cfg(target_os = "windows")]
fn process_has_visible_window(pid: u32) -> bool {
    extern "system" {
        fn EnumWindows(
            lp_enum_func: unsafe extern "system" fn(*mut c_void, isize) -> i32,
            l_param: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(h_wnd: *mut c_void, lpdw_process_id: *mut u32) -> u32;
        fn IsWindowVisible(h_wnd: *mut c_void) -> i32;
    }

    #[repr(C)]
    struct Search {
        pid: u32,
        found: bool,
    }

    // SAFETY: 作为 EnumWindows 回调，lparam 恒为调用处传入的 &mut search（见下方），
    // 该指针在枚举期间保持有效且独占，回调内按 *mut Search 还原并解引用因此安全。
    unsafe extern "system" fn enum_proc(hwnd: *mut c_void, lparam: isize) -> i32 {
        let s = &mut *(lparam as *mut Search);
        let mut wpid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut wpid);
        if wpid == s.pid && IsWindowVisible(hwnd) != 0 {
            s.found = true;
            0 // 停止枚举
        } else {
            1 // 继续
        }
    }

    let mut search = Search { pid, found: false };
    // SAFETY: enum_proc 是签名匹配的 "system" 回调，search 在本栈帧存活至调用结束，
    // 其地址作为 lparam 传入；EnumWindows 同步枚举完才返回，期间指针保持有效。
    unsafe {
        EnumWindows(enum_proc, &mut search as *mut Search as isize);
    }
    search.found
}

#[cfg(not(target_os = "windows"))]
fn process_has_visible_window(_pid: u32) -> bool {
    true
}

/// 返回目录下最新的 `crash-*.txt` 文件路径（崩溃报告）。
fn latest_crash_report(dir: &Path) -> Option<String> {
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

/// 游戏窗口首次可见的轮询间隔（毫秒）。
const WINDOW_POLL_MS: u64 = 300;

/// `spawn_and_watch` 的装配参数（参数超过 7 个时用上下文结构体，避免 too_many_arguments）。
pub struct SpawnRequest<'a> {
    /// 完整启动命令 argv（`args[0]` = java 可执行文件）
    pub args: &'a [String],
    /// 版本隔离 gameDir：子进程 cwd（游戏相对路径正确，crash-reports/logs 落在此处）
    pub game_dir: &'a Path,
    /// 子进程 stdout/stderr 落盘路径
    pub log_path: &'a Path,
    pub process_priority: ProcessPriority,
}

/// 启动游戏子进程并完成全部接线：cwd/stdio 装配 → spawn（Job Object 脱钩）→
/// 进程优先级 → stdout/stderr 排空线程 → 后台监听（窗口可见 + 退出）。
/// 两个事件回调由调用方注入（本域不依赖 tauri 与事件常量），回调首参为子进程 pid，
/// 使调用方无需在 spawn 前就持有它。返回 pid。
pub fn spawn_and_watch<W, E>(
    req: SpawnRequest<'_>,
    on_window_visible: W,
    on_exit: E,
) -> Result<u32, String>
where
    W: FnMut(u32) + Send + 'static,
    E: FnOnce(u32, Option<i32>, Option<String>) + Send + 'static,
{
    let SpawnRequest {
        args,
        game_dir,
        log_path,
        process_priority,
    } = req;
    let Some((program, rest)) = args.split_first() else {
        return Err("启动命令为空\n无法启动游戏".into());
    };
    let mut command = std::process::Command::new(program);
    command
        .args(rest)
        .current_dir(game_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());
    let mut child = spawn_game_child(&mut command)?;
    let pid = child.id();
    // 进程刚启动即设置优先级（Windows: SetPriorityClass）
    crate::launcher::platform::set_process_priority(pid, process_priority);
    // 捕获子进程 stdout/stderr 写入日志文件（避免 GBK 乱码污染终端，同时保留诊断信息）
    if let Some(mut so) = child.stdout.take() {
        let lp = log_path.to_path_buf();
        std::thread::spawn(move || drain_output(&mut so, &lp, false));
    }
    if let Some(mut se) = child.stderr.take() {
        let lp = log_path.to_path_buf();
        std::thread::spawn(move || drain_output(&mut se, &lp, true));
    }
    let crash_dir = game_dir.join("crash-reports");
    let mut on_window_visible = on_window_visible;
    watch_game_process(
        child,
        pid,
        crash_dir,
        WINDOW_POLL_MS,
        move || on_window_visible(pid),
        move |exit_code, crash_report| on_exit(pid, exit_code, crash_report),
    );
    Ok(pid)
}

/// 合一监听循环（后台线程）：try_wait 非阻塞查退出 + 可见窗口轮询；
/// 进程退出后回调 on_exit（含崩溃报告定位），窗口首次可见回调 on_window_visible。
pub fn watch_game_process<W, E>(
    mut child: std::process::Child,
    pid: u32,
    crash_dir: PathBuf,
    window_poll_ms: u64,
    mut on_window_visible: W,
    on_exit: E,
) where
    W: FnMut() + Send + 'static,
    E: FnOnce(Option<i32>, Option<String>) + Send + 'static,
{
    std::thread::spawn(move || {
        let mut window_notified = false;
        loop {
            // 进程已退出：回调退出事件（含崩溃报告）并结束
            match child.try_wait() {
                Ok(Some(status)) => {
                    // 退出码随回调送出：调用方据此区分正常退出与崩溃（非 0 → 错误级提示）
                    let exit_code = status.code();
                    // 崩溃时定位最新 crash report 文件
                    let crash_report = if !matches!(exit_code, Some(0)) {
                        latest_crash_report(&crash_dir)
                    } else {
                        None
                    };
                    on_exit(exit_code, crash_report);
                    return;
                }
                Ok(None) => {}
                Err(_) => {
                    // try_wait 失败（状态查询异常）：按未知退出处理，避免线程永久挂起
                    on_exit(None, None);
                    return;
                }
            }
            // 主窗口尚未可见：轮询一次（Windows EnumWindows 按 pid 匹配可见顶层窗口）
            if !window_notified && process_has_visible_window(pid) {
                window_notified = true;
                on_window_visible();
            }
            std::thread::sleep(std::time::Duration::from_millis(window_poll_ms));
        }
    });
}

/// 读取游戏日志尾部（最多 200 行）：优先版本目录 logs/latest.log（游戏自身日志，
/// 游戏 cwd 是版本目录，latest.log/crash-reports 都落在那里），
/// 为空/不存在时回退启动器捕获日志 <game_dir>/logs/elytra-launch.log；均不可读返回空串。
pub fn read_game_log(mc_dir: &Path, version_name: &str) -> String {
    let version_logs = mc_dir.join("versions").join(version_name).join("logs");
    let game_log = std::fs::read_to_string(version_logs.join("latest.log")).unwrap_or_default();
    if !game_log.trim().is_empty() {
        return crate::util::tail_lines(&game_log);
    }
    std::fs::read_to_string(mc_dir.join("logs").join("elytra-launch.log"))
        .map(|text| crate::util::tail_lines(&text))
        .unwrap_or_default()
}

// ── 启动装配（spawn_game / export_launch_script 共用前置）──

/// 按启动模式构造认证（online/external/offline），第三方账号的 api_url 从当前账号读。
pub fn build_auth(
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
        "external" => {
            // 第三方登录：账号从 secrets 读（uuid/name/token/api_url 一致），
            // 前端传 username/uuid/token；api_url 从当前第三方账号取
            let api_url = read()
                .map(|a| a.api_url)
                .unwrap_or_default();
            AuthMode::External {
                username: username.to_string(),
                access_token: access_token.to_string(),
                uuid: uuid.to_string(),
                api_url,
            }
        }
        _ => AuthMode::Offline {
            username: username.to_string(),
            uuid: String::new(), // 由 apply_offline_skin 按离线皮肤设置填充
        },
    }
}

/// 正版模式 xuid 缺失时从 Minecraft access token（JWT）解析兜底
/// （XSTS 的 xid 字段部分账号缺失，旧配置也可能未保存 xuid）。
pub fn fill_missing_xuid(auth: &mut AuthMode) {
    if let AuthMode::Online {
        xuid, access_token, ..
    } = auth
    {
        if xuid.is_empty() {
            *xuid = extract_xuid_from_jwt(access_token);
        }
    }
}

/// 启动类命令（launch_game / export_launch_script / spawn_game）共享的前置：
/// 构造账号（补 xuid）→ 加载配置与版本 JSON。
#[allow(clippy::type_complexity)]
pub async fn prepare_launch(
    version_dir_name: &str,
    auth_mode: &str,
    username: &str,
    access_token: &str,
    uuid: &str,
    xuid: &str,
) -> Result<
    (
        AuthMode,
        PathBuf,
        VersionJson,
        PathBuf,
        LaunchOptions,
        u32,
    ),
    String,
> {
    let mut auth = build_auth(auth_mode, username, access_token, uuid, xuid);
    fill_missing_xuid(&mut auth);
    let (java_path, version_json, mc_dir, launcher_config, java_major) =
        load_launch_config(version_dir_name)?;
    // 第三方登录：启动前确保 authlib-injector.jar 就绪（注入必需，缺失会启动失败）。
    // 缺失时先尝试自动补齐（官方 → BMCLAPI 双源），仍失败才报错——不该为了一个
    // 组件文件让用户重走一遍登录流程。
    if matches!(auth, AuthMode::External { .. }) {
        ensure_jar().await.map_err(|e| {
            format!("第三方登录组件缺失且自动下载失败\n请检查网络后重试\n{e}")
        })?;
    }
    Ok((
        auth,
        java_path,
        version_json,
        mc_dir,
        launcher_config,
        java_major,
    ))
}
