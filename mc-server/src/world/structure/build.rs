//! 구조물을 짓는 붓. 구조물 전체를 먼저 그려 두고(청크 밖 포함), 울타리·유리판 연결을
//! 맞춘 뒤 이 청크에 들어오는 블록만 씁니다.
//!
//! 좌표는 시작점 기준의 지역 좌표이고, 시작점마다 정한 회전(0~3, 시계 방향 90°씩)을
//! 블록 상태(방향·축·회전·옆면 연결)까지 함께 돌립니다.

use std::collections::HashMap;

use super::{Height, Start};
use crate::data::{block, blocks};
use crate::world::Dimension;
use crate::world::biome::{self, Biome};
use crate::world::blockset::{b, bp, name_of};
use crate::world::features::{self, Placer, Rng};
use crate::world::noise::hash;

pub type Pos = (i32, i32, i32);

pub struct Builder<'p, 'c> {
    p: &'p mut Placer<'c>,
    pub seed: u64,
    pub dim: Dimension,
    pub biome: Biome,
    pub structure: &'static str,
    /// 시작점 (월드 좌표). 지역 (0,0,0)이 여기.
    pub origin: Pos,
    pub rot: u8,
    pub rng: Rng,
    blocks: HashMap<Pos, u32>,
    loot: Vec<(Pos, &'static str)>,
    spawners: Vec<(Pos, &'static str)>,
}

const SIDES: [&str; 4] = ["north", "east", "south", "west"];

/// 방향 이름을 시계 방향으로 `r`번.
fn turn(dir: &str, r: u8) -> &'static str {
    match SIDES.iter().position(|d| *d == dir) {
        Some(i) => SIDES[(i + r as usize) % 4],
        None => match dir {
            "up" => "up",
            "down" => "down",
            _ => "north",
        },
    }
}

/// 블록 상태를 시계 방향으로 `r`번 돌립니다.
pub fn rotate_state(state: u32, r: u8) -> u32 {
    if r == 0 {
        return state;
    }
    let mut s = state;
    if let Some(f) = blocks::property(state, "facing") {
        s = blocks::with_property(s, "facing", turn(f, r));
    }
    if r % 2 == 1
        && let Some(a) = blocks::property(state, "axis")
    {
        s = blocks::with_property(
            s,
            "axis",
            match a {
                "x" => "z",
                "z" => "x",
                o => o,
            },
        );
    }
    if r % 2 == 1
        && let Some(shape) = blocks::property(state, "shape")
    {
        let swapped = match shape {
            "north_south" => Some("east_west"),
            "east_west" => Some("north_south"),
            _ => None,
        };
        if let Some(v) = swapped {
            s = blocks::with_property(s, "shape", v);
        }
    }
    if let Some(rot) = blocks::property(state, "rotation")
        && let Ok(v) = rot.parse::<u8>()
    {
        s = blocks::with_property(s, "rotation", &((v + 4 * r) % 16).to_string());
    }
    // 옆면 속성 (울타리·덩굴·벽·레드스톤): 값들을 함께 돌림
    if blocks::has_property(state, "north") {
        let vals: Vec<&str> = SIDES
            .iter()
            .map(|d| blocks::property(state, d).unwrap_or("false"))
            .collect();
        for (i, d) in SIDES.iter().enumerate() {
            s = blocks::with_property(s, turn(d, r), vals[i]);
        }
    }
    s
}

fn connectable(state: u32) -> Option<&'static str> {
    let n = name_of(state);
    if n.ends_with("_fence") || n == "iron_bars" || n.ends_with("glass_pane") || n == "glass_pane" {
        Some("true")
    } else if n.ends_with("_wall") && blocks::has_property(state, "up") {
        Some("low")
    } else {
        None
    }
}

fn solid_for_connection(state: u32) -> bool {
    if connectable(state).is_some() || name_of(state).ends_with("fence_gate") {
        return true;
    }
    !features::soft(state)
        && state != block::LAVA
        && !name_of(state).contains("torch")
        && !name_of(state).ends_with("_door")
        && !name_of(state).ends_with("_slab")
        && !name_of(state).ends_with("_stairs")
        && !name_of(state).ends_with("_carpet")
}

