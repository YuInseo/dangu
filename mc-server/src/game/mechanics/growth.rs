//! 무작위 틱으로 자라는 것들: 작물, 줄기 열매, 베리 덤불, 선인장·사탕수수·대나무, 묘목 → 나무,
//! 잔디 퍼짐, 농지 마름.

use std::collections::HashMap;

use crate::data::{block, blocks};
use crate::packets::world::BlockPos;
use crate::server::Server;
use crate::world::Dimension;
use crate::world::biome::Tree;
use crate::world::blockset::{b, bp, name_of};
use crate::world::features::{self, Canvas, Rng};

/// 무작위 틱을 받는 블록인가 (섹션을 건너뛸지 빨리 고르려고).
pub fn ticks_randomly(n: &str) -> bool {
    matches!(
        n,
        "wheat"
            | "carrots"
            | "potatoes"
            | "beetroots"
            | "melon_stem"
            | "pumpkin_stem"
            | "sweet_berry_bush"
            | "nether_wart"
            | "cactus"
            | "sugar_cane"
            | "bamboo"
            | "dirt"
            | "farmland"
            | "cocoa"
            | "kelp"
            | "torchflower_crop"
            | "pitcher_crop"
    ) || n.ends_with("_sapling")
        || n == "mangrove_propagule"
}

fn age(state: u32) -> Option<(u8, u8)> {
    let a: u8 = blocks::property(state, "age")?.parse().ok()?;
    let max: u8 = blocks::property_values(state, "age")?.len() as u8 - 1;
    Some((a, max))
}

fn set_age(state: u32, a: u8) -> u32 {
    blocks::with_property(state, "age", &a.to_string())
}

pub fn random_tick(server: &Server, dim: Dimension, pos: BlockPos, state: u32) {
    let n = name_of(state);
    let get = |p: BlockPos| server.world.get(dim, p.0, p.1, p.2);
    let r = crate::game::rand_u32();
    match n {
        "wheat" | "carrots" | "potatoes" | "beetroots" | "nether_wart" | "sweet_berry_bush"
        | "cocoa" | "torchflower_crop" => {
            // 젖은 농지에서 더 잘 자람 (바닐라의 성장 확률을 단순하게)
            let wet = blocks::property(get((pos.0, pos.1 - 1, pos.2)), "moisture")
                .is_some_and(|m| m != "0");
            let chance = if n == "nether_wart" || n == "sweet_berry_bush" {
                10
            } else if wet {
                3
            } else {
                6
            };
            // 길드 땅이면 업그레이드만큼 더 자주 ([`crate::plugins::guild::Guild::growth_multiplier`])
            let boost =
                crate::plugins::guild::growth_at(&server.name, dim, (pos.0 >> 4, pos.2 >> 4));
            if ((r % 10_000) as f64) < 10_000.0 * boost / chance as f64
                && let Some((a, max)) = age(state)
                && a < max
            {
                server.set_block(dim, pos, set_age(state, a + 1));
            }
        }
        "melon_stem" | "pumpkin_stem" => {
            if !r.is_multiple_of(3) {
                return;
            }
            match age(state) {
                Some((a, max)) if a < max => server.set_block(dim, pos, set_age(state, a + 1)),
                Some(_) => {
                    // 다 자란 줄기: 옆의 빈 땅에 열매
                    let dirs = [
                        ((1, 0, 0), "east"),
                        ((-1, 0, 0), "west"),
                        ((0, 0, 1), "south"),
                        ((0, 0, -1), "north"),
                    ];
                    let (d, facing) = dirs[(r >> 4) as usize % 4];
                    let fruit = (pos.0 + d.0, pos.1, pos.2 + d.2);
                    let ground = name_of(get((fruit.0, fruit.1 - 1, fruit.2)));
                    if get(fruit) == block::AIR
                        && matches!(
                            ground,
                            "dirt" | "grass_block" | "farmland" | "coarse_dirt" | "podzol"
                        )
                    {
                        let (fruit_block, attached) = if n == "melon_stem" {
                            ("melon", "attached_melon_stem")
                        } else {
                            ("pumpkin", "attached_pumpkin_stem")
                        };
                        server.set_block(dim, fruit, b(fruit_block));
                        server.set_block(dim, pos, bp(attached, &[("facing", facing)]));
                    }
                }
                None => {}
            }
        }
        "cactus" | "sugar_cane" | "bamboo" | "kelp" => grow_column(server, dim, pos, state, n),
        "dirt" => {
            // 잔디가 옆(위아래 한 칸 포함)에 있고 위가 트였으면 잔디로
            if get((pos.0, pos.1 + 1, pos.2)) != block::AIR || !r.is_multiple_of(4) {
                return;
            }
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if get((pos.0 + dx, pos.1 + dy, pos.2 + dz)) == block::GRASS_BLOCK {
                            server.set_block(dim, pos, block::GRASS_BLOCK);
                            return;
                        }
                    }
                }
            }
        }
        "farmland" => {
            let moist = water_within(server, dim, pos, 4);
            let m: u8 = blocks::property(state, "moisture")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if moist && m < 7 {
                server.set_block(dim, pos, blocks::with_property(state, "moisture", "7"));
            } else if !moist {
                if m > 0 {
                    server.set_block(
                        dim,
                        pos,
                        blocks::with_property(state, "moisture", &(m - 1).to_string()),
                    );
                } else if !crop_above(get((pos.0, pos.1 + 1, pos.2))) {
                    server.set_block(dim, pos, block::DIRT);
                }
            }
        }
        _ if n.ends_with("_sapling") || n == "mangrove_propagule" => {
            if !r.is_multiple_of(7) {
                return;
            }
            let stage: u8 = blocks::property(state, "stage")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            if stage == 0 {
                server.set_block(dim, pos, blocks::with_property(state, "stage", "1"));
            } else {
                grow_tree(server, dim, pos, n, r);
            }
        }
        _ => {}
    }
}

