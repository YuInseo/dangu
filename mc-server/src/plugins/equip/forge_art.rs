//! 장비 손질 창의 그림: 바탕 한 장 + 탭 불빛·칸 테두리·막대·버튼 덮개 (2배 해상도, 코드로 그림).
//!
//! 자리(상자 왼쪽 위 기준 GUI px)는 [`super::forge`]와 같이 씁니다.
//!
//! ```text
//!  -76        0                              176          252
//!   ┌──────────────────────────────────────────────────────┐ 0
//!   │ 장비 손질 · 강화                              골드 … │
//!   ├─────────┬──[강화][연마][조율][전승][분해]──┬─────────┤ 18
//!   │ 장비    │ ▢          ◇ (장비) ◇        ▢  │ 예상    │
//!   │ 정보    │ ▢                              │ 능력치  │
//!   │         │ ▢   장인의 기운 ▬▬▬▬▬▬▬▬▬▬     │ 필요    │
//!   │         │ ◀       [  강화하기  ]       ⚒ │ 재료    │ 126
//!   ├─────────┼──────── 인벤토리 ────────────┼─────────┤
//!   │ 도움말  │ ▢▢▢▢▢▢▢▢▢ (가방 3줄 + 핫바)  │ 재료    │
//!   └─────────┴────────────────────────────────┴─────────┘ 222
//! ```

use std::sync::OnceLock;

use crate::plugins::contents::source::Pack;
use crate::plugins::gui::{self, CHEST_H, CHEST_W, Glyph};
use crate::plugins::hud::art::Image;

pub const FONT: &str = "hoya:gui/forge";

// ---- 자리 ----

/// 바탕 전체 (상자 밖으로 양옆에 판).
pub const LEFT: i32 = -76;
pub const RIGHT: i32 = CHEST_W + 76;
/// 왼쪽·오른쪽 판의 안쪽 (글을 쓰는 곳).
pub const SIDE_L: (i32, i32) = (-72, 0);
pub const SIDE_R: (i32, i32) = (176, 248);
/// 위 판(제목·탭·가운데)과 아래 판(가방) 경계.
pub const TOP_Y: (i32, i32) = (18, 126);
pub const BOTTOM_Y: (i32, i32) = (130, 218);

/// 탭 다섯: 탭 k의 왼쪽 x와 폭.
pub const TAB_W: i32 = 31;
pub const fn tab_x(k: usize) -> i32 {
    8 + 33 * k as i32
}
/// 탭 줄의 칸 → 탭 번호.
pub const TAB_OF_CELL: [usize; 9] = [0, 0, 1, 1, 2, 3, 3, 4, 4];

/// 가운데 장비 칸, 그 둘레 마름모.
pub const CENTER: usize = 22;
pub const DIAMOND: (i32, i32, i32) = (88, 62, 27);
/// 착용 장비 칸 (왼쪽 세로 줄).
pub const WORN: [usize; 4] = [10, 19, 28, 37];
/// 연마: 주문서 칸.
pub const SCROLL: usize = 25;
/// 전승: 보내는 장비 · 받는 장비.
pub const SOURCE: usize = 20;
pub const TARGET: usize = 24;
/// 큰 버튼 (칸 세 개).
pub const BUTTON: [usize; 3] = [48, 49, 50];
pub const BUTTON_RECT: (i32, i32, i32, i32) = (61, 107, 115, 125);
/// 작은 버튼: 장비 창으로 · 대장간.
pub const BACK: usize = 45;
pub const SMITH: usize = 53;
/// 장인의 기운 막대 (틀 바깥, 안쪽은 1px 안).
pub const BAR: (i32, i32, i32, i32) = (46, 101, 150, 107);

// ---- 글자 번호 ----

