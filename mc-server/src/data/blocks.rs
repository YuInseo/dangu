//! 블록 상태 다루기: 상태 ID ↔ (블록, 속성 값).

use std::sync::OnceLock;

use super::generated::{BLOCKS, ITEM_BLOCK, TAGS};

pub type BlockId = usize;
pub type StateId = u32;

/// 상태가 속한 블록.
pub fn block_of(state: StateId) -> BlockId {
    BLOCKS.partition_point(|&(_, first, _, _)| first <= state) - 1
}

/// 블록 종류 수.
pub fn count() -> usize {
    BLOCKS.len()
}

pub fn name(block: BlockId) -> &'static str {
    BLOCKS[block].0
}

pub fn default_state(block: BlockId) -> StateId {
    BLOCKS[block].2
}

/// 이 아이템을 놓으면 생기는 블록.
pub fn block_for_item(item: i32) -> Option<BlockId> {
    let b = *ITEM_BLOCK.get(usize::try_from(item).ok()?)?;
    (b >= 0).then_some(b as BlockId)
}

/// 이 블록을 집으면(크리에이티브 가운데 클릭) 나오는 아이템.
pub fn item_for_block(block: BlockId) -> Option<i32> {
    ITEM_BLOCK
        .iter()
        .position(|&b| b == block as i16)
        .map(|i| i as i32)
}

/// 상태를 속성별 값 번호로 풉니다 (속성 순서는 BLOCKS와 같음).
fn decode(state: StateId) -> (BlockId, Vec<usize>) {
    let block = block_of(state);
    let (_, first, _, props) = BLOCKS[block];
    let mut rest = (state - first) as usize;
    let mut idx = vec![0; props.len()];
    for (i, (_, values)) in props.iter().enumerate().rev() {
        idx[i] = rest % values.len();
        rest /= values.len();
    }
    (block, idx)
}

fn encode(block: BlockId, idx: &[usize]) -> StateId {
    let (_, first, _, props) = BLOCKS[block];
    let mut offset = 0;
    for (i, (_, values)) in props.iter().enumerate() {
        offset = offset * values.len() + idx[i];
    }
    first + offset as u32
}

/// 속성 값 읽기.
pub fn property(state: StateId, prop: &str) -> Option<&'static str> {
    let (block, idx) = decode(state);
    let props = BLOCKS[block].3;
    let i = props.iter().position(|(k, _)| *k == prop)?;
    Some(props[i].1[idx[i]])
}

/// 속성 하나를 바꾼 상태. 블록에 그 속성이나 값이 없으면 원래 상태 그대로.
pub fn with_property(state: StateId, prop: &str, value: &str) -> StateId {
    let (block, mut idx) = decode(state);
    let props = BLOCKS[block].3;
    let Some(i) = props.iter().position(|(k, _)| *k == prop) else {
        return state;
    };
    let Some(v) = props[i].1.iter().position(|x| *x == value) else {
        return state;
    };
    idx[i] = v;
    encode(block, &idx)
}

/// 이 블록의 속성이 가질 수 있는 값들.
pub fn property_values(state: StateId, prop: &str) -> Option<&'static [&'static str]> {
    BLOCKS[block_of(state)]
        .3
        .iter()
        .find(|(k, _)| *k == prop)
        .map(|(_, v)| *v)
}

pub fn has_property(state: StateId, prop: &str) -> bool {
    BLOCKS[block_of(state)].3.iter().any(|(k, _)| *k == prop)
}

/// 블록 태그 하나를 블록별 참/거짓 표로.
fn block_tag(name: &str) -> Vec<bool> {
    let mut set = vec![false; BLOCKS.len()];
    let (_, tags) = TAGS
        .iter()
        .find(|(r, _)| *r == "block")
        .expect("block tags");
    if let Some((_, ids)) = tags.iter().find(|(t, _)| *t == name) {
        for &id in ids.iter() {
            set[id as usize] = true;
        }
    }
    set
}

/// 다른 블록을 놓으면 덮어써지는 블록 (공기, 물, 풀 등 — 바닐라 `#replaceable`).
pub fn is_replaceable(state: StateId) -> bool {
    static TAG: OnceLock<Vec<bool>> = OnceLock::new();
    TAG.get_or_init(|| block_tag("replaceable"))[block_of(state)]
}

pub fn is_air(state: StateId) -> bool {
    static TAG: OnceLock<Vec<bool>> = OnceLock::new();
    TAG.get_or_init(|| block_tag("air"))[block_of(state)]
}

/// 상태의 모든 속성 (이름, 값). 월드 파일에 쓸 때.
pub fn properties(state: StateId) -> Vec<(&'static str, &'static str)> {
    let (block, idx) = decode(state);
    BLOCKS[block]
        .3
        .iter()
        .zip(idx)
        .map(|((k, values), i)| (*k, values[i]))
        .collect()
}

/// 이름과 속성으로 상태 (월드 파일을 읽을 때). 모르는 속성·값은 기본값으로 둡니다.
pub fn state_from(name: &str, props: &[(&str, &str)]) -> Option<StateId> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    let mut state = default_state(by_name_fast(name)?);
    for (k, v) in props {
        state = with_property(state, k, v);
    }
    Some(state)
}

fn by_name_fast(name: &str) -> Option<BlockId> {
    static MAP: OnceLock<std::collections::HashMap<&'static str, BlockId>> = OnceLock::new();
    MAP.get_or_init(|| BLOCKS.iter().enumerate().map(|(i, b)| (b.0, i)).collect())
        .get(name)
        .copied()
}

/// 이름으로 블록 찾기.
pub fn by_name(name: &str) -> Option<BlockId> {
    BLOCKS.iter().position(|(n, _, _, _)| *n == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::block;

    #[test]
    fn properties_round_trip_through_names() {
        for state in (0..crate::data::BLOCK_STATE_COUNT).step_by(7) {
            let props = properties(state);
            assert_eq!(state_from(name(block_of(state)), &props), Some(state));
        }
    }

    #[test]
    fn decode_encode_round_trip_for_every_state() {
        for state in 0..crate::data::BLOCK_STATE_COUNT {
            let (b, idx) = decode(state);
            assert_eq!(encode(b, &idx), state);
        }
    }

    #[test]
    fn known_states() {
        assert_eq!(name(block_of(block::GRASS_BLOCK)), "grass_block");
        assert_eq!(property(block::GRASS_BLOCK, "snowy"), Some("false"));
        assert_eq!(with_property(block::GRASS_BLOCK, "snowy", "true"), 8);
        assert_eq!(with_property(block::OAK_LOG, "axis", "x"), 139);
        assert!(is_air(block::AIR));
        assert!(is_replaceable(block::WATER));
        assert!(is_replaceable(block::SHORT_GRASS));
        assert!(!is_replaceable(block::STONE));
    }

    #[test]
    fn items_map_to_blocks() {
        let stone = block_for_item(1).unwrap(); // 아이템 1 = stone
        assert_eq!(name(stone), "stone");
        assert_eq!(item_for_block(stone), Some(1));
        assert_eq!(block_for_item(0), None); // air
    }
}
