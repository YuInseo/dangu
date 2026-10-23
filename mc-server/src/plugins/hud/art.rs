//! HUD 그림: 글자·아이콘·판·막대 그림판, 줄마다의 폰트, 그리고 글자를 화면 왼쪽·오른쪽 끝으로
//! 옮기고 색을 입히는 텍스트 셰이더.
//!
//! 마인크래프트는 보스바 제목과 액션바를 화면 가운데에 그립니다. 그래서
//! - 줄(세로 위치)은 폰트마다 `ascent`를 달리 해서 정하고 (`hoya:hud/t0`, `b3`, …),
//! - 가로 위치는 공백 글자(`space` 공급자)로 정하고,
//! - 글자 색 `#FD gg bb`를 표시로 씁니다: `gg`가 기준점([`Anchor`]: 08 오른쪽 끝, 0C 가운데,
//!   10 왼쪽 끝), `bb - 0xF0`이 [`PALETTE`]의 색 번호. 셰이더가 글자를 화면 반 폭만큼 옮기고
//!   (화면 크기를 서버가 몰라도 끝에 붙음) 진짜 색을 입힙니다.
//! - 기본 폰트 글(퀘스트 이름)의 그림자는 `shadow_color`를 `#FC gg bb`로 주면 셰이더가 같이 옮기고
//!   어둡게 칠합니다. 그 밖의 그림자(바닐라가 색의 1/4로 만드는 것)는 지웁니다.
//!
//! 바닐라 하트·갑옷 아이콘은 투명 그림으로 덮어 숨기고 (대신 체력·마나바, 방어력은 스탯 판),
//! 분홍 보스바의 막대도 투명하게 합니다 (HUD의 위쪽 줄들은 분홍 보스바 제목으로 띄움).

use std::collections::HashMap;
use std::sync::OnceLock;

use super::glyphs::{ASCII_ADVANCE, LABEL_GLYPHS};
use super::sprites::{ICONS, LETTERS};
use crate::plugins::contents::source::Pack;
use crate::resource_pack::png;

// ---- 기준점과 색 ----

/// 글자를 어디에 붙이나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Left,
    Center,
    Right,
}

impl Anchor {
    fn green(self) -> u32 {
        match self {
            Anchor::Right => 0x08,
            Anchor::Center => 0x0C,
            Anchor::Left => 0x10,
        }
    }

    /// 화면 가운데에서 이 기준점까지 (미리보기용).
    pub fn offset(self, screen_w: i32) -> i32 {
        match self {
            Anchor::Right => screen_w / 2,
            Anchor::Center => 0,
            Anchor::Left => -screen_w / 2,
        }
    }
}

/// [`PALETTE`]의 색 이름.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pal {
    White,
    Light,
    Gray,
    Dark,
    Gold,
    Yellow,
    Green,
    Aqua,
    Red,
    Purple,
    Orange,
    Blue,
    Pink,
    Mint,
    Cream,
    Black,
}

/// 셰이더가 입히는 16색 (그림판의 색에 곱함).
pub const PALETTE: [u32; 16] = [
    0xFFFFFF, 0xD2D2DC, 0x8C8C9A, 0x3A3A48, 0xFFC94A, 0xFFF07A, 0x7BE07B, 0x6FE3FF, 0xFF5A5A,
    0xC79BFF, 0xFF9F43, 0x5AA0FF, 0xFF8FD0, 0x9CF2C8, 0xFFF2D6, 0x000000,
];

/// [`Pal`]을 번호 순서대로.
pub const PALS: [Pal; 16] = [
    Pal::White,
    Pal::Light,
    Pal::Gray,
    Pal::Dark,
    Pal::Gold,
    Pal::Yellow,
    Pal::Green,
    Pal::Aqua,
    Pal::Red,
    Pal::Purple,
    Pal::Orange,
    Pal::Blue,
    Pal::Pink,
    Pal::Mint,
    Pal::Cream,
    Pal::Black,
];

/// 기준점 + 색.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tint {
    pub anchor: Anchor,
    pub pal: Pal,
}

pub const fn tint(anchor: Anchor, pal: Pal) -> Tint {
    Tint { anchor, pal }
}

impl Tint {
    /// 글자 색 표시 `#FDggbb`.
    pub fn color(self) -> String {
        format!(
            "#FD{:02X}{:02X}",
            self.anchor.green(),
            0xF0 + self.pal as u32
        )
    }

