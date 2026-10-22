//! 화면 HUD: 오른쪽 위 재화 세 줄, 오른쪽 아래 RPG 스탯, 핫바 위 체력바.
//!
//! ```text
//!                                            ┌──────────────┐
//!                                            │ 🪙  1,234,567 │  돈
//!                                            │ 💎        120 │  캐시
//!                                            │ 🌙         45 │  잠수 포인트
//!                                            └──────────────┘
//!
//!                                            ┌──────────────┐
//!                                            │ ★  Lv     12 │
//!                                            │ ⚔          7 │  공격력
//!                                            │ 🛡          8 │  방어력
//!   ┌────────────── 18/20 ─┐                 │ ♥         20 │  최대 체력
//!   └──────────────────────┘                 │ 👢       100% │  이동 속도
//!   [핫바 ........................]           │ 🔥        209 │  전투력
//!                                            └──────────────┘
//! ```
//!
//! 위쪽은 분홍 보스바의 제목으로, 아래쪽과 체력바는 액션바 글로 1초마다 다시 보냅니다. 그림과
//! 폰트·셰이더는 [`art`]가 리소스 팩에 넣습니다. 리소스 팩을 받지 않은 사람에게는 보내지 않습니다.
//! `/hud`로 재화·스탯 판을 끄고 켭니다 (체력바는 바닐라 하트를 대신하므로 남음).

pub mod art;
mod session;

use std::sync::atomic::{AtomicBool, Ordering};

use art::{BOTTOM, Bar, HP, MP, RIGHT, RowFont, STAY, TOP};

use crate::plugins::rpg::Stats;
use crate::protocol::nbt::Tag;

pub use session::HudState;

static ENABLED: AtomicBool = AtomicBool::new(true);

/// 서버 전체에서 HUD를 쓰나 (`MC_HUD`, 기본 켬). 끄면 리소스 팩에도 넣지 않습니다.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// 화면 오른쪽 끝에서 띄우는 거리.
const MARGIN: i32 = 4;
/// 글자 사이 (테두리가 겹치도록 한 칸 당김).
const TRACKING: i32 = -1;

// ---- 그리기 ----

/// 폭 0인 한 줄에 글자를 원하는 x에 놓습니다. 줄 전체 폭이 0이라 줄의 시작이 곧 화면 가운데이고,
/// [`RIGHT`] 색 글자는 셰이더가 화면 반 폭만큼 옮기므로 그 x가 곧 오른쪽 끝 기준이 됩니다.
#[derive(Default)]
pub struct Canvas {
    runs: Vec<(&'static str, &'static str, String)>,
    cursor: i32,
}

impl Canvas {
    fn push(&mut self, font: &'static str, color: &'static str, c: char) {
        match self.runs.last_mut() {
            Some((f, col, s)) if *f == font && *col == color => s.push(c),
            _ => self.runs.push((font, color, c.to_string())),
        }
    }

    fn move_to(&mut self, font: &'static str, color: &'static str, x: i32) {
        let mut dx = x - self.cursor;
        let negative = dx < 0;
        dx = dx.abs();
        for k in (0..=10u32).rev() {
            while dx >= 1 << k {
                self.push(font, color, art::space_char(k, negative));
                dx -= 1 << k;
            }
        }
        self.cursor = x;
    }

    pub fn glyph(&mut self, row: &RowFont, color: &'static str, x: i32, c: char) {
        self.move_to(row.id, color, x);
        self.push(row.id, color, c);
        self.cursor += art::advance(c);
    }

    /// 글을 x부터. 끝난 x를 돌려줍니다.
    pub fn text(&mut self, row: &RowFont, color: &'static str, x: i32, s: &str) -> i32 {
        let mut x = x;
        for c in s.chars() {
            self.glyph(row, color, x, c);
            x += art::advance(c) + TRACKING;
        }
        x
    }