impl<'p, 'c> Builder<'p, 'c> {
    pub fn begin(p: &'p mut Placer<'c>, seed: u64, dim: Dimension, start: &Start) -> Self {
        let (cx, cz) = start.chunk;
        let mut rng = Rng::new(hash(seed ^ 0xB11D, cx, cz) ^ start.structure.len() as u64);
        let (x, z) = (cx * 16 + 8, cz * 16 + 8);
        let y = match start.kind.height {
            Height::Surface => ground_y(dim, seed, x, z),
            Height::SeaFloor => biome::surface_height(seed, x, z),
            Height::Range(lo, hi) => rng.range(lo, hi),
        };
        let rot = rng.below(4) as u8;
        Self {
            p,
            seed,
            dim,
            biome: start.biome,
            structure: start.structure,
            origin: (x, y, z),
            rot,
            rng,
            blocks: HashMap::new(),
            loot: Vec::new(),
            spawners: Vec::new(),
        }
    }

    /// 지역 좌표 → 월드 좌표.
    pub fn world(&self, l: Pos) -> Pos {
        let (x, z) = match self.rot {
            0 => (l.0, l.2),
            1 => (-l.2, l.0),
            2 => (-l.0, -l.2),
            _ => (l.2, -l.0),
        };
        (self.origin.0 + x, self.origin.1 + l.1, self.origin.2 + z)
    }

    /// 지역 좌표에 블록 (상태는 회전에 맞게 돌림).
    pub fn set(&mut self, l: Pos, state: u32) {
        let w = self.world(l);
        let s = rotate_state(state, self.rot);
        self.blocks.insert(w, s);
    }

    /// 월드 좌표에 그대로 (지형을 따라가는 길 등).
    pub fn set_world(&mut self, w: Pos, state: u32) {
        self.blocks.insert(w, state);
    }

    pub fn get_world(&self, w: Pos) -> Option<u32> {
        self.blocks
            .get(&w)
            .copied()
            .or_else(|| self.p.get(w.0, w.1, w.2))
    }

    pub fn fill(&mut self, a: Pos, c: Pos, state: u32) {
        for x in a.0.min(c.0)..=a.0.max(c.0) {
            for y in a.1.min(c.1)..=a.1.max(c.1) {
                for z in a.2.min(c.2)..=a.2.max(c.2) {
                    self.set((x, y, z), state);
                }
            }
        }
    }

