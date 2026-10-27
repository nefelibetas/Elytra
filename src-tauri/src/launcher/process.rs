//! 游戏进程生命周期：子进程 spawn（Windows Job Object 脱钩重试）、stdout/stderr 落盘、可见窗口轮询、退出监听与崩溃报告定位；不依赖 tauri（回调由调用方注入）。

use std::collections::HashSet;
#[cfg(target_os = "windows")]
use std::ffi::c_void;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
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
    command
        .spawn()
        .map_err(common_err_ctx_key!("errors.launch.failed", "启动游戏失败"))
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

/// 启动器捕获的 stdout/stderr 定位：`path` + `start`（本次 spawn 前的文件长度）。捕获文件跨多次启动
/// 追加写，故只认该偏移之后的字节，避免把历史启动的输出算成本次证据。
#[derive(Clone)]
pub struct CaptureLog {
    pub path: PathBuf,
    pub start: u64,
}

/// 启动期失败特征 → 结论（顺序即优先级：越具体的工具链证据越靠前；末尾 `Exception in thread "main"`
/// 兜底为「主线程崩了但原因未归类」）。表里同时给出中文兜底结论与可操作建议。
const FAILURE_SIGNATURES: &[(&str, &str, &str, &str)] = &[
    (
        "cannot be cast to class java.net.URLClassLoader",
        "errors.launch.jdk_too_new",
        "Java 版本过高：该版本需要 Java 8（launchwrapper 在 Java 9+ 上无法启动）",
        "请在「设置 → Java 运行时」下载并选择 Java 8",
    ),
    (
        "UnsupportedClassVersionError",
        "errors.launch.jdk_too_old",
        "Java 版本过低：游戏字节码版本高于当前 Java",
        "请在「设置 → Java 运行时」切换或下载更高版本（自动选择会按版本要求匹配）",
    ),
    (
        "Error occurred during initialization of VM",
        "errors.launch.memory_too_large",
        "JVM 无法启动：内存参数超出可用内存",
        "请调小「内存管理」中的最大内存或关闭其他占内存的程序",
    ),
    (
        "Could not reserve enough space",
        "errors.launch.memory_too_large",
        "JVM 无法启动：内存参数超出可用内存",
        "请调小「内存管理」中的最大内存或关闭其他占内存的程序",
    ),
    (
        "Unrecognized VM option",
        "errors.launch.jvm_args",
        "JVM 参数不被当前 Java 支持",
        "请检查「设置 → Java 虚拟机参数」（部分参数仅老版本 Java 支持）",
    ),
    (
        "Unrecognized option",
        "errors.launch.jvm_args",
        "JVM 参数不被当前 Java 支持",
        "请检查「设置 → Java 虚拟机参数」（部分参数仅老版本 Java 支持）",
    ),
    (
        "Could not find or load main class",
        "errors.launch.missing_files",
        "启动文件不完整：缺少主类或支持库",
        "请在「版本设置」里用修复补全缺失文件",
    ),
    (
        "NoClassDefFoundError",
        "errors.launch.missing_files",
        "启动文件不完整：缺少支持库",
        "请在「版本设置」里用修复补全缺失文件",
    ),
    (
        "ClassNotFoundException",
        "errors.launch.missing_files",
        "启动文件不完整：缺少支持库",
        "请在「版本设置」里用修复补全缺失文件",
    ),
    (
        "OutOfMemoryError",
        "errors.launch.oom",
        "内存不足：游戏堆内存耗尽",
        "请在「设置 → 内存管理」调大最大内存",
    ),
    (
        "A fatal error has been detected",
        "errors.launch.native_crash",
        "JVM 崩溃（native 层，常见于显卡驱动/内存问题）",
        "请查看游戏目录下的 hs_err_pid*.log，并更新显卡驱动",
    ),
    (
        "Exception in thread \"main\"",
        "errors.launch.unknown",
        "原因未归类：主线程异常退出",
        "请把弹窗里的详情（含末尾输出）反馈给开发者",
    ),
];