    /// `x0..x1` 판 (반투명 검정, 둥근 모서리). 줄보다 1px씩 위아래로 큽니다.
    pub fn panel(&mut self, row: &RowFont, color: &'static str, x0: i32, x1: i32) {
        self.glyph(row, color, x0, art::PANEL_LEFT);
        let mut x = x0 + 2;
        let mut left = (x1 - 2) - x;
        for (k, c) in art::PANEL_MID.iter().enumerate().rev() {
            while left >= 1 << k {
                self.glyph(row, color, x, *c);
                x += 1 << k;
                left -= 1 << k;
            }
        }
        self.glyph(row, color, x1 - 2, art::PANEL_RIGHT);
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// 줄 폭을 0으로 되돌리고 텍스트 컴포넌트로.
    pub fn finish(mut self) -> Tag {
        if let Some(&(font, color, _)) = self.runs.last() {
            self.move_to(font, color, 0);
        }
        let parts = self
            .runs
            .into_iter()
            .map(|(font, color, s)| {
                Tag::Compound(vec![
                    ("text".into(), Tag::String(s)),
                    ("font".into(), Tag::String(font.into())),
                    ("color".into(), Tag::String(color.into())),
                ])
            })
            .collect();
        Tag::Compound(vec![
            ("text".into(), Tag::String(String::new())),
            ("extra".into(), Tag::List(parts)),
        ])
    }
}

pub fn text_width(s: &str) -> i32 {
    s.chars().map(|c| art::advance(c) + TRACKING).sum::<i32>() - TRACKING
}

/// `1,234,567`. 너무 길면 `12.3M`처럼.
pub fn number(n: i64) -> String {
    let (sign, n) = if n < 0 { ("-", -n) } else { ("", n) };
    let full = {
        let digits = n.to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        out
    };
    if full.len() <= 11 {
        return format!("{sign}{full}");
    }
    // 11자(999,999,999)를 넘으면 10억 단위로
    format!("{sign}{:.1}B", n as f64 / 1e9)
}

/// 아이콘과 값이 있는 판 한 묶음 (오른쪽 끝에 붙임).
fn column(c: &mut Canvas, rows: &[RowFont], items: &[(char, String)]) {
    let widths: Vec<i32> = items
        .iter()
        .map(|(_, v)| art::advance(art::COIN) + 3 + text_width(v))
        .collect();
    let w = widths.iter().copied().max().unwrap_or(0);
    let x1 = -MARGIN;
    let x0 = x1 - w;
    for (row, (icon, value)) in rows.iter().zip(items) {
        c.panel(row, RIGHT, x0 - 3, x1 + 3);
        c.glyph(row, RIGHT, x0, *icon);
        c.text(row, RIGHT, x1 - text_width(value), value);
    }
}

pub struct Currencies {
    pub money: i64,
    pub cash: i64,
    pub afk: i64,
}

/// 오른쪽 위: 돈 · 캐시 · 잠수 포인트.
pub fn top(cur: &Currencies) -> Tag {
    let mut c = Canvas::default();
    column(
        &mut c,
        &TOP,
        &[
            (art::COIN, number(cur.money)),
            (art::GEM, number(cur.cash)),
            (art::MOON, number(cur.afk)),
        ],
    );
    c.finish()
}

/// 아래쪽 한 줄에 들어가는 것들. `None`이면 그 부분을 그리지 않습니다.
pub struct Bottom<'a> {
    pub stats: Option<&'a Stats>,
    /// (지금, 최대)
    pub health: Option<(f32, f32)>,
    pub mana: Option<(f32, f32)>,
}

/// 오른쪽 아래 스탯 판, 핫바 위 체력·마나바.
pub fn bottom(b: &Bottom) -> Tag {
    let mut c = Canvas::default();
    if let Some((now, max)) = b.health {
        bar(&mut c, &HP, Bar::Health, now, max);
    }
    if let Some((now, max)) = b.mana {
        bar(&mut c, &MP, Bar::Mana, now, max);
    }
    let stats = b.stats;
    if let Some(s) = stats {
        column(
            &mut c,
            &BOTTOM,
            &[
                (art::STAR, format!("Lv{}", s.level)),
                (art::SWORD, trim(s.attack)),
                (art::SHIELD, trim(s.defense)),
                (art::HEART, trim(s.max_health)),
                (art::BOOT, format!("{:.0}%", s.speed)),
                (art::FLAME, number(s.combat_power())),
            ],
        );
    }
    c.finish()
}

