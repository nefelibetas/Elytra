//! 游戏进程生命周期：子进程 spawn（Windows Job Object 脱钩重试）、stdout/stderr 落盘、可见窗口轮询、退出监听与崩溃报告定位；不依赖 tauri（回调由调用方注入）。

#[cfg(target_os = "windows")]
use std::ffi::c_void;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::SystemTime;

use crate::auth::external::{ensure_jar, read};
use crate::auth::microsoft::extract_xuid_from_jwt;
use crate::game::version::model::VersionJson;
use crate::launcher::args::{ensure_old_style_natives, mc_version_of};
use crate::launcher::env::{load_launch_config, pre_launch_check};
use crate::launcher::model::{AuthMode, LaunchOptions, ProcessPriority};
#[cfg(target_os = "windows")]
use crate::util::NoWindow as _;

/// 启动游戏子进程（cwd/stdio 由调用方预设）；Windows 用 CREATE_BREAKAWAY_FROM_JOB 与启动器所在 Job Object 脱钩，否则宿主 job（如终端 KILL_ON_JOB_CLOSE）关闭会连带杀死游戏。
/// 宿主 job 不允许 breakaway 时该标志会让 CreateProcess 失败 ERROR_ACCESS_DENIED(5)，回退不带标志重试（游戏留在 job 中）。
#[cfg(target_os = "windows")]
pub fn spawn_game_child(
    command: &mut std::process::Command,
) -> Result<std::process::Child, String> {
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    // 附加标志（CREATE_NO_WINDOW 由 util::process 合并：游戏是控制台程序，不带会弹命令行窗口）
    let mut flags = CREATE_BREAKAWAY_FROM_JOB;
    loop {
        command.no_window_with(flags);
        match command.spawn() {
            Ok(c) => return Ok(c),
            Err(e) if e.raw_os_error() == Some(5) && flags & CREATE_BREAKAWAY_FROM_JOB != 0 => {
                flags &= !CREATE_BREAKAWAY_FROM_JOB; // 最多重试一次
            }
            Err(e) => bail_key!("errors.launch.failed", "启动游戏失败\n{e}"),
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn spawn_game_child(
    command: &mut std::process::Command,
) -> Result<std::process::Child, String> {
    command.spawn().map_err(common_err_ctx_key!("errors.launch.failed", "启动游戏失败"))
}

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

/// 启动前完整性与 natives 准备（check_launch_env 与 spawn_game 共用）。
pub fn ensure_game_ready(
    version_json: &VersionJson,
    mc_dir: &Path,
    version_dir_name: &str,
) -> Result<(), String> {
    pre_launch_check(version_json, mc_dir, version_dir_name)?;
    let natives_dir = mc_dir.join("natives").join(mc_version_of(version_dir_name));
    ensure_old_style_natives(version_json, &mc_dir.join("libraries"), &natives_dir)?;
    Ok(())
}

/// 该进程是否存在可见顶层窗口（Minecraft 主窗口）；Windows 用 EnumWindows+IsWindowVisible，其他平台视为可见（前端另有 90s 超时兜底）。
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

    // SAFETY: 作为 EnumWindows 回调，lparam 恒为调用处传入的 &mut search（见下方），该指针在枚举期间保持有效且独占，回调内按 *mut Search 还原并解引用因此安全。
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
    // SAFETY: enum_proc 是签名匹配的 "system" 回调，search 在本栈帧存活至调用结束，其地址作为 lparam 传入；EnumWindows 同步枚举完才返回，期间指针保持有效。
    unsafe {
        EnumWindows(enum_proc, &mut search as *mut Search as isize);
    }
    search.found
}

#[cfg(not(target_os = "windows"))]
fn process_has_visible_window(_pid: u32) -> bool {
    true
}

/// 返回目录下**本次启动之后**产生的最新 `crash-*.txt` 路径；时间窗 `since` 排除历史残留报告。
fn latest_crash_report(dir: &Path, since: SystemTime) -> Option<String> {
    let mut latest: Option<(SystemTime, String)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("crash-") || path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let Ok(modified) = path.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if modified < since {
            continue;
        }
        if latest.as_ref().is_none_or(|(t, _)| modified > *t) {
            latest = Some((modified, path.display().to_string()));
        }
    }
    latest.map(|(_, p)| p)
}

/// `logs/latest.log` 在时间窗内是否含崩溃特征（覆盖 OOM 等不写 crash-report 的崩溃；只看尾部）。
fn has_crash_signature(log: &Path, since: SystemTime) -> bool {
    let Ok(modified) = std::fs::metadata(log).and_then(|m| m.modified()) else {
        return false;
    };
    if modified < since {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(log) else {
        return false;
    };
    let tail = crate::util::tail_lines(&text);
    tail.contains("---- Minecraft Crash Report ----")
        || tail.contains("OutOfMemoryError")
        || tail.contains("A fatal error has been detected")
}

/// 崩溃判定：非零退出码 + 窗口内出现崩溃证据 → 返回证据路径（崩溃报告或 latest.log）。
fn detect_crash(game_dir: &Path, exit_code: Option<i32>, since: SystemTime) -> Option<String> {
    if matches!(exit_code, Some(0)) {
        return None;
    }
    if let Some(report) = latest_crash_report(&game_dir.join("crash-reports"), since) {
        return Some(report);
    }
    let log = game_dir.join("logs").join("latest.log");
    has_crash_signature(&log, since).then(|| log.display().to_string())
}

/// 游戏窗口首次可见的轮询间隔（毫秒）。
const WINDOW_POLL_MS: u64 = 300;

pub struct SpawnRequest<'a> {
    /// 完整启动命令 argv（`args[0]` = java 可执行文件）
    pub args: &'a [String],
    /// 版本隔离 gameDir：子进程 cwd（游戏相对路径正确，crash-reports/logs 落在此处）
    pub game_dir: &'a Path,
    /// 子进程 stdout/stderr 落盘路径
    pub log_path: &'a Path,
    pub process_priority: ProcessPriority,
}

/// 启动子进程并接线：cwd/stdio → spawn（脱钩）→ 优先级 → 输出排空线程 → 后台监听（窗口可见 + 退出）；回调由调用方注入，首参为 pid。
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
    let game_dir_owned = game_dir.to_path_buf();
    let mut on_window_visible = on_window_visible;
    watch_game_process(
        child,
        pid,
        game_dir_owned,
        WINDOW_POLL_MS,
        move || on_window_visible(pid),
        move |exit_code, crash_report| on_exit(pid, exit_code, crash_report),
    );
    Ok(pid)
}

