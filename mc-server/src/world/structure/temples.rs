//! 지상 건물: 사막 피라미드, 정글 사원, 이글루, 늪 오두막, 약탈자 전초 기지, 삼림 대저택.

use super::build::{Builder, palette};
use crate::data::block;
use crate::world::blockset::{b, bp};

pub fn desert_pyramid(bd: &mut Builder) {
    let ss = b("sandstone");
    let cut = b("cut_sandstone");
    let chisel = b("chiseled_sandstone");
    let orange = b("orange_terracotta");
    let blue = b("blue_terracotta");
    let n = 10; // 반 폭: 21×21
    bd.foundation(-n, n, -n, n, 0, ss, 12);
    // 계단식 피라미드
    for layer in 0..=n {
        let r = n - layer;
        bd.fill((-r, layer, -r), (r, layer, r), ss);
        if layer > 0 && r > 1 {
            bd.fill((-r + 1, layer, -r + 1), (r - 1, layer, r - 1), block::AIR);
        }
    }
    // 바닥과 무늬
    bd.fill((-n, 0, -n), (n, 0, n), ss);
    bd.fill((-3, 0, -3), (3, 0, 3), orange);
    bd.set((0, 0, 0), blue);
    for (x, z) in [(-2, 0), (2, 0), (0, -2), (0, 2)] {
        bd.set((x, 0, z), orange);
    }
    // 입구와 두 탑
    bd.fill((-1, 1, -n), (1, 3, -n + 1), block::AIR);
    for tx in [-n + 2, n - 2] {
        bd.fill((tx - 2, 0, -n - 2), (tx + 2, 10, -n + 2), cut);
        bd.fill((tx - 1, 1, -n - 1), (tx + 1, 9, -n + 1), block::AIR);
        bd.set((tx, 7, -n - 2), orange);
        bd.set((tx, 8, -n - 2), chisel);
    }
    // 숨은 방: 바닥 아래 11칸, 상자 넷과 압력판 아래 TNT
    let floor = -11;
    bd.fill((-2, floor - 1, -2), (2, floor + 3, 2), ss);
    bd.fill((-1, floor, -1), (1, floor + 3, 1), block::AIR);
    bd.fill((0, floor + 4, 0), (0, -1, 0), block::AIR);
    bd.set((0, 0, 0), blue);
    bd.set((0, -1, 0), block::AIR);
    for (x, z, f) in [
        (-2, 0, "east"),
        (2, 0, "west"),
        (0, -2, "south"),
        (0, 2, "north"),
    ] {
        bd.fill((x, floor, z), (x, floor + 2, z), block::AIR);
        bd.chest((x, floor, z), f, "chests/desert_pyramid");
    }
    bd.fill((-1, floor - 2, -1), (1, floor - 2, 1), b("tnt"));
    bd.set((0, floor, 0), b("stone_pressure_plate"));
}

pub fn jungle_pyramid(bd: &mut Builder) {
    let mix = [(b("cobblestone"), 3), (b("mossy_cobblestone"), 2)];
    let (w, d) = (6, 7); // 반 폭
    bd.foundation(-w, w, -d, d, 0, b("cobblestone"), 10);
    bd.fill_mix((-w, 0, -d), (w, 9, d), &mix);
    bd.fill((-w + 1, 1, -d + 1), (w - 1, 3, d - 1), block::AIR);
    bd.fill((-w + 2, 5, -d + 2), (w - 2, 7, d - 2), block::AIR);
    bd.fill((-1, 1, -d), (1, 3, -d), block::AIR);
    bd.fill((-1, 4, -d + 1), (1, 4, -d + 3), block::AIR);
    for x in [-w + 1, w - 1] {
        for z in (-d + 2..d - 1).step_by(3) {
            bd.set((x, 2, z), b("chiseled_stone_bricks"));
        }
    }
    bd.chest((-w + 1, 1, d - 1), "east", "chests/jungle_temple");
    bd.chest((w - 2, 5, d - 2), "west", "chests/jungle_temple");
    bd.set((0, 1, 0), bp("lever", &[("face", "floor")]));
    for x in -w..=w {
        if bd.rng.chance(0.4) {
            let h = bd.rng.range(2, 6);
            for y in 0..h {
                bd.set((x, 9 - y, -d - 1), bp("vine", &[("south", "true")]));
            }
        }
    }
}

