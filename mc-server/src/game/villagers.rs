//! 주민: 직업·레벨·거래 목록. 거래표는 바닐라 26.3 데이터([`crate::data::trades`])에서.
//!
//! 바닐라처럼 레벨마다 그 레벨 거래 가운데 둘을 골라 늘어나고, 거래로 쌓인 경험치가
//! 10 / 70 / 150 / 250을 넘으면 다음 레벨이 됩니다. 직업 블록(작업대)을 찾아 직업을 얻는
//! 대신 태어날 때 직업을 정합니다.

use super::rand_u32;
use crate::data::items;
use crate::data::trades::{TRADES, Trade, VARIANTS};
use crate::packets::world::Item;

/// 거래하는 직업들 (무직·멍청이는 거래가 없음).
pub const PROFESSIONS: [&str; 13] = [
    "armorer",
    "butcher",
    "cartographer",
    "cleric",
    "farmer",
    "fisherman",
    "fletcher",
    "leatherworker",
    "librarian",
    "mason",
    "shepherd",
    "toolsmith",
    "weaponsmith",
];

/// `villager_profession` 내장 레지스트리 순서 (등록 순: none이 맨 앞).
const PROFESSION_IDS: [&str; 15] = [
    "none",
    "armorer",
    "butcher",
    "cartographer",
    "cleric",
    "farmer",
    "fisherman",
    "fletcher",
    "leatherworker",
    "librarian",
    "mason",
    "nitwit",
    "shepherd",
    "toolsmith",
    "weaponsmith",
];

/// 다음 레벨까지 필요한 누적 경험치 (레벨 1→2, 2→3, …).
const LEVEL_XP: [u32; 4] = [10, 70, 150, 250];
/// 레벨마다 새로 생기는 거래 수.
const TRADES_PER_LEVEL: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Offer {
    pub trade: &'static Trade,
    pub uses: u32,
}

impl Offer {
    pub fn disabled(&self) -> bool {
        self.uses >= self.trade.max_uses as u32
    }

    pub fn wants(&self) -> Option<(i32, i32)> {
        let (n, c) = self.trade.wants;
        Some((items::by_name(n)?, c as i32))
    }

    pub fn extra(&self) -> Option<(i32, i32)> {
        let (n, c) = self.trade.extra?;
        Some((items::by_name(n)?, c as i32))
    }

    pub fn gives(&self) -> Option<Item> {
        let (n, c) = self.trade.gives;
        Some(Item::new(items::by_name(n)?, c as i32))
    }

    /// 넣은 두 칸으로 이 거래를 할 수 있나.
    pub fn matches(&self, a: Option<Item>, b: Option<Item>) -> bool {
        let enough = |slot: Option<Item>, want: Option<(i32, i32)>| match (slot, want) {
            (_, None) => true,
            (Some(i), Some((id, n))) => i.id == id && i.custom.is_none() && i.count >= n,
            (None, Some(_)) => false,
        };
        !self.disabled()
            && self.wants().is_some()
            && self.gives().is_some()
            && enough(a, self.wants())
            && (self.extra().is_none() || enough(b, self.extra()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Villager {
    pub profession: &'static str,
    /// [`VARIANTS`] 번호 (desert, jungle, plains, …).
    pub variant: u8,
    pub level: u8,
    pub xp: u32,
    pub offers: Vec<Offer>,
}

impl Villager {
    pub fn new(profession: &'static str, variant: &str) -> Self {
        let variant = VARIANTS.iter().position(|v| *v == variant).unwrap_or(2) as u8;
        let mut v = Self {
            profession,
            variant,
            level: 1,
            xp: 0,
            offers: Vec::new(),
        };
        v.add_offers(1);
        v
    }

    pub fn trades(&self) -> bool {
        !self.offers.is_empty()
    }

    /// 엔티티 데이터용 (종류, 직업, 레벨) 레지스트리 번호.
    pub fn data(&self) -> (i32, i32, i32) {
        let prof = PROFESSION_IDS
            .iter()
            .position(|p| *p == self.profession)
            .unwrap_or(0);
        (self.variant as i32, prof as i32, self.level as i32)
    }

    fn add_offers(&mut self, level: u8) {
        let Some((_, _, pool)) = TRADES
            .iter()
            .find(|(p, l, _)| *p == self.profession && *l == level)
        else {
            return;
        };
        let mut pool: Vec<&'static Trade> = pool
            .iter()
            .filter(|t| t.variants == 0 || t.variants & (1 << self.variant) != 0)
            .filter(|t| items::by_name(t.wants.0).is_some() && items::by_name(t.gives.0).is_some())
            .collect();
        for _ in 0..TRADES_PER_LEVEL {
            if pool.is_empty() {
                break;
            }
            let t = pool.swap_remove(rand_u32() as usize % pool.len());
            self.offers.push(Offer { trade: t, uses: 0 });
        }
    }

    /// 거래를 `times`번 했음. 레벨이 올랐으면 true.
    pub fn traded(&mut self, offer: usize, times: u32) -> bool {
        let Some(o) = self.offers.get_mut(offer) else {
            return false;
        };
        o.uses += times;
        self.xp += o.trade.xp as u32 * times;
        let mut up = false;
        while self.level < 5 && self.xp >= LEVEL_XP[self.level as usize - 1] {
            self.level += 1;
            self.add_offers(self.level);
            up = true;
        }
        up
    }

    /// 다 쓴 거래를 다시 채웁니다 (바닐라는 하루 두 번, 작업대에서).
    pub fn restock(&mut self) {
        for o in &mut self.offers {
            o.uses = 0;
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self.profession {
            "armorer" => "갑옷 제조인",
            "butcher" => "도축업자",
            "cartographer" => "지도 제작자",
            "cleric" => "성직자",
            "farmer" => "농부",
            "fisherman" => "어부",
            "fletcher" => "화살 제조인",
            "leatherworker" => "가죽 세공인",
            "librarian" => "사서",
            "mason" => "석공",
            "shepherd" => "양치기",
            "toolsmith" => "도구 대장장이",
            "weaponsmith" => "무기 대장장이",
            "nitwit" => "멍청이",
            _ => "주민",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profession_has_first_level_trades() {
        for p in PROFESSIONS {
            for variant in VARIANTS {
                let v = Villager::new(p, variant);
                assert!(!v.offers.is_empty(), "{p} ({variant}) has trades");
                for o in &v.offers {
                    assert!(o.wants().is_some() && o.gives().is_some(), "{:?}", o.trade);
                }
            }
        }
    }

    #[test]
    fn trading_levels_up_and_adds_offers() {
        let mut v = Villager::new("farmer", "plains");
        assert_eq!(v.offers.len(), 2);
        let wheat = v
            .offers
            .iter()
            .position(|o| o.trade.wants.0 != "emerald")
            .unwrap_or(0);
        let mut times = 0;
        while v.level == 1 {
            v.traded(wheat, 1);
            times += 1;
            assert!(times < 20);
        }
        assert_eq!(v.level, 2);
        assert!(v.offers.len() >= 3);
        assert_eq!(v.data().1, 5, "farmer is profession #5");
        let o = v.offers[0];
        let (id, n) = o.wants().unwrap();
        assert!(o.matches(Some(Item::new(id, n)), None) || o.disabled());
        if n > 1 && o.extra().is_none() {
            assert!(!o.matches(Some(Item::new(id, n - 1)), None));
        }
    }
}