// ── 启动取消闸门 ──

/// 取消请求标志：spawn_game 在拉起进程前查一次，命中即不启动。
static LAUNCH_CANCELLED: AtomicBool = AtomicBool::new(false);
/// 本次启动已拉起的游戏进程 pid（0 = 尚未拉起）：取消时按 pid 终止。
static LAUNCH_PID: AtomicU32 = AtomicU32::new(0);

/// 新一次启动开始：清掉上一次的取消标志与 pid 登记（前端在启动流程最前调用，此后到达的取消一律生效）。
pub fn begin_launch() {
    LAUNCH_PID.store(0, Ordering::SeqCst);
    LAUNCH_CANCELLED.store(false, Ordering::SeqCst);
}

/// 本次启动是否已被请求取消。
pub fn cancel_requested() -> bool {
    LAUNCH_CANCELLED.load(Ordering::SeqCst)
}

/// 请求取消：置标志并终止已拉起的游戏进程。标志先于取 pid 写入，故与 `note_launched` 竞争时不会漏杀。
pub fn request_cancel() {
    LAUNCH_CANCELLED.store(true, Ordering::SeqCst);
    let pid = LAUNCH_PID.swap(0, Ordering::SeqCst);
    if pid != 0 {
        kill_process(pid);
    }
}

/// 登记已拉起的游戏进程；登记时若取消已到达则立即终止（返回 true 表示已终止）。
pub fn note_launched(pid: u32) -> bool {
    LAUNCH_PID.store(pid, Ordering::SeqCst);
    if !cancel_requested() {
        return false;
    }
    LAUNCH_PID.store(0, Ordering::SeqCst);
    kill_process(pid);
    true
}

/// 进程退出后注销 pid：否则后续取消可能误杀复用同一 pid 的其他进程。
pub fn clear_launched(pid: u32) {
    let _ = LAUNCH_PID.compare_exchange(pid, 0, Ordering::SeqCst, Ordering::SeqCst);
}

