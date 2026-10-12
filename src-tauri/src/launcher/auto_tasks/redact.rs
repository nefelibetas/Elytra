//! 崩溃包脱敏：抹掉凭据、用户名与路径中的用户名。
//!
//! 崩溃包会被用户直接发给他人排查，凭据/昵称/家目录用户名属于必须清除的内容。
//! 全部为纯文本级替换（不依赖正则），保证可测且幂等。

/// 需要抹掉值的 JSON 键（游戏与启动器日志里出现过的凭据字段）。
const TOKEN_KEYS: [&str; 6] = [
    "accessToken",
    "clientToken",
    "sessionId",
    "access_token",
    "client_token",
    "refresh_token",
];

const MASK: &str = "<redacted>";

/// 脱敏入口：凭据 → `--username` 参数 → 家目录路径。
pub fn redact(text: &str, username: &str) -> String {
    let out = redact_json_secrets(text);
    let out = redact_username_arg(&out);
    redact_home_paths(&out, username)
}

/// 抹掉 `"accessToken": "xxx"` 这类键的值（保留键名，便于排查结构）。
fn redact_json_secrets(text: &str) -> String {
    let mut out = text.to_string();
    for key in TOKEN_KEYS {
        let needle = format!("\"{key}\"");
        let mut from = 0;
        while let Some(pos) = out[from..].find(&needle) {
            let key_end = from + pos + needle.len();
            let Some(vstart) = value_start(&out, key_end) else {
                from = key_end;
                continue;
            };
            let Some(vend) = value_end(&out, vstart) else {
                from = vstart;
                continue;
            };
            out.replace_range(vstart..vend, MASK);
            from = vstart + MASK.len();
        }
    }
    out
}

/// 键名之后找值的起始引号：跳过空白与冒号，要求下一个字符是 `"`。
fn value_start(text: &str, key_end: usize) -> Option<usize> {
    let rest = text.get(key_end..)?;
    let trimmed = rest.trim_start();
    if !trimmed.starts_with(':') {
        return None;
    }
    let after_colon = trimmed[1..].trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }
    Some(text.len() - after_colon.len() + 1)
}

/// 字符串值的结束引号（处理 `\"` 转义）。
fn value_end(text: &str, vstart: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = vstart;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// 抹掉 `--username <值>`（启动命令行会出现在日志里）。
fn redact_username_arg(text: &str) -> String {
    let needle = "--username";
    let mut out = text.to_string();
    let mut from = 0;
    while let Some(pos) = out[from..].find(needle) {
        let arg_end = from + pos + needle.len();
        let rest = &out[arg_end..];
        let spaces = rest.len() - rest.trim_start().len();
        let after = &rest[spaces..];
        if spaces == 0 || after.is_empty() {
            from = arg_end;
            continue;
        }
        let value_len = after
            .find(|c: char| c.is_whitespace() || c == '"')
            .unwrap_or(after.len());
        if value_len == 0 {
            from = arg_end;
            continue;
        }
        let vstart = arg_end + spaces;
        out.replace_range(vstart..vstart + value_len, MASK);
        from = vstart + MASK.len();
    }
    out
}

/// 抹掉家目录路径里的用户名（三平台：`C:\Users\x`、`/home/x`、`/Users/x`）。
fn redact_home_paths(text: &str, username: &str) -> String {
    if username.is_empty() {
        return text.to_string();
    }
    let mut out = text.to_string();
    for prefix in ["C:\\Users\\", "c:\\Users\\", "/home/", "/Users/"] {
        out = out.replace(&format!("{prefix}{username}"), &format!("{prefix}{MASK}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_access_token_json() {
        let src = r#"{"accessToken":"eyJhbGciOi.J9xyz","other":"keep"}"#;
        let out = redact(src, "");
        assert!(!out.contains("eyJhbGciOi.J9xyz"));
        assert!(out.contains(r#""accessToken":"<redacted>""#));
        assert!(out.contains(r#""other":"keep""#), "非凭据字段不得改动");
    }

    #[test]
    fn redacts_token_with_spaces_and_escapes() {
        let src = r#"{ "accessToken" : "ab\"cd" }"#;
        let out = redact(src, "");
        assert!(!out.contains("ab\\\"cd"), "转义引号不应截断值");
        assert!(out.contains(MASK));
    }

    #[test]
    fn redacts_multiple_occurrences() {
        let src = "{\"sessionId\": \"AAA\"}\n{\"sessionId\": \"BBB\"}";
        let out = redact(src, "");
        assert!(!out.contains("AAA") && !out.contains("BBB"));
        assert_eq!(out.matches(MASK).count(), 2);
    }

    #[test]
    fn redacts_username_argument() {
        let src = "java -Xmx2G --username Steve --version 1.20.1";
        let out = redact(src, "");
        assert!(!out.contains("Steve"));
        assert!(out.contains("--username <redacted>"));
        assert!(out.contains("--version 1.20.1"), "后续参数不受影响");
    }

    #[test]
    fn redacts_home_paths_three_platforms() {
        for (src, expected_absent) in [
            (r"C:\Users\MuQiaoYu\.minecraft", "MuQiaoYu"),
            ("/home/MuQiaoYu/.minecraft", "MuQiaoYu"),
            ("/Users/MuQiaoYu/.minecraft", "MuQiaoYu"),
        ] {
            let out = redact(src, "MuQiaoYu");
            assert!(!out.contains(expected_absent), "应抹掉 {src}");
            assert!(out.contains(MASK));
        }
    }

    #[test]
    fn keeps_unrelated_text() {
        let src = "---- Minecraft Crash Report ----\njava.lang.OutOfMemoryError";
        assert_eq!(redact(src, "Steve"), src);
    }

    #[test]
    fn idempotent() {
        let src = r#"{"accessToken":"secret"}`--username Steve`C:\Users\Steve\x"#;
        let once = redact(src, "Steve");
        let twice = redact(&once, "Steve");
        assert_eq!(once, twice, "脱敏应幂等");
    }
}
