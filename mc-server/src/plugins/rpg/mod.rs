//! RPG 스탯: 서버가 전투에 실제로 쓰는 값 (HUD 오른쪽 아래, 길드 전투력, 채팅 소개).
//! 단위는 바닐라의 5배 ([`crate::plugins::equip::SCALE`]): 맨손 체력 100, 좀비 한 대 15.
//!
//! | 스탯 | 어디서 | 쓰는 곳 |
//! |---|---|---|
//! | 레벨 | 직업(SharedData) 가운데 가장 높은 레벨 | 체력·마력 기본값 |
//! | 공격력 | 장비 무기, 아니면 든 것의 바닐라 피해 ×5 (+ 길드 신호기 힘) + 장비·세트·잠재 | 근접 피해 = 공격력 ÷ 5 |
//! | 마법력 | 지팡이·세트·잠재 | 아이템 능력 피해 × (1 + 마법력/100) |
//! | 방어력 | 장비(강화·세트·잠재) + 바닐라 갑옷 점수 ×5 | 받는 피해 × 100/(100+방어력) |
//! | 체력 | 100 + (레벨-1)×5 + 장비 | 바닐라 체력 20을 이 값으로 나눠 씀 (HUD 체력바) |
//! | 마력 | 100 + (레벨-1)×5 + 장비, 초당 2 + 최대/50 회복 | 아이템 능력 (`mana_cost`) |
//! | 치명타 확률 · 크리티컬 피해 | 기본 5% · 150% + 악세서리·세트·잠재 | 근접 공격 |
//! | 전투력 | ((공+마)×2 + 방어×3 + 체력/5 + 마력/10) × 치명타 기댓값 + 레벨×5 | 길드 전투력 |

use crate::data::items;
use crate::plugins::equip::{self, SCALE, Stat};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub level: u32,
    pub attack: f32,
    pub magic: f32,
    pub defense: f32,
    pub max_health: f32,
    pub max_mana: f32,
    /// 0..=100
    pub crit_chance: f32,
    /// 치명타일 때 피해 % (150 = 1.5배)
    pub crit_damage: f32,
    /// 걷기 = 100.
    pub speed: f32,
}

impl Stats {
    pub fn combat_power(&self) -> i64 {
        let crit = 1.0 + self.crit_chance / 100.0 * (self.crit_damage - 100.0).max(0.0) / 100.0;
        (((self.attack + self.magic) * 2.0
            + self.defense * 3.0
            + self.max_health / 5.0
            + self.max_mana / 10.0)
            * crit) as i64
            + self.level as i64 * 5
    }
}

/// 갑옷 한 벌의 바닐라 방어 점수. `slot`: 0 머리 1 몸 2 다리 3 발.
pub fn armor_points(item: i32) -> f32 {
    let name = items::name(item);
    let Some((material, piece)) = name.rsplit_once('_') else {
        return 0.0;
    };
    let slot = match piece {
        "helmet" => 0,
        "chestplate" => 1,
        "leggings" => 2,
        "boots" => 3,
        _ => return 0.0,
    };
    let table: [f32; 4] = match material {
        "leather" => [1.0, 3.0, 2.0, 1.0],
        "chainmail" | "golden" => [2.0, 5.0, 3.0 + (material == "chainmail") as u8 as f32, 1.0],
        "iron" => [2.0, 6.0, 5.0, 2.0],
        "copper" => [2.0, 4.0, 3.0, 1.0],
        "diamond" | "netherite" => [3.0, 8.0, 6.0, 3.0],
        "turtle" => [2.0, 0.0, 0.0, 0.0],
        _ => return 0.0,
    };
    table[slot]
}

pub struct Input {
    pub job_levels: Vec<u32>,
    /// 바닐라 단위 근접 피해 (장비 무기를 들었으면 길드 보너스만)
    pub held_damage: f32,
    /// 머리·몸·다리·발의 바닐라 갑옷 (장비는 빼고)
    pub armor: [Option<i32>; 4],
    /// 장비·세트·잠재능력 합 ([`crate::plugins::equip`])
    pub gear: equip::Stats,
    pub sprinting: bool,
    pub sneaking: bool,
}

