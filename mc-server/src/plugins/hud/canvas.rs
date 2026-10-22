//! 한 줄(보스바 제목·액션바 글)에 글자를 원하는 x에 놓는 도구.
//!
//! 줄 전체 폭이 0이 되게 끝에서 되돌아가므로 줄의 시작이 곧 화면 가운데이고, 글자마다의
//! [`art::Anchor`]에 따라 셰이더가 화면 반 폭만큼 옮기므로 x는 그 기준점(왼쪽 끝·가운데·오른쪽 끝)에서의
//! 거리가 됩니다.

use super::art::{self, RowFont, Strip, Tint};
use crate::protocol::nbt::Tag;

/// 5×7 글자 사이 (테두리가 겹치도록 한 칸 당김).
const TRACKING: i32 = -1;
/// 공백만 있는 조각의 폰트 (공백은 모든 HUD 폰트에 있음).
const SPACE_FONT: &str = art::T0.id;

#[derive(Debug, Clone, PartialEq)]
struct Run {
    /// None = 바닐라 기본 폰트
    font: Option<&'static str>,
    tint: Option<Tint>,
    text: String,
}

#[derive(Default)]
pub struct Canvas {
    runs: Vec<Run>,
    cursor: i32,
}

impl Canvas {
    fn push(&mut self, font: Option<&'static str>, tint: Option<Tint>, c: char) {
        match self.runs.last_mut() {
            Some(r) if r.font == font && r.tint == tint => r.text.push(c),
            _ => self.runs.push(Run {
                font,
                tint,
                text: c.to_string(),
            }),
        }
    }

    fn move_to(&mut self, x: i32) {
        let mut dx = x - self.cursor;
        if dx == 0 {
            return;
        }
        // 공백은 앞 조각에 이어 붙임 (HUD 폰트일 때)
        let (font, tint) = match self.runs.last() {
            Some(r) => (r.font, r.tint),
            _ => (Some(SPACE_FONT), None),
        };
        let negative = dx < 0;
        dx = dx.abs();
        for k in (0..=10u32).rev() {
            while dx >= 1 << k {
                self.push(font, tint, art::space_char(k, negative));
                dx -= 1 << k;
            }
        }
        self.cursor = x;
    }

    /// 그림판 글자 하나.
    pub fn glyph(&mut self, row: &RowFont, tint: Tint, x: i32, c: char) {
        debug_assert!(
            art::sheet_of(c).is_some_and(|s| s.fits(row)),
            "{c:?}는 {}에 없음",
            row.id
        );
        self.move_to(x);
        self.push(Some(row.id), Some(tint), c);
        self.cursor += art::advance(c);
    }

    /// 5×7 글자·아이콘으로 쓴 글. 끝난 x를 돌려줍니다.
    pub fn small(&mut self, row: &RowFont, tint: Tint, x: i32, s: &str) -> i32 {
        let mut x = x;
        for c in s.chars() {
            self.glyph(row, tint, x, c);
            x += art::advance(c) + TRACKING;
        }
        x
    }

    /// 라벨 한글로 쓴 글 (없는 글자는 건너뜀).
    pub fn label(&mut self, row: &RowFont, tint: Tint, x: i32, s: &str) -> i32 {
        let mut x = x;
        for c in s.chars() {
            if c == ' ' {
                x += 3;
            } else if let Some(g) = art::label_char(c) {
                self.glyph(row, tint, x, g);
                x += art::advance(g);
            }
        }
        x
    }

    /// 바닐라 글자로 쓴 글 ([`art::TEXT_FONT`]; 줄의 원래 높이에 그려짐; 폭을 모르는 글자는 뺌).
    pub fn text(&mut self, tint: Tint, x: i32, s: &str) -> i32 {
        self.move_to(x);
        for c in s.chars() {
            if let Some(w) = art::default_advance(c) {
                self.push(None, Some(tint), c);
                self.cursor += w;
            }
        }
        self.cursor
    }

