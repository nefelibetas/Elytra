// ── 内存与 GC 策略：JVM 堆分配 + GC 参数 ──
// 纯策略函数（输入配置/目录 → 输出参数），由 args.rs 的 build_launch_command 调用。

use std::path::Path;

use crate::launcher::model::GcType;

/// 根据 GC 类型返回对应的 JVM 参数
pub fn gc_args(gc: &GcType) -> Vec<String> {
    match gc {
        GcType::PreferZgc => vec![
            "-XX:+UnlockExperimentalVMOptions".into(),
            "-XX:+UseZGC".into(),
            "-XX:+PerfDisableSharedMem".into(),
        ],
        GcType::PreferGenerationalZgc => vec![
            "-XX:+UnlockExperimentalVMOptions".into(),
            "-XX:+UseZGC".into(),
            "-XX:+ZGenerational".into(),
            "-XX:+PerfDisableSharedMem".into(),
        ],
        GcType::StandardG1Gc => vec![
            "-XX:+UnlockExperimentalVMOptions".into(),
            "-XX:+UseG1GC".into(),
            "-XX:+PerfDisableSharedMem".into(),
        ],
        GcType::TunedG1Gc => vec![
            "-XX:+UnlockExperimentalVMOptions".into(),
            "-XX:+UseG1GC".into(),
            "-XX:G1NewSizePercent=20".into(),
            "-XX:G1ReservePercent=20".into(),
            "-XX:G1HeapRegionSize=32M".into(),
            "-XX:MaxGCPauseMillis=50".into(),
            "-XX:+PerfDisableSharedMem".into(),
        ],
        GcType::Custom => vec![],
    }
}

/// 统计版本目录 mods/ 下的 mod 数量（.jar 文件数）。
fn count_mods(version_dir: &Path) -> u32 {
    std::fs::read_dir(version_dir.join("mods"))
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "jar").unwrap_or(false))
                .count() as u32
        })
        .unwrap_or(0)
}

/// 自动推测游戏内存（MB）：按剩余内存 + mod 数量动态分配。
///
/// 策略（-Xmx 是堆上限，给多不预占，核心约束是别超过系统剩余）：
/// - 基准比例 60%（0 mod）；每 mod +0.3%，60 个 mod 封顶（最多 78%）
/// - 封顶剩余内存的 85%（留呼吸空间防 OOM）；下限 2GB
pub fn guess_auto_memory(version_dir: &Path) -> u32 {
    auto_alloc_memory(system_memory_mb().1, count_mods(version_dir))
}

/// 系统物理内存（MB）：(总内存, 可用内存)。三平台统一入口，启动内存显示与自动分配共用。
pub fn system_memory_mb() -> (u64, u64) {
    #[cfg(target_os = "windows")]
    // SAFETY: MEMORYSTATUSEX 为 #[repr(C)]，dw_length 已置为结构体真实大小；
    // 传入的指针指向本栈帧上已完整初始化的 status，GlobalMemoryStatusEx 只会写入该结构体内，返回值已检查。
    unsafe {
        #[allow(clippy::upper_case_acronyms)]
        type DWORD = u32;
        #[allow(clippy::upper_case_acronyms)]
        type BOOL = i32;
        #[repr(C)]
        #[allow(clippy::upper_case_acronyms)]
        struct MEMORYSTATUSEX {
            dw_length: DWORD,
            dw_memory_load: DWORD,
            ull_total_phys: u64,
            ull_avail_phys: u64,
            ull_total_page_file: u64,
            ull_avail_page_file: u64,
            ull_total_virtual: u64,
            ull_avail_virtual: u64,
            ull_avail_extended_virtual: u64,
        }
        extern "system" {
            fn GlobalMemoryStatusEx(lp_buffer: *mut MEMORYSTATUSEX) -> BOOL;
        }
        let mut status = MEMORYSTATUSEX {
            dw_length: size_of::<MEMORYSTATUSEX>() as DWORD,
            dw_memory_load: 0,
            ull_total_phys: 0,
            ull_avail_phys: 0,
            ull_total_page_file: 0,
            ull_avail_page_file: 0,
            ull_total_virtual: 0,
            ull_avail_virtual: 0,
            ull_avail_extended_virtual: 0,
        };
        if GlobalMemoryStatusEx(&mut status) != 0 {
            (
                status.ull_total_phys / 1024 / 1024,
                status.ull_avail_phys / 1024 / 1024,
            )
        } else {
            (16384, 8192)
        }
    }
    #[cfg(target_os = "linux")]
    {
        // /proc/meminfo: MemTotal / MemAvailable（kB）——MemAvailable 是真实可用（含可回收缓存）
        let content = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let mut total_kb = 0u64;
        let mut avail_kb = 0u64;
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                total_kb = meminfo_kb(rest);
            } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
                avail_kb = meminfo_kb(rest);
            }
        }
        (total_kb / 1024, avail_kb / 1024)
    }
    #[cfg(target_os = "macos")]
    {
        // sysctl hw.memsize（字节）→ 总内存；可用内存取总内存 60% 保守估算
        // （macOS 无直接的 available sysctl，free+inactive+speculative 组合复杂，
        // 60% 与 Windows 兜底同保守级别，够自动分配用）
        let total = sysctl_u64("hw.memsize").map(|b| b / 1024 / 1024).unwrap_or(16384);
        (total, total * 60 / 100)
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        (16384, 8192)
    }
}

