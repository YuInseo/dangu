//! 물과 용암의 흐름 (바닐라 규칙을 줄인 것).
//!
//! `level` 0 = 원천, 1~7 = 흐르는 물(멀수록 큼), 8 이상 = 위에서 떨어지는 물.
//! 한 칸 옆으로 갈 때마다 물은 1, 용암은 2(네더 1)씩 약해지고, 아래가 비면 먼저 떨어집니다.
//! 원천 둘 사이의 물은 새 원천이 되고(물만), 용암이 물에 닿으면 원천은 흑요석, 흐르는 용암은
//! 조약돌이 됩니다.

use super::{NEIGHBORS, add};
use crate::data::{block, blocks};
use crate::packets::world::BlockPos;
use crate::server::Server;
use crate::world::Dimension;
use crate::world::blockset::{b, bp, name_of};

const HORIZONTAL: [(i32, i32, i32); 4] = [(-1, 0, 0), (1, 0, 0), (0, 0, -1), (0, 0, 1)];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fluid {
    Water,
    Lava,
}

fn fluid(state: u32) -> Option<(Fluid, u8)> {
    let f = match name_of(state) {
        "water" => Fluid::Water,
        "lava" => Fluid::Lava,
        _ => return None,
    };
    let level = blocks::property(state, "level")
        .and_then(|l| l.parse().ok())
        .unwrap_or(0);
    Some((f, level))
}

fn state_of(f: Fluid, level: u8) -> u32 {
    bp(
        if f == Fluid::Water { "water" } else { "lava" },
        &[("level", &level.to_string())],
    )
}

fn drop_per_block(f: Fluid, dim: Dimension) -> u8 {
    if f == Fluid::Lava && dim != Dimension::Nether {
        2
    } else {
        1
    }
}

fn delay(f: Fluid, dim: Dimension) -> u64 {
    match f {
        Fluid::Water => 5,
        Fluid::Lava if dim == Dimension::Nether => 10,
        Fluid::Lava => 30,
    }
}

/// 물이 밀고 들어갈 수 있는 블록 (공기, 풀·꽃 같은 것). 원천은 못 밀어냄.
fn can_enter(state: u32) -> bool {
    if state == block::AIR {
        return true;
    }
    let n = name_of(state);
    fluid(state).is_none() && crate::world::features::soft(state) && n != "water" && n != "lava"
}

/// 실질 세기: 떨어지는 물은 원천처럼 0으로 셉니다.
fn eff(level: u8) -> u8 {
    if level >= 8 { 0 } else { level }
}

