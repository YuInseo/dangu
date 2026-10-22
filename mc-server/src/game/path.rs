//! 길찾기: 블록 격자 위의 A*.
//!
//! 몹이 설 수 있는 칸 = 발과 머리 자리에 부딪히는 블록이 없고(공기·풀·꽃·물), 발밑이 단단하거나
//! 물. 한 번에 옆으로 한 칸, 한 칸 턱은 오르고, 세 칸까지는 뛰어내립니다. 대각선은 양옆이 모두
//! 트였을 때만 (모서리를 파고들지 않게). 용암·불·선인장·가루눈·울타리는 피합니다.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::packets::world::BlockPos;
use crate::server::Server;
use crate::world::Dimension;
use crate::world::blockset::name_of;

/// 한 번에 살펴볼 칸 수의 상한 (몹이 많아도 틱이 밀리지 않게).
const MAX_NODES: usize = 600;
const MAX_DROP: i32 = 3;

fn passable(state: u32) -> bool {
    let n = name_of(state);
    !crate::data::blocks::has_collision(state)
        && !matches!(
            n,
            "lava" | "fire" | "soul_fire" | "sweet_berry_bush" | "cobweb" | "powder_snow"
        )
}

fn standable(state: u32) -> bool {
    let n = name_of(state);
    (crate::data::blocks::has_collision(state) || n == "water")
        && !matches!(n, "lava" | "cactus" | "magma_block" | "campfire" | "fire")
        && !n.ends_with("_fence")
        && !n.ends_with("_wall")
}

pub struct Grid<'a> {
    server: &'a Server,
    dim: Dimension,
    cache: HashMap<BlockPos, u32>,
}

impl<'a> Grid<'a> {
    pub fn new(server: &'a Server, dim: Dimension) -> Self {
        Self {
            server,
            dim,
            cache: HashMap::new(),
        }
    }

    fn get(&mut self, p: BlockPos) -> u32 {
        *self
            .cache
            .entry(p)
            .or_insert_with(|| self.server.world.get(self.dim, p.0, p.1, p.2))
    }

    /// 이 칸에 설 수 있나.
    pub fn can_stand(&mut self, p: BlockPos) -> bool {
        passable(self.get(p))
            && passable(self.get((p.0, p.1 + 1, p.2)))
            && standable(self.get((p.0, p.1 - 1, p.2)))
    }

    fn neighbors(&mut self, p: BlockPos, out: &mut Vec<(BlockPos, u32)>) {
        out.clear();
        for (dx, dz) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let diagonal = dx != 0 && dz != 0;
            if diagonal
                && !(passable(self.get((p.0 + dx, p.1, p.2)))
                    && passable(self.get((p.0 + dx, p.1 + 1, p.2)))
                    && passable(self.get((p.0, p.1, p.2 + dz)))
                    && passable(self.get((p.0, p.1 + 1, p.2 + dz))))
            {
                continue;
            }
            let cost = if diagonal { 14 } else { 10 };
            let flat = (p.0 + dx, p.1, p.2 + dz);
            if self.can_stand(flat) {
                out.push((flat, cost));
                continue;
            }
            // 한 칸 턱 오르기 (머리 위가 트여야)
            let up = (flat.0, flat.1 + 1, flat.2);
            if !diagonal && passable(self.get((p.0, p.1 + 2, p.2))) && self.can_stand(up) {
                out.push((up, cost + 10));
                continue;
            }
            // 뛰어내리기
            if passable(self.get(flat)) && passable(self.get((flat.0, flat.1 + 1, flat.2))) {
                for d in 1..=MAX_DROP {
                    let down = (flat.0, flat.1 - d, flat.2);
                    if self.can_stand(down) {
                        out.push((down, cost + 4 * d as u32));
                        break;
                    }
                    if !passable(self.get(down)) {
                        break;
                    }
                }
            }
        }
    }
}

fn heuristic(a: BlockPos, b: BlockPos) -> u32 {
    let (dx, dy, dz) = ((a.0 - b.0).abs(), (a.1 - b.1).abs(), (a.2 - b.2).abs());
    let (lo, hi) = (dx.min(dz), dx.max(dz));
    (14 * lo + 10 * (hi - lo) + 10 * dy) as u32
}

/// `from`에서 `to`(또는 `within`칸 안)까지의 길. 못 찾으면 가장 가까이 간 곳까지.
pub fn find(
    server: &Server,
    dim: Dimension,
    from: BlockPos,
    to: BlockPos,
    within: i32,
) -> Vec<BlockPos> {
    let mut grid = Grid::new(server, dim);
    let mut open = BinaryHeap::new();
    let mut came: HashMap<BlockPos, BlockPos> = HashMap::new();
    let mut cost: HashMap<BlockPos, u32> = HashMap::new();
    cost.insert(from, 0);
    open.push(Reverse((heuristic(from, to), from)));
    let mut best = (heuristic(from, to), from);
    let mut buf = Vec::new();
    let mut seen = 0;
    while let Some(Reverse((_, p))) = open.pop() {
        seen += 1;
        let h = heuristic(p, to);
        if h < best.0 {
            best = (h, p);
        }
        let close =
            (p.0 - to.0).abs() <= within && (p.2 - to.2).abs() <= within && (p.1 - to.1).abs() <= 1;
        if close || seen > MAX_NODES {
            break;
        }
        let g = cost[&p];
        grid.neighbors(p, &mut buf);
        for &(n, c) in &buf {
            let ng = g + c;
            if cost.get(&n).is_none_or(|&old| ng < old) {
                cost.insert(n, ng);
                came.insert(n, p);
                open.push(Reverse((ng + heuristic(n, to), n)));
            }
        }
    }
    // 가장 가까이 간 곳에서 거꾸로 따라가기
    let mut path = vec![best.1];
    let mut cur = best.1;
    while let Some(&prev) = came.get(&cur) {
        if prev == from {
            break;
        }
        path.push(prev);
        cur = prev;
    }
    path.reverse();
    if path == [from] { Vec::new() } else { path }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::block;
    use crate::packets::status::ServerInfo;

    #[test]
    fn walks_around_a_wall_and_climbs_a_step() {
        let s = Server::new(
            ServerInfo {
                name: "t".into(),
                description: "t".into(),
                max_players: 1,
            },
            3,
        );
        let d = Dimension::Overworld;
        let y = 200;
        for x in -2..=12 {
            for z in -6..=6 {
                s.world.set(d, x, y - 1, z, block::STONE);
                for dy in 0..4 {
                    s.world.set(d, x, y + dy, z, block::AIR);
                }
            }
        }
        // x=5에 벽 (z -4..=4), 끝 쪽은 한 칸 높은 단
        for z in -4..=4 {
            for dy in 0..3 {
                s.world.set(d, 5, y + dy, z, block::STONE);
            }
        }
        for z in -6..=6 {
            s.world.set(d, 10, y, z, block::STONE);
            s.world.set(d, 11, y, z, block::STONE);
        }
        let path = find(&s, d, (0, y, 0), (11, y + 1, 0), 0);
        assert_eq!(path.last(), Some(&(11, y + 1, 0)), "{path:?}");
        assert!(
            path.iter().all(|p| p.0 != 5 || p.2.abs() > 4),
            "went around the wall"
        );
        assert!(path.len() < 30);
    }
}