pub const BG: char = '\u{E800}';
pub const TAB_ON: char = '\u{E801}';
pub const SLOT: char = '\u{E802}';
pub const BUTTON_OFF: char = '\u{E803}';
pub const BUTTON_WARN: char = '\u{E804}';
pub const ARROW: char = '\u{E805}';
/// 막대 채움 조각 1·2·4…64px.
pub const BAR_PIECE: char = '\u{E810}';
pub const BAR_PIECES: usize = 7;

// ---- 색 (RGBA) ----

const OUT: u32 = 0x0b0806ff;
const BASE: u32 = 0x18120dff;
const PANEL: u32 = 0x221913ff;
const PANEL_HI: u32 = 0x2c2118ff;
const GOLD_D: u32 = 0x5e4729ff;
const GOLD: u32 = 0xa9844bff;
const GOLD_L: u32 = 0xe9cd8fff;
const SLOT_BG: u32 = 0x100c09ff;
const SLOT_EDGE: u32 = 0x4b3b27ff;

fn rgba(c: u32) -> [f32; 4] {
    [
        (c >> 24 & 0xff) as f32,
        (c >> 16 & 0xff) as f32,
        (c >> 8 & 0xff) as f32,
        (c & 0xff) as f32 / 255.0,
    ]
}

fn pack_rgba(r: f32, g: f32, b: f32, a: f32) -> u32 {
    let c = |v: f32| v.round().clamp(0.0, 255.0) as u32;
    c(r) << 24 | c(g) << 16 | c(b) << 8 | c(a * 255.0)
}

/// 밝기를 `k`배.
fn shade(c: u32, k: f32) -> u32 {
    let [r, g, b, a] = rgba(c);
    pack_rgba(r * k, g * k, b * k, a)
}

fn mix(a: u32, b: u32, t: f32) -> u32 {
    let (x, y) = (rgba(a), rgba(b));
    pack_rgba(
        x[0] + (y[0] - x[0]) * t,
        x[1] + (y[1] - x[1]) * t,
        x[2] + (y[2] - x[2]) * t,
        x[3] + (y[3] - x[3]) * t,
    )
}

