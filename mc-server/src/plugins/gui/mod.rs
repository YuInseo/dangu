//! 상자 창을 통째로 다시 그리는 창 (리소스 팩 + 창 제목).
//!
//! 바닐라 상자 창은 바탕 그림 → 창 제목 → 아이템 순서로 그립니다. 그래서 창 제목에
//! - 큰 그림 글자 하나(바탕)를 상자 왼쪽 위에 맞춰 넣으면 바닐라 바탕을 덮고 (아이템은 그 위에),
//! - 글과 작은 그림(탭 불빛·막대·칸 테두리)을 원하는 (x, y)에 놓습니다.
//!
//! 위치를 정하는 법:
//! - 창 제목은 상자 왼쪽 위에서 (8, 6)에 그려집니다. 가로는 HUD와 같은 공백 글자로 옮기고,
//! - 세로는 글자 색 표시 `#FA gg bb`로 정합니다: 셰이더(`hud::art`의 `text.vsh`)가 그 글자를
//!   `gg - 32`px 아래로 옮기고 (-32..=223: 창 위에서 아래 끝까지) `bb - 0xF0`번 [`art::PALETTE`] 색을 입힙니다.
//! - 바탕 그림만은 표시 없이 `ascent`로 자리를 잡아, 셰이더가 없어도 제자리에 옵니다.
//!
//! 그림은 2배 해상도로 만듭니다 (글리프 `height` = 그림 높이 / 2): GUI 배율 2 이상에서 선이 고움.
//!
//! 좌표는 모두 상자 왼쪽 위 기준 GUI 픽셀 (6줄 상자: 176 × 222, 칸 (c, r)은 `8 + 18c, 18 + 18r`).

use crate::plugins::hud::art::{self, Image, Pal};
use crate::protocol::nbt::Tag;

/// 창 제목이 그려지는 곳 (상자 왼쪽 위 기준).
pub const TITLE_X: i32 = 8;
pub const TITLE_Y: i32 = 6;

/// 6줄 상자 창의 크기.
pub const CHEST_W: i32 = 176;
pub const CHEST_H: i32 = 222;

/// 위쪽 상자 칸 `slot`(0..54)의 왼쪽 위.
pub const fn cell(slot: usize) -> (i32, i32) {
    (8 + 18 * (slot % 9) as i32, 18 + 18 * (slot / 9) as i32)
}

/// 그림 글자 하나 (2배 해상도 그림).
pub struct Glyph {
    pub ch: char,
    /// 리소스 팩 안 그림 (`hoya:font/gui/...png`)
    pub file: String,
    pub img: Image,
    /// 표시 없이 놓는 바탕이면 그 윗변 y (그 밖의 그림은 표시로 옮김)
    pub fixed_top: Option<i32>,
}

impl Glyph {
    /// 화면 높이 (GUI px).
    pub fn height(&self) -> i32 {
        (self.img.h / 2) as i32
    }

    fn ascent(&self) -> i32 {
        // 글자 윗변 = 제목 y + 7 - ascent (+ 표시로 옮긴 만큼)
        match self.fixed_top {
            Some(top) => TITLE_Y + 7 - top,
            None => 0,
        }
    }

    /// 바닐라 bitmap 공급자의 글자 폭: (잉크 폭 × 배율 + 0.5) 버림 + 1.
    pub fn advance(&self) -> i32 {
        (0.5 + self.img.ink_width() as f32 * 0.5) as i32 + 1
    }

    /// 표시로 옮겨 윗변을 `y`에 두려면 몇 px.
    fn shift_to(&self, y: i32) -> i32 {
        y - (TITLE_Y + 7 - self.ascent())
    }
}

/// 그림 글자들로 된 폰트 JSON (공백 글자 포함).
pub fn font_json(glyphs: &[Glyph]) -> String {
    let mut providers = vec![art::space_provider()];
    for g in glyphs {
        providers.push(format!(
            "{{\"type\":\"bitmap\",\"file\":\"{}\",\"ascent\":{},\"height\":{},\"chars\":[\"{}\"]}}",
            g.file,
            g.ascent(),
            g.height(),
            art::json_chars(&g.ch.to_string())
        ));
    }
    format!("{{\"providers\":[{}]}}", providers.join(","))
}

