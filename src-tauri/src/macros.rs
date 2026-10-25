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

/// 可本地化消息（非 `Result` 路径，如事件负载/状态文本）：首行 `@i18n:<key>`（可带 `?k=v` 参数，前端注入
/// i18n 插值），第二行中文兜底，第三行起技术细节。两段都是 format 模板，可用内联捕获。
macro_rules! keyed_msg {
    ($key: literal, $($t: tt)*) => {
        format!("@i18n:{}\n{}", format!($key), format!($($t)*))
    };
    ($key: expr, $($t: tt)*) => {
        format!("@i18n:{}\n{}", $key, format!($($t)*))
    };
}

#[cfg(test)]
mod tests {
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
        let err: Result<(), String> =
            (|| bail_key!("errors.download.failed", "下载失败\nHTTP 404"))();
        assert_eq!(
            err.unwrap_err(),
            "@i18n:errors.download.failed\n下载失败\nHTTP 404"
        );
    }

    #[test]
    fn keyed_msg_interpolates_key_params_and_body() {
        let need = 17;
        let cur = 8;
        assert_eq!(
            keyed_msg!(
                "errors.jdk.too_old?need={need}&cur={cur}",
                "该版本需要 Java {need}（当前 {cur}）\n详情"
            ),
            "@i18n:errors.jdk.too_old?need=17&cur=8\n该版本需要 Java 17（当前 8）\n详情"
        );
    }
}