/// 결 무늬용 작은 해시 (늘 같은 그림).
fn hash(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// 2배 해상도 그림판. 좌표는 GUI px(상자 기준)이고, `t`가 붙은 것은 반 픽셀(텍셀) 단위.
struct Paint {
    img: Image,
    /// 그림 왼쪽 위의 GUI 좌표
    x0: i32,
    y0: i32,
}

impl Paint {
    fn new(x0: i32, y0: i32, w: i32, h: i32) -> Self {
        Self {
            img: Image::new((w * 2) as usize, (h * 2) as usize),
            x0,
            y0,
        }
    }

    fn tx(&self, x: i32) -> i32 {
        (x - self.x0) * 2
    }

    fn ty(&self, y: i32) -> i32 {
        (y - self.y0) * 2
    }

    fn put(&mut self, tx: i32, ty: i32, c: u32) {
        if tx >= 0 && ty >= 0 {
            self.img.set(tx as usize, ty as usize, c);
        }
    }

    fn get(&self, tx: i32, ty: i32) -> u32 {
        if tx < 0 || ty < 0 || tx as usize >= self.img.w || ty as usize >= self.img.h {
            return 0;
        }
        self.img.get(tx as usize, ty as usize)
    }

    /// 반투명 덧칠.
    fn blend(&mut self, tx: i32, ty: i32, c: u32) {
        let [r, g, b, a] = rgba(c);
        let under = self.get(tx, ty);
        let [ur, ug, ub, ua] = rgba(under);
        let out_a = a + ua * (1.0 - a);
        if out_a <= 0.0 {
            return;
        }
        let f = |s: f32, d: f32| (s * a + d * ua * (1.0 - a)) / out_a;
        self.put(tx, ty, pack_rgba(f(r, ur), f(g, ug), f(b, ub), out_a));
    }

    /// 텍셀 사각형 `[x0, x1) × [y0, y1)`.
    fn rect_t(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u32) {
        for y in y0..y1 {
            for x in x0..x1 {
                self.put(x, y, c);
            }
        }
    }

    /// GUI px 사각형.
    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u32) {
        let (a, b, cc, d) = (self.tx(x0), self.ty(y0), self.tx(x1), self.ty(y1));
        self.rect_t(a, b, cc, d, c);
    }

    /// 결이 있는 판: 위가 조금 밝고, 가장자리가 어두움.
    fn grain(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u32) {
        let (a, b, cc, d) = (self.tx(x0), self.ty(y0), self.tx(x1), self.ty(y1));
        let h = (d - b).max(1) as f32;
        for y in b..d {
            for x in a..cc {
                let n = (hash(x, y) % 9) as f32 / 8.0 - 0.5;
                let edge = (x - a).min(cc - 1 - x).min(y - b).min(d - 1 - y) as f32;
                let vignette = 1.0 - (6.0 - edge.min(6.0)) * 0.025;
                let k = (1.04 - 0.1 * (y - b) as f32 / h + n * 0.05) * vignette;
                self.put(x, y, shade(c, k));
            }
        }
    }

    /// 금테 판: 바깥 어두운 선, 금선, 안쪽 결.
    fn framed(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, fill: u32) {
        let (a, b, cc, d) = (self.tx(x0), self.ty(y0), self.tx(x1), self.ty(y1));
        self.rect_t(a, b, cc, d, OUT);
        self.rect_t(a + 1, b + 1, cc - 1, d - 1, GOLD_D);
        self.rect_t(a + 2, b + 2, cc - 2, d - 2, OUT);
        self.grain(x0, y0, x1, y1, fill);
        // grain이 다시 칠한 테두리를 돌려놓음
        for x in a..cc {
            for (y, c) in [(b, OUT), (b + 1, GOLD_D), (d - 2, GOLD_D), (d - 1, OUT)] {
                self.put(x, y, c);
            }
        }
        for y in b..d {
            for (x, c) in [(a, OUT), (a + 1, GOLD_D), (cc - 2, GOLD_D), (cc - 1, OUT)] {
                if y > b && y < d - 1 {
                    self.put(x, y, c);
                }
            }
        }
        // 안쪽 위 빛
        for x in a + 2..cc - 2 {
            self.blend(x, b + 2, 0xffe6b022);
        }
        // 모서리 장식
        for (x, y) in [
            (a + 1, b + 1),
            (cc - 2, b + 1),
            (a + 1, d - 2),
            (cc - 2, d - 2),
        ] {
            self.put(x, y, GOLD_L);
        }
    }

    /// 아이템 칸 테두리 (칸 왼쪽 위 = 아이템 자리).
    fn slot(&mut self, x: i32, y: i32) {
        let (a, b) = (self.tx(x - 1), self.ty(y - 1));
        self.rect_t(a, b, a + 36, b + 36, SLOT_EDGE);
        self.rect_t(a + 1, b + 1, a + 35, b + 35, OUT);
        self.rect_t(a + 2, b + 2, a + 34, b + 34, SLOT_BG);
        // 아래·오른쪽 빛 (움푹 들어간 느낌)
        for i in 2..34 {
            self.put(a + i, b + 33, 0x2a2016ff);
            self.put(a + 33, b + i, 0x2a2016ff);
        }
    }

    /// 머리줄: 판 위쪽의 어두운 띠와 금선.
    fn header(&mut self, x0: i32, y0: i32, x1: i32) {
        let (a, b, cc) = (self.tx(x0), self.ty(y0), self.tx(x1));
        for y in b..b + 22 {
            for x in a..cc {
                self.blend(x, y, 0x00000055);
            }
        }
        for x in a..cc {
            let t = ((x - a) as f32 / (cc - a) as f32 * std::f32::consts::PI).sin();
            self.put(x, b + 22, mix(GOLD_D, GOLD, t));
        }
    }

    /// 마름모 장식: 겹 마름모 · 빛살 · 꼭짓점 보석.
    fn diamond(&mut self, cx: i32, cy: i32, r: i32) {
        let (cx, cy, r) = (self.tx(cx), self.ty(cy), r * 2);
        // 안쪽 빛
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                let d = (x - cx).abs() + (y - cy).abs();
                if d <= r {
                    let t = 1.0 - d as f32 / r as f32;
                    let a = (t * t * 90.0) as u32;
                    self.blend(x, y, 0xffc46a00 | a);
                }
            }
        }
        // 빛살 (안쪽 마름모 안에서만, 희미하게)
        for k in 0..24 {
            let ang = k as f32 / 24.0 * std::f32::consts::TAU;
            for s in 14..(r - 4) {
                let x = cx + (ang.cos() * s as f32).round() as i32;
                let y = cy + (ang.sin() * s as f32).round() as i32;
                if (x - cx).abs() + (y - cy).abs() < r - 6 {
                    self.blend(x, y, 0xe9cd8f30);
                }
            }
        }
        // 겹 마름모 선
        for (rr, c, w) in [(r, GOLD, 2), (r - 8, GOLD_D, 1), (r - 14, GOLD_L, 1)] {
            for y in cy - rr..=cy + rr {
                for x in cx - rr..=cx + rr {
                    let d = (x - cx).abs() + (y - cy).abs();
                    if d <= rr && d > rr - w {
                        self.put(x, y, c);
                    }
                }
            }
        }
        // 바깥 꺾쇠 (모서리에서 바깥으로 짧은 선)
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            for i in 0..10 {
                self.put(cx + dx * (r + 2 + i), cy + dy * (r + 2 + i), GOLD_D);
            }
            // 꼭짓점 보석
            let (gx, gy) = (cx + dx * r, cy + dy * r);
            for y in -3..=3 {
                for x in -3..=3 {
                    let d = i32::abs(x) + i32::abs(y);
                    if d <= 3 {
                        self.put(gx + x, gy + y, if d <= 1 { GOLD_L } else { GOLD });
                    }
                }
            }
        }
    }

    /// 큰 버튼 바탕.
    fn button(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let (a, b, cc, d) = (self.tx(x0), self.ty(y0), self.tx(x1), self.ty(y1));
        self.rect_t(a, b, cc, d, OUT);
        self.rect_t(a + 1, b + 1, cc - 1, d - 1, GOLD);
        let h = (d - b - 4) as f32;
        for y in b + 2..d - 2 {
            let t = (y - b - 2) as f32 / h;
            for x in a + 2..cc - 2 {
                self.put(x, y, mix(0x6a4f27ff, 0x2e2211ff, t));
            }
        }
        for x in a + 2..cc - 2 {
            self.put(x, b + 2, GOLD_L);
        }
    }
}

