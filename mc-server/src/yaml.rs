//! ItemsAdder 설정 파일을 읽을 만큼의 YAML. 결과는 [`Json`]으로 돌려줍니다.
//!
//! 되는 것: 들여쓰기 맵·목록(`- `), 목록 안의 맵, 한 줄 목록 `[a, b]`과 맵 `{a: 1}`,
//! 따옴표 문자열(`'..'`, `".."` 이스케이프), 숫자·참거짓·null, `#` 주석, 블록 문자열 `|`·`>`.
//! 안 되는 것: 앵커·별칭(`&`, `*`), 여러 문서(`---`), 태그(`!!`).

use crate::json::Json;

#[derive(Debug, Clone)]
struct Line {
    indent: usize,
    text: String,
    no: usize,
}

/// 한 줄에서 주석을 떼어 냅니다 (따옴표 안의 `#`는 그대로).
fn strip_comment(s: &str) -> &str {
    let (mut single, mut double) = (false, false);
    let bytes = s.as_bytes();
    for (i, &c) in bytes.iter().enumerate() {
        match c {
            b'\'' if !double => single = !single,
            b'"' if !single => double = !double,
            b'#' if !single
                && !double
                && (i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') =>
            {
                return &s[..i];
            }
            _ => {}
        }
    }
    s
}

pub fn parse(input: &str) -> Result<Json, String> {
    let raw: Vec<&str> = input.lines().collect();
    let mut lines = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let full = raw[i].trim_end_matches('\r');
        let text = strip_comment(full).trim_end();
        if text.trim().is_empty() || text.trim() == "---" {
            i += 1;
            continue;
        }
        let indent = text.len() - text.trim_start().len();
        let mut t = text.trim_start().to_owned();
        // 블록 문자열: `key: |` 또는 `key: >` 뒤의 더 깊은 줄들
        if t.ends_with(": |") || t.ends_with(": >") || t.ends_with(": |-") || t.ends_with(": >-") {
            let folded = t.contains(": >");
            let head = t[..t.rfind(':').unwrap()].to_owned();
            let mut body = Vec::new();
            let mut j = i + 1;
            while j < raw.len() {
                let l = raw[j].trim_end_matches('\r');
                let ind = l.len() - l.trim_start().len();
                if !l.trim().is_empty() && ind <= indent {
                    break;
                }
                body.push(l.trim().to_owned());
                j += 1;
            }
            let joined = body.join(if folded { " " } else { "\n" });
            t = format!("{head}: {}", quote(joined.trim_end()));
            lines.push(Line {
                indent,
                text: t,
                no: i + 1,
            });
            i = j;
            continue;
        }
        lines.push(Line {
            indent,
            text: t,
            no: i + 1,
        });
        i += 1;
    }
    if lines.is_empty() {
        return Ok(Json::Object(Vec::new()));
    }
    let mut pos = 0;
    let v = block(&lines, &mut pos, lines[0].indent)?;
    if pos < lines.len() {
        return Err(format!(
            "{}번째 줄: 들여쓰기가 맞지 않습니다",
            lines[pos].no
        ));
    }
    Ok(v)
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// 같은 들여쓰기의 줄들을 맵이나 목록으로.
fn block(lines: &[Line], pos: &mut usize, indent: usize) -> Result<Json, String> {
    if lines[*pos].text.starts_with("- ") || lines[*pos].text == "-" {
        let mut items = Vec::new();
        while *pos < lines.len()
            && lines[*pos].indent == indent
            && lines[*pos].text.starts_with('-')
        {
            let line = lines[*pos].clone();
            let rest = line.text[1..].trim_start().to_owned();
            *pos += 1;
            if rest.is_empty() {
                if *pos < lines.len() && lines[*pos].indent > indent {
                    let inner = lines[*pos].indent;
                    items.push(block(lines, pos, inner)?);
                } else {
                    items.push(Json::Null);
                }
            } else if let Some((k, v)) = split_key(&rest) {
                // `- key: value` 다음 줄들이 같은 맵 (들여쓰기는 `- ` 만큼 더 깊음)
                let inner = indent + (line.text.len() - rest.len());
                let mut fields = vec![];
                push_field(&mut fields, k, v, lines, pos, inner, line.no)?;
                while *pos < lines.len()
                    && lines[*pos].indent == inner
                    && !lines[*pos].text.starts_with("- ")
                {
                    let l = lines[*pos].clone();
                    *pos += 1;
                    let (k, v) = split_key(&l.text)
                        .ok_or_else(|| format!("{}번째 줄: `키: 값`이 아닙니다", l.no))?;
                    push_field(&mut fields, k, v, lines, pos, inner, l.no)?;
                }
                items.push(Json::Object(fields));
            } else {
                items.push(scalar(&rest)?);
            }
        }
        return Ok(Json::Array(items));
    }
    let mut fields = Vec::new();
    while *pos < lines.len() && lines[*pos].indent == indent {
        let l = lines[*pos].clone();
        if l.text.starts_with("- ") {
            break;
        }
        *pos += 1;
        let (k, v) = split_key(&l.text)
            .ok_or_else(|| format!("{}번째 줄: `키: 값`이 아닙니다: {}", l.no, l.text))?;
        push_field(&mut fields, k, v, lines, pos, indent, l.no)?;
    }
    if *pos < lines.len() && lines[*pos].indent > indent {
        return Err(format!(
            "{}번째 줄: 들여쓰기가 맞지 않습니다",
            lines[*pos].no
        ));
    }
    Ok(Json::Object(fields))
}

fn push_field(
    fields: &mut Vec<(String, Json)>,
    key: String,
    value: String,
    lines: &[Line],
    pos: &mut usize,
    indent: usize,
    no: usize,
) -> Result<(), String> {
    let v = if value.is_empty() {
        // 값이 다음 줄들에 (더 깊게, 또는 같은 깊이의 `- ` 목록)
        match lines.get(*pos) {
            Some(n) if n.indent > indent || (n.indent == indent && n.text.starts_with("- ")) => {
                let inner = n.indent;
                block(lines, pos, inner)?
            }
            _ => Json::Null,
        }
    } else {
        scalar(&value).map_err(|e| format!("{no}번째 줄: {e}"))?
    };
    fields.retain(|(k, _)| *k != key);
    fields.push((key, v));
    Ok(())
}

/// `key: value` 나누기. 키는 따옴표로 감쌀 수 있음.
fn split_key(s: &str) -> Option<(String, String)> {
    let (key, rest) = if let Some(q) = s.strip_prefix('"').or_else(|| s.strip_prefix('\'')) {
        let quote = s.chars().next().unwrap();
        let end = q.find(quote)?;
        let after = q[end + 1..].trim_start();
        (q[..end].to_owned(), after.strip_prefix(':')?)
    } else {
        // 따옴표·괄호 밖의 첫 `: ` (또는 줄 끝의 `:`)
        let bytes = s.as_bytes();
        let mut idx = None;
        for (i, &c) in bytes.iter().enumerate() {
            if c == b'"' || c == b'\'' || c == b'[' || c == b'{' {
                break;
            }
            if c == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' ') {
                idx = Some(i);
                break;
            }
        }
        let i = idx?;
        (s[..i].trim().to_owned(), &s[i + 1..])
    };
    Some((key, rest.trim().to_owned()))
}

