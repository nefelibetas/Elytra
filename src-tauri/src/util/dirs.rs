//! 配置 / 数据 / 游戏目录基准（平台感知）。

#[cfg(any(not(target_os = "windows"), test))]
use std::path::Path;
use std::path::PathBuf;

use crate::constants;

/// 配置目录（Settings.toml / secrets.bin / tasks.db / 日志的基准）。
/// - Windows：exe 旁 `Elytra/`（便携语义，与历史布局一致）
/// - macOS：`~/Library/Application Support/Elytra`——current_exe 指向 app bundle 内部
///   （`Contents/MacOS/`），写入会破坏签名且 /Applications 下不可写，配置必须外置
/// - Linux：`$XDG_DATA_HOME/Elytra`（缺省 `~/.local/share/Elytra`）——exe 位置由
///   包管理器/挂载器决定（/usr 只读、AppImage 挂载点每次不同），数据一律走用户目录
/// - dev/cargo test 进程（exe 位于 cargo target 下）走 exe 旁：测试隔离 + dev 行为不变
pub fn config_dir() -> PathBuf {
    #[cfg(not(target_os = "windows"))]
    {
        if is_dev_runtime() {
            return exe_dir().join(constants::CONFIG_DIR);
        }
        if let Some(root) = platform_data_root() {
            return root.join(constants::CONFIG_DIR);
        }
    }
    exe_dir().join(constants::CONFIG_DIR)
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
#[cfg(not(target_os = "windows"))]
fn is_dev_runtime() -> bool {
    std::env::current_exe()
        .ok()
        .map(|p| path_under_cargo_target(&p))
        .unwrap_or(false)
}

/// 路径是否形如 `…/target/{debug|release}/…`（纯函数，便于单测）。
#[cfg(any(not(target_os = "windows"), test))]
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

/// 默认游戏目录（.minecraft）的基准目录。
/// - Windows：exe 旁（便携语义，与历史一致）
/// - macOS/Linux：应用数据目录（`<数据根>/Elytra`）——默认 .minecraft 由启动器自持，
///   与官方/其它启动器默认位置隔离；需要共享的目录在游戏目录列表里显式添加
pub fn default_game_dir_base() -> PathBuf {
    #[cfg(target_os = "windows")]
    return exe_dir();
    // macOS/Linux：dev/cargo test 进程走 exe 旁（与 config_dir 同规则，测试隔离）；
    // 发布形态 = 应用数据目录（默认游戏目录 = <数据根>/Elytra/.minecraft）
    #[cfg(not(target_os = "windows"))]
    {
        if is_dev_runtime() {
            exe_dir()
        } else if let Some(root) = platform_data_root() {
            root.join(constants::CONFIG_DIR)
        } else if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home)
        } else {
            exe_dir()
        }
    }
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
}
