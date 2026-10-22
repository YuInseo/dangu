//! 구조물: 바이옴처럼 표로 정리한 체계.
//!
//! 1. **어디에** — 바닐라의 구조물 묶음([`data::STRUCTURE_SETS`])을 그대로 씁니다. 간격·분리·
//!    소금과 Java `Random`으로 구한 후보 청크가 바닐라와 같고, 요새는 동심원 배치입니다.
//! 2. **어느 바이옴에** — 바닐라 `has_structure/*` 태그를 펼친 [`data::STRUCTURE_BIOMES`].
//! 3. **무엇을** — 구조물 이름마다 [`KINDS`]의 짓기 함수. 바닐라의 직소 조각(NBT 템플릿)은
//!    클라이언트에도 서버 데이터에도 없어서, 같은 모습을 코드로 짓습니다.
//!
//! 구조물 하나는 여러 청크에 걸칩니다. 청크를 만들 때마다 그 청크에 닿을 수 있는 시작점들을
//! 모두 찾아 처음부터 짓고([`Builder`]), 이 청크 안의 블록만 씁니다 — 나무와 같은 방식이라
//! 어느 청크를 먼저 만들든 구조물이 이어집니다.

mod build;
pub mod data;
mod nether_end;
mod ruins;
mod temples;
mod underground;
mod village;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub use build::Builder;

use super::Dimension;
use super::biome::{self, Biome};
use super::features::{Placer, Rng};
use super::noise::hash;

pub enum Placement {
    Spread {
        spacing: i32,
        separation: i32,
        salt: i64,
        frequency: f32,
        triangular: bool,
    },
    Rings {
        distance: i32,
        count: i32,
        spread: i32,
    },
}

pub struct SetData {
    pub name: &'static str,
    pub placement: Placement,
    pub structures: &'static [(&'static str, u32)],
}

pub struct LootPool {
    pub rolls: (i32, i32),
    /// (아이템, 가중치, 최소, 최대, 마법 부여)
    pub entries: &'static [(&'static str, u32, i32, i32, bool)],
}

/// 구조물을 놓을 높이.
#[derive(Clone, Copy)]
pub enum Height {
    /// 시작 청크 가운데의 지표.
    Surface,
    /// 바다 밑바닥.
    SeaFloor,
    /// 이 범위 안에서 무작위.
    Range(i32, i32),
}

pub struct Kind {
    pub name: &'static str,
    pub dimension: Dimension,
    /// 시작 청크에서 몇 청크까지 뻗을 수 있나.
    pub reach: i32,
    pub height: Height,
    pub build: fn(&mut Builder),
}

macro_rules! kind {
    ($name:literal, $dim:ident, $reach:literal, $h:expr, $f:path) => {
        Kind {
            name: $name,
            dimension: Dimension::$dim,
            reach: $reach,
            height: $h,
            build: $f,
        }
    };
}

