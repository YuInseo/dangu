//! 네더 차원문: 흑요석 틀(안쪽 2×3 ~ 21×21)에 부싯돌로 불을 붙이면 차원문 블록이 차고,
//! 그 안에 4초 서 있으면(크리에이티브는 바로) 오버월드 ↔ 네더를 오갑니다. 좌표는 8:1로
//! 바뀌고, 도착한 곳 근처에 차원문이 없으면 새로 만듭니다.

use crate::data::block;
use crate::packets::world::BlockPos;
use crate::server::Server;
use crate::world::Dimension;
use crate::world::blockset::{b, bp, name_of};

const MAX_SIZE: i32 = 21;

fn is_obsidian(server: &Server, dim: Dimension, p: BlockPos) -> bool {
    name_of(server.world.get(dim, p.0, p.1, p.2)) == "obsidian"
}

fn is_open(server: &Server, dim: Dimension, p: BlockPos) -> bool {
    let s = server.world.get(dim, p.0, p.1, p.2);
    s == block::AIR || matches!(name_of(s), "fire" | "nether_portal")
}

/// `inside`(틀 안의 빈 칸)에서 틀을 찾아 차원문을 채웁니다. 됐으면 true.
pub fn try_light(server: &Server, dim: Dimension, inside: BlockPos) -> bool {
    if dim == Dimension::End {
        return false;
    }
    for (axis, (ax, az)) in [("x", (1, 0)), ("z", (0, 1))] {
        if let Some((corner, w, h)) = find_frame(server, dim, inside, (ax, az)) {
            let portal = bp("nether_portal", &[("axis", axis)]);
            for i in 0..w {
                for j in 0..h {
                    server.set_block_quiet(
                        dim,
                        (corner.0 + ax * i, corner.1 + j, corner.2 + az * i),
                        portal,
                    );
                }
            }
            server.play_sound(dim, inside, "block.portal.trigger");
            return true;
        }
    }
    false
}

/// 틀 안쪽의 왼쪽 아래 모서리와 너비·높이.
fn find_frame(
    server: &Server,
    dim: Dimension,
    inside: BlockPos,
    (ax, az): (i32, i32),
) -> Option<(BlockPos, i32, i32)> {
    if !is_open(server, dim, inside) {
        return None;
    }
    // 바닥까지 내려감
    let mut p = inside;
    let mut n = 0;
    while is_open(server, dim, (p.0, p.1 - 1, p.2)) {
        p.1 -= 1;
        n += 1;
        if n > MAX_SIZE {
            return None;
        }
    }
    if !is_obsidian(server, dim, (p.0, p.1 - 1, p.2)) {
        return None;
    }
    // 왼쪽 끝으로
    let mut left = p;
    for _ in 0..MAX_SIZE {
        let next = (left.0 - ax, left.1, left.2 - az);
        if !is_open(server, dim, next) {
            break;
        }
        left = next;
    }
    if !is_obsidian(server, dim, (left.0 - ax, left.1, left.2 - az)) {
        return None;
    }
    let mut w = 0;
    while w <= MAX_SIZE && is_open(server, dim, (left.0 + ax * w, left.1, left.2 + az * w)) {
        w += 1;
    }
    let mut h = 0;
    while h <= MAX_SIZE && is_open(server, dim, (left.0, left.1 + h, left.2)) {
        h += 1;
    }
    if !(2..=MAX_SIZE).contains(&w) || !(3..=MAX_SIZE).contains(&h) {
        return None;
    }
    // 틀 전체 확인
    for i in 0..w {
        for j in 0..h {
            if !is_open(server, dim, (left.0 + ax * i, left.1 + j, left.2 + az * i)) {
                return None;
            }
        }
        if !is_obsidian(server, dim, (left.0 + ax * i, left.1 - 1, left.2 + az * i))
            || !is_obsidian(server, dim, (left.0 + ax * i, left.1 + h, left.2 + az * i))
        {
            return None;
        }
    }
    for j in 0..h {
        if !is_obsidian(server, dim, (left.0 - ax, left.1 + j, left.2 - az))
            || !is_obsidian(server, dim, (left.0 + ax * w, left.1 + j, left.2 + az * w))
        {
            return None;
        }
    }
    Some((left, w, h))
}