pub fn igloo(bd: &mut Builder) {
    let snow = b("snow_block");
    let r = 3;
    bd.clear((-r - 1, 1, -r - 1), (r + 1, 5, r + 1));
    for x in -r..=r {
        for z in -r..=r {
            for y in 0..=4 {
                let d = ((x * x + z * z) as f64 + ((y as f64) * 1.3).powi(2)).sqrt();
                if d <= r as f64 + 0.5 {
                    bd.set(
                        (x, y, z),
                        if d > r as f64 - 0.6 || y == 0 {
                            snow
                        } else {
                            block::AIR
                        },
                    );
                }
            }
        }
    }
    bd.fill((0, 1, -r), (0, 2, -r - 1), block::AIR);
    bd.set((0, 1, -r - 1), block::AIR);
    bd.fill((-2, 0, -2), (2, 0, 2), b("white_carpet"));
    bd.fill((-2, 0, -2), (2, 0, 2), snow);
    bd.fill((-2, 1, -2), (2, 1, 2), b("white_carpet"));
    bd.set(
        (-2, 1, 0),
        bp("red_bed", &[("facing", "south"), ("part", "foot")]),
    );
    bd.set(
        (-2, 1, 1),
        bp("red_bed", &[("facing", "south"), ("part", "head")]),
    );
    bd.set((2, 1, 1), bp("furnace", &[("facing", "west")]));
    bd.set((2, 1, -1), b("crafting_table"));
    bd.set((0, 3, 0), bp("redstone_torch", &[]));
    // 반은 지하실이 있습니다 (사다리, 상자, 양조대)
    if bd.rng.chance(0.5) {
        bd.set(
            (1, 1, 1),
            bp("oak_trapdoor", &[("half", "top"), ("facing", "south")]),
        );
        for y in -8..=0 {
            bd.set((1, y, 1), bp("ladder", &[("facing", "north")]));
        }
        bd.shell((-2, -10, -1), (4, -6, 4), b("stone_bricks"));
        bd.fill((1, -9, 1), (1, -7, 1), bp("ladder", &[("facing", "north")]));
        bd.chest((3, -9, 3), "west", "chests/igloo_chest");
        bd.set((-1, -9, 3), b("brewing_stand"));
        bd.set((0, -9, 3), b("cauldron"));
    }
}

pub fn swamp_hut(bd: &mut Builder) {
    let planks = b("spruce_planks");
    let log = bp("oak_log", &[("axis", "y")]);
    let (w, d) = (5, 7);
    let floor = 3;
    bd.clear((-1, 1, -1), (w + 1, floor + 6, d + 1));
    for (x, z) in [(0, 1), (w, 1), (0, d), (w, d)] {
        bd.fill((x, -3, z), (x, floor - 1, z), log);
    }
    bd.fill((0, floor, 1), (w, floor, d), planks);
    bd.fill((1, floor, 0), (w - 1, floor, 0), planks);
    bd.walls((0, floor + 1, 1), (w, floor + 3, d), planks);
    bd.fill((2, floor + 1, 1), (2, floor + 2, 1), block::AIR);
    bd.set((0, floor + 2, 4), b("glass_pane"));
    bd.set((w, floor + 2, 4), b("glass_pane"));
    bd.fill(
        (-1, floor + 4, 0),
        (w + 1, floor + 4, d + 1),
        bp("spruce_slab", &[("type", "bottom")]),
    );
    bd.set((1, floor + 1, d - 1), b("crafting_table"));
    bd.set((w - 1, floor + 1, d - 1), b("cauldron"));
    bd.set((1, floor + 2, d - 1), b("potted_red_mushroom"));
    bd.set((w - 1, floor + 1, 2), bp("oak_fence", &[]));
}

