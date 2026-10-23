//! 장비 그림: 부위마다 모양 틀(마스크)에 재질 팔레트를 입히고, 테두리·빛(왼쪽 위)·그늘(오른쪽
//! 아래)을 자동으로 넣습니다. 검·지팡이는 단계마다 모양이 바뀌고(기본 · 넓은 날 · 장식), 보스
//! 악세서리는 보석이 큽니다. 입었을 때의 갑옷 겉모습(64×32), 광석 블록, 재료, 주문서도 여기서.
//!
//! 마스크 글자: `.` 빈칸 · `#` 몸(자동 명암) · `1`~`5` 몸 명암 직접(어둠→빛) · `a`/`A` 장식(보석)·밝은 장식 ·
//! `g`/`G` 손잡이·어두운 손잡이 · `-` 홈(테두리 색) · `w` 반짝임 · `s`/`S`/`x` 돌(중간·어둠·밝음).

use super::Part;
use super::config::{EquipConfig, GearDef, Material, OreDef};
use crate::plugins::contents::source::Pack;
use crate::resource_pack::png;

type Rgba = [u8; 4];

/// 재질 색: 테두리 · 어둠 · 중간 · 밝음 · 빛 · 장식 · 밝은 장식.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub o: Rgba,
    pub d: Rgba,
    pub m: Rgba,
    pub l: Rgba,
    pub h: Rgba,
    pub a: Rgba,
    pub aa: Rgba,
}

const fn rgb(c: u32) -> Rgba {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8, 0xff]
}

const fn pal(o: u32, d: u32, m: u32, l: u32, h: u32, a: u32, aa: u32) -> Palette {
    Palette {
        o: rgb(o),
        d: rgb(d),
        m: rgb(m),
        l: rgb(l),
        h: rgb(h),
        a: rgb(a),
        aa: rgb(aa),
    }
}

/// 이름 → 팔레트. 모르는 이름은 은색.
pub fn palette(name: &str) -> Palette {
    match name {
        "mithril" => pal(
            0x16263a, 0x456b8c, 0x7fb0d6, 0xb9dcf5, 0xf0faff, 0x2fd8ff, 0xb5f6ff,
        ),
        "cobalt" => pal(
            0x0b1236, 0x1d348f, 0x3a64e6, 0x86a6ff, 0xdbe5ff, 0xf2c23a, 0xfff0a8,
        ),
        "orichalcum" => pal(
            0x331505, 0x8f4210, 0xe0832a, 0xffbf6e, 0xfff0c8, 0xe8283a, 0xff9c9c,
        ),
        "soul_crystal" => pal(
            0x052a28, 0x0f6f69, 0x2bcdc1, 0x8ff7ee, 0xeafffd, 0x8a5cff, 0xd8c8ff,
        ),
        "seraphium" => pal(
            0x40310c, 0xae8a2a, 0xf0d266, 0xfff2b6, 0xffffff, 0x3ecbff, 0xd9f6ff,
        ),
        "evilium" => pal(
            0x120309, 0x46091c, 0x9d1638, 0xdf2a50, 0xff9fb2, 0xff7a1a, 0xffd27a,
        ),
        "arcana" => pal(
            0x140827, 0x3a1b70, 0x7640cf, 0xb98cff, 0xf1e6ff, 0x34eaff, 0xbefcff,
        ),
        "goblin_king" => pal(
            0x332405, 0x85650f, 0xd6ae28, 0xffe27a, 0xfffbe0, 0x2fd640, 0xbaffb2,
        ),
        "fallen_knight" => pal(
            0x0a0a0e, 0x2a2a34, 0x535468, 0x8e8fa6, 0xd2d4e8, 0xff2a2a, 0xffa0a0,
        ),
        "abyss_lord" => pal(
            0x050309, 0x1a0e2e, 0x3a1f63, 0x6c40b2, 0xcaa9ff, 0xff3ff0, 0xffc6fb,
        ),
        "enhance" => pal(
            0x16202c, 0x3d4f66, 0x6b829e, 0xa4b8cf, 0xe4eef8, 0xffc93a, 0xfff2a6,
        ),
        "scroll_low" => pal(
            0x3a2a14, 0xb69a66, 0xe9d6a8, 0xfaf0d4, 0xffffff, 0x4fbf5a, 0xb9f2bd,
        ),
        "scroll_mid" => pal(
            0x3a2a14, 0xb69a66, 0xe9d6a8, 0xfaf0d4, 0xffffff, 0x3f7bff, 0xb5ccff,
        ),
        "scroll_high" => pal(
            0x3a2a14, 0xb69a66, 0xe9d6a8, 0xfaf0d4, 0xffffff, 0xb34dff, 0xffd24a,
        ),
        _ => pal(
            0x222222, 0x555555, 0x8a8a8a, 0xbdbdbd, 0xf0f0f0, 0x3fb7ff, 0xc6ecff,
        ),
    }
}

