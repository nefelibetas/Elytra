//! 配置 / 数据 / 游戏目录基准（平台感知）。

use std::path::{Path, PathBuf};

use crate::constants;

/// 配置目录（Settings.toml / secrets.bin / tasks.db / 日志基准）：三端统一走平台用户数据目录
/// （Windows `%APPDATA%\Elytra`、macOS `~/Library/Application Support/Elytra`、Linux `$XDG_DATA_HOME/Elytra`）；
/// exe 旁在只读安装位置不可写、多用户共用会因 `secrets.bin` 存 token 而互相污染；dev / cargo test 仍走 exe 旁隔离。
pub fn config_dir() -> PathBuf {
    if is_dev_runtime() {
        return exe_dir().join(constants::CONFIG_DIR);
    }
    if let Some(root) = platform_data_root() {
        return root.join(constants::CONFIG_DIR);
    }
    // 极端环境（APPDATA/HOME 均缺失）：退回 exe 旁，至少让应用能起来
    exe_dir().join(constants::CONFIG_DIR)
}

/// 平台用户数据根目录（Windows）：`%APPDATA%`（Roaming），缺省回退 `%USERPROFILE%\AppData\Roaming`。
#[cfg(target_os = "windows")]
fn platform_data_root() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("AppData/Roaming"))
    })
}

/// 平台用户数据根目录（macOS）：`~/Library/Application Support`。
#[cfg(target_os = "macos")]
fn platform_data_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
}

/// 平台用户数据根目录（Linux）：`$XDG_DATA_HOME` 非空用之，否则 `~/.local/share`（XDG）。
#[cfg(all(unix, not(target_os = "macos")))]
fn platform_data_root() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        if !x.is_empty() {
            return Some(PathBuf::from(x));
        }
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
}

/// exe 是否位于 cargo target/debug|release 下（dev 与 cargo test 进程）。
/// 此类进程配置走 exe 旁，使测试隔离于 `target/debug/deps/`、不污染真实用户目录。
fn is_dev_runtime() -> bool {
    std::env::current_exe()
        .ok()
        .map(|p| path_under_cargo_target(&p))
        .unwrap_or(false)
}

/// 路径是否形如 `…/target/{debug|release}/…`（纯函数，便于单测）。
fn path_under_cargo_target(p: &Path) -> bool {
    let comps: Vec<&str> = p
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    comps
        .windows(2)
        .any(|w| w[0] == "target" && matches!(w[1], "debug" | "release"))
}

/// 启动器 exe 所在目录（Windows 便携语义锚点）；macOS/Linux 数据布局不依赖 exe 位置。
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .or_else(|| {
            // current_exe 解析失败（极端环境）：回退用户主目录，不依赖进程 cwd
            std::env::var_os("HOME").map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 默认游戏目录（.minecraft）基准；`None` = 无合适默认，交用户显式添加。Windows exe 旁（便携，整目录拷走即迁移），
/// 仅只读安装位置（Program Files，MSI 默认）无默认（建目录都会失败）；macOS/Linux 一律无默认（exe 位置由包管理器/挂载器决定）；
/// dev / cargo test 走 exe 旁隔离。装机形态游戏目录可能在安装目录内，**卸载程序必须保留它**。
pub fn default_game_dir_base() -> Option<PathBuf> {
    if is_dev_runtime() {
        return Some(exe_dir());
    }
    #[cfg(target_os = "windows")]
    if !exe_dir_is_readonly() {
        return Some(exe_dir());
    }
    None
}

/// exe 是否位于常见的只读安装位置（Windows 的 Program Files）。
#[cfg(target_os = "windows")]
fn exe_dir_is_readonly() -> bool {
    path_is_readonly_install(&exe_dir())
}

/// 路径是否位于只读安装位置（Program Files）——纯函数不做 IO；命中时默认游戏目录改走用户数据目录。
#[cfg(target_os = "windows")]
fn path_is_readonly_install(p: &Path) -> bool {
    p.to_string_lossy().to_lowercase().contains("program files")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// cargo 产物路径判定：target/{debug|release} 段命中（deps 测试进程在 debug 下）。
    #[test]
    fn path_under_cargo_target_detects_dev_runtime() {
        assert!(path_under_cargo_target(Path::new(
            "src-tauri/target/debug/Elytra.exe"
        )));
        assert!(path_under_cargo_target(Path::new(
            "src-tauri/target/debug/deps/Elytra-abc.exe"
        )));
        assert!(path_under_cargo_target(Path::new(
            "home/u/elytra/target/release/elytra"
        )));
        assert!(!path_under_cargo_target(Path::new("/usr/bin/elytra")));
        assert!(!path_under_cargo_target(Path::new("target/debuggy/x")));
        assert!(!path_under_cargo_target(Path::new(
            "/tmp/.mount_elytraX/usr/bin/elytra"
        )));
    }

    /// 只读安装位置判定：Program Files 下默认游戏目录不能放 exe 旁（建目录会失败），应回退用户数据目录。
    #[cfg(target_os = "windows")]
    #[test]
    fn readonly_install_detects_program_files() {
        assert!(path_is_readonly_install(Path::new(
            r"C:\Program Files\Elytra\Elytra Launcher.exe"
        )));
        assert!(path_is_readonly_install(Path::new(
            r"C:\Program Files (x86)\Elytra\Elytra.exe"
        )));
        // 用户级安装（NSIS 默认位置）与便携目录都不算只读
        assert!(!path_is_readonly_install(Path::new(
            r"C:\Users\u\AppData\Local\Programs\Elytra\Elytra.exe"
        )));
        assert!(!path_is_readonly_install(Path::new(
            r"D:\Games\Elytra\Elytra.exe"
        )));
    }
}