    /// 기본 폰트 글의 그림자 표시 (ARGB, `#FCggbb`).
    pub fn shadow(self) -> i32 {
        (0xFF00_0000 | 0xFC << 16 | self.anchor.green() << 8 | (0xF0 + self.pal as u32)) as i32
    }

    /// `#FDggbb` → 표시 (미리보기용).
    pub fn parse(color: &str) -> Option<Tint> {
        let v = u32::from_str_radix(color.strip_prefix("#FD")?, 16).ok()?;
        let anchor = match v >> 8 {
            0x08 => Anchor::Right,
            0x0C => Anchor::Center,
            0x10 => Anchor::Left,
            _ => return None,
        };
        let i = (v & 0xff).checked_sub(0xF0)? as usize;
        Some(Tint {
            anchor,
            pal: *PALS.get(i)?,
        })
    }
}

// ---- 줄 ----

/// 한 줄의 폰트: 이름과, 줄(보스바 제목·액션바 글)의 위쪽에서 아래로 내린 픽셀.
pub struct RowFont {
    pub id: &'static str,
    pub down: i32,
}

const fn row(id: &'static str, down: i32) -> RowFont {
    RowFont { id, down }
}

/// 보스바 제목 줄 (화면 위에서 3, 22, 41, 60px — 줄마다 같은 폰트를 씀).
pub const T0: RowFont = row("hoya:hud/t0", 0);
pub const T1: RowFont = row("hoya:hud/t1", 12);
pub const T2: RowFont = row("hoya:hud/t2", 24);
/// 퀘스트 카드 바탕 (제목 줄보다 3px 위에서 17px).
pub const TQ: RowFont = row("hoya:hud/tq", -3);
/// 퀘스트 진행 막대 (제목 글 아래).
pub const TB: RowFont = row("hoya:hud/tb", 10);
/// 액션바 글 줄 (화면 아래에서 72px): 오른쪽 아래 스탯 여섯 줄, 마지막 줄이 바닥 1px 위.
pub const BOTTOM: [RowFont; 6] = [
    row("hoya:hud/b0", 1),
    row("hoya:hud/b1", 13),
    row("hoya:hud/b2", 25),
    row("hoya:hud/b3", 37),
    row("hoya:hud/b4", 49),
    row("hoya:hud/b5", 61),
];
/// 바닐라 하트 자리 (화면 아래에서 39px, 핫바 왼쪽 위).
pub const HP: RowFont = row("hoya:hud/hp", 33);
/// 그 위 바닐라 갑옷 아이콘 자리.
pub const MP: RowFont = row("hoya:hud/mp", 23);

/// 바뀌는 글(퀘스트 이름 등)의 폰트: 바닐라 기본 폰트의 글자들을 그대로 참조하되, 바닐라와 달리
/// "유니코드 글꼴 강제" 옵션(`uniform` 거르기)을 타지 않아 폭이 늘 [`default_advance`]와 같습니다.
/// 줄의 원래 높이(위쪽 = 줄 위쪽)에 그려지고, HUD 공백 글자도 들어 있습니다.
pub const TEXT_FONT: &str = "hoya:hud/text";

pub fn all_rows() -> Vec<&'static RowFont> {
    let mut v = vec![&T0, &T1, &T2, &TQ, &TB, &HP, &MP];
    v.extend(BOTTOM.iter());
    v
}

// ---- 글자 번호 ----

/// 판: `E500 + 16 × 종류` 부터 왼쪽 끝, 오른쪽 끝, 가운데 1·2·4·…·64px.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strip {
    /// 반투명 검정 카드 (11px, 둥근 모서리)
    Card11,
    /// 퀘스트 카드 (17px)
    Card17,
    /// 흰색 (색을 입혀 씀): 카드 왼쪽 줄무늬
    Solid11,
    Solid17,
    /// 진행 막대 (3px)
    Solid3,
}

const STRIPS: [Strip; 5] = [
    Strip::Card11,
    Strip::Card17,
    Strip::Solid11,
    Strip::Solid17,
    Strip::Solid3,
];