/// 바탕 한 장.
fn background() -> Image {
    let mut p = Paint::new(LEFT, 0, RIGHT - LEFT, CHEST_H);
    p.rect(LEFT, 0, RIGHT, CHEST_H, BASE);
    p.framed(LEFT, 0, RIGHT, CHEST_H, BASE);
    // 제목 줄
    p.header(LEFT + 2, 2, RIGHT - 2);
    // 왼쪽·오른쪽·가운데 판
    p.framed(SIDE_L.0, TOP_Y.0, SIDE_L.1, TOP_Y.1, PANEL);
    p.framed(SIDE_R.0, TOP_Y.0, SIDE_R.1, TOP_Y.1, PANEL);
    p.framed(SIDE_L.0, BOTTOM_Y.0, SIDE_L.1, BOTTOM_Y.1, PANEL);
    p.framed(SIDE_R.0, BOTTOM_Y.0, SIDE_R.1, BOTTOM_Y.1, PANEL);
    for (x0, x1) in [SIDE_L, SIDE_R] {
        p.header(x0 + 1, TOP_Y.0 + 1, x1 - 1);
        p.header(x0 + 1, BOTTOM_Y.0 + 1, x1 - 1);
    }
    // 탭 줄
    p.rect(4, 17, 172, 35, 0x100b08ff);
    for k in 0..5 {
        let x = tab_x(k);
        p.framed(x, 18, x + TAB_W, 34, PANEL);
    }
    // 가운데 판
    p.framed(4, 36, 172, 127, PANEL_HI);
    p.diamond(DIAMOND.0, DIAMOND.1, DIAMOND.2);
    let (cx, cy) = gui::cell(CENTER);
    p.slot(cx, cy);
    // 막대 틀
    let (bx0, by0, bx1, by1) = BAR;
    p.rect(bx0, by0, bx1, by1, OUT);
    let (a, b, c, d) = (p.tx(bx0), p.ty(by0), p.tx(bx1), p.ty(by1));
    p.rect_t(a + 1, b + 1, c - 1, d - 1, GOLD_D);
    p.rect_t(a + 2, b + 2, c - 2, d - 2, 0x0d0907ff);
    // 큰 버튼, 작은 버튼 칸
    let (x0, y0, x1, y1) = BUTTON_RECT;
    p.button(x0, y0, x1, y1);
    for s in [BACK, SMITH] {
        let (x, y) = gui::cell(s);
        p.slot(x, y);
    }
    // 가방 판: 이름표 띠(바닐라 "인벤토리" 글이 회색이라 밝은 띠 위에) + 칸
    p.framed(4, 127, 172, 219, PANEL);
    let (a, b, c) = (p.tx(5), p.ty(128), p.tx(171));
    for y in b..b + 20 {
        for x in a..c {
            let t = (y - b) as f32 / 20.0;
            p.put(x, y, mix(0xb79c6fff, 0x8f7653ff, t));
        }
    }
    for x in a..c {
        p.put(x, b + 20, GOLD_D);
    }
    for r in 0..3 {
        for c in 0..9 {
            p.slot(8 + 18 * c, 139 + 18 * r);
        }
    }
    for c in 0..9 {
        p.slot(8 + 18 * c, 197);
    }
    p.img
}

