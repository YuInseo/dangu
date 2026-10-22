//! 지형 위의 장식: 나무, 풀·꽃, 광석, 눈.
//!
//! 나무처럼 청크 경계를 넘는 것은 "원점 청크"를 정해 결정적으로 만듭니다. 청크 하나를 만들
//! 때 주변 청크들의 나무도 모두 계산하고, 이 청크 안에 들어오는 블록만 씁니다([`Placer`]).
//! 그래서 어느 청크를 먼저 만들든 경계에서 나무가 잘리지 않습니다.

use super::biome::{self, Biome, Plant, SEA_LEVEL, Tree};
use super::blockset::{b, bp, name_of};
use super::noise::hash;
use super::terrain::Column;
use crate::data::block;

/// splitmix64 난수. 같은 씨앗이면 같은 수열.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// [0, n)
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as u32
        }
    }

    /// [lo, hi]
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.below((hi - lo + 1).max(1) as u32) as i32
    }

    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.f64() < p
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u32) as usize]
    }

    /// 가중치 고르기.
    pub fn weighted<T: Copy>(&mut self, items: &[(T, u32)]) -> T {
        let total: u32 = items.iter().map(|(_, w)| w).sum();
        let mut r = self.below(total);
        for &(t, w) in items {
            if r < w {
                return t;
            }
            r -= w;
        }
        items[0].0
    }
}

/// 나무처럼 생성 중에도, 다 만든 월드에서도(묘목이 자랄 때) 그릴 수 있는 바탕.
pub trait Canvas {
    /// 이 자리의 블록. 모르면(그릴 수 없는 곳) None.
    fn get(&self, x: i32, y: i32, z: i32) -> Option<u32>;
    fn set(&mut self, x: i32, y: i32, z: i32, state: u32);
    /// 비켜 줄 수 있는 블록일 때만.
    fn set_soft(&mut self, x: i32, y: i32, z: i32, state: u32) {
        if self.get(x, y, z).is_some_and(soft) {
            self.set(x, y, z, state);
        }
    }
}

impl Canvas for Placer<'_> {
    fn get(&self, x: i32, y: i32, z: i32) -> Option<u32> {
        Placer::get(self, x, y, z)
    }

    fn set(&mut self, x: i32, y: i32, z: i32, state: u32) {
        Placer::set(self, x, y, z, state);
    }
}

/// 한 청크에만 쓰는 붓. 월드 좌표로 받고, 청크 밖은 조용히 버립니다.
pub struct Placer<'a> {
    pub col: &'a mut Column,
    pub cx: i32,
    pub cz: i32,
}

impl<'a> Placer<'a> {
    pub fn new(col: &'a mut Column, cx: i32, cz: i32) -> Self {
        Self { col, cx, cz }
    }

    fn local(&self, x: i32, z: i32) -> Option<(usize, usize)> {
        let (lx, lz) = (x - self.cx * 16, z - self.cz * 16);
        ((0..16).contains(&lx) && (0..16).contains(&lz)).then_some((lx as usize, lz as usize))
    }

    pub fn inside(&self, x: i32, z: i32) -> bool {
        self.local(x, z).is_some()
    }

    /// 청크 밖이면 None.
    pub fn get(&self, x: i32, y: i32, z: i32) -> Option<u32> {
        self.local(x, z).map(|(lx, lz)| self.col.get(lx, y, lz))
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, state: u32) {
        if let Some((lx, lz)) = self.local(x, z) {
            self.col.set(lx, y, lz, state);
        }
    }

    /// 공기·풀·물처럼 비켜 줄 수 있는 블록일 때만 (나뭇잎 등).
    pub fn set_soft(&mut self, x: i32, y: i32, z: i32, state: u32) {
        if self.get(x, y, z).is_some_and(soft) {
            self.set(x, y, z, state);
        }
    }

    /// 상자 채우기 (양 끝 포함).
    pub fn fill(&mut self, a: (i32, i32, i32), c: (i32, i32, i32), state: u32) {
        for x in a.0.min(c.0)..=a.0.max(c.0) {
            for z in a.2.min(c.2)..=a.2.max(c.2) {
                if !self.inside(x, z) {
                    continue;
                }
                for y in a.1.min(c.1)..=a.1.max(c.1) {
                    self.set(x, y, z, state);
                }
            }
        }
    }

