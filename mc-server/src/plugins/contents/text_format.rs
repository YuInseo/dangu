//! ItemsAdder 글 형식 → 텍스트 컴포넌트.
//!
//! 받는 것 (섞여 있어도 됨):
//! - 옛 코드: `&6`, `§l`, `&r`, Spigot 16진수 `&x&F&F&0&0&0&0`, 그리고 흔히 쓰는 `&#FF0000`
//! - ItemsAdder 16진수: `%#FE5A00%`
//! - MiniMessage: `<red>`, `<#FE5A00>`, `<color:#..>`, `<bold>`/`<b>`, `<italic>`/`<i>`,
//!   `<underlined>`/`<u>`, `<strikethrough>`/`<st>`, `<obfuscated>`/`<obf>`, `<reset>`, 닫는 태그,
//!   `<gradient:#a:#b>`(글자마다 색을 섞음), `<rainbow>`, `<font:alt>`, `<lang:key>`
//! - JSON 컴포넌트: `{"text":"..."}`로 시작하면 그대로
//!
//! 이름·설명은 바닐라처럼 기울이지 않습니다 (`italic: false`).

use crate::protocol::nbt::Tag;

#[derive(Debug, Clone, Default, PartialEq)]
struct Style {
    color: Option<String>,
    bold: bool,
    italic: bool,
    underlined: bool,
    strikethrough: bool,
    obfuscated: bool,
    font: Option<String>,
    /// 글자마다 섞을 색들 (그라데이션·무지개).
    gradient: Option<Vec<(u8, u8, u8)>>,
}

const NAMED: &[(&str, char)] = &[
    ("black", '0'),
    ("dark_blue", '1'),
    ("dark_green", '2'),
    ("dark_aqua", '3'),
    ("dark_red", '4'),
    ("dark_purple", '5'),
    ("gold", '6'),
    ("gray", '7'),
    ("dark_gray", '8'),
    ("blue", '9'),
    ("green", 'a'),
    ("aqua", 'b'),
    ("red", 'c'),
    ("light_purple", 'd'),
    ("yellow", 'e'),
    ("white", 'f'),
];

fn code_color(c: char) -> Option<&'static str> {
    NAMED.iter().find(|(_, k)| *k == c).map(|(n, _)| *n)
}

