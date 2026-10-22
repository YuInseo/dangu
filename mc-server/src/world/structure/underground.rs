//! 지하 구조물: 폐광, 요새(엔드 차원문 방), 시련의 회당, 고대 도시.

use super::build::{Builder, Pos};
use crate::data::block;
use crate::world::blockset::{b, bp};

/// 축 방향 (지역 좌표).
const DIRS: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];

fn left(d: (i32, i32)) -> (i32, i32) {
    (d.1, -d.0)
}

fn right(d: (i32, i32)) -> (i32, i32) {
    (-d.1, d.0)
}

/// 폐광: 갈라지며 뻗는 굴, 기둥과 들보, 레일, 거미줄, 상자.
pub fn mineshaft(bd: &mut Builder) {
    let mesa = bd.structure == "mineshaft_mesa";
    let planks = b(if mesa {
        "dark_oak_planks"
    } else {
        "oak_planks"
    });
    let fence = b(if mesa { "dark_oak_fence" } else { "oak_fence" });
    let mut queue: Vec<(Pos, (i32, i32), u32)> = DIRS.iter().map(|&d| ((0, 0, 0), d, 0)).collect();
    // 가운데 방
    bd.fill((-4, 0, -4), (4, 4, 4), block::AIR);
    bd.fill((-4, -1, -4), (4, -1, 4), b("dirt"));
    let mut pieces = 0;
    while let Some((start, dir, depth)) = queue.pop() {
        pieces += 1;
        if pieces > 28 || depth > 5 {
            continue;
        }
        let len = bd.rng.range(8, 20);
        let (sx, sy, sz) = (start.0 + dir.0 * 5, start.1, start.2 + dir.1 * 5);
        let end = (sx + dir.0 * len, sy, sz + dir.1 * len);
        if end.0.abs() > 56 || end.2.abs() > 56 {
            continue;
        }
        let side = (dir.1.abs(), dir.0.abs());
        for t in 0..=len {
            let (x, z) = (sx + dir.0 * t, sz + dir.1 * t);
            for o in -1..=1 {
                let (cx, cz) = (x + side.0 * o, z + side.1 * o);
                bd.fill((cx, sy, cz), (cx, sy + 2, cz), block::AIR);
                if bd.rng.chance(0.08) {
                    bd.set((cx, sy + 2, cz), b("cobweb"));
                }
                // 바닥이 비어 있으면 판자로 다리
                let floor = if bd.rng.chance(0.15) {
                    planks
                } else {
                    b("dirt")
                };
                bd.set((cx, sy - 1, cz), floor);
            }
            let shape = if dir.0 == 0 {
                "north_south"
            } else {
                "east_west"
            };
            if bd.rng.chance(0.7) {
                bd.set((x, sy, z), bp("rail", &[("shape", shape)]));
            }
            if t % 4 == 0 {
                for o in [-1, 1] {
                    let (cx, cz) = (x + side.0 * o, z + side.1 * o);
                    bd.fill((cx, sy, cz), (cx, sy + 1, cz), fence);
                }
                for o in -1..=1 {
                    bd.set((x + side.0 * o, sy + 2, z + side.1 * o), planks);
                }
                if bd.rng.chance(0.1) {
                    bd.set(
                        (x, sy + 1, z),
                        bp(
                            "wall_torch",
                            &[("facing", if dir.0 == 0 { "east" } else { "south" })],
                        ),
                    );
                }
            }
        }
        if bd.rng.chance(0.18) {
            let t = bd.rng.range(1, len);
            let (x, z) = (sx + dir.0 * t + side.0, sz + dir.1 * t + side.1);
            bd.chest((x, sy, z), "north", "chests/abandoned_mineshaft");
        }
        if bd.rng.chance(0.05) {
            let t = bd.rng.range(1, len);
            bd.spawner((sx + dir.0 * t, sy + 1, sz + dir.1 * t), "cave_spider");
        }
        let next = (
            end.0 - dir.0 * 4,
            sy + bd.rng.range(-1, 1),
            end.2 - dir.1 * 4,
        );
        for d in [dir, left(dir), right(dir)] {
            if bd.rng.chance(if d == dir { 0.7 } else { 0.4 }) {
                queue.push((next, d, depth + 1));
            }
        }
    }
}