    /// 이 칸 아래로 땅이 나올 때까지 채웁니다 (구조물 기초). 최대 `max`칸.
    pub fn foundation(&mut self, x: i32, y: i32, z: i32, state: u32, max: i32) {
        for d in 1..=max {
            match self.get(x, y - d, z) {
                Some(s) if !soft(s) => break,
                Some(_) => self.set(x, y - d, z, state),
                None => break,
            }
        }
    }
}

/// 나무·구조물이 덮어써도 되는 블록.
pub fn soft(state: u32) -> bool {
    if state == block::AIR || state == block::WATER || state == block::SHORT_GRASS {
        return true;
    }
    let n = name_of(state);
    n.ends_with("grass")
        || n.ends_with("fern")
        || n.ends_with("leaves")
        || n == "snow"
        || n == "seagrass"
        || n == "tall_seagrass"
        || n == "leaf_litter"
        || n == "bush"
        || n == "vine"
        || n.ends_with("flower")
        || n == "cave_air"
}

// ---- 나무 ----

struct Wood {
    log: u32,
    leaves: u32,
}

fn wood(log: &str, leaves: &str) -> Wood {
    Wood {
        log: b(log),
        leaves: bp(leaves, &[("persistent", "false")]),
    }
}

fn blob(p: &mut impl Canvas, c: (i32, i32, i32), r: f64, leaves: u32, rng: &mut Rng) {
    let ri = r.ceil() as i32;
    for dx in -ri..=ri {
        for dy in -ri..=ri {
            for dz in -ri..=ri {
                let d = ((dx * dx + dy * dy + dz * dz) as f64).sqrt();
                if d <= r - 0.3 || (d <= r + 0.2 && rng.chance(0.5)) {
                    p.set_soft(c.0 + dx, c.1 + dy, c.2 + dz, leaves);
                }
            }
        }
    }
}

/// 층마다 반지름이 다른 잎 (위에서부터 `radii`).
fn layered(
    p: &mut impl Canvas,
    x: i32,
    top: i32,
    z: i32,
    radii: &[i32],
    leaves: u32,
    rng: &mut Rng,
) {
    for (i, &r) in radii.iter().enumerate() {
        let y = top - i as i32;
        for dx in -r..=r {
            for dz in -r..=r {
                let corner = dx.abs() == r && dz.abs() == r && r > 0;
                if corner && rng.chance(0.6) {
                    continue;
                }
                p.set_soft(x + dx, y, z + dz, leaves);
            }
        }
    }
}

fn trunk(p: &mut impl Canvas, x: i32, y: i32, z: i32, h: i32, log: u32) {
    for dy in 0..h {
        p.set(x, y + dy, z, log);
    }
}

fn trunk2(p: &mut impl Canvas, x: i32, y: i32, z: i32, h: i32, log: u32) {
    for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        trunk(p, x + dx, y, z + dz, h, log);
    }
}