/// 强制终止进程（Windows 递归进程树；其他平台 SIGKILL）。
fn kill_process(pid: u32) {
    #[cfg(target_os = "windows")]
    {
        // taskkill 是控制台程序：不装配无窗口标志即一闪而过的命令行窗口
        let mut kill = std::process::Command::new("taskkill");
        kill.no_window();
        let _ = kill
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(not(target_os = "windows"))]
    {
        // SAFETY: kill(2) 对任意 pid 调用安全；SIGKILL 无内存安全影响，失败仅返回 -1。
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
}

/// 合一监听循环（后台线程）：try_wait 非阻塞查退出 + 可见窗口轮询；退出回调 on_exit（含崩溃判定与证据定位），窗口首次可见回调 on_window_visible。
pub fn watch_game_process<W, E>(
    mut child: std::process::Child,
    pid: u32,
    game_dir: PathBuf,
    window_poll_ms: u64,
    mut on_window_visible: W,
    on_exit: E,
) where
    W: FnMut() + Send + 'static,
    E: FnOnce(Option<i32>, Option<String>) + Send + 'static,
{
    std::thread::spawn(move || {
        let mut window_notified = false;
        // 判定时间窗起点：监听开始时（早于游戏产生任何日志，晚于 pre_launch 钩子）
        let watch_started = SystemTime::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // 退出码随回调送出：调用方据此区分正常退出与崩溃（非 0 → 错误级提示）
                    let exit_code = status.code();
                    // 崩溃时定位**本次**启动后的崩溃证据（报告或 latest.log 特征）
                    let crash_report = detect_crash(&game_dir, exit_code, watch_started);
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
            if !window_notified && process_has_visible_window(pid) {
                window_notified = true;
                on_window_visible();
            }
            std::thread::sleep(std::time::Duration::from_millis(window_poll_ms));
        }
    });
}

/// 读取游戏日志尾部（≤200 行）：优先版本目录 logs/latest.log，否则 `<game_dir>/logs/elytra-launch.log`；均不可读返回空串。
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
            let api_url = read().map(|a| a.api_url).unwrap_or_default();
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

/// 正版模式 xuid 缺失时从 Minecraft access token（JWT）解析兜底（部分账号 XSTS xid 字段缺失）。
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

