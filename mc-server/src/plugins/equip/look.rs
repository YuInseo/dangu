//! 장비의 이름과 설명: 일련번호마다 강화 단계·잠재능력을 넣어 그때그때 만듭니다.
//!
//! ```text
//! +12 미스릴 투구                      (등급 색)
//! ■■■■□□□□□□  강화 +12 / 30
//! 모자 · 고급 · 미스릴 세트
//!
//! 방어력 +8 (+4.8)                     강화로 오른 만큼 초록
//! 마력 +24 (+9)
//!
//! ◆ 미스릴 세트
//!   2개  방어력 +5%
//!   3개  체력 +5%
//!   5개  공격력 +5% · 마법력 +5%
//! ```
//! 악세서리는 강화 대신 잠재능력 줄과 남은 횟수.

use super::config::{EquipConfig, GearDef, grade_color};
use super::store::{Gear, gears, is_gear_serial};
use super::{MAX_ENHANCE, Stat, enhance_multiplier};
use crate::game::custom_items::Look;
use crate::protocol::nbt::Tag;

/// 색 입힌 조각 (기울임 없음).
fn span(text: &str, color: &str) -> Tag {
    Tag::Compound(vec![
        ("text".into(), Tag::String(text.to_owned())),
        ("color".into(), Tag::String(color.to_owned())),
        ("italic".into(), Tag::Byte(0)),
    ])
}

fn line(parts: Vec<Tag>) -> Tag {
    Tag::Compound(vec![
        ("text".into(), Tag::String(String::new())),
        ("italic".into(), Tag::Byte(0)),
        ("extra".into(), Tag::List(parts)),
    ])
}

fn blank() -> Tag {
    span(" ", "gray")
}

/// 강화 막대 (10칸, 3단계에 한 칸).
pub fn enhance_bar(level: u8) -> String {
    let filled = (level as usize).div_ceil(3).min(10);
    format!("{}{}", "■".repeat(filled), "□".repeat(10 - filled))
}

/// 설정에 적힌 그대로의 설명 (아직 일련번호가 없을 때, 콘텐츠 팩에 들어감).
pub fn base_lore(g: &GearDef, cfg: &EquipConfig) -> Vec<String> {
    let mut v = vec![format!("{} · {}", g.part.name(), g.grade)];
    v.extend(g.stats.entries().into_iter().map(|(k, x)| k.show(x)));
    if let Some(set) = g.set.as_deref().and_then(|s| cfg.set(s)) {
        v.push(format!("{} 세트", set.name));
    }
    if g.part.is_accessory() {
        v.push(format!(
            "잠재능력 없음 · {}번 바를 수 있음",
            g.potential_tries
        ));
    }
    v.push(g.lore.clone());
    v
}

/// 일련번호 하나의 겉모습.
pub fn look_of(serial: u32) -> Option<Look> {
    if !is_gear_serial(serial) {
        return None;
    }
    let gear = gears().read(|d| d.gears.get(&serial).cloned())?;
    super::config::with(|cfg| {
        let def = cfg.gear_def(&gear.def)?;
        Some(build(def, &gear, cfg))
    })
}

