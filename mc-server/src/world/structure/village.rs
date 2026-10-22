//! 마을: 가운데 우물, 지형을 따라 뻗는 길, 길가의 집·농장·대장간·도서관·가로등.
//! 바이옴마다(평원·사막·사바나·눈·타이가) 재료와 지붕 모양이 다릅니다.

use super::build::{Builder, Palette, Pos, palette};
use crate::data::block;
use crate::world::biome::SEA_LEVEL;
use crate::world::blockset::{b, bp};

fn style(structure: &str) -> &'static str {
    match structure {
        "village_desert" => "desert",
        "village_savanna" => "savanna",
        "village_snowy" => "snowy",
        "village_taiga" => "taiga",
        _ => "plains",
    }
}

pub fn build(bd: &mut Builder) {
    let st = style(bd.structure);
    let pal = palette(st);
    well(bd, &pal, st);

    // 네 방향 길, 가끔 갈림길
    let dirs = [(1, 0), (0, 1), (-1, 0), (0, -1)];
    let mut sites: Vec<(Pos, (i32, i32))> = Vec::new();
    for &(dx, dz) in &dirs {
        let len = bd.rng.range(16, 34);
        road(bd, &pal, (dx * 4, dz * 4), (dx, dz), len, &mut sites);
        if bd.rng.chance(0.5) {
            let at = bd.rng.range(10, len.max(11));
            let (px, pz) = (dx * (4 + at), dz * (4 + at));
            let side = if bd.rng.chance(0.5) { 1 } else { -1 };
            let (bx, bz) = (-dz * side, dx * side);
            let blen = bd.rng.range(8, 18);
            road(
                bd,
                &pal,
                (px + bx * 2, pz + bz * 2),
                (bx, bz),
                blen,
                &mut sites,
            );
        }
    }

    // 길가 자리에 건물
    let origin = bd.origin;
    let mut placed: Vec<(i32, i32, i32)> = Vec::new(); // (월드 x, z, 반지름)
    let mut has_smithy = false;
    let mut has_library = false;
    for (road_pt, (dx, dz)) in sites {
        // 길에서 바깥쪽: 길 방향에 수직인 양쪽 중 하나
        let side = if bd.rng.chance(0.5) { 1 } else { -1 };
        let (nx, nz) = (-dz * side, dx * side);
        let kind = bd.rng.range(0, 9);
        let (w, d) = match kind {
            0..=3 => (5, 5),
            4 | 5 => (7, 6),
            6 => (9, 7),
            7 => (7, 7),
            _ => (1, 1),
        };
        let front = (road_pt.0 + nx * 3, road_pt.2 + nz * 3);
        let center_w = bd.world((front.0 + nx * d / 2, 0, front.1 + nz * d / 2));
        let r = w.max(d) / 2 + 2;
        if placed.iter().any(|&(px, pz, pr)| {
            (px - center_w.0).abs() < pr + r && (pz - center_w.2).abs() < pr + r
        }) {
            continue;
        }
        // 마을 가운데(우물)와 겹치지 않게
        if (center_w.0 - origin.0).abs() < r + 4 && (center_w.2 - origin.2).abs() < r + 4 {
            continue;
        }
        let ground = bd.ground(center_w.0, center_w.2);
        if ground < SEA_LEVEL {
            continue;
        }
        placed.push((center_w.0, center_w.2, r));
        // 건물의 지역 틀: 문이 길(-z) 쪽을 보도록 회전을 정함
        let facing_rot = match (nx, nz) {
            (0, 1) => 0,
            (-1, 0) => 1,
            (0, -1) => 2,
            _ => 3,
        };
        let saved = (bd.origin, bd.rot);
        let corner = bd.world((front.0, 0, front.1));
        bd.rot = (saved.1 + facing_rot) % 4;
        bd.origin = (corner.0, ground, corner.2);
        // 문이 가운데 오도록 x를 반 폭만큼 옮김
        let shift = bd.world((-w / 2, 0, 0));
        bd.origin = (shift.0, ground, shift.2);
        match kind {
            0..=3 => small_house(bd, &pal, st),
            4 | 5 => medium_house(bd, &pal, st),
            6 => farm(bd, &pal),
            7 if !has_smithy => {
                has_smithy = true;
                smithy(bd, &pal);
            }
            7 if !has_library => {
                has_library = true;
                library(bd, &pal, st);
            }
            7 => small_house(bd, &pal, st),
            _ => lamp(bd, &pal),
        }
        bd.origin = saved.0;
        bd.rot = saved.1;
    }
}

