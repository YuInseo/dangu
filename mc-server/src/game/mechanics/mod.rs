//! 살아 있는 월드: 블록이 스스로 바뀌는 규칙들.
//!
//! - **예약 틱**: 블록이 바뀌면 이웃 중 반응하는 블록(물·용암, 모래·자갈, 레드스톤, 받침이
//!   필요한 식물)에 틱을 예약하고, 때가 되면 처리합니다 ([`Ticks`]).
//! - **무작위 틱**: 플레이어 근처 청크에서 섹션마다 매 틱 세 칸을 골라 작물이 자라고, 묘목이
//!   나무가 되고, 잔디가 퍼집니다 ([`growth`]).
//! - 화로([`furnace`]), 레드스톤([`redstone`]), 폭발과 TNT([`explosion`]).

pub mod explosion;
pub mod fluids;
pub mod furnace;
pub mod growth;
pub mod portal;
pub mod redstone;

use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::data::{blocks, items};
use crate::packets::world::{BlockPos, Item};
use crate::server::{PlayerState, Server};
use crate::world::Dimension;
use crate::world::blockset::name_of;

/// 한 틱에 처리할 예약 틱의 상한 (물이 끝없이 퍼져도 서버가 멈추지 않도록).
const MAX_SCHEDULED_PER_TICK: usize = 4096;
/// 무작위 틱: 섹션마다 매 틱 고르는 칸 수 (바닐라 `randomTickSpeed` 기본값).
const RANDOM_TICK_SPEED: u32 = 3;
/// 플레이어 둘레 몇 청크까지 무작위 틱을 주는가.
const RANDOM_TICK_RADIUS: i32 = 6;

pub const NEIGHBORS: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (-1, 0, 0),
    (1, 0, 0),
    (0, 0, -1),
    (0, 0, 1),
];

pub fn add(p: BlockPos, d: (i32, i32, i32)) -> BlockPos {
    (p.0 + d.0, p.1 + d.1, p.2 + d.2)
}

/// 예약 틱.
#[derive(Default)]
pub struct Ticks {
    now: AtomicU64,
    scheduled: Mutex<BTreeMap<u64, Vec<(Dimension, BlockPos)>>>,
    pending: Mutex<HashSet<(Dimension, BlockPos, u64)>>,
}

impl Ticks {
    pub fn now(&self) -> u64 {
        self.now.load(Ordering::Relaxed)
    }

    /// `delay`틱 뒤에 이 자리를 다시 봅니다 (같은 때 같은 자리는 한 번만).
    pub fn schedule(&self, dim: Dimension, pos: BlockPos, delay: u64) {
        let at = self.now() + delay.max(1);
        if self.pending.lock().unwrap().insert((dim, pos, at)) {
            self.scheduled
                .lock()
                .unwrap()
                .entry(at)
                .or_default()
                .push((dim, pos));
        }
    }

    fn take_due(&self, now: u64) -> Vec<(Dimension, BlockPos)> {
        let mut out = Vec::new();
        let mut s = self.scheduled.lock().unwrap();
        while let Some((&at, _)) = s.iter().next() {
            if at > now || out.len() >= MAX_SCHEDULED_PER_TICK {
                break;
            }
            let list = s.remove(&at).unwrap();
            let mut pending = self.pending.lock().unwrap();
            for (d, p) in &list {
                pending.remove(&(*d, *p, at));
            }
            out.extend(list);
        }
        out
    }

