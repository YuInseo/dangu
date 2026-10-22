//! 화면 HUD: 왼쪽 위 재화, 오른쪽 위 퀘스트, 오른쪽 아래 RPG 스탯, 핫바 위 체력·마나바.
//!
//! ```text
//!  ┃🪙 돈     1,234,567┃                         ┃📜 퀘스트          2개 진행 중┃
//!  ┃💎 캐시         120┃                         ┃주간 나무 베기             12/30┃
//!  ┃🌙 잠수          45┃                         ┃▬▬▬▬▬▬▬▬▬▬▬───────────────┃
//!                                                ┃길드 철광석 캐기 (함께)    40/200┃
//!                                                ┃▬▬▬▬───────────────────────┃
//!
//!                                                             ┃★ 레벨      12┃
//!                                                             ┃⚔ 공격력     7┃
//!                                                             ┃🛡 방어력     8┃
//!   💧 [■■■■■■■■■■ 96/155 ─────]                              ┃♥ 체력      20┃
//!   ♥ [■■■■■■■■■■■■■ 14/20 ──]                                ┃👢 속도    100%┃
//!   [핫바 ........................]                            ┃🔥 전투력   209┃
//! ```
//!
//! 위쪽은 분홍 보스바 여러 개의 제목으로 (줄마다 19px; 퀘스트 이름은 바닐라 폰트로 그 줄에),
//! 아래쪽과 체력바는 액션바 글로 1초마다 다시 보냅니다. 그림·폰트·셰이더는 [`art`], 줄에 글자를
//! 놓는 것은 [`canvas`]. 리소스 팩을 받지 않은 사람에게는 보내지 않습니다. `/hud`로 재화·퀘스트·
//! 스탯 판을 끄고 켭니다 (체력·마나바는 바닐라 하트를 대신하므로 남음).

pub mod art;
pub mod canvas;
mod glyphs;
#[cfg(test)]
mod preview;
mod session;
pub mod sprites;

use std::sync::atomic::{AtomicBool, Ordering};

use art::{Anchor, BOTTOM, HP, MP, Pal, RowFont, Strip, T0, T1, T2, TB, TQ, tint};
use canvas::{Canvas, fit, label_width, small_width};

use crate::plugins::quest::{HudQuest, Kind};
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

/// 화면 끝에서 띄우는 거리.
const MARGIN: i32 = 4;
/// 위쪽에 쓰는 보스바 줄 수 (머리 + 퀘스트 3개).
pub const TOP_LINES: usize = 4;

/// `1,234,567`. 너무 길면 `12.3B`처럼.
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

/// 판 한 묶음의 한 줄: 아이콘, 라벨, 값, 왼쪽 줄무늬 색(없으면 None), 값 색.
struct Item {
    icon: char,
    label: &'static str,
    value: String,
    accent: Option<Pal>,
    value_pal: Pal,
}

/// 카드 여러 장을 세로로 (모두 같은 폭). `x0`이 왼쪽 끝 (기준점에서).
fn cards(c: &mut Canvas, rows: &[RowFont], anchor: Anchor, left: bool, items: &[Item]) {
    let label_w = items
        .iter()
        .map(|i| label_width(i.label))
        .max()
        .unwrap_or(0);
    let value_w = items
        .iter()
        .map(|i| small_width(&i.value))
        .max()
        .unwrap_or(0);
    let pad = if items.iter().any(|i| i.accent.is_some()) {
        5
    } else {
        3
    };
    let w = pad + 9 + 3 + label_w + 7 + value_w + 4;
    let (x0, x1) = if left {
        (MARGIN, MARGIN + w)
    } else {
        (-MARGIN - w, -MARGIN)
    };
    let white = tint(anchor, Pal::White);
    for (row, it) in rows.iter().zip(items) {
        c.strip(row, Strip::Card11, white, x0, x1);
        if let Some(a) = it.accent {
            c.strip(row, Strip::Solid11, tint(anchor, a), x0, x0 + 2);
        }
        c.glyph(row, white, x0 + pad, it.icon);
        c.label(row, tint(anchor, Pal::Light), x0 + pad + 12, it.label);
        c.small(
            row,
            tint(anchor, it.value_pal),
            x1 - 4 - small_width(&it.value),
            &it.value,
        );
    }
}

pub struct Currencies {
    pub money: i64,
    pub cash: i64,
    pub afk: i64,
}

fn kind_style(k: Kind) -> (&'static str, Pal) {
    match k {
        Kind::Normal => ("일반", Pal::Green),
        Kind::Daily => ("일일", Pal::Pink),
        Kind::Weekly => ("주간", Pal::Gold),
        Kind::Guild => ("길드", Pal::Aqua),
    }
}