/// 마을 가운데의 우물 (지역 원점이 가운데).
fn well(bd: &mut Builder, pal: &Palette, st: &str) {
    bd.clear((-3, 1, -3), (3, 5, 3));
    bd.foundation(-3, 3, -3, 3, 0, pal.foundation, 10);
    bd.fill((-3, 0, -3), (3, 0, 3), pal.path);
    bd.fill((-2, -4, -2), (2, 1, 2), pal.foundation);
    bd.fill((-1, -3, -1), (1, 1, 1), block::WATER);
    for (x, z) in [(-2, -2), (2, -2), (-2, 2), (2, 2)] {
        bd.fill((x, 2, z), (x, 3, z), pal.fence);
    }
    bd.fill(
        (-2, 4, -2),
        (2, 4, 2),
        if st == "desert" {
            b("cut_sandstone")
        } else {
            pal.slab
        },
    );
    bd.set((0, 3, 0), pal.light);
    // 종
    if st != "desert" {
        bd.set((0, 4, 0), pal.planks);
    }
}

/// 길: `start`(지역)에서 `dir`로 `len`칸, 너비 3. 지형 높이를 따라갑니다.
fn road(
    bd: &mut Builder,
    pal: &Palette,
    start: (i32, i32),
    dir: (i32, i32),
    len: i32,
    sites: &mut Vec<(Pos, (i32, i32))>,
) {
    let (dx, dz) = dir;
    for t in 0..len {
        let (x, z) = (start.0 + dx * t, start.1 + dz * t);
        let mut ok = true;
        for o in -1..=1 {
            let (lx, lz) = (x + dz * o, z + dx * o);
            let w = bd.world((lx, 0, lz));
            let g = bd.ground(w.0, w.2);
            if g < SEA_LEVEL - 1 {
                ok = false;
                continue;
            }
            bd.set_world((w.0, g, w.2), pal.path);
            for y in 1..=3 {
                bd.set_world((w.0, g + y, w.2), block::AIR);
            }
        }
        if !ok {
            break;
        }
        if t > 3 && t % 8 == 4 {
            sites.push(((x, 0, z), dir));
        }
    }
}

/// 박공 지붕: x 방향으로 `w`, z 방향 깊이 `d`, 처마 높이 `y`. 용마루는 x 방향.
fn gable_roof(bd: &mut Builder, pal: &Palette, w: i32, d: i32, y: i32) {
    let half = d / 2;
    for k in 0..=half {
        let yy = y + k;
        for x in -1..=w {
            if k < half || d % 2 == 0 {
                bd.set((x, yy, k - 1), pal.roof("south"));
                bd.set((x, yy, d - k), pal.roof("north"));
            }
        }
        // 박공 벽
        for z in k..=d - 1 - k {
            if yy > y {
                bd.set((0, yy, z), pal.planks);
                bd.set((w - 1, yy, z), pal.planks);
            }
        }
    }
    if d % 2 == 1 {
        for x in -1..=w {
            bd.set((x, y + half + 1, half), pal.slab);
        }
    }
}

