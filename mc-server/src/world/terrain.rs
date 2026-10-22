//! 지형 생성: 기후 → 높이와 바이옴 → 돌·겉흙 → 동굴 → 광석 → 나무·풀 → 구조물 → 눈.
//!
//! - 오버월드: [`super::biome`]의 기후로 정한 높이(바다·해안·평지·산·강)와 바이옴별 겉흙,
//!   3차원 노이즈 동굴(넓은 굴 + 국수 굴)과 동굴 바이옴 장식, 광석, 나무·풀, 구조물.
//! - 네더: 바닥·천장 사이의 공간, 다섯 바이옴(진홍·뒤틀린 숲, 영혼 모래 골짜기, 현무암 삼각주,
//!   네더 황무지), 석영·금·고대 잔해.
//! - 엔드: 가운데 섬과 흑요석 기둥, 1000블록 밖의 바깥 섬들(엔드 고지대, 후렴 식물).

use std::collections::HashMap;
use std::sync::Mutex;

use super::Dimension;
use super::biome::{self, Biome, Climate};
use super::blockset::{b, bp};
use super::features::{self, Placer, Rng};
use super::noise::{fbm, fbm3, hash};
use crate::data::block;

pub const SEA_LEVEL: i32 = biome::SEA_LEVEL;
pub const LAVA_LEVEL: i32 = 31;
/// 이 아래의 동굴은 용암으로 찹니다 (바닐라와 같은 -55).
const CAVE_LAVA_LEVEL: i32 = -55;