pub fn tick(server: &Server, dim: Dimension, pos: BlockPos, state: u32) {
    let Some((f, level)) = fluid(state) else {
        return;
    };
    let get = |p: BlockPos| server.world.get(dim, p.0, p.1, p.2);
    let drop = drop_per_block(f, dim);

    // 용암이 물에 닿으면 굳습니다.
    if f == Fluid::Lava {
        for d in [(0, 1, 0), (-1, 0, 0), (1, 0, 0), (0, 0, -1), (0, 0, 1)] {
            if fluid(get(add(pos, d))).is_some_and(|(g, _)| g == Fluid::Water) {
                let hard = if level == 0 {
                    b("obsidian")
                } else {
                    b("cobblestone")
                };
                server.set_block(dim, pos, hard);
                server.play_sound(dim, pos, "block.lava.extinguish");
                return;
            }
        }
    }

    // 1. 흐르는 물은 먹여 주는 이웃을 보고 세기를 다시 정합니다.
    let mut level = level;
    if level != 0 {
        let above = fluid(get(add(pos, (0, 1, 0))));
        let new_level = if above.is_some_and(|(g, _)| g == f) {
            Some(8)
        } else {
            let mut sources = 0;
            let mut best: Option<u8> = None;
            for d in HORIZONTAL {
                if let Some((g, l)) = fluid(get(add(pos, d)))
                    && g == f
                {
                    if l == 0 {
                        sources += 1;
                    }
                    let e = eff(l);
                    best = Some(best.map_or(e, |b: u8| b.min(e)));
                }
            }
            let below = get(add(pos, (0, -1, 0)));
            let below_holds =
                !can_enter(below) && fluid(below).is_none_or(|(g, l)| g == f && l == 0);
            if f == Fluid::Water && sources >= 2 && below_holds {
                Some(0)
            } else {
                best.map(|b| b + drop).filter(|l| *l <= 7)
            }
        };
        match new_level {
            None => {
                server.set_block(dim, pos, block::AIR);
                return;
            }
            Some(l) if l != level => {
                level = l;
                server.set_block(dim, pos, state_of(f, l));
            }
            _ => {}
        }
    }

    // 2. 퍼지기: 아래가 비었으면 떨어지고, 아니면 옆으로.
    let below_pos = add(pos, (0, -1, 0));
    let below = get(below_pos);
    if below_pos.1 >= dim.min_y() && can_enter(below) {
        server.set_block(dim, below_pos, state_of(f, 8));
        server.ticks.schedule(dim, below_pos, delay(f, dim));
        return;
    }
    if fluid(below).is_some_and(|(g, _)| g == f) && level != 0 {
        // 같은 물 위로 떨어지는 중이면 옆으로 퍼지지 않음
        return;
    }
    let next = eff(level) + drop;
    if next > 7 {
        return;
    }
    for d in HORIZONTAL {
        let p = add(pos, d);
        let s = get(p);
        let weaker = fluid(s).is_some_and(|(g, l)| g == f && l != 0 && l < 8 && l > next);
        if can_enter(s) || weaker {
            server.set_block(dim, p, state_of(f, next));
            server.ticks.schedule(dim, p, delay(f, dim));
        } else if f == Fluid::Water && fluid(s).is_some_and(|(g, _)| g == Fluid::Lava) {
            // 물이 용암으로 흘러들면 용암 쪽이 굳음
            server.ticks.schedule(dim, p, 1);
        }
    }
    let _ = NEIGHBORS;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::status::ServerInfo;

    fn server() -> Server {
        Server::new(
            ServerInfo {
                name: "t".into(),
                description: "t".into(),
                max_players: 1,
            },
            3,
        )
    }

    fn run(s: &Server, ticks: u64) {
        for _ in 0..ticks {
            let now = s.ticks.now() + 1;
            super::super::tick(s, now, &[]);
        }
    }

    #[test]
    fn water_flows_down_and_out_then_dries_up() {
        let s = server();
        let d = Dimension::Overworld;
        // 공중의 돌판 위에 원천
        for x in -9..=9 {
            for z in -9..=9 {
                s.world.set(d, x, 199, z, block::STONE);
                for y in 200..203 {
                    s.world.set(d, x, y, z, block::AIR);
                }
            }
        }
        s.set_block(d, (0, 200, 0), state_of(Fluid::Water, 0));
        run(&s, 60);
        let get = |x, z| fluid(s.world.get(d, x, 200, z));
        assert_eq!(get(1, 0), Some((Fluid::Water, 1)));
        assert_eq!(get(0, 3), Some((Fluid::Water, 3)));
        assert_eq!(get(7, 0), Some((Fluid::Water, 7)));
        assert_eq!(get(8, 0), None, "water reaches 7 blocks");
        // 원천을 치우면 말라 버림
        s.set_block(d, (0, 200, 0), block::AIR);
        run(&s, 80);
        assert_eq!(get(3, 0), None);
    }

    #[test]
    fn lava_meeting_water_hardens() {
        let s = server();
        let d = Dimension::Overworld;
        for x in -3..=3 {
            s.world.set(d, x, 199, 0, block::STONE);
            s.world.set(d, x, 200, 0, block::AIR);
        }
        s.set_block(d, (0, 200, 0), state_of(Fluid::Lava, 0));
        s.set_block(d, (1, 200, 0), state_of(Fluid::Water, 0));
        run(&s, 40);
        assert_eq!(name_of(s.world.get(d, 0, 200, 0)), "obsidian");
    }
}
