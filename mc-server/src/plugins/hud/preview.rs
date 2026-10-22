//! 미리보기: 클라이언트가 할 배치(공백 폭, 줄마다 ascent, 기준점·색 표시)를 흉내 내어 GUI 화면
//! 한 장을 그립니다. `MC_HUD_PREVIEW=<경로.png> cargo test hud_preview -- --ignored`
//!
//! 바닐라 기본 폰트 글은 그림이 없으므로 라벨 한글이 있으면 그것으로, 숫자는 HUD 숫자로, 나머지는
//! 회색 네모로 대신합니다 (자리·폭은 같음).

use super::art::{self, Image, PALETTE, all_rows};
use super::canvas::{advance_in, walk};
use crate::protocol::nbt::Tag;
use crate::resource_pack::png;

pub struct Screen {
    img: Image,
}

impl Screen {
    pub fn new(w: usize, h: usize) -> Self {
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                // 하늘 → 풀밭
                let c = if y < h * 3 / 5 {
                    0x86b7f0ff - ((y as u32 / 8) << 16)
                } else if (x / 16 + y / 16) % 2 == 0 {
                    0x5d9b3aff
                } else {
                    0x558f35ff
                };
                img.set(x, y, c);
            }
        }
        // 핫바 자리
        let (cx, hb) = (w as i32 / 2, h as i32 - 22);
        for y in hb..h as i32 {
            for x in cx - 91..cx + 91 {
                let edge = y == hb || x == cx - 91 || x == cx + 90 || (x - cx + 91) % 20 == 0;
                img.set(
                    x as usize,
                    y as usize,
                    if edge { 0x202020ff } else { 0x8b8b8bff },
                );
            }
        }
        Self { img }
    }

    fn blend(&mut self, x: i32, y: i32, c: u32) {
        if x < 0 || y < 0 || x as usize >= self.img.w || y as usize >= self.img.h {
            return;
        }
        let a = (c & 0xff) as f32 / 255.0;
        let d = self.img.get(x as usize, y as usize);
        let mix = |s: u32| {
            let (sc, dc) = ((c >> s) & 0xff, (d >> s) & 0xff);
            ((sc as f32 * a + dc as f32 * (1.0 - a)) as u32) << s
        };
        self.img
            .set(x as usize, y as usize, mix(24) | mix(16) | mix(8) | 0xff);
    }

    fn draw(&mut self, img: &Image, x0: i32, y0: i32, tint: u32) {
        for gy in 0..img.h {
            for gx in 0..img.w {
                let px = img.get(gx, gy);
                if px & 0xff == 0 {
                    continue;
                }
                let ch = |s: u32| ((px >> s & 0xff) * (tint >> (s - 8) & 0xff) / 255) << s;
                self.blend(
                    x0 + gx as i32,
                    y0 + gy as i32,
                    ch(24) | ch(16) | ch(8) | (px & 0xff),
                );
            }
        }
    }

    /// 기본 폰트 글자 대신 그릴 그림.
    fn default_glyph(c: char) -> Option<Image> {
        if let Some(l) = art::label_char(c) {
            let s = art::sheet_of(l)?;
            return s
                .glyphs
                .iter()
                .find(|(g, _)| *g == l)
                .map(|(_, i)| i.half());
        }
        if c.is_ascii_digit() || c == '/' {
            let s = art::sheet_of(c)?;
            return s.glyphs.iter().find(|(g, _)| *g == c).map(|(_, i)| {
                // 9px 숫자를 8px 줄에: 위 한 줄을 버림
                let mut out = Image::new(i.w, 8);
                for y in 0..8 {
                    for x in 0..i.w {
                        out.set(x, y, i.get(x, y + 1));
                    }
                }
                out
            });
        }
        if c == ' ' {
            return None;
        }
        let w = art::default_advance(c).unwrap_or(0).max(2) as usize - 1;
        let mut out = Image::new(w, 8);
        for y in 1..8 {
            for x in 0..w {
                out.set(x, y, 0xb0b0b0c0);
            }
        }
        Some(out)
    }

    /// 텍스트 한 줄을 `y`(보스바 제목이면 3 + 19×줄, 액션바면 화면 높이 - 72)에.
    pub fn draw_line(&mut self, tag: &Tag, y: i32) {
        let rows = all_rows();
        let mut cursor = 0;
        for (font, t, c) in walk(tag) {
            let adv = advance_in(&font, c);
            if art::space_advance(c).is_some() {
                cursor += adv;
                continue;
            }
            let t = t.expect("표시 색 없는 글자");
            let x0 = self.img.w as i32 / 2 + t.anchor.offset(self.img.w as i32) + cursor;
            let color = PALETTE[t.pal as usize];
            match &font {
                None => {
                    if let Some(img) = Self::default_glyph(c) {
                        let dark = (color >> 2) & 0x3f3f3f;
                        self.draw(&img, x0 + 1, y + 1, dark);
                        self.draw(&img, x0, y, color);
                    }
                }
                Some(f) => {
                    let down = rows.iter().find(|r| r.id == f).unwrap().down;
                    let sheet = art::sheet_of(c).expect("모르는 글자");
                    let img = sheet
                        .glyphs
                        .iter()
                        .find(|(g, _)| *g == c)
                        .map(|(_, i)| i.clone())
                        .unwrap();
                    let img = if sheet.height < img.h {
                        img.half()
                    } else {
                        img
                    };
                    self.draw(&img, x0, y + down - sheet.lift, color);
                }
            }
            cursor += adv;
        }
    }

    /// `scale`배 키운 PNG.
    pub fn png(&self, scale: usize) -> Vec<u8> {
        let (w, h) = (self.img.w * scale, self.img.h * scale);
        let mut big = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                big.set(x, y, self.img.get(x / scale, y / scale));
            }
        }
        png(w as u32, h as u32, &big.rgba())
    }
}

#[test]
#[ignore]
fn hud_preview() {
    use crate::plugins::quest::{HudQuest, Kind};
    let Ok(path) = std::env::var("MC_HUD_PREVIEW") else {
        return;
    };
    let (w, h) = (480, 270);
    let mut s = Screen::new(w, h);
    let quests = vec![
        HudQuest {
            kind: Kind::Weekly,
            name: "나무 베기".into(),
            progress: 12,
            target: 30,
            done: false,
        },
        HudQuest {
            kind: Kind::Guild,
            name: "철광석 캐기".into(),
            progress: 40,
            target: 200,
            done: false,
        },
        HudQuest {
            kind: Kind::Normal,
            name: "농부의 부탁".into(),
            progress: 5,
            target: 5,
            done: true,
        },
    ];
    let top = super::top(
        Some(&super::Currencies {
            money: 1_234_567,
            cash: 120,
            afk: 45,
        }),
        Some(&quests),
    );
    let stats = crate::plugins::rpg::Stats {
        level: 12,
        attack: 7.0,
        defense: 8.0,
        max_health: 20.0,
        speed: 100.0,
        max_mana: 155.0,
    };
    let bottom = super::bottom(&super::Bottom {
        stats: Some(&stats),
        health: Some((14.0, 20.0)),
        mana: Some((96.0, 155.0)),
    });
    for (i, line) in top.iter().enumerate() {
        s.draw_line(line, 3 + 19 * i as i32);
    }
    s.draw_line(&bottom, h as i32 - 72);
    std::fs::write(path, s.png(3)).unwrap();
}