/// 捕获文件本次启动段的可读文本：首窗 + 尾窗（段短于窗口时即整段；输出源编码不统一，按有损 UTF-8 解码）。
fn capture_text(capture: &CaptureLog) -> Option<String> {
    const WINDOW: u64 = 64 * 1024;
    let mut file = std::fs::File::open(&capture.path).ok()?;
    let len = file.metadata().ok()?.len();
    if len <= capture.start {
        return None;
    }
    let head = read_window(&mut file, capture.start, (len - capture.start).min(WINDOW));
    let tail_start = len.saturating_sub(WINDOW);
    if tail_start <= capture.start {
        return Some(head);
    }
    Some(format!(
        "{head}\n{}",
        read_window(&mut file, tail_start, len - tail_start)
    ))
}

/// 捕获段是否含失败特征（证据判定：启动即死写在段首，运行中崩溃写在段尾）。
fn capture_has_failure(capture: &CaptureLog) -> bool {
    capture_text(capture).is_some_and(|text| {
        FAILURE_SIGNATURES
            .iter()
            .any(|(sig, ..)| text.contains(sig))
    })
}

/// 把捕获输出推断成可操作结论（`@i18n` 协议串，key 见 `FAILURE_SIGNATURES`）：短结论 + 原始行 + 建议；
/// 未归类时附末尾输出。返回 None 表示没有可供推断的失败输出。
fn diagnose_capture(capture: &CaptureLog) -> Option<String> {
    diagnose_text(&capture_text(capture)?)
}

/// 结论推断的纯函数部分（便于单测）：见 `FAILURE_SIGNATURES` 的优先级。
fn diagnose_text(text: &str) -> Option<String> {
    let (sig, key, summary, advice) = FAILURE_SIGNATURES
        .iter()
        .find(|(sig, ..)| text.contains(sig))?;
    let detail = if *key == "errors.launch.unknown" {
        format!("{advice}\n{}", tail_lines(text, 5))
    } else {
        let mut d = line_with(text, sig);
        if *key == "errors.launch.jdk_too_old" {
            if let Some(java) = class_file_java_major(text) {
                d.push_str(&format!("\n报错行声明的字节码对应 Java {java}"));
            }
        }
        d.push_str(&format!("\n{advice}"));
        d
    };
    Some(keyed_msg!(key, "{summary}\n{detail}"))
}

/// 原始输出行去 `[stderr] ` 前缀与空白，超长截断（弹窗详情要能贴进对话）。
fn clean_line(line: &str) -> String {
    let line = line.trim().trim_start_matches("[stderr]").trim();
    if line.chars().count() > 200 {
        format!("{}…", line.chars().take(200).collect::<String>())
    } else {
        line.to_string()
    }
}

/// 含指定特征的原始行（去前缀）；找不到返回空串。
fn line_with(text: &str, sig: &str) -> String {
    text.lines()
        .find(|l| l.contains(sig))
        .map(clean_line)
        .unwrap_or_default()
}

/// 末尾 `n` 行非空输出（去前缀）：未归类失败时给排查线索。
fn tail_lines(text: &str, n: usize) -> String {
    let mut lines: Vec<String> = text
        .lines()
        .map(clean_line)
        .filter(|l| !l.is_empty())
        .collect();
    lines.drain(..lines.len().saturating_sub(n));
    lines.join("\n")
}

/// `UnsupportedClassVersionError` 报错行里的 `class file version 61.0` → 需要的 Java 主版本（61 → 17）。
fn class_file_java_major(text: &str) -> Option<u32> {
    let line = text.lines().find(|l| l.contains("class file version"))?;
    let rest = line.split("class file version").nth(1)?;
    let major: u32 = rest.trim().split('.').next()?.trim().parse().ok()?;
    major.checked_sub(44)
}

/// 读取区间字节并把字节数组交给有损 UTF-8 解码。
fn read_window(file: &mut std::fs::File, from: u64, len: u64) -> String {
    if file.seek(SeekFrom::Start(from)).is_err() {
        return String::new();
    }
    let mut buf = vec![0u8; len as usize];
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => filled += n,
        }
    }
    buf.truncate(filled);
    String::from_utf8_lossy(&buf).into_owned()
}

