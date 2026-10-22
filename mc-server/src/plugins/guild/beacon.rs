//! 길드 신호기: 범위 제한 없음 · 무한 지속. 접속한 길드원 모두에게 늘 걸립니다.
//!
//! - n번째 효과를 여는 값 = 신호기 n개 (처음 1개, 두 번째 2개 …). 기획의 "추가 효과를 언락 시
//!   더 얻고 싶으면 신호기 추가 2개" 그대로.
//! - 단계 올리기 (1 → 2 → 3) = 광물 + 돈 ([`Effect::tier_cost`]).
//!
//! 클라이언트가 스스로 처리하는 효과(성급함·점프 강화·야간 투시·화염 저항·수중 호흡)는 효과
//! 패킷만으로 되고, 신속은 이동 속도 속성도 보내며, 힘·저항·재생은 서버가 피해·회복에 반영합니다.

use crate::packets::world::Item;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Speed,
    Haste,
    Strength,
    Resistance,
    Regeneration,
    JumpBoost,
    NightVision,
    FireResistance,
    WaterBreathing,
}

/// 길드가 연 효과와 단계 (1부터).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owned {
    pub effect: Effect,
    pub tier: u8,
}

impl Effect {
    pub const ALL: [Effect; 9] = [
        Effect::Speed,
        Effect::Haste,
        Effect::Strength,
        Effect::Resistance,
        Effect::Regeneration,
        Effect::JumpBoost,
        Effect::NightVision,
        Effect::FireResistance,
        Effect::WaterBreathing,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Effect::Speed => "신속",
            Effect::Haste => "성급함",
            Effect::Strength => "힘",
            Effect::Resistance => "저항",
            Effect::Regeneration => "재생",
            Effect::JumpBoost => "점프 강화",
            Effect::NightVision => "야간 투시",
            Effect::FireResistance => "화염 저항",
            Effect::WaterBreathing => "수중 호흡",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Effect::Speed => "speed",
            Effect::Haste => "haste",
            Effect::Strength => "strength",
            Effect::Resistance => "resistance",
            Effect::Regeneration => "regeneration",
            Effect::JumpBoost => "jump_boost",
            Effect::NightVision => "night_vision",
            Effect::FireResistance => "fire_resistance",
            Effect::WaterBreathing => "water_breathing",
        }
    }

    pub fn from_key(k: &str) -> Option<Effect> {
        Effect::ALL.into_iter().find(|e| e.key() == k)
    }

    /// `mob_effect` 레지스트리 번호 (바닐라 등록 순서).
    pub fn network_id(self) -> i32 {
        match self {
            Effect::Speed => 0,
            Effect::Haste => 2,
            Effect::Strength => 4,
            Effect::JumpBoost => 7,
            Effect::Regeneration => 9,
            Effect::Resistance => 10,
            Effect::FireResistance => 11,
            Effect::WaterBreathing => 12,
            Effect::NightVision => 15,
        }
    }

    pub fn max_tier(self) -> u8 {
        match self {
            Effect::NightVision | Effect::FireResistance | Effect::WaterBreathing => 1,
            Effect::Resistance | Effect::Regeneration => 2,
            _ => 3,
        }
    }

    /// 창에 보일 아이템.
    pub fn icon(self) -> &'static str {
        match self {
            Effect::Speed => "sugar",
            Effect::Haste => "golden_pickaxe",
            Effect::Strength => "blaze_powder",
            Effect::Resistance => "iron_chestplate",
            Effect::Regeneration => "ghast_tear",
            Effect::JumpBoost => "rabbit_foot",
            Effect::NightVision => "golden_carrot",
            Effect::FireResistance => "magma_cream",
            Effect::WaterBreathing => "pufferfish",
        }
    }

    /// `tier`에서 한 단계 올리는 값: 돈과 광물 (아이템 이름, 개수).
    pub fn tier_cost(self, tier: u8) -> (i64, Vec<(&'static str, i32)>) {
        let t = tier as i32;
        (
            100_000 * t as i64,
            vec![
                ("iron_ingot", 64 * t),
                ("gold_ingot", 32 * t),
                ("diamond", 16 * t),
                ("emerald", 8 * t),
            ],
        )
    }
}

/// 다음(n+1번째) 효과를 여는 데 드는 신호기 수.
pub fn unlock_cost(owned: usize) -> i32 {
    owned as i32 + 1
}

/// 효과가 서버 쪽 계산에 주는 값.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bonus {
    /// 때리는 피해에 더함 (힘: 단계마다 +3).
    pub damage: f32,
    /// 받는 피해를 줄이는 비율 (저항: 단계마다 20%).
    pub resist: f32,
    /// 초당 회복 (재생: 1단계 0.4, 2단계 0.8).
    pub regen: f32,
    /// 이동 속도 배율 (신속: 단계마다 +20%).
    pub speed: f32,
    pub fire_immune: bool,
    pub water_breathing: bool,
}

pub fn bonus(owned: &[Owned]) -> Bonus {
    let mut b = Bonus {
        speed: 1.0,
        ..Bonus::default()
    };
    for o in owned {
        let t = o.tier as f32;
        match o.effect {
            Effect::Strength => b.damage += 3.0 * t,
            Effect::Resistance => b.resist = (0.2 * t).min(0.8),
            Effect::Regeneration => b.regen += 0.4 * t,
            Effect::Speed => b.speed += 0.2 * t,
            Effect::FireResistance => b.fire_immune = true,
            Effect::WaterBreathing => b.water_breathing = true,
            _ => {}
        }
    }
    b
}