pub fn pillager_outpost(bd: &mut Builder) {
    let log = b("dark_oak_log");
    let planks = b("birch_planks");
    let cobble = b("cobblestone");
    bd.clear((-4, 1, -4), (4, 24, 4));
    bd.foundation(-3, 3, -3, 3, 0, cobble, 10);
    bd.fill((-3, 0, -3), (3, 0, 3), cobble);
    for floor in 0..4 {
        let y = 1 + floor * 5;
        bd.walls((-3, y, -3), (3, y + 4, 3), planks);
        for (x, z) in [(-3, -3), (3, -3), (-3, 3), (3, 3)] {
            bd.fill((x, y, z), (x, y + 4, z), log);
        }
        bd.fill((-2, y + 4, -2), (2, y + 4, 2), b("dark_oak_planks"));
        bd.fill(
            (2, y, 2),
            (2, y + 4, 2),
            bp("ladder", &[("facing", "north")]),
        );
        for x in [-1, 1] {
            bd.set((x, y + 2, -3), b("dark_oak_fence"));
        }
    }
    bd.fill((0, 1, -3), (0, 2, -3), block::AIR);
    bd.fill((-4, 21, -4), (4, 21, 4), b("dark_oak_planks"));
    bd.walls((-4, 22, -4), (4, 22, 4), b("dark_oak_fence"));
    bd.chest((-2, 21 - 4, -2), "south", "chests/pillager_outpost");
    bd.set((0, 22, 0), b("white_wool"));
}

pub fn mansion(bd: &mut Builder) {
    let pal = palette("dark");
    let cobble = b("cobblestone");
    let (w, d) = (15, 12); // 반 폭
    bd.clear((-w - 1, 1, -d - 1), (w + 1, 18, d + 1));
    bd.foundation(-w, w, -d, d, 0, cobble, 14);
    bd.fill((-w, 0, -d), (w, 0, d), cobble);
    for (y0, mat) in [(1, pal.planks), (7, b("birch_planks"))] {
        bd.walls((-w, y0, -d), (w, y0 + 5, d), mat);
        bd.fill((-w + 1, y0 + 5, -d + 1), (w - 1, y0 + 5, d - 1), pal.planks);
        for x in (-w..=w).step_by(5) {
            bd.fill((x, y0, -d), (x, y0 + 5, -d), pal.log);
            bd.fill((x, y0, d), (x, y0 + 5, d), pal.log);
        }
        for x in (-w + 2..w).step_by(3) {
            bd.set((x, y0 + 2, -d), pal.pane);
            bd.set((x, y0 + 2, d), pal.pane);
        }
        // 방 칸막이
        for x in [-5, 5] {
            bd.fill((x, y0, -d + 1), (x, y0 + 4, d - 1), mat);
            bd.fill((x, y0, 0), (x, y0 + 1, 0), block::AIR);
        }
        bd.fill(
            (-w + 1, y0 + 1, d - 1),
            (-w + 1, y0 + 1, d - 1),
            b("bookshelf"),
        );
    }
    // 지붕
    for k in 0..=d + 1 {
        let y = 13 + k / 2;
        for x in -w - 1..=w + 1 {
            bd.set((x, y, -d - 1 + k), pal.roof("south"));
            bd.set((x, y, d + 1 - k), pal.roof("north"));
        }
        if k >= d {
            break;
        }
    }
    bd.fill((-1, 1, -d), (1, 3, -d), block::AIR);
    bd.set((0, 1, -d), pal.door("north", false));
    bd.set((0, 2, -d), pal.door("north", true));
    bd.chest((-w + 2, 1, d - 2), "north", "chests/woodland_mansion");
    bd.chest((w - 2, 7, -d + 2), "south", "chests/woodland_mansion");
    bd.fill((-3, 1, -3), (3, 1, 3), b("red_carpet"));
    bd.fill((-3, 7, -3), (3, 7, 3), b("white_carpet"));
    for x in (-w + 3..w - 2).step_by(6) {
        bd.set((x, 4, 0), bp("lantern", &[("hanging", "true")]));
        bd.set((x, 10, 0), bp("lantern", &[("hanging", "true")]));
    }
}