const GRIP: Rgba = rgb(0x7a4a24);
const GRIP_DARK: Rgba = rgb(0x4a2a12);
const WHITE: Rgba = rgb(0xffffff);

/// RGBA 그림.
#[derive(Clone, Debug, PartialEq)]
pub struct Img {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgba>,
}

impl Img {
    pub fn new(w: usize, h: usize) -> Self {
        Img {
            w,
            h,
            px: vec![[0; 4]; w * h],
        }
    }
    pub fn get(&self, x: i32, y: i32) -> Rgba {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return [0; 4];
        }
        self.px[y as usize * self.w + x as usize]
    }
    pub fn set(&mut self, x: i32, y: i32, c: Rgba) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }
    pub fn rgba(&self) -> Vec<u8> {
        self.px.iter().flatten().copied().collect()
    }
    pub fn png(&self) -> Vec<u8> {
        png(self.w as u32, self.h as u32, &self.rgba())
    }
    /// 다른 그림을 (x, y)에 (불투명한 것만).
    pub fn draw(&mut self, src: &Img, x: i32, y: i32, scale: i32) {
        for sy in 0..src.h as i32 {
            for sx in 0..src.w as i32 {
                let c = src.get(sx, sy);
                if c[3] == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        self.set(x + sx * scale + dx, y + sy * scale + dy, c);
                    }
                }
            }
        }
    }
}

/// 마스크(16×16 글자)에 색을 입힘.
pub fn paint(rows: &[&str], p: &Palette) -> Img {
    let h = rows.len();
    let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    let grid: Vec<Vec<char>> = rows.iter().map(|r| r.chars().collect()).collect();
    let at = |x: i32, y: i32| -> char {
        if x < 0 || y < 0 {
            return '.';
        }
        grid.get(y as usize)
            .and_then(|r| r.get(x as usize))
            .copied()
            .unwrap_or('.')
    };
    // 몸 범위 (명암 기울기)
    let body: Vec<(i32, i32)> = (0..h as i32)
        .flat_map(|y| (0..w as i32).map(move |x| (x, y)))
        .filter(|&(x, y)| at(x, y) == '#')
        .collect();
    let (minx, maxx) = body
        .iter()
        .fold((i32::MAX, i32::MIN), |(a, b), &(x, _)| (a.min(x), b.max(x)));
    let (miny, maxy) = body
        .iter()
        .fold((i32::MAX, i32::MIN), |(a, b), &(_, y)| (a.min(y), b.max(y)));
    let shades = [p.o, p.d, p.m, p.l, p.h];
    let mut img = Img::new(w, h);
    let filled = |c: char| c != '.';
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let c = at(x, y);
            let color = match c {
                '#' => {
                    let tx = (x - minx) as f32 / (maxx - minx).max(1) as f32;
                    let ty = (y - miny) as f32 / (maxy - miny).max(1) as f32;
                    let t = (tx + ty) / 2.0;
                    let mut i: i32 = if t < 0.3 {
                        3
                    } else if t < 0.68 {
                        2
                    } else {
                        1
                    };
                    if !filled(at(x, y - 1)) || !filled(at(x - 1, y)) {
                        i += 1; // 위·왼쪽 가장자리는 빛
                    }
                    if !filled(at(x, y + 1)) || !filled(at(x + 1, y)) {
                        i -= 1; // 아래·오른쪽 가장자리는 그늘
                    }
                    shades[i.clamp(1, 4) as usize]
                }
                '1'..='5' => shades[c as usize - '1' as usize],
                'a' => p.a,
                'A' => p.aa,
                'g' => GRIP,
                'G' => GRIP_DARK,
                '-' => p.o,
                'w' => WHITE,
                's' => rgb(0x7d7d7d),
                'S' => rgb(0x555555),
                'x' => rgb(0xa8a8a8),
                _ => continue,
            };
            img.set(x, y, color);
        }
    }
    // 테두리: 빈칸 가운데 몸에 닿은 곳 (반짝임 제외)
    let src = img.clone();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if src.get(x, y)[3] != 0 {
                continue;
            }
            let touches = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
                let c = at(x + dx, y + dy);
                filled(c) && c != 'w'
            });
            if touches {
                img.set(x, y, p.o);
            }
        }
    }
    img
}

// ---- 모양 틀 ----

const HELMET_A: [&str; 16] = [
    "................",
    "................",
    ".......aa.......",
    "......aAAa......",
    ".....######.....",
    "....########....",
    "...##########...",
    "...####Aa####...",
    "...##########...",
    "...#--------#...",
    "...##########...",
    "...###-##-###...",
    "...##########...",
    "....##....##....",
    "................",
    "................",
];

const HELMET_B: [&str; 16] = [
    "................",
    "....a..aa..a....",
    "....aa.AA.aa....",
    ".a...######...a.",
    ".aa.########.aa.",
    "..a##########a..",
    "...####Aa####...",
    "...##########...",
    "...#--------#...",
    "...##########...",
    "...###-##-###...",
    "...##########...",
    "....##....##....",
    "................",
    "................",
    "................",
];