/// 그림 글자들의 리소스 팩 파일 (그림 + 폰트).
pub fn font_files(font: &str, glyphs: &[Glyph]) -> Vec<(String, Vec<u8>)> {
    let path = |id: &str, dir: &str, ext: &str| {
        let (ns, name) = id.split_once(':').unwrap_or(("minecraft", id));
        format!("assets/{ns}/{dir}/{name}{ext}")
    };
    let mut files: Vec<(String, Vec<u8>)> = glyphs
        .iter()
        .map(|g| {
            (
                path(&g.file, "textures", ""),
                crate::resource_pack::png(g.img.w as u32, g.img.h as u32, &g.img.rgba()),
            )
        })
        .collect();
    files.push((path(font, "font", ".json"), font_json(glyphs).into_bytes()));
    files
}

/// 표시로 옮길 수 있는 세로 범위의 위쪽 끝 (`gg = dy + 32`).
pub const SHIFT_BASE: i32 = 32;

/// 세로 표시 색: `#FA gg bb`.
pub fn marker(dy: i32, pal: Pal) -> String {
    debug_assert!((-SHIFT_BASE..=255 - SHIFT_BASE).contains(&dy), "{dy}");
    format!(
        "#FA{:02X}{:02X}",
        (dy.clamp(-SHIFT_BASE, 255 - SHIFT_BASE) + SHIFT_BASE) as u32,
        0xF0 + pal as u32
    )
}

/// `#FAggbb` → (아래로 옮길 px, 색) (미리보기·테스트용).
pub fn parse_marker(color: &str) -> Option<(i32, Pal)> {
    let v = u32::from_str_radix(color.strip_prefix("#FA")?, 16).ok()?;
    let pal = (v & 0xff).checked_sub(0xF0)? as usize;
    Some(((v >> 8) as i32 - SHIFT_BASE, *art::PALS.get(pal)?))
}

#[derive(Debug, Clone, PartialEq)]
struct Run {
    font: &'static str,
    color: Option<String>,
    text: String,
}

/// 창 제목 하나를 짓는 도구. x는 상자 왼쪽 기준이고, 글자를 놓을 때마다 공백으로 그 자리까지 옮깁니다.
pub struct Canvas {
    runs: Vec<Run>,
    cursor: i32,
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas {
    pub fn new() -> Self {
        Self {
            runs: Vec::new(),
            cursor: TITLE_X,
        }
    }

    fn push(&mut self, font: &'static str, color: Option<String>, c: char) {
        match self.runs.last_mut() {
            Some(r) if r.font == font && r.color == color => r.text.push(c),
            _ => self.runs.push(Run {
                font,
                color,
                text: c.to_string(),
            }),
        }
    }

    fn move_to(&mut self, x: i32) {
        let mut dx = x - self.cursor;
        if dx == 0 {
            return;
        }
        // 공백은 앞 조각에 이어 붙임 (공백은 모든 폰트에 있음)
        let (font, color) = match self.runs.last() {
            Some(r) => (r.font, r.color.clone()),
            None => (art::TEXT_FONT, None),
        };
        let negative = dx < 0;
        dx = dx.abs();
        for k in (0..=10u32).rev() {
            while dx >= 1 << k {
                self.push(font, color.clone(), art::space_char(k, negative));
                dx -= 1 << k;
            }
        }
        self.cursor = x;
    }

    /// 그림 글자를 왼쪽 위 (x, y)에. 바탕(`fixed_top`)이면 y는 무시.
    pub fn image(&mut self, font: &'static str, g: &Glyph, x: i32, y: i32) {
        self.move_to(x);
        let color = match g.fixed_top {
            Some(_) => None,
            None => Some(marker(g.shift_to(y), Pal::White)),
        };
        self.push(font, color, g.ch);
        self.cursor += g.advance();
    }

    /// 같은 그림을 가로로 늘어놓아 `x0..x1`을 채움 (1·2·4…px 조각들로).
    pub fn fill(&mut self, font: &'static str, pieces: &[Glyph], x0: i32, x1: i32, y: i32) {
        let mut x = x0;
        for g in pieces.iter().rev() {
            let w = g.advance() - 1;
            while w > 0 && x1 - x >= w {
                self.image(font, g, x, y);
                x += w;
            }
        }
    }

    /// 바닐라 글자로 쓴 글 (윗변 y). 끝난 x.
    pub fn text(&mut self, x: i32, y: i32, s: &str, pal: Pal) -> i32 {
        self.move_to(x);
        let color = Some(marker(y - TITLE_Y, pal));
        for c in s.chars() {
            if let Some(w) = art::default_advance(c) {
                self.push(art::TEXT_FONT, color.clone(), c);
                self.cursor += w;
            }
        }
        self.cursor
    }

    /// 가운데 맞춤.
    pub fn text_center(&mut self, cx: i32, y: i32, s: &str, pal: Pal) {
        self.text(cx - art::default_width(s) / 2, y, s, pal);
    }

    /// 오른쪽 맞춤 (끝이 rx).
    pub fn text_right(&mut self, rx: i32, y: i32, s: &str, pal: Pal) {
        self.text(rx - art::default_width(s), y, s, pal);
    }

    /// 창 제목 텍스트 컴포넌트로.
    pub fn finish(self) -> Tag {
        let parts = self
            .runs
            .into_iter()
            .map(|r| {
                let mut f = vec![
                    ("text".into(), Tag::String(r.text)),
                    ("font".into(), Tag::String(r.font.into())),
                ];
                match r.color {
                    Some(c) => f.push(("color".into(), Tag::String(c))),
                    // 바탕: 그림 색 그대로
                    None => f.push(("color".into(), Tag::String("white".into()))),
                }
                Tag::Compound(f)
            })
            .collect();
        Tag::Compound(vec![
            ("text".into(), Tag::String(String::new())),
            ("italic".into(), Tag::Byte(0)),
            ("extra".into(), Tag::List(parts)),
        ])
    }
}

/// 셰이더 팩: 바닐라 `core/text.vsh`에 HUD·창 표시 색 처리를 더한 것 (HUD를 꺼도 늘 들어감).
pub fn pack() -> crate::plugins::contents::source::Pack {
    crate::plugins::contents::source::Pack {
        name: "hoya_gui".into(),
        configs: Vec::new(),
        files: vec![(
            "assets/minecraft/shaders/core/text.vsh".into(),
            art::text_vsh().into_bytes(),
        )],
    }
}

/// `max` 폭에 맞게 자름 (자르면 끝에 `..`).
pub fn fit(s: &str, max: i32) -> String {
    crate::plugins::hud::canvas::fit(s, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(ch: char, w: usize, h: usize, fixed_top: Option<i32>) -> Glyph {
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.set(x, y, 0xffffffff);
            }
        }
        Glyph {
            ch,
            file: "hoya:font/gui/t.png".into(),
            img,
            fixed_top,
        }
    }