fn crop_above(state: u32) -> bool {
    matches!(
        name_of(state),
        "wheat"
            | "carrots"
            | "potatoes"
            | "beetroots"
            | "melon_stem"
            | "pumpkin_stem"
            | "torchflower_crop"
    )
}

fn water_within(server: &Server, dim: Dimension, pos: BlockPos, r: i32) -> bool {
    for dx in -r..=r {
        for dz in -r..=r {
            for dy in 0..=1 {
                if name_of(server.world.get(dim, pos.0 + dx, pos.1 + dy, pos.2 + dz)) == "water" {
                    return true;
                }
            }
        }
    }
    false
}

/// 선인장·사탕수수(3칸)·대나무(16칸)·켈프는 위로 한 칸씩.
fn grow_column(server: &Server, dim: Dimension, pos: BlockPos, state: u32, n: &str) {
    let max_height = match n {
        "bamboo" => 16,
        "kelp" => 25,
        _ => 3,
    };
    let get = |y: i32| server.world.get(dim, pos.0, y, pos.2);
    let above = get(pos.1 + 1);
    let room = if n == "kelp" {
        name_of(above) == "water"
    } else {
        above == block::AIR
    };
    if !room {
        return;
    }
    let mut height = 1;
    while height < max_height && name_of(get(pos.1 - height)) == n {
        height += 1;
    }
    if height >= max_height {
        return;
    }
    match age(state) {
        Some((a, max)) if a < max && n != "kelp" => {
            server.set_block(dim, pos, set_age(state, a + 1))
        }
        _ => {
            if n == "kelp" {
                server.set_block(dim, pos, b("kelp_plant"));
                server.set_block(dim, (pos.0, pos.1 + 1, pos.2), b("kelp"));
            } else {
                server.set_block(
                    dim,
                    (pos.0, pos.1 + 1, pos.2),
                    blocks::with_property(state, "age", "0"),
                );
                if blocks::has_property(state, "age") {
                    server.set_block(dim, pos, set_age(state, 0));
                }
            }
        }
    }
}

