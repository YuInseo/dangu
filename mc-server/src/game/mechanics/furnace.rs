//! 화로·용광로·훈연기: 칸 0 재료, 1 연료, 2 결과. 바닐라 굽기 조리법과 연료 시간.
//!
//! 내용물은 상자와 같은 저장소([`crate::game::containers`])의 앞 세 칸에 있고, 굽는 중인
//! 화로만 여기 목록에 올라 매 틱 진행합니다. 불이 켜지면 블록의 `lit`도 켭니다.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::data::{COOKING, blocks, items};
use crate::packets::world::{BlockPos, Item};
use crate::server::{Event, Server};
use crate::world::Dimension;
use crate::world::blockset::name_of;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Cook {
    burn_left: u32,
    burn_total: u32,
    progress: u32,
    total: u32,
}

#[derive(Default)]
pub struct Furnaces {
    active: Mutex<HashMap<(Dimension, BlockPos), Cook>>,
}

/// 화로 종류: 0 화로, 1 용광로, 2 훈연기.
pub fn kind_of(name: &str) -> Option<u8> {
    match name {
        "furnace" => Some(0),
        "blast_furnace" => Some(1),
        "smoker" => Some(2),
        _ => None,
    }
}

/// (결과, 개수, 걸리는 틱).
fn recipe(kind: u8, input: i32) -> Option<(i32, i32, u32)> {
    COOKING
        .iter()
        .find(|(k, ing, ..)| *k == kind && ing.contains(&(input as u16)))
        .map(|&(_, _, res, n, t, _)| (res as i32, n as i32, t as u32))
}

/// 연료로 탈 수 있는 틱 (바닐라 값).
pub fn fuel_ticks(item: i32) -> u32 {
    let n = items::name(item);
    match n {
        "lava_bucket" => 20000,
        "coal_block" => 16000,
        "dried_kelp_block" => 4000,
        "blaze_rod" => 2400,
        "coal" | "charcoal" => 1600,
        "stick" | "bowl" | "dead_bush" => 100,
        "bamboo" | "scaffolding" => 50,
        "crafting_table" | "bookshelf" | "chest" | "trapped_chest" | "barrel" | "lectern"
        | "composter" | "bow" | "crossbow" | "fishing_rod" | "ladder" | "jukebox"
        | "note_block" | "loom" | "cartography_table" | "fletching_table" | "smithing_table" => 300,
        _ if n.ends_with("_log")
            || n.ends_with("_wood")
            || n.ends_with("_planks")
            || n.ends_with("_stem")
            || n.ends_with("_hyphae") =>
        {
            if n.starts_with("crimson") || n.starts_with("warped") {
                0
            } else {
                300
            }
        }
        _ if n.ends_with("_boat") || n.ends_with("_raft") => 1200,
        _ if n.ends_with("_slab") && is_wooden(n) => 150,
        _ if (n.ends_with("_stairs")
            || n.ends_with("_fence")
            || n.ends_with("_fence_gate")
            || n.ends_with("_trapdoor")
            || n.ends_with("_pressure_plate"))
            && is_wooden(n) =>
        {
            300
        }
        _ if n.ends_with("_door") && is_wooden(n) => 200,
        _ if n.ends_with("_button") && is_wooden(n) => 100,
        _ if n.ends_with("_sapling") => 100,
        _ if n.starts_with("wooden_") => 200,
        _ if n.ends_with("_wool") => 100,
        _ if n.ends_with("_carpet") => 67,
        _ if n.ends_with("_sign") => 200,
        _ => 0,
    }
}

fn is_wooden(n: &str) -> bool {
    [
        "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "pale_oak",
        "bamboo", "poplar",
    ]
    .iter()
    .any(|w| n.starts_with(w))
}

impl Furnaces {
    /// 칸이 바뀌었거나 열었을 때: 구울 거리가 있으면 목록에 올립니다.
    pub fn wake(&self, dim: Dimension, pos: BlockPos) {
        self.active.lock().unwrap().entry((dim, pos)).or_default();
    }

    /// 창에 보일 진행 값 (불 남은 시간, 불 전체, 굽기 진행, 굽기 전체).
    pub fn data(&self, dim: Dimension, pos: BlockPos) -> [i16; 4] {
        let c = self
            .active
            .lock()
            .unwrap()
            .get(&(dim, pos))
            .copied()
            .unwrap_or_default();
        [
            c.burn_left as i16,
            c.burn_total.min(i16::MAX as u32) as i16,
            c.progress as i16,
            c.total as i16,
        ]
    }

