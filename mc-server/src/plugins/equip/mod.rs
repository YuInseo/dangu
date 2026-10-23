//! 장비 (RPG 기획 "장비 아이템"): 세트 장비 · +30 강화 · 악세서리 잠재능력 · 광물 · 대장간 · 보스.
//!
//! | 기획 | 구현 |
//! |---|---|
//! | 제작 장비: 모자·상의·하의·신발, 무기, 반지·목걸이·귀걸이 | `/대장간` 창에서 광물 주괴로 제작. 높은 단계는 직업 "대장장이" 레벨이 필요 |
//! | 보스 장비: 고티 재료(마석) → 대장장이가 제작, 반지·목걸이·귀걸이(제작보다 좋음) | 보스가 마석·강화석·보스 악세서리·주문서를 떨굼. 마석으로 최상위 "아르카나" 세트 |
//! | 갑옷·무기: 공격력 or 마법력, HP or MP, 방어력 = 세트 효과 | 검(공격력)·지팡이(마법력·MP), 모자·신발(방어력·MP), 상의·하의(방어력·HP). 같은 세트 2·3·5개 효과 |
//! | 악세: 치확·치피 = 잠재력 효과 | 반지(치명타 확률) · 목걸이(크리티컬 피해) · 귀걸이(둘 다) + 잠재능력 줄 |
//! | 최대 30강, 할수록 확률이 낮아짐. 악세는 강화 없음 | `/강화` 창: +1 95% … +30 1%. 단계마다 기본 능력치 +5%, +10·+20·+30에서 +10% 더 |
//! | 잠재력은 악세마다 바를 수 있는 횟수가 정해짐 | 제작 3번, 보스 5번, 레이드 7번. 다 쓰면 그 악세는 버리고 새로 |
//! | 주문서 하급(상점, 1~2줄)·중급(던전, 2~3줄)·상급(레이드에서만 50%로 1개, 3~4줄) | 하급은 대장간 상점, 중급은 보스, 상급은 레이드 보스 "심연 군주" |
//! | 아이템을 장비에 drop하면 옵션이 붙음 | 가방에서 주문서를 집어 악세 위에 놓으면(클릭) 발림 |
//! | 잠재: 힘·민첩·지능, 체력·마력·공격력·방어력(깡/퍼), 치명타 확률·크리티컬 피해 | 그대로 (값 범위는 주문서 등급마다) |
//! | 광물: 미스릴·오리하르콘·세라피움(오버월드), 코발트·소울 크리스탈·이블리움(지옥) | 월드 생성에 광석 블록으로, 캐면 원석 → 대장간에서 주괴로 |
//! | 마석 = 상급 | 보스 재료 (아이템 등급 "상급") |
//!
//! 장비 한 개 한 개는 일련번호([`crate::packets::world::Item::serial`])로 구별하고, 강화 단계·잠재능력은
//! [`store`]에 (모든 인스턴스가 같은 파일). 이름·설명은 그 번호로 그때그때 만듭니다 ([`look`]).
//! 내용(장비·세트·광물·조합법·보스·확률)은 [`config`] (`world/equipment.json`, 웹 관리 화면).
//!
//! 능력치 단위: 바닐라 피해·체력의 5배 ([`SCALE`]). 맨손 체력 100, 좀비 한 대 15.

pub mod art;
pub mod config;
pub mod forge;
pub mod forge_art;
pub mod gui;
pub mod look;
mod session;
pub mod store;

use std::ops::{Index, IndexMut};

pub use config::{EquipConfig, GearDef};
pub use store::{Gear, gears};

/// 서버를 켤 때 (콘텐츠를 읽기 전에): 설정·장비 파일.
pub fn open_global() {
    config::open_global();
    store::open_global();
}

/// 콘텐츠를 (다시) 읽은 뒤: 겉모습·보스 훅과 광석 블록 상태.
pub fn after_contents() {
    session::install();
    session::install_ores();
}

/// RPG 능력치 = 바닐라 수치 × SCALE.
pub const SCALE: f32 = 5.0;
pub const MAX_ENHANCE: u8 = 30;

/// 장비 부위.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    Helmet,
    Chest,
    Legs,
    Boots,
    Sword,
    Staff,
    Ring,
    Necklace,
    Earring,
}

impl Part {
    pub const ALL: [Part; 9] = [
        Part::Helmet,
        Part::Chest,
        Part::Legs,
        Part::Boots,
        Part::Sword,
        Part::Staff,
        Part::Ring,
        Part::Necklace,
        Part::Earring,
    ];
    pub const ACCESSORIES: [Part; 3] = [Part::Ring, Part::Necklace, Part::Earring];