/// 다 만든 월드에 나무를 그리는 바탕: 바뀐 블록을 모았다가 한꺼번에 적용합니다.
struct WorldCanvas<'a> {
    server: &'a Server,
    dim: Dimension,
    changes: HashMap<BlockPos, u32>,
}

impl Canvas for WorldCanvas<'_> {
    fn get(&self, x: i32, y: i32, z: i32) -> Option<u32> {
        Some(
            self.changes
                .get(&(x, y, z))
                .copied()
                .unwrap_or_else(|| self.server.world.get(self.dim, x, y, z)),
        )
    }

    fn set(&mut self, x: i32, y: i32, z: i32, state: u32) {
        self.changes.insert((x, y, z), state);
    }
}

fn grow_tree(server: &Server, dim: Dimension, pos: BlockPos, sapling: &str, r: u32) {
    let kind = match sapling {
        "oak_sapling" if r.is_multiple_of(10) => Tree::FancyOak,
        "oak_sapling" => Tree::Oak,
        "birch_sapling" => Tree::Birch,
        "spruce_sapling" => Tree::Spruce,
        "jungle_sapling" => Tree::Jungle,
        "acacia_sapling" => Tree::Acacia,
        "dark_oak_sapling" => Tree::DarkOak,
        "pale_oak_sapling" => Tree::PaleOak,
        "cherry_sapling" => Tree::Cherry,
        "mangrove_propagule" => Tree::Mangrove,
        "poplar_sapling" => Tree::Poplar,
        _ => return,
    };
    // 자랄 자리가 비어 있는지 (줄기 높이만큼)
    for dy in 1..5 {
        if !features::soft(server.world.get(dim, pos.0, pos.1 + dy, pos.2)) {
            return;
        }
    }
    let mut canvas = WorldCanvas {
        server,
        dim,
        changes: HashMap::new(),
    };
    canvas.set(pos.0, pos.1, pos.2, block::AIR);
    let mut rng = Rng::new(r as u64 ^ ((pos.0 as u64) << 20) ^ pos.2 as u64);
    features::tree(&mut canvas, kind, pos.0, pos.1, pos.2, &mut rng);
    for (p, s) in canvas.changes {
        server.set_block(dim, p, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::status::ServerInfo;

    #[test]
    fn crops_age_and_saplings_become_trees() {
        let s = Server::new(
            ServerInfo {
                name: "t".into(),
                description: "t".into(),
                max_players: 1,
            },
            3,
        );
        let d = Dimension::Overworld;
        let p = (0, 200, 0);
        s.world
            .set(d, 0, 199, 0, bp("farmland", &[("moisture", "7")]));
        s.world.set(d, 0, 200, 0, bp("wheat", &[("age", "0")]));
        for _ in 0..200 {
            random_tick(&s, d, p, s.world.get(d, 0, 200, 0));
        }
        assert_eq!(
            blocks::property(s.world.get(d, 0, 200, 0), "age"),
            Some("7")
        );

        let t = (40, 200, 40);
        s.world.set(d, 40, 199, 40, block::DIRT);
        for y in 200..215 {
            s.world.set(d, 40, y, 40, block::AIR);
        }
        s.world
            .set(d, 40, 200, 40, bp("oak_sapling", &[("stage", "1")]));
        for _ in 0..400 {
            let st = s.world.get(d, t.0, t.1, t.2);
            if name_of(st) != "oak_sapling" {
                break;
            }
            random_tick(&s, d, t, st);
        }
        assert!(
            name_of(s.world.get(d, 40, 201, 40)).ends_with("_log"),
            "grew a tree"
        );
    }
}