    pub fn len(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 블록이 바뀐 뒤: 그 자리와 이웃 중 반응하는 블록에 틱을 예약합니다.
pub fn neighbor_changed(server: &Server, dim: Dimension, pos: BlockPos) {
    for d in std::iter::once((0, 0, 0)).chain(NEIGHBORS) {
        let p = add(pos, d);
        let state = server.world.get(dim, p.0, p.1, p.2);
        if let Some(delay) = reaction_delay(dim, state) {
            server.ticks.schedule(dim, p, delay);
        }
    }
    // 레드스톤은 대각선 아래·위의 가루도 영향을 받습니다.
    redstone::nearby_changed(server, dim, pos);
}

/// 이 블록이 이웃 변화에 반응한다면 몇 틱 뒤에.
fn reaction_delay(dim: Dimension, state: u32) -> Option<u64> {
    let n = name_of(state);
    match n {
        "water" => Some(5),
        "lava" => Some(if dim == Dimension::Nether { 10 } else { 30 }),
        _ if falls(n) => Some(2),
        _ if redstone::is_component(n) => Some(1),
        _ if needs_support(n) => Some(1),
        _ => None,
    }
}

/// 받침이 없으면 떨어지는 블록.
pub fn falls(name: &str) -> bool {
    matches!(
        name,
        "sand" | "red_sand" | "gravel" | "suspicious_sand" | "suspicious_gravel"
    ) || name.ends_with("concrete_powder")
        || name.ends_with("anvil")
}

/// 아래 블록이 없으면 부서지는 것 (풀, 꽃, 작물, 횃불, 사탕수수 …).
pub fn needs_support(name: &str) -> bool {
    matches!(
        name,
        "short_grass"
            | "fern"
            | "dead_bush"
            | "sugar_cane"
            | "cactus"
            | "torch"
            | "redstone_torch"
            | "wheat"
            | "carrots"
            | "potatoes"
            | "beetroots"
            | "sweet_berry_bush"
            | "nether_wart"
            | "brown_mushroom"
            | "red_mushroom"
            | "lily_pad"
            | "bamboo"
            | "short_dry_grass"
            | "tall_dry_grass"
            | "bush"
            | "firefly_bush"
            | "leaf_litter"
            | "wildflowers"
            | "pink_petals"
            | "cactus_flower"
            | "lever"
            | "redstone_wire"
            | "repeater"
            | "comparator"
            | "rail"
            | "snow"
            | "moss_carpet"
    ) || name.ends_with("_sapling")
        || name.ends_with("_carpet")
        || name.ends_with("pressure_plate")
        || name.ends_with("_tulip")
        || matches!(
            name,
            "dandelion"
                | "poppy"
                | "blue_orchid"
                | "allium"
                | "azure_bluet"
                | "oxeye_daisy"
                | "cornflower"
                | "lily_of_the_valley"
                | "torchflower"
                | "open_eyeblossom"
                | "closed_eyeblossom"
        )
}

/// 바닥이 있어야 하는 블록이 그대로 있을 수 있는가.
fn supported(server: &Server, dim: Dimension, pos: BlockPos, name: &str) -> bool {
    let below = server.world.get(dim, pos.0, pos.1 - 1, pos.2);
    let bn = name_of(below);
    match name {
        "sugar_cane" => bn == "sugar_cane" || (is_soil(bn) || bn == "sand" || bn == "red_sand"),
        "cactus" => bn == "cactus" || bn == "sand" || bn == "red_sand",
        "cactus_flower" => bn == "cactus",
        "bamboo" => bn == "bamboo" || is_soil(bn) || bn == "sand" || bn == "gravel",
        "lily_pad" => bn == "water",
        "nether_wart" => bn == "soul_sand",
        "wheat" | "carrots" | "potatoes" | "beetroots" => bn == "farmland",
        _ => !crate::world::features::soft(below) || bn == "farmland",
    }
}

fn is_soil(n: &str) -> bool {
    matches!(
        n,
        "grass_block"
            | "dirt"
            | "coarse_dirt"
            | "podzol"
            | "rooted_dirt"
            | "mud"
            | "moss_block"
            | "mycelium"
            | "farmland"
    )
}

/// 예약 틱 하나.
fn scheduled_tick(server: &Server, dim: Dimension, pos: BlockPos) {
    let state = server.world.get(dim, pos.0, pos.1, pos.2);
    let n = name_of(state);
    match n {
        "water" | "lava" => fluids::tick(server, dim, pos, state),
        _ if falls(n) => fall(server, dim, pos, state),
        _ if needs_support(n) && !supported(server, dim, pos, n) => {
            break_naturally(server, dim, pos, state);
        }
        _ if redstone::is_component(n) => redstone::update(server, dim, pos),
        _ => {}
    }
}

/// 모래·자갈: 아래가 비었으면 떨어질 수 있는 가장 낮은 곳으로 (낙하 엔티티 없이 바로).
fn fall(server: &Server, dim: Dimension, pos: BlockPos, state: u32) {
    let mut y = pos.1;
    while y > dim.min_y()
        && crate::world::features::soft(server.world.get(dim, pos.0, y - 1, pos.2))
    {
        y -= 1;
    }
    if y == pos.1 {
        return;
    }
    server.set_block(dim, pos, crate::data::block::AIR);
    server.set_block(dim, (pos.0, y, pos.2), state);
}

/// 부서져서 아이템이 떨어집니다 (받침을 잃은 식물 등).
pub fn break_naturally(server: &Server, dim: Dimension, pos: BlockPos, state: u32) {
    server.set_block_broken(dim, pos, crate::data::block::AIR, state);
    let r = crate::game::rand_u32();
    if let Some(drop) = items::block_drop(state, r % 1000, r >> 10) {
        server.entities.drop_at_block(server, dim, pos, drop);
    }
}

/// 이 블록을 다른 블록으로 (상태 이름으로).
pub fn with_name(state: u32, name: &str) -> u32 {
    let mut s = crate::world::blockset::b(name);
    for (k, v) in blocks::properties(state) {
        s = blocks::with_property(s, k, v);
    }
    s
}

/// 1틱: 예약 틱, 무작위 틱, 화로, TNT.
pub fn tick(server: &Server, now: u64, players: &[PlayerState]) {
    server.ticks.now.store(now, Ordering::Relaxed);
    for (dim, pos) in server.ticks.take_due(now) {
        scheduled_tick(server, dim, pos);
    }
    random_ticks(server, players);
    furnace::tick(server);
    explosion::tick(server, now);
}

fn random_ticks(server: &Server, players: &[PlayerState]) {
    let mut chunks: HashSet<(Dimension, i32, i32)> = HashSet::new();
    for p in players.iter().filter(|p| !p.dead) {
        let (cx, cz) = (
            (p.position.0.floor() as i32) >> 4,
            (p.position.2.floor() as i32) >> 4,
        );
        for dx in -RANDOM_TICK_RADIUS..=RANDOM_TICK_RADIUS {
            for dz in -RANDOM_TICK_RADIUS..=RANDOM_TICK_RADIUS {
                chunks.insert((p.dimension, cx + dx, cz + dz));
            }
        }
    }
    let mut picked = Vec::new();
    for (dim, cx, cz) in chunks {
        let Some(col) = server.world.chunk_if_loaded(dim, cx, cz) else {
            continue;
        };
        for s in 0..dim.section_count() {
            if let crate::world::column::SectionView::Uniform(v) = col.section(s)
                && !growth::ticks_randomly(name_of(v))
            {
                continue;
            }
            for _ in 0..RANDOM_TICK_SPEED {
                let r = crate::game::rand_u32();
                let (x, y, z) = (
                    (r & 15) as usize,
                    ((r >> 4) & 15) as i32,
                    ((r >> 8) & 15) as usize,
                );
                let wy = dim.min_y() + s as i32 * 16 + y;
                let state = col.get(x, wy, z);
                if growth::ticks_randomly(name_of(state)) {
                    picked.push((dim, (cx * 16 + x as i32, wy, cz * 16 + z as i32), state));
                }
            }
        }
    }
    for (dim, pos, state) in picked {
        growth::random_tick(server, dim, pos, state);
    }
}

/// 아이템 하나를 그 자리에 떨어뜨립니다.
pub fn drop_item(server: &Server, dim: Dimension, pos: BlockPos, item: Item) {
    server.entities.drop_at_block(server, dim, pos, item);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_dedupes_and_orders() {
        let t = Ticks::default();
        t.schedule(Dimension::Overworld, (1, 2, 3), 5);
        t.schedule(Dimension::Overworld, (1, 2, 3), 5);
        t.schedule(Dimension::Overworld, (0, 0, 0), 2);
        assert_eq!(t.len(), 2);
        assert_eq!(t.take_due(1), vec![]);
        assert_eq!(t.take_due(2), vec![(Dimension::Overworld, (0, 0, 0))]);
        assert_eq!(t.take_due(10), vec![(Dimension::Overworld, (1, 2, 3))]);
        assert!(t.is_empty());
    }
}