    pub fn forget(&self, dim: Dimension, pos: BlockPos) {
        self.active.lock().unwrap().remove(&(dim, pos));
    }
}

pub fn tick(server: &Server) {
    let keys: Vec<(Dimension, BlockPos)> = server
        .furnaces
        .active
        .lock()
        .unwrap()
        .keys()
        .copied()
        .collect();
    for (dim, pos) in keys {
        step(server, dim, pos);
    }
}

fn step(server: &Server, dim: Dimension, pos: BlockPos) {
    let state = server.world.get(dim, pos.0, pos.1, pos.2);
    let Some(kind) = kind_of(name_of(state)) else {
        server.furnaces.forget(dim, pos);
        return;
    };
    let before = server
        .furnaces
        .active
        .lock()
        .unwrap()
        .get(&(dim, pos))
        .copied()
        .unwrap_or_default();
    let mut c = before;
    let mut slots = server.containers.get(dim, pos);
    let (input, fuel, output) = (slots[0], slots[1], slots[2]);
    let mut changed = [false; 3];

    let cooking = input
        .and_then(|i| recipe(kind, i.id))
        .filter(|&(res, n, _)| {
            output.is_none_or(|o| {
                o.id == res && o.custom.is_none() && o.count + n <= items::max_stack(res)
            })
        });
    if c.burn_left == 0
        && cooking.is_some()
        && let Some(f) = fuel
        && fuel_ticks(f.id) > 0
    {
        c.burn_total = fuel_ticks(f.id);
        c.burn_left = c.burn_total;
        slots[1] = if items::name(f.id) == "lava_bucket" {
            Some(Item::new(items::by_name("bucket").unwrap_or(f.id), 1))
        } else {
            (f.count > 1).then(|| f.with_count(f.count - 1))
        };
        changed[1] = true;
    }
    if c.burn_left > 0 {
        c.burn_left -= 1;
        match cooking {
            Some((res, n, time)) => {
                c.total = time;
                c.progress += 1;
                if c.progress >= time {
                    c.progress = 0;
                    slots[2] = Some(match output {
                        Some(o) => o.with_count(o.count + n),
                        None => Item::new(res, n),
                    });
                    let i = input.unwrap();
                    slots[0] = (i.count > 1).then(|| i.with_count(i.count - 1));
                    changed[0] = true;
                    changed[2] = true;
                }
            }
            None => c.progress = 0,
        }
    } else {
        c.progress = c.progress.saturating_sub(2);
    }

    for (i, ch) in changed.iter().enumerate() {
        if *ch {
            server.containers.set_slot(dim, pos, i, slots[i]);
            server.publish(Event::ContainerChanged {
                dimension: dim,
                pos,
                slot: i,
                item: slots[i],
            });
        }
    }
    let lit = c.burn_left > 0;
    if blocks::property(state, "lit").is_some_and(|l| (l == "true") != lit) {
        server.set_block(
            dim,
            pos,
            blocks::with_property(state, "lit", &lit.to_string()),
        );
    }
    let idle = c.burn_left == 0 && c.progress == 0;
    {
        let mut active = server.furnaces.active.lock().unwrap();
        if idle {
            active.remove(&(dim, pos));
        } else {
            active.insert((dim, pos), c);
        }
    }
    if c != before {
        server.publish(Event::ContainerData {
            dimension: dim,
            pos,
            values: [
                c.burn_left as i16,
                c.burn_total.min(i16::MAX as u32) as i16,
                c.progress as i16,
                c.total as i16,
            ],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::status::ServerInfo;
    use crate::world::blockset::bp;

    #[test]
    fn furnace_smelts_iron_with_coal() {
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
            .set(d, 0, 200, 0, bp("furnace", &[("lit", "false")]));
        let id = |n| items::by_name(n).unwrap();
        s.containers
            .set_slot(d, p, 0, Some(Item::new(id("raw_iron"), 2)));
        s.containers
            .set_slot(d, p, 1, Some(Item::new(id("coal"), 1)));
        s.furnaces.wake(d, p);
        for _ in 0..450 {
            tick(&s);
        }
        let slots = s.containers.get(d, p);
        assert_eq!(slots[2], Some(Item::new(id("iron_ingot"), 2)));
        assert_eq!(slots[0], None);
        assert_eq!(slots[1], None, "coal burned");
        assert_eq!(fuel_ticks(id("oak_planks")), 300);
        assert_eq!(fuel_ticks(id("crimson_planks")), 0);
    }
}
