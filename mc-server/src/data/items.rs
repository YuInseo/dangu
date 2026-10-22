//! 아이템 성질: 최대 개수, 음식, 공격력, 도구 등급. 그리고 조합법과 블록 전리품.

use super::blocks::{self, BlockId};
use super::generated::{BLOCK_DROPS, ITEM_NAMES, ITEMS, RECIPES};
use crate::packets::world::Item;

pub fn name(item: i32) -> &'static str {
    ITEM_NAMES.get(item as usize).copied().unwrap_or("air")
}

pub fn by_name(name: &str) -> Option<i32> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    ITEM_NAMES.iter().position(|n| *n == name).map(|i| i as i32)
}

pub fn max_stack(item: i32) -> i32 {
    ITEMS.get(item as usize).map_or(64, |i| i.0 as i32)
}

/// (회복량, 포만도). 음식이 아니면 None.
pub fn food(item: i32) -> Option<(i32, f32)> {
    let (_, nutrition, saturation, _) = *ITEMS.get(item as usize)?;
    (nutrition > 0).then_some((nutrition as i32, saturation))
}

/// 이 아이템으로 때릴 때의 피해 (맨손 1).
pub fn attack_damage(item: Option<Item>) -> f32 {
    item.and_then(|i| ITEMS.get(i.id as usize))
        .map_or(1.0, |i| i.3)
}

/// 곡괭이 등급: 0 없음, 1 나무·금, 2 돌, 3 철, 4 다이아몬드, 5 네더라이트.
fn pickaxe_tier(item: Option<Item>) -> u8 {
    let Some(name) = item.map(|i| name(i.id)) else {
        return 0;
    };
    match name.strip_suffix("_pickaxe") {
        Some("wooden" | "golden") => 1,
        Some("stone") => 2,
        Some("copper") => 2,
        Some("iron") => 3,
        Some("diamond") => 4,
        Some("netherite") => 5,
        _ => 0,
    }
}

fn in_block_tag(tag: &str, block: BlockId) -> bool {
    let (_, tags) = super::TAGS.iter().find(|(r, _)| *r == "block").unwrap();
    tags.iter()
        .find(|(t, _)| *t == tag)
        .is_some_and(|(_, ids)| ids.contains(&(block as u32)))
}

/// 이 도구로 부수면 전리품이 나오는가. 곡괭이 블록은 곡괭이와 등급이 있어야 합니다.
/// (바닐라의 "올바른 도구 필요" 표시는 데이터에 없어서 `mineable/pickaxe`로 대신합니다.)
pub fn can_harvest(block: BlockId, tool: Option<Item>) -> bool {
    if !in_block_tag("mineable/pickaxe", block) {
        return true;
    }
    let tier = pickaxe_tier(tool);
    let needed = if in_block_tag("needs_diamond_tool", block) {
        4
    } else if in_block_tag("needs_iron_tool", block) {
        3
    } else if in_block_tag("needs_stone_tool", block) {
        2
    } else {
        1
    };
    tier >= needed
}

/// 블록을 부쉈을 때 떨어지는 것. `roll`은 0..1000의 난수.
pub fn block_drop(state: u32, roll: u32, count_roll: u32) -> Option<Item> {
    let block = blocks::block_of(state);
    // 두 칸짜리는 아래 반쪽(또는 침대 머리)에서만
    if blocks::property(state, "half") == Some("upper")
        || blocks::property(state, "part") == Some("head")
    {
        return None;
    }
    let (item, lo, hi, chance) = *BLOCK_DROPS.get(block)?;
    if item < 0 || roll >= chance as u32 {
        return None;
    }
    let count = lo as u32 + count_roll % (hi as u32 - lo as u32 + 1);
    (count > 0).then_some(Item::new(item as i32, count as i32))
}

