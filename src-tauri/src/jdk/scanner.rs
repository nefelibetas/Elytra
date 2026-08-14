use crate::jdk::model::JdkInfo;
use std::collections::HashSet;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::Path;

pub fn scan_custom_dir(path: &str) -> Result<Vec<JdkInfo>, String> {
    let dir = Path::new(path);
    if !dir.exists() {
        return Err("目标目录不存在".into());
    }
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    scan_dir_and_children(dir, &mut found, &mut seen, false);
    if found.is_empty() {
        Err(format!("未在目录中找到 Java 环境: {}", path))
    } else {
        Ok(found)
    }
}

pub fn auto_scan() -> Vec<JdkInfo> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for dir_str in candidate_dirs() {
        let dir = Path::new(&dir_str);
        if dir.is_dir() {
            scan_dir_and_children(dir, &mut found, &mut seen, true);
        }
    }
    found
}

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
    let java_bin = crate::jdk::java_bin_path(java_home);
    if !java_bin.exists() {
        return Err(format!("未找到 Java 可执行文件: {}", java_bin.display()));
    }
    let mut cmd = std::process::Command::new(java_bin);
    cmd.arg("-version");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // 避免调用指令出现命令行窗口
    let output = cmd.output().map_err(|e| format!("Java 检测失败: {}", e))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let version = parse_version(&stderr).ok_or_else(|| format!("Java 版本解析失败: {}", stderr))?;
    Ok(JdkInfo {
        path: java_home.to_string_lossy().to_string(),
        version,
        vendor: detect_vendor(&stderr),
        bits: if stderr.contains("64-Bit") { 64 } else { 32 },
    })
}

fn parse_version(output: &str) -> Option<String> {
    let start = output.find('"')?;
    let end = output[start + 1..].find('"')?;
    Some(output[start + 1..start + 1 + end].to_string())
}

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