    #[test]
    fn markers_round_trip() {
        assert_eq!(marker(-6, Pal::White), "#FA1AF0");
        assert_eq!(parse_marker("#FA1AF0"), Some((-6, Pal::White)));
        assert_eq!(
            parse_marker(&marker(212, Pal::Gold)),
            Some((212, Pal::Gold))
        );
        // 창 맨 아래(가방 판)까지 닿음
        const { assert!(CHEST_H - TITLE_Y <= 255 - SHIFT_BASE) };
        assert_eq!(parse_marker("#FD10F4"), None);
    }

    #[test]
    fn glyphs_land_where_asked() {
        // 바탕: 윗변 0 → ascent 13, 셰이더 없이 제자리
        let bg = glyph('\u{E800}', 40, 30, Some(0));
        assert_eq!(bg.ascent(), 13);
        assert_eq!(bg.height(), 15);
        assert_eq!(bg.advance(), 21);
        // 그림: ascent 0 → 윗변 13 + 옮김
        let g = glyph('\u{E801}', 10, 10, None);
        assert_eq!(g.shift_to(40), 27);
        let json = font_json(&[bg, g]);
        let j = crate::json::parse(&json).unwrap();
        for p in j
            .get("providers")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .skip(1)
        {
            let a = p.get("ascent").unwrap().as_f64().unwrap();
            let h = p.get("height").unwrap().as_f64().unwrap();
            assert!(a <= h);
        }
    }

    #[test]
    fn canvas_moves_with_spaces() {
        let g = glyph('\u{E801}', 10, 10, None);
        let mut c = Canvas::new();
        c.image("hoya:gui/t", &g, -20, 40);
        let end = c.text(30, 50, "ab", Pal::Gold);
        assert_eq!(end, 30 + art::default_width("ab"));
        let tag = c.finish();
        let Tag::Compound(f) = &tag else { panic!() };
        let Some((_, Tag::List(parts))) = f.iter().find(|(k, _)| k == "extra") else {
            panic!()
        };
        // 공백 → 그림 → 공백 → 글
        let mut x = TITLE_X;
        for p in parts {
            let Tag::Compound(p) = p else { panic!() };
            let text = p.iter().find_map(|(k, v)| match (k.as_str(), v) {
                ("text", Tag::String(s)) => Some(s.clone()),
                _ => None,
            });
            for ch in text.unwrap().chars() {
                if let Some(a) = art::space_advance(ch) {
                    x += a;
                } else if ch == '\u{E801}' {
                    assert_eq!(x, -20);
                    x += 6;
                } else {
                    if ch == 'a' {
                        assert_eq!(x, 30);
                    }
                    x += art::default_advance(ch).unwrap();
                }
            }
        }
    }
}