const CHEST: [&str; 16] = [
    "................",
    "................",
    "..###......###..",
    ".#####....#####.",
    ".######--######.",
    ".##############.",
    "..#####Aa#####..",
    "...####aa####...",
    "...##########...",
    "...#-##--##-#...",
    "...##########...",
    "...#--------#...",
    "...####Aa####...",
    "................",
    "................",
    "................",
];

const CHEST_B: [&str; 16] = [
    "................",
    ".a............a.",
    ".aa##......##aa.",
    ".#####....#####.",
    ".######--######.",
    ".##############.",
    "..#####Aa#####..",
    "...###aAAa###...",
    "...####aa####...",
    "...#-##--##-#...",
    "...##########...",
    "...#--------#...",
    "...####Aa####...",
    "................",
    "................",
    "................",
];

const LEGS: [&str; 16] = [
    "................",
    "................",
    "...##########...",
    "...#-##Aa##-#...",
    "...##########...",
    "...####..####...",
    "...####..####...",
    "...#a##..##a#...",
    "...####..####...",
    "...####..####...",
    "...####..####...",
    "...####..####...",
    "...#--#..#--#...",
    "................",
    "................",
    "................",
];

const BOOTS: [&str; 16] = [
    "................",
    "................",
    "...#######......",
    "...#aAAaa#......",
    "...#######......",
    "...#######......",
    "...###-###......",
    "...#######......",
    "...########.....",
    "...##########...",
    "...###########..",
    "...#####-#####..",
    "...-----------..",
    "................",
    "................",
    "................",
];

const RING: [&str; 16] = [
    "................",
    "................",
    "................",
    "......aAa.......",
    ".....aAAAa......",
    "......aaa.......",
    ".....##.##......",
    "....##...##.....",
    "...##.....##....",
    "...##.....##....",
    "...##.....##....",
    "....##...##.....",
    ".....#####......",
    "................",
    "................",
    "................",
];

const RING_BOSS: [&str; 16] = [
    "................",
    "................",
    ".....w..........",
    ".....aAAa.......",
    "....aAAAAa......",
    "....aAaaaa......",
    ".....aaaa.......",
    "....##..##......",
    "...##....##.....",
    "...#......#.....",
    "...##....##.....",
    "....##..##......",
    ".....####.......",
    "................",
    "................",
    "................",
];

const NECKLACE: [&str; 16] = [
    "................",
    "..#..........#..",
    "..#..........#..",
    "...#........#...",
    "...#........#...",
    "....#......#....",
    ".....#....#.....",
    "......####......",
    ".......##.......",
    "......aAAa......",
    ".....aAAaaa.....",
    ".....aAaaaa.....",
    "......aaaa......",
    ".......aa.......",
    "................",
    "................",
];

const NECKLACE_BOSS: [&str; 16] = [
    "................",
    "..#..........#..",
    "..#..........#..",
    "...#........#...",
    "...#........#...",
    "....#......#....",
    ".....##..##.....",
    "....w.####.w....",
    "......#aA#......",
    ".....#aAAa#.....",
    "....#aAAaaa#....",
    "....#aAaaaa#....",
    ".....#aaaa#.....",
    "......#aa#......",
    ".......##.......",
    "................",
];

const EARRING: [&str; 16] = [
    "................",
    "......###.......",
    ".....#...#......",
    ".....#...#......",
    "......#.#.......",
    ".......#........",
    ".......#........",
    "......aAa.......",
    ".....aAAaa......",
    ".....aAaaa......",
    "......aaa.......",
    ".......a........",
    "................",
    "................",
    "................",
    "................",
];

const EARRING_BOSS: [&str; 16] = [
    "................",
    "......###.......",
    ".....#...#......",
    ".....#...#.w....",
    "......#.#.......",
    ".......#........",
    "......#a#.......",
    ".....#aAa#......",
    "....#aAAAa#.....",
    "....#aAaaa#.....",
    ".....#aaa#......",
    "......#a#.......",
    ".......#........",
    "................",
    "................",
    "................",
];

