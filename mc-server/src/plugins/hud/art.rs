//! HUD 그림: 직접 찍은 글자·아이콘·판·체력바, 줄마다의 폰트, 그리고 글자를 화면 오른쪽 끝으로
//! 옮겨 주는 텍스트 셰이더.
//!
//! 마인크래프트는 보스바 제목과 액션바를 화면 가운데에 그립니다. 그래서
//! - 줄(세로 위치)은 폰트마다 `ascent`를 달리 해서 정하고 (`dangu:hud/top0`, `bot3`, …),
//! - 가로 위치는 공백 글자(`space` 공급자)로 정하고,
//! - 오른쪽에 붙일 글자는 표시 색 [`RIGHT`]로 칠해 두면 셰이더가 화면 폭의 절반만큼 오른쪽으로
//!   옮깁니다 (화면 크기를 서버가 몰라도 오른쪽 끝에 붙음). [`STAY`]는 옮기지 않고 색만 지웁니다.
//! - 두 표시 색의 그림자는 셰이더가 지웁니다.
//!
//! 바닐라 하트·갑옷 아이콘은 투명 그림으로 덮어 숨기고 (대신 체력·마나바, 방어력은 스탯 판),
//! 분홍 보스바의 막대도 투명하게 합니다
//! (HUD의 위쪽 줄은 분홍 보스바 제목으로 띄움).

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::plugins::contents::source::Pack;
use crate::resource_pack::png;

/// 오른쪽 끝에 붙이는 글자 색 (셰이더가 알아봄).
pub const RIGHT: &str = "#FD02F9";
/// 가운데 기준 그대로 두는 글자 색 (그림자만 지움).
pub const STAY: &str = "#FD03F9";

/// 한 줄의 폰트: 이름과 가운데(보스바 제목·액션바 글) 기준 아래로 내린 픽셀.
pub struct RowFont {
    pub id: &'static str,
    pub down: i32,
}

/// 오른쪽 위 (보스바 제목, 화면 위에서 3px): 재화 세 줄.
pub const TOP: [RowFont; 3] = [
    RowFont {
        id: "dangu:hud/top0",
        down: 0,
    },
    RowFont {
        id: "dangu:hud/top1",
        down: 11,
    },
    RowFont {
        id: "dangu:hud/top2",
        down: 22,
    },
];
/// 오른쪽 아래 (액션바 글, 화면 아래에서 72px): 스탯 여섯 줄, 마지막 줄이 화면 바닥 2px 위.
pub const BOTTOM: [RowFont; 6] = [
    RowFont {
        id: "dangu:hud/bot0",
        down: 6,
    },
    RowFont {
        id: "dangu:hud/bot1",
        down: 17,
    },
    RowFont {
        id: "dangu:hud/bot2",
        down: 28,
    },
    RowFont {
        id: "dangu:hud/bot3",
        down: 39,
    },
    RowFont {
        id: "dangu:hud/bot4",
        down: 50,
    },
    RowFont {
        id: "dangu:hud/bot5",
        down: 61,
    },
];
/// 바닐라 하트 자리 (화면 아래에서 39px, 핫바 왼쪽 위).
pub const HP: RowFont = RowFont {
    id: "dangu:hud/hp",
    down: 33,
};
/// 그 위 바닐라 갑옷 아이콘 자리 (방어력은 스탯 판에 있으므로 아이콘은 숨김).
pub const MP: RowFont = RowFont {
    id: "dangu:hud/mp",
    down: 23,
};

// ---- 글자 번호 ----

pub const COIN: char = '\u{E100}';
pub const GEM: char = '\u{E101}';
pub const MOON: char = '\u{E102}';
pub const STAR: char = '\u{E103}';
pub const SWORD: char = '\u{E104}';
pub const SHIELD: char = '\u{E105}';
pub const HEART: char = '\u{E106}';
pub const BOOT: char = '\u{E107}';
pub const FLAME: char = '\u{E108}';

pub const PANEL_LEFT: char = '\u{E200}';
pub const PANEL_RIGHT: char = '\u{E201}';
/// 판 가운데 조각: 1, 2, 4, … 64px.
pub const PANEL_MID: [char; 7] = [
    '\u{E202}', '\u{E203}', '\u{E204}', '\u{E205}', '\u{E206}', '\u{E207}', '\u{E208}',
];

