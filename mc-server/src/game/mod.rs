//! 게임 진행: 틱마다 움직이는 엔티티, 상자 내용, 플레이어 저장.

pub mod containers;
pub mod custom_items;
pub mod entities;
pub mod mechanics;
pub mod mobs;
pub mod path;
pub mod playerdata;
pub mod projectiles;
pub mod spawning;
pub mod villagers;

/// 1틱 = 50ms (초당 20틱).
pub const TICK: std::time::Duration = std::time::Duration::from_millis(50);

/// 아주 단순한 난수 (xorshift). 드롭 확률·몹 행동처럼 예측 가능해도 되는 곳에만 씁니다.
pub fn rand_u32() -> u32 {
    use std::cell::Cell;
    thread_local! {
        static STATE: Cell<u64> = Cell::new(u64::from_le_bytes(crate::crypto::random::bytes::<8>()) | 1);
    }
    STATE.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        (x >> 32) as u32
    })
}

/// [0, 1)
pub fn rand_f64() -> f64 {
    rand_u32() as f64 / (u32::MAX as f64 + 1.0)
}