/// 퀘스트 카드 폭.
const QUEST_MIN_W: i32 = 130;
const QUEST_MAX_W: i32 = 180;

fn progress_text(q: &HudQuest) -> String {
    if q.done {
        "완료".into()
    } else {
        format!("{}/{}", q.progress.min(q.target), q.target)
    }
}

/// 위쪽 줄들: 0번 줄은 재화(왼쪽)와 퀘스트 머리(오른쪽), 1번부터 퀘스트 하나씩.
/// `quests`가 None이면 퀘스트 판을 그리지 않습니다.
pub fn top(cur: Option<&Currencies>, quests: Option<&[HudQuest]>) -> Vec<Tag> {
    let mut first = Canvas::default();
    if let Some(cur) = cur {
        cards(
            &mut first,
            &[T0, T1, T2],
            Anchor::Left,
            true,
            &[
                Item {
                    icon: sprites::COIN,
                    label: "돈",
                    value: number(cur.money),
                    accent: Some(Pal::Gold),
                    value_pal: Pal::White,
                },
                Item {
                    icon: sprites::GEM,
                    label: "캐시",
                    value: number(cur.cash),
                    accent: Some(Pal::Aqua),
                    value_pal: Pal::White,
                },
                Item {
                    icon: sprites::MOON,
                    label: "잠수",
                    value: number(cur.afk),
                    accent: Some(Pal::Yellow),
                    value_pal: Pal::White,
                },
            ],
        );
    }
    let mut lines = Vec::new();
    if let Some(qs) = quests {
        let qs = &qs[..qs.len().min(TOP_LINES - 1)];
        // 폭: 가장 긴 줄에 맞추되 정해진 범위 안에서
        let want = qs
            .iter()
            .map(|q| {
                let (tag, _) = kind_style(q.kind);
                5 + art::default_width(tag)
                    + 4
                    + art::default_width(&q.name)
                    + 8
                    + art::default_width(&progress_text(q))
                    + 4
            })
            .max()
            .unwrap_or(0);
        let w = want.clamp(QUEST_MIN_W, QUEST_MAX_W);
        let (x0, x1) = (-MARGIN - w, -MARGIN);
        let r = |p| tint(Anchor::Right, p);

        // 머리
        first.strip(&T0, Strip::Card11, r(Pal::White), x0, x1);
        first.glyph(&T0, r(Pal::White), x0 + 3, sprites::SCROLL);
        first.text(r(Pal::Gold), x0 + 15, "퀘스트");
        let note = if qs.is_empty() {
            "/퀘스트 로 받기".to_owned()
        } else {
            format!("{}개 진행 중", qs.iter().filter(|q| !q.done).count())
        };
        first.text(r(Pal::Gray), x1 - 4 - art::default_width(&note), &note);

        for q in qs {
            let mut c = Canvas::default();
            let (tag, pal) = kind_style(q.kind);
            let prog = progress_text(q);
            c.strip(&TQ, Strip::Card17, r(Pal::White), x0, x1);
            c.strip(&TQ, Strip::Solid17, r(pal), x0, x0 + 2);
            let x = c.text(r(pal), x0 + 5, tag);
            let room = (x1 - 4 - art::default_width(&prog) - 6) - (x + 4);
            c.text(
                r(if q.done { Pal::Gray } else { Pal::White }),
                x + 4,
                &fit(&q.name, room),
            );
            c.text(
                r(if q.done { Pal::Green } else { Pal::Light }),
                x1 - 4 - art::default_width(&prog),
                &prog,
            );
            // 진행 막대
            let (b0, b1) = (x0 + 5, x1 - 4);
            c.strip(&TB, Strip::Solid3, r(Pal::Dark), b0, b1);
            let filled = if q.done {
                b1 - b0
            } else {
                ((b1 - b0) as i64 * i64::from(q.progress.min(q.target)))
                    .checked_div(i64::from(q.target))
                    .unwrap_or(0) as i32
            };
            if filled > 0 {
                c.strip(&TB, Strip::Solid3, r(pal), b0, b0 + filled.max(2));
            }
            lines.push(c.finish());
        }
    }
    if first.is_empty() && lines.is_empty() {
        return Vec::new();
    }
    lines.insert(0, first.finish());
    lines
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
        bar(&mut c, &HP, sprites::HEART, Pal::Red, now, max);
    }
    if let Some((now, max)) = b.mana {
        bar(&mut c, &MP, sprites::DROP, Pal::Blue, now, max);
    }
    if let Some(s) = b.stats {
        let item = |icon, label, value: String, value_pal| Item {
            icon,
            label,
            value,
            accent: None,
            value_pal,
        };
        cards(
            &mut c,
            &BOTTOM,
            Anchor::Right,
            false,
            &[
                item(sprites::STAR, "레벨", s.level.to_string(), Pal::Yellow),
                item(
                    sprites::SWORD,
                    "공격력",
                    number(s.attack as i64),
                    Pal::White,
                ),
                item(sprites::DROP, "마법력", number(s.magic as i64), Pal::Aqua),
                item(
                    sprites::SHIELD,
                    "방어력",
                    number(s.defense as i64),
                    Pal::White,
                ),
                item(
                    sprites::HEART,
                    "치명타",
                    format!("{:.0}%", s.crit_chance),
                    Pal::Pink,
                ),
                item(
                    sprites::FLAME,
                    "전투력",
                    number(s.combat_power()),
                    Pal::Orange,
                ),
            ],
        );
    }
    c.finish()
}