/// 7.0 → "7", 7.5 → "7.5".
fn trim(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{:.0}", v)
    } else {
        format!("{v:.1}")
    }
}

/// 핫바 왼쪽 위(가운데에서 -91..-10): 바닐라 하트 자리에 체력, 그 위 갑옷 자리에 마나.
fn bar(c: &mut Canvas, row: &RowFont, bar: Bar, now: f32, max: f32) {
    const X: i32 = -91;
    c.glyph(row, STAY, X, bar.frame());
    let ratio = if max > 0.0 {
        (now / max).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let cells = (ratio * art::BAR_INNER as f32).round() as i32;
    // 조금이라도 남았으면 한 칸은 보이게
    if let Some(fill) = bar.fill(cells.max(i32::from(now > 0.0))) {
        c.glyph(row, STAY, X, fill);
    }
    let label = format!("{}/{}", now.ceil() as i64, max.ceil() as i64);
    let mid = X + (art::BAR_INNER + 2) / 2;
    c.text(row, STAY, mid - text_width(&label) / 2, &label);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total_advance(tag: &Tag) -> i32 {
        let Tag::Compound(f) = tag else { panic!() };
        let Some((_, Tag::List(parts))) = f.iter().find(|(k, _)| k == "extra") else {
            panic!()
        };
        let mut sum = 0;
        for p in parts {
            let Tag::Compound(f) = p else { panic!() };
            let Some((_, Tag::String(s))) = f.iter().find(|(k, _)| k == "text") else {
                panic!()
            };
            for ch in s.chars() {
                let u = ch as u32;
                sum += match u {
                    0xE400..=0xE40A => 1 << (u - 0xE400),
                    0xE410..=0xE41A => -(1 << (u - 0xE410)),
                    _ => art::advance(ch),
                };
            }
        }
        sum
    }

    /// 공백을 뺀 보이는 글자들.
    fn visible(tag: &Tag) -> String {
        let Tag::Compound(f) = tag else { panic!() };
        let Some((_, Tag::List(parts))) = f.iter().find(|(k, _)| k == "extra") else {
            panic!()
        };
        let mut out = String::new();
        for p in parts {
            let Tag::Compound(f) = p else { panic!() };
            if let Some((_, Tag::String(s))) = f.iter().find(|(k, _)| k == "text") {
                out.extend(
                    s.chars()
                        .filter(|&c| !(0xE400..=0xE41F).contains(&(c as u32))),
                );
            }
        }
        out
    }

    #[test]
    fn lines_have_zero_width_so_they_center_on_the_screen() {
        let t = top(&Currencies {
            money: 1_234_567,
            cash: 120,
            afk: 45,
        });
        assert_eq!(total_advance(&t), 0);
        let s = crate::plugins::rpg::Stats {
            level: 12,
            attack: 7.0,
            defense: 8.0,
            max_health: 20.0,
            speed: 100.0,
            max_mana: 100.0,
        };
        let b = bottom(&Bottom {
            stats: Some(&s),
            health: Some((18.0, 20.0)),
            mana: Some((42.0, 100.0)),
        });
        assert_eq!(total_advance(&b), 0);
        let shown = visible(&b);
        assert!(
            shown.contains("Lv12") && shown.contains("18/20") && shown.contains("42/100"),
            "{shown}"
        );
        let raw = format!("{b:?}");
        assert!(raw.contains(RIGHT) && raw.contains(STAY));
    }

    #[test]
    fn numbers_read_well() {
        assert_eq!(number(0), "0");
        assert_eq!(number(1234), "1,234");
        assert_eq!(number(1_234_567), "1,234,567");
        assert_eq!(number(-5000), "-5,000");
        assert_eq!(number(999_999_999), "999,999,999");
        assert_eq!(number(12_345_678_901), "12.3B");
        assert_eq!(trim(7.0), "7");
        assert_eq!(trim(7.5), "7.5");
    }
}