pub fn compute(i: &Input) -> Stats {
    let level = i.job_levels.iter().copied().max().unwrap_or(0).max(1);
    let mut base = equip::Stats::default();
    base[Stat::Attack] = (i.held_damage * SCALE) as f64;
    base[Stat::Hp] = 100.0 + 5.0 * (level - 1) as f64;
    base[Stat::Mp] = max_mana(level) as f64;
    base[Stat::Defense] = (i
        .armor
        .iter()
        .flatten()
        .map(|&a| armor_points(a))
        .sum::<f32>()
        * SCALE) as f64;
    base[Stat::CritChance] = 5.0;
    let f = equip::finalize(&base, &i.gear);
    Stats {
        level,
        attack: f.attack as f32,
        magic: f.magic as f32,
        defense: f.defense as f32,
        max_health: f.max_hp as f32,
        max_mana: f.max_mp as f32,
        crit_chance: f.crit_chance as f32,
        crit_damage: f.crit_damage as f32,
        speed: if i.sneaking {
            30.0
        } else if i.sprinting {
            130.0
        } else {
            100.0
        },
    }
}

pub fn max_mana(level: u32) -> f32 {
    100.0 + 5.0 * level.saturating_sub(1) as f32
}

/// 지금 마나. 접속할 때 가득 찬 채로 시작합니다 (저장하지 않음).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mana {
    pub current: f32,
    pub max: f32,
}

impl Default for Mana {
    fn default() -> Self {
        Self {
            current: max_mana(1),
            max: max_mana(1),
        }
    }
}

impl Mana {
    /// 초당 회복량.
    pub fn regen_rate(&self) -> f32 {
        2.0 + self.max / 50.0
    }

    pub fn tick(&mut self, secs: f32, max: f32) {
        self.max = max;
        self.current = (self.current + self.regen_rate() * secs).min(self.max);
    }

    /// 충분하면 쓰고 true.
    pub fn spend(&mut self, cost: f32) -> bool {
        if self.current + f32::EPSILON < cost {
            return false;
        }
        self.current = (self.current - cost).max(0.0);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armor_and_power() {
        let id = |n| items::by_name(n).unwrap();
        assert_eq!(armor_points(id("diamond_chestplate")), 8.0);
        assert_eq!(armor_points(id("chainmail_leggings")), 4.0);
        assert_eq!(armor_points(id("golden_leggings")), 3.0);
        assert_eq!(armor_points(id("stone")), 0.0);
        let s = compute(&Input {
            job_levels: vec![3, 7],
            held_damage: 7.0,
            armor: [
                Some(id("iron_helmet")),
                Some(id("iron_chestplate")),
                None,
                None,
            ],
            gear: equip::Stats::default(),
            sprinting: true,
            sneaking: false,
        });
        assert_eq!((s.level, s.defense, s.speed), (7, 40.0, 130.0));
        assert_eq!((s.attack, s.max_health, s.max_mana), (35.0, 130.0, 130.0));
        assert_eq!((s.crit_chance, s.crit_damage), (5.0, 150.0));
        // ((35 + 0)×2 + 40×3 + 130/5 + 130/10) × 1.025 + 35
        assert_eq!(
            s.combat_power(),
            ((70.0 + 120.0 + 26.0 + 13.0) * 1.025f32) as i64 + 35
        );
        // 장비가 더해짐
        let g = compute(&Input {
            job_levels: vec![1],
            held_damage: 0.0,
            armor: [None; 4],
            gear: equip::Stats::of(&[(Stat::Attack, 42.0), (Stat::HpPct, 10.0)]),
            sprinting: false,
            sneaking: false,
        });
        assert_eq!((g.attack, g.max_health), (42.0, 110.0));
    }

    #[test]
    fn mana_spends_and_regenerates() {
        let mut m = Mana::default();
        assert!(m.spend(80.0));
        assert!(!m.spend(30.0));
        assert_eq!(m.current, 20.0);
        m.tick(1.0, 100.0);
        assert_eq!(m.current, 24.0);
        m.tick(100.0, 100.0);
        assert_eq!(m.current, 100.0);
    }
}