fn tab_on() -> Image {
    let mut p = Paint::new(0, 0, TAB_W, 16);
    p.button(0, 0, TAB_W, 16);
    // 아래 빛줄
    let w = p.img.w as i32;
    for x in 2..w - 2 {
        p.put(x, 29, GOLD_L);
    }
    p.img
}

fn slot_frame() -> Image {
    let mut p = Paint::new(-1, -1, 18, 18);
    p.slot(0, 0);
    // 테두리만 금빛으로
    let (w, h) = (p.img.w as i32, p.img.h as i32);
    for i in 0..w {
        p.put(i, 0, GOLD);
        p.put(i, h - 1, GOLD_D);
    }
    for i in 0..h {
        p.put(0, i, GOLD);
        p.put(w - 1, i, GOLD_D);
    }
    p.img
}

/// 막대 채움 조각 (폭 w px, 높이 4px): 금빛 위아래 그러데이션.
fn bar_piece(w: i32) -> Image {
    let mut p = Paint::new(0, 0, w, 4);
    for y in 0..8 {
        let t = y as f32 / 7.0;
        let c = if y == 0 {
            0xfff1c4ff
        } else {
            mix(0xf2c45aff, 0x9c6a1eff, t)
        };
        for x in 0..w * 2 {
            p.put(x, y, c);
        }
    }
    p.img
}

/// 버튼 덮개: 끈 버튼(어둡게) · 확인 버튼(붉게).
fn button_cover(c: u32) -> Image {
    let (x0, y0, x1, y1) = BUTTON_RECT;
    let mut p = Paint::new(0, 0, x1 - x0, y1 - y0);
    let (w, h) = (p.img.w as i32, p.img.h as i32);
    for y in 0..h {
        for x in 0..w {
            p.put(x, y, c);
        }
    }
    let _ = y0;
    p.img
}

