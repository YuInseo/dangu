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

// ---- 아이템 설명(lore) 꾸미기 ----

/// 설명 기본 글자색 (바닐라의 보라 기울임 대신 읽기 쉬운 밝은 회색).
const LORE_BASE: &str = "#c9c9d6";
const LORE_LABEL: &str = "#8c8c9a";
const LORE_VALUE: &str = "#f2f3f8";

fn part(text: &str, color: &str) -> Tag {
    Tag::Compound(vec![
        ("text".into(), Tag::String(text.to_owned())),
        ("color".into(), Tag::String(color.to_owned())),
        ("italic".into(), Tag::Byte(0)),
    ])
}

fn bold(text: &str, color: &str) -> Tag {
    let Tag::Compound(mut f) = part(text, color) else {
        unreachable!()
    };
    f.push(("bold".into(), Tag::Byte(1)));
    Tag::Compound(f)
}

/// 설명 한 줄을 모양에 따라 꾸밉니다. 모든 메뉴·아이템 설명이 여기를 거칩니다.
///
/// | 줄 | 모양 |
/// |---|---|
/// | `§` 코드가 있음 (`&a`도 됨) | 그대로 (클라이언트가 색을 입힘) |
/// | `---` / `──` | 짙은 회색 가로줄 |
/// | `▶ …` `▸ …` `좌클릭…` `우클릭…` `Shift…` `누르면…` `클릭…` | 금색 `▸` + 노란 글 (할 수 있는 것) |
/// | `  …` (앞에 빈칸 둘) | 하늘색 `◆` + 흰 글 (보상·목록) |
/// | `■□` 막대 | 찬 칸 초록, 빈 칸 짙은 회색 |
/// | `이름표\t값`, `이름표  값` | 이름표 회색 + 값 흰색 |
/// | `준비 중`, `(…)` | 짙은 회색 |
/// | `보상` 같은 한 낱말 머리 | 금색 굵게 |
pub fn lore_line(line: &str) -> Tag {
    let line = &legacy_colors(line);
    if line.contains('§') {
        return part(line, LORE_BASE);
    }
    let t = line.trim();
    if t.is_empty() {
        return part("", LORE_BASE);
    }
    if t.chars().all(|c| matches!(c, '-' | '─' | '━' | '=')) && t.chars().count() >= 3 {
        return part("──────────────", "#4a4a5a");
    }
    let action = ["좌클릭", "우클릭", "Shift", "누르면", "클릭", "휠클릭"];
    if let Some(rest) = t
        .strip_prefix('▶')
        .or_else(|| t.strip_prefix('▸'))
        .or_else(|| t.strip_prefix('>'))
    {
        return join(vec![part("▸ ", "gold"), part(rest.trim_start(), "yellow")]);
    }
    if action.iter().any(|a| t.starts_with(a)) {
        return join(vec![part("▸ ", "gold"), part(t, "yellow")]);
    }
    if t == "준비 중" || (t.starts_with('(') && t.ends_with(')')) {
        return part(t, "dark_gray");
    }
    if line.starts_with("  ") {
        let (label, value) = split_label(t).unwrap_or(("", t));
        let mut v = vec![part(" ◆ ", "aqua")];
        if !label.is_empty() {
            v.push(part(&format!("{label} "), LORE_BASE));
        }
        v.push(part(value, LORE_VALUE));
        return join(v);
    }
    if t.contains('■') || t.contains('□') {
        let mut v = Vec::new();
        let mut cur = String::new();
        let mut kind = 0; // 0 글, 1 찬, 2 빈
        let flush = |v: &mut Vec<Tag>, cur: &mut String, kind: i32| {
            if !cur.is_empty() {
                let color = match kind {
                    1 => "#7be07b",
                    2 => "#3a3a48",
                    _ => LORE_VALUE,
                };
                v.push(part(cur, color));
                cur.clear();
            }
        };
        for c in t.chars() {
            let k = match c {
                '■' => 1,
                '□' => 2,
                _ => 0,
            };
            if k != kind {
                flush(&mut v, &mut cur, kind);
                kind = k;
            }
            cur.push(c);
        }
        flush(&mut v, &mut cur, kind);
        return join(v);
    }
    if let Some((label, value)) = split_label(t) {
        return join(vec![
            part(&format!("{label}  "), LORE_LABEL),
            part(value, LORE_VALUE),
        ]);
    }
    if !t.contains(' ')
        && t.chars().count() <= 6
        && ["보상", "효과", "능력치", "세트", "특혜", "설명", "가격"].contains(&t)
    {
        return bold(t, "gold");
    }
    part(t, LORE_BASE)
}

/// `이름표\t값` 또는 `이름표  값` (이름표가 짧을 때만).
fn split_label(t: &str) -> Option<(&str, &str)> {
    let (l, v) = t.split_once('\t').or_else(|| t.split_once("  "))?;
    let (l, v) = (l.trim(), v.trim());
    (!l.is_empty() && !v.is_empty() && l.chars().count() <= 10).then_some((l, v))
}

#[cfg(test)]
mod lore_tests {
    use super::*;

    #[test]
    fn lore_lines_get_shapes() {
        let flat = |l: &str| plain_text(&lore_line(l));
        assert_eq!(flat("▶ 클릭: 이동"), "▸ 클릭: 이동");
        assert_eq!(flat("좌클릭: 구매"), "▸ 좌클릭: 구매");
        assert_eq!(flat("  돈 1,000"), " ◆ 돈 1,000");
        assert_eq!(flat("---"), "──────────────");
        assert_eq!(flat("길드  달빛"), "길드  달빛");
        assert_eq!(flat("■■□ 2/3"), "■■□ 2/3");
        // 막대는 색이 셋 (찬·빈·글)
        let bar = format!("{:?}", lore_line("■■□ 2/3"));
        assert!(bar.contains("#7be07b") && bar.contains("#3a3a48"));
        // § 코드는 그대로
        assert_eq!(flat("&a초록"), "§a초록");
        assert!(format!("{:?}", lore_line("보상")).contains("bold"));
    }
}