fn brick_mix(bd: &Builder) -> [(u32, u32); 3] {
    let _ = bd;
    [
        (b("stone_bricks"), 6),
        (b("mossy_stone_bricks"), 2),
        (b("cracked_stone_bricks"), 2),
    ]
}

/// 요새: 돌벽돌 복도로 이어진 방들, 도서관, 그리고 엔드 차원문 방.
pub fn stronghold(bd: &mut Builder) {
    let mix = brick_mix(bd);
    // 계단 방 (시작)
    bd.fill_mix((-3, -1, -3), (3, 5, 3), &mix);
    bd.fill((-2, 0, -2), (2, 4, 2), block::AIR);
    let mut queue: Vec<(Pos, (i32, i32), u32)> = DIRS.iter().map(|&d| ((0, 0, 0), d, 0)).collect();
    let mut pieces = 0;
    let mut portal = false;
    while let Some((start, dir, depth)) = queue.pop() {
        pieces += 1;
        if pieces > 22 {
            break;
        }
        let len = bd.rng.range(6, 14);
        let side = (dir.1.abs(), dir.0.abs());
        let (sx, sy, sz) = (start.0 + dir.0 * 4, start.1, start.2 + dir.1 * 4);
        let end = (sx + dir.0 * len, sy, sz + dir.1 * len);
        if end.0.abs() > 52 || end.2.abs() > 52 {
            continue;
        }
        for t in 0..=len {
            let (x, z) = (sx + dir.0 * t, sz + dir.1 * t);
            for o in -2..=2 {
                let (cx, cz) = (x + side.0 * o, z + side.1 * o);
                for y in -1..=4 {
                    let edge = o.abs() == 2 || y == -1 || y == 4;
                    let s = if edge {
                        bd.rng.weighted(&mix)
                    } else {
                        block::AIR
                    };
                    bd.set((cx, sy + y, cz), s);
                }
            }
            if t == 0 {
                let facing = match dir {
                    (0, 1) => "south",
                    (1, 0) => "east",
                    (0, -1) => "north",
                    _ => "west",
                };
                bd.set(
                    (x, sy, z),
                    bp("iron_door", &[("facing", facing), ("half", "lower")]),
                );
                bd.set(
                    (x, sy + 1, z),
                    bp("iron_door", &[("facing", facing), ("half", "upper")]),
                );
            }
            if t % 6 == 3 && bd.rng.chance(0.5) {
                bd.set((x + side.0, sy + 2, z + side.1), b("torch"));
            }
        }
        if bd.rng.chance(0.25) {
            bd.chest(
                (end.0 + side.0, sy, end.2 + side.1),
                "north",
                "chests/stronghold_corridor",
            );
        }
        // 끝에 방
        let room = (end.0 + dir.0 * 5, sy, end.2 + dir.1 * 5);
        let kind = if !portal && (depth >= 2 || pieces > 14) {
            portal = true;
            0
        } else {
            bd.rng.range(1, 4)
        };
        match kind {
            0 => portal_room(bd, room, &mix),
            1 => library(bd, room, &mix),
            2 => crossing(bd, room, &mix),
            _ => {
                bd.fill_mix(
                    (room.0 - 4, sy - 1, room.2 - 4),
                    (room.0 + 4, sy + 5, room.2 + 4),
                    &mix,
                );
                bd.fill(
                    (room.0 - 3, sy, room.2 - 3),
                    (room.0 + 3, sy + 4, room.2 + 3),
                    block::AIR,
                );
                bd.fill(
                    (room.0 - 1, sy, room.2 - 1),
                    (room.0 + 1, sy + 2, room.2 + 1),
                    b("iron_bars"),
                );
            }
        }
        if depth < 4 {
            for d in [dir, left(dir), right(dir)] {
                if bd.rng.chance(0.55) {
                    queue.push(((room.0, sy, room.2), d, depth + 1));
                }
            }
        }
    }
}

