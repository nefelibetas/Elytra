pub fn parse_seed(text: &str) -> Result<i64, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err(keyed_msg!(
            "errors.seedmap.seedEmpty",
            "种子不能为空\n请输入数字或任意字符串"
        ));
    }
    if let Ok(v) = t.parse::<i64>() {
        return Ok(v);
    }
    // MC `WorldOptions.parseSeed`：数字解析失败时用 Java String.hashCode。
    let mut h: i32 = 0;
    for unit in t.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(unit as i32);
    }
    Ok(h as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_seed_keeps_i64() {
        assert_eq!(
            parse_seed("7354360020768317315").unwrap(),
            7354360020768317315
        );
        assert_eq!(
            parse_seed("-3829811542736183482").unwrap(),
            -3829811542736183482
        );
    }

    #[test]
    fn string_seed_uses_java_hash() {
        assert_eq!(parse_seed("hello").unwrap(), 99162322);
        assert!(parse_seed("").is_err());
    }
}
