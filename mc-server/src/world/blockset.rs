//! 이름으로 블록 상태 찾기. 지형·구조물 생성은 수많은 블록을 이름으로 쓰므로, 이름 → 기본
//! 상태를 한 번만 만들어 둡니다.
//!
//! ```ignore
//! let log = b("spruce_log");
//! let stairs = bp("oak_stairs", &[("facing", "north"), ("half", "top")]);
//! ```

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::data::blocks;

fn table() -> &'static HashMap<&'static str, u32> {
    static T: OnceLock<HashMap<&'static str, u32>> = OnceLock::new();
    T.get_or_init(|| {
        (0..crate::data::blocks::count())
            .map(|b| (blocks::name(b), blocks::default_state(b)))
            .collect()
    })
}

/// 블록 이름(접두사 없이)의 기본 상태. 없는 이름은 개발 중 실수이므로 멈춥니다.
pub fn b(name: &str) -> u32 {
    *table()
        .get(name)
        .unwrap_or_else(|| panic!("unknown block {name}"))
}

/// 있으면 그 블록, 없으면 None (판마다 다를 수 있는 블록).
pub fn try_b(name: &str) -> Option<u32> {
    table().get(name).copied()
}

/// 속성을 바꾼 상태. 블록에 없는 속성은 무시합니다.
pub fn bp(name: &str, props: &[(&str, &str)]) -> u32 {
    with(b(name), props)
}

pub fn with(mut state: u32, props: &[(&str, &str)]) -> u32 {
    for (k, v) in props {
        if blocks::has_property(state, k) {
            state = blocks::with_property(state, k, v);
        }
    }
    state
}

/// 상태의 블록 이름.
pub fn name_of(state: u32) -> &'static str {
    blocks::name(blocks::block_of(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_new_blocks() {
        assert_eq!(b("stone"), crate::data::block::STONE);
        assert!(try_b("poplar_log").is_some());
        assert!(try_b("sulfur").is_some());
        let s = bp("oak_stairs", &[("facing", "east"), ("half", "top")]);
        assert_eq!(blocks::property(s, "facing"), Some("east"));
        assert_eq!(name_of(s), "oak_stairs");
    }
}
