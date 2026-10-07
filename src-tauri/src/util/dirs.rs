//! 配置 / 数据 / 游戏目录基准（平台感知）。

use std::path::{Path, PathBuf};

use crate::constants;

/// 配置目录（Settings.toml / secrets.bin / tasks.db / 日志的基准）。
/// - 三端统一走平台用户数据目录：Windows `%APPDATA%\Elytra`、macOS
///   `~/Library/Application Support/Elytra`、Linux `$XDG_DATA_HOME/Elytra`。
///   理由与 macOS/Linux 原本相同，只是 Windows 此前是例外：安装到只读位置
///   （Program Files / /Applications / /usr）时 exe 旁根本不可写；多用户共用一台
///   机器时账号与配置也不应互相污染（`secrets.bin` 存着 token）。
/// - dev / cargo test 进程（exe 位于 cargo target 下）仍走 exe 旁：测试天然隔离在
///   `target/debug/deps/`，`pnpm tauri dev` 的数据也留在 target 下，不碰真实用户目录。
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

/// 平台用户数据根目录（配置与数据的用户级基准，三端语义对齐）。
/// - Windows：`%APPDATA%`（Roaming），缺省回退 `%USERPROFILE%\AppData\Roaming`
/// - macOS：`~/Library/Application Support`
/// - Linux：`$XDG_DATA_HOME` 非空用之，否则 `~/.local/share`（XDG Base Directory）
#[cfg(target_os = "windows")]
fn platform_data_root() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("AppData/Roaming")))
}

/// 平台用户数据根目录（macOS/Linux，config 与默认游戏目录的共同基准）。
/// - macOS：`~/Library/Application Support`
/// - Linux：`$XDG_DATA_HOME` 非空用之，否则 `~/.local/share`（XDG Base Directory）
#[cfg(target_os = "macos")]
fn platform_data_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
}

/// 平台用户数据根目录（macOS/Linux，config 与默认游戏目录的共同基准）。
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
/// 此类进程配置走 exe 旁：单元测试进程在 `target/debug/deps/` 下，配置天然落在
/// 该隔离区（不污染真实用户数据目录），`pnpm tauri dev` 行为也与历史一致。
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

/// 启动器可执行文件所在目录。Windows 便携语义锚点（exe 旁数据/游戏目录）；
/// macOS/Linux 的数据布局不依赖 exe 位置（见 config_dir / default_game_dir_base）。
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

/// 默认游戏目录（.minecraft）的基准目录；`None` = 该平台没有合适的默认位置，
/// 交由用户显式添加游戏目录。
/// - Windows：exe 旁（便携语义——与 PCL2 / HMCL 等国内启动器习惯一致，用户找得到、
///   备份与迁移就是整目录拷走）。仅当 exe 落在只读位置（Program Files，MSI 默认）
///   时无默认：那里连建目录都会失败。
/// - macOS/Linux：**无默认**。exe 位置由包管理器/挂载器决定（/usr 只读、AppImage
///   挂载点每次不同），猜一个「应用数据目录下的 .minecraft」用户既找不到也想不到；
///   游戏目录一律由用户在列表里显式添加。
/// - dev / cargo test 进程：exe 旁（与 config_dir 同规则，测试隔离）。
///
/// 注意：装机形态下游戏目录可能位于安装目录内，**卸载程序必须保留它**，
/// 否则用户几个 GB 的整合包会随卸载一起消失。
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

/// 路径是否位于常见的只读安装位置（Program Files / Program Files (x86)）。
/// 纯函数、不做 IO：命中时默认游戏目录改走用户数据目录，避免「建目录即失败」。
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

    /// 只读安装位置判定：Program Files 下不能把默认游戏目录放在 exe 旁
    /// （建目录就会失败），此时应回退到用户数据目录。
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