    pub fn key(self) -> &'static str {
        match self {
            Part::Helmet => "helmet",
            Part::Chest => "chest",
            Part::Legs => "legs",
            Part::Boots => "boots",
            Part::Sword => "sword",
            Part::Staff => "staff",
            Part::Ring => "ring",
            Part::Necklace => "necklace",
            Part::Earring => "earring",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Part::Helmet => "모자",
            Part::Chest => "상의",
            Part::Legs => "하의",
            Part::Boots => "신발",
            Part::Sword => "검",
            Part::Staff => "지팡이",
            Part::Ring => "반지",
            Part::Necklace => "목걸이",
            Part::Earring => "귀걸이",
        }
    }

    pub fn parse(s: &str) -> Option<Part> {
        Part::ALL
            .into_iter()
            .find(|p| p.key() == s || p.name() == s)
    }

    pub fn is_accessory(self) -> bool {
        Part::ACCESSORIES.contains(&self)
    }

    pub fn is_weapon(self) -> bool {
        matches!(self, Part::Sword | Part::Staff)
    }

    /// 갑옷 칸 (0 머리 · 1 몸 · 2 다리 · 3 발).
    pub fn armor_index(self) -> Option<usize> {
        match self {
            Part::Helmet => Some(0),
            Part::Chest => Some(1),
            Part::Legs => Some(2),
            Part::Boots => Some(3),
            _ => None,
        }
    }

    /// 악세서리 칸 (0 반지 · 1 목걸이 · 2 귀걸이).
    pub fn accessory_index(self) -> Option<usize> {
        Part::ACCESSORIES.iter().position(|p| *p == self)
    }
}

/// 능력치 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stat {
    Attack,
    Magic,
    Hp,
    Mp,
    Defense,
    CritChance,
    CritDamage,
    Str,
    Dex,
    Int,
    AttackPct,
    MagicPct,
    HpPct,
    MpPct,
    DefensePct,
}

impl Stat {
    pub const ALL: [Stat; 15] = [
        Stat::Attack,
        Stat::Magic,
        Stat::Hp,
        Stat::Mp,
        Stat::Defense,
        Stat::CritChance,
        Stat::CritDamage,
        Stat::Str,
        Stat::Dex,
        Stat::Int,
        Stat::AttackPct,
        Stat::MagicPct,
        Stat::HpPct,
        Stat::MpPct,
        Stat::DefensePct,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Stat::Attack => "attack",
            Stat::Magic => "magic",
            Stat::Hp => "hp",
            Stat::Mp => "mp",
            Stat::Defense => "defense",
            Stat::CritChance => "crit_chance",
            Stat::CritDamage => "crit_damage",
            Stat::Str => "str",
            Stat::Dex => "dex",
            Stat::Int => "int",
            Stat::AttackPct => "attack_pct",
            Stat::MagicPct => "magic_pct",
            Stat::HpPct => "hp_pct",
            Stat::MpPct => "mp_pct",
            Stat::DefensePct => "defense_pct",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Stat::Attack | Stat::AttackPct => "공격력",
            Stat::Magic | Stat::MagicPct => "마법력",
            Stat::Hp | Stat::HpPct => "체력",
            Stat::Mp | Stat::MpPct => "마력",
            Stat::Defense | Stat::DefensePct => "방어력",
            Stat::CritChance => "치명타 확률",
            Stat::CritDamage => "크리티컬 피해",
            Stat::Str => "힘",
            Stat::Dex => "민첩",
            Stat::Int => "지능",
        }
    }

    /// 값 뒤에 %를 붙이나.
    pub fn is_percent(self) -> bool {
        matches!(
            self,
            Stat::CritChance
                | Stat::CritDamage
                | Stat::AttackPct
                | Stat::MagicPct
                | Stat::HpPct
                | Stat::MpPct
                | Stat::DefensePct
        )
    }

    /// 기본 능력치 (강화로 오르는 것).
    pub fn is_main(self) -> bool {
        matches!(
            self,
            Stat::Attack | Stat::Magic | Stat::Hp | Stat::Mp | Stat::Defense
        )
    }

    pub fn parse(s: &str) -> Option<Stat> {
        Stat::ALL.into_iter().find(|x| x.key() == s)
    }

    /// `공격력 +12`, `체력 +5%`.
    pub fn show(self, v: f64) -> String {
        let n = if (v - v.round()).abs() < 0.05 {
            format!("{v:+.0}")
        } else {
            format!("{v:+.1}")
        };
        format!(
            "{} {n}{}",
            self.name(),
            if self.is_percent() { "%" } else { "" }
        )
    }
}

/// 능력치 묶음.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stats(pub [f64; 15]);

impl Index<Stat> for Stats {
    type Output = f64;
    fn index(&self, s: Stat) -> &f64 {
        &self.0[s as usize]
    }
}

impl IndexMut<Stat> for Stats {
    fn index_mut(&mut self, s: Stat) -> &mut f64 {
        &mut self.0[s as usize]
    }
}

impl Stats {
    pub fn of(list: &[(Stat, f64)]) -> Stats {
        let mut s = Stats::default();
        for &(k, v) in list {
            s[k] += v;
        }
        s
    }

    pub fn add(&mut self, other: &Stats) {
        for i in 0..self.0.len() {
            self.0[i] += other.0[i];
        }
    }

    /// 0이 아닌 것들.
    pub fn entries(&self) -> Vec<(Stat, f64)> {
        Stat::ALL
            .into_iter()
            .filter(|s| self[*s].abs() > 1e-9)
            .map(|s| (s, self[s]))
            .collect()
    }