fn arrow() -> Image {
    let mut p = Paint::new(0, 0, 11, 9);
    let rows = [
        "..........##........",
        "..........###.......",
        "..........####......",
        "#################...",
        "##################..",
        "###################.",
        "##################..",
        "#################...",
        "..........####......",
        "..........###.......",
        "..........##........",
    ];
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if ch == '#' {
                let t = y as f32 / rows.len() as f32;
                p.put(x as i32 + 1, y as i32 + 3, mix(GOLD_L, GOLD, t));
            }
        }
    }
    p.img
}

/// 그림 글자 전부.
pub fn glyphs() -> &'static [Glyph] {
    static GLYPHS: OnceLock<Vec<Glyph>> = OnceLock::new();
    GLYPHS.get_or_init(|| {
        let g = |ch: char, name: &str, img: Image, fixed_top: Option<i32>| Glyph {
            ch,
            file: format!("hoya:font/gui/forge_{name}.png"),
            img,
            fixed_top,
        };
        let mut out = vec![
            g(BG, "bg", background(), Some(0)),
            g(TAB_ON, "tab", tab_on(), None),
            g(SLOT, "slot", slot_frame(), None),
            g(BUTTON_OFF, "off", button_cover(0x0b0806b0), None),
            g(BUTTON_WARN, "warn", button_cover(0xc0302a70), None),
            g(ARROW, "arrow", arrow(), None),
        ];
        for k in 0..BAR_PIECES {
            out.push(g(
                char::from_u32(BAR_PIECE as u32 + k as u32).unwrap(),
                &format!("bar{}", 1 << k),
                bar_piece(1 << k),
                None,
            ));
        }
        out
    })
}

pub fn glyph(ch: char) -> &'static Glyph {
    glyphs().iter().find(|g| g.ch == ch).expect("forge glyph")
}

/// 막대 조각들 (작은 것부터).
pub fn bar_pieces() -> &'static [Glyph] {
    &glyphs()[glyphs().len() - BAR_PIECES..]
}

pub fn pack() -> Pack {
    Pack {
        name: "hoya_forge".into(),
        configs: Vec::new(),
        files: gui::font_files(FONT, glyphs()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_fits_the_art() {
        let bg = glyph(BG);
        assert_eq!(bg.img.w as i32, (RIGHT - LEFT) * 2);
        assert_eq!(bg.img.h as i32, CHEST_H * 2);
        // 바탕은 상자 전체를 덮음 (바닐라 회색이 비치지 않게)
        for y in 0..bg.img.h {
            for x in ((-LEFT * 2) as usize)..(((CHEST_W - LEFT) * 2) as usize) {
                assert_eq!(bg.img.get(x, y) & 0xff, 0xff, "({x}, {y})");
            }
        }
        assert_eq!(tab_x(4) + TAB_W, 171);
        for (c, &t) in TAB_OF_CELL.iter().enumerate() {
            let center = gui::cell(c).0 + 8;
            let d = (tab_x(t) + TAB_W / 2 - center).abs();
            assert!(d <= 18, "칸 {c} → 탭 {t}");
        }
        for (i, g) in bar_pieces().iter().enumerate() {
            assert_eq!(g.advance(), (1 << i) + 1);
        }
        let mut chars: Vec<char> = glyphs().iter().map(|g| g.ch).collect();
        chars.dedup();
        assert_eq!(chars.len(), glyphs().len());
        assert!(
            pack()
                .files
                .iter()
                .any(|(n, _)| n == "assets/hoya/font/gui/forge.json")
        );
        assert!(
            pack()
                .files
                .iter()
                .any(|(n, _)| n == "assets/hoya/textures/font/gui/forge_bg.png")
        );
    }
}