fn flat_roof(bd: &mut Builder, pal: &Palette, w: i32, d: i32, y: i32) {
    bd.fill((0, y, 0), (w - 1, y, d - 1), pal.wall);
    for x in 0..w {
        bd.set((x, y + 1, 0), pal.slab);
        bd.set((x, y + 1, d - 1), pal.slab);
    }
    for z in 0..d {
        bd.set((0, y + 1, z), pal.slab);
        bd.set((w - 1, y + 1, z), pal.slab);
    }
}

/// 집의 뼈대: 바닥, 벽, 모서리 기둥, 문, 창. 지역 (0,0,0)~(w-1, ·, d-1), 문은 z=0 가운데.
fn frame(bd: &mut Builder, pal: &Palette, w: i32, d: i32, h: i32) {
    bd.clear((-1, 1, -1), (w, h + d / 2 + 3, d));
    bd.foundation(0, w - 1, 0, d - 1, 0, pal.foundation, 12);
    bd.fill((0, 0, 0), (w - 1, 0, d - 1), pal.foundation);
    bd.fill((1, 0, 1), (w - 2, 0, d - 2), pal.planks);
    bd.walls((0, 1, 0), (w - 1, h, d - 1), pal.wall);
    for (x, z) in [(0, 0), (w - 1, 0), (0, d - 1), (w - 1, d - 1)] {
        bd.fill((x, 1, z), (x, h, z), pal.log);
    }
    let door_x = w / 2;
    bd.set((door_x, 1, 0), pal.door("north", false));
    bd.set((door_x, 2, 0), pal.door("north", true));
    // 문 앞 계단 한 칸
    bd.set((door_x, 0, -1), pal.path);
    // 창문: 양옆과 뒤
    for z in (2..d - 1).step_by(2) {
        bd.set((0, 2, z), pal.pane);
        bd.set((w - 1, 2, z), pal.pane);
    }
    for x in (1..w - 1).step_by(2) {
        if x != door_x {
            bd.set((x, 2, d - 1), pal.pane);
        }
    }
    bd.set((door_x, 3, -1), bp("wall_torch", &[("facing", "north")]));
}

fn bed(bd: &mut Builder, x: i32, z: i32, color: &str) {
    let name = format!("{color}_bed");
    bd.set(
        (x, 1, z),
        bp(&name, &[("facing", "south"), ("part", "foot")]),
    );
    bd.set(
        (x, 1, z + 1),
        bp(&name, &[("facing", "south"), ("part", "head")]),
    );
}

fn house_loot(st: &str) -> &'static str {
    match st {
        "desert" => "chests/village/village_desert_house",
        "savanna" => "chests/village/village_savanna_house",
        "snowy" => "chests/village/village_snowy_house",
        "taiga" => "chests/village/village_taiga_house",
        _ => "chests/village/village_plains_house",
    }
}

fn roof(bd: &mut Builder, pal: &Palette, st: &str, w: i32, d: i32, h: i32) {
    if st == "desert" {
        flat_roof(bd, pal, w, d, h + 1);
    } else {
        bd.fill((0, h + 1, 0), (w - 1, h + 1, d - 1), pal.planks);
        bd.fill((1, h + 1, 1), (w - 2, h + 1, d - 2), block::AIR);
        gable_roof(bd, pal, w, d, h + 1);
    }
}

fn small_house(bd: &mut Builder, pal: &Palette, st: &str) {
    let (w, d, h) = (5, 5, 3);
    frame(bd, pal, w, d, h);
    roof(bd, pal, st, w, d, h);
    let color = *bd
        .rng
        .pick(&["red", "white", "yellow", "light_blue", "lime"]);
    bed(bd, 1, 2, color);
    bd.set((3, 1, 3), b("crafting_table"));
    if bd.rng.chance(0.5) {
        bd.chest((3, 1, 2), "west", house_loot(st));
    }
    bd.set((2, 3, 3), bp("wall_torch", &[("facing", "north")]));
}