/// 막대 안쪽 칸 수 (테두리 빼고).
pub const BAR_INNER: i32 = 79;

/// 막대 두 가지: 체력(빨강, `E300`: 틀, `E301..=E34F`: 1..=79칸 채움), 마나(파랑, `E350`부터).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    Health,
    Mana,
}

impl Bar {
    fn base(self) -> u32 {
        match self {
            Bar::Health => 0xE300,
            Bar::Mana => 0xE350,
        }
    }

    pub fn frame(self) -> char {
        char::from_u32(self.base()).unwrap()
    }

    pub fn fill(self, cells: i32) -> Option<char> {
        (1..=BAR_INNER)
            .contains(&cells)
            .then(|| char::from_u32(self.base() + cells as u32).unwrap())
    }

    /// (테두리, 바탕, 위·가운데·아래 채움색)
    fn colors(self) -> (u32, u32, [u32; 3]) {
        match self {
            Bar::Health => (0x1a0808ff, 0x3a1414c8, [0xff6a6aff, 0xe02a2aff, 0xa01818ff]),
            Bar::Mana => (0x080d1aff, 0x14203ac8, [0x7ac4ffff, 0x2a7ae0ff, 0x1850a0ff]),
        }
    }

    fn file(self) -> &'static str {
        match self {
            Bar::Health => "dangu:font/hud/bar_hp.png",
            Bar::Mana => "dangu:font/hud/bar_mp.png",
        }
    }
}

/// 공백: `E400+k` = +2^k, `E410+k` = -2^k (k = 0..=10).
pub fn space_char(k: u32, negative: bool) -> char {
    char::from_u32(0xE400 + if negative { 0x10 } else { 0 } + k).unwrap()
}

// ---- 그림 ----

/// 5×7 글자 (`#` 채움). 가장자리 1px은 코드가 어둡게 둘러 줍니다.
const LETTERS: &[(char, [&str; 7])] = &[
    (
        '0',
        [
            ".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###.",
        ],
    ),
    (
        '1',
        [
            "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.",
        ],
    ),
    (
        '2',
        [
            ".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####",
        ],
    ),
    (
        '3',
        [
            ".###.", "#...#", "....#", "..##.", "....#", "#...#", ".###.",
        ],
    ),
    (
        '4',
        [
            "...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#.",
        ],
    ),
    (
        '5',
        [
            "#####", "#....", "####.", "....#", "....#", "#...#", ".###.",
        ],
    ),
    (
        '6',
        [
            "..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###.",
        ],
    ),
    (
        '7',
        [
            "#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...",
        ],
    ),
    (
        '8',
        [
            ".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###.",
        ],
    ),
    (
        '9',
        [
            ".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##..",
        ],
    ),
    (',', ["..", "..", "..", "..", ".#", ".#", "#."]),
    ('.', [".", ".", ".", ".", ".", ".", "#"]),
    (
        '%',
        [
            "##..#", "##..#", "...#.", "..#..", ".#...", "#..##", "#..##",
        ],
    ),
    (
        '/',
        [
            "....#", "...#.", "...#.", "..#..", ".#...", ".#...", "#....",
        ],
    ),
    (
        '+',
        [
            ".....", "..#..", "..#..", "#####", "..#..", "..#..", ".....",
        ],
    ),
    (
        '-',
        [
            ".....", ".....", ".....", "#####", ".....", ".....", ".....",
        ],
    ),
    (
        'K',
        [
            "#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#",
        ],
    ),
    (
        'M',
        [
            "#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#",
        ],
    ),
    (
        'B',
        [
            "####.", "#...#", "#...#", "####.", "#...#", "#...#", "####.",
        ],
    ),
    (
        'L',
        [
            "#....", "#....", "#....", "#....", "#....", "#....", "#####",
        ],
    ),
    (
        'v',
        [
            ".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#..",
        ],
    ),
];