/// macOS：读 sysctl 数值（如 hw.memsize）。
#[cfg(target_os = "macos")]
fn sysctl_u64(name: &str) -> Option<u64> {
    let mut out = 0u64;
    let mut len = std::mem::size_of::<u64>() as usize;
    let c_name = std::ffi::CString::new(name).ok()?;
    // SAFETY: out 是本栈帧上的 u64，len 为其字节长度，c_name 是有效的 NUL 结尾 C 字符串；
    // sysctlbyname 按 len 写入 out，返回值已检查，仅在 ret == 0 时读取 out。
    let ret = unsafe {
        libc::sysctlbyname(
            c_name.as_ptr(),
            &mut out as *mut u64 as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if ret == 0 {
        Some(out)
    } else {
        None
    }
}

/// 解析 /proc/meminfo 行尾的 `123456 kB`。
#[cfg(target_os = "linux")]
fn meminfo_kb(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap_or(0)
}

/// 自动分配公式（纯函数，便于测试）：剩余内存 × 比例，封顶 85%，下限 2GB。
fn auto_alloc_memory(available_mb: u64, mods: u32) -> u32 {
    let ratio = 0.60 + 0.003 * (mods.min(60) as f64);
    let alloc = (available_mb as f64 * ratio).min(available_mb as f64 * 0.85);
    alloc.max(2048.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用临时目录模拟版本目录，写入指定数量的 mod jar。
    fn dir_with_mods(n: u32) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("elytra-mods-test-{}", std::process::id()));
        let mods = d.join("mods");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&mods).unwrap();
        for i in 0..n {
            std::fs::write(mods.join(format!("mod{i}.jar")), b"x").unwrap();
        }
        // 非 jar 文件不应计入
        std::fs::write(mods.join("config.json"), b"{}").unwrap();
        d
    }

    #[test]
    fn test_count_mods_only_jars() {
        let d = dir_with_mods(5);
        assert_eq!(count_mods(&d), 5);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_auto_memory_strategy() {
        let avail = 9216u64; // 剩余 9GB
                             // 0 mod → 60% = 5529MB
        assert_eq!(auto_alloc_memory(avail, 0), (9216.0 * 0.60) as u32);
        // 30 mod → 69%
        assert_eq!(auto_alloc_memory(avail, 30), (9216.0 * 0.69) as u32);
        // 60 mod → 78% 封顶（比例不再随 mod 增长）
        assert_eq!(auto_alloc_memory(avail, 60), (9216.0 * 0.78) as u32);
        assert_eq!(auto_alloc_memory(avail, 80), (9216.0 * 0.78) as u32);
        // 大 mod 数不超过 85% 封顶
        assert!(auto_alloc_memory(avail, 1000) <= (9216.0 * 0.85) as u32);
    }

    #[test]
    fn test_auto_memory_min_floor() {
        // 剩余 2.5GB：60% = 1536MB → 下限 2048MB；剩余 5GB 时正常分配
        assert_eq!(auto_alloc_memory(2560, 0), 2048);
        assert_eq!(auto_alloc_memory(2560, 30), 2048); // 66% 仍低于下限
        assert_eq!(auto_alloc_memory(5120, 0), (5120.0 * 0.60) as u32);
    }
}