/// 나무 하나. (x, y, z)는 줄기 밑동 (땅 바로 위).
pub fn tree(p: &mut impl Canvas, kind: Tree, x: i32, y: i32, z: i32, rng: &mut Rng) {
    match kind {
        Tree::Oak | Tree::Birch | Tree::TallBirch => {
            let w = match kind {
                Tree::Oak => wood("oak_log", "oak_leaves"),
                _ => wood("birch_log", "birch_leaves"),
            };
            let h = match kind {
                Tree::TallBirch => rng.range(10, 13),
                Tree::Birch => rng.range(5, 7),
                _ => rng.range(4, 6),
            };
            let top = y + h - 1;
            layered(p, x, top + 1, z, &[1, 1, 2, 2], w.leaves, rng);
            trunk(p, x, y, z, h, w.log);
        }
        Tree::FancyOak => {
            let w = wood("oak_log", "oak_leaves");
            let h = rng.range(7, 10);
            for _ in 0..rng.range(2, 4) {
                let by = y + rng.range(h / 2, h - 2);
                let (dx, dz) = (rng.range(-3, 3), rng.range(-3, 3));
                let axis = if dx.abs() > dz.abs() { "x" } else { "z" };
                let steps = dx.abs().max(dz.abs()).max(1);
                for s in 1..=steps {
                    let (bx, bz) = (x + dx * s / steps, z + dz * s / steps);
                    p.set(bx, by + s / 2, bz, bp("oak_log", &[("axis", axis)]));
                }
                blob(p, (x + dx, by + steps / 2 + 1, z + dz), 2.5, w.leaves, rng);
            }
            blob(p, (x, y + h, z), 2.8, w.leaves, rng);
            trunk(p, x, y, z, h, w.log);
        }
        Tree::Spruce | Tree::Pine => {
            let w = wood("spruce_log", "spruce_leaves");
            let h = if kind == Tree::Pine {
                rng.range(8, 11)
            } else {
                rng.range(6, 9)
            };
            let radii: Vec<i32> = if kind == Tree::Pine {
                vec![0, 1, 1, 2, 1]
            } else {
                let mut v = vec![0, 1];
                let mut r = 1;
                for i in 0..h - 3 {
                    r = if i % 2 == 0 { (r + 1).min(3) } else { r - 1 }.max(1);
                    v.push(r);
                }
                v
            };
            layered(p, x, y + h, z, &radii, w.leaves, rng);
            trunk(p, x, y, z, h, w.log);
        }
        Tree::MegaSpruce => {
            let w = wood("spruce_log", "spruce_leaves");
            let h = rng.range(15, 24);
            let cone = rng.range(8, 12);
            for i in 0..cone {
                let r = 1 + i * 4 / cone;
                let yy = y + h - i;
                for dx in -r..=r + 1 {
                    for dz in -r..=r + 1 {
                        let (ex, ez) = (dx as f64 - 0.5, dz as f64 - 0.5);
                        if ex * ex + ez * ez <= (r as f64 + 0.6).powi(2) {
                            p.set_soft(x + dx, yy, z + dz, w.leaves);
                        }
                    }
                }
            }
            trunk2(p, x, y, z, h, w.log);
            // 둘레 흙을 회백토로
            for dx in -2..=3 {
                for dz in -2..=3 {
                    if p.get(x + dx, y - 1, z + dz) == Some(block::GRASS_BLOCK) && rng.chance(0.6) {
                        p.set(x + dx, y - 1, z + dz, b("podzol"));
                    }
                }
            }
        }
        Tree::Jungle => {
            let w = wood("jungle_log", "jungle_leaves");
            let h = rng.range(6, 10);
            layered(p, x, y + h, z, &[1, 2, 2, 2], w.leaves, rng);
            trunk(p, x, y, z, h, w.log);
            vines(p, x, y, z, h, rng);
        }
        Tree::MegaJungle => {
            let w = wood("jungle_log", "jungle_leaves");
            let h = rng.range(15, 25);
            blob(p, (x, y + h, z), 4.0, w.leaves, rng);
            for _ in 0..3 {
                let by = y + rng.range(h / 2, h - 3);
                let (dx, dz) = (rng.range(-4, 4), rng.range(-4, 4));
                blob(p, (x + dx, by, z + dz), 2.5, w.leaves, rng);
                let axis = if dx.abs() > dz.abs() { "x" } else { "z" };
                for s in 1..=3 {
                    p.set(
                        x + dx * s / 3,
                        by - 1,
                        z + dz * s / 3,
                        bp("jungle_log", &[("axis", axis)]),
                    );
                }
            }
            trunk2(p, x, y, z, h, w.log);
            vines(p, x, y, z, h, rng);
        }
        Tree::Acacia => {
            let w = wood("acacia_log", "acacia_leaves");
            let h = rng.range(5, 7);
            let (dx, dz) = *rng.pick(&[(1, 0), (-1, 0), (0, 1), (0, -1)]);
            let bend = h - rng.range(1, 3);
            let (mut cx, mut cz) = (x, z);
            for i in 0..h {
                if i >= bend {
                    cx += dx;
                    cz += dz;
                }
                p.set(cx, y + i, cz, w.log);
            }
            let top = y + h;
            layered(p, cx, top, cz, &[1, 3], w.leaves, rng);
        }
        Tree::DarkOak | Tree::PaleOak => {
            let w = if kind == Tree::DarkOak {
                wood("dark_oak_log", "dark_oak_leaves")
            } else {
                wood("pale_oak_log", "pale_oak_leaves")
            };
            let h = rng.range(6, 9);
            layered(p, x, y + h + 1, z, &[1, 2, 3, 3], w.leaves, rng);
            layered(p, x + 1, y + h + 1, z + 1, &[1, 2, 3, 3], w.leaves, rng);
            trunk2(p, x, y, z, h, w.log);
            if kind == Tree::PaleOak {
                // 잎 아래로 늘어진 창백한 이끼, 드물게 삐걱이 심장
                for _ in 0..rng.range(3, 8) {
                    let (mx, mz) = (x + rng.range(-3, 4), z + rng.range(-3, 4));
                    let my = y + h - 2;
                    if p.get(mx, my, mz) == Some(block::AIR)
                        && p.get(mx, my + 1, mz) == Some(w.leaves)
                    {
                        p.set(mx, my, mz, b("pale_hanging_moss"));
                    }
                }
                if rng.chance(0.1) {
                    p.set(x, y + 2, z, bp("creaking_heart", &[("axis", "y")]));
                }
            }
        }
        Tree::Cherry => {
            let w = wood("cherry_log", "cherry_leaves");
            let h = rng.range(4, 6);
            trunk(p, x, y, z, h, w.log);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if rng.chance(0.55) {
                    let len = rng.range(2, 4);
                    let axis = if dx != 0 { "x" } else { "z" };
                    for s in 1..=len {
                        p.set(
                            x + dx * s,
                            y + h - 2 + s / 2,
                            z + dz * s,
                            bp("cherry_log", &[("axis", axis)]),
                        );
                    }
                    blob(
                        p,
                        (x + dx * len, y + h + len / 2, z + dz * len),
                        2.6,
                        w.leaves,
                        rng,
                    );
                }
            }
            blob(p, (x, y + h + 1, z), 2.8, w.leaves, rng);
        }
        Tree::Mangrove => {
            let w = wood("mangrove_log", "mangrove_leaves");
            let base = y + rng.range(1, 3);
            let h = rng.range(5, 8);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1)] {
                if rng.chance(0.7) {
                    for d in 0..=base - y + 1 {
                        p.set_soft(
                            x + dx * (1 + d / 2),
                            base - d,
                            z + dz * (1 + d / 2),
                            b("mangrove_roots"),
                        );
                    }
                }
            }
            trunk(p, x, base, z, h, w.log);
            blob(p, (x, base + h, z), 3.0, w.leaves, rng);
        }
        Tree::Poplar => {
            // 26.3 포플러: 곧고 가는 줄기, 길쭉한 잎 기둥. 빨강·주황·노랑 중 하나.
            let leaves = bp(
                rng.pick(&[
                    "red_poplar_leaves",
                    "orange_poplar_leaves",
                    "yellow_poplar_leaves",
                ]),
                &[("persistent", "false")],
            );
            let log = b("poplar_log");
            let h = rng.range(8, 12);
            let start = y + rng.range(2, 3);
            for yy in start..=y + h + 1 {
                let r = if yy == y + h + 1 || yy == start { 0 } else { 1 };
                for dx in -r..=r {
                    for dz in -r..=r {
                        p.set_soft(x + dx, yy, z + dz, leaves);
                    }
                }
                if yy > start + 1 && yy < y + h - 1 {
                    for (dx, dz) in [(2, 0), (-2, 0), (0, 2), (0, -2)] {
                        if rng.chance(0.3) {
                            p.set_soft(x + dx, yy, z + dz, leaves);
                        }
                    }
                }
            }
            trunk(p, x, y, z, h, log);
            // 발밑에 떨어진 잎
            for _ in 0..4 {
                let (lx, lz) = (x + rng.range(-3, 3), z + rng.range(-3, 3));
                if p.get(lx, y, lz) == Some(block::AIR)
                    && p.get(lx, y - 1, lz)
                        .is_some_and(|s| s == block::GRASS_BLOCK)
                {
                    p.set(lx, y, lz, leaf_litter(rng));
                }
            }
        }
        Tree::HugeMushroom => {
            let stem = b("mushroom_stem");
            let h = rng.range(5, 8);
            trunk(p, x, y, z, h, stem);
            if rng.chance(0.5) {
                let cap = b("brown_mushroom_block");
                for dx in -3i32..=3 {
                    for dz in -3i32..=3 {
                        if dx.abs() + dz.abs() < 6 {
                            p.set_soft(x + dx, y + h, z + dz, cap);
                        }
                    }
                }
            } else {
                let cap = b("red_mushroom_block");
                for dy in 0..3 {
                    let r = if dy == 2 { 1 } else { 2 };
                    for dx in -r..=r {
                        for dz in -r..=r {
                            let (dx, dz): (i32, i32) = (dx, dz);
                            let edge = dx.abs() == r || dz.abs() == r || dy == 2;
                            if edge {
                                p.set_soft(x + dx, y + h - 2 + dy, z + dz, cap);
                            }
                        }
                    }
                }
            }
        }
    }
    // 줄기 밑은 흙
    if let Some(s) = p.get(x, y - 1, z)
        && (s == block::GRASS_BLOCK || name_of(s) == "podzol" || name_of(s) == "mycelium")
    {
        p.set(x, y - 1, z, block::DIRT);
    }
}

