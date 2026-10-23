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

/// 여러 조각을 한 줄로. NBT 목록은 한 종류만 담으므로 문자열 조각도 `{text}`로 감쌉니다.
pub fn join(parts: Vec<Tag>) -> Tag {
    let extra = parts
        .into_iter()
        .map(|t| match t {
            Tag::String(s) => Tag::Compound(vec![("text".into(), Tag::String(s))]),
            other => other,
        })
        .collect();
    Tag::Compound(vec![
        ("text".into(), Tag::String(String::new())),
        ("extra".into(), Tag::List(extra)),
    ])
}

/// 조각(색 입힌 것)에 마우스를 올리면 보일 글과, 누르면 채팅 창에 넣을 글을 붙입니다.
pub fn interactive(tag: Tag, hover: Option<&str>, suggest: Option<&str>) -> Tag {
    with_hover(tag, hover.map(|h| Tag::String(h.to_owned())), suggest)
}

/// [`interactive`]와 같되 마우스를 올리면 보일 것이 색 입힌 글(텍스트 컴포넌트).
pub fn with_hover(tag: Tag, hover: Option<Tag>, suggest: Option<&str>) -> Tag {
    let mut fields = match tag {
        Tag::Compound(f) => f,
        Tag::String(s) => vec![("text".into(), Tag::String(s))],
        other => return other,
    };
    if let Some(hover) = hover {
        fields.push((
            "hover_event".into(),
            Tag::Compound(vec![
                ("action".into(), Tag::String("show_text".into())),
                ("value".into(), hover),
            ]),
        ));
    }
    if let Some(c) = suggest {
        fields.push((
            "click_event".into(),
            Tag::Compound(vec![
                ("action".into(), Tag::String("suggest_command".into())),
                ("command".into(), Tag::String(c.to_owned())),
            ]),
        ));
    }
    Tag::Compound(fields)
}

/// 누르면 주소를 여는 조각.
pub fn open_url(tag: Tag, url: &str) -> Tag {
    let mut fields = match tag {
        Tag::Compound(f) => f,
        Tag::String(s) => vec![("text".into(), Tag::String(s))],
        other => return other,
    };
    fields.push((
        "click_event".into(),
        Tag::Compound(vec![
            ("action".into(), Tag::String("open_url".into())),
            ("url".into(), Tag::String(url.to_owned())),
        ]),
    ));
    Tag::Compound(fields)
}

/// 텍스트 컴포넌트의 글만 이어 붙입니다 (설명·꾸밈 빼고; 로그·테스트용).
pub fn plain_text(tag: &Tag) -> String {
    match tag {
        Tag::String(s) => s.clone(),
        Tag::Compound(f) => {
            let mut out = String::new();
            for (k, v) in f {
                match (k.as_str(), v) {
                    ("text", Tag::String(s)) => out.push_str(s),
                    ("extra", Tag::List(parts)) => {
                        out.extend(parts.iter().map(plain_text));
                    }
                    _ => {}
                }
            }
            out
        }
        _ => String::new(),
    }
}