/// 검: 날(오른쪽 위로) · 가드 · 손잡이 · 장식. `style` 0 기본, 1 넓은 날, 2 장식(빛나는 날 끝).
fn sword(style: u8, p: &Palette) -> Img {
    let mut rows = vec![vec!['.'; 16]; 16];
    let mut put = |x: i32, y: i32, c: char| {
        if (0..16).contains(&x) && (0..16).contains(&y) {
            rows[y as usize][x as usize] = c;
        }
    };
    // 날: x + y = 15 줄, (6,9)부터 (13,2)까지
    for i in 0..8 {
        let (x, y) = (6 + i, 9 - i);
        put(x, y, '5');
        put(x + 1, y, '3');
        put(x + 1, y + 1, '2');
        if style >= 1 && i > 0 {
            put(x, y - 1, '4');
        }
    }
    put(14, 1, '5');
    if style >= 2 {
        // 날 끝의 빛과 홈
        for i in 1..6 {
            put(6 + i, 10 - i, 'A');
        }
        put(14, 0, 'w');
    }
    // 가드: (5,10)을 가운데로 (1,1) 방향
    let guard = if style >= 1 { 3 } else { 2 };
    for k in -guard..=guard {
        put(5 + k, 10 + k, if k == 0 { 'A' } else { 'a' });
    }
    // 손잡이와 끝 장식
    for (x, y) in [(4, 11), (3, 12), (2, 13)] {
        put(x, y, 'g');
    }
    put(3, 11, 'G');
    put(1, 14, 'A');
    put(1, 13, 'a');
    put(2, 14, 'a');
    let strs: Vec<String> = rows.iter().map(|r| r.iter().collect()).collect();
    let refs: Vec<&str> = strs.iter().map(String::as_str).collect();
    paint(&refs, p)
}

/// 지팡이: 대 · 머리(발톱) · 보주. `style` 0 나무 대, 1 쇠 대 + 큰 보주, 2 날개 + 빛.
fn staff(style: u8, p: &Palette) -> Img {
    let mut rows = vec![vec!['.'; 16]; 16];
    let mut put = |x: i32, y: i32, c: char| {
        if (0..16).contains(&x) && (0..16).contains(&y) {
            rows[y as usize][x as usize] = c;
        }
    };
    // 대: (2,14) → (9,7)
    for i in 0..8 {
        let (x, y) = (2 + i, 14 - i);
        let c = if style == 0 { 'g' } else { '3' };
        put(x, y, c);
        put(x + 1, y, if style == 0 { 'G' } else { '2' });
    }
    // 머리: 발톱 고리
    for (x, y) in [
        (9, 5),
        (10, 4),
        (12, 4),
        (13, 5),
        (13, 7),
        (12, 8),
        (10, 8),
        (9, 7),
    ] {
        put(x, y, '4');
    }
    put(9, 6, '3');
    put(13, 6, '2');
    put(11, 3, '5');
    put(11, 9, '2');
    // 보주
    let orb: &[(i32, i32)] = if style >= 1 {
        &[
            (10, 5),
            (11, 5),
            (12, 5),
            (10, 6),
            (11, 6),
            (12, 6),
            (10, 7),
            (11, 7),
            (12, 7),
        ]
    } else {
        &[(11, 5), (10, 6), (11, 6), (12, 6), (11, 7)]
    };
    for &(x, y) in orb {
        put(x, y, 'a');
    }
    put(11, 6, 'A');
    put(10, 5, 'A');
    if style >= 2 {
        // 날개와 반짝임
        for (x, y) in [(7, 4), (8, 3), (7, 3), (14, 8), (15, 9), (15, 8)] {
            put(x, y, '4');
        }
        for (x, y) in [(14, 1), (7, 8), (15, 3)] {
            put(x, y, 'w');
        }
    }
    let strs: Vec<String> = rows.iter().map(|r| r.iter().collect()).collect();
    let refs: Vec<&str> = strs.iter().map(String::as_str).collect();
    paint(&refs, p)
}

/// 장비 하나의 아이콘.
pub fn gear_icon(g: &GearDef) -> Img {
    let p = palette(&g.palette);
    let style = match g.tier {
        0..=2 => 0,
        3..=4 => 1,
        _ => 2,
    };
    let boss = g.set.is_none() && g.potential_tries >= 5;
    match g.part {
        Part::Helmet => paint(if style == 2 { &HELMET_B } else { &HELMET_A }, &p),
        Part::Chest => paint(if style == 2 { &CHEST_B } else { &CHEST }, &p),
        Part::Legs => paint(&LEGS, &p),
        Part::Boots => paint(&BOOTS, &p),
        Part::Sword => sword(style, &p),
        Part::Staff => staff(style, &p),
        Part::Ring => paint(if boss { &RING_BOSS } else { &RING }, &p),
        Part::Necklace => paint(if boss { &NECKLACE_BOSS } else { &NECKLACE }, &p),
        Part::Earring => paint(if boss { &EARRING_BOSS } else { &EARRING }, &p),
    }
}

// ---- 재료 ----

const RAW: [&str; 16] = [
    "................",
    "................",
    "................",
    "......sss.......",
    "....ssaAss......",
    "...sxAAaasS.....",
    "...saa#aSsS.....",
    "..sxa###assS....",
    "..ssa##aaasS....",
    "..Ssaaasa#sS....",
    "...SsssaassS....",
    "....SSssSS......",
    "................",
    "................",
    "................",
    "................",
];