fn vines(p: &mut impl Canvas, x: i32, y: i32, z: i32, h: i32, rng: &mut Rng) {
    for (dx, dz, face) in [
        (1, 0, "west"),
        (-1, 0, "east"),
        (0, 1, "north"),
        (0, -1, "south"),
    ] {
        if !rng.chance(0.5) {
            continue;
        }
        let len = rng.range(2, h - 2);
        for dy in 0..len {
            p.set_soft(
                x + dx,
                y + h - 2 - dy,
                z + dz,
                bp("vine", &[(face, "true")]),
            );
        }
    }
}

fn leaf_litter(rng: &mut Rng) -> u32 {
    let amount = rng.range(1, 4).to_string();
    let facing = *rng.pick(&["north", "south", "east", "west"]);
    bp(
        "leaf_litter",
        &[("segment_amount", &amount), ("facing", facing)],
    )
}

/// 나무 자리: 땅이 흙 계열이고 물 위가 아니면 (높이, 바이옴). 순수 함수라 어느 청크에서
/// 물어도 같은 답입니다.
fn tree_site(seed: u64, x: i32, z: i32) -> Option<(i32, Biome)> {
    let h = biome::surface_height(seed, x, z);
    if h < SEA_LEVEL {
        return None;
    }
    let bio = biome::surface_biome(seed, x, z, h);
    let top = bio.def().top;
    matches!(
        top,
        "grass_block" | "podzol" | "mycelium" | "mud" | "coarse_dirt"
    )
    .then_some((h, bio))
}