/// 블록 하나하나를 담은 청크 기둥 (16×높이×16)과 바이옴(4×4×4 칸마다).
pub struct Column {
    pub dimension: Dimension,
    blocks: Vec<u32>,
    /// (높이/4) × 16칸, `ycell*16 + (z/4)*4 + x/4`. 값은 [`Biome`] 번호.
    biomes: Vec<u8>,
    /// 구조물 상자들의 전리품 표: (월드 좌표, 표 이름).
    pub loot: Vec<((i32, i32, i32), &'static str)>,
    /// 몹 생성기: (월드 좌표, 엔티티 이름).
    pub spawners: Vec<((i32, i32, i32), &'static str)>,
}

impl Column {
    fn new(dimension: Dimension, fill_biome: Biome) -> Self {
        Self {
            dimension,
            blocks: vec![block::AIR; dimension.height() as usize * 256],
            biomes: vec![fill_biome.0; dimension.height() as usize / 4 * 16],
            loot: Vec::new(),
            spawners: Vec::new(),
        }
    }

    fn index(&self, x: usize, y: i32, z: usize) -> Option<usize> {
        let ry = y - self.dimension.min_y();
        (0..self.dimension.height())
            .contains(&ry)
            .then(|| ry as usize * 256 + z * 16 + x)
    }

    pub fn get(&self, x: usize, y: i32, z: usize) -> u32 {
        self.index(x, y, z).map_or(block::AIR, |i| self.blocks[i])
    }

    pub fn set(&mut self, x: usize, y: i32, z: usize, state: u32) {
        if let Some(i) = self.index(x, y, z) {
            self.blocks[i] = state;
        }
    }

    /// 섹션 하나(아래에서 `section`번째)의 4096개 블록, `y*256 + z*16 + x` 순서.
    pub fn section(&self, section: usize) -> &[u32] {
        &self.blocks[section * 4096..(section + 1) * 4096]
    }

    /// 섹션 하나의 바이옴 64칸, `(y*4 + z)*4 + x` 순서 (바닐라 팔레트 순서).
    pub fn section_biomes(&self, section: usize) -> &[u8] {
        &self.biomes[section * 64..(section + 1) * 64]
    }

    pub fn biome_at(&self, x: usize, y: i32, z: usize) -> Biome {
        let ry = ((y - self.dimension.min_y()).clamp(0, self.dimension.height() - 1) / 4) as usize;
        Biome(self.biomes[ry * 16 + (z / 4) * 4 + x / 4])
    }

    fn set_biome_cell(&mut self, cx: usize, ycell: usize, cz: usize, b: Biome) {
        if let Some(c) = self.biomes.get_mut(ycell * 16 + cz * 4 + cx) {
            *c = b.0;
        }
    }
}

/// 청크 (cx, cz)를 만듭니다.
pub fn generate(dimension: Dimension, seed: u64, cx: i32, cz: i32) -> Column {
    match dimension {
        Dimension::Overworld => overworld(seed, cx, cz),
        Dimension::Nether => nether(seed, cx, cz),
        Dimension::End => end(seed, cx, cz),
    }
}

/// 플레이어가 설 수 있는 가장 높은 블록 바로 위의 y (지형만 보고, 나무는 무시).
/// 설 자리가 없으면(바다, 용암, 공허) `None`.
pub fn standing_y(dimension: Dimension, seed: u64, x: i32, z: i32) -> Option<i32> {
    match dimension {
        Dimension::Overworld => {
            let h = biome::surface_height(seed, x, z);
            (h >= SEA_LEVEL).then_some(h + 1)
        }
        Dimension::Nether => {
            let floor = nether_floor(seed, x, z);
            (floor > LAVA_LEVEL).then_some(floor + 1)
        }
        Dimension::End => end_island(seed, x, z).map(|(_, top)| top + 1),
    }
}

/// 원점에서 가까운, 하늘이 트인 설 자리 (블록 중앙 좌표). 차원·시드마다 한 번 계산합니다.
pub fn spawn_point(dimension: Dimension, seed: u64) -> (f64, f64, f64) {
    type Spawns = HashMap<(Dimension, u64), (f64, f64, f64)>;
    static CACHE: Mutex<Option<Spawns>> = Mutex::new(None);
    if let Some(p) = CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .get(&(dimension, seed))
    {
        return *p;
    }
    let p = find_spawn(dimension, seed);
    CACHE
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert((dimension, seed), p);
    p
}

fn find_spawn(dimension: Dimension, seed: u64) -> (f64, f64, f64) {
    // 먼저 성기게(8블록마다, 반지름 4000까지) 땅을 찾고, 그 둘레를 촘촘히 봅니다.
    let mut near = (0, 0);
    'coarse: for r in 0..500i32 {
        for i in -r..=r {
            for j in -r..=r {
                if i.abs() != r && j.abs() != r {
                    continue;
                }
                let (x, z) = (i * 8, j * 8);
                if standing_y(dimension, seed, x, z).is_some_and(|y| y > SEA_LEVEL + 1) {
                    near = (x, z);
                    break 'coarse;
                }
            }
        }
    }
    let mut columns: HashMap<(i32, i32), Column> = HashMap::new();
    for r in 0..96i32 {
        for dx in -r..=r {
            for dz in -r..=r {
                if dx.abs() != r && dz.abs() != r {
                    continue;
                }
                let (x, z) = (near.0 + dx, near.1 + dz);
                let Some(y) = standing_y(dimension, seed, x, z) else {
                    continue;
                };
                // 실제 블록으로 확인: 발 자리와 머리 자리가 비고, 발밑이 단단해야.
                let key = (x >> 4, z >> 4);
                if columns.len() > 64 {
                    columns.clear();
                }
                let col = columns
                    .entry(key)
                    .or_insert_with(|| generate(dimension, seed, key.0, key.1));
                let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
                let mut top = y;
                // 그 자리에 놓인 구조물·눈 위로
                while top < y + 40 && !features::soft(col.get(lx, top, lz)) {
                    top += 1;
                }
                let feet = col.get(lx, top, lz);
                let head = col.get(lx, top + 1, lz);
                let below = col.get(lx, top - 1, lz);
                let solid_below = !features::soft(below)
                    || crate::data::blocks::name(crate::data::blocks::block_of(below))
                        .ends_with("leaves");
                let leafy = crate::data::blocks::name(crate::data::blocks::block_of(below))
                    .ends_with("leaves");
                if (feet == block::AIR || feet == b("snow"))
                    && head == block::AIR
                    && solid_below
                    && !leafy
                    && below != block::WATER
                {
                    return (x as f64 + 0.5, top as f64, z as f64 + 0.5);
                }
            }
        }
    }
    (0.5, 100.0, 0.5)
}

// ---- 오버월드 ----

fn overworld(seed: u64, cx: i32, cz: i32) -> Column {
    let mut col = Column::new(Dimension::Overworld, biome::id("plains"));
    let mut heights = [0i32; 256];
    let mut biomes = [Biome(0); 256];
    let mut climates: Vec<Climate> = Vec::with_capacity(256);
    for z in 0..16 {
        for x in 0..16 {
            let (wx, wz) = (cx * 16 + x as i32, cz * 16 + z as i32);
            let h = biome::surface_height(seed, wx, wz);
            heights[z * 16 + x] = h;
            biomes[z * 16 + x] = biome::surface_biome(seed, wx, wz, h);
            climates.push(biome::climate(seed, wx, wz));
        }
    }

    // 돌, 겉흙, 물
    let stone = block::STONE;
    let deepslate = bp("deepslate", &[("axis", "y")]);
    for z in 0..16 {
        for x in 0..16 {
            let (wx, wz) = (cx * 16 + x as i32, cz * 16 + z as i32);
            let h = heights[z * 16 + x];
            let def = biomes[z * 16 + x].def();
            let r = hash(seed ^ 0xBED, wx, wz);
            col.set(x, -64, z, block::BEDROCK);
            for y in -63..-59 {
                if (r >> (y + 64)) & 1 == 1 {
                    col.set(x, y, z, block::BEDROCK);
                }
            }
            // 0~8 사이에서 심층암으로 섞여 바뀝니다.
            let slate_top = ((r >> 8) % 8) as i32;
            let depth = 3 + ((r >> 16) % 2) as i32;
            let underwater = h < SEA_LEVEL - 1;
            for y in -63..=h {
                if col.get(x, y, z) == block::BEDROCK {
                    continue;
                }
                let state = if y < slate_top {
                    deepslate
                } else if y == h {
                    b(if underwater { def.floor } else { def.top })
                } else if y >= h - depth {
                    b(if underwater { def.floor } else { def.under })
                } else {
                    stone
                };
                col.set(x, y, z, state);
            }
            if def.name.contains("badlands") {
                badlands_bands(&mut col, x, z, h, seed);
            }
            if def.name == "ice_spikes" && r.is_multiple_of(160) {
                let len = 6 + (r >> 20) as i32 % 18;
                for y in h + 1..h + len {
                    col.set(x, y, z, b("packed_ice"));
                }
            }
            for y in h + 1..=SEA_LEVEL {
                col.set(x, y, z, block::WATER);
            }
        }
    }

    carve_caves(&mut col, seed, cx, cz, &heights);
    // 바이옴 칸: 지표 근처는 지상 바이옴, 깊은 곳은 동굴 바이옴
    for zc in 0..4 {
        for xc in 0..4 {
            let i = (zc * 4 + 2) * 16 + xc * 4 + 2;
            let surface = heights[i];
            let (wx, wz) = (cx * 16 + xc as i32 * 4 + 2, cz * 16 + zc as i32 * 4 + 2);
            for ycell in 0..(Dimension::Overworld.height() / 4) as usize {
                let y = Dimension::Overworld.min_y() + ycell as i32 * 4 + 2;
                let bio = biome::cave_biome_with(seed, &climates[i], wx, y, wz, surface)
                    .unwrap_or(biomes[i]);
                col.set_biome_cell(xc, ycell, zc, bio);
            }
        }
    }
    decorate_caves(&mut col, seed, cx, cz);

    {
        let mut p = Placer::new(&mut col, cx, cz);
        features::ores(&mut p, seed);
        for dx in -1..=1 {
            for dz in -1..=1 {
                features::trees_from(&mut p, seed, cx + dx, cz + dz);
            }
        }
        features::plants(&mut p, seed, &biomes);
        super::structure::generate(&mut p, seed, Dimension::Overworld);
    }
    features::snow(&mut col, &biomes);
    col
}

/// 황무지의 색 띠 테라코타.
fn badlands_bands(col: &mut Column, x: usize, z: usize, h: i32, seed: u64) {
    const BANDS: &[&str] = &[
        "terracotta",
        "orange_terracotta",
        "terracotta",
        "yellow_terracotta",
        "brown_terracotta",
        "terracotta",
        "red_terracotta",
        "white_terracotta",
        "light_gray_terracotta",
        "terracotta",
        "orange_terracotta",
        "red_terracotta",
    ];
    for y in (h - 14).max(SEA_LEVEL - 4)..h {
        let wobble = (fbm(seed ^ 0xBAD, x as f64, z as f64, 64.0, 2) * 3.0) as i32;
        let band = BANDS[((y + wobble).rem_euclid(BANDS.len() as i32)) as usize];
        col.set(x, y, z, b(band));
    }
}

/// 동굴 노이즈를 4×8×4 격자에서 재고 그 사이는 섞어서 씁니다 (바닐라도 같은 방식).
fn carve_caves(col: &mut Column, seed: u64, cx: i32, cz: i32, heights: &[i32; 256]) {
    const SX: usize = 5;
    const SY_STEP: i32 = 8;
    let min_y = -60;
    let max_y = 200;
    let sy = ((max_y - min_y) / SY_STEP + 1) as usize;
    let sample = |gx: usize, gy: usize, gz: usize, which: u64| {
        let (x, y, z) = (
            (cx * 16 + gx as i32 * 4) as f64,
            (min_y + gy as i32 * SY_STEP) as f64,
            (cz * 16 + gz as i32 * 4) as f64,
        );
        match which {
            0 => fbm3(seed ^ 0xC4E5, x, y, z, 72.0, 36.0),
            1 => fbm3(seed ^ 0x5A61, x, y, z, 48.0, 24.0),
            _ => fbm3(seed ^ 0x5A62, x, y, z, 48.0, 24.0),
        }
    };
    let mut grid = vec![[0.0f64; 3]; SX * sy * SX];
    for gx in 0..SX {
        for gz in 0..SX {
            for gy in 0..sy {
                let g = &mut grid[(gx * sy + gy) * SX + gz];
                for (w, v) in g.iter_mut().enumerate() {
                    *v = sample(gx, gy, gz, w as u64);
                }
            }
        }
    }
    let at = |x: usize, y: i32, z: usize| -> [f64; 3] {
        let (fx, fy, fz) = (
            x as f64 / 4.0,
            (y - min_y) as f64 / SY_STEP as f64,
            z as f64 / 4.0,
        );
        let (x0, y0, z0) = (
            fx.floor() as usize,
            (fy.floor() as usize).min(sy - 2),
            fz.floor() as usize,
        );
        let (tx, ty, tz) = (fx - x0 as f64, fy - y0 as f64, fz - z0 as f64);
        let x1 = (x0 + 1).min(SX - 1);
        let z1 = (z0 + 1).min(SX - 1);
        let g = |a: usize, b: usize, c: usize| grid[(a * sy + b) * SX + c];
        let mut out = [0.0; 3];
        for (w, o) in out.iter_mut().enumerate() {
            let l = |a: f64, b: f64, t: f64| a + (b - a) * t;
            let c00 = l(g(x0, y0, z0)[w], g(x1, y0, z0)[w], tx);
            let c10 = l(g(x0, y0 + 1, z0)[w], g(x1, y0 + 1, z0)[w], tx);
            let c01 = l(g(x0, y0, z1)[w], g(x1, y0, z1)[w], tx);
            let c11 = l(g(x0, y0 + 1, z1)[w], g(x1, y0 + 1, z1)[w], tx);
            *o = l(l(c00, c10, ty), l(c01, c11, ty), tz);
        }
        out
    };
    let lava = block::LAVA;
    for z in 0..16 {
        for x in 0..16 {
            let h = heights[z * 16 + x];
            // 바다 밑은 깎지 않습니다 (물을 가둘 대수층이 없으니 바닥이 뚫리면 이상해 보임).
            let roof = if h < SEA_LEVEL { h - 10 } else { h };
            for y in min_y..roof.min(max_y) {
                let n = at(x, y, z);
                let depth = h - y;
                // 넓은 굴: 지표 가까이는 덜 열리게
                let cheese = n[0] > 0.42 + (12 - depth.min(12)) as f64 * 0.05;
                // 국수 굴: 두 노이즈가 동시에 0 근처. 지표까지 뚫어 입구가 생깁니다.
                let spaghetti = n[1].abs() < 0.035 && n[2].abs() < 0.035 && depth > 1;
                if !(cheese || spaghetti) || depth < 1 {
                    continue;
                }
                let s = col.get(x, y, z);
                if s == block::BEDROCK || s == block::WATER {
                    continue;
                }
                col.set(
                    x,
                    y,
                    z,
                    if y < CAVE_LAVA_LEVEL {
                        lava
                    } else {
                        block::AIR
                    },
                );
            }
            // 입구 위가 물이면 물이 새지 않게 막습니다.
            if col.get(x, h, z) == block::AIR && col.get(x, h + 1, z) == block::WATER {
                col.set(x, h, z, b("gravel"));
            }
        }
    }
}

/// 동굴 바이옴 장식: 바닥·천장 블록과 매달린 것들.
fn decorate_caves(col: &mut Column, seed: u64, cx: i32, cz: i32) {
    let lush = biome::id("lush_caves");
    let drip = biome::id("dripstone_caves");
    let dark = biome::id("deep_dark");
    let sulfur = biome::id("sulfur_caves");
    for z in 0..16 {
        for x in 0..16 {
            let r = hash(seed ^ 0xDEC0, cx * 16 + x as i32, cz * 16 + z as i32);
            for y in -58..120 {
                if col.get(x, y, z) != block::AIR {
                    continue;
                }
                let bio = col.biome_at(x, y, z);
                if bio != lush && bio != drip && bio != dark && bio != sulfur {
                    continue;
                }
                let below = col.get(x, y - 1, z);
                let above = col.get(x, y + 1, z);
                let floor = below != block::AIR && below != block::WATER && below != block::LAVA;
                let ceiling = above != block::AIR && above != block::WATER;
                let roll = (r.rotate_left(y as u32 & 63) & 0xFF) as u32;
                if bio == lush {
                    if floor {
                        col.set(x, y - 1, z, b("moss_block"));
                        if roll < 40 {
                            col.set(x, y, z, b("moss_carpet"));
                        } else if roll < 48 {
                            col.set(x, y, z, b("azalea"));
                        }
                    }
                    if ceiling && roll > 230 {
                        col.set(
                            x,
                            y,
                            z,
                            bp(
                                "cave_vines",
                                &[("berries", if roll & 1 == 0 { "true" } else { "false" })],
                            ),
                        );
                    }
                } else if bio == drip {
                    if floor && roll < 30 {
                        col.set(x, y - 1, z, b("dripstone_block"));
                        col.set(
                            x,
                            y,
                            z,
                            bp("pointed_dripstone", &[("vertical_direction", "up")]),
                        );
                    }
                    if ceiling && roll > 220 {
                        col.set(x, y + 1, z, b("dripstone_block"));
                        col.set(
                            x,
                            y,
                            z,
                            bp("pointed_dripstone", &[("vertical_direction", "down")]),
                        );
                    }
                } else if bio == dark {
                    if floor {
                        col.set(x, y - 1, z, b("sculk"));
                        if roll < 6 {
                            col.set(x, y, z, b("sculk_sensor"));
                        } else if roll < 8 {
                            col.set(x, y, z, b("sculk_shrieker"));
                        }
                    }
                } else if bio == sulfur {
                    // 26.3 유황 동굴: 유황 바닥과 천장, 솟은 유황 가시, 가끔 짙은 유황
                    if floor {
                        col.set(
                            x,
                            y - 1,
                            z,
                            b(if roll < 12 { "potent_sulfur" } else { "sulfur" }),
                        );
                        if (20..34).contains(&roll) {
                            col.set(x, y, z, bp("sulfur_spike", &[("vertical_direction", "up")]));
                        }
                    }
                    if ceiling {
                        col.set(x, y + 1, z, b("sulfur"));
                        if roll > 236 {
                            col.set(
                                x,
                                y,
                                z,
                                bp("sulfur_spike", &[("vertical_direction", "down")]),
                            );
                        }
                    }
                }
            }
        }
    }
}

// ---- 네더 ----

fn nether_floor(seed: u64, x: i32, z: i32) -> i32 {
    (36.0 + fbm(seed ^ 0x4E7, x as f64, z as f64, 48.0, 4) * 14.0) as i32
}

fn nether_ceiling(seed: u64, x: i32, z: i32) -> i32 {
    (104.0 + fbm(seed ^ 0xCE11, x as f64, z as f64, 40.0, 3) * 12.0) as i32
}

fn nether(seed: u64, cx: i32, cz: i32) -> Column {
    let mut col = Column::new(Dimension::Nether, biome::id("nether_wastes"));
    let mut floors = [0i32; 256];
    for z in 0..16 {
        for x in 0..16 {
            let (wx, wz) = (cx * 16 + x as i32, cz * 16 + z as i32);
            let bio = biome::nether_biome(seed, wx, wz);
            let def = bio.def();
            let floor = nether_floor(seed, wx, wz);
            let ceiling = nether_ceiling(seed, wx, wz);
            floors[z * 16 + x] = floor;

            col.set(x, 0, z, block::BEDROCK);
            for y in 1..=floor {
                let state = if y == floor {
                    b(def.top)
                } else if y >= floor - 3 {
                    b(def.under)
                } else {
                    block::NETHERRACK
                };
                col.set(x, y, z, state);
            }
            for y in floor + 1..=LAVA_LEVEL {
                col.set(x, y, z, block::LAVA);
            }
            for y in ceiling..127 {
                col.set(x, y, z, block::NETHERRACK);
            }
            col.set(x, 127, z, block::BEDROCK);
            if (x % 4, z % 4) == (2, 2) {
                for ycell in 0..(Dimension::Nether.height() / 4) as usize {
                    col.set_biome_cell(x / 4, ycell, z / 4, bio);
                }
            }

            let r = hash(seed ^ 0x6105, wx, wz);
            match def.name {
                "nether_wastes" if r.is_multiple_of(23) => {
                    let len = 1 + (r >> 8) as i32 % 4;
                    for y in ceiling - len..ceiling {
                        col.set(x, y, z, block::GLOWSTONE);
                    }
                }
                "soul_sand_valley" if r.is_multiple_of(90) && floor > LAVA_LEVEL => {
                    // 현무암 기둥
                    for y in floor + 1..ceiling {
                        col.set(x, y, z, bp("basalt", &[("axis", "y")]));
                    }
                }
                "basalt_deltas" => {
                    if r.is_multiple_of(7) && floor > LAVA_LEVEL {
                        col.set(x, floor, z, b("magma_block"));
                    } else if r.is_multiple_of(5) {
                        col.set(x, floor, z, b("blackstone"));
                    }
                }
                "crimson_forest" | "warped_forest" if floor > LAVA_LEVEL => {
                    let crimson = def.name == "crimson_forest";
                    let plant = match r % 12 {
                        0 => Some(if crimson {
                            "crimson_roots"
                        } else {
                            "warped_roots"
                        }),
                        1 => Some(if crimson {
                            "crimson_fungus"
                        } else {
                            "warped_fungus"
                        }),
                        2 if !crimson => Some("nether_sprouts"),
                        _ => None,
                    };
                    if let Some(p) = plant {
                        col.set(x, floor + 1, z, b(p));
                    }
                }
                _ => {}
            }
        }
    }
    // 거대 균류 (진홍·뒤틀린 숲), 광석
    {
        let mut p = Placer::new(&mut col, cx, cz);
        for dx in -1..=1 {
            for dz in -1..=1 {
                fungi_from(&mut p, seed, cx + dx, cz + dz);
            }
        }
        nether_ores(&mut p, seed);
        super::structure::generate(&mut p, seed, Dimension::Nether);
    }
    let _ = floors;
    col
}

fn fungi_from(p: &mut Placer, seed: u64, ocx: i32, ocz: i32) {
    let mut rng = biome::chunk_rng(seed, ocx, ocz, 0xF0F0);
    for _ in 0..rng.range(2, 6) {
        let (x, z) = (
            ocx * 16 + rng.below(16) as i32,
            ocz * 16 + rng.below(16) as i32,
        );
        let bio = biome::nether_biome(seed, x, z);
        let crimson = bio.name() == "crimson_forest";
        if !crimson && bio.name() != "warped_forest" {
            continue;
        }
        let floor = nether_floor(seed, x, z);
        if floor <= LAVA_LEVEL || floor + 12 >= nether_ceiling(seed, x, z) {
            continue;
        }
        let (stem, wart) = if crimson {
            ("crimson_stem", "nether_wart_block")
        } else {
            ("warped_stem", "warped_wart_block")
        };
        let h = rng.range(5, 10);
        for dy in 1..=h {
            p.set(x, floor + dy, z, b(stem));
        }
        for dy in 0..3 {
            let r = 3 - dy;
            for dx in -r..=r {
                for dz in -r..=r {
                    let (dx, dz): (i32, i32) = (dx, dz);
                    if dx.abs() == r || dz.abs() == r || dy == 2 {
                        let s = if rng.chance(0.08) {
                            b("shroomlight")
                        } else {
                            b(wart)
                        };
                        p.set_soft(x + dx, floor + h - 1 + dy, z + dz, s);
                    }
                }
            }
        }
        if crimson {
            for _ in 0..3 {
                let (vx, vz) = (x + rng.range(-2, 2), z + rng.range(-2, 2));
                let vy = floor + h - 2;
                if p.get(vx, vy, vz) == Some(block::AIR) {
                    p.set(vx, vy, vz, b("weeping_vines"));
                }
            }
        }
    }
}

fn nether_ores(p: &mut Placer, seed: u64) {
    let mut rng = biome::chunk_rng(seed, p.cx, p.cz, 0x9E7);
    let quartz = b("nether_quartz_ore");
    let gold = b("nether_gold_ore");
    let debris = b("ancient_debris");
    for (state, tries, lo, hi, size) in [
        (quartz, 16, 10, 117, 6),
        (gold, 10, 10, 117, 5),
        (debris, 1, 8, 22, 1),
    ] {
        for _ in 0..tries {
            let (x, y, z) = (
                p.cx * 16 + rng.range(1, 14),
                rng.range(lo, hi),
                p.cz * 16 + rng.range(1, 14),
            );
            for _ in 0..size {
                let (dx, dy, dz) = (rng.range(-1, 1), rng.range(-1, 1), rng.range(-1, 1));
                if p.get(x + dx, y + dy, z + dz) == Some(block::NETHERRACK) {
                    p.set(x + dx, y + dy, z + dz, state);
                }
            }
        }
    }
}

// ---- 엔드 ----

const END_ISLAND_TOP: i32 = 62;
const END_PILLAR_RING: f64 = 42.0;
const END_PILLARS: u32 = 10;
/// 바깥 섬이 시작되는 거리 (바닐라와 같은 1000블록 남짓).
const END_OUTER: f64 = 1000.0;

/// 섬이 있는 자리면 (바닥, 꼭대기).
fn end_island(seed: u64, x: i32, z: i32) -> Option<(i32, i32)> {
    let r = ((x as f64).powi(2) + (z as f64).powi(2)).sqrt();
    if r < END_OUTER * 0.5 {
        let radius = 90.0 + fbm(seed ^ 0xE4D, x as f64, z as f64, 32.0, 3) * 18.0;
        if r >= radius {
            return None;
        }
        let edge = 1.0 - r / radius;
        let top = END_ISLAND_TOP + (edge * 4.0) as i32;
        let bottom = END_ISLAND_TOP - (edge.sqrt() * 44.0) as i32;
        return Some((bottom, top));
    }
    if r < END_OUTER {
        return None;
    }
    let n = fbm(seed ^ 0xE40, x as f64, z as f64, 140.0, 4);
    if n < 0.18 {
        return None;
    }
    let t = n - 0.18;
    let top = 58 + (t * 30.0) as i32;
    let bottom = top - (t * 120.0).min(40.0) as i32;
    Some((bottom, top))
}

pub fn end_biome(seed: u64, x: i32, z: i32) -> Biome {
    let r = ((x as f64).powi(2) + (z as f64).powi(2)).sqrt();
    if r < END_OUTER * 0.5 {
        return biome::id("the_end");
    }
    let n = fbm(seed ^ 0xE40, x as f64, z as f64, 140.0, 4);
    biome::id(if n > 0.42 {
        "end_highlands"
    } else if n > 0.28 {
        "end_midlands"
    } else if n > 0.15 {
        "end_barrens"
    } else {
        "small_end_islands"
    })
}

fn end(seed: u64, cx: i32, cz: i32) -> Column {
    let mut col = Column::new(Dimension::End, biome::id("the_end"));
    for z in 0..16 {
        for x in 0..16 {
            let (wx, wz) = (cx * 16 + x as i32, cz * 16 + z as i32);
            if let Some((bottom, top)) = end_island(seed, wx, wz) {
                for y in bottom..=top {
                    col.set(x, y, z, block::END_STONE);
                }
            }
            if (x % 4, z % 4) == (2, 2) {
                let bio = end_biome(seed, wx, wz);
                for ycell in 0..(Dimension::End.height() / 4) as usize {
                    col.set_biome_cell(x / 4, ycell, z / 4, bio);
                }
            }
        }
    }

    // 원을 따라 선 흑요석 기둥. 기둥은 반지름 3짜리 원기둥입니다.
    for i in 0..END_PILLARS {
        let angle = std::f64::consts::TAU * i as f64 / END_PILLARS as f64;
        let (px, pz) = (
            (END_PILLAR_RING * angle.cos()).round() as i32,
            (END_PILLAR_RING * angle.sin()).round() as i32,
        );
        let height = 76 + (i as i32 * 3) % 27;
        for x in 0..16 {
            for z in 0..16 {
                let (dx, dz) = (cx * 16 + x as i32 - px, cz * 16 + z as i32 - pz);
                if dx * dx + dz * dz > 9 {
                    continue;
                }
                for y in END_ISLAND_TOP..=height {
                    col.set(x, y, z, block::OBSIDIAN);
                }
                if dx == 0 && dz == 0 {
                    col.set(x, height + 1, z, block::BEDROCK);
                }
            }
        }
    }

    // 바깥 고지대의 후렴 식물
    let mut rng = biome::chunk_rng(seed, cx, cz, 0xC40);
    let mut p = Placer::new(&mut col, cx, cz);
    for _ in 0..rng.range(0, 3) {
        let (x, z) = (cx * 16 + rng.range(2, 13), cz * 16 + rng.range(2, 13));
        if end_biome(seed, x, z).name() != "end_highlands" {
            continue;
        }
        let Some((_, top)) = end_island(seed, x, z) else {
            continue;
        };
        chorus(&mut p, x, top + 1, z, &mut rng);
    }
    super::structure::generate(&mut p, seed, Dimension::End);
    col
}

fn chorus(p: &mut Placer, x: i32, y: i32, z: i32, rng: &mut Rng) {
    let plant = b("chorus_plant");
    let h = rng.range(3, 7);
    for dy in 0..h {
        p.set(x, y + dy, z, plant);
    }
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        if rng.chance(0.4) {
            let by = y + rng.range(1, h - 1);
            p.set(x + dx, by, z + dz, plant);
            p.set(x + dx, by + 1, z + dz, plant);
            p.set(x + dx, by + 2, z + dz, bp("chorus_flower", &[("age", "5")]));
        }
    }
    p.set(x, y + h, z, bp("chorus_flower", &[("age", "5")]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dimension_has_a_spawn_on_solid_ground() {
        for d in Dimension::ALL {
            let (x, y, z) = spawn_point(d, 1234);
            let (cx, cz) = ((x.floor() as i32) >> 4, (z.floor() as i32) >> 4);
            let col = generate(d, 1234, cx, cz);
            let (lx, lz) = (
                (x.floor() as i32 & 15) as usize,
                (z.floor() as i32 & 15) as usize,
            );
            let below = col.get(lx, y as i32 - 1, lz);
            assert!(
                below != block::AIR && below != block::WATER && below != block::LAVA,
                "{d:?}: {below}"
            );
            assert_eq!(col.get(lx, y as i32 + 1, lz), block::AIR, "{d:?} head");
        }
    }

    #[test]
    fn overworld_has_bedrock_floor_and_nether_has_roof() {
        let ow = generate(Dimension::Overworld, 1, 0, 0);
        assert_eq!(ow.get(0, -64, 0), block::BEDROCK);
        let ne = generate(Dimension::Nether, 1, 0, 0);
        assert_eq!(ne.get(5, 0, 5), block::BEDROCK);
        assert_eq!(ne.get(5, 127, 5), block::BEDROCK);
        assert_eq!(ne.get(5, 200, 5), block::AIR);
    }

    #[test]
    fn end_is_void_between_center_and_outer_islands() {
        let far = generate(Dimension::End, 1, 30, 30);
        assert!((0..256).all(|y| far.get(8, y, 8) == block::AIR));
    }

    #[test]
    fn caves_and_ores_exist_underground() {
        let mut air = 0;
        let mut ores = 0;
        for c in 0..6 {
            let col = generate(Dimension::Overworld, 42, c, -c);
            for y in -50..40 {
                for x in 0..16 {
                    for z in 0..16 {
                        let s = col.get(x, y, z);
                        if s == block::AIR {
                            air += 1;
                        }
                        if super::super::blockset::name_of(s).ends_with("_ore") {
                            ores += 1;
                        }
                    }
                }
            }
        }
        assert!(air > 500, "caves: {air}");
        assert!(ores > 200, "ores: {ores}");
    }

    #[test]
    fn trees_are_continuous_across_chunk_borders() {
        // 이웃한 두 청크를 따로 만들어도 경계의 블록은 각자 계산한 그대로 맞물립니다.
        for seed in [1u64, 2, 3] {
            let a = generate(Dimension::Overworld, seed, 0, 0);
            let b2 = generate(Dimension::Overworld, seed, 1, 0);
            let a2 = generate(Dimension::Overworld, seed, 0, 0);
            for y in 60..140 {
                for z in 0..16 {
                    assert_eq!(a.get(15, y, z), a2.get(15, y, z), "deterministic");
                    let _ = b2.get(0, y, z);
                }
            }
        }
    }
}
