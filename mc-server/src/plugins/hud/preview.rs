//! 미리보기: 클라이언트가 할 배치(공백 폭, 줄마다 ascent, 기준점·색 표시, 셰이더의 색 입히기)를
//! 흉내 내어 GUI 화면 한 장을 그립니다. `MC_HUD_PREVIEW=<경로.png> cargo test hud_preview -- --ignored`
//!
//! GUI 배율 2로 그립니다(한 GUI 픽셀 = 2×2): 반으로 줄여 그리는 unifont 한글·라벨이 실제 게임처럼
//! 칸 그대로 보입니다. 바뀌는 글은 바닐라 ascii.png와 unifont 그림([`super::glyphs`])으로 그리고,
//! 그 자료에 없는 글자만 회색 네모로 대신합니다 (자리·폭은 같음).

use super::art::{self, Image, PALETTE, all_rows};
use super::canvas::{advance_in, walk};
use super::glyphs::{ASCII_GLYPHS, PREVIEW_GLYPHS};
use crate::protocol::nbt::Tag;
use crate::resource_pack::png;

/// GUI 픽셀 하나가 몇 칸인가.
const SCALE: i32 = 2;

pub struct Screen {
    img: Image,
    /// GUI 크기.
    w: i32,
    h: i32,
}

impl Screen {
    pub fn new(w: usize, h: usize) -> Self {
        let s = SCALE as usize;
        let mut img = Image::new(w * s, h * s);
        for y in 0..h * s {
            for x in 0..w * s {
                let (gx, gy) = (x / s, y / s);
                // 하늘 → 풀밭
                let c = if gy < h * 3 / 5 {
                    0x86b7f0ff - ((gy as u32 / 8) << 16)
                } else if (gx / 16 + gy / 16) % 2 == 0 {
                    0x5d9b3aff
                } else {
                    0x558f35ff
                };
                img.set(x, y, c);
            }
        }
        let mut sc = Self {
            img,
            w: w as i32,
            h: h as i32,
        };
        // 핫바 자리
        let (cx, hb) = (sc.w / 2, sc.h - 22);
        for y in hb..sc.h {
            for x in cx - 91..cx + 91 {
                let edge = y == hb || x == cx - 91 || x == cx + 90 || (x - cx + 91) % 20 == 0;
                sc.fill_gui(x, y, if edge { 0x202020ff } else { 0x8b8b8bff });
            }
        }
        sc
    }

    fn fill_gui(&mut self, x: i32, y: i32, c: u32) {
        for dy in 0..SCALE {
            for dx in 0..SCALE {
                self.blend(x * SCALE + dx, y * SCALE + dy, c);
            }
        }
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

    /// 그림을 GUI 좌표 (x, y)에, 그림 한 픽셀을 `px`칸으로 (1이면 반 GUI 픽셀), 색을 곱해서.
    fn draw(&mut self, img: &Image, x: i32, y: i32, px: i32, tint: u32) {
        for gy in 0..img.h {
            for gx in 0..img.w {
                let p = img.get(gx, gy);
                if p & 0xff == 0 {
                    continue;
                }
                let ch = |s: u32| ((p >> s & 0xff) * (tint >> (s - 8) & 0xff) / 255) << s;
                let c = ch(24) | ch(16) | ch(8) | (p & 0xff);
                for dy in 0..px {
                    for dx in 0..px {
                        self.blend(
                            x * SCALE + gx as i32 * px + dx,
                            y * SCALE + gy as i32 * px + dy,
                            c,
                        );
                    }
                }
            }
        }
    }

    /// 바닐라 글자 그림과 그 한 픽셀의 칸 수.
    fn vanilla_glyph(c: char) -> Option<(Image, i32)> {
        let u = c as usize;
        if u < 128 {
            let mut img = Image::new(8, 8);
            for (y, bits) in ASCII_GLYPHS[u].iter().enumerate() {
                for x in 0..8 {
                    if bits & (0x80 >> x) != 0 {
                        img.set(x, y, 0xffffffff);
                    }
                }
            }
            return Some((img, SCALE));
        }
        if let Some((_, rows)) = PREVIEW_GLYPHS.iter().find(|(g, _)| *g == c) {
            // unifont 한글: 16×16을 반으로, 바닐라 설정대로 1열부터
            let mut img = Image::new(15, 16);
            for (y, bits) in rows.iter().enumerate() {
                for x in 1..16 {
                    if bits & (0x8000 >> x) != 0 {
                        img.set(x - 1, y, 0xffffffff);
                    }
                }
            }
            return Some((img, 1));
        }
        // 자료에 없는 글자: 자리만
        let w = art::default_advance(c)? - 1;
        let mut img = Image::new(w as usize, 7);
        for y in 0..7 {
            for x in 0..w as usize {
                img.set(x, y, 0xb0b0b0c0);
            }
        }
        Some((img, SCALE))
    }

    /// 텍스트 한 줄을 GUI `y`(보스바 제목이면 3 + 19×줄, 액션바면 화면 높이 - 72)에.
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
            let x0 = self.w / 2 + t.anchor.offset(self.w) + cursor;
            let color = PALETTE[t.pal as usize];
            match &font {
                None => {
                    if let Some((img, px)) = Self::vanilla_glyph(c) {
                        // 그림자: 1 GUI 픽셀 아래·오른쪽, 색의 1/4
                        let dark = (color >> 2) & 0x3f3f3f;
                        self.draw(&img, x0 + 1, y + 1, px, dark);
                        self.draw(&img, x0, y, px, color);
                    }
                }
                Some(f) => {
                    let down = rows.iter().find(|r| r.id == f).unwrap().down;
                    let sheet = art::sheet_of(c).expect("모르는 글자");
                    let img = &sheet.glyphs.iter().find(|(g, _)| *g == c).unwrap().1;
                    let px = SCALE * sheet.height as i32 / img.h as i32;
                    self.draw(img, x0, y + down - sheet.lift, px, color);
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
        attack: 186.0,
        magic: 42.0,
        defense: 64.0,
        max_health: 455.0,
        max_mana: 155.0,
        crit_chance: 18.0,
        crit_damage: 175.0,
        speed: 100.0,
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
    std::fs::write(path, s.png(2)).unwrap();
}
