//! 몹의 다리: A* 길을 따라 한 틱 걷기, 턱 뛰어오르기, 앞 칸 살피기.

use crate::game::entities::{Entity, TickOut, Vec3};
use crate::game::{path, rand_u32};
use crate::packets::world::BlockPos;
use crate::server::Server;
use crate::world::Dimension;

use super::*;

pub(super) fn block_of(p: Vec3) -> BlockPos {
    (
        p.0.floor() as i32,
        (p.1 + 0.01).floor() as i32,
        p.2.floor() as i32,
    )
}

/// `to` 쪽으로 한 틱 걷습니다: 길을 찾아 그 칸들을 따라가고, 턱 앞에서는 뜁니다. 길이 없고
/// 앞이 벽이면 false.
pub(super) fn walk(
    server: &Server,
    e: &mut Entity,
    m: &mut Mob,
    to: Vec3,
    speed: f64,
    now: u32,
    out: &mut TickOut,
) -> bool {
    let goal = block_of(to);
    let feet = block_of(e.pos);
    let (gx, gz) = (to.0 - e.pos.0, to.2 - e.pos.2);
    let close = gx * gx + gz * gz < 2.0 && (to.1 - e.pos.1).abs() < 1.0;
    if close {
        m.path.clear();
    } else {
        let stale = m.path.is_empty()
            || m.path_goal.is_none_or(|g| {
                (g.0 - goal.0).abs() + (g.1 - goal.1).abs() + (g.2 - goal.2).abs() > 1
            });
        if stale && now >= m.repath_at && out.path_budget > 0 {
            out.path_budget -= 1;
            m.path = path::find(server, e.dimension, feet, goal, 1);
            m.path_goal = Some(goal);
            m.repath_at = now + 10 + rand_u32() % 20;
        }
    }
    // 다다른 칸은 버림
    while let Some(&wp) = m.path.first() {
        let (dx, dz) = (wp.0 as f64 + 0.5 - e.pos.0, wp.2 as f64 + 0.5 - e.pos.2);
        if dx * dx + dz * dz < 0.15 && (e.pos.1 - wp.1 as f64).abs() < 1.1 {
            m.path.remove(0);
        } else {
            break;
        }
    }
    let (aim, climb) = match m.path.first() {
        Some(&wp) => (
            (wp.0 as f64 + 0.5, wp.1 as f64, wp.2 as f64 + 0.5),
            wp.1 > feet.1,
        ),
        None => (to, false),
    };
    let (dx, dz) = (aim.0 - e.pos.0, aim.2 - e.pos.2);
    let len = (dx * dx + dz * dz).sqrt();
    if len < 0.05 {
        stop(e);
        return true;
    }
    let mut dir = (dx / len, dz / len);
    let mut jump = climb && len < 1.5;
    if m.path.is_empty() {
        // 길이 없으면 곧장: 한 칸 턱은 뛰고, 벽이면 x나 z 한쪽으로 비켜 봅니다.
        let tries = [dir, (dir.0.signum(), 0.0), (0.0, dir.1.signum())];
        let free = tries.into_iter().find_map(|(ux, uz)| {
            if ux == 0.0 && uz == 0.0 {
                return None;
            }
            match step(server, e.dimension, e.pos, ux, uz) {
                Step::Free => Some(((ux, uz), false)),
                Step::Jump => Some(((ux, uz), true)),
                Step::Wall => None,
            }
        });
        match free {
            Some((d, j)) => {
                dir = d;
                jump |= j;
            }
            None => {
                stop(e);
                return false;
            }
        }
    }
    if e.on_ground && (jump || e.collided) {
        e.vel.1 = 0.42;
    }
    let s = speed.min(len.max(0.02));
    e.vel.0 = dir.0 * s;
    e.vel.2 = dir.1 * s;
    e.yaw = (-dir.0.atan2(dir.1)).to_degrees() as f32;
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    Free,
    /// 한 칸 턱: 뛰면 넘어감
    Jump,
    Wall,
}

/// (ux, uz) 쪽으로 몸 반쪽(0.3) 너머, 곧 들어갈 칸을 봅니다.
pub(super) fn step(server: &Server, dimension: Dimension, pos: Vec3, ux: f64, uz: f64) -> Step {
    let world = &server.world;
    let here = (pos.0.floor() as i32, pos.2.floor() as i32);
    let ahead = (
        (pos.0 + ux * 0.6).floor() as i32,
        (pos.2 + uz * 0.6).floor() as i32,
    );
    if ahead == here {
        return Step::Free;
    }
    let feet = pos.1.floor() as i32;
    let solid = |dy: i32| world.is_solid(dimension, ahead.0, feet + dy, ahead.1);
    match (solid(0), solid(1), solid(2)) {
        (false, false, _) => Step::Free,
        (true, false, false) => Step::Jump,
        _ => Step::Wall,
    }
}