/// 핫바 왼쪽 위(가운데에서 -91..-10): 아이콘 + 막대. 바닐라 하트 자리에 체력, 그 위 갑옷 자리에 마나.
fn bar(c: &mut Canvas, row: &RowFont, icon: char, pal: Pal, now: f32, max: f32) {
    const X: i32 = -91;
    let stay = |p| tint(Anchor::Center, p);
    c.glyph(row, stay(Pal::White), X, icon);
    let bx = X + 10;
    c.glyph(row, stay(Pal::White), bx, art::BAR_FRAME);
    let ratio = if max > 0.0 {
        (now / max).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let cells = (ratio * art::BAR_INNER as f32).round() as i32;
    // 조금이라도 남았으면 한 칸은 보이게
    if let Some(fill) = art::bar_fill(cells.max(i32::from(now > 0.0))) {
        c.glyph(row, stay(pal), bx, fill);
    }
    let label = format!("{}/{}", now.ceil() as i64, max.ceil() as i64);
    let mid = bx + art::BAR_W / 2;
    c.small(row, stay(Pal::White), mid - small_width(&label) / 2, &label);
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas::{total_advance, walk};

    fn quests() -> Vec<HudQuest> {
        vec![
            HudQuest {
                kind: Kind::Weekly,
                name: "나무 베기".into(),
                progress: 12,
                target: 30,
                done: false,
            },
            HudQuest {
                kind: Kind::Guild,
                name: "철광석 캐기 (길드원 모두 함께 아주 길게 이름을 붙임)".into(),
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
        ]
    }

    fn visible(tag: &Tag) -> String {
        walk(tag)
            .into_iter()
            .filter(|(_, _, c)| art::space_advance(*c).is_none())
            .map(|(_, _, c)| c)
            .collect()
    }

    #[test]
    fn lines_have_zero_width_so_they_center_on_the_screen() {
        let qs = quests();
        let lines = top(
            Some(&Currencies {
                money: 1_234_567,
                cash: 120,
                afk: 45,
            }),
            Some(&qs),
        );
        assert_eq!(lines.len(), 4);
        for l in &lines {
            assert_eq!(total_advance(l), 0);
        }
        assert!(visible(&lines[1]).contains("나무 베기"));
        assert!(visible(&lines[2]).contains(".."), "긴 이름은 잘림");
        assert!(visible(&lines[3]).contains("완료"));
        // 재화는 왼쪽, 퀘스트는 오른쪽
        let anchors: Vec<Anchor> = walk(&lines[0])
            .iter()
            .filter_map(|(_, t, c)| t.filter(|_| art::space_advance(*c).is_none()))
            .map(|t| t.anchor)
            .collect();
        assert!(anchors.contains(&Anchor::Left) && anchors.contains(&Anchor::Right));

        let none = top(None, None);
        assert!(none.is_empty());
        let empty = top(None, Some(&[]));
        assert_eq!(empty.len(), 1);
        assert!(visible(&empty[0]).contains("받기"));

        let s = crate::plugins::rpg::Stats {
            level: 12,
            attack: 186.0,
            magic: 42.0,
            defense: 64.0,
            max_health: 455.0,
            max_mana: 100.0,
            crit_chance: 18.0,
            crit_damage: 175.0,
            speed: 100.0,
        };
        let b = bottom(&Bottom {
            stats: Some(&s),
            health: Some((18.0, 20.0)),
            mana: Some((42.0, 100.0)),
        });
        assert_eq!(total_advance(&b), 0);
        let shown = visible(&b);
        assert!(
            shown.contains("18/20") && shown.contains("42/100"),
            "{shown}"
        );
    }

    #[test]
    fn numbers_read_well() {
        assert_eq!(number(0), "0");
        assert_eq!(number(1234), "1,234");
        assert_eq!(number(1_234_567), "1,234,567");
        assert_eq!(number(-5000), "-5,000");
        assert_eq!(number(999_999_999), "999,999,999");
        assert_eq!(number(12_345_678_901), "12.3B");
    }
}
