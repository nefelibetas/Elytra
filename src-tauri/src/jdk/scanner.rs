use std::collections::HashSet;
use std::path::Path;

use crate::jdk::model::JdkInfo;
#[cfg(target_os = "windows")]
use crate::util::exe_dir;
use crate::util::java_bin_path;
use crate::util::NoWindow as _;

pub fn scan_custom_dir(path: &str) -> Result<Vec<JdkInfo>, String> {
    let dir = Path::new(path);
    if !dir.exists() {
        return Err(keyed_msg!(
            "errors.jdk.dirMissing",
            "目标目录不存在\n{path}"
        ));
    }
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    // 指定目录以及往下一层目录内扫描
    scan_dir_and_children(dir, &mut found, &mut seen, false);
    if found.is_empty() {
        Err(keyed_msg!(
            "errors.jdk.noJavaFound",
            "未在目录中找到 Java 环境\n{path}"
        ))
    } else {
        Ok(found)
    }
}

// 自动扫描：从常用目录扫描jdk
pub fn auto_scan() -> Vec<JdkInfo> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let mut dirs = candidate_dirs();
    // Elytra 自下载的 runtime 跟随激活游戏目录（<minecraft_dir>/runtime），exe 旁只是历史假定
    let mc_runtime = crate::persist::downloader_config()
        .minecraft_dir_abs()
        .join("runtime");
    dirs.push(mc_runtime.to_string_lossy().to_string());
    for dir_str in dirs {
        let dir = Path::new(&dir_str);
        if dir.is_dir() {
            // managed runtime 至少是 runtime/{platform}/java-{major}，官方启动器还会再深一层。
            scan_dir_recursive(dir, &mut found, &mut seen, true, 4);
        }
    }
    found
}

fn scan_dir_recursive(
    dir: &Path,
    found: &mut Vec<JdkInfo>,
    seen: &mut HashSet<String>,
    only_64bit: bool,
    depth: u8,
) {
    probe_home(dir, found, seen, only_64bit);
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir_recursive(&path, found, seen, only_64bit, depth - 1);
        }
    }
}

// 扫描指定目录以及此目录下的二级目录
fn scan_dir_and_children(
    dir: &Path,
    found: &mut Vec<JdkInfo>,
    seen: &mut HashSet<String>,
    only_64bit: bool,
) {
    probe_home(dir, found, seen, only_64bit);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            probe_home(&path, found, seen, only_64bit);
        }
    }
}

fn probe_home(dir: &Path, found: &mut Vec<JdkInfo>, seen: &mut HashSet<String>, only_64bit: bool) {
    let Ok(env) = inspect(dir) else {
        return;
    };
    if seen.insert(env.path.clone()) && (!only_64bit || env.bits == 64) {
        found.push(env);
    }
}

