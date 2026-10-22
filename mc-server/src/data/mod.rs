//! 바닐라 데이터. 내용은 `tools/gen_data.py`가 만든 [`generated`]에 있고, 여기는 찾기만 합니다.

#[rustfmt::skip]
#[allow(clippy::type_complexity)]
mod generated;

pub mod blocks;
mod collision;
pub mod items;
pub mod trades;

pub use generated::{
    BLOCK_ENTITY_TYPES, BLOCK_STATE_COUNT, COMPONENT_NAMES, COOKING, ENTITY_NAMES, PARTICLE_NAMES,
    SOUND_NAMES, STONECUTTING, SYNCED_REGISTRIES, TAGS, argument_type, block, component,
    entity_type, menu, particle, sound,
};

fn index_of(table: &'static [&'static str], name: &str) -> Option<i32> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    type Cache = HashMap<(usize, String), Option<i32>>;
    static CACHE: Mutex<Option<Cache>> = Mutex::new(None);
    let key = (table.as_ptr() as usize, name.to_owned());
    let mut c = CACHE.lock().unwrap();
    let map = c.get_or_insert_with(HashMap::new);
    *map.entry(key)
        .or_insert_with(|| table.iter().position(|n| *n == name).map(|i| i as i32))
}

/// 소리 이름(`entity.generic.explode`) → 네트워크 ID.
pub fn sound_id(name: &str) -> Option<i32> {
    index_of(SOUND_NAMES, name)
}

/// 입자 이름(`explosion`) → 네트워크 ID.
pub fn particle_id(name: &str) -> Option<i32> {
    index_of(PARTICLE_NAMES, name)
}

/// 아이템 컴포넌트 이름(`custom_model_data`) → 네트워크 ID.
pub fn component_id(name: &str) -> Option<i32> {
    index_of(
        COMPONENT_NAMES,
        name.strip_prefix("minecraft:").unwrap_or(name),
    )
}

/// 바닐라 아이템 모양 정의 (`assets/minecraft/items/<이름>.json`의 내용). 사용자 정의 모델
/// 번호(custom_model_data)로 바닐라 아이템 모양을 바꿀 때, 번호가 없으면 이것으로 돌아갑니다.
/// misode/mcmeta `26.3-summary`의 `item_definition`을 gzip으로 넣어 둔 것.
pub fn vanilla_item_definition(item: &str) -> Option<crate::json::Json> {
    use std::sync::OnceLock;
    static DEFS: OnceLock<Option<crate::json::Json>> = OnceLock::new();
    let all = DEFS.get_or_init(|| {
        let gz = include_bytes!("item_definitions.json.gz");
        let raw = crate::protocol::zlib::gunzip(gz, 8 << 20).ok()?;
        crate::json::parse(&String::from_utf8_lossy(&raw)).ok()
    });
    all.as_ref()?
        .get(item.strip_prefix("minecraft:").unwrap_or(item))
        .cloned()
}

/// 엔티티 이름(`tnt`) → 네트워크 ID.
pub fn entity_id(name: &str) -> Option<i32> {
    index_of(ENTITY_NAMES, name)
}

/// 동기화 레지스트리 안에서 `entry`의 네트워크 ID. 생성 데이터에 없는 이름이면 패닉 —
/// 코드에 박힌 이름이 틀린 것이므로 시작하자마자 드러나는 편이 낫습니다.
pub fn registry_id(registry: &str, entry: &str) -> i32 {
    let (_, entries) = SYNCED_REGISTRIES
        .iter()
        .find(|(name, _)| *name == registry)
        .unwrap_or_else(|| panic!("unknown registry {registry}"));
    entries
        .iter()
        .position(|e| *e == entry)
        .unwrap_or_else(|| panic!("{entry} is not in {registry}")) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_up_vanilla_entries() {
        assert_eq!(registry_id("dimension_type", "overworld"), 0);
        registry_id("worldgen/biome", "plains");
        registry_id("worldgen/biome", "nether_wastes");
        registry_id("worldgen/biome", "the_end");
        registry_id("world_clock", "overworld");
    }

    #[test]
    fn global_palette_needs_16_bits() {
        assert_eq!(u32::BITS - (BLOCK_STATE_COUNT - 1).leading_zeros(), 16);
    }
}

#[cfg(test)]
mod definition_tests {
    #[test]
    fn vanilla_item_definitions_and_components() {
        let paper = super::vanilla_item_definition("paper").unwrap();
        assert_eq!(
            paper.get("model").unwrap().get("model").unwrap().as_str(),
            Some("minecraft:item/paper")
        );
        assert!(super::vanilla_item_definition("clock").is_some());
        assert_eq!(super::component_id("custom_data"), Some(0));
        assert!(super::component_id("custom_model_data").is_some());
    }
}