    /// `x0..x1` 판.
    pub fn strip(&mut self, row: &RowFont, strip: Strip, tint: Tint, x0: i32, x1: i32) {
        let cap = strip.cap_width();
        let mut x = x0;
        let caps = x1 - x0 >= cap * 2;
        if caps {
            self.glyph(row, tint, x0, strip.cap(false));
            x += cap;
        }
        let mut left = if caps { x1 - cap } else { x1 } - x;
        for k in (0..=6u32).rev() {
            while left >= 1 << k {
                self.glyph(row, tint, x, strip.mid(k));
                x += 1 << k;
                left -= 1 << k;
            }
        }
        if caps {
            self.glyph(row, tint, x1 - cap, strip.cap(true));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// 줄 폭을 0으로 되돌리고 텍스트 컴포넌트로.
    pub fn finish(mut self) -> Tag {
        self.move_to(0);
        let parts = self
            .runs
            .into_iter()
            .map(|r| {
                let mut f = vec![("text".into(), Tag::String(r.text))];
                f.push((
                    "font".into(),
                    Tag::String(r.font.unwrap_or(art::TEXT_FONT).into()),
                ));
                if let Some(t) = r.tint {
                    f.push(("color".into(), Tag::String(t.color())));
                    if r.font.is_none() {
                        f.push(("shadow_color".into(), Tag::Int(t.shadow())));
                    }
                }
                Tag::Compound(f)
            })
            .collect();
        Tag::Compound(vec![
            ("text".into(), Tag::String(String::new())),
            ("extra".into(), Tag::List(parts)),
        ])
    }
}

pub fn small_width(s: &str) -> i32 {
    if s.is_empty() {
        return 0;
    }
    s.chars().map(|c| art::advance(c) + TRACKING).sum::<i32>() - TRACKING
}

pub fn label_width(s: &str) -> i32 {
    s.chars()
        .map(|c| match c {
            ' ' => 3,
            c => art::label_char(c).map_or(0, art::advance),
        })
        .sum()
}

/// 기본 폰트 글을 `max` 폭에 맞게 자릅니다 (자르면 끝에 `..`).
pub fn fit(s: &str, max: i32) -> String {
    if art::default_width(s) <= max {
        return s.to_owned();
    }
    let dots = art::default_width("..");
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = art::default_advance(c).unwrap_or(0);
        if w + cw + dots > max {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.trim_end().to_owned() + ".."
}

/// 줄 하나의 글자를 하나씩 (공백 포함) 꺼냅니다: (폰트, 표시, 글자). 미리보기·테스트용.
#[cfg(test)]
pub fn walk(tag: &Tag) -> Vec<(Option<String>, Option<Tint>, char)> {
    let Tag::Compound(f) = tag else { panic!() };
    let Some((_, Tag::List(parts))) = f.iter().find(|(k, _)| k == "extra") else {
        panic!()
    };
    let mut out = Vec::new();
    for p in parts {
        let Tag::Compound(f) = p else { panic!() };
        let get = |k: &str| {
            f.iter().find_map(|(n, v)| match v {
                Tag::String(s) if n == k => Some(s.clone()),
                _ => None,
            })
        };
        let font = get("font").filter(|f| f != art::TEXT_FONT);
        let tint = get("color").and_then(|c| Tint::parse(&c));
        for c in get("text").unwrap().chars() {
            out.push((font.clone(), tint, c));
        }
    }
    out
}

/// 그 글자의 폭 (기본 폰트면 기본 폰트 폭).
#[cfg(test)]
pub fn advance_in(font: &Option<String>, c: char) -> i32 {
    if let Some(a) = art::space_advance(c) {
        return a;
    }
    match font {
        None => art::default_advance(c).unwrap_or(0),
        Some(_) => art::advance(c),
    }
}

#[cfg(test)]
pub fn total_advance(tag: &Tag) -> i32 {
    walk(tag).iter().map(|(f, _, c)| advance_in(f, *c)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::hud::art::{Anchor, Pal, T0, tint};

    #[test]
    fn lines_return_to_zero() {
        let mut c = Canvas::default();
        let t = tint(Anchor::Right, Pal::White);
        c.strip(&T0, Strip::Card11, t, -120, -4);
        c.text(t, -100, "주간 나무 베기 Lv3");
        c.small(&T0, t, -30, "12/30");
        c.label(&T0, t, -60, "레벨");
        let tag = c.finish();
        assert_eq!(total_advance(&tag), 0);
        let raw = format!("{tag:?}");
        assert!(raw.contains("shadow_color"));
    }

    #[test]
    fn fitting_text() {
        assert_eq!(fit("짧음", 100), "짧음");
        let f = fit("아주 아주 긴 퀘스트 이름입니다", 50);
        assert!(f.ends_with("..") && art::default_width(&f) <= 50, "{f}");
    }
}
