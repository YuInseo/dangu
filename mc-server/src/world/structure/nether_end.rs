//! 네더와 엔드 구조물: 네더 요새, 보루 잔해, 네더 화석, 엔드 도시.

use super::build::Builder;
use crate::data::block;
use crate::world::blockset::{b, bp};

/// 네더 요새: 넓은 다리가 십자로 뻗고, 끝마다 방(블레이즈 생성기, 네더 사마귀 밭, 상자).
pub fn fortress(bd: &mut Builder) {
    let nb = b("nether_bricks");
    let fence = b("nether_brick_fence");
    let arms: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
    bd.fill((-3, -1, -3), (3, 5, 3), nb);
    bd.fill((-2, 0, -2), (2, 4, 2), block::AIR);
    for (i, &(dx, dz)) in arms.iter().enumerate() {
        let len = bd.rng.range(20, 44);
        let side = (dz.abs(), dx.abs());
        for t in 3..len {
            let (x, z) = (dx * t, dz * t);
            for o in -2i32..=2 {
                let (cx, cz) = (x + side.0 * o, z + side.1 * o);
                bd.set((cx, -1, cz), nb);
                if o.abs() == 2 {
                    bd.set((cx, 0, cz), fence);
                } else {
                    bd.fill((cx, 0, cz), (cx, 3, cz), block::AIR);
                }
            }
            // 다리 기둥이 용암 바다까지
            if t % 8 == 0 {
                for o in [-2, 2] {
                    let (cx, cz) = (x + side.0 * o, z + side.1 * o);
                    for y in -20..-1 {
                        bd.set((cx, y, cz), nb);
                    }
                }
            }
        }
        let (rx, rz) = (dx * (len + 3), dz * (len + 3));
        bd.fill((rx - 4, -1, rz - 4), (rx + 4, 5, rz + 4), nb);
        bd.fill((rx - 3, 0, rz - 3), (rx + 3, 4, rz + 3), block::AIR);
        bd.fill(
            (rx - 4 * side.0 - dx * 4, 0, rz - 4 * side.1 - dz * 4),
            (rx + 4 * side.0 - dx * 4, 2, rz + 4 * side.1 - dz * 4),
            block::AIR,
        );
        match i {
            0 => {
                bd.spawner((rx, 1, rz), "blaze");
                bd.fill((rx - 1, 0, rz - 1), (rx + 1, 0, rz + 1), nb);
            }
            1 => {
                for x in rx - 2..=rx + 2 {
                    for z in [rz - 2, rz + 2] {
                        bd.set((x, -1, z), b("soul_sand"));
                        let age = bd.rng.range(0, 3).to_string();
                        bd.set((x, 0, z), bp("nether_wart", &[("age", &age)]));
                    }
                }
                bd.chest((rx, 0, rz), "north", "chests/nether_bridge");
            }
            _ => {
                if bd.rng.chance(0.5) {
                    bd.chest((rx + 2, 0, rz + 2), "west", "chests/nether_bridge");
                }
                bd.fill((rx - 3, 4, rz - 3), (rx + 3, 4, rz + 3), fence);
            }
        }
    }
}

/// 보루 잔해: 흑암 성벽과 금 블록, 보물 상자.
pub fn bastion(bd: &mut Builder) {
    let mix = [
        (b("polished_blackstone_bricks"), 5),
        (b("cracked_polished_blackstone_bricks"), 2),
        (b("blackstone"), 2),
        (b("gilded_blackstone"), 1),
    ];
    let n = 12;
    bd.fill_mix((-n, -2, -n), (n, 0, n), &mix);
    for k in 0..4 {
        let r = n - k * 2;
        bd.walls((-r, 1 + k * 4, -r), (r, 4 + k * 4, r), mix[0].0);
        bd.fill_mix((-r, 4 + k * 4, -r), (r, 4 + k * 4, r), &mix);
        bd.fill(
            (-r + 1, 4 + k * 4, -r + 1),
            (r - 1, 4 + k * 4, r - 1),
            block::AIR,
        );
        bd.fill((-1, 1 + k * 4, -r), (1, 3 + k * 4, -r), block::AIR);
    }
    bd.fill((-2, 1, -2), (2, 1, 2), b("gold_block"));
    bd.chest((0, 2, 0), "north", "chests/bastion_treasure");
    bd.chest((n - 2, 1, n - 2), "west", "chests/bastion_other");
    bd.chest((-n + 2, 1, n - 2), "east", "chests/bastion_bridge");
    for (x, z) in [(-n + 1, -n + 1), (n - 1, n - 1)] {
        bd.set((x, 1, z), b("lava"));
    }
}

/// 네더 화석: 뼈 블록으로 된 갈비뼈 모양.
pub fn nether_fossil(bd: &mut Builder) {
    let bone = bp("bone_block", &[("axis", "x")]);
    let g = bd.ground_local(0, 0);
    let len = bd.rng.range(5, 9);
    for x in 0..len {
        bd.set((x, g + 1, 0), bone);
        if x % 2 == 0 {
            for y in 1..=3 {
                bd.set(
                    (x, g + 1 + y, -1 - y / 2),
                    bp("bone_block", &[("axis", "y")]),
                );
                bd.set(
                    (x, g + 1 + y, 1 + y / 2),
                    bp("bone_block", &[("axis", "y")]),
                );
            }
        }
    }
}

/// 엔드 도시: 보라색 탑 몇 층, 엔드 막대 조명, 보물 상자.
pub fn end_city(bd: &mut Builder) {
    let purpur = b("purpur_block");
    let pillar = bp("purpur_pillar", &[("axis", "y")]);
    let bricks = b("end_stone_bricks");
    let floors = bd.rng.range(3, 6);
    bd.fill((-5, 0, -5), (5, 0, 5), bricks);
    for f in 0..floors {
        let y = 1 + f * 6;
        let r = 4 - (f % 2);
        bd.walls((-r, y, -r), (r, y + 4, r), purpur);
        for (x, z) in [(-r, -r), (r, -r), (-r, r), (r, r)] {
            bd.fill((x, y, z), (x, y + 5, z), pillar);
        }
        bd.fill((-r, y + 5, -r), (r, y + 5, r), purpur);
        bd.fill((-1, y, -r), (1, y + 2, -r), block::AIR);
        bd.fill(
            (0, y, 0),
            (0, y + 5, 0),
            bp("ladder", &[("facing", "north")]),
        );
        bd.fill((0, y, 1), (0, y + 5, 1), purpur);
        for x in [-r + 1, r - 1] {
            bd.set((x, y + 3, 0), bp("end_rod", &[("facing", "up")]));
        }
    }
    let top = 1 + floors * 6;
    bd.fill((-5, top, -5), (5, top, 5), purpur);
    bd.walls(
        (-5, top + 1, -5),
        (5, top + 1, 5),
        bp("purpur_slab", &[("type", "bottom")]),
    );
    bd.chest((2, top + 1, 2), "west", "chests/end_city_treasure");
    bd.chest((-2, top + 1, -2), "east", "chests/end_city_treasure");
}