const INGOT: [&str; 16] = [
    "................",
    "................",
    "................",
    "................",
    "................",
    ".......######...",
    ".....########...",
    "...##########...",
    "..###########...",
    "..##########....",
    "..#########.....",
    "..########......",
    "................",
    "................",
    "................",
    "................",
];

const GEM: [&str; 16] = [
    "................",
    "................",
    ".......w........",
    "......##........",
    ".....####.......",
    "....##aa##......",
    "...##aAAa##.....",
    "...#aAAaaa#.....",
    "...##aaaa##.....",
    "....##aa##......",
    ".....####.......",
    "......##........",
    "................",
    "................",
    "................",
    "................",
];

const MAGIC_STONE: [&str; 16] = [
    "................",
    "...w........w...",
    "......####......",
    ".....#aAa##.....",
    "....#aAAAaa#....",
    "...#aAAaaaaa#...",
    "...#aAaaaaa##...",
    "...#aaaaaa#a#...",
    "...##aaaa#aa#...",
    "....##aaaaa#....",
    ".....##aaa#.....",
    "......####......",
    "..w.........w...",
    "................",
    "................",
    "................",
];

const ENHANCE_STONE: [&str; 16] = [
    "................",
    "................",
    "................",
    ".....######.....",
    "....########....",
    "...###aAa###....",
    "...##a#A#a##....",
    "...##aAAAa##....",
    "...##a#A#a##....",
    "...###aaa###....",
    "....########....",
    ".....######.....",
    "................",
    "........w.......",
    "................",
    "................",
];

const SCROLL: [&str; 16] = [
    "................",
    "................",
    "...4433333322...",
    "..54######3322..",
    "...4#-----#2....",
    "...4#######2....",
    "...4#-----#2....",
    "...4###aa##2....",
    "...4#--aA-#2....",
    "...4###aa##2....",
    "...4#-----#2....",
    "..54######3322..",
    "...4433333322...",
    "................",
    "................",
    "................",
];

pub fn material_icon(m: &Material) -> Img {
    let p = palette(&m.palette);
    match m.art.as_str() {
        "raw" => paint(&RAW, &p),
        "ingot" => paint(&INGOT, &p),
        "gem" => paint(&GEM, &p),
        "magic_stone" => paint(&MAGIC_STONE, &p),
        "enhance_stone" => paint(&ENHANCE_STONE, &p),
        "scroll" => {
            let mut img = paint(&SCROLL, &p);
            // 주문서 등급 리본
            let ribbon = p.a;
            for (x, y) in [(7, 12), (8, 12), (7, 13), (9, 13), (6, 14), (9, 14)] {
                img.set(x, y, ribbon);
            }
            img
        }
        _ => paint(&GEM, &p),
    }
}

// ---- 광석 블록 ----

const ORE_VEINS: [&str; 16] = [
    "................",
    "..##.......##...",
    ".#A##.....#A#...",
    ".###a......#....",
    "..#....##.......",
    ".......#A#......",
    "......###a...##.",
    ".......##...#A#.",
    "..##.........#..",
    ".#A##...........",
    "..##.....##.....",
    ".........#A#....",
    "...##.....##....",
    "..#A#...........",
    "...#........##..",
    "............#...",
];

/// 결정론적 잡음 (같은 이름이면 같은 그림).
fn noise(seed: u32, x: i32, y: i32) -> u32 {
    let mut h = seed ^ (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h
}

pub fn ore_block(o: &OreDef) -> Img {
    let p = palette(&o.palette);
    let seed = o
        .block
        .bytes()
        .fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let base: [Rgba; 3] = if o.nether {
        [rgb(0x5e1f1f), rgb(0x7a2a2a), rgb(0x8f3b36)]
    } else {
        [rgb(0x6e6e6e), rgb(0x7f7f7f), rgb(0x939393)]
    };
    let mut img = Img::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            img.set(x, y, base[(noise(seed, x, y) % 3) as usize]);
        }
    }
    // 결정 광맥 (바닐라 광석처럼 테두리 있는 덩어리); 이름마다 뒤집어 다르게
    let flip = seed % 4;
    let rows: Vec<String> = ORE_VEINS
        .iter()
        .map(|r| {
            let r: String = if flip & 1 == 1 {
                r.chars().rev().collect()
            } else {
                (*r).to_owned()
            };
            r
        })
        .collect();
    let mut rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    if flip & 2 == 2 {
        rows.reverse();
    }
    let veins = paint(&rows, &p);
    img.draw(&veins, 0, 0, 1);
    img
}

// ---- 입었을 때 겉모습 (64×32) ----

