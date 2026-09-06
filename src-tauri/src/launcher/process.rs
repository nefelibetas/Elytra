//! 游戏进程生命周期（launcher 域，原 commands/launch.rs 内嵌逻辑下沉）：
//! 子进程 spawn（Windows Job Object 脱钩重试）、stdout/stderr 落盘、可见窗口轮询、
//! 退出监听与崩溃报告定位。命令层（commands/launch.rs）只做编排与事件派发，
//! 本模块不依赖 tauri：退出/窗口回调由调用方注入。

use std::path::{Path, PathBuf};

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
    use std::os::windows::process::CommandExt;
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
            Err(e) => return Err(format!("启动游戏失败\n{e}")),
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn spawn_game_child(
    command: &mut std::process::Command,
) -> Result<std::process::Child, String> {
    command.spawn().map_err(|e| format!("启动游戏失败\n{}", e))
}

/// 读取子进程输出并追加写入日志文件，直到 EOF 或出错。
pub fn drain_output(reader: &mut impl std::io::Read, path: &Path, is_stderr: bool) {
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
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", command]);
        cmd.creation_flags(0x08000000);
        cmd.status().map_err(|e| format!("启动进程失败\n{e}"))?
    };
    #[cfg(not(target_os = "windows"))]
    let status = std::process::Command::new("sh")
        .args(["-c", command])
        .status()
        .map_err(|e| format!("启动进程失败\n{e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("退出码 {}", status.code().unwrap_or(-1)))
    }
}

/// 启动前完整性与 natives 准备（check_launch_env 与 spawn_game 共用）。
pub fn ensure_game_ready(
    version_json: &crate::downloader::model::VersionJson,
    mc_dir: &Path,
    version_dir_name: &str,
) -> Result<(), String> {
    crate::launcher::env::pre_launch_check(version_json, mc_dir, version_dir_name)?;
    let natives_dir = mc_dir
        .join("natives")
        .join(crate::launcher::args::mc_version_of(version_dir_name));
    crate::launcher::args::ensure_old_style_natives(
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
    use std::ffi::c_void;

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
