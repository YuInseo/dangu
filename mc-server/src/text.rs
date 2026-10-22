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