/// 상자 한 면을 금속판으로: 가장자리 어둡게, 위·왼쪽 빛, 가운데 무늬.
fn plate(img: &mut Img, x0: i32, y0: i32, w: i32, h: i32, p: &Palette, front: Option<&str>) {
    for y in 0..h {
        for x in 0..w {
            let edge = x == 0 || y == 0 || x == w - 1 || y == h - 1;
            let c = if edge {
                if x == 0 || y == 0 { p.l } else { p.d }
            } else if (x + y) % 7 == 0 {
                p.l
            } else {
                p.m
            };
            img.set(x0 + x, y0 + y, c);
        }
    }
    match front {
        Some("visor") => {
            for x in 1..w - 1 {
                img.set(x0 + x, y0 + h / 2, p.o);
            }
            img.set(x0 + w / 2, y0 + 1, p.a);
            img.set(x0 + w / 2 - 1, y0 + 1, p.aa);
        }
        Some("chest") => {
            img.set(x0 + w / 2, y0 + 3, p.aa);
            img.set(x0 + w / 2 - 1, y0 + 3, p.a);
            img.set(x0 + w / 2, y0 + 4, p.a);
            img.set(x0 + w / 2 - 1, y0 + 4, p.a);
            for x in 1..w - 1 {
                img.set(x0 + x, y0 + h - 3, p.o);
            }
        }
        Some("belt") => {
            for x in 0..w {
                img.set(x0 + x, y0 + 1, p.o);
            }
            img.set(x0 + w / 2, y0 + 1, p.a);
        }
        Some("knee") => {
            img.set(x0 + w / 2, y0 + 5, p.a);
            img.set(x0 + w / 2 - 1, y0 + 5, p.aa);
        }
        _ => {}
    }
}

/// 상자(가로 w, 세로 h, 깊이 d)의 여섯 면을 바닐라 배치대로. `rows`는 그릴 세로 범위 (부츠·바지).
#[allow(clippy::too_many_arguments)]
fn cuboid(
    img: &mut Img,
    u: i32,
    v: i32,
    w: i32,
    h: i32,
    d: i32,
    p: &Palette,
    front: Option<&str>,
    rows: std::ops::Range<i32>,
) {
    let faces = [
        (u + d, v, w, d, None),             // 위
        (u + d + w, v, w, d, None),         // 아래
        (u, v + d, d, h, None),             // 오른쪽
        (u + d, v + d, w, h, front),        // 앞
        (u + d + w, v + d, d, h, None),     // 왼쪽
        (u + d + w + d, v + d, w, h, None), // 뒤
    ];
    for (i, (x, y, fw, fh, f)) in faces.into_iter().enumerate() {
        if i < 2 {
            if rows.start == 0 {
                plate(img, x, y, fw, fh, p, None);
            }
            continue;
        }
        let top = rows.start.min(fh);
        let bottom = rows.end.min(fh);
        if bottom > top {
            plate(img, x, y + top, fw, bottom - top, p, f);
        }
    }
}

/// (humanoid 층: 투구·흉갑·장화, humanoid_leggings 층: 각반)
pub fn armor_layers(palette_name: &str) -> (Img, Img) {
    let p = palette(palette_name);
    let mut a = Img::new(64, 32);
    cuboid(&mut a, 0, 0, 8, 8, 8, &p, Some("visor"), 0..8); // 머리
    cuboid(&mut a, 16, 16, 8, 12, 4, &p, Some("chest"), 0..12); // 몸
    cuboid(&mut a, 40, 16, 4, 12, 4, &p, None, 0..12); // 팔
    cuboid(&mut a, 0, 16, 4, 12, 4, &p, None, 7..12); // 다리 아래쪽 = 장화
    let mut b = Img::new(64, 32);
    cuboid(&mut b, 16, 16, 8, 12, 4, &p, Some("belt"), 8..12); // 허리
    cuboid(&mut b, 0, 16, 4, 12, 4, &p, Some("knee"), 0..9); // 다리
    (a, b)
}

// ---- 콘텐츠 팩 ----

fn yaml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// 아이템 id의 짧은 이름 (`hoya:mithril_helmet` → `mithril_helmet`).
fn short(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, n)| n)
}

