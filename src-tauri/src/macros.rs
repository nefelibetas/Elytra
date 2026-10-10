/// 错误上下文宏: 短文案 + 错误原因, 中间换行
/// 形态1: `common_err_ctx!("短文案")`，必须为纯文本
/// 形态2: `common_err_ctx!("fmt文本", arg1, arg2, ...)`，可格式化
macro_rules! common_err_ctx {
    ($fmt: literal, $($arg: expr),* $(,)?) => {
        |e| format!("{}\n{}", format!($fmt, $($arg)*), e)
    };
    ($ctx: expr) => {
        |e| format!("{}\n{}", $ctx, e)
    };
}

/// 快速返回错误
/// bail!("{err_prefix}失败\nHTTP {}", resp.status())
macro_rules! bail {
    ($($t: tt)*) => {
        return Err(format!($($t)*))
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
}