fn library(bd: &mut Builder, c: Pos, mix: &[(u32, u32)]) {
    bd.fill_mix(
        (c.0 - 6, c.1 - 1, c.2 - 6),
        (c.0 + 6, c.1 + 9, c.2 + 6),
        mix,
    );
    bd.fill(
        (c.0 - 5, c.1, c.2 - 5),
        (c.0 + 5, c.1 + 8, c.2 + 5),
        block::AIR,
    );
    for x in (c.0 - 4..=c.0 + 4).step_by(2) {
        bd.fill((x, c.1, c.2 - 4), (x, c.1 + 3, c.2 + 4), b("bookshelf"));
        bd.fill((x, c.1, c.2), (x, c.1 + 1, c.2), block::AIR);
    }
    bd.fill(
        (c.0 - 5, c.1 + 4, c.2 - 5),
        (c.0 + 5, c.1 + 4, c.2 - 4),
        b("oak_planks"),
    );
    bd.chest(
        (c.0 - 5, c.1 + 5, c.2 - 5),
        "south",
        "chests/stronghold_library",
    );
    bd.chest(
        (c.0 + 5, c.1, c.2 + 5),
        "north",
        "chests/stronghold_library",
    );
    bd.set((c.0, c.1 + 7, c.2), bp("cobweb", &[]));
}

fn crossing(bd: &mut Builder, c: Pos, mix: &[(u32, u32)]) {
    bd.fill_mix(
        (c.0 - 5, c.1 - 1, c.2 - 5),
        (c.0 + 5, c.1 + 6, c.2 + 5),
        mix,
    );
    bd.fill(
        (c.0 - 4, c.1, c.2 - 4),
        (c.0 + 4, c.1 + 5, c.2 + 4),
        block::AIR,
    );
    bd.fill(
        (c.0 - 1, c.1, c.2 - 1),
        (c.0 + 1, c.1 + 5, c.2 + 1),
        b("smooth_stone"),
    );
    bd.fill(
        (c.0 - 4, c.1 + 3, c.2 - 4),
        (c.0 + 4, c.1 + 3, c.2 - 2),
        b("smooth_stone_slab"),
    );
    bd.chest(
        (c.0 + 3, c.1, c.2 + 3),
        "west",
        "chests/stronghold_crossing",
    );
    bd.set((c.0 - 3, c.1 + 1, c.2 + 3), b("torch"));
}

/// 엔드 차원문 방: 용암 웅덩이 위 계단, 12개의 차원문 틀(각각 10% 확률로 눈), 좀벌레 생성기.
fn portal_room(bd: &mut Builder, c: Pos, mix: &[(u32, u32)]) {
    let (x0, z0) = (c.0 - 5, c.2 - 8);
    bd.fill_mix((x0, c.1 - 1, z0), (x0 + 10, c.1 + 7, z0 + 15), mix);
    bd.fill(
        (x0 + 1, c.1, z0 + 1),
        (x0 + 9, c.1 + 6, z0 + 14),
        block::AIR,
    );
    // 용암 웅덩이와 계단
    bd.fill(
        (x0 + 4, c.1 - 1, z0 + 8),
        (x0 + 6, c.1 - 1, z0 + 12),
        block::LAVA,
    );
    for (i, z) in (z0 + 3..z0 + 7).enumerate() {
        bd.fill(
            (x0 + 4, c.1, z),
            (x0 + 6, c.1 + i as i32, z),
            b("stone_bricks"),
        );
    }
    bd.spawner((x0 + 5, c.1 + 4, z0 + 6), "silverfish");
    // 틀: 3×3 안쪽을 둘러싼 12개, 모두 안쪽을 봄
    let (fx, fz, fy) = (x0 + 4, z0 + 9, c.1 + 3);
    bd.fill(
        (fx - 1, fy - 1, fz - 1),
        (fx + 3, fy - 1, fz + 3),
        b("stone_bricks"),
    );
    let mut frames: Vec<(i32, i32, &str)> = Vec::new();
    for i in 0..3 {
        frames.push((fx + i, fz - 1, "south"));
        frames.push((fx + i, fz + 3, "north"));
        frames.push((fx - 1, fz + i, "east"));
        frames.push((fx + 3, fz + i, "west"));
    }
    for (x, z, facing) in frames {
        let eye = if bd.rng.chance(0.1) { "true" } else { "false" };
        bd.set(
            (x, fy, z),
            bp("end_portal_frame", &[("facing", facing), ("eye", eye)]),
        );
    }
    for (x, z) in [
        (x0 + 1, z0 + 1),
        (x0 + 9, z0 + 1),
        (x0 + 1, z0 + 14),
        (x0 + 9, z0 + 14),
    ] {
        bd.set((x, c.1 + 2, z), b("torch"));
    }
}