/// 청크 (ocx, ocz)를 원점으로 하는 나무들을 `p`에 그립니다.
pub fn trees_from(p: &mut Placer, seed: u64, ocx: i32, ocz: i32) {
    let mut rng = biome::chunk_rng(seed, ocx, ocz, 0x7EE5);
    // 청크 가운데의 바이옴으로 수를 정합니다.
    let (mx, mz) = (ocx * 16 + 8, ocz * 16 + 8);
    let mh = biome::surface_height(seed, mx, mz);
    let def = biome::surface_biome(seed, mx, mz, mh).def();
    if def.trees.is_empty() {
        return;
    }
    let mut count = def.tree_density / 10;
    if rng.below(10) < def.tree_density % 10 {
        count += 1;
    }
    for _ in 0..count {
        let (x, z) = (
            ocx * 16 + rng.below(16) as i32,
            ocz * 16 + rng.below(16) as i32,
        );
        let Some((h, bio)) = tree_site(seed, x, z) else {
            continue;
        };
        if bio.def().trees.is_empty() {
            continue;
        }
        let kind = rng.weighted(bio.def().trees);
        let mut tr = Rng::new(hash(seed ^ 0x7EE6, x, z));
        tree(p, kind, x, h + 1, z, &mut tr);
    }
}

// ---- 풀과 꽃 ----

