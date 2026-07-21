use std::fmt::Formatter;

#[derive(Debug)]
pub enum JdkError {
    // 从环境变量读取JAVA环境
    JavaHomeNotSet,
    JavaHomeInvalid(String),
    // 存在java环境, 但读取解析期间出现问题
    DetectFailed(String),
    ParseFailed(String),
    // 指定目录检测出现问题
    TargetDirNotFound,
    JavaEnvNotFound(String),
    // IO错误
    IoError(String),
}

impl std::fmt::Display for JdkError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            JdkError::JavaHomeNotSet => write!(f, "JAVA_HOME not set"),
            JdkError::JavaHomeInvalid(p) => write!(f, "JAVA_HOME invalid: {}", p),
            JdkError::DetectFailed(p) => write!(f, "Java detect failed: {}", p),
            JdkError::ParseFailed(p) => write!(f, "Java version parse failed: {}", p),
            JdkError::TargetDirNotFound => write!(f, "Target directory not found"),
            JdkError::JavaEnvNotFound(p) => write!(f, "Java environment not found in dir: {}", p),
            JdkError::IoError(p) => write!(f, "read dir file failure: {}", p),
        }
    }
}

impl From<std::io::Error> for JdkError {
    fn from(err: std::io::Error) -> JdkError {
        JdkError::IoError(err.to_string())
    }
}
