//! 바닐라 데이터. 내용은 `tools/gen_data.py`가 만든 [`generated`]에 있고, 여기는 찾기만 합니다.

#[rustfmt::skip]
#[allow(clippy::type_complexity)]
mod generated;

pub use generated::{
    BLOCK_STATE_COUNT, SYNCED_REGISTRIES, TAGS, argument_type, block, entity_type,
};

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
