// ── 平台级运行调节：GPU 偏好 / 内存优化 / 进程优先级 ──
// 纯逻辑无命令依赖，由 spawn_game 在启动前后调用。

use crate::config::launcher::model::ProcessPriority;

/// 设置 Windows DirectX 用户 GPU 偏好（HKCU\Software\Microsoft\DirectX\UserGpuPreferences）。
/// 开启：java.exe → GpuPreference=2;（高性能独显）；关闭：删除该项。
pub fn apply_gpu_preference(enabled: bool) {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "Advapi32")]
        extern "system" {
            fn RegCreateKeyExW(
                h_key: *mut std::ffi::c_void,
                lp_sub_key: *const u16,
                reserved: u32,
                lp_class: *const u16,
                dw_options: u32,
                sam_desired: u32,
                lp_security_attributes: *mut std::ffi::c_void,
                phk_result: *mut *mut std::ffi::c_void,
                lpdw_disposition: *mut u32,
            ) -> i32;
            fn RegSetValueExW(
                h_key: *mut std::ffi::c_void,
                lp_value_name: *const u16,
                reserved: u32,
                dw_type: u32,
                lp_data: *const u8,
                cb_data: u32,
            ) -> i32;
            fn RegDeleteValueW(h_key: *mut std::ffi::c_void, lp_value_name: *const u16) -> i32;
            fn RegCloseKey(h_key: *mut std::ffi::c_void) -> i32;
        }
        const HKEY_CURRENT_USER: *mut std::ffi::c_void = 0x80000001usize as _;
        const KEY_SET_VALUE: u32 = 0x0002;
        const REG_SZ: u32 = 1;
        const KEY_PATH: &[u16] = &[
            'S' as u16,
            'o' as u16,
            'f' as u16,
            't' as u16,
            'w' as u16,
            'a' as u16,
            'r' as u16,
            'e' as u16,
            '\\' as u16,
            'M' as u16,
            'i' as u16,
            'c' as u16,
            'r' as u16,
            'o' as u16,
            's' as u16,
            'o' as u16,
            'f' as u16,
            't' as u16,
            '\\' as u16,
            'D' as u16,
            'i' as u16,
            'r' as u16,
            'e' as u16,
            'c' as u16,
            't' as u16,
            'X' as u16,
            '\\' as u16,
            'U' as u16,
            's' as u16,
            'e' as u16,
            'r' as u16,
            'G' as u16,
            'p' as u16,
            'u' as u16,
            'P' as u16,
            'r' as u16,
            'e' as u16,
            'f' as u16,
            'e' as u16,
            'r' as u16,
            'e' as u16,
            'n' as u16,
            'c' as u16,
            'e' as u16,
            's' as u16,
            0,
        ];
        const VALUE_NAME: &[u16] = &[
            'j' as u16, 'a' as u16, 'v' as u16, 'a' as u16, '.' as u16, 'e' as u16, 'x' as u16,
            'e' as u16, 0,
        ];
        unsafe {
            let mut key: *mut std::ffi::c_void = std::ptr::null_mut();
            let r = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                KEY_PATH.as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_SET_VALUE,
                std::ptr::null_mut(),
                &mut key,
                std::ptr::null_mut(),
            );
            if r != 0 || key.is_null() {
                return;
            }
            if enabled {
                const DATA: &[u16] = &[
                    'G' as u16, 'p' as u16, 'u' as u16, 'P' as u16, 'r' as u16, 'e' as u16,
                    'f' as u16, 'e' as u16, 'r' as u16, 'e' as u16, 'n' as u16, 'c' as u16,
                    'e' as u16, '=' as u16, '2' as u16, ';' as u16, 0,
                ];
                let _ = RegSetValueExW(
                    key,
                    VALUE_NAME.as_ptr(),
                    0,
                    REG_SZ,
                    DATA.as_ptr() as *const u8,
                    (DATA.len() * 2) as u32,
                );
            } else {
                let _ = RegDeleteValueW(key, VALUE_NAME.as_ptr());
            }
            let _ = RegCloseKey(key);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = enabled;
    }
}

/// 启动前内存优化：将启动器进程工作集降至最低（Windows SetProcessWorkingSetSize）。
pub fn optimize_launcher_memory() {
    #[cfg(target_os = "windows")]
    unsafe {
        #[link(name = "Kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn SetProcessWorkingSetSize(
                h_process: *mut std::ffi::c_void,
                dw_minimum: usize,
                dw_maximum: usize,
            ) -> i32;
        }
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
    #[cfg(not(target_os = "windows"))]
    {}
}

/// 设置游戏进程优先级（Windows: SetPriorityClass；Linux: setpriority nice 值）。
pub fn set_process_priority(pid: u32, priority: ProcessPriority) {
    use crate::config::launcher::model::ProcessPriority as P;

    #[cfg(target_os = "windows")]
    {
        let class = match priority {
            P::Normal => return,        // 中（平衡）= 系统默认，无需设置
            P::Low => 0x0000_4000,      // BELOW_NORMAL_PRIORITY_CLASS
            P::High => 0x0000_0080,     // HIGH_PRIORITY_CLASS
            P::RealTime => 0x0000_0100, // REALTIME_PRIORITY_CLASS
        };
        #[link(name = "Kernel32")]
        extern "system" {
            fn OpenProcess(
                dw_desired_access: u32,
                b_inherit_handle: i32,
                dw_process_id: u32,
            ) -> *mut std::ffi::c_void;
            fn SetPriorityClass(h_process: *mut std::ffi::c_void, dw_priority_class: u32) -> i32;
            fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
        }
        const PROCESS_SET_INFORMATION: u32 = 0x0200;
        unsafe {
            let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
            if !handle.is_null() {
                let _ = SetPriorityClass(handle, class);
                let _ = CloseHandle(handle);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        // nice 值映射（-20 最高优先级，19 最低；Normal = 0 默认无需设置）
        let nice = match priority {
            P::Normal => return,
            P::Low => 10,
            P::High => -5,
            P::RealTime => -10, // 负 nice 需要 CAP_SYS_NICE/root，失败静默忽略
        };
        extern "C" {
            fn setpriority(which: i32, who: u32, prio: i32) -> i32;
        }
        const PRIO_PROCESS: i32 = 0;
        unsafe {
            let _ = setpriority(PRIO_PROCESS, pid, nice);
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = (pid, priority);
    }
}