/// 인벤토리(9..45)에서 이 아이템이 몇 개 있나.
pub fn count_in(inv: &[Option<Item>], name: &str) -> i32 {
    let Some(id) = crate::data::items::by_name(name) else {
        return 0;
    };
    inv.iter()
        .flatten()
        .filter(|i| i.id == id && i.custom.is_none())
        .map(|i| i.count)
        .sum()
}

/// 인벤토리에서 꺼냅니다 (모자라면 아무것도 안 하고 false). 바뀐 칸 번호들.
pub fn take_from(
    inv: &mut [Option<Item>],
    start: usize,
    wants: &[(&str, i32)],
) -> Option<Vec<usize>> {
    if wants.iter().any(|(n, c)| count_in(&inv[start..], n) < *c) {
        return None;
    }
    let mut changed = Vec::new();
    for (name, count) in wants {
        let id = crate::data::items::by_name(name)?;
        let mut left = *count;
        for (i, slot) in inv.iter_mut().enumerate().skip(start) {
            if left == 0 {
                break;
            }
            if let Some(it) = slot
                && it.id == id
                && it.custom.is_none()
            {
                let n = it.count.min(left);
                it.count -= n;
                left -= n;
                if it.count == 0 {
                    *slot = None;
                }
                changed.push(i);
            }
        }
    }
    Some(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn costs_and_bonuses() {
        assert_eq!(unlock_cost(0), 1);
        assert_eq!(unlock_cost(1), 2);
        let (money, ores) = Effect::Speed.tier_cost(2);
        assert_eq!(money, 200_000);
        assert_eq!(ores[2], ("diamond", 32));
        let b = bonus(&[
            Owned {
                effect: Effect::Strength,
                tier: 2,
            },
            Owned {
                effect: Effect::Speed,
                tier: 1,
            },
        ]);
        assert_eq!((b.damage, b.speed), (6.0, 1.2));
        for e in Effect::ALL {
            assert_eq!(Effect::from_key(e.key()), Some(e));
            assert!(
                crate::data::items::by_name(e.icon()).is_some(),
                "{}",
                e.icon()
            );
        }
    }

    #[test]
    fn taking_items_is_all_or_nothing() {
        let d = crate::data::items::by_name("diamond").unwrap();
        let mut inv = vec![None; 45];
        inv[10] = Some(Item::new(d, 10));
        inv[20] = Some(Item::new(d, 10));
        assert!(take_from(&mut inv, 9, &[("diamond", 30)]).is_none());
        assert_eq!(count_in(&inv, "diamond"), 20);
        let changed = take_from(&mut inv, 9, &[("diamond", 15)]).unwrap();
        assert_eq!(changed, vec![10, 20]);
        assert_eq!(count_in(&inv, "diamond"), 5);
    }
}

/// 신호기 일 하나에 드는 것.
#[derive(Debug, Clone, PartialEq)]
pub struct Cost {
    pub beacons: i32,
    pub money: i64,
    pub items: Vec<(&'static str, i32)>,
}

impl super::Data {
    /// 열기(`unlock`) 또는 단계 올리기에 드는 것 (권한·상태 확인 포함).
    pub fn beacon_plan(&self, actor: u128, effect: Effect, unlock: bool) -> Result<Cost, String> {
        let g = self.of(actor).ok_or("길드에 들어가 있지 않습니다.")?;
        if !g.can(actor, super::Perm::Upgrade) {
            return Err(format!("권한이 없습니다: {}", super::Perm::Upgrade.name()));
        }
        let owned = g.beacon.iter().find(|o| o.effect == effect);
        match (unlock, owned) {
            (true, Some(_)) => Err("이미 연 효과입니다.".into()),
            (true, None) if g.beacon.len() >= g.level_row().beacon_slots => Err(format!(
                "길드 레벨 {}에서는 신호기 효과를 {}개까지 열 수 있습니다.",
                g.level(),
                g.level_row().beacon_slots
            )),
            (true, None) => Ok(Cost {
                beacons: unlock_cost(g.beacon.len()),
                money: 0,
                items: Vec::new(),
            }),
            (false, None) => Err("먼저 효과를 여세요.".into()),
            (false, Some(o)) if o.tier >= effect.max_tier() => Err("이미 끝 단계입니다.".into()),
            (false, Some(o)) => {
                let (money, items) = effect.tier_cost(o.tier);
                Ok(Cost {
                    beacons: 0,
                    money,
                    items,
                })
            }
        }
    }

    /// 값을 받은 뒤에: 열거나 올립니다.
    pub fn beacon_apply(&mut self, actor: u128, effect: Effect, unlock: bool) -> super::Outcome {
        self.beacon_plan(actor, effect, unlock)?;
        let g = self.of_mut(actor).unwrap();
        if unlock {
            g.beacon.push(Owned { effect, tier: 1 });
            Ok(format!("신호기 효과를 열었습니다: {} I", effect.name()))
        } else {
            let o = g.beacon.iter_mut().find(|o| o.effect == effect).unwrap();
            o.tier += 1;
            Ok(format!("{} {}단계가 되었습니다.", effect.name(), o.tier))
        }
    }
}