fn named_color(n: &str) -> Option<String> {
    let n = n.to_ascii_lowercase();
    let n = match n.as_str() {
        "grey" => "gray",
        "dark_grey" => "dark_gray",
        other => other,
    };
    NAMED
        .iter()
        .find(|(name, _)| *name == n)
        .map(|(name, _)| (*name).to_owned())
        .or_else(|| hex(n).map(|(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}")))
}

fn hex(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn rgb_of(color: &str) -> Option<(u8, u8, u8)> {
    if let Some(h) = hex(color) {
        return Some(h);
    }
    const RGB: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (0, 0, 170),
        (0, 170, 0),
        (0, 170, 170),
        (170, 0, 0),
        (170, 0, 170),
        (255, 170, 0),
        (170, 170, 170),
        (85, 85, 85),
        (85, 85, 255),
        (85, 255, 85),
        (85, 255, 255),
        (255, 85, 85),
        (255, 85, 255),
        (255, 255, 85),
        (255, 255, 255),
    ];
    NAMED.iter().position(|(n, _)| *n == color).map(|i| RGB[i])
}

struct Builder {
    parts: Vec<Tag>,
    text: String,
    style: Style,
}

impl Builder {
    fn flush(&mut self) {
        if self.text.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.text);
        match self.style.gradient.clone() {
            Some(stops) if stops.len() >= 2 => {
                let chars: Vec<char> = text.chars().collect();
                let n = chars.len().max(2) - 1;
                for (i, ch) in chars.iter().enumerate() {
                    let t = i as f32 / n as f32 * (stops.len() - 1) as f32;
                    let seg = (t.floor() as usize).min(stops.len() - 2);
                    let f = t - seg as f32;
                    let (a, b) = (stops[seg], stops[seg + 1]);
                    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
                    let c = format!(
                        "#{:02X}{:02X}{:02X}",
                        mix(a.0, b.0),
                        mix(a.1, b.1),
                        mix(a.2, b.2)
                    );
                    self.parts.push(self.part(ch.to_string(), Some(c)));
                }
            }
            _ => {
                let p = self.part(text, self.style.color.clone());
                self.parts.push(p);
            }
        }
    }

    fn part(&self, text: String, color: Option<String>) -> Tag {
        let mut f = vec![("text".into(), Tag::String(text))];
        if let Some(c) = color {
            f.push(("color".into(), Tag::String(c)));
        }
        for (k, on) in [
            ("bold", self.style.bold),
            ("underlined", self.style.underlined),
            ("strikethrough", self.style.strikethrough),
            ("obfuscated", self.style.obfuscated),
        ] {
            if on {
                f.push((k.into(), Tag::Byte(1)));
            }
        }
        f.push(("italic".into(), Tag::Byte(self.style.italic as i8)));
        if let Some(font) = &self.style.font {
            f.push(("font".into(), Tag::String(font.clone())));
        }
        Tag::Compound(f)
    }

    /// 색을 바꾸면 옛 코드처럼 꾸밈도 풀립니다.
    fn set_color(&mut self, color: String) {
        self.flush();
        let font = self.style.font.take();
        self.style = Style {
            color: Some(color),
            font,
            ..Style::default()
        };
    }
}

/// MiniMessage 태그 하나를 적용. 모르는 태그면 false (글자 그대로 둠).
fn apply_tag(b: &mut Builder, tag: &str, default: &Style) -> bool {
    let (closing, body) = match tag.strip_prefix('/') {
        Some(t) => (true, t),
        None => (false, tag),
    };
    let mut args = body.split(':');
    let name = args.next().unwrap_or("").to_ascii_lowercase();
    let rest: Vec<&str> = args.collect();
    b.flush();
    let st = &mut b.style;
    match name.as_str() {
        "bold" | "b" => st.bold = !closing,
        "italic" | "i" | "em" => st.italic = !closing,
        "underlined" | "u" => st.underlined = !closing,
        "strikethrough" | "st" => st.strikethrough = !closing,
        "obfuscated" | "obf" => st.obfuscated = !closing,
        "reset" | "r" => *st = default.clone(),
        "font" if closing => st.font = None,
        "font" => {
            st.font = rest.first().map(|f| {
                if f.contains(':') || rest.len() < 2 {
                    (*f).to_owned()
                } else {
                    rest.join(":")
                }
            })
        }
        "gradient" | "rainbow" if closing => st.gradient = None,
        "gradient" => {
            let stops: Vec<_> = rest
                .iter()
                .filter_map(|c| named_color(c).and_then(|c| rgb_of(&c)))
                .collect();
            st.gradient = (stops.len() >= 2).then_some(stops);
        }
        "rainbow" => {
            st.gradient = Some(vec![
                (255, 0, 0),
                (255, 165, 0),
                (255, 255, 0),
                (0, 200, 0),
                (0, 120, 255),
                (140, 0, 255),
            ])
        }
        "color" | "c" | "colour" if !closing => match rest.first().and_then(|c| named_color(c)) {
            Some(c) => b.set_color(c),
            None => return false,
        },
        "color" | "c" | "colour" => st.color = default.color.clone(),
        n => match named_color(n) {
            Some(_) if closing => st.color = default.color.clone(),
            Some(c) => b.set_color(c),
            None => return false,
        },
    }
    true
}

/// 글 한 줄을 컴포넌트로. `default_color`는 색이 없을 때의 색.
pub fn parse(input: &str, default_color: Option<&str>) -> Tag {
    let trimmed = input.trim_start();
    if trimmed.starts_with('{')
        && let Ok(j) = crate::json::parse(trimmed)
    {
        return json_to_tag(&j);
    }
    let default = Style {
        color: default_color.map(str::to_owned),
        ..Style::default()
    };
    let mut b = Builder {
        parts: Vec::new(),
        text: String::new(),
        style: default.clone(),
    };
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // %#RRGGBB%
        if c == '%' && chars.get(i + 1) == Some(&'#') && chars.get(i + 8) == Some(&'%') {
            let h: String = chars[i + 1..i + 8].iter().collect();
            if hex(&h).is_some() {
                b.set_color(h.to_uppercase());
                i += 9;
                continue;
            }
        }
        if (c == '&' || c == '§') && i + 1 < chars.len() {
            let n = chars[i + 1].to_ascii_lowercase();
            // &#RRGGBB
            if n == '#' && i + 8 <= chars.len() {
                let h: String = chars[i + 1..i + 8].iter().collect();
                if hex(&h).is_some() {
                    b.set_color(h.to_uppercase());
                    i += 8;
                    continue;
                }
            }
            // &x&R&R&G&G&B&B
            if n == 'x' && i + 14 <= chars.len() {
                let digits: String = (0..6).map(|k| chars[i + 3 + 2 * k]).collect();
                if (0..6).all(|k| matches!(chars[i + 2 + 2 * k], '&' | '§'))
                    && let Some(rgb) = hex(&format!("#{digits}"))
                {
                    b.set_color(format!("#{:02X}{:02X}{:02X}", rgb.0, rgb.1, rgb.2));
                    i += 14;
                    continue;
                }
            }
            if let Some(name) = code_color(n) {
                b.set_color(name.to_owned());
                i += 2;
                continue;
            }
            let flag = match n {
                'l' => Some(0),
                'o' => Some(1),
                'n' => Some(2),
                'm' => Some(3),
                'k' => Some(4),
                'r' => Some(5),
                _ => None,
            };
            if let Some(f) = flag {
                b.flush();
                match f {
                    0 => b.style.bold = true,
                    1 => b.style.italic = true,
                    2 => b.style.underlined = true,
                    3 => b.style.strikethrough = true,
                    4 => b.style.obfuscated = true,
                    _ => b.style = default.clone(),
                }
                i += 2;
                continue;
            }
        }
        if c == '<'
            && let Some(end) = chars[i + 1..].iter().position(|&x| x == '>')
        {
            let tag: String = chars[i + 1..i + 1 + end].iter().collect();
            // ItemsAdder 글 효과 `<r 글>`(띄어쓰기)는 태그가 아님
            if !tag.contains(' ') {
                if let Some(key) = tag.strip_prefix("lang:") {
                    b.flush();
                    let mut f = vec![
                        ("translate".into(), Tag::String(key.to_owned())),
                        ("italic".into(), Tag::Byte(0)),
                    ];
                    if let Some(c) = &b.style.color {
                        f.push(("color".into(), Tag::String(c.clone())));
                    }
                    b.parts.push(Tag::Compound(f));
                    i += end + 2;
                    continue;
                }
                if apply_tag(&mut b, &tag, &default) {
                    i += end + 2;
                    continue;
                }
            }
        }
        b.text.push(c);
        i += 1;
    }
    b.flush();
    match b.parts.len() {
        0 => b.part(String::new(), default.color.clone()),
        1 => b.parts.pop().unwrap(),
        _ => Tag::Compound(vec![
            ("text".into(), Tag::String(String::new())),
            ("italic".into(), Tag::Byte(0)),
            ("extra".into(), Tag::List(b.parts)),
        ]),
    }
}

