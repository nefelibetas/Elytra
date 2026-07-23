use std::fmt::Formatter;

#[derive(Debug)]
pub enum JdkError {
    DetectFailed(String),
    ParseFailed(String),
    TargetDirNotFound,
    JavaEnvNotFound(String),
    IoError(String),
}

impl std::fmt::Display for JdkError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            JdkError::DetectFailed(p) => write!(f, "Java detect failed: {}", p),
            JdkError::ParseFailed(p) => write!(f, "Java version parse failed: {}", p),
            JdkError::TargetDirNotFound => write!(f, "Target directory not found"),
            JdkError::JavaEnvNotFound(p) => write!(f, "Java environment not found in dir: {}", p),
            JdkError::IoError(p) => write!(f, "IO failure: {}", p),
        }
    }
}

impl From<std::io::Error> for JdkError {
    fn from(err: std::io::Error) -> JdkError {
        JdkError::IoError(err.to_string())
    }
}