/// 9×9 아이콘과 팔레트.
type Icon = (char, [&'static str; 9], &'static [(char, u32)]);

const ICONS: &[Icon] = &[
    (
        COIN,
        [
            "...ddd...",
            ".ddyyydd.",
            ".dyYYyyd.",
            "dyYyyyyod",
            "dyYyyyyod",
            "dyyyyyyod",
            ".dyyyood.",
            ".ddooodd.",
            "...ddd...",
        ],
        &[
            ('d', 0x7a4a00ff),
            ('y', 0xf5c542ff),
            ('Y', 0xfff3b0ff),
            ('o', 0xc9901aff),
        ],
    ),
    (
        GEM,
        [
            ".........",
            "..ddddd..",
            ".dCCcbbd.",
            "dCCccbbbd",
            ".dcccbbd.",
            "..dccbd..",
            "...dcd...",
            "....d....",
            ".........",
        ],
        &[
            ('d', 0x123a6bff),
            ('c', 0x55e0ffff),
            ('C', 0xc8f7ffff),
            ('b', 0x2a8fd6ff),
        ],
    ),
    (
        MOON,
        [
            "...ddd...",
            "..dmmmd..",
            ".dmmdd...",
            ".dmd...z.",
            ".dmd..zzz",
            ".dmmdd.z.",
            "..dmmmd..",
            "...ddd...",
            ".........",
        ],
        &[('d', 0x6b5a1eff), ('m', 0xfff2a8ff), ('z', 0xbfe3ffff)],
    ),
    (
        STAR,
        [
            "....d....",
            "...dyd...",
            "dddyyyddd",
            "dyyyYyyyd",
            ".dyyyyyd.",
            "..dyyyd..",
            ".dyydyyd.",
            "dyd...dyd",
            "dd.....dd",
        ],
        &[('d', 0x7a5200ff), ('y', 0xffd84aff), ('Y', 0xfff6c0ff)],
    ),
    (
        SWORD,
        [
            "......ddd",
            ".....dssd",
            "....dssd.",
            "d..dssd..",
            ".ddssd...",
            "..dhd....",
            ".dhdd....",
            "dhd.d....",
            "dd.......",
        ],
        &[('d', 0x2e2e2eff), ('s', 0xe6e6e6ff), ('h', 0x8b5a2bff)],
    ),
    (
        SHIELD,
        [
            "ddddddddd",
            "dsssbsssd",
            "dsssbsssd",
            "dbbbbbbbd",
            "dsssbsssd",
            ".dssbssd.",
            "..dsbsd..",
            "...dsd...",
            "....d....",
        ],
        &[('d', 0x22324aff), ('s', 0x6fa0d8ff), ('b', 0xf2d16bff)],
    ),
    (
        HEART,
        [
            ".dd...dd.",
            "dRrd.dRrd",
            "dRrrdrrrd",
            "drrrrrrrd",
            "drrrrrrrd",
            ".drrrrrd.",
            "..drrrd..",
            "...drd...",
            "....d....",
        ],
        &[('d', 0x5a0a10ff), ('r', 0xe8323cff), ('R', 0xff9a9aff)],
    ),
    (
        BOOT,
        [
            "..ddddd..",
            "..dbbbd..",
            "..dbbbd..",
            "..dbbbd..",
            "..dbbbdd.",
            ".dbbbbbbd",
            "dbbbbbbbd",
            "ddddddddd",
            ".........",
        ],
        &[('d', 0x3e2412ff), ('b', 0x9b6a3cff)],
    ),
    (
        FLAME,
        [
            "....d....",
            "...dod...",
            "..doood..",
            "..dooood.",
            ".doyyood.",
            ".doyyyod.",
            "doyyyyyod",
            ".doyyyod.",
            "..ddddd..",
        ],
        &[('d', 0x7a1e00ff), ('o', 0xff7a1aff), ('y', 0xffe066ff)],
    ),
];

/// 글자 테두리 색과 글자 색.
const OUTLINE: u32 = 0x1e1e1eff;
const FILL: u32 = 0xffffffff;
/// 판 (반투명 검정).
const PANEL: u32 = 0x00000078;

/// RGBA 그림 한 장.
#[derive(Clone)]
struct Image {
    w: usize,
    h: usize,
    px: Vec<u32>,
}

impl Image {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![0; w * h],
        }
    }
    fn set(&mut self, x: usize, y: usize, c: u32) {
        if x < self.w && y < self.h {
            self.px[y * self.w + x] = c;
        }
    }
    fn get(&self, x: usize, y: usize) -> u32 {
        self.px[y * self.w + x]
    }
    fn blit(&mut self, src: &Image, ox: usize, oy: usize) {
        for y in 0..src.h {
            for x in 0..src.w {
                self.set(ox + x, oy + y, src.get(x, y));
            }
        }
    }
    fn rgba(&self) -> Vec<u8> {
        self.px.iter().flat_map(|p| p.to_be_bytes()).collect()
    }
    /// 바닐라 폰트가 재는 글자 폭: 오른쪽에서부터 처음 불투명한 열 + 1.
    fn ink_width(&self) -> i32 {
        (0..self.w)
            .rev()
            .find(|&x| (0..self.h).any(|y| self.get(x, y) & 0xff != 0))
            .map_or(0, |x| x as i32 + 1)
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

fn panel_piece(width: usize, cap: Option<bool>) -> Image {
    let mut img = Image::new(width, 11);
    for y in 0..11 {
        for x in 0..width {
            // 둥근 모서리: 바깥 열의 맨 위·아래는 비움
            let edge = match cap {
                Some(false) => x == 0,
                Some(true) => x + 1 == width,
                None => false,
            };
            if !(edge && (y == 0 || y == 10)) {
                img.set(x, y, PANEL);
            }
        }
    }
    img
}

fn bar_frame(bar: Bar) -> Image {
    let (edge, back, _) = bar.colors();
    let w = BAR_INNER as usize + 2;
    let mut img = Image::new(w, 9);
    for y in 0..9 {
        for x in 0..w {
            let border = x == 0 || y == 0 || x + 1 == w || y == 8;
            let corner = (x == 0 || x + 1 == w) && (y == 0 || y == 8);
            if corner {
                continue;
            }
            img.set(x, y, if border { edge } else { back });
        }
    }
    img
}

fn bar_fill_image(bar: Bar, cells: usize) -> Image {
    let (_, _, [hi, mid, lo]) = bar.colors();
    let mut img = Image::new(BAR_INNER as usize + 2, 9);
    for x in 1..=cells {
        for y in 1..8 {
            let c = match y {
                1 | 2 => hi,
                6 | 7 => lo,
                _ => mid,
            };
            img.set(x, y, c);
        }
    }
    img
}

// ---- 폰트 ----

/// 글자 하나: 어느 그림판의 몇 번째 칸인가, 화면에서 차지하는 폭.
struct Sheet {
    file: &'static str,
    cell: (usize, usize),
    per_row: usize,
    /// 줄 위쪽보다 몇 px 위에서 그리기 시작하나 (판은 1px 위).
    lift: i32,
    glyphs: Vec<(char, Image)>,
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
}

fn json_chars(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '"' => "\\\"".to_owned(),
            '\\' => "\\\\".to_owned(),
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => format!("\\u{:04x}", c as u32),
            c => c.to_string(),
        })
        .collect()
}