/// 조합 칸(`width`×`width`, 2 또는 3)에 맞는 결과.
pub fn craft(grid: &[Option<Item>], width: usize) -> Option<Item> {
    let ids: Vec<Option<u16>> = grid
        .iter()
        .map(|s| s.filter(|i| i.count > 0).map(|i| i.id as u16))
        .collect();
    if ids.iter().all(Option::is_none) {
        return None;
    }
    // 채워진 칸을 감싸는 가장 작은 직사각형
    let filled: Vec<(usize, usize)> = (0..grid.len())
        .filter(|&i| ids[i].is_some())
        .map(|i| (i % width, i / width))
        .collect();
    let (x0, x1) = (
        filled.iter().map(|p| p.0).min()?,
        filled.iter().map(|p| p.0).max()?,
    );
    let (y0, y1) = (
        filled.iter().map(|p| p.1).min()?,
        filled.iter().map(|p| p.1).max()?,
    );
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    let cell = |x: usize, y: usize| ids[(y0 + y) * width + x0 + x];

    for &(shaped, rw, rh, cells, result, count) in RECIPES {
        let ok = if shaped {
            (rw as usize, rh as usize) == (w, h)
                && [false, true].iter().any(|&mirror| {
                    (0..h).all(|y| {
                        (0..w).all(|x| {
                            let rx = if mirror { w - 1 - x } else { x };
                            match (cells[y * w + rx], cell(x, y)) {
                                (None, None) => true,
                                (Some(ing), Some(id)) => ing.contains(&id),
                                _ => false,
                            }
                        })
                    })
                })
        } else {
            let mut items: Vec<u16> = ids.iter().flatten().copied().collect();
            items.len() == cells.len()
                && cells.iter().all(|ing| {
                    let ing = ing.unwrap_or_default();
                    match items.iter().position(|id| ing.contains(id)) {
                        Some(i) => {
                            items.swap_remove(i);
                            true
                        }
                        None => false,
                    }
                })
        };
        if ok {
            return Some(Item::new(result as i32, count as i32));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, count: i32) -> Option<Item> {
        Some(Item::new(by_name(name).unwrap(), count))
    }

    #[test]
    fn crafts_planks_sticks_table_pickaxe() {
        // 2×2: 원목 하나 → 판자 4
        assert_eq!(
            craft(&[None, item("oak_log", 1), None, None], 2),
            item("oak_planks", 4)
        );
        // 판자 두 개 세로 → 막대 4 (어느 열이든)
        assert_eq!(
            craft(
                &[None, item("oak_planks", 1), None, item("birch_planks", 1)],
                2
            ),
            item("stick", 4)
        );
        // 2×2 판자 → 작업대
        let p = item("spruce_planks", 1);
        assert_eq!(craft(&[p, p, p, p], 2), item("crafting_table", 1));
        // 3×3 나무 곡괭이
        let (pl, st) = (item("oak_planks", 1), item("stick", 1));
        assert_eq!(
            craft(&[pl, pl, pl, None, st, None, None, st, None], 3),
            item("wooden_pickaxe", 1)
        );
        // 맞지 않는 모양
        assert_eq!(
            craft(&[pl, None, None, None, st, None, None, None, pl], 3),
            None
        );
    }

    #[test]
    fn stone_drops_cobblestone_only_with_pickaxe() {
        let stone = super::super::block::STONE;
        assert_eq!(block_drop(stone, 0, 0), item("cobblestone", 1));
        let b = blocks::block_of(stone);
        assert!(!can_harvest(b, None));
        assert!(can_harvest(b, item("wooden_pickaxe", 1)));
        let iron_ore = blocks::by_name("iron_ore").unwrap();
        assert!(!can_harvest(iron_ore, item("wooden_pickaxe", 1)));
        assert!(can_harvest(iron_ore, item("stone_pickaxe", 1)));
        // 흙은 맨손으로
        assert!(can_harvest(blocks::by_name("dirt").unwrap(), None));
        assert_eq!(
            block_drop(super::super::block::GRASS_BLOCK, 0, 0),
            item("dirt", 1)
        );
    }

    #[test]
    fn food_and_damage() {
        assert_eq!(food(by_name("bread").unwrap()), Some((5, 6.0)));
        assert_eq!(food(by_name("stone").unwrap()), None);
        assert_eq!(attack_damage(item("diamond_sword", 1)), 7.0);
        assert_eq!(attack_damage(None), 1.0);
        assert_eq!(max_stack(by_name("diamond_sword").unwrap()), 1);
        assert_eq!(max_stack(by_name("ender_pearl").unwrap()), 16);
    }
}