impl Strip {
    fn index(self) -> u32 {
        STRIPS.iter().position(|s| *s == self).unwrap() as u32
    }
    pub fn height(self) -> usize {
        match self {
            Strip::Card11 | Strip::Solid11 => 11,
            Strip::Card17 | Strip::Solid17 => 17,
            Strip::Solid3 => 3,
        }
    }
    /// 줄 위쪽보다 몇 px 위에서 시작하나.
    fn lift(self) -> i32 {
        match self {
            Strip::Card11 | Strip::Solid11 => 1,
            _ => 0,
        }
    }
    fn card(self) -> bool {
        matches!(self, Strip::Card11 | Strip::Card17)
    }
    /// 끝 조각 폭.
    pub fn cap_width(self) -> i32 {
        if self.card() { 2 } else { 1 }
    }
    pub fn cap(self, right: bool) -> char {
        char::from_u32(0xE500 + self.index() * 16 + u32::from(right)).unwrap()
    }
    /// 가운데 조각 2^k px (k = 0..=6).
    pub fn mid(self, k: u32) -> char {
        char::from_u32(0xE500 + self.index() * 16 + 2 + k).unwrap()
    }
}

/// 막대 (체력·마나 같이, 색은 셰이더가): 틀 `E300`, 채움 `E301..` 1..=[`BAR_INNER`]칸.
pub const BAR_FRAME: char = '\u{E300}';
pub const BAR_W: i32 = 71;
pub const BAR_INNER: i32 = BAR_W - 2;

pub fn bar_fill(cells: i32) -> Option<char> {
    (1..=BAR_INNER)
        .contains(&cells)
        .then(|| char::from_u32(0xE300 + cells as u32).unwrap())
}

/// 라벨 한글 (`E600`부터, [`LABEL_GLYPHS`] 순서).
pub fn label_char(c: char) -> Option<char> {
    LABEL_GLYPHS
        .iter()
        .position(|(g, _)| *g == c)
        .map(|i| char::from_u32(0xE600 + i as u32).unwrap())
}

/// 공백: `E400+k` = +2^k, `E410+k` = -2^k (k = 0..=10).
pub fn space_char(k: u32, negative: bool) -> char {
    char::from_u32(0xE400 + if negative { 0x10 } else { 0 } + k).unwrap()
}

/// 공백 글자의 폭 (공백이 아니면 None).
pub fn space_advance(c: char) -> Option<i32> {
    let u = c as u32;
    match u {
        0xE400..=0xE40A => Some(1 << (u - 0xE400)),
        0xE410..=0xE41A => Some(-(1 << (u - 0xE410))),
        _ => None,
    }
}

/// 바닐라 기본 폰트의 글자 폭 (모르는 글자는 None: HUD에 넣지 않음).
pub fn default_advance(c: char) -> Option<i32> {
    let u = c as u32;
    match u {
        0..=127 => Some(i32::from(ASCII_ADVANCE[u as usize])).filter(|w| *w > 0),
        // 바닐라 unifont 설정: 한글 음절 left 1 right 15 → 15/2 + 1
        0xAC00..=0xD7A3 => Some(8),
        // 자모·한자·가나 (left 0 right 15)
        0x1100..=0x11FF | 0x3001..=0x30FF | 0x3130..=0x318F | 0x3200..=0x9FFF => Some(9),
        _ => None,
    }
}

pub fn default_width(s: &str) -> i32 {
    s.chars().filter_map(default_advance).sum()
}

// ---- 그림 ----

/// 글자 테두리 색과 글자 색.
const OUTLINE: u32 = 0x1e1e1eff;
const FILL: u32 = 0xffffffff;

/// RGBA 그림 한 장.
#[derive(Clone)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    px: Vec<u32>,
}

impl Image {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![0; w * h],
        }
    }
    pub fn set(&mut self, x: usize, y: usize, c: u32) {
        if x < self.w && y < self.h {
            self.px[y * self.w + x] = c;
        }
    }
    pub fn get(&self, x: usize, y: usize) -> u32 {
        self.px[y * self.w + x]
    }
    pub fn blit(&mut self, src: &Image, ox: usize, oy: usize) {
        for y in 0..src.h {
            for x in 0..src.w {
                self.set(ox + x, oy + y, src.get(x, y));
            }
        }
    }
    pub fn rgba(&self) -> Vec<u8> {
        self.px.iter().flat_map(|p| p.to_be_bytes()).collect()
    }
    /// 바닐라 폰트가 재는 글자 폭: 오른쪽에서부터 처음 불투명한 열 + 1.
    pub fn ink_width(&self) -> i32 {
        (0..self.w)
            .rev()
            .find(|&x| (0..self.h).any(|y| self.get(x, y) & 0xff != 0))
            .map_or(0, |x| x as i32 + 1)
    }
    /// 반으로 줄임 (2×2 중 하나라도 칠해졌으면 칠함; 미리보기용).
    pub fn half(&self) -> Image {
        let mut out = Image::new(self.w / 2, self.h / 2);
        for y in 0..out.h {
            for x in 0..out.w {
                let c = [(0, 0), (1, 0), (0, 1), (1, 1)]
                    .iter()
                    .map(|(dx, dy)| self.get(x * 2 + dx, y * 2 + dy))
                    .max_by_key(|p| p & 0xff)
                    .unwrap();
                out.set(x, y, c);
            }
        }
        out
    }
}