fn sheets() -> Vec<Sheet> {
    let mut small: Vec<(char, Image)> = LETTERS.iter().map(|(c, r)| (*c, letter(r))).collect();
    small.extend(ICONS.iter().map(|(c, rows, pal)| (*c, icon(rows, pal))));
    let mut panel = vec![
        (PANEL_LEFT, panel_piece(2, Some(false))),
        (PANEL_RIGHT, panel_piece(2, Some(true))),
    ];
    for (i, c) in PANEL_MID.iter().enumerate() {
        panel.push((*c, panel_piece(1 << i, None)));
    }
    let bar_sheet = |bar: Bar| {
        let mut glyphs = vec![(bar.frame(), bar_frame(bar))];
        for k in 1..=BAR_INNER {
            glyphs.push((bar.fill(k).unwrap(), bar_fill_image(bar, k as usize)));
        }
        Sheet {
            file: bar.file(),
            cell: (81, 9),
            per_row: 10,
            lift: 0,
            glyphs,
        }
    };
    vec![
        Sheet {
            file: "dangu:font/hud/small.png",
            cell: (9, 9),
            per_row: 16,
            lift: 0,
            glyphs: small,
        },
        Sheet {
            file: "dangu:font/hud/panel.png",
            cell: (64, 11),
            per_row: 9,
            lift: 1,
            glyphs: panel,
        },
        bar_sheet(Bar::Health),
        bar_sheet(Bar::Mana),
    ]
}

