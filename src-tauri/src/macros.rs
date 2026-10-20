/// 错误上下文宏：短文案 + 换行 + 错误原因。
/// 形态1 `common_err_ctx!("纯文本")`；形态2 `common_err_ctx!("fmt", args...)` 可格式化。
macro_rules! common_err_ctx {
    ($fmt: literal, $($arg: expr),* $(,)?) => {
        |e| format!("{}\n{}", format!($fmt, $($arg)*), e)
    };
    ($ctx: expr) => {
        |e| format!("{}\n{}", $ctx, e)
    };
}

/// 快速返回错误（`bail!(...)` → `return Err(format!(...))`）
macro_rules! bail {
    ($($t: tt)*) => {
        return Err(format!($($t)*))
    };
}

/// 可本地化错误上下文：首行 `@i18n:<key>`，第二行中文兜底，第三行起技术细节。
macro_rules! common_err_ctx_key {
    ($key: literal, $fmt: literal, $($arg: expr),* $(,)?) => {
        |e| format!("@i18n:{}\n{}\n{}", $key, format!($fmt, $($arg)*), e)
    };
    ($key: literal, $ctx: expr) => {
        |e| format!("@i18n:{}\n{}\n{}", $key, $ctx, e)
    };
}

/// 可本地化 `bail`：首行 key + 中文兜底（可含 `\n` 技术细节）。
macro_rules! bail_key {
    ($key: literal, $($t: tt)*) => {
        return Err(format!("@i18n:{}\n{}", $key, format!($($t)*)))
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn ctx_plain_and_formatted() {
        assert_eq!(common_err_ctx!("下载失败")("boom"), "下载失败\nboom");
        let p = "header";
        assert_eq!(
            common_err_ctx!("{}, 下载失败", p)("boom"),
            "header, 下载失败\nboom"
        );
    }

    #[test]
    fn plain_form_keeps_braces_as_literal() {
        assert_eq!(common_err_ctx!("失败{}")("boom"), "失败{}\nboom");
    }

    #[test]
    fn keyed_ctx_emits_protocol_and_fallback() {
        assert_eq!(
            common_err_ctx_key!("errors.download.failed", "下载失败")("boom"),
            "@i18n:errors.download.failed\n下载失败\nboom"
        );
        assert_eq!(
            common_err_ctx_key!("errors.download.failed", "下载 {} 失败", "x")("boom"),
            "@i18n:errors.download.failed\n下载 x 失败\nboom"
        );
    }

    #[test]
    fn keyed_bail_keeps_detail_lines() {
        let err: Result<(), String> = (|| bail_key!("errors.download.failed", "下载失败\nHTTP 404"))();
        assert_eq!(
            err.unwrap_err(),
            "@i18n:errors.download.failed\n下载失败\nHTTP 404"
        );
    }
}
