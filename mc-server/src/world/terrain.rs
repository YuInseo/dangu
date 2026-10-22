//! 지형 생성. 바닐라 생성기를 흉내 내지 않고, 차원마다 알아보기 쉬운 단순한 지형을 만듭니다.
//!
//! - 오버월드: 언덕진 초원과 바다, 모래 해변, 참나무, 풀. 아래는 돌과 심층암, 바닥은 기반암.
//! - 네더: 네더랙 바닥과 천장, y=31까지 찬 용암 바다, 영혼 모래 지대, 천장의 발광석.
//! - 엔드: 원점의 엔드 돌 섬과 그 둘레의 흑요석 기둥, 나머지는 공허.

use super::Dimension;
use super::noise::{fbm, hash};
use crate::data::block;

pub const SEA_LEVEL: i32 = 63;
pub const LAVA_LEVEL: i32 = 31;

/// 블록 하나하나를 담은 청크 기둥 (16×높이×16).
pub struct Column {
    pub dimension: Dimension,
    blocks: Vec<u32>,
}

impl Column {
    fn new(dimension: Dimension) -> Self {
        Self {
            dimension,
            blocks: vec![block::AIR; dimension.height() as usize * 256],
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
}

/// 청크 (cx, cz)를 만듭니다.
pub fn generate(dimension: Dimension, seed: u64, cx: i32, cz: i32) -> Column {
    let mut col = Column::new(dimension);
    match dimension {
        Dimension::Overworld => overworld(&mut col, seed, cx, cz),
        Dimension::Nether => nether(&mut col, seed, cx, cz),
        Dimension::End => end(&mut col, seed, cx, cz),
    }
    col
}

/// 플레이어가 설 수 있는 가장 높은 블록 바로 위의 y. 스폰 위치를 고를 때 씁니다.
/// 설 자리가 없으면(바다, 용암, 공허) `None`.
pub fn standing_y(dimension: Dimension, seed: u64, x: i32, z: i32) -> Option<i32> {
    match dimension {
        Dimension::Overworld => {
            let h = overworld_height(seed, x, z);
            (h > SEA_LEVEL).then_some(h + 1)
        }
        Dimension::Nether => {
            let floor = nether_floor(seed, x, z);
            (floor > LAVA_LEVEL).then_some(floor + 1)
        }
        Dimension::End => end_island(seed, x, z).map(|(_, top)| top + 1),
    }
}

/// 원점에서 가까운, 설 수 있는 자리 (블록 중앙 좌표).
pub fn spawn_point(dimension: Dimension, seed: u64) -> (f64, f64, f64) {
    // 원점부터 바깥으로 네모나게 돌며 찾습니다.
    for r in 0..128i32 {
        for x in -r..=r {
            for z in -r..=r {
                if x.abs() != r && z.abs() != r {
                    continue;
                }
                if let Some(y) = standing_y(dimension, seed, x, z) {
                    return (x as f64 + 0.5, y as f64, z as f64 + 0.5);
                }
            }
        }
    }
    (0.5, 100.0, 0.5)
}

// ---- 오버월드 ----

fn overworld_height(seed: u64, x: i32, z: i32) -> i32 {
    let (fx, fz) = (x as f64, z as f64);
    let base = fbm(seed, fx, fz, 96.0, 4); // 큰 기복
    let hills = fbm(seed ^ 0xA5A5, fx, fz, 32.0, 3); // 잔 언덕
    let mountains = fbm(seed ^ 0x5A5A, fx, fz, 256.0, 2).max(0.0); // 드문 산
    (SEA_LEVEL as f64 + 2.0 + base * 14.0 + hills * 4.0 + mountains * mountains * 60.0) as i32
}

fn overworld(col: &mut Column, seed: u64, cx: i32, cz: i32) {
    for x in 0..16 {
        for z in 0..16 {
            let (wx, wz) = (cx * 16 + x as i32, cz * 16 + z as i32);
            let h = overworld_height(seed, wx, wz);
            let beach = (SEA_LEVEL - 1..=SEA_LEVEL + 1).contains(&h);
            col.set(x, -64, z, block::BEDROCK);
            for y in -63..=h {
                let state = if y < 0 {
                    block::DEEPSLATE
                } else if y < h - 3 {
                    block::STONE
                } else if beach {
                    block::SAND
                } else if h < SEA_LEVEL {
                    if y == h { block::GRAVEL } else { block::DIRT }
                } else if y == h {
                    block::GRASS_BLOCK
                } else {
                    block::DIRT
                };
                col.set(x, y, z, state);
            }
            for y in h + 1..=SEA_LEVEL {
                col.set(x, y, z, block::WATER);
            }
            if h > SEA_LEVEL + 1 && hash(seed ^ 0x6A55, wx, wz).is_multiple_of(6) {
                col.set(x, h + 1, z, block::SHORT_GRASS);
            }
        }
    }

    // 나무는 잎이 청크 밖으로 나가지 않는 자리에만 심습니다 — 이웃 청크를 몰라도 되도록.
    for x in 2..14 {
        for z in 2..14 {
            let (wx, wz) = (cx * 16 + x as i32, cz * 16 + z as i32);
            if !hash(seed ^ 0x7EE, wx, wz).is_multiple_of(90) {
                continue;
            }
            let h = overworld_height(seed, wx, wz);
            if col.get(x, h, z) != block::GRASS_BLOCK {
                continue;
            }
            tree(col, x, h + 1, z, 4 + (hash(seed, wx, wz) % 3) as i32);
        }
    }
}

fn tree(col: &mut Column, x: usize, base: i32, z: usize, trunk: i32) {
    let top = base + trunk - 1;
    for dy in -2..=1i32 {
        let r: i32 = if dy < 0 { 2 } else { 1 };
        for dx in -r..=r {
            for dz in -r..=r {
                if dy == 1 && dx.abs() == 1 && dz.abs() == 1 {
                    continue; // 꼭대기 모서리는 둥글게
                }
                let (lx, lz) = ((x as i32 + dx) as usize, (z as i32 + dz) as usize);
                if col.get(lx, top + dy, lz) == block::AIR {
                    col.set(lx, top + dy, lz, block::OAK_LEAVES);
                }
            }
        }
    }
    for y in base..=top {
        col.set(x, y, z, block::OAK_LOG);
    }
    col.set(x, base - 1, z, block::DIRT);
}

// ---- 네더 ----

fn nether_floor(seed: u64, x: i32, z: i32) -> i32 {
    (36.0 + fbm(seed ^ 0x4E7, x as f64, z as f64, 48.0, 4) * 14.0) as i32
}

fn nether_ceiling(seed: u64, x: i32, z: i32) -> i32 {
    (104.0 + fbm(seed ^ 0xCE11, x as f64, z as f64, 40.0, 3) * 12.0) as i32
}

fn nether(col: &mut Column, seed: u64, cx: i32, cz: i32) {
    for x in 0..16 {
        for z in 0..16 {
            let (wx, wz) = (cx * 16 + x as i32, cz * 16 + z as i32);
            let floor = nether_floor(seed, wx, wz);
            let ceiling = nether_ceiling(seed, wx, wz);
            let soul = fbm(seed ^ 0x5011, wx as f64, wz as f64, 24.0, 2) > 0.35;

            col.set(x, 0, z, block::BEDROCK);
            for y in 1..=floor {
                let top = y >= floor - 2;
                col.set(
                    x,
                    y,
                    z,
                    if top && soul {
                        block::SOUL_SAND
                    } else {
                        block::NETHERRACK
                    },
                );
            }
            for y in floor + 1..=LAVA_LEVEL {
                col.set(x, y, z, block::LAVA);
            }
            for y in ceiling..127 {
                col.set(x, y, z, block::NETHERRACK);
            }
            col.set(x, 127, z, block::BEDROCK);

            // 천장에 매달린 발광석 덩어리
            if hash(seed ^ 0x6105, wx, wz).is_multiple_of(23) {
                let len = 1 + (hash(seed, wx, wz) % 4) as i32;
                for y in ceiling - len..ceiling {
                    col.set(x, y, z, block::GLOWSTONE);
                }
            }
        }
    }
}

// ---- 엔드 ----

const END_ISLAND_TOP: i32 = 62;
const END_PILLAR_RING: f64 = 42.0;
const END_PILLARS: u32 = 10;

/// 섬이 있는 자리면 (바닥, 꼭대기).
fn end_island(seed: u64, x: i32, z: i32) -> Option<(i32, i32)> {
    let r = ((x * x + z * z) as f64).sqrt();
    let radius = 90.0 + fbm(seed ^ 0xE4D, x as f64, z as f64, 32.0, 3) * 18.0;
    if r >= radius {
        return None;
    }
    let edge = 1.0 - r / radius; // 가운데 1, 가장자리 0
    let top = END_ISLAND_TOP + (edge * 4.0) as i32;
    let bottom = END_ISLAND_TOP - (edge.sqrt() * 44.0) as i32;
    Some((bottom, top))
}

fn end(col: &mut Column, seed: u64, cx: i32, cz: i32) {
    for x in 0..16 {
        for z in 0..16 {
            let (wx, wz) = (cx * 16 + x as i32, cz * 16 + z as i32);
            if let Some((bottom, top)) = end_island(seed, wx, wz) {
                for y in bottom..=top {
                    col.set(x, y, z, block::END_STONE);
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
    fn end_is_void_far_away() {
        let far = generate(Dimension::End, 1, 100, 100);
        assert!((0..256).all(|y| far.get(8, y, 8) == block::AIR));
    }
}
