//! 启动器日志落盘（stdout/stderr 重定向）。

use super::dirs::config_dir;

#[cfg(target_os = "windows")]
use std::os::windows::io::AsRawHandle;
use std::path::Path;

/// launcher.log 单文件上限 5 MiB：超限则在启动时轮转为 `launcher.log.1`（仅保留一份备份）。
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// 把进程 stdout/stderr 重定向到 exe 旁 Elytra/launcher.log（追加）。GUI 进程无控制台，
/// eprintln 无人可见，落盘后设置页可查看。注意：file 句柄必须保活（mem::forget），
/// 否则句柄被关闭后遭系统复用，重定向失效。
pub fn init_file_logging() {
    let path = config_dir().join("launcher.log");
    rotate_if_oversized(&path);
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

/// 轮转只在启动时执行（运行期句柄已打开，文件无法改名）；改名失败时保持追加，不影响日志功能。
fn rotate_if_oversized(path: &Path) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() <= MAX_LOG_BYTES {
        return;
    }
    let backup = path.with_extension("log.1");
    let _ = std::fs::remove_file(&backup);
    let _ = std::fs::rename(path, &backup);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 用例独立目录（tag + 进程 id 隔离；先清后建）
    fn case_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("elytra-log-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建用例目录");
        dir
    }

    #[test]
    fn no_rotation_below_limit() {
        let dir = case_dir("below");
        let path = dir.join("launcher.log");
        std::fs::write(&path, b"few lines").expect("写日志");
        rotate_if_oversized(&path);
        assert!(path.exists(), "未超上限不得轮转");
        assert!(!path.with_extension("log.1").exists());
        assert_eq!(std::fs::read(&path).expect("读日志"), b"few lines");
    }

    #[test]
    fn no_rotation_without_file() {
        let dir = case_dir("missing");
        let path = dir.join("launcher.log");
        rotate_if_oversized(&path);
        assert!(!path.exists() && !path.with_extension("log.1").exists());
    }

    #[test]
    fn rotation_moves_oversized_file_and_replaces_old_backup() {
        let dir = case_dir("over");
        let path = dir.join("launcher.log");
        let backup = path.with_extension("log.1");
        std::fs::write(&backup, b"previous").expect("写旧备份");
        let file = std::fs::File::create(&path).expect("建日志文件");
        file.set_len(MAX_LOG_BYTES + 1).expect("置文件长度");
        rotate_if_oversized(&path);
        assert!(!path.exists(), "超上限文件应被移走");
        assert_eq!(
            std::fs::metadata(&backup).expect("备份存在").len(),
            MAX_LOG_BYTES + 1,
            "新备份须覆盖旧备份"
        );
    }
}