/// 启动类命令共享的前置：构造账号（补 xuid）→ 加载配置与版本 JSON。
#[allow(clippy::type_complexity)]
pub async fn prepare_launch(
    version_dir_name: &str,
    auth_mode: &str,
    username: &str,
    access_token: &str,
    uuid: &str,
    xuid: &str,
) -> Result<(AuthMode, PathBuf, VersionJson, PathBuf, LaunchOptions, u32), String> {
    let mut auth = build_auth(auth_mode, username, access_token, uuid, xuid);
    fill_missing_xuid(&mut auth);
    let (java_path, version_json, mc_dir, launcher_config, java_major) =
        load_launch_config(version_dir_name)?;
    // 第三方登录：启动前确保 authlib-injector.jar 就绪（缺失会启动失败）；先自动补齐（官方 → BMCLAPI），仍失败才报错。
    if matches!(auth, AuthMode::External { .. }) {
        ensure_jar()
            .await
            .map_err(|e| format!("第三方登录组件缺失且自动下载失败\n请检查网络后重试\n{e}"))?;
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

#[cfg(test)]
mod crash_tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("elytra-crash-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn clean_exit_never_reports_crash() {
        let game = tmp_dir("clean");
        write(
            &game.join("crash-reports/crash-2026-01-01_00.00.00-client.txt"),
            "---- Minecraft Crash Report ----",
        );
        let since = SystemTime::now() - Duration::from_secs(60);
        assert_eq!(
            detect_crash(&game, Some(0), since),
            None,
            "退出码 0 即使存在报告文件也不算崩溃"
        );
    }

    #[test]
    fn fresh_crash_report_detected() {
        let game = tmp_dir("fresh");
        let report = game.join("crash-reports/crash-2026-10-12_08.00.00-client.txt");
        write(&report, "---- Minecraft Crash Report ----");
        let since = SystemTime::now() - Duration::from_secs(60);
        let detected = detect_crash(&game, Some(1), since).expect("本次窗口内的报告应被判定为崩溃");
        assert!(
            detected.contains("crash-2026-10-12"),
            "应返回报告路径: {detected}"
        );
    }

    #[test]
    fn stale_crash_report_ignored() {
        let game = tmp_dir("stale");
        write(
            &game.join("crash-reports/crash-2026-01-01_00.00.00-client.txt"),
            "---- Minecraft Crash Report ----",
        );
        // 窗口起点取未来：模拟"本窗口之前的旧报告"
        let since = SystemTime::now() + Duration::from_secs(60);
        assert_eq!(
            detect_crash(&game, Some(1), since),
            None,
            "旧报告不得误判成本次崩溃"
        );
    }

    #[test]
    fn non_report_txt_ignored() {
        let game = tmp_dir("other");
        write(&game.join("crash-reports/notes.txt"), "just a note");
        let since = SystemTime::now() - Duration::from_secs(60);
        assert_eq!(detect_crash(&game, Some(1), since), None);
    }

    #[test]
    fn crash_signature_in_latest_log_without_report() {
        let game = tmp_dir("logsig");
        write(
            &game.join("logs/latest.log"),
            "[12:00:00] [main/INFO]: Starting\njava.lang.OutOfMemoryError: Java heap space\n",
        );
        let since = SystemTime::now() - Duration::from_secs(60);
        let detected = detect_crash(&game, Some(1), since).expect("OOM 应被日志特征判定为崩溃");
        assert!(
            detected.ends_with("latest.log"),
            "应回退到 latest.log: {detected}"
        );
    }

    #[test]
    fn normal_log_without_signature_is_not_crash() {
        let game = tmp_dir("normallog");
        write(
            &game.join("logs/latest.log"),
            "[12:00:00] [main/INFO]: Stopping!\n",
        );
        let since = SystemTime::now() - Duration::from_secs(60);
        assert_eq!(detect_crash(&game, Some(1), since), None);
    }
}

#[cfg(test)]
mod cancel_tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// 长命子进程（作被终止目标）；Windows 走 cmd，其他平台走 sleep。
    fn spawn_idle_child() -> std::process::Child {
        #[cfg(target_os = "windows")]
        {
            let mut cmd = std::process::Command::new("cmd");
            cmd.args(["/C", "ping", "-n", "30", "127.0.0.1"]);
            cmd.no_window();
            cmd.stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("启动测试子进程失败")
        }
        #[cfg(not(target_os = "windows"))]
        {
            std::process::Command::new("sleep")
                .arg("30")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("启动测试子进程失败")
        }
    }

    /// 轮询至多 5s 等子进程退出。
    fn wait_exit(child: &mut std::process::Child) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn cancel_flag_survives_until_next_begin() {
        begin_launch();
        assert!(!cancel_requested(), "begin_launch 后不应处于已取消状态");
        request_cancel();
        assert!(cancel_requested(), "request_cancel 后应处于已取消状态");
        // 重复请求与未登记 pid 时再取消都不得清除标志
        request_cancel();
        assert!(cancel_requested(), "重复取消不应清除标志");
        begin_launch();
        assert!(
            !cancel_requested(),
            "下一次启动开始必须清掉上一次的取消标志"
        );
    }

    #[test]
    fn note_launched_without_cancel_only_registers() {
        begin_launch();
        let mut child = spawn_idle_child();
        assert!(!note_launched(child.id()), "未请求取消时不应终止进程");
        let _ = child.kill();
        let _ = child.wait();
        clear_launched(child.id());
    }

    #[test]
    fn note_launched_after_cancel_kills_process() {
        begin_launch();
        request_cancel(); // 尚未登记 pid：先置标志，模拟「进进程后注册前」到达的取消
        let mut child = spawn_idle_child();
        assert!(note_launched(child.id()), "取消已到达时应立即终止进程");
        assert!(wait_exit(&mut child), "被取消的启动进程应已退出");
        clear_launched(child.id());
    }

    #[test]
    fn request_cancel_kills_registered_process() {
        begin_launch();
        let mut child = spawn_idle_child();
        assert!(!note_launched(child.id()));
        request_cancel();
        assert!(wait_exit(&mut child), "取消应终止已登记的游戏进程");
    }
}