/// 지을 수 있는 구조물들. 바닐라 이름과 같습니다.
pub const KINDS: &[Kind] = &[
    kind!(
        "village_plains",
        Overworld,
        4,
        Height::Surface,
        village::build
    ),
    kind!(
        "village_desert",
        Overworld,
        4,
        Height::Surface,
        village::build
    ),
    kind!(
        "village_savanna",
        Overworld,
        4,
        Height::Surface,
        village::build
    ),
    kind!(
        "village_snowy",
        Overworld,
        4,
        Height::Surface,
        village::build
    ),
    kind!(
        "village_taiga",
        Overworld,
        4,
        Height::Surface,
        village::build
    ),
    kind!(
        "desert_pyramid",
        Overworld,
        1,
        Height::Surface,
        temples::desert_pyramid
    ),
    kind!(
        "jungle_pyramid",
        Overworld,
        1,
        Height::Surface,
        temples::jungle_pyramid
    ),
    kind!("igloo", Overworld, 1, Height::Surface, temples::igloo),
    kind!(
        "swamp_hut",
        Overworld,
        1,
        Height::Surface,
        temples::swamp_hut
    ),
    kind!(
        "pillager_outpost",
        Overworld,
        1,
        Height::Surface,
        temples::pillager_outpost
    ),
    kind!("mansion", Overworld, 2, Height::Surface, temples::mansion),
    kind!(
        "ruined_portal",
        Overworld,
        1,
        Height::Surface,
        ruins::ruined_portal
    ),
    kind!(
        "ruined_portal_desert",
        Overworld,
        1,
        Height::Surface,
        ruins::ruined_portal
    ),
    kind!(
        "ruined_portal_jungle",
        Overworld,
        1,
        Height::Surface,
        ruins::ruined_portal
    ),
    kind!(
        "ruined_portal_swamp",
        Overworld,
        1,
        Height::Surface,
        ruins::ruined_portal
    ),
    kind!(
        "ruined_portal_mountain",
        Overworld,
        1,
        Height::Surface,
        ruins::ruined_portal
    ),
    kind!(
        "ruined_portal_ocean",
        Overworld,
        1,
        Height::SeaFloor,
        ruins::ruined_portal
    ),
    kind!(
        "shipwreck",
        Overworld,
        1,
        Height::SeaFloor,
        ruins::shipwreck
    ),
    kind!(
        "shipwreck_beached",
        Overworld,
        1,
        Height::Surface,
        ruins::shipwreck
    ),
    kind!(
        "ocean_ruin_cold",
        Overworld,
        1,
        Height::SeaFloor,
        ruins::ocean_ruin
    ),
    kind!(
        "ocean_ruin_warm",
        Overworld,
        1,
        Height::SeaFloor,
        ruins::ocean_ruin
    ),
    kind!(
        "buried_treasure",
        Overworld,
        0,
        Height::Surface,
        ruins::buried_treasure
    ),
    kind!(
        "trail_ruins",
        Overworld,
        1,
        Height::Surface,
        ruins::trail_ruins
    ),
    kind!("monument", Overworld, 2, Height::SeaFloor, ruins::monument),
    kind!(
        "mineshaft",
        Overworld,
        4,
        Height::Range(10, 40),
        underground::mineshaft
    ),
    kind!(
        "mineshaft_mesa",
        Overworld,
        4,
        Height::Range(30, 60),
        underground::mineshaft
    ),
    kind!(
        "stronghold",
        Overworld,
        4,
        Height::Range(10, 30),
        underground::stronghold
    ),
    kind!(
        "trial_chambers",
        Overworld,
        2,
        Height::Range(-40, -20),
        underground::trial_chambers
    ),
    kind!(
        "ancient_city",
        Overworld,
        3,
        Height::Range(-51, -51),
        underground::ancient_city
    ),
    kind!(
        "fortress",
        Nether,
        5,
        Height::Range(48, 70),
        nether_end::fortress
    ),
    kind!(
        "bastion_remnant",
        Nether,
        2,
        Height::Range(33, 75),
        nether_end::bastion
    ),
    kind!(
        "nether_fossil",
        Nether,
        0,
        Height::Range(32, 60),
        nether_end::nether_fossil
    ),
    kind!(
        "ruined_portal_nether",
        Nether,
        1,
        Height::Range(40, 90),
        ruins::ruined_portal
    ),
    kind!("end_city", End, 2, Height::Surface, nether_end::end_city),
];

/// 26.3의 버려진 야영지는 바이옴마다 이름이 다른 구조물 18개입니다. 모두 같은 짓기 함수로.
fn kind_for(name: &str) -> Option<&'static Kind> {
    static CAMP: Kind = Kind {
        name: "abandoned_camp",
        dimension: Dimension::Overworld,
        reach: 1,
        height: Height::Surface,
        build: ruins::abandoned_camp,
    };
    if name.starts_with("abandoned_camp_") {
        return Some(&CAMP);
    }
    KINDS.iter().find(|k| k.name == name)
}

fn biomes_of(structure: &str) -> &'static [&'static str] {
    data::STRUCTURE_BIOMES
        .iter()
        .find(|(n, _)| *n == structure)
        .map_or(&[], |(_, b)| b)
}

// ---- Java Random (바닐라와 같은 후보 위치를 위해) ----

struct JavaRandom(i64);

impl JavaRandom {
    fn new(seed: i64) -> Self {
        Self((seed ^ 0x5DEE_CE66D) & ((1 << 48) - 1))
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.0 = (self.0.wrapping_mul(0x5DEE_CE66D).wrapping_add(0xB)) & ((1 << 48) - 1);
        (self.0 >> (48 - bits)) as i32
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        if bound <= 0 {
            return 0;
        }
        if bound & -bound == bound {
            return ((bound as i64 * self.next(31) as i64) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let val = bits % bound;
            if bits - val + (bound - 1) >= 0 {
                return val;
            }
        }
    }

    fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }

    /// `WorldgenRandom.setLargeFeatureWithSalt`.
    fn large_feature(world_seed: u64, x: i32, z: i32, salt: i64) -> Self {
        let s = (x as i64)
            .wrapping_mul(341_873_128_712)
            .wrapping_add((z as i64).wrapping_mul(132_897_987_541))
            .wrapping_add(world_seed as i64)
            .wrapping_add(salt);
        Self::new(s)
    }
}

/// 묶음의 영역 (rx, rz) 안 후보 청크.
fn spread_candidate(seed: u64, set: &SetData, rx: i32, rz: i32) -> Option<(i32, i32)> {
    let Placement::Spread {
        spacing,
        separation,
        salt,
        frequency,
        triangular,
    } = set.placement
    else {
        return None;
    };
    let mut r = JavaRandom::large_feature(seed, rx, rz, salt);
    let k = (spacing - separation).max(1);
    let eval = |r: &mut JavaRandom| {
        if triangular {
            (r.next_int(k) + r.next_int(k)) / 2
        } else {
            r.next_int(k)
        }
    };
    let (ox, oz) = (eval(&mut r), eval(&mut r));
    let (cx, cz) = (rx * spacing + ox, rz * spacing + oz);
    if frequency < 1.0 {
        let mut f = JavaRandom::large_feature(seed, cx, cz, salt);
        if f.next_float() >= frequency {
            return None;
        }
    }
    Some((cx, cz))
}