pub fn build(def: &GearDef, gear: &Gear, cfg: &EquipConfig) -> Look {
    let color = grade_color(&def.grade);
    let name = if gear.enhance > 0 {
        line(vec![
            span(&format!("+{} ", gear.enhance), "#ffd866"),
            span(&def.name, color),
        ])
    } else {
        span(&def.name, color)
    };
    let mut lore = Vec::new();
    if !def.part.is_accessory() {
        lore.push(line(vec![
            span(&enhance_bar(gear.enhance), "#ffd866"),
            span(
                &format!("  강화 +{} / {MAX_ENHANCE}", gear.enhance),
                "#c8c8d2",
            ),
        ]));
    }
    let mut kind = vec![
        span(def.part.name(), "#c8c8d2"),
        span(" · ", "dark_gray"),
        span(&def.grade, color),
    ];
    let set = def.set.as_deref().and_then(|s| cfg.set(s));
    if let Some(set) = set {
        kind.push(span(" · ", "dark_gray"));
        kind.push(span(&format!("{} 세트", set.name), &set.color));
    }
    lore.push(line(kind));
    lore.push(blank());
    // 기본 능력치 (강화로 오른 만큼)
    let mult = enhance_multiplier(gear.enhance, cfg.enhance.per_level, cfg.enhance.milestone);
    for (k, base) in def.stats.entries() {
        let total = if k.is_main() { base * mult } else { base };
        let mut parts = vec![span(&k.show(total), "white")];
        if (total - base).abs() > 0.05 {
            parts.push(span(&format!(" (+{:.0})", total - base), "#7be07b"));
        }
        lore.push(line(parts));
    }
    // 잠재능력
    if def.part.is_accessory() {
        lore.push(blank());
        let grade = gear.scroll.as_deref().unwrap_or("");
        let pcolor = match grade {
            "상급" => "#ffb347",
            "중급" => "#c77dff",
            _ => "#6fe3ff",
        };
        if gear.potential.is_empty() {
            lore.push(span("잠재능력 없음", "#8c8c9a"));
            lore.push(span("주문서를 집어 이 장신구 위에 놓으세요", "dark_gray"));
        } else {
            lore.push(line(vec![
                span("✦ 잠재능력", pcolor),
                span(
                    &if grade.is_empty() {
                        String::new()
                    } else {
                        format!(" · {grade}")
                    },
                    "dark_gray",
                ),
            ]));
            for (k, v) in &gear.potential {
                lore.push(span(&format!("  {}", k.show(*v)), pcolor));
            }
        }
        lore.push(line(vec![
            span("남은 횟수 ", "#8c8c9a"),
            span(
                &format!("{} / {}", gear.tries_left, def.potential_tries),
                if gear.tries_left == 0 {
                    "#ff5a5a"
                } else {
                    "white"
                },
            ),
        ]));
    }
    // 세트 효과
    if let Some(set) = set {
        lore.push(blank());
        lore.push(span(&format!("◆ {} 세트", set.name), &set.color));
        for (need, bonus) in &set.bonuses {
            let text = bonus
                .entries()
                .into_iter()
                .map(|(k, v)| k.show(v))
                .collect::<Vec<_>>()
                .join(" · ");
            lore.push(line(vec![
                span(&format!("  {need}개  "), "#8c8c9a"),
                span(&text, "#c8c8d2"),
            ]));
        }
    }
    if !def.lore.is_empty() {
        lore.push(blank());
        lore.push(span(&def.lore, "dark_gray"));
    }
    Look {
        name: None,
        lore: Vec::new(),
        glint: Some(gear.enhance >= 10 || gear.potential.len() >= 3),
        rich: Some((name, lore)),
    }
}

/// 잠재능력 줄을 한 줄 글로 (채팅 알림).
pub fn potential_text(lines: &[(Stat, f64)]) -> String {
    lines
        .iter()
        .map(|(k, v)| k.show(*v))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_shows_enhance_set_and_potential() {
        let cfg = EquipConfig::default();
        let helm = cfg.gear_def("dangu:mithril_helmet").unwrap();
        let g = Gear {
            def: helm.id.clone(),
            enhance: 12,
            potential: Vec::new(),
            tries_left: 0,
            scroll: None,
        };
        let look = build(helm, &g, &cfg);
        let (name, lore) = look.rich.unwrap();
        let text = format!("{name:?} {lore:?}");
        assert!(text.contains("+12 ") && text.contains("미스릴 투구"));
        assert!(text.contains("강화 +12 / 30") && text.contains("미스릴 세트"));
        assert_eq!(enhance_bar(12), "■■■■□□□□□□");
        assert_eq!(enhance_bar(30), "■■■■■■■■■■");

        let ring = cfg.gear_def("dangu:goblin_king_ring").unwrap();
        let g = Gear {
            def: ring.id.clone(),
            enhance: 0,
            potential: vec![(Stat::Str, 9.0), (Stat::CritDamage, 12.0)],
            tries_left: 4,
            scroll: Some("중급".into()),
        };
        let text = format!("{:?}", build(ring, &g, &cfg).rich.unwrap().1);
        assert!(text.contains("잠재능력") && text.contains("힘 +9") && text.contains("4 / 5"));
        assert!(!text.contains("강화 +"));
        assert!(base_lore(ring, &cfg).iter().any(|l| l.contains("5번")));
    }
}
