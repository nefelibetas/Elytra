//! 子进程装配（跨域小工具）：应用运行期间**不得弹出控制台窗口**。
//!
//! 本应用是 GUI 进程（Windows 下无控制台附着），直接 spawn 控制台程序
//! （`java` / `cmd` / `taskkill` …）时系统会为子进程**新建一个控制台窗口**，
//! 子进程退出即消失。实测表现为装 Forge/NeoForge 时连弹多个命令行窗口
//! （processors 引擎逐个 `java -cp …` 跑安装器工具）。
//!
//! **所有 spawn 点必须经 [`NoWindow`] 装配。**

/// Windows 进程创建标志：子进程不创建控制台窗口。
#[cfg(target_os = "windows")]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 无窗口装配：Windows 合并 `CREATE_NO_WINDOW`；其它平台无此概念（空实现）。
///
/// `creation_flags` 是**覆盖式**设置，同一条命令只能设一次：需要附加标志
/// （`CREATE_BREAKAWAY_FROM_JOB` / `CREATE_NEW_PROCESS_GROUP` …）时走
/// [`NoWindow::no_window_with`]，不要自己再调用一次 `creation_flags`（会丢掉
/// `CREATE_NO_WINDOW`，命令行窗口又冒出来）。
pub trait NoWindow {
    /// 隐藏控制台窗口并附加 `extra_flags`（Windows 有效，其它平台为空实现）。
    fn no_window_with(&mut self, extra_flags: u32) -> &mut Self;
    /// 仅隐藏控制台窗口。
    fn no_window(&mut self) -> &mut Self {
        self.no_window_with(0)
    }
}

#[cfg(target_os = "windows")]
impl NoWindow for std::process::Command {
    fn no_window_with(&mut self, extra_flags: u32) -> &mut Self {
        use std::os::windows::process::CommandExt as _;
        self.creation_flags(CREATE_NO_WINDOW | extra_flags);
        self
    }
}

#[cfg(not(target_os = "windows"))]
impl NoWindow for std::process::Command {
    fn no_window_with(&mut self, _extra_flags: u32) -> &mut Self {
        self
    }
}

#[cfg(target_os = "windows")]
impl NoWindow for tokio::process::Command {
    fn no_window_with(&mut self, extra_flags: u32) -> &mut Self {
        self.creation_flags(CREATE_NO_WINDOW | extra_flags);
        self
    }
}

#[cfg(not(target_os = "windows"))]
impl NoWindow for tokio::process::Command {
    fn no_window_with(&mut self, _extra_flags: u32) -> &mut Self {
        self
    }
}

/// 为当前进程分配一个**隐藏的**控制台（仅当当前没有控制台时）。
///
/// # 为什么需要
///
/// 本仓 spawn 点已统一经 [`NoWindow`] 装配 `CREATE_NO_WINDOW`，但**第三方库内部的
/// spawn 无法装配**（easytier 在 Windows 上用 `cmd /C ver`、`ipconfig`、`ping`、
/// `nslookup` 探测系统信息 / 配置 DNS）。GUI 进程（无控制台）spawn 控制台程序时，
/// Windows 会**为子进程新建控制台窗口**——这就是「应用跑着跑着闪出命令行窗口」的
/// 兜底来源。只要本进程持有一个**隐藏**控制台，这些子进程就继承它，不再新建窗口。
///
/// # 保守语义
///
/// 幂等且不越界：已有控制台（dev 下从终端启动 console 子系统构建）直接返回，
/// **绝不隐藏用户的终端窗口**；分配失败（如被阻止）静默忽略。
pub fn ensure_hidden_console() {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "Kernel32")]
        extern "system" {
            fn GetConsoleWindow() -> *mut std::ffi::c_void;
            fn AllocConsole() -> i32;
        }
        #[link(name = "User32")]
        extern "system" {
            fn ShowWindow(hwnd: *mut std::ffi::c_void, cmd_show: i32) -> i32;
        }
        const SW_HIDE: i32 = 0;
        // SAFETY: 三个调用只读写本进程的控制台状态，无指针解引用；失败返回
        // 空句柄 / 0，静默忽略即可（分配不出控制台时行为退回原状）。
        unsafe {
            if !GetConsoleWindow().is_null() {
                return;
            }
            if AllocConsole() == 0 {
                return;
            }
            ShowWindow(GetConsoleWindow(), SW_HIDE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 装配调用不得 panic，且链式用法可继续设置 stdio（两种命令类型都覆盖）。
    #[test]
    fn no_window_is_chainable() {
        let mut std_cmd = std::process::Command::new("java");
        std_cmd
            .no_window_with(0x0000_0200)
            .stdout(std::process::Stdio::null());
        assert_eq!(std_cmd.get_program().to_string_lossy(), "java");

        let mut tokio_cmd = tokio::process::Command::new("java");
        tokio_cmd.no_window().stdout(std::process::Stdio::null());
        assert_eq!(tokio_cmd.as_std().get_program().to_string_lossy(), "java");
    }

    /// 隐藏控制台引导必须幂等（测试进程已有控制台 → 走"已存在直接返回"分支，
    /// 不会隐藏终端，也不会 panic）。
    #[test]
    fn hidden_console_bootstrap_is_idempotent() {
        ensure_hidden_console();
        ensure_hidden_console();
    }

    /// 关键保守语义：**已有控制台时不得改动它**——否则 dev 模式（从终端启动）
    /// 会隐藏用户的终端窗口。无控制台环境（CI 等）跳过该断言。
    #[cfg(target_os = "windows")]
    #[test]
    fn hidden_console_keeps_existing_console() {
        #[link(name = "Kernel32")]
        extern "system" {
            fn GetConsoleWindow() -> *mut std::ffi::c_void;
        }
        let before = unsafe { GetConsoleWindow() };
        ensure_hidden_console();
        let after = unsafe { GetConsoleWindow() };
        if !before.is_null() {
            assert_eq!(before, after, "已有控制台时不得重新分配或隐藏");
        }
    }
}