/// 崩溃判定：非零退出码 + 窗口内出现崩溃证据 → 返回证据路径（崩溃报告、游戏 latest.log、启动器捕获日志）。
fn detect_crash(
    game_dir: &Path,
    exit_code: Option<i32>,
    since: SystemTime,
    capture: Option<&CaptureLog>,
) -> Option<String> {
    if matches!(exit_code, Some(0)) {
        return None;
    }
    if let Some(report) = latest_crash_report(&game_dir.join("crash-reports"), since) {
        return Some(report);
    }
    let log = game_dir.join("logs").join("latest.log");
    if has_crash_signature(&log, since) {
        return Some(log.display().to_string());
    }
    // JVM 在游戏日志成型前退出（JDK 不兼容、主类缺失、参数被拒）：证据只在捕获的 stdout/stderr 里
    capture
        .filter(|c| capture_has_failure(c))
        .map(|c| c.path.display().to_string())
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
    E: FnOnce(u32, Option<i32>, Option<String>, Option<String>) + Send + 'static,
{
    let SpawnRequest {
        args,
        game_dir,
        log_path,
        process_priority,
    } = req;
    let Some((program, rest)) = args.split_first() else {
        return Err(keyed_msg!(
            "errors.launch.commandEmpty",
            "启动命令为空\n无法启动游戏"
        ));
    };
    // 捕获文件是追加写的：先记长度，退出判定只认本次 spawn 之后新增的字节
    let capture = CaptureLog {
        path: log_path.to_path_buf(),
        start: std::fs::metadata(log_path).map(|m| m.len()).unwrap_or(0),
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
        capture,
        WINDOW_POLL_MS,
        move || on_window_visible(pid),
        move |exit_code, crash_report, diagnosis| on_exit(pid, exit_code, crash_report, diagnosis),
    );
    Ok(pid)
}

// ── 启动取消闸门 ──

/// 启动代际：每次 begin_launch 递增；取消只作用于对应代际。
static LAUNCH_SEQ: AtomicU64 = AtomicU64::new(0);
static CANCELLED_GENS: LazyLock<Mutex<HashSet<u64>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
/// 本次启动已拉起的游戏进程 pid 与代际（0 = 尚未拉起）：取消时按 pid 终止。
static LAUNCH_PID: AtomicU32 = AtomicU32::new(0);
static LAUNCH_PID_GEN: AtomicU64 = AtomicU64::new(0);

/// 新一次启动开始：分配新代际，并取消/终止上一代（旧 pre-launch 可能仍在运行）。
pub fn begin_launch() -> u64 {
    let generation = LAUNCH_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    if generation > 1 {
        let previous = generation - 1;
        CANCELLED_GENS.lock().unwrap().insert(previous);
        let pid_gen = LAUNCH_PID_GEN.load(Ordering::SeqCst);
        let pid = LAUNCH_PID.load(Ordering::SeqCst);
        if pid_gen == previous && pid != 0 {
            LAUNCH_PID.store(0, Ordering::SeqCst);
            LAUNCH_PID_GEN.store(0, Ordering::SeqCst);
            kill_process(pid);
        }
    }
    LAUNCH_PID.store(0, Ordering::SeqCst);
    LAUNCH_PID_GEN.store(0, Ordering::SeqCst);
    generation
}

/// 指定代际是否已被请求取消。
pub fn cancel_requested(generation: u64) -> bool {
    CANCELLED_GENS.lock().unwrap().contains(&generation)
}

/// 请求取消：先登记代际再取 pid，故与 `note_launched` 竞争时不会漏杀。
pub fn request_cancel(generation: u64) {
    CANCELLED_GENS.lock().unwrap().insert(generation);
    if LAUNCH_PID_GEN.load(Ordering::SeqCst) == generation {
        let pid = LAUNCH_PID.swap(0, Ordering::SeqCst);
        if pid != 0 {
            kill_process(pid);
        }
    }
}

/// 登记已拉起的游戏进程；非当前代际或已取消则立即终止。
pub fn note_launched(generation: u64, pid: u32) -> bool {
    if LAUNCH_SEQ.load(Ordering::SeqCst) != generation {
        kill_process(pid);
        return true;
    }
    LAUNCH_PID.store(pid, Ordering::SeqCst);
    LAUNCH_PID_GEN.store(generation, Ordering::SeqCst);
    if !cancel_requested(generation) {
        return false;
    }
    if LAUNCH_PID_GEN.load(Ordering::SeqCst) == generation {
        LAUNCH_PID.store(0, Ordering::SeqCst);
    }
    kill_process(pid);
    true
}

/// 进程退出后注销 pid：否则后续取消可能误杀复用同一 pid 的其他进程。
pub fn clear_launched(generation: u64, pid: u32) {
    if LAUNCH_PID_GEN.load(Ordering::SeqCst) == generation
        && LAUNCH_PID.load(Ordering::SeqCst) == pid
    {
        LAUNCH_PID.store(0, Ordering::SeqCst);
        LAUNCH_PID_GEN.store(0, Ordering::SeqCst);
    }
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
    capture: CaptureLog,
    window_poll_ms: u64,
    mut on_window_visible: W,
    on_exit: E,
) where
    W: FnMut() + Send + 'static,
    E: FnOnce(Option<i32>, Option<String>, Option<String>) + Send + 'static,
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
                    // 崩溃时定位**本次**启动后的崩溃证据（报告、游戏日志或捕获输出）
                    let crash_report =
                        detect_crash(&game_dir, exit_code, watch_started, Some(&capture));
                    // 失败原因推断（非零退出才有意义）：给前端一句可操作的结论
                    let diagnosis = match exit_code {
                        Some(0) | None => None,
                        Some(_) => diagnose_capture(&capture),
                    };
                    on_exit(exit_code, crash_report, diagnosis);
                    return;
                }
                Ok(None) => {}
                Err(_) => {
                    // try_wait 失败（状态查询异常）：按未知退出处理，避免线程永久挂起
                    on_exit(None, None, None);
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
        ensure_jar().await.map_err(|e| {
            keyed_msg!(
                "errors.launch.authlibMissing",
                "第三方登录组件缺失且自动下载失败\n请检查网络后重试\n{e}"
            )
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
            detect_crash(&game, Some(0), since, None),
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
        let detected =
            detect_crash(&game, Some(1), since, None).expect("本次窗口内的报告应被判定为崩溃");
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
            detect_crash(&game, Some(1), since, None),
            None,
            "旧报告不得误判成本次崩溃"
        );
    }

    #[test]
    fn non_report_txt_ignored() {
        let game = tmp_dir("other");
        write(&game.join("crash-reports/notes.txt"), "just a note");
        let since = SystemTime::now() - Duration::from_secs(60);
        assert_eq!(detect_crash(&game, Some(1), since, None), None);
    }

    #[test]
    fn crash_signature_in_latest_log_without_report() {
        let game = tmp_dir("logsig");
        write(
            &game.join("logs/latest.log"),
            "[12:00:00] [main/INFO]: Starting\njava.lang.OutOfMemoryError: Java heap space\n",
        );
        let since = SystemTime::now() - Duration::from_secs(60);
        let detected =
            detect_crash(&game, Some(1), since, None).expect("OOM 应被日志特征判定为崩溃");
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
        assert_eq!(detect_crash(&game, Some(1), since, None), None);
    }

    /// 写一份捕获日志：`old` 为历史启动的输出（不计入本次），返回其本次段起点。
    fn capture_log(log: &Path, old: &str, new: &str) -> CaptureLog {
        write(log, &format!("{old}{new}"));
        CaptureLog {
            path: log.to_path_buf(),
            start: old.len() as u64,
        }
    }

    #[test]
    fn capture_failure_in_this_run_detected() {
        let game = tmp_dir("capture_new");
        let log = game.join("logs/elytra-launch.log");
        let capture = capture_log(
            &log,
            "[stderr] Session finished\n",
            "[stderr] Exception in thread \"main\" \n[stderr] java.lang.ClassCastException: AppClassLoader\n",
        );
        let detected = detect_crash(&game, Some(1), SystemTime::now(), Some(&capture))
            .expect("本次捕获段的启动期失败应判为崩溃");
        assert!(
            detected.ends_with("elytra-launch.log"),
            "证据路径应指向捕获日志: {detected}"
        );
    }

    #[test]
    fn capture_failure_before_start_offset_ignored() {
        let game = tmp_dir("capture_old");
        let log = game.join("logs/elytra-launch.log");
        let capture = capture_log(
            &log,
            "[stderr] Exception in thread \"main\"\n",
            "[stderr] Session finished\n",
        );
        assert_eq!(
            detect_crash(&game, Some(1), SystemTime::now(), Some(&capture)),
            None,
            "偏移之前的历史输出不得算成本次证据"
        );
    }

    #[test]
    fn capture_other_thread_exception_ignored() {
        let game = tmp_dir("capture_thread");
        let log = game.join("logs/elytra-launch.log");
        let capture = capture_log(
            &log,
            "",
            "[stderr] Exception in thread \"Thread-0\" \n[stderr] java.io.FileNotFoundException\n",
        );
        assert_eq!(
            detect_crash(&game, Some(1), SystemTime::now(), Some(&capture)),
            None,
            "mod 自定义线程的异常不影响主流程，不是崩溃证据"
        );
    }

    #[test]
    fn capture_tail_window_scanned() {
        let game = tmp_dir("capture_tail");
        let log = game.join("logs/elytra-launch.log");
        // 失败特征落在首窗之外：本次段首部是正常输出，退出前才报错
        let filler = "x".repeat(70 * 1024);
        let capture = capture_log(
            &log,
            "",
            &format!("{filler}\n[stderr] Error: Could not find or load main class X\n"),
        );
        assert!(
            detect_crash(&game, Some(1), SystemTime::now(), Some(&capture)).is_some(),
            "尾部窗口内的启动期失败应被识别"
        );
    }

    /// 全链烟测：真实子进程非零退出 → stderr 落盘 → 证据路径为捕获日志（含偏移语义：历史段不计）。
    #[test]
    fn spawn_and_watch_reports_capture_evidence() {
        let tmp = tmp_dir("spawn_capture");
        let game = tmp.join("game");
        fs::create_dir_all(&game).unwrap();
        let log = tmp.join("logs/elytra-launch.log");
        fs::create_dir_all(log.parent().unwrap()).unwrap();
        // 历史启动的同类输出：不得被算成本次证据（偏移在 spawn 前测定）
        fs::write(&log, "[stderr] Exception in thread \"main\"\n").unwrap();

        #[cfg(target_os = "windows")]
        let args = vec![
            "cmd".to_string(),
            "/C".to_string(),
            "echo Error: Could not find or load main class Foo 1>&2 & exit 3".to_string(),
        ];
        #[cfg(not(target_os = "windows"))]
        let args = vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo 'Error: Could not find or load main class Foo' >&2; exit 3".to_string(),
        ];

        let (tx, rx) = std::sync::mpsc::channel();
        let pid = spawn_and_watch(
            SpawnRequest {
                args: &args,
                game_dir: &game,
                log_path: &log,
                process_priority: ProcessPriority::Normal,
            },
            |_| {},
            move |_pid, code, report, cause| {
                let _ = tx.send((code, report, cause));
            },
        )
        .unwrap();
        assert!(pid > 0);

        let (code, report, cause) = rx
            .recv_timeout(Duration::from_secs(60))
            .expect("子进程应退出并回调");
        assert_eq!(code, Some(3));
        let report = report.expect("本次 stderr 的启动期失败应作为崩溃证据");
        assert!(
            report.ends_with("elytra-launch.log"),
            "证据路径应指向捕获日志: {report}"
        );
        let cause = cause.expect("同样应从捕获输出推断出结论");
        assert!(
            cause.starts_with("@i18n:errors.launch.missing_files\n"),
            "主类缺失应归到启动文件不完整: {cause}"
        );
    }

    /// 结论推断：每个特征映射到对应 key；未归类时附末尾输出，且 Java 版本能从字节码版本推出来。
    #[test]
    fn diagnose_maps_failures_to_actionable_causes() {
        let cases = [
            (
                "java.lang.ClassCastException: class jdk.internal.loader.ClassLoaders$AppClassLoader cannot be cast to class java.net.URLClassLoader",
                "errors.launch.jdk_too_new",
            ),
            (
                "java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version of the Java Runtime (class file version 61.0)",
                "errors.launch.jdk_too_old",
            ),
            ("java.lang.OutOfMemoryError: Java heap space", "errors.launch.oom"),
            (
                "Error occurred during initialization of VM\nCould not reserve enough space for 8388608KB object heap",
                "errors.launch.memory_too_large",
            ),
            ("Unrecognized VM option 'CMSIncrementalMode'", "errors.launch.jvm_args"),
            (
                "Error: Could not find or load main class net.minecraft.launchwrapper.Launch",
                "errors.launch.missing_files",
            ),
            (
                "# A fatal error has been detected by the Java Runtime Environment",
                "errors.launch.native_crash",
            ),
            (
                "[stderr] Exception in thread \"main\" java.lang.IllegalStateException: boom",
                "errors.launch.unknown",
            ),
        ];
        for (text, key) in cases {
            let msg = diagnose_text(text).unwrap_or_else(|| panic!("应推断出结论: {text}"));
            assert!(msg.starts_with(&format!("@i18n:{key}\n")), "{text} → {msg}");
        }
        // 字节码版本 → 需要的 Java 主版本（61 → 17）写进详情
        let too_old = diagnose_text(
            "java.lang.UnsupportedClassVersionError: X (class file version 61.0)\n\tat java.base/x",
        )
        .unwrap();
        assert!(too_old.contains("对应 Java 17"), "{too_old}");
        // 只有 mod 线程的异常不算证据
        assert!(
            diagnose_text("Exception in thread \"Thread-0\" java.io.FileNotFoundException")
                .is_none(),
            "非主线程异常不是启动期失败"
        );
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
        let gen = begin_launch();
        assert!(!cancel_requested(gen), "begin_launch 后不应处于已取消状态");
        request_cancel(gen);
        assert!(cancel_requested(gen), "request_cancel 后应处于已取消状态");
        // 重复请求与未登记 pid 时再取消都不得清除标志
        request_cancel(gen);
        assert!(cancel_requested(gen), "重复取消不应清除标志");
        let next = begin_launch();
        assert!(
            !cancel_requested(next),
            "下一次启动开始必须清掉上一次的取消标志"
        );
        assert!(cancel_requested(gen), "旧代际仍应保持取消");
    }

    #[test]
    fn note_launched_without_cancel_only_registers() {
        let gen = begin_launch();
        let mut child = spawn_idle_child();
        assert!(!note_launched(gen, child.id()), "未请求取消时不应终止进程");
        let _ = child.kill();
        let _ = child.wait();
        clear_launched(gen, child.id());
    }

    #[test]
    fn note_launched_after_cancel_kills_process() {
        let gen = begin_launch();
        request_cancel(gen); // 尚未登记 pid：先置标志，模拟「进进程后注册前」到达的取消
        let mut child = spawn_idle_child();
        assert!(note_launched(gen, child.id()), "取消已到达时应立即终止进程");
        assert!(wait_exit(&mut child), "被取消的启动进程应已退出");
        clear_launched(gen, child.id());
    }

    #[test]
    fn request_cancel_kills_registered_process() {
        let gen = begin_launch();
        let mut child = spawn_idle_child();
        assert!(!note_launched(gen, child.id()));
        request_cancel(gen);
        assert!(wait_exit(&mut child), "取消应终止已登记的游戏进程");
    }

    #[test]
    fn next_generation_kills_previous_registered_process() {
        let gen = begin_launch();
        let mut child = spawn_idle_child();
        assert!(!note_launched(gen, child.id()));
        let _next = begin_launch();
        assert!(wait_exit(&mut child), "新启动开始应终止上一代进程");
        clear_launched(gen, child.id());
    }
}
