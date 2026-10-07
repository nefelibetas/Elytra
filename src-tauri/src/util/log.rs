//! 启动器日志落盘（stdout/stderr 重定向）。

use super::dirs::config_dir;

#[cfg(target_os = "windows")]
use std::os::windows::io::AsRawHandle;

/// 启动器日志落盘：把进程 stdout/stderr 重定向到 exe 旁 Elytra/launcher.log（追加）。
/// 启动器为 GUI 进程（Windows 无控制台），eprintln 输出无人可见，落盘后设置页可查看。
/// 注意：file 句柄必须保活（mem::forget），否则 Drop 关闭句柄后被系统复用，重定向失效。
pub fn init_file_logging() {
    let path = config_dir().join("launcher.log");
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    #[cfg(target_os = "windows")]
    {
        let handle = file.as_raw_handle();
        // SAFETY: handle 来自仍存活的 file（函数末尾 mem::forget 保活，避免句柄被关闭后复用），
        // 是有效的文件句柄；SetStdHandle 仅记录该句柄，两个常量符合 STD_OUTPUT/STD_ERROR 定义。
        unsafe {
            extern "system" {
                fn SetStdHandle(n_std_handle: u32, h_handle: *mut std::ffi::c_void) -> i32;
            }
            const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
            const STD_ERROR_HANDLE: u32 = -12i32 as u32;
            SetStdHandle(STD_OUTPUT_HANDLE, handle);
            SetStdHandle(STD_ERROR_HANDLE, handle);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // POSIX：dup2 到 stdout/stderr（std::os::unix::io::AsRawFd + libc::dup2）
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = file.as_raw_fd();
            // SAFETY: fd 来自仍存活的 file（mem::forget 保活），1/2 分别为 stdout/stderr 的 fd；
            // dup2 的参数均为有效 fd，返回副本独立有效，调用无指针解引用。
            unsafe {
                libc::dup2(fd, 1);
                libc::dup2(fd, 2);
            }
        }
    }
    std::mem::forget(file);
}
