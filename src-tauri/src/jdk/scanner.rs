use crate::jdk::error::JdkError;
use crate::jdk::model::JavaEnv;
use std::path::Path;

pub fn scan_jdk() -> Result<JavaEnv, JdkError> {
    let java_home = std::env::var("JAVA_HOME").map_err(|_| JdkError::JavaHomeNotSet)?;

    let java_home_path = Path::new(&java_home);
    if !java_home_path.exists() {
        return Err(JdkError::JavaHomeInvalid(java_home));
    }

    java_bin_handler(java_home_path, false)
}

pub fn scan_custom_dir(path: &str) -> Result<Vec<JavaEnv>, JdkError> {
    let mut envs = vec![];
    let dir = Path::new(path);
    if !dir.exists() {
        return Err(JdkError::TargetDirNotFound);
    }
    if let Ok(env) = java_bin_handler(dir, false) {
        envs.push(env);
    }
    for entry in std::fs::read_dir(&path)? {
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

fn java_bin_handler(path: &Path, is_bin_dir: bool) -> Result<JavaEnv, JdkError> {
    let java_home = if is_bin_dir {
        path.parent().unwrap_or(&path)
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
    Ok(JavaEnv {
        path: java_home.to_string_lossy().to_string(),
        version,
        vendor: detect_vendor(&version_str),
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