fn letter(rows: &[&str; 7]) -> Image {
    let w = rows[0].len() + 2;
    let mut img = Image::new(w, 9);
    let filled = |x: i32, y: i32| {
        x >= 1
            && y >= 1
            && (y as usize) <= 7
            && rows[y as usize - 1].as_bytes().get(x as usize - 1) == Some(&b'#')
    };
    for y in 0..9i32 {
        for x in 0..w as i32 {
            if filled(x, y) {
                img.set(x as usize, y as usize, FILL);
            } else if (-1..=1).any(|dy| (-1..=1).any(|dx| filled(x + dx, y + dy))) {
                img.set(x as usize, y as usize, OUTLINE);
            }
        }
    }
    img
}

fn icon(rows: &[&str; 9], palette: &[(char, u32)]) -> Image {
    let mut img = Image::new(9, 9);
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if let Some((_, c)) = palette.iter().find(|(p, _)| *p == ch) {
                img.set(x, y, *c);
            }
        }
    }
    img
}

/// 라벨 한글 16×16 (흰색, 가장자리 어둡게).
fn label_image(bits: &[u16; 16]) -> Image {
    let mut img = Image::new(16, 16);
    let on = |x: i32, y: i32| {
        (0..16).contains(&x) && (0..16).contains(&y) && bits[y as usize] & (0x8000 >> x) != 0
    };
    for y in 0..16 {
        for x in 0..16 {
            if on(x, y) {
                img.set(x as usize, y as usize, FILL);
            }
        }
    }
    img
}

/// 판 조각. `cap`: 왼쪽(false)·오른쪽(true) 끝이면 바깥 열의 모서리를 비움.
fn strip_piece(strip: Strip, width: usize, cap: Option<bool>) -> Image {
    let h = strip.height();
    let mut img = Image::new(width, h);
    for y in 0..h {
        for x in 0..width {
            let outer = match cap {
                Some(false) => x == 0,
                Some(true) => x + 1 == width,
                None => false,
            };
            if outer && (y == 0 || y + 1 == h) {
                continue;
            }
            let c = if strip.card() {
                match y {
                    0 => 0x3c3c50b4,
                    _ if y + 1 == h => 0x05050ac0,
                    _ => 0x12121ca8,
                }
            } else {
                0xffffffff
            };
            img.set(x, y, c);
        }
    }
    img
}

fn bar_frame() -> Image {
    let w = BAR_W as usize;
    let mut img = Image::new(w, 9);
    for y in 0..9 {
        for x in 0..w {
            let border = x == 0 || y == 0 || x + 1 == w || y == 8;
            let corner = (x == 0 || x + 1 == w) && (y == 0 || y == 8);
            if corner {
                continue;
            }
            img.set(x, y, if border { 0x101014e6 } else { 0x26262ec8 });
        }
    }
    img
}

/// 채움 (회색조: 셰이더가 빨강·파랑을 곱함). 위가 밝고 아래로 어두워짐.
fn bar_fill_image(cells: usize) -> Image {
    let mut img = Image::new(BAR_W as usize, 9);
    for x in 1..=cells {
        for y in 1..8 {
            let v: u32 = match y {
                1 => 0xff,
                2 => 0xf0,
                3..=5 => 0xd8,
                6 => 0xb4,
                _ => 0x90,
            };
            // 끝 열은 조금 밝게 (차오르는 끝)
            let v = if x == cells { (v + 0x20).min(0xff) } else { v };
            img.set(x, y, v << 24 | v << 16 | v << 8 | 0xff);
        }
    }
    img
}

// ---- 폰트 ----

/// 그림판 한 장 = 폰트의 bitmap 공급자 하나.
pub struct Sheet {
    file: &'static str,
    cell: (usize, usize),
    /// 화면에서의 높이 (cell 높이와 다르면 그만큼 줄여 그림).
    pub height: usize,
    per_row: usize,
    /// 줄 위쪽보다 몇 px 위에서 그리기 시작하나.
    pub lift: i32,
    pub glyphs: Vec<(char, Image)>,
}