/// 探测单个 JAVA_HOME 目录的版本信息（jdk/download.rs 的 managed runtime 注册复用）。
pub(crate) fn inspect(java_home: &Path) -> Result<JdkInfo, String> {
    let java_bin = java_bin_path(java_home);
    if !java_bin.exists() {
        bail_key!(
            "errors.jdk.failed",
            "未找到 Java 可执行文件\n{}",
            java_bin.display()
        );
    }
    let mut cmd = std::process::Command::new(java_bin);
    cmd.arg("-version");
    // java 是控制台程序：应用运行期探版本不得弹出命令行窗口
    cmd.no_window();
    let output = cmd
        .output()
        .map_err(common_err_ctx_key!("errors.jdk.scan", "Java 检测失败"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let version = parse_version(&stderr)
        .ok_or_else(|| keyed_msg!("errors.jdk.versionParse", "无法解析 Java 版本\n{stderr}"))?;
    Ok(JdkInfo {
        path: java_home.to_string_lossy().to_string(),
        version,
        vendor: detect_vendor(&stderr),
        bits: if stderr.contains("64-Bit") { 64 } else { 32 },
    })
}

// 从 java -version 输出解析带引号的版本号
fn parse_version(output: &str) -> Option<String> {
    let start = output.find('"')?;
    let end = output[start + 1..].find('"')?;
    Some(output[start + 1..start + 1 + end].to_string())
}

// jdk出品商解析
fn detect_vendor(output: &str) -> String {
    if output.contains("GraalVM") {
        "GraalVM".into()
    } else if output.contains("Temurin") || output.contains("Adoptium") {
        "Eclipse Temurin".into()
    } else if output.contains("Java(TM)") {
        "Oracle".into()
    } else if output.contains("OpenJDK") {
        "OpenJDK".into()
    } else {
        "Unknown".into()
    }
}

#[cfg(windows)]
fn candidate_dirs() -> Vec<String> {
    let mut dirs = Vec::new();
    // Elytra 自下载的 runtime 目录
    dirs.push(format!(
        r"{}\.minecraft\runtime",
        exe_dir().to_string_lossy()
    ));
    let pf = env("ProgramFiles");
    let pf86 = env("ProgramFiles(x86)");
    let appdata = env("APPDATA");
    let local = env("LOCALAPPDATA");
    let user = env("USERPROFILE");

    if let Some(ref p) = pf {
        dirs.push(format!(r"{}\Java", p));
        dirs.push(format!(r"{}\Eclipse Adoptium", p));
        dirs.push(format!(r"{}\Amazon Corretto", p));
        dirs.push(format!(r"{}\Zulu", p));
        dirs.push(format!(r"{}\Microsoft", p));
    }
    if let Some(ref a) = appdata {
        dirs.push(format!(r"{}\.minecraft\runtime", a));
        dirs.push(format!(r"{}\.hmcl\java", a));
        dirs.push(format!(r"{}\ATLauncher\runtimes\minecraft", a));
        dirs.push(format!(r"{}\ModrinthApp\meta\java_versions", a));
        dirs.push(format!(r"{}\PrismLauncher\java", a));
    }
    if let Some(ref l) = local {
        dirs.push(format!(
            r"{}\Packages\Microsoft.4297127D64EC6_8wekyb3d8bbwe\LocalCache\Local\runtime",
            l
        ));
    }
    if let Some(ref p) = pf86 {
        dirs.push(format!(r"{}\Minecraft Launcher\runtime", p));
        dirs.push(format!(r"{}\Minecraft\runtime", p));
    }
    if let Some(ref u) = user {
        dirs.push(format!(r"{}\.jdks", u));
        dirs.push(format!(r"{}\.jbr", u));
        dirs.push(format!(r"{}\curseforge\minecraft\Install\runtime", u));
    }
    if let Ok(h) = std::env::var("JAVA_HOME") {
        dirs.push(h);
    }
    if let Ok(h) = std::env::var("JDK_HOME") {
        dirs.push(h);
    }

    dirs
}

#[cfg(target_os = "linux")]
fn candidate_dirs() -> Vec<String> {
    let mut dirs = Vec::new();

    dirs.push("/usr/lib/jvm".into());

    if let Ok(home) = std::env::var("HOME") {
        dirs.push(format!("{}/.sdkman/candidates/java", home));
    }
    if let Ok(h) = std::env::var("JAVA_HOME") {
        dirs.push(h);
    }
    if let Ok(h) = std::env::var("JDK_HOME") {
        dirs.push(h);
    }

    dirs
}

#[cfg(target_os = "macos")]
fn candidate_dirs() -> Vec<String> {
    env("HOME")
        .map(|h| format!("{}/Library/Java/JavaVirtualMachines", h))
        .into_iter()
        .collect()
}

// 从环境变量种获取（仅 Windows/macOS 的 candidate_dirs 使用；Linux 分支内联 std::env::var）
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_extracts_quoted_string() {
        assert_eq!(
            parse_version("openjdk version \"17.0.11\" 2024-04-16"),
            Some("17.0.11".to_string())
        );
        assert_eq!(
            parse_version("java version \"1.8.0_202\""),
            Some("1.8.0_202".to_string())
        );
        assert_eq!(
            parse_version("java version \"25-ea\" 2025-..."),
            Some("25-ea".to_string())
        );
        assert_eq!(parse_version("no quotes here"), None);
    }

    #[test]
    fn detect_vendor_handles_real_outputs() {
        // Temurin 真实输出含 "OpenJDK Runtime Environment Temurin-..."，必须先判 Temurin
        assert_eq!(
            detect_vendor("openjdk version \"21.0.2\"\nOpenJDK Runtime Environment Temurin-21.0.2+13 (build 21.0.2+13)"),
            "Eclipse Temurin"
        );
        assert_eq!(
            detect_vendor(
                "openjdk version \"17.0.11\"\nOpenJDK Runtime Environment (build 17.0.11+9)"
            ),
            "OpenJDK"
        );
        assert_eq!(
            detect_vendor("java version \"25-ea\"\nJava(TM) SE Runtime Environment"),
            "Oracle"
        );
        assert_eq!(
            detect_vendor("openjdk version \"21\"\nGraalVM CE 21.0.2+13.1 (build ...)"),
            "GraalVM"
        );
        assert_eq!(detect_vendor("mystery runtime"), "Unknown");
    }
}
