use std::collections::HashSet;

use crate::config::jdk::error::JdkError;
use crate::config::jdk::model::JavaEnv;
use std::path::Path;

pub fn scan_custom_dir(path: &str) -> Result<Vec<JavaEnv>, JdkError> {
    let mut envs = vec![];
    let dir = Path::new(path);
    if !dir.exists() {
        return Err(JdkError::TargetDirNotFound);
    }
    if let Ok(env) = java_bin_handler(dir, false) {
        envs.push(env);
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let filename = entry.file_name();
        if file_type.is_dir() {
            let name_is_bin = filename == "bin";
            if let Ok(env) = java_bin_handler(&entry.path(), name_is_bin) {
                envs.push(env);
            }
        }
    }
    if envs.is_empty() {
        Err(JdkError::JavaEnvNotFound(path.to_string()))
    } else {
        Ok(envs)
    }
}

pub(crate) fn java_bin_handler(path: &Path, is_bin_dir: bool) -> Result<JavaEnv, JdkError> {
    let java_home = if is_bin_dir {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    let java_bin_path = if is_bin_dir {
        path.join(if cfg!(windows) { "java.exe" } else { "java" })
    } else {
        path.join("bin")
            .join(if cfg!(windows) { "java.exe" } else { "java" })
    };
    if !java_bin_path.exists() {
        return Err(JdkError::TargetDirNotFound);
    }

    let output = std::process::Command::new(&java_bin_path)
        .arg("-version")
        .output()
        .map_err(|e| JdkError::DetectFailed(e.to_string()))?;

    let version_str = String::from_utf8_lossy(&output.stderr);
    let version = parse_version(&version_str)
        .ok_or_else(|| JdkError::ParseFailed(version_str.to_string()))?;
    let arch = if version_str.contains("64-Bit") {
        "64-Bit".into()
    } else {
        "32-Bit".into()
    };

    Ok(JavaEnv {
        path: java_home.to_string_lossy().to_string(),
        version,
        vendor: detect_vendor(&version_str),
        arch,
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
    } else if output.contains("OpenJDK") {
        "OpenJDK".into()
    } else if output.contains("Java(TM)") {
        "Oracle".into()
    } else if output.contains("Eclipse") || output.contains("Adoptium") {
        "Eclipse Temurin".into()
    } else {
        "Unknown".into()
    }
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn scan_one(dir: &Path, found: &mut Vec<JavaEnv>, seen: &mut HashSet<String>) {
    if let Ok(env) = java_bin_handler(dir, false) {
        if seen.insert(env.path.clone()) && env.arch == "64-Bit" {
            found.push(env);
        }
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
    Vec::new()
}

pub fn auto_scan() -> Vec<JavaEnv> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();

    for dir_str in candidate_dirs() {
        let dir = Path::new(&dir_str);
        if !dir.exists() {
            continue;
        }

        scan_one(dir, &mut found, &mut seen);

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scan_one(&path, &mut found, &mut seen);
                }
            }
        }
    }

    found
}