/// 장비·재료·광석 아이템과 그림 (`hoya` 콘텐츠 팩).
pub fn pack(cfg: &EquipConfig) -> Pack {
    let mut yml = String::from(
        "# 서버에 들어 있는 장비·재료 (설정: world/equipment.json)\ninfo:\n  namespace: hoya\nitems:\n",
    );
    let mut files = Vec::new();
    let mut equipments: Vec<String> = Vec::new();
    for g in &cfg.gear {
        let name = short(&g.id);
        let (grade_color, _) = (super::config::grade_color(&g.grade), ());
        let (material, slot) = match g.part {
            Part::Helmet => ("leather_helmet", Some("head")),
            Part::Chest => ("leather_chestplate", Some("chest")),
            Part::Legs => ("leather_leggings", Some("legs")),
            Part::Boots => ("leather_boots", Some("feet")),
            Part::Sword => ("iron_sword", None),
            Part::Staff => ("stick", None),
            _ => ("paper", None),
        };
        yml.push_str(&format!(
            "  {name}:\n    name: {}\n",
            yaml_str(&format!("<{grade_color}>{}", g.name))
        ));
        yml.push_str("    lore:\n");
        for line in super::look::base_lore(g, cfg) {
            yml.push_str(&format!("      - {}\n", yaml_str(&format!("<gray>{line}"))));
        }
        yml.push_str(&format!(
            "    material: {material}\n    max_stack_size: 1\n"
        ));
        yml.push_str("    item_flags: [HIDE_ATTRIBUTES, HIDE_DYE, HIDE_UNBREAKABLE]\n");
        yml.push_str("    durability: {unbreakable: true}\n");
        let parent = if g.part.is_weapon() {
            ", parent: item/handheld"
        } else {
            ""
        };
        yml.push_str(&format!(
            "    graphics: {{texture: item/equip/{name}{parent}}}\n"
        ));
        if let Some(slot) = slot {
            let eq = g.set.clone().unwrap_or_else(|| g.palette.clone());
            yml.push_str(&format!("    equipment: {{id: hoya:{eq}, slot: {slot}}}\n"));
            if !equipments.contains(&eq) {
                equipments.push(eq);
            }
        }
        files.push((
            format!("assets/hoya/textures/item/equip/{name}.png"),
            gear_icon(g).png(),
        ));
    }
    for m in &cfg.materials {
        let name = short(&m.id);
        yml.push_str(&format!(
            "  {name}:\n    name: {}\n",
            yaml_str(&format!("<{}>{}", m.color, m.name))
        ));
        yml.push_str("    lore:\n");
        yml.push_str(&format!(
            "      - {}\n",
            yaml_str(&format!(
                "<{}>{} 재료",
                super::config::grade_color(&m.grade),
                m.grade
            ))
        ));
        for line in &m.lore {
            yml.push_str(&format!("      - {}\n", yaml_str(&format!("<gray>{line}"))));
        }
        let stack = if m.art == "scroll" { 16 } else { 64 };
        yml.push_str(&format!(
            "    material: paper\n    max_stack_size: {stack}\n"
        ));
        if m.art == "magic_stone" || m.art == "scroll" && m.palette == "scroll_high" {
            yml.push_str("    enchant_glint: true\n");
        }
        yml.push_str(&format!("    graphics: {{texture: item/equip/{name}}}\n"));
        files.push((
            format!("assets/hoya/textures/item/equip/{name}.png"),
            material_icon(m).png(),
        ));
    }
    for o in &cfg.ores {
        let name = short(&o.block);
        let color = palette_hex(&o.palette);
        yml.push_str(&format!(
            "  {name}:\n    name: {}\n",
            yaml_str(&format!("<{color}>{}", o.name))
        ));
        yml.push_str(&format!(
            "    lore: [{}]\n",
            yaml_str(&format!(
                "<gray>{} 이상 곡괭이로 캐면 원석이 나옵니다",
                pickaxe_name(&o.pickaxe)
            ))
        ));
        yml.push_str(&format!(
            "    resource: {{material: PAPER, generate: true, textures: [block/{name}.png]}}\n"
        ));
        yml.push_str("    specific_properties:\n      block:\n        placed_model: {type: REAL_NOTE}\n        drop_when_mined: false\n");
        files.push((
            format!("assets/hoya/textures/block/{name}.png"),
            ore_block(o).png(),
        ));
    }
    if !equipments.is_empty() {
        yml.push_str("equipments:\n");
        for eq in &equipments {
            yml.push_str(&format!(
                "  {eq}: {{layer_1: entity/{eq}_layer_1, layer_2: entity/{eq}_layer_2}}\n"
            ));
            let (l1, l2) = armor_layers(eq);
            files.push((
                format!("assets/hoya/textures/entity/{eq}_layer_1.png"),
                l1.png(),
            ));
            files.push((
                format!("assets/hoya/textures/entity/{eq}_layer_2.png"),
                l2.png(),
            ));
        }
    }
    Pack {
        name: "hoya".into(),
        configs: vec![("(서버 내장) hoya/configs/equipment.yml".into(), yml)],
        files,
    }
}

pub fn pickaxe_name(p: &str) -> &'static str {
    match p {
        "wooden" => "나무",
        "stone" => "돌",
        "iron" => "철",
        "golden" => "금",
        "diamond" => "다이아몬드",
        "netherite" => "네더라이트",
        _ => "아무",
    }
}