/// 동심원 배치의 청크들 (요새). 시드마다 한 번만 계산합니다.
fn ring_positions(seed: u64, set: &SetData) -> Vec<(i32, i32)> {
    type Rings = HashMap<(u64, &'static str), Vec<(i32, i32)>>;
    static CACHE: Mutex<Option<Rings>> = Mutex::new(None);
    if let Some(v) = CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .get(&(seed, set.name))
    {
        return v.clone();
    }
    let Placement::Rings {
        distance,
        count,
        spread,
    } = set.placement
    else {
        return Vec::new();
    };
    let mut rng = Rng::new(seed ^ 0x5770);
    let mut angle = rng.f64() * std::f64::consts::TAU;
    let (mut in_ring, mut ring, mut spread) = (0, 0, spread);
    let mut out = Vec::new();
    for i in 0..count {
        let dist =
            (4 * distance + distance * ring * 6) as f64 + (rng.f64() - 0.5) * distance as f64 * 2.5;
        out.push((
            (angle.cos() * dist).round() as i32,
            (angle.sin() * dist).round() as i32,
        ));
        angle += std::f64::consts::TAU / spread as f64;
        in_ring += 1;
        if in_ring == spread {
            ring += 1;
            in_ring = 0;
            spread += 2 * spread / (ring + 1);
            spread = spread.min(count - i);
            angle += rng.f64() * std::f64::consts::TAU;
        }
    }
    CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert((seed, set.name), out.clone());
    out
}

/// 그 자리의 바이옴 (구조물이 생길지 고를 때).
pub fn biome_at(dim: Dimension, seed: u64, x: i32, z: i32) -> Biome {
    match dim {
        Dimension::Overworld => {
            let h = biome::surface_height(seed, x, z);
            biome::surface_biome(seed, x, z, h)
        }
        Dimension::Nether => biome::nether_biome(seed, x, z),
        Dimension::End => super::terrain::end_biome(seed, x, z),
    }
}

/// 시작점 하나.
#[derive(Clone)]
pub struct Start {
    pub kind: &'static Kind,
    /// 바닐라 구조물 이름 (야영지는 바이옴별 이름 그대로).
    pub structure: &'static str,
    pub chunk: (i32, i32),
    pub biome: Biome,
}

/// 묶음 하나에서 청크 (cx, cz)의 시작점 (있으면).
fn start_at(seed: u64, dim: Dimension, set: &'static SetData, cx: i32, cz: i32) -> Option<Start> {
    let (x, z) = (cx * 16 + 8, cz * 16 + 8);
    let bio = biome_at(dim, seed, x, z);
    // 가중치 순서로 골라 보고, 바이옴이 맞지 않으면 다음 것 (바닐라와 같은 방식)
    let mut rng = Rng::new(hash(seed ^ 0x57A7, cx, cz));
    let mut left: Vec<(&'static str, u32)> = set.structures.to_vec();
    while !left.is_empty() {
        let name = rng.weighted(&left);
        left.retain(|(n, _)| *n != name);
        let Some(kind) = kind_for(name) else { continue };
        if kind.dimension != dim || !biomes_of(name).contains(&bio.name()) {
            continue;
        }
        return Some(Start {
            kind,
            structure: name,
            chunk: (cx, cz),
            biome: bio,
        });
    }
    None
}

/// 청크 (cx, cz)에 닿는 모든 시작점.
pub fn starts_near(seed: u64, dim: Dimension, cx: i32, cz: i32) -> Vec<Start> {
    let mut out = Vec::new();
    for set in data::STRUCTURE_SETS {
        let reach = set
            .structures
            .iter()
            .filter_map(|(n, _)| kind_for(n))
            .filter(|k| k.dimension == dim)
            .map(|k| k.reach)
            .max();
        let Some(reach) = reach else { continue };
        match set.placement {
            Placement::Spread { spacing, .. } => {
                let (r0x, r1x) = (
                    (cx - reach).div_euclid(spacing),
                    (cx + reach).div_euclid(spacing),
                );
                let (r0z, r1z) = (
                    (cz - reach).div_euclid(spacing),
                    (cz + reach).div_euclid(spacing),
                );
                for rx in r0x..=r1x {
                    for rz in r0z..=r1z {
                        let Some((sx, sz)) = spread_candidate(seed, set, rx, rz) else {
                            continue;
                        };
                        if (sx - cx).abs() > reach || (sz - cz).abs() > reach {
                            continue;
                        }
                        if let Some(s) = start_at(seed, dim, set, sx, sz)
                            && (sx - cx).abs() <= s.kind.reach
                            && (sz - cz).abs() <= s.kind.reach
                        {
                            out.push(s);
                        }
                    }
                }
            }
            Placement::Rings { .. } => {
                if dim != Dimension::Overworld {
                    continue;
                }
                for (sx, sz) in ring_positions(seed, set) {
                    if (sx - cx).abs() <= reach && (sz - cz).abs() <= reach {
                        let bio = biome_at(dim, seed, sx * 16 + 8, sz * 16 + 8);
                        let kind = kind_for("stronghold").unwrap();
                        out.push(Start {
                            kind,
                            structure: "stronghold",
                            chunk: (sx, sz),
                            biome: bio,
                        });
                    }
                }
            }
        }
    }
    out
}

/// 청크 하나를 만들 때: 닿는 구조물을 모두 짓습니다.
pub fn generate(p: &mut Placer, seed: u64, dim: Dimension) {
    for start in starts_near(seed, dim, p.cx, p.cz) {
        let mut b = Builder::begin(p, seed, dim, &start);
        (start.kind.build)(&mut b);
        b.finish();
    }
}

/// 가장 가까운 구조물 (블록 좌표). `name`은 바닐라 이름 또는 묶음 이름(`village` 등).
pub fn locate(
    seed: u64,
    dim: Dimension,
    name: &str,
    from: (i32, i32),
    max_chunks: i32,
) -> Option<(i32, i32, &'static str)> {
    let (fcx, fcz) = (from.0 >> 4, from.1 >> 4);
    let matches = |structure: &str| {
        structure == name
            || structure.starts_with(&format!("{name}_"))
            || (name == "camp" && structure.starts_with("abandoned_camp"))
    };
    let mut best: Option<(i64, i32, i32, &'static str)> = None;
    for set in data::STRUCTURE_SETS {
        if !set.structures.iter().any(|(n, _)| matches(n)) && set.name != name {
            continue;
        }
        let mut consider = |sx: i32, sz: i32| {
            if let Some(s) = start_at(seed, dim, set, sx, sz)
                && (matches(s.structure) || set.name == name)
            {
                let d = ((sx - fcx) as i64).pow(2) + ((sz - fcz) as i64).pow(2);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, sx * 16 + 8, sz * 16 + 8, s.structure));
                }
            }
        };
        match set.placement {
            Placement::Spread { spacing, .. } => {
                let r = max_chunks / spacing + 1;
                let (rcx, rcz) = (fcx.div_euclid(spacing), fcz.div_euclid(spacing));
                for rx in rcx - r..=rcx + r {
                    for rz in rcz - r..=rcz + r {
                        if let Some((sx, sz)) = spread_candidate(seed, set, rx, rz) {
                            consider(sx, sz);
                        }
                    }
                }
            }
            Placement::Rings { .. } if dim == Dimension::Overworld => {
                for (sx, sz) in ring_positions(seed, set) {
                    let d = ((sx - fcx) as i64).pow(2) + ((sz - fcz) as i64).pow(2);
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, sx * 16 + 8, sz * 16 + 8, "stronghold"));
                    }
                }
            }
            Placement::Rings { .. } => {}
        }
    }
    best.map(|(_, x, z, n)| (x, z, n))
}