/// 꾸밈을 모두 뺀 글 (목록·로그용).
pub fn plain(input: &str) -> String {
    fn collect(t: &Tag, out: &mut String) {
        match t {
            Tag::String(s) => out.push_str(s),
            Tag::Compound(f) => {
                for (k, v) in f {
                    match (k.as_str(), v) {
                        ("text", Tag::String(s)) | ("translate", Tag::String(s)) => out.push_str(s),
                        ("extra", Tag::List(l)) => l.iter().for_each(|x| collect(x, out)),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    collect(&parse(input, None), &mut out);
    out
}

/// JSON 텍스트 컴포넌트 → NBT.
pub fn json_to_tag(j: &crate::json::Json) -> Tag {
    use crate::json::Json;
    match j {
        Json::String(s) => Tag::String(s.clone()),
        Json::Number(n) => Tag::Int(*n as i32),
        Json::Bool(b) => Tag::Byte(*b as i8),
        Json::Array(a) => {
            // 목록은 첫 조각에 나머지를 붙임
            let mut parts = a.iter().map(json_to_tag).map(|t| match t {
                Tag::String(s) => Tag::Compound(vec![("text".into(), Tag::String(s))]),
                o => o,
            });
            let Some(Tag::Compound(mut first)) = parts.next() else {
                return Tag::String(String::new());
            };
            let rest: Vec<Tag> = parts.collect();
            if !rest.is_empty() {
                first.push(("extra".into(), Tag::List(rest)));
            }
            Tag::Compound(first)
        }
        Json::Object(f) => {
            let mut out: Vec<(String, Tag)> =
                f.iter().map(|(k, v)| (k.clone(), json_to_tag(v))).collect();
            if !out.iter().any(|(k, _)| k == "italic") {
                out.push(("italic".into(), Tag::Byte(0)));
            }
            Tag::Compound(out)
        }
        Json::Null => Tag::String(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(t: &'a Tag, key: &str) -> Vec<&'a Tag> {
        let mut out = Vec::new();
        fn walk<'a>(t: &'a Tag, key: &str, out: &mut Vec<&'a Tag>) {
            if let Tag::Compound(f) = t {
                for (k, v) in f {
                    if k == key {
                        out.push(v);
                    }
                    walk(v, key, out);
                }
            }
            if let Tag::List(l) = t {
                l.iter().for_each(|x| walk(x, key, out));
            }
        }
        walk(t, key, &mut out);
        out
    }

    #[test]
    fn legacy_hex_and_minimessage() {
        assert_eq!(plain("&6&lExample &rBundle"), "Example Bundle");
        let t = parse("&6Gold &#FF0000Red", None);
        let colors: Vec<_> = find(&t, "color").into_iter().cloned().collect();
        assert_eq!(
            colors,
            vec![Tag::String("gold".into()), Tag::String("#FF0000".into())]
        );
        let t = parse("%#FE5A00%chain_events", None);
        assert_eq!(find(&t, "color"), vec![&Tag::String("#FE5A00".into())]);
        let t = parse("&x&F&F&0&0&0&0Spigot", None);
        assert_eq!(find(&t, "color"), vec![&Tag::String("#FF0000".into())]);
        let t = parse("<red>Magic <green><bold>Fuel</bold>!", None);
        assert_eq!(plain("<red>Magic <green><bold>Fuel</bold>!"), "Magic Fuel!");
        assert_eq!(find(&t, "bold").len(), 1);
        let t = parse("<gradient:#ff0000:#0000ff>ab", None);
        assert_eq!(
            find(&t, "color"),
            vec![
                &Tag::String("#FF0000".into()),
                &Tag::String("#0000FF".into())
            ]
        );
        let t = parse("<font:alt>x</font>", None);
        assert_eq!(find(&t, "font"), vec![&Tag::String("alt".into())]);
        let t = parse("<lang:my.key>", None);
        assert_eq!(find(&t, "translate"), vec![&Tag::String("my.key".into())]);
        // 효과 문법·모르는 태그·2 < 3은 글자 그대로
        assert_eq!(plain("<r 무지개> 2 < 3 <nope>"), "<r 무지개> 2 < 3 <nope>");
        assert_eq!(plain(r#"{"text":"TEST","font":"alt"}"#), "TEST");
        // 기본은 기울이지 않음
        assert_eq!(
            find(&parse("x", Some("white")), "italic"),
            vec![&Tag::Byte(0)]
        );
    }
}
