//! 텍스트 컴포넌트. Play·Configuration 단계에서는 NBT로, Login 단계에서는 JSON으로 갑니다.

use crate::protocol::nbt::Tag;

/// 색 없는 문자열. NBT에서는 문자열 태그 하나가 곧 `{"text": ...}`입니다.
pub fn plain(text: &str) -> Tag {
    Tag::String(text.to_owned())
}

/// 색을 입힌 문자열. `color`는 `"gold"` 같은 이름 또는 `"#RRGGBB"`.
pub fn colored(text: &str, color: &str) -> Tag {
    Tag::Compound(vec![
        ("text".into(), Tag::String(text.to_owned())),
        ("color".into(), Tag::String(color.to_owned())),
    ])
}

/// NBT 바이트로.
pub fn to_nbt(tag: &Tag) -> Vec<u8> {
    let mut out = Vec::new();
    tag.write_root(&mut out);
    out
}

/// `&a`, `&l` 같은 색·꾸밈 코드를 `§`로 바꿉니다 (클라이언트가 글자 안의 `§` 코드를 그립니다).
/// `&&`는 `&` 그대로.
pub fn legacy_colors(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&'
            && let Some(&n) = chars.peek()
        {
            if n == '&' {
                chars.next();
                out.push('&');
                continue;
            }
            if "0123456789abcdefklmnor".contains(n.to_ascii_lowercase()) {
                chars.next();
                out.push('§');
                out.push(n.to_ascii_lowercase());
                continue;
            }
        }
        out.push(c);
    }
    out
}