/// 가장 가까운 바이옴 (나선으로 32블록마다).
pub fn locate_biome(
    seed: u64,
    dim: Dimension,
    target: Biome,
    from: (i32, i32),
    radius: i32,
) -> Option<(i32, i32)> {
    let step = 32;
    for r in 0..=radius / step {
        for i in -r..=r {
            for j in -r..=r {
                if i.abs() != r && j.abs() != r {
                    continue;
                }
                let (x, z) = (from.0 + i * step, from.1 + j * step);
                if biome_at(dim, seed, x, z) == target {
                    return Some((x, z));
                }
            }
        }
    }
    None
}

/// 구조물 이름들 (`/locate` 자동 완성).
pub fn names() -> Vec<&'static str> {
    static N: OnceLock<Vec<&'static str>> = OnceLock::new();
    N.get_or_init(|| {
        let mut v: Vec<&'static str> = KINDS.iter().map(|k| k.name).collect();
        v.push("abandoned_camp");
        v.push("village");
        v.push("ruined_portal");
        v.sort();
        v.dedup();
        v
    })
    .clone()
}

// ---- 전리품 ----

/// 전리품 표로 상자 내용을 만듭니다: (칸, 아이템 이름, 개수, 마법 부여).
pub fn roll_loot(table: &str, seed: u64) -> Vec<(usize, &'static str, i32, bool)> {
    let Some((_, pools)) = data::LOOT_TABLES.iter().find(|(n, _)| *n == table) else {
        return Vec::new();
    };
    let mut rng = Rng::new(seed);
    let mut items = Vec::new();
    for pool in *pools {
        let rolls = rng.range(pool.rolls.0, pool.rolls.1);
        let weighted: Vec<(usize, u32)> = pool
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (i, e.1))
            .collect();
        if weighted.is_empty() {
            continue;
        }
        for _ in 0..rolls {
            let (name, _, lo, hi, ench) = pool.entries[rng.weighted(&weighted)];
            if name.is_empty() {
                continue;
            }
            let n = rng.range(lo, hi);
            if n > 0 {
                items.push((name, n, ench));
            }
        }
    }
    // 바닐라처럼 27칸에 흩어 놓습니다.
    let mut slots: Vec<usize> = (0..27).collect();
    for i in (1..slots.len()).rev() {
        let j = rng.below(i as u32 + 1) as usize;
        slots.swap(i, j);
    }
    items
        .into_iter()
        .zip(slots)
        .map(|((name, n, e), slot)| (slot, name, n, e))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_random_matches_known_values() {
        // new Random(0).nextInt(10) == 0, nextInt(10) == 8, nextInt(10) == 9 (Java 표준 라이브러리)
        let mut r = JavaRandom::new(0);
        assert_eq!([r.next_int(10), r.next_int(10), r.next_int(10)], [0, 8, 9]);
        // 명세의 알고리즘을 그대로 옮긴 파이썬 구현과 같은 값
        let mut r = JavaRandom::new(42);
        assert_eq!(
            [r.next_int(100), r.next_int(100), r.next_int(100)],
            [30, 63, 48]
        );
    }

    #[test]
    fn every_vanilla_structure_has_a_builder_or_is_listed() {
        let mut missing = Vec::new();
        for set in data::STRUCTURE_SETS {
            for (name, _) in set.structures {
                if kind_for(name).is_none() {
                    missing.push(*name);
                }
            }
        }
        assert!(missing.is_empty(), "no builder: {missing:?}");
    }

    #[test]
    fn villages_can_be_located() {
        let found = locate(1, Dimension::Overworld, "village", (0, 0), 400);
        assert!(found.is_some(), "no village within 6400 blocks");
        let found = locate(1, Dimension::Overworld, "stronghold", (0, 0), 400).unwrap();
        let d = ((found.0 as f64).powi(2) + (found.1 as f64).powi(2)).sqrt();
        assert!((1000.0..4000.0).contains(&d), "stronghold at {d}");
    }

    #[test]
    fn loot_tables_roll_real_items() {
        let items = roll_loot("chests/simple_dungeon", 7);
        assert!(!items.is_empty());
        for (_, name, n, _) in items {
            assert!(crate::data::items::by_name(name).is_some(), "{name}");
            assert!(n > 0);
        }
    }
}