fn medium_house(bd: &mut Builder, pal: &Palette, st: &str) {
    let (w, d, h) = (7, 6, 4);
    frame(bd, pal, w, d, h);
    roof(bd, pal, st, w, d, h);
    let color = *bd.rng.pick(&["red", "white", "yellow", "orange", "cyan"]);
    bed(bd, 1, 2, color);
    bed(bd, 5, 2, color);
    bd.set((3, 1, 4), b("crafting_table"));
    bd.chest((2, 1, 4), "north", house_loot(st));
    bd.set((4, 1, 4), bp("furnace", &[("facing", "north")]));
    bd.set((3, 1, 2), pal.fence);
    bd.set((3, 2, 2), b("oak_pressure_plate"));
    bd.set((3, 3, 4), bp("wall_torch", &[("facing", "north")]));
}

fn farm(bd: &mut Builder, pal: &Palette) {
    let (w, d) = (9, 7);
    bd.clear((0, 1, 0), (w - 1, 3, d - 1));
    bd.foundation(0, w - 1, 0, d - 1, 0, b("dirt"), 8);
    bd.fill((0, 0, 0), (w - 1, 0, d - 1), pal.log);
    let crop = *bd.rng.pick(&["wheat", "carrots", "potatoes", "beetroots"]);
    let max_age = if crop == "beetroots" { 3 } else { 7 };
    for x in 1..w - 1 {
        for z in 1..d - 1 {
            if x == w / 2 {
                bd.set((x, 0, z), block::WATER);
                continue;
            }
            bd.set((x, 0, z), bp("farmland", &[("moisture", "7")]));
            let age = bd.rng.range(2, max_age).to_string();
            bd.set((x, 1, z), bp(crop, &[("age", &age)]));
        }
    }
    bd.set((0, 1, 0), b("composter"));
}

fn smithy(bd: &mut Builder, pal: &Palette) {
    let (w, d, h) = (7, 7, 4);
    bd.clear((-1, 1, -1), (w, h + 5, d));
    bd.foundation(0, w - 1, 0, d - 1, 0, b("cobblestone"), 12);
    bd.fill((0, 0, 0), (w - 1, 0, d - 1), b("cobblestone"));
    bd.walls((0, 1, 2), (w - 1, h, d - 1), b("cobblestone"));
    // 앞쪽은 기둥만 있는 열린 작업장
    for x in [0, w - 1] {
        bd.fill((x, 1, 0), (x, h, 0), pal.log);
    }
    bd.fill((0, h + 1, 0), (w - 1, h + 1, d - 1), pal.slab);
    bd.set((1, 1, 1), b("anvil"));
    bd.set((3, 1, 1), b("smithing_table"));
    bd.set((5, 1, 3), bp("blast_furnace", &[("facing", "north")]));
    bd.set((1, 1, 5), block::LAVA);
    bd.set((1, 2, 4), b("iron_bars"));
    bd.chest((5, 1, 5), "north", "chests/village/village_weaponsmith");
    bd.set((3, 1, 2), pal.door("north", false));
    bd.set((3, 2, 2), pal.door("north", true));
    bd.set((3, 3, 1), bp("wall_torch", &[("facing", "north")]));
}

fn library(bd: &mut Builder, pal: &Palette, st: &str) {
    let (w, d, h) = (7, 7, 4);
    frame(bd, pal, w, d, h);
    roof(bd, pal, st, w, d, h);
    for z in 1..d - 1 {
        for y in 1..=2 {
            bd.set((1, y, z), b("bookshelf"));
            bd.set((w - 2, y, z), b("bookshelf"));
        }
    }
    bd.set((3, 1, 4), bp("lectern", &[("facing", "north")]));
    bd.set((3, 1, 5), b("cartography_table"));
    bd.set((3, 3, 5), bp("wall_torch", &[("facing", "north")]));
}

fn lamp(bd: &mut Builder, pal: &Palette) {
    bd.clear((0, 1, 0), (0, 5, 0));
    bd.fill((0, 1, 0), (0, 3, 0), pal.fence);
    bd.set((0, 4, 0), b("glowstone"));
}