/// 글자마다 화면에서 차지하는 폭 (바닐라: 잉크 폭 + 1).
pub fn advance(c: char) -> i32 {
    static TABLE: OnceLock<HashMap<char, i32>> = OnceLock::new();
    let t = TABLE.get_or_init(|| {
        sheets()
            .into_iter()
            .flat_map(|s| s.glyphs.into_iter())
            .map(|(c, g)| (c, g.ink_width() + 1))
            .collect()
    });
    t.get(&c).copied().unwrap_or(0)
}

fn font_json(down: i32, sheets: &[Sheet]) -> String {
    let mut spaces = Vec::new();
    for k in 0..=10u32 {
        spaces.push(format!(
            "\"{}\":{}",
            json_chars(&space_char(k, false).to_string()),
            1 << k
        ));
        spaces.push(format!(
            "\"{}\":-{}",
            json_chars(&space_char(k, true).to_string()),
            1 << k
        ));
    }
    let mut providers = vec![format!(
        "{{\"type\":\"space\",\"advances\":{{{}}}}}",
        spaces.join(",")
    )];
    for s in sheets {
        // 줄의 위쪽 = 기준 + down; 바닐라 글자의 위쪽은 기준 + 7 - ascent
        let ascent = 7 - down + s.lift;
        providers.push(format!(
            "{{\"type\":\"bitmap\",\"file\":\"{}\",\"ascent\":{},\"height\":{},\"chars\":{}}}",
            s.file,
            ascent,
            s.cell.1,
            s.chars_json()
        ));
    }
    format!("{{\"providers\":[{}]}}", providers.join(","))
}