/// 차원문을 지나 도착할 자리 (도착 차원, 발 위치). 근처 차원문이 없으면 만듭니다.
pub fn destination(
    server: &Server,
    from: Dimension,
    pos: (f64, f64, f64),
) -> (Dimension, (f64, f64, f64)) {
    let (to, scale) = match from {
        Dimension::Nether => (Dimension::Overworld, 8.0),
        _ => (Dimension::Nether, 1.0 / 8.0),
    };
    let (x, z) = (
        (pos.0 * scale).floor() as i32,
        (pos.2 * scale).floor() as i32,
    );
    // 근처(±12)의 차원문 찾기: 청크를 통째로 받아 훑음
    let (y_lo, y_hi) = if to == Dimension::Nether {
        (32, 120)
    } else {
        (-50, 250)
    };
    let mut best: Option<(i32, BlockPos)> = None;
    for cx in (x - 12) >> 4..=(x + 12) >> 4 {
        for cz in (z - 12) >> 4..=(z + 12) >> 4 {
            let col = server.world.chunk(to, cx, cz);
            for lx in 0..16 {
                for lz in 0..16 {
                    let (wx, wz) = (cx * 16 + lx as i32, cz * 16 + lz as i32);
                    let d = (wx - x).abs().max((wz - z).abs());
                    if d > 12 {
                        continue;
                    }
                    for y in y_lo..y_hi {
                        if name_of(col.get(lx, y, lz)) == "nether_portal"
                            && name_of(col.get(lx, y - 1, lz)) != "nether_portal"
                            && best.is_none_or(|(bd, _)| d < bd)
                        {
                            best = Some((d, (wx, y, wz)));
                        }
                    }
                }
            }
        }
    }
    if let Some((_, p)) = best {
        return (to, (p.0 as f64 + 0.5, p.1 as f64, p.2 as f64 + 0.5));
    }
    // 없으면 새로: 설 자리를 찾아 그 위에 틀과 문을 세움
    let y = match crate::world::terrain::standing_y(to, server.seed, x, z) {
        Some(y) if y < y_hi - 6 => y,
        _ => {
            if to == Dimension::Nether {
                70
            } else {
                80
            }
        }
    };
    build(server, to, (x, y, z));
    (to, (x as f64 + 1.0, y as f64, z as f64 + 0.5))
}

/// (x, y, z)를 바닥으로 4×5 틀과 2×3 차원문, 발밑 흑요석 받침.
fn build(server: &Server, dim: Dimension, base: BlockPos) {
    let obsidian = b("obsidian");
    let portal = bp("nether_portal", &[("axis", "x")]);
    for i in -1..=4 {
        for dz in -1..=1 {
            server.set_block_quiet(dim, (base.0 + i - 1, base.1 - 1, base.2 + dz), obsidian);
        }
    }
    for i in 0..4 {
        for j in 0..5 {
            let p = (base.0 + i - 1, base.1 + j - 1, base.2);
            let edge = i == 0 || i == 3 || j == 0 || j == 4;
            server.set_block_quiet(dim, p, if edge { obsidian } else { portal });
        }
    }
    // 둘레를 비워 나올 수 있게
    for i in -1..=4 {
        for j in 0..3 {
            for dz in [-1, 1] {
                server.set_block_quiet(dim, (base.0 + i - 1, base.1 + j, base.2 + dz), block::AIR);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::status::ServerInfo;

    #[test]
    fn frame_lights_and_leads_to_the_nether() {
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
        // 4×5 틀 (안쪽 2×3)
        for i in 0..4 {
            for j in 0..5 {
                let edge = i == 0 || i == 3 || j == 0 || j == 4;
                s.world.set(
                    d,
                    i,
                    y + j,
                    0,
                    if edge { b("obsidian") } else { block::AIR },
                );
            }
        }
        assert!(!try_light(&s, d, (5, y + 1, 0)), "not inside a frame");
        assert!(try_light(&s, d, (1, y + 1, 0)));
        assert_eq!(name_of(s.world.get(d, 2, y + 3, 0)), "nether_portal");
        let (to, pos) = destination(&s, d, (16.5, 200.0, 8.5));
        assert_eq!(to, Dimension::Nether);
        let (x, z) = (pos.0.floor() as i32, pos.2.floor() as i32);
        assert!((x - 2).abs() <= 2 && (z - 1).abs() <= 2, "{pos:?}");
        // 다시 가면 같은 문을 찾음
        let (_, again) = destination(&s, d, (16.5, 200.0, 8.5));
        assert!(
            (again.0 - pos.0).abs() <= 1.0 && again.1 == pos.1,
            "{again:?} vs {pos:?}"
        );
    }
}