fn top_of(col: &Column, x: usize, z: usize) -> i32 {
    let d = col.dimension;
    let mut y = d.min_y() + d.height() - 1;
    while y > d.min_y() && col.get(x, y, z) == block::AIR {
        y -= 1;
    }
    y
}

fn double(p: &mut Placer, x: i32, y: i32, z: i32, name: &str) {
    if p.get(x, y + 1, z) == Some(block::AIR) {
        p.set(x, y, z, bp(name, &[("half", "lower")]));
        p.set(x, y + 1, z, bp(name, &[("half", "upper")]));
    }
}

fn near_water(col: &Column, x: usize, y: i32, z: usize) -> bool {
    [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .any(|&(dx, dz)| {
            let (nx, nz) = (x as i32 + dx, z as i32 + dz);
            (0..16).contains(&nx)
                && (0..16).contains(&nz)
                && col.get(nx as usize, y, nz as usize) == block::WATER
        })
}

pub fn plants(p: &mut Placer, seed: u64, biomes: &[Biome; 256]) {
    let (cx, cz) = (p.cx, p.cz);
    let mut rng = biome::chunk_rng(seed, cx, cz, 0x91A7);
    let center = biomes[8 * 16 + 8].def();
    for &(plant, tries) in center.plants {
        for _ in 0..tries {
            let (lx, lz) = (rng.below(16) as usize, rng.below(16) as usize);
            let def = biomes[lz * 16 + lx].def();
            let y = top_of(p.col, lx, lz);
            let (x, z) = (cx * 16 + lx as i32, cz * 16 + lz as i32);
            let ground = p.col.get(lx, y, lz);
            let gname = name_of(ground);
            let above = p.col.get(lx, y + 1, lz);
            let on_soil = matches!(
                gname,
                "grass_block" | "podzol" | "dirt" | "coarse_dirt" | "mycelium" | "mud"
            );
            let on_sand = matches!(gname, "sand" | "red_sand");
            let dry = above == block::AIR;
            match plant {
                Plant::Grass if on_soil && dry && gname != "mycelium" => {
                    p.set(x, y + 1, z, block::SHORT_GRASS)
                }
                Plant::TallGrass if on_soil && dry => double(p, x, y + 1, z, "tall_grass"),
                Plant::Fern if on_soil && dry => {
                    if rng.chance(0.3) {
                        double(p, x, y + 1, z, "large_fern");
                    } else {
                        p.set(x, y + 1, z, b("fern"));
                    }
                }
                Plant::Flowers if on_soil && dry && !def.flowers.is_empty() => {
                    let f = *rng.pick(def.flowers);
                    if f == "pink_petals" || f == "wildflowers" {
                        let amount = rng.range(1, 4).to_string();
                        p.set(x, y + 1, z, bp(f, &[("flower_amount", &amount)]));
                    } else {
                        p.set(x, y + 1, z, b(f));
                    }
                }
                Plant::Wildflowers if on_soil && dry => {
                    let amount = rng.range(1, 4).to_string();
                    p.set(
                        x,
                        y + 1,
                        z,
                        bp("wildflowers", &[("flower_amount", &amount)]),
                    );
                }
                Plant::Sunflowers if on_soil && dry => double(p, x, y + 1, z, "sunflower"),
                Plant::Cactus if on_sand && dry => {
                    let free = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|&(dx, dz)| {
                        p.get(x + dx, y + 1, z + dz).is_none_or(|s| s == block::AIR)
                    });
                    if free {
                        let h = rng.range(1, 3);
                        for dy in 1..=h {
                            p.set(x, y + dy, z, b("cactus"));
                        }
                        if rng.chance(0.25) {
                            p.set(x, y + h + 1, z, b("cactus_flower"));
                        }
                    }
                }
                Plant::DeadBush
                    if (on_sand || gname == "terracotta" || gname == "coarse_dirt") && dry =>
                {
                    p.set(x, y + 1, z, b("dead_bush"))
                }
                Plant::DryGrass if (on_sand || on_soil) && dry => {
                    if rng.chance(0.3) {
                        p.set(x, y + 1, z, b("tall_dry_grass"));
                    } else {
                        p.set(x, y + 1, z, b("short_dry_grass"));
                    }
                }
                Plant::SugarCane if (on_soil || on_sand) && dry && near_water(p.col, lx, y, lz) => {
                    for dy in 1..=rng.range(1, 3) {
                        p.set(x, y + dy, z, b("sugar_cane"));
                    }
                }
                Plant::Pumpkin if on_soil && dry && rng.chance(0.15) => {
                    p.set(x, y + 1, z, b("pumpkin"))
                }
                Plant::Melon if on_soil && dry && rng.chance(0.3) => p.set(x, y + 1, z, b("melon")),
                Plant::SweetBerries if on_soil && dry && rng.chance(0.3) => {
                    p.set(x, y + 1, z, bp("sweet_berry_bush", &[("age", "3")]))
                }
                Plant::Mushrooms if on_soil && dry && rng.chance(0.3) => p.set(
                    x,
                    y + 1,
                    z,
                    b(if rng.chance(0.5) {
                        "brown_mushroom"
                    } else {
                        "red_mushroom"
                    }),
                ),
                Plant::Bamboo if on_soil && dry => {
                    let h = rng.range(5, 12);
                    for dy in 1..=h {
                        let leaves = if dy == h {
                            "large"
                        } else if dy == h - 1 {
                            "small"
                        } else {
                            "none"
                        };
                        p.set(
                            x,
                            y + dy,
                            z,
                            bp("bamboo", &[("leaves", leaves), ("age", "1")]),
                        );
                    }
                }
                Plant::LilyPad if ground == block::WATER && above == block::AIR => {
                    p.set(x, y + 1, z, b("lily_pad"))
                }
                Plant::LeafLitter if on_soil && dry => p.set(x, y + 1, z, leaf_litter(&mut rng)),
                Plant::FireflyBush if on_soil && dry && near_water(p.col, lx, y, lz) => {
                    p.set(x, y + 1, z, b("firefly_bush"))
                }
                Plant::Bush if on_soil && dry => p.set(x, y + 1, z, b("bush")),
                Plant::PaleMoss if on_soil && dry => {
                    p.set(x, y, z, b("pale_moss_block"));
                    p.set(x, y + 1, z, b("pale_moss_carpet"));
                }
                Plant::Seagrass | Plant::Kelp if ground == block::WATER => {
                    // 바닥까지 내려가서
                    let mut fy = y;
                    while fy > -60 && p.col.get(lx, fy, lz) == block::WATER {
                        fy -= 1;
                    }
                    let depth = y - fy;
                    if depth < 2 {
                        continue;
                    }
                    if plant == Plant::Seagrass {
                        p.set(x, fy + 1, z, b("seagrass"));
                    } else {
                        let h = rng.range(2, depth.min(12));
                        for dy in 1..h {
                            p.set(x, fy + dy, z, b("kelp_plant"));
                        }
                        p.set(x, fy + h, z, b("kelp"));
                    }
                }
                _ => {}
            }
        }
    }
}