    /// 속이 빈 상자 (벽·바닥·천장).
    pub fn shell(&mut self, a: Pos, c: Pos, state: u32) {
        let (x0, x1, y0, y1, z0, z1) = (
            a.0.min(c.0),
            a.0.max(c.0),
            a.1.min(c.1),
            a.1.max(c.1),
            a.2.min(c.2),
            a.2.max(c.2),
        );
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    let edge = x == x0 || x == x1 || y == y0 || y == y1 || z == z0 || z == z1;
                    self.set((x, y, z), if edge { state } else { block::AIR });
                }
            }
        }
    }

    /// 벽만 (바닥·천장 없이).
    pub fn walls(&mut self, a: Pos, c: Pos, state: u32) {
        let (x0, x1, z0, z1) = (a.0.min(c.0), a.0.max(c.0), a.2.min(c.2), a.2.max(c.2));
        for y in a.1.min(c.1)..=a.1.max(c.1) {
            for x in x0..=x1 {
                self.set((x, y, z0), state);
                self.set((x, y, z1), state);
            }
            for z in z0..=z1 {
                self.set((x0, y, z), state);
                self.set((x1, y, z), state);
            }
        }
    }

    /// 무작위로 섞어 채우기: (상태, 가중치).
    pub fn fill_mix(&mut self, a: Pos, c: Pos, mix: &[(u32, u32)]) {
        for x in a.0.min(c.0)..=a.0.max(c.0) {
            for y in a.1.min(c.1)..=a.1.max(c.1) {
                for z in a.2.min(c.2)..=a.2.max(c.2) {
                    let s = self.rng.weighted(mix);
                    self.set((x, y, z), s);
                }
            }
        }
    }

    /// 바닥 아래를 땅까지 채웁니다 (`max`칸까지). 지역 좌표의 바닥 줄 기준.
    #[allow(clippy::too_many_arguments)]
    pub fn foundation(
        &mut self,
        x0: i32,
        x1: i32,
        z0: i32,
        z1: i32,
        floor_y: i32,
        state: u32,
        max: i32,
    ) {
        for x in x0.min(x1)..=x0.max(x1) {
            for z in z0.min(z1)..=z0.max(z1) {
                let w = self.world((x, floor_y, z));
                let g = self.ground(w.0, w.2);
                for y in (g + 1).max(w.1 - max)..w.1 {
                    self.set_world((w.0, y, w.2), state);
                }
            }
        }
    }

    /// 지역 상자 안을 비웁니다 (지형·나무를 걷어 냄).
    pub fn clear(&mut self, a: Pos, c: Pos) {
        self.fill(a, c, block::AIR);
    }

    /// 그 자리(월드)의 땅 높이.
    pub fn ground(&self, x: i32, z: i32) -> i32 {
        ground_y(self.dim, self.seed, x, z)
    }

    /// 지역 (x, z)의 땅 높이를 지역 y로.
    pub fn ground_local(&self, x: i32, z: i32) -> i32 {
        let w = self.world((x, 0, z));
        self.ground(w.0, w.2) - self.origin.1
    }

    /// 상자: 블록과 전리품 표.
    pub fn chest(&mut self, l: Pos, facing: &str, table: &'static str) {
        self.set(l, bp("chest", &[("facing", facing)]));
        let w = self.world(l);
        self.loot.push((w, table));
    }

    pub fn barrel(&mut self, l: Pos, table: &'static str) {
        self.set(l, bp("barrel", &[("facing", "up")]));
        let w = self.world(l);
        self.loot.push((w, table));
    }

    /// 몹 생성기.
    pub fn spawner(&mut self, l: Pos, entity: &'static str) {
        self.set(l, b("spawner"));
        let w = self.world(l);
        self.spawners.push((w, entity));
    }

    /// 다 그렸으면: 연결을 맞추고 이 청크에 씁니다.
    pub fn finish(self) {
        let Builder {
            p,
            mut blocks,
            loot,
            spawners,
            ..
        } = self;
        let positions: Vec<Pos> = blocks
            .iter()
            .filter(|(_, s)| connectable(**s).is_some())
            .map(|(pos, _)| *pos)
            .collect();
        for pos in positions {
            let state = blocks[&pos];
            let on = connectable(state).unwrap();
            let mut s = state;
            for (dir, (dx, dz)) in [
                ("north", (0, -1)),
                ("east", (1, 0)),
                ("south", (0, 1)),
                ("west", (-1, 0)),
            ] {
                let n = (pos.0 + dx, pos.1, pos.2 + dz);
                let neighbor = blocks.get(&n).copied().or_else(|| p.get(n.0, n.1, n.2));
                let connect = neighbor.is_some_and(solid_for_connection);
                s = blocks::with_property(
                    s,
                    dir,
                    if connect {
                        on
                    } else if on == "low" {
                        "none"
                    } else {
                        "false"
                    },
                );
            }
            blocks.insert(pos, s);
        }
        for (pos, s) in blocks {
            p.set(pos.0, pos.1, pos.2, s);
        }
        for (pos, table) in loot {
            if p.inside(pos.0, pos.2) {
                p.col.loot.push((pos, table));
            }
        }
        for (pos, e) in spawners {
            if p.inside(pos.0, pos.2) {
                p.col.spawners.push((pos, e));
            }
        }
    }
}

/// 차원마다 "땅" 높이 (맨 위 고체).
pub fn ground_y(dim: Dimension, seed: u64, x: i32, z: i32) -> i32 {
    match dim {
        Dimension::Overworld => biome::surface_height(seed, x, z).max(biome::SEA_LEVEL - 1),
        Dimension::Nether => crate::world::terrain::standing_y(dim, seed, x, z).unwrap_or(65) - 1,
        Dimension::End => crate::world::terrain::standing_y(dim, seed, x, z).unwrap_or(61) - 1,
    }
}

/// 자주 쓰는 블록 묶음 (마을·건물의 재료).
#[derive(Clone, Copy)]
pub struct Palette {
    pub planks: u32,
    pub log: u32,
    pub slab: u32,
    pub fence: u32,
    pub door: &'static str,
    pub wall: u32,
    pub foundation: u32,
    pub roof: &'static str,
    pub path: u32,
    pub pane: u32,
    pub light: u32,
}

