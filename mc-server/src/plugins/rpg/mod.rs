//! RPG 스탯: 지금 서버가 실제로 계산하는 값만 모아 보여 줍니다 (HUD 오른쪽 아래, 길드 전투력).
//!
//! | 스탯 | 어디서 |
//! |---|---|
//! | 레벨 | 직업(SharedData) 가운데 가장 높은 레벨 |
//! | 공격력 | 손에 든 것으로 때릴 때의 피해 (사용자 정의 아이템이면 그 값) |
//! | 방어력 | 입은 갑옷의 바닐라 방어 점수 합 (아직 피해를 줄이지는 않음 — F6) |
//! | 최대 체력 | 20 (하트 10칸) |
//! | 이동 속도 | 걷기 100%, 달리기 130%, 웅크리기 30% |
//! | 마나 | 최대 100 + (레벨-1)×5, 초당 2 + 최대/50 회복. 사용자 정의 아이템 능력이 씀 (`mana_cost`) |
//! | 전투력 | 공격력×10 + 방어력×8 + 최대 체력×2 + 레벨×5 |

use crate::data::items;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub level: u32,
    pub attack: f32,
    pub defense: f32,
    pub max_health: f32,
    /// 걷기 = 100.
    pub speed: f32,
    pub max_mana: f32,
}

impl Stats {
    pub fn combat_power(&self) -> i64 {
        (self.attack * 10.0 + self.defense * 8.0 + self.max_health * 2.0) as i64
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
    pub held_damage: f32,
    /// 머리·몸·다리·발.
    pub armor: [Option<i32>; 4],
    pub sprinting: bool,
    pub sneaking: bool,
}

pub fn compute(i: &Input) -> Stats {
    Stats {
        level: i.job_levels.iter().copied().max().unwrap_or(0).max(1),
        attack: i.held_damage,
        defense: i.armor.iter().flatten().map(|&a| armor_points(a)).sum(),
        max_health: 20.0,
        max_mana: max_mana(i.job_levels.iter().copied().max().unwrap_or(1)),
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
            sprinting: true,
            sneaking: false,
        });
        assert_eq!((s.level, s.defense, s.speed), (7, 8.0, 130.0));
        assert_eq!(s.combat_power(), 70 + 64 + 40 + 35);
        assert_eq!(s.max_mana, 130.0);
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