impl Sheet {
    fn image(&self) -> Image {
        let rows = self.glyphs.len().div_ceil(self.per_row);
        let mut img = Image::new(self.cell.0 * self.per_row, self.cell.1 * rows);
        for (i, (_, g)) in self.glyphs.iter().enumerate() {
            img.blit(
                g,
                (i % self.per_row) * self.cell.0,
                (i / self.per_row) * self.cell.1,
            );
        }
        img
    }
    fn chars_json(&self) -> String {
        let rows: Vec<String> = self
            .glyphs
            .chunks(self.per_row)
            .map(|chunk| {
                let mut s: String = chunk.iter().map(|(c, _)| *c).collect();
                // 마지막 줄은 빈 글자(\0)로 채웁니다
                while s.chars().count() < self.per_row {
                    s.push('\0');
                }
                format!("\"{}\"", json_chars(&s))
            })
            .collect();
        format!("[{}]", rows.join(","))
    }
    /// 이 줄 폰트에 넣을 수 있나 (바닐라: ascent ≤ height).
    pub fn fits(&self, row: &RowFont) -> bool {
        self.ascent(row) <= self.height as i32
    }
    fn ascent(&self, row: &RowFont) -> i32 {
        // 줄의 위쪽 = 기준 + down; 바닐라 글자의 위쪽은 기준 + 7 - ascent
        7 - row.down + self.lift
    }
    /// 바닐라 bitmap 공급자의 글자 폭: (잉크 폭 × 배율 + 0.5) 버림 + 1.
    fn advance(&self, img: &Image) -> i32 {
        let scale = self.height as f32 / self.cell.1 as f32;
        (0.5 + img.ink_width() as f32 * scale) as i32 + 1
    }
}

pub fn json_chars(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '"' => "\\\"".to_owned(),
            '\\' => "\\\\".to_owned(),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => format!("\\u{:04x}", c as u32),
            c => c.to_string(),
        })
        .collect()
}

pub fn sheets() -> &'static [Sheet] {
    static SHEETS: OnceLock<Vec<Sheet>> = OnceLock::new();
    SHEETS.get_or_init(|| {
        let mut small: Vec<(char, Image)> = LETTERS.iter().map(|(c, r)| (*c, letter(r))).collect();
        small.extend(ICONS.iter().map(|(c, rows, pal)| (*c, icon(rows, pal))));
        let labels = LABEL_GLYPHS
            .iter()
            .map(|(c, bits)| (label_char(*c).unwrap(), label_image(bits)))
            .collect();
        let mut out = vec![
            Sheet {
                file: "hoya:font/hud/small.png",
                cell: (9, 9),
                height: 9,
                per_row: 16,
                lift: 0,
                glyphs: small,
            },
            Sheet {
                file: "hoya:font/hud/label.png",
                cell: (16, 16),
                height: 8,
                per_row: 16,
                lift: 0,
                glyphs: labels,
            },
        ];
        for s in STRIPS {
            let mut glyphs = vec![
                (
                    s.cap(false),
                    strip_piece(s, s.cap_width() as usize, Some(false)),
                ),
                (
                    s.cap(true),
                    strip_piece(s, s.cap_width() as usize, Some(true)),
                ),
            ];
            for k in 0..=6 {
                glyphs.push((s.mid(k), strip_piece(s, 1 << k, None)));
            }
            out.push(Sheet {
                file: match s {
                    Strip::Card11 => "hoya:font/hud/card11.png",
                    Strip::Card17 => "hoya:font/hud/card17.png",
                    Strip::Solid11 => "hoya:font/hud/solid11.png",
                    Strip::Solid17 => "hoya:font/hud/solid17.png",
                    Strip::Solid3 => "hoya:font/hud/solid3.png",
                },
                cell: (64, s.height()),
                height: s.height(),
                per_row: 9,
                lift: s.lift(),
                glyphs,
            });
        }
        let mut bar = vec![(BAR_FRAME, bar_frame())];
        for k in 1..=BAR_INNER {
            bar.push((bar_fill(k).unwrap(), bar_fill_image(k as usize)));
        }
        out.push(Sheet {
            file: "hoya:font/hud/bar.png",
            cell: (BAR_W as usize, 9),
            height: 9,
            per_row: 10,
            lift: 0,
            glyphs: bar,
        });
        out
    })
}