impl Palette {
    pub fn roof(&self, facing: &str) -> u32 {
        bp(self.roof, &[("facing", facing), ("half", "bottom")])
    }

    pub fn door(&self, facing: &str, upper: bool) -> u32 {
        bp(
            self.door,
            &[
                ("facing", facing),
                ("half", if upper { "upper" } else { "lower" }),
            ],
        )
    }
}

/// 바이옴(또는 마을 종류)에 맞는 재료.
pub fn palette(style: &str) -> Palette {
    let wood = |w: &'static str, stairs: &'static str, door: &'static str| (w, stairs, door);
    let (planks, stairs, door) = match style {
        "desert" => wood("smooth_sandstone", "sandstone_stairs", "oak_door"),
        "savanna" => wood("acacia_planks", "acacia_stairs", "acacia_door"),
        "snowy" | "taiga" => wood("spruce_planks", "spruce_stairs", "spruce_door"),
        "dark" => wood("dark_oak_planks", "dark_oak_stairs", "dark_oak_door"),
        "jungle" => wood("jungle_planks", "jungle_stairs", "jungle_door"),
        "birch" => wood("birch_planks", "birch_stairs", "birch_door"),
        "cherry" => wood("cherry_planks", "cherry_stairs", "cherry_door"),
        "pale" => wood("pale_oak_planks", "pale_oak_stairs", "pale_oak_door"),
        "poplar" => wood("poplar_planks", "poplar_stairs", "oak_door"),
        "mangrove" => wood("mangrove_planks", "mangrove_stairs", "mangrove_door"),
        _ => wood("oak_planks", "oak_stairs", "oak_door"),
    };
    let log = match style {
        "desert" => "cut_sandstone",
        "savanna" => "acacia_log",
        "snowy" | "taiga" => "spruce_log",
        "dark" => "dark_oak_log",
        "jungle" => "jungle_log",
        "birch" => "birch_log",
        "cherry" => "cherry_log",
        "pale" => "pale_oak_log",
        "poplar" => "poplar_log",
        "mangrove" => "mangrove_log",
        _ => "oak_log",
    };
    let fence = match style {
        "desert" => "sandstone_wall",
        "savanna" => "acacia_fence",
        "snowy" | "taiga" => "spruce_fence",
        "dark" => "dark_oak_fence",
        "jungle" => "jungle_fence",
        "birch" => "birch_fence",
        "cherry" => "cherry_fence",
        "pale" => "pale_oak_fence",
        "mangrove" => "mangrove_fence",
        _ => "oak_fence",
    };
    Palette {
        planks: b(planks),
        log: b(log),
        slab: bp(&stairs.replace("_stairs", "_slab"), &[("type", "bottom")]),
        fence: b(fence),
        door,
        wall: match style {
            "desert" => b("sandstone"),
            "savanna" => b("orange_terracotta"),
            "snowy" => b("spruce_planks"),
            _ => b(planks),
        },
        foundation: match style {
            "desert" => b("sandstone"),
            "savanna" => b("terracotta"),
            _ => b("cobblestone"),
        },
        roof: match style {
            "desert" => "smooth_sandstone_stairs",
            "savanna" => "acacia_stairs",
            "snowy" | "taiga" => "spruce_stairs",
            "dark" => "dark_oak_stairs",
            _ => stairs,
        },
        path: b(if style == "desert" {
            "smooth_sandstone"
        } else {
            "dirt_path"
        }),
        pane: b("glass_pane"),
        light: if style == "snowy" || style == "taiga" {
            bp("lantern", &[("hanging", "false")])
        } else {
            bp("torch", &[])
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_turns_facing_axis_and_sides() {
        let s = bp("oak_stairs", &[("facing", "north")]);
        assert_eq!(blocks::property(rotate_state(s, 1), "facing"), Some("east"));
        assert_eq!(
            blocks::property(rotate_state(s, 2), "facing"),
            Some("south")
        );
        let log = bp("oak_log", &[("axis", "x")]);
        assert_eq!(blocks::property(rotate_state(log, 1), "axis"), Some("z"));
        let fence = bp("oak_fence", &[("north", "true")]);
        let r = rotate_state(fence, 1);
        assert_eq!(blocks::property(r, "east"), Some("true"));
        assert_eq!(blocks::property(r, "north"), Some("false"));
    }
}