fn scalar(s: &str) -> Result<Json, String> {
    let s = s.trim();
    if s.starts_with('[') || s.starts_with('{') {
        let mut p = Flow {
            s: s.as_bytes(),
            i: 0,
        };
        let v = p.value()?;
        return Ok(v);
    }
    Ok(plain(s))
}

fn unquote_double(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

fn plain(s: &str) -> Json {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return Json::String(unquote_double(&s[1..s.len() - 1]));
    }
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        return Json::String(s[1..s.len() - 1].replace("''", "'"));
    }
    match s {
        "true" | "True" | "TRUE" | "yes" | "on" => return Json::Bool(true),
        "false" | "False" | "FALSE" | "no" | "off" => return Json::Bool(false),
        "null" | "~" | "" => return Json::Null,
        _ => {}
    }
    if let Ok(n) = s.parse::<f64>()
        && !s.starts_with('+')
        && s.chars().any(|c| c.is_ascii_digit())
    {
        return Json::Number(n);
    }
    Json::String(s.to_owned())
}

/// 한 줄 목록·맵: `[a, 'b c', {x: 1}]`.
struct Flow<'a> {
    s: &'a [u8],
    i: usize,
}

impl Flow<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.ws();
        match self.s.get(self.i) {
            Some(b'[') => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.ws();
                    if self.s.get(self.i) == Some(&b']') {
                        self.i += 1;
                        return Ok(Json::Array(items));
                    }
                    items.push(self.value()?);
                    self.ws();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {}
                        _ => return Err("`[` 목록이 닫히지 않았습니다".into()),
                    }
                }
            }
            Some(b'{') => {
                self.i += 1;
                let mut fields = Vec::new();
                loop {
                    self.ws();
                    if self.s.get(self.i) == Some(&b'}') {
                        self.i += 1;
                        return Ok(Json::Object(fields));
                    }
                    let key = match self.value()? {
                        Json::String(k) => k,
                        Json::Number(n) => n.to_string(),
                        other => format!("{other:?}"),
                    };
                    self.ws();
                    if self.s.get(self.i) != Some(&b':') {
                        return Err("`{` 맵에 `:`가 없습니다".into());
                    }
                    self.i += 1;
                    let v = self.value()?;
                    fields.push((key, v));
                    self.ws();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {}
                        _ => return Err("`{` 맵이 닫히지 않았습니다".into()),
                    }
                }
            }
            Some(&q @ (b'"' | b'\'')) => {
                let start = self.i;
                self.i += 1;
                while self.i < self.s.len() {
                    if self.s[self.i] == b'\\' && q == b'"' {
                        self.i += 2;
                        continue;
                    }
                    if self.s[self.i] == q {
                        break;
                    }
                    self.i += 1;
                }
                self.i += 1;
                let text =
                    std::str::from_utf8(&self.s[start..self.i.min(self.s.len())]).unwrap_or("");
                Ok(plain(text))
            }
            _ => {
                let start = self.i;
                while self.i < self.s.len() && !matches!(self.s[self.i], b',' | b']' | b'}' | b':')
                {
                    self.i += 1;
                }
                // `{a: 1}`의 키 뒤 `:`, 또는 값 안의 `ns:id`
                if self.s.get(self.i) == Some(&b':')
                    && self.s.get(self.i + 1).is_some_and(|c| *c != b' ')
                {
                    self.i += 1;
                    while self.i < self.s.len()
                        && !matches!(self.s[self.i], b',' | b']' | b'}' | b' ')
                    {
                        self.i += 1;
                    }
                }
                let text = std::str::from_utf8(&self.s[start..self.i])
                    .unwrap_or("")
                    .trim();
                Ok(plain(text))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_itemsadder_style_file() {
        let y = r#"
info:
  namespace: myitems   # 이름공간
items:
  ruby:
    display_name: "&c루비 #1"
    lore:
      - '&7빛나는 보석'
      - "두 번째 줄"
    resource:
      material: EMERALD
      generate: true
      textures:
      - item/ruby.png
    attribute_modifiers:
      mainhand: {attackDamage: 7, attackSpeed: -2.4}
    tags: [gem, shiny]
font_images:
  heart:
    path: font/heart.png
    scale_ratio: 9
    y_position: 8
categories:
  gems:
    icon: myitems:ruby
    items: [myitems:ruby, myitems:sapphire]
recipes:
  - name: first
    amount: 2
  - plain
description: |
  여러 줄
  설명
"#;
        let j = parse(y).unwrap();
        assert_eq!(
            j.get("info").unwrap().get("namespace").unwrap().as_str(),
            Some("myitems")
        );
        let ruby = j.get("items").unwrap().get("ruby").unwrap();
        assert_eq!(
            ruby.get("display_name").unwrap().as_str(),
            Some("&c루비 #1")
        );
        let lore = ruby.get("lore").unwrap().as_array().unwrap();
        assert_eq!(lore[0].as_str(), Some("&7빛나는 보석"));
        assert_eq!(lore.len(), 2);
        let res = ruby.get("resource").unwrap();
        assert_eq!(res.get("generate"), Some(&Json::Bool(true)));
        assert_eq!(
            res.get("textures").unwrap().as_array().unwrap()[0].as_str(),
            Some("item/ruby.png")
        );
        let main = ruby
            .get("attribute_modifiers")
            .unwrap()
            .get("mainhand")
            .unwrap();
        assert_eq!(main.get("attackDamage").unwrap().as_f64(), Some(7.0));
        assert_eq!(main.get("attackSpeed").unwrap().as_f64(), Some(-2.4));
        let heart = j.get("font_images").unwrap().get("heart").unwrap();
        assert_eq!(heart.get("scale_ratio").unwrap().as_f64(), Some(9.0));
        let cat = j.get("categories").unwrap().get("gems").unwrap();
        assert_eq!(
            cat.get("items").unwrap().as_array().unwrap()[1].as_str(),
            Some("myitems:sapphire")
        );
        let rec = j.get("recipes").unwrap().as_array().unwrap();
        assert_eq!(rec[0].get("amount").unwrap().as_f64(), Some(2.0));
        assert_eq!(rec[1].as_str(), Some("plain"));
        assert_eq!(
            j.get("description").unwrap().as_str(),
            Some("여러 줄\n설명")
        );
    }

    #[test]
    fn reports_bad_indentation() {
        assert!(parse("a:\n  b: 1\n c: 2\n").is_err());
    }
}