/// 그 글자가 있는 그림판.
pub fn sheet_of(c: char) -> Option<&'static Sheet> {
    static INDEX: OnceLock<HashMap<char, usize>> = OnceLock::new();
    let i = INDEX.get_or_init(|| {
        sheets()
            .iter()
            .enumerate()
            .flat_map(|(i, s)| s.glyphs.iter().map(move |(c, _)| (*c, i)))
            .collect()
    });
    i.get(&c).map(|&i| &sheets()[i])
}

/// 그림판 글자가 화면에서 차지하는 폭 (공백 글자 포함; 모르는 글자는 0).
pub fn advance(c: char) -> i32 {
    static TABLE: OnceLock<HashMap<char, i32>> = OnceLock::new();
    if let Some(a) = space_advance(c) {
        return a;
    }
    let t = TABLE.get_or_init(|| {
        sheets()
            .iter()
            .flat_map(|s| s.glyphs.iter().map(move |(c, g)| (*c, s.advance(g))))
            .collect()
    });
    t.get(&c).copied().unwrap_or(0)
}

pub fn space_provider() -> String {
    let mut spaces = Vec::new();
    for k in 0..=10u32 {
        for neg in [false, true] {
            spaces.push(format!(
                "\"{}\":{}{}",
                json_chars(&space_char(k, neg).to_string()),
                if neg { "-" } else { "" },
                1 << k
            ));
        }
    }
    format!(
        "{{\"type\":\"space\",\"advances\":{{{}}}}}",
        spaces.join(",")
    )
}

fn font_json(row: &RowFont) -> String {
    let mut providers = vec![space_provider()];
    for s in sheets().iter().filter(|s| s.fits(row)) {
        providers.push(format!(
            "{{\"type\":\"bitmap\",\"file\":\"{}\",\"ascent\":{},\"height\":{},\"chars\":{}}}",
            s.file,
            s.ascent(row),
            s.height,
            s.chars_json()
        ));
    }
    format!("{{\"providers\":[{}]}}", providers.join(","))
}

fn text_font_json() -> String {
    format!(
        "{{\"providers\":[{},{{\"type\":\"reference\",\"id\":\"minecraft:include/space\"}},\
         {{\"type\":\"reference\",\"id\":\"minecraft:include/default\"}},\
         {{\"type\":\"reference\",\"id\":\"minecraft:include/unifont\"}}]}}",
        space_provider()
    )
}

fn palette_glsl() -> String {
    let colors: Vec<String> = PALETTE
        .iter()
        .map(|c| {
            format!(
                "vec3({:.4}, {:.4}, {:.4})",
                (c >> 16 & 0xff) as f32 / 255.0,
                (c >> 8 & 0xff) as f32 / 255.0,
                (c & 0xff) as f32 / 255.0
            )
        })
        .collect();
    format!(
        "const vec3 HOYA_PALETTE[16] = vec3[16](\n    {}\n);",
        colors.join(",\n    ")
    )
}

/// 바닐라 26.3 `core/text.vsh`에 표시 색 처리를 더한 것 (HUD와 [`crate::plugins::gui`] 창이 같이 씀;
/// HUD를 꺼도 창을 위해 늘 리소스 팩에 들어감).
pub fn text_vsh() -> String {
    TEXT_VSH.replace("//PALETTE//", &palette_glsl())
}

const TEXT_VSH: &str = r#"#version 330
#extension GL_ARB_separate_shader_objects : require

#if !defined(IS_GUI) && !defined(IS_SEE_THROUGH)
#include <minecraft:fog.glsl>
#include <minecraft:sample_lightmap.glsl>
#endif

#include <minecraft:dynamictransforms.glsl>
#include <minecraft:projection.glsl>

layout(location = 0) in vec3 Position;
layout(location = 1) in vec4 Color;
layout(location = 2) in vec2 UV0;
#if !defined(IS_GUI) && !defined(IS_SEE_THROUGH)
layout(location = 3) in ivec2 UV2;
#endif

#if !defined(IS_GUI) && !defined(IS_SEE_THROUGH)
uniform sampler2D Sampler2;
layout(location = 0) out float sphericalVertexDistance;
layout(location = 1) out float cylindricalVertexDistance;
#endif

layout(location = 2) out vec4 vertexColor;
layout(location = 3) out vec2 texCoord0;

//PALETTE//