// ---- 광석 ----

struct Ore {
    name: &'static str,
    tries: u32,
    min_y: i32,
    max_y: i32,
    size: u32,
}

const ORES: &[Ore] = &[
    Ore {
        name: "coal_ore",
        tries: 20,
        min_y: 0,
        max_y: 192,
        size: 14,
    },
    Ore {
        name: "iron_ore",
        tries: 12,
        min_y: -24,
        max_y: 72,
        size: 9,
    },
    Ore {
        name: "copper_ore",
        tries: 14,
        min_y: -16,
        max_y: 112,
        size: 10,
    },
    Ore {
        name: "gold_ore",
        tries: 4,
        min_y: -64,
        max_y: 32,
        size: 8,
    },
    Ore {
        name: "redstone_ore",
        tries: 6,
        min_y: -64,
        max_y: 15,
        size: 8,
    },
    Ore {
        name: "lapis_ore",
        tries: 3,
        min_y: -64,
        max_y: 64,
        size: 7,
    },
    Ore {
        name: "diamond_ore",
        tries: 6,
        min_y: -64,
        max_y: 14,
        size: 6,
    },
    Ore {
        name: "granite",
        tries: 3,
        min_y: 0,
        max_y: 120,
        size: 40,
    },
    Ore {
        name: "diorite",
        tries: 3,
        min_y: 0,
        max_y: 120,
        size: 40,
    },
    Ore {
        name: "andesite",
        tries: 3,
        min_y: 0,
        max_y: 120,
        size: 40,
    },
    Ore {
        name: "tuff",
        tries: 2,
        min_y: -64,
        max_y: 0,
        size: 40,
    },
    Ore {
        name: "dirt",
        tries: 4,
        min_y: 0,
        max_y: 150,
        size: 24,
    },
    Ore {
        name: "gravel",
        tries: 4,
        min_y: -64,
        max_y: 150,
        size: 24,
    },
];