/// 시련의 회당: 응회암 벽돌 큰 방과 곁방들, 시련 생성기, 금고, 보급품 상자.
pub fn trial_chambers(bd: &mut Builder) {
    let tuff = b("tuff_bricks");
    let polished = b("polished_tuff");
    let copper = b("waxed_copper_block");
    let chiseled = b("waxed_chiseled_copper");
    let n = 9;
    bd.fill((-n - 1, -1, -n - 1), (n + 1, 9, n + 1), tuff);
    bd.fill((-n, 0, -n), (n, 8, n), block::AIR);
    bd.fill((-n, -1, -n), (n, -1, n), polished);
    for x in -n..=n {
        for z in -n..=n {
            if (x + z).rem_euclid(4) == 0 && x.abs() < n - 1 && z.abs() < n - 1 {
                bd.set((x, -1, z), chiseled);
            }
        }
    }
    for (x, z) in [(-4, -4), (4, -4), (-4, 4), (4, 4)] {
        bd.set((x, 0, z), b("trial_spawner"));
        bd.fill((x, 7, z), (x, 8, z), copper);
        bd.set((x, 6, z), b("copper_bulb"));
    }
    bd.set((0, 0, 0), bp("vault", &[("facing", "north")]));
    // 네 곁방
    for (dx, dz) in DIRS {
        let (cx, cz) = (dx * (n + 6), dz * (n + 6));
        bd.fill((cx - 4, -1, cz - 4), (cx + 4, 5, cz + 4), tuff);
        bd.fill((cx - 3, 0, cz - 3), (cx + 3, 4, cz + 3), block::AIR);
        let (gx, gz) = (dx * (n + 1), dz * (n + 1));
        bd.fill(
            (gx - dz.abs(), 0, gz - dx.abs()),
            (gx + dx + dz.abs(), 2, gz + dz + dx.abs()),
            block::AIR,
        );
        bd.barrel((cx + 2, 0, cz + 2), "chests/trial_chambers/supply");
        bd.chest(
            (cx - 2, 0, cz - 2),
            "north",
            "chests/trial_chambers/corridor",
        );
        bd.set((cx, 0, cz), b("decorated_pot"));
        bd.set((cx, 4, cz), b("copper_bulb"));
    }
}

/// 고대 도시: 깊은 어둠 속 심층암 광장, 가운데 강화 심층암 문, 스컬크, 상자.
pub fn ancient_city(bd: &mut Builder) {
    let bricks = b("deepslate_bricks");
    let tiles = b("deepslate_tiles");
    let reinforced = b("reinforced_deepslate");
    let n = 26;
    bd.fill((-n, 1, -n), (n, 14, n), block::AIR);
    for x in -n..=n {
        for z in -n..=n {
            let s = if (x + z) % 3 == 0 { tiles } else { bricks };
            bd.set((x, 0, z), s);
            if bd.rng.chance(0.08) {
                bd.set((x, 0, z), b("sculk"));
            }
        }
    }
    // 가운데 문 (강화 심층암 테두리)
    for x in -6i32..=6 {
        for y in 1..=12 {
            let edge = x.abs() >= 5 || y >= 11;
            if edge && x.abs() <= 6 {
                bd.set((x, y, 0), reinforced);
            }
        }
    }
    // 건물들
    for (cx, cz) in [(-16, -16), (16, -16), (-16, 16), (16, 16), (0, 18)] {
        bd.walls((cx - 4, 1, cz - 4), (cx + 4, 6, cz + 4), bricks);
        bd.fill((cx - 4, 7, cz - 4), (cx + 4, 7, cz + 4), tiles);
        bd.fill((cx, 1, cz - 4), (cx, 3, cz - 4), block::AIR);
        bd.chest((cx + 2, 1, cz + 2), "west", "chests/ancient_city");
        bd.set((cx - 2, 1, cz + 2), b("sculk_shrieker"));
        bd.set((cx, 5, cz), bp("soul_lantern", &[("hanging", "true")]));
    }
    for _ in 0..12 {
        let (x, z) = (bd.rng.range(-n + 2, n - 2), bd.rng.range(-n + 2, n - 2));
        bd.set((x, 1, z), b("sculk_sensor"));
    }
    bd.chest((0, 1, -4), "north", "chests/ancient_city_ice_box");
    bd.set((0, 0, -4), b("packed_ice"));
}