void main() {
    gl_Position = ProjMat * ModelViewMat * vec4(Position, 1.0);

#if !defined(IS_GUI) && !defined(IS_SEE_THROUGH)
    sphericalVertexDistance = fog_spherical_distance(Position);
    cylindricalVertexDistance = fog_cylindrical_distance(Position);
    vertexColor = Color * sample_lightmap(Sampler2, UV2);
#else
    vertexColor = Color;
#endif
    texCoord0 = UV0;

    // hoya HUD: #FD gg bb = 글자, #FC gg bb = 그 그림자.
    // gg: 08 화면 오른쪽 끝으로, 0C 그대로, 10 왼쪽 끝으로. bb - 240: 색 번호.
    ivec3 hoya_c = ivec3(round(Color.rgb * 255.0));
    bool hoya_anchor = hoya_c.g == 8 || hoya_c.g == 12 || hoya_c.g == 16;
    if ((hoya_c.r == 253 || hoya_c.r == 252) && hoya_anchor && hoya_c.b >= 240) {
        if (hoya_c.g == 8) {
            gl_Position.x += gl_Position.w;
        } else if (hoya_c.g == 16) {
            gl_Position.x -= gl_Position.w;
        }
        vec3 hoya_p = HOYA_PALETTE[hoya_c.b - 240];
        vertexColor = vec4(hoya_c.r == 252 ? hoya_p * 0.25 : hoya_p, Color.a);
    } else if (hoya_c.r == 250 && hoya_c.b >= 240) {
        // hoya 창 (plugins::gui): #FA gg bb = (gg - 32)px 아래로, bb - 240: 색 번호.
        gl_Position += ProjMat * ModelViewMat * vec4(0.0, float(hoya_c.g - 32), 0.0, 0.0);
        vertexColor = vec4(HOYA_PALETTE[hoya_c.b - 240], Color.a);
    } else if (hoya_c.r == 63 && hoya_c.g >= 2 && hoya_c.g <= 4 && hoya_c.b >= 60) {
        // 바닐라가 만든 표시 색의 그림자 (색 × 1/4)
        vertexColor = vec4(0.0);
    }
}
"#;

/// 숨길 바닐라 하트 그림.
const HEARTS: &[&str] = &[
    "container",
    "container_blinking",
    "container_hardcore",
    "container_hardcore_blinking",
    "full",
    "full_blinking",
    "half",
    "half_blinking",
    "hardcore_full",
    "hardcore_full_blinking",
    "hardcore_half",
    "hardcore_half_blinking",
    "absorbing_full",
    "absorbing_full_blinking",
    "absorbing_half",
    "absorbing_half_blinking",
    "absorbing_hardcore_full",
    "absorbing_hardcore_full_blinking",
    "absorbing_hardcore_half",
    "absorbing_hardcore_half_blinking",
    "poisoned_full",
    "poisoned_full_blinking",
    "poisoned_half",
    "poisoned_half_blinking",
    "poisoned_hardcore_full",
    "poisoned_hardcore_full_blinking",
    "poisoned_hardcore_half",
    "poisoned_hardcore_half_blinking",
    "withered_full",
    "withered_full_blinking",
    "withered_half",
    "withered_half_blinking",
    "withered_hardcore_full",
    "withered_hardcore_full_blinking",
    "withered_hardcore_half",
    "withered_hardcore_half_blinking",
    "frozen_full",
    "frozen_full_blinking",
    "frozen_half",
    "frozen_half_blinking",
    "frozen_hardcore_full",
    "frozen_hardcore_full_blinking",
    "frozen_hardcore_half",
    "frozen_hardcore_half_blinking",
];