/// 바닐라 26.3 `core/text.vsh`에 표시 색 처리를 더한 것.
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

    // dangu HUD: #FD02F9 = 화면 오른쪽 끝 기준으로 옮김, #FD03F9 = 그대로, 둘의 그림자는 지움
    ivec3 dangu_c = ivec3(round(Color.rgb * 255.0));
    if (dangu_c == ivec3(253, 2, 249)) {
        gl_Position.x += gl_Position.w;
        vertexColor = vec4(1.0, 1.0, 1.0, Color.a);
    } else if (dangu_c == ivec3(253, 3, 249)) {
        vertexColor = vec4(1.0, 1.0, 1.0, Color.a);
    } else if (dangu_c == ivec3(63, 0, 62)) {
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
    let sheets = sheets();
    let mut files = Vec::new();
    for s in &sheets {
        let img = s.image();
        let path = s.file.trim_start_matches("dangu:");
        files.push((
            format!("assets/dangu/textures/{path}"),
            png(img.w as u32, img.h as u32, &img.rgba()),
        ));
    }
    for row in TOP.iter().chain(BOTTOM.iter()).chain([&HP, &MP]) {
        let name = row.id.trim_start_matches("dangu:");
        files.push((
            format!("assets/dangu/font/{name}.json"),
            font_json(row.down, &sheets).into_bytes(),
        ));
    }
    files.push((
        "assets/minecraft/shaders/core/text.vsh".into(),
        TEXT_VSH.as_bytes().to_vec(),
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
        name: "dangu_hud".into(),
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
        assert_eq!(advance(COIN), 10);
        assert_eq!(advance(PANEL_MID[3]), 9);
        assert_eq!(advance(Bar::Health.fill(10).unwrap()), 12);
        assert_eq!(advance(Bar::Mana.frame()), 82);
        assert_eq!(Bar::Mana.fill(BAR_INNER), Some('\u{E39F}'));
    }

    #[test]
    fn fonts_are_valid_json_with_legal_ascent() {
        let sheets = sheets();
        for row in TOP.iter().chain(BOTTOM.iter()).chain([&HP, &MP]) {
            let j = crate::json::parse(&font_json(row.down, &sheets)).unwrap();
            for p in j
                .get("providers")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .skip(1)
            {
                let ascent = p.get("ascent").unwrap().as_f64().unwrap();
                let height = p.get("height").unwrap().as_f64().unwrap();
                assert!(ascent <= height, "{} {ascent} > {height}", row.id);
            }
        }
        let p = pack();
        assert!(
            p.files
                .iter()
                .any(|(n, _)| n == "assets/minecraft/shaders/core/text.vsh")
        );
        assert!(
            p.files
                .iter()
                .any(|(n, _)| n.ends_with("hud/heart/full.png"))
        );
    }
}

/// 미리보기: 클라이언트가 할 배치(공백 폭, 줄마다 ascent, 오른쪽 표시 색)를 흉내 내어 GUI 화면
/// 한 장을 그립니다. `MC_HUD_PREVIEW=<경로.png> cargo test hud_preview -- --ignored`
#[cfg(test)]
pub(crate) mod preview {
    use super::*;
    use crate::protocol::nbt::Tag;

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

        /// 텍스트 한 줄을 `y`(보스바 제목이면 3, 액션바면 화면 높이 - 72)에.
        pub fn draw_line(&mut self, tag: &Tag, y: i32) {
            let sheets = sheets();
            let glyph = |c: char| {
                sheets.iter().find_map(|s| {
                    s.glyphs
                        .iter()
                        .find(|(g, _)| *g == c)
                        .map(|(_, img)| (img.clone(), s.lift))
                })
            };
            let rows: Vec<&RowFont> = TOP.iter().chain(BOTTOM.iter()).chain([&HP, &MP]).collect();
            let Tag::Compound(f) = tag else { return };
            let Some((_, Tag::List(parts))) = f.iter().find(|(k, _)| k == "extra") else {
                return;
            };
            let mut cursor = 0;
            for p in parts {
                let Tag::Compound(f) = p else { continue };
                let get = |k: &str| {
                    f.iter().find_map(|(n, v)| match v {
                        Tag::String(s) if n == k => Some(s.clone()),
                        _ => None,
                    })
                };
                let (text, font, color) = (
                    get("text").unwrap(),
                    get("font").unwrap(),
                    get("color").unwrap(),
                );
                let down = rows.iter().find(|r| r.id == font).unwrap().down;
                let shift = if color == RIGHT {
                    self.img.w as i32 / 2
                } else {
                    0
                };
                for c in text.chars() {
                    let u = c as u32;
                    if (0xE400..=0xE40A).contains(&u) {
                        cursor += 1 << (u - 0xE400);
                        continue;
                    }
                    if (0xE410..=0xE41A).contains(&u) {
                        cursor -= 1 << (u - 0xE410);
                        continue;
                    }
                    let (img, lift) = glyph(c).expect("모르는 글자");
                    let x0 = self.img.w as i32 / 2 + shift + cursor;
                    let y0 = y + down - lift;
                    for gy in 0..img.h {
                        for gx in 0..img.w {
                            let px = img.get(gx, gy);
                            if px & 0xff != 0 {
                                self.blend(x0 + gx as i32, y0 + gy as i32, px);
                            }
                        }
                    }
                    cursor += advance(c);
                }
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
        let Ok(path) = std::env::var("MC_HUD_PREVIEW") else {
            return;
        };
        let (w, h) = (480, 270);
        let mut s = Screen::new(w, h);
        let top = crate::plugins::hud::top(&crate::plugins::hud::Currencies {
            money: 1_234_567,
            cash: 120,
            afk: 45,
        });
        let stats = crate::plugins::rpg::Stats {
            level: 12,
            attack: 7.0,
            defense: 8.0,
            max_health: 20.0,
            speed: 100.0,
            max_mana: 155.0,
        };
        let bottom = crate::plugins::hud::bottom(&crate::plugins::hud::Bottom {
            stats: Some(&stats),
            health: Some((14.0, 20.0)),
            mana: Some((96.0, 155.0)),
        });
        s.draw_line(&top, 3);
        s.draw_line(&bottom, h as i32 - 72);
        std::fs::write(path, s.png(3)).unwrap();
    }
}
