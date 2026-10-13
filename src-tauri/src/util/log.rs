//! 启动器日志落盘（stdout/stderr 重定向）。

use super::dirs::config_dir;

#[cfg(target_os = "windows")]
use std::os::windows::io::AsRawHandle;

/// 把进程 stdout/stderr 重定向到 exe 旁 Elytra/launcher.log（追加）。GUI 进程无控制台，
/// eprintln 无人可见，落盘后设置页可查看。注意：file 句柄必须保活（mem::forget），
/// 否则句柄被关闭后遭系统复用，重定向失效。
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
        // SAFETY: handle 来自仍存活的文件（末尾 mem::forget 保活）；SetStdHandle 仅记录句柄。
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
            // SAFETY: fd 来自仍存活的文件（mem::forget 保活），1/2 为 stdout/stderr；dup2 参数均为有效 fd。
            unsafe {
                libc::dup2(fd, 1);
                libc::dup2(fd, 2);
            }
        }
    }
    std::mem::forget(file);
}