/// 팔레트의 밝은 색을 `#rrggbb`로 (이름 색).
pub fn palette_hex(name: &str) -> String {
    let c = palette(name).l;
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// 모든 그림을 한 장에 (미리보기): 세트마다 한 줄, 그다음 악세·재료·광석, 갑옷 겉모습 앞모습.
pub fn showcase(cfg: &EquipConfig, scale: i32) -> Img {
    let cell = 18 * scale;
    let mut rows: Vec<Vec<Img>> = Vec::new();
    let mut sets: Vec<String> = Vec::new();
    for g in &cfg.gear {
        let key = g.set.clone().unwrap_or_else(|| format!("~{}", g.palette));
        match sets.iter().position(|s| *s == key) {
            Some(i) => rows[i].push(gear_icon(g)),
            None => {
                sets.push(key);
                rows.push(vec![gear_icon(g)]);
            }
        }
    }
    // 세트 줄 끝에 입은 모습 (앞)
    for (i, key) in sets.iter().enumerate() {
        if !key.starts_with('~') {
            rows[i].push(figure(key));
        }
    }
    rows.push(cfg.materials.iter().map(material_icon).collect());
    rows.push(cfg.ores.iter().map(ore_block).collect());
    let cols = rows.iter().map(Vec::len).max().unwrap_or(1);
    let mut img = Img::new(
        cols * cell as usize + scale as usize * 4,
        rows.len() * (cell as usize + 2 * scale as usize) + scale as usize * 4,
    );
    for p in img.px.iter_mut() {
        *p = rgb(0x1b1d26);
    }
    for (ri, row) in rows.iter().enumerate() {
        let y = 2 * scale + ri as i32 * (cell + 2 * scale);
        // 줄 바탕
        for yy in y..y + cell {
            for xx in scale..img.w as i32 - scale {
                if (ri % 2) == 0 {
                    img.set(xx, yy, rgb(0x232633));
                }
            }
        }
        for (ci, icon) in row.iter().enumerate() {
            let x = 2 * scale + ci as i32 * cell;
            let sc = if icon.h > 16 { scale / 2 } else { scale };
            let ox = (cell - icon.w as i32 * sc) / 2;
            let oy = (cell - icon.h as i32 * sc) / 2;
            img.draw(icon, x + ox.max(0), y + oy.max(0), sc.max(1));
        }
    }
    img
}

/// 갑옷 겉모습의 앞모습 (16×32): 머리·몸·팔·다리 앞면을 이어 붙임.
pub fn figure(eq: &str) -> Img {
    let (a, b) = armor_layers(eq);
    let mut f = Img::new(16, 32);
    let copy = |f: &mut Img, src: &Img, sx: i32, sy: i32, w: i32, h: i32, dx: i32, dy: i32| {
        for y in 0..h {
            for x in 0..w {
                let c = src.get(sx + x, sy + y);
                if c[3] != 0 {
                    f.set(dx + x, dy + y, c);
                }
            }
        }
    };
    copy(&mut f, &a, 8, 8, 8, 8, 4, 0); // 머리 앞
    copy(&mut f, &a, 20, 20, 8, 12, 4, 8); // 몸 앞
    copy(&mut f, &a, 44, 20, 4, 12, 0, 8); // 팔
    copy(&mut f, &a, 44, 20, 4, 12, 12, 8);
    copy(&mut f, &b, 20, 20, 8, 12, 4, 8); // 허리
    copy(&mut f, &b, 4, 20, 4, 12, 4, 20); // 다리
    copy(&mut f, &b, 4, 20, 4, 12, 8, 20);
    copy(&mut f, &a, 4, 20, 4, 12, 4, 20); // 장화
    copy(&mut f, &a, 4, 20, 4, 12, 8, 20);
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_are_square_and_every_item_has_art() {
        for m in [
            &HELMET_A,
            &HELMET_B,
            &CHEST,
            &CHEST_B,
            &LEGS,
            &BOOTS,
            &RING,
            &RING_BOSS,
            &NECKLACE,
            &NECKLACE_BOSS,
            &EARRING,
            &EARRING_BOSS,
            &RAW,
            &INGOT,
            &GEM,
            &MAGIC_STONE,
            &ENHANCE_STONE,
            &SCROLL,
        ] {
            for (y, r) in m.iter().enumerate() {
                assert_eq!(r.chars().count(), 16, "row {y}: {r}");
            }
        }
        let cfg = EquipConfig::default();
        for g in &cfg.gear {
            let i = gear_icon(g);
            assert!(i.px.iter().filter(|p| p[3] != 0).count() > 20, "{}", g.id);
        }
        let (a, b) = armor_layers("mithril");
        assert_eq!((a.w, a.h, b.w, b.h), (64, 32, 64, 32));
        let p = pack(&cfg);
        let yml = &p.configs[0].1;
        assert!(
            yml.contains("mithril_helmet:")
                && yml.contains("equipment: {id: hoya:mithril, slot: head}")
        );
        assert!(yml.contains("drop_when_mined: false"));
        assert!(
            p.files
                .iter()
                .any(|(n, _)| n.ends_with("entity/arcana_layer_2.png"))
        );
    }

    #[test]
    #[ignore]
    fn equipment_showcase() {
        let Ok(path) = std::env::var("MC_EQUIP_PREVIEW") else {
            return;
        };
        let img = showcase(&EquipConfig::default(), 6);
        std::fs::write(path, img.png()).unwrap();
    }
}