/// HUD 콘텐츠 팩.
pub fn pack() -> Pack {
    let mut files = Vec::new();
    for s in sheets() {
        let img = s.image();
        let path = s.file.trim_start_matches("hoya:");
        files.push((
            format!("assets/hoya/textures/{path}"),
            png(img.w as u32, img.h as u32, &img.rgba()),
        ));
    }
    for row in all_rows() {
        let name = row.id.trim_start_matches("hoya:");
        files.push((
            format!("assets/hoya/font/{name}.json"),
            font_json(row).into_bytes(),
        ));
    }
    files.push((
        format!(
            "assets/hoya/font/{}.json",
            TEXT_FONT.trim_start_matches("hoya:")
        ),
        text_font_json().into_bytes(),
    ));
    files.push((
        "assets/hoya/textures/font/hud/label_LICENSE.txt".into(),
        b"label.png glyphs: GNU Unifont 17.0.01, SIL Open Font License 1.1 (https://openfontlicense.org)\n"
            .to_vec(),
    ));
    let clear = |w: u32, h: u32| png(w, h, &vec![0; (w * h * 4) as usize]);
    for h in HEARTS {
        files.push((
            format!("assets/minecraft/textures/gui/sprites/hud/heart/{h}.png"),
            clear(9, 9),
        ));
    }
    for a in ["armor_full", "armor_half", "armor_empty"] {
        files.push((
            format!("assets/minecraft/textures/gui/sprites/hud/{a}.png"),
            clear(9, 9),
        ));
    }
    for b in ["pink_background", "pink_progress"] {
        files.push((
            format!("assets/minecraft/textures/gui/sprites/boss_bar/{b}.png"),
            clear(182, 5),
        ));
    }
    Pack {
        name: "hoya_hud".into(),
        configs: Vec::new(),
        files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_is_well_formed() {
        for (c, rows) in LETTERS {
            let w = rows[0].len();
            assert!(rows.iter().all(|r| r.len() == w), "{c}");
        }
        for (c, rows, pal) in ICONS {
            for r in rows {
                assert_eq!(r.chars().count(), 9, "{c:?} {r}");
                assert!(
                    r.chars()
                        .all(|ch| ch == '.' || pal.iter().any(|(p, _)| *p == ch)),
                    "{c:?} {r}"
                );
            }
        }
        // 숫자 폭: 5 + 테두리 2 + 1
        assert_eq!(advance('0'), 8);
        assert_eq!(advance('.'), 4);
        assert_eq!(advance(super::super::sprites::COIN), 10);
        assert_eq!(advance(Strip::Card11.mid(3)), 9);
        assert_eq!(advance(bar_fill(10).unwrap()), 12);
        assert_eq!(advance(BAR_FRAME), BAR_W + 1);
        assert_eq!(advance(space_char(4, true)), -16);
        // 라벨 한글: 16px을 8px로 → 잉크 폭의 반 + 1
        let ga = label_char('공').unwrap();
        assert!((7..=9).contains(&advance(ga)), "{}", advance(ga));
        // 글자 번호가 겹치지 않음
        let mut seen = std::collections::HashSet::new();
        for s in sheets() {
            for (c, _) in &s.glyphs {
                assert!(seen.insert(*c), "{c:?}");
            }
        }
    }

    #[test]
    fn markers_round_trip() {
        let t = tint(Anchor::Left, Pal::Gold);
        assert_eq!(t.color(), "#FD10F4");
        assert_eq!(Tint::parse(&t.color()), Some(t));
        assert_eq!(tint(Anchor::Right, Pal::White).color(), "#FD08F0");
        assert_eq!(tint(Anchor::Center, Pal::Black).shadow() as u32, 0xFFFC0CFF);
        assert_eq!(default_width("Lv 12"), 6 + 6 + 4 + 6 + 6);
        assert_eq!(default_width("퀘스트"), 24);
    }

    #[test]
    fn fonts_are_valid_json_with_legal_ascent() {
        for row in all_rows() {
            let j = crate::json::parse(&font_json(row)).unwrap();
            let providers = j.get("providers").unwrap().as_array().unwrap();
            assert!(providers.len() > 1, "{}", row.id);
            for p in providers.iter().skip(1) {
                let ascent = p.get("ascent").unwrap().as_f64().unwrap();
                let height = p.get("height").unwrap().as_f64().unwrap();
                assert!(ascent <= height, "{} {ascent} > {height}", row.id);
            }
        }
        let t = crate::json::parse(&text_font_json()).unwrap();
        let refs: Vec<&str> = t
            .get("providers")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p.get("id").and_then(|i| i.as_str()))
            .collect();
        assert_eq!(
            refs,
            [
                "minecraft:include/space",
                "minecraft:include/default",
                "minecraft:include/unifont"
            ]
        );
        let p = pack();
        assert!(
            p.files
                .iter()
                .any(|(n, _)| n == "assets/hoya/font/hud/text.json")
        );
        let vsh = text_vsh();
        assert!(vsh.contains("hoya_c.r == 250"));
        assert!(vsh.contains("HOYA_PALETTE[16] = vec3[16]("));
        assert!(!vsh.contains("//PALETTE//"));
        assert!(
            p.files
                .iter()
                .any(|(n, _)| n.ends_with("hud/heart/full.png"))
        );
    }
}