fn replaceable_stone(state: u32) -> bool {
    state == block::STONE
        || state == block::DEEPSLATE
        || matches!(name_of(state), "granite" | "diorite" | "andesite" | "tuff")
}

pub fn ores(p: &mut Placer, seed: u64) {
    let mut rng = biome::chunk_rng(seed, p.cx, p.cz, 0x0AE5);
    for ore in ORES {
        for _ in 0..ore.tries {
            let (x, y, z) = (
                p.cx * 16 + rng.range(2, 13),
                rng.range(ore.min_y, ore.max_y),
                p.cz * 16 + rng.range(2, 13),
            );
            vein(p, ore, (x, y, z), &mut rng);
        }
    }
    // 산에만 나는 에메랄드
    for _ in 0..4 {
        let (x, y, z) = (
            p.cx * 16 + rng.range(0, 15),
            rng.range(-16, 220),
            p.cz * 16 + rng.range(0, 15),
        );
        if biome::surface_height(seed, x, z) > 110 && p.get(x, y, z) == Some(block::STONE) {
            p.set(x, y, z, b("emerald_ore"));
        }
    }
}

fn vein(p: &mut Placer, ore: &Ore, c: (i32, i32, i32), rng: &mut Rng) {
    let (mut x, mut y, mut z) = (c.0 as f64, c.1 as f64, c.2 as f64);
    let r = ((ore.size as f64).cbrt() * 0.7).max(0.8);
    for _ in 0..ore.size.div_ceil(3) {
        let ri = r.ceil() as i32;
        for dx in -ri..=ri {
            for dy in -ri..=ri {
                for dz in -ri..=ri {
                    if (dx * dx + dy * dy + dz * dz) as f64 > r * r {
                        continue;
                    }
                    let (bx, by, bz) = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
                    let Some(s) = p.get(bx, by, bz) else { continue };
                    if !replaceable_stone(s) {
                        continue;
                    }
                    let deep = s == block::DEEPSLATE || s == b("tuff");
                    let name = if ore.name.ends_with("_ore") && deep {
                        format!("deepslate_{}", ore.name)
                    } else {
                        ore.name.to_owned()
                    };
                    p.set(bx, by, bz, b(&name));
                }
            }
        }
        x += rng.f64() * 2.0 - 1.0;
        y += rng.f64() * 2.0 - 1.0;
        z += rng.f64() * 2.0 - 1.0;
    }
}

// ---- 눈 ----

/// 눈 덮인 바이옴: 물은 얼리고, 땅 위에는 눈 한 겹.
pub fn snow(col: &mut Column, biomes: &[Biome; 256]) {
    let snow = b("snow");
    let ice = b("ice");
    let snowy_grass = bp("grass_block", &[("snowy", "true")]);
    for z in 0..16 {
        for x in 0..16 {
            if !biomes[z * 16 + x].def().snowy {
                continue;
            }
            let y = top_of(col, x, z);
            let s = col.get(x, y, z);
            if s == block::WATER {
                if y == SEA_LEVEL - 1 || y == SEA_LEVEL {
                    col.set(x, y, z, ice);
                }
            } else if s == block::GRASS_BLOCK {
                col.set(x, y, z, snowy_grass);
                col.set(x, y + 1, z, snow);
            } else if !soft(s) && s != b("packed_ice") {
                col.set(x, y + 1, z, snow);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_and_bounded() {
        let (mut a, mut b) = (Rng::new(5), Rng::new(5));
        for _ in 0..100 {
            let v = a.range(-3, 7);
            assert_eq!(v, b.range(-3, 7));
            assert!((-3..=7).contains(&v));
        }
    }
}