    /// 기본 능력치만 `mult`배.
    pub fn enhanced(&self, mult: f64) -> Stats {
        let mut out = *self;
        for s in Stat::ALL.into_iter().filter(|s| s.is_main()) {
            out[s] *= mult;
        }
        out
    }
}

/// 강화 단계의 기본 능력치 배율: 단계마다 `per_level`, +10·+20·+30에서 `milestone` 더.
pub fn enhance_multiplier(level: u8, per_level: f64, milestone: f64) -> f64 {
    1.0 + level as f64 * per_level + (level / 10) as f64 * milestone
}

/// 이 장비 하나가 주는 능력치 (강화 + 잠재).
pub fn gear_stats(def: &GearDef, gear: Option<&Gear>, cfg: &EquipConfig) -> Stats {
    let level = gear.map_or(0, |g| g.enhance);
    let mut s = def.stats.enhanced(enhance_multiplier(
        level,
        cfg.enhance.per_level,
        cfg.enhance.milestone,
    ));
    if let Some(g) = gear {
        for &(k, v) in &g.potential {
            s[k] += v;
        }
    }
    s
}

/// 세트마다 입은 개수 → 켜진 세트 효과.
pub fn set_bonuses(worn_sets: &[&str], cfg: &EquipConfig) -> (Stats, Vec<(String, u8)>) {
    let mut out = Stats::default();
    let mut counts: Vec<(String, u8)> = Vec::new();
    for s in worn_sets {
        match counts.iter_mut().find(|(k, _)| k == s) {
            Some((_, n)) => *n += 1,
            None => counts.push(((*s).to_owned(), 1)),
        }
    }
    for (id, n) in &counts {
        if let Some(set) = cfg.set(id) {
            for (need, bonus) in &set.bonuses {
                if n >= need {
                    out.add(bonus);
                }
            }
        }
    }
    (out, counts)
}

/// 최종 능력치.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Final {
    pub attack: f64,
    pub magic: f64,
    pub max_hp: f64,
    pub max_mp: f64,
    pub defense: f64,
    /// 0..=100
    pub crit_chance: f64,
    /// 치명타 때 피해 % (기본 150)
    pub crit_damage: f64,
}

/// 기본(레벨·맨손) + 장비 → 최종. 힘 1 = 공격력 1%·체력 0.5%, 민첩 1 = 치명타 확률 0.2%p·크리티컬
/// 피해 0.5%, 지능 1 = 마법력 1%·마력 0.5%.
pub fn finalize(base: &Stats, bonus: &Stats) -> Final {
    let mut t = *base;
    t.add(bonus);
    let pct = |p: f64| 1.0 + p / 100.0;
    Final {
        attack: t[Stat::Attack] * pct(t[Stat::AttackPct] + t[Stat::Str]),
        magic: t[Stat::Magic] * pct(t[Stat::MagicPct] + t[Stat::Int]),
        max_hp: (t[Stat::Hp] * pct(t[Stat::HpPct] + t[Stat::Str] * 0.5)).max(1.0),
        max_mp: t[Stat::Mp] * pct(t[Stat::MpPct] + t[Stat::Int] * 0.5),
        defense: t[Stat::Defense] * pct(t[Stat::DefensePct]),
        crit_chance: (t[Stat::CritChance] + t[Stat::Dex] * 0.2).clamp(0.0, 100.0),
        crit_damage: 150.0 + t[Stat::CritDamage] + t[Stat::Dex] * 0.5,
    }
}

/// 방어력만큼 줄인 피해: `피해 × 100 / (100 + 방어력)`.
pub fn after_defense(damage: f64, defense: f64) -> f64 {
    damage * 100.0 / (100.0 + defense.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_math() {
        assert_eq!(enhance_multiplier(0, 0.05, 0.1), 1.0);
        assert!((enhance_multiplier(10, 0.05, 0.1) - 1.6).abs() < 1e-9);
        assert!((enhance_multiplier(30, 0.05, 0.1) - 2.8).abs() < 1e-9);
        let base = Stats::of(&[
            (Stat::Attack, 100.0),
            (Stat::Hp, 100.0),
            (Stat::CritChance, 5.0),
        ]);
        let bonus = Stats::of(&[
            (Stat::AttackPct, 10.0),
            (Stat::Str, 10.0),
            (Stat::Dex, 10.0),
        ]);
        let f = finalize(&base, &bonus);
        assert!((f.attack - 120.0).abs() < 1e-9);
        assert!((f.max_hp - 105.0).abs() < 1e-9);
        assert!((f.crit_chance - 7.0).abs() < 1e-9);
        assert!((f.crit_damage - 155.0).abs() < 1e-9);
        assert_eq!(after_defense(100.0, 100.0), 50.0);
        assert_eq!(Stat::HpPct.show(5.0), "체력 +5%");
        assert_eq!(Stat::Attack.show(12.0), "공격력 +12");
        let e = Stats::of(&[(Stat::Attack, 10.0), (Stat::CritChance, 3.0)]).enhanced(2.0);
        assert_eq!((e[Stat::Attack], e[Stat::CritChance]), (20.0, 3.0));
    }
}
