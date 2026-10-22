//! 손으로 쓰는 JSON에 필요한 최소한.

/// JSON 문자열 리터럴 안에 넣을 수 있도록 이스케이프합니다 (따옴표는 붙이지 않음).
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// 이미 이스케이프된 문자열로 `{"text":"..."}` 컴포넌트를 만듭니다.
pub fn text_component(escaped: &str) -> String {
    format!(r#"{{"text":"{escaped}"}}"#)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_specials() {
        assert_eq!(escape("a\"b\\c\nd\u{1}"), r#"a\"b\\c\nd\u0001"#);
    }
}
