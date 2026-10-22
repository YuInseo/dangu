//! 폐허와 작은 구조물: 무너진 차원문, 난파선, 바다 유적, 묻힌 보물, 옛길 유적, 바다 신전,
//! 그리고 26.3의 버려진 야영지.

use super::build::{Builder, palette};
use crate::data::block;
use crate::world::biome::SEA_LEVEL;
use crate::world::blockset::{b, bp};

pub fn ruined_portal(bd: &mut Builder) {
    let variant = bd
        .structure
        .strip_prefix("ruined_portal_")
        .unwrap_or("standard");
    let (w, h) = (bd.rng.range(4, 6), bd.rng.range(5, 7));
    let obsidian = b("obsidian");
    let crying = b("crying_obsidian");
    let around = match variant {
        "desert" => b("sandstone"),
        "jungle" | "swamp" => b("mossy_stone_bricks"),
        "nether" => b("blackstone"),
        _ => b("stone_bricks"),
    };
    // 지붕이 날아간 차원문 테두리. 몇 칸은 빠지거나 우는 흑요석.
    let tilt = bd.rng.chance(0.3);
    for x in 0..w {
        for y in 0..h {
            let edge = x == 0 || x == w - 1 || y == 0 || y == h - 1;
            if !edge {
                continue;
            }
            if bd.rng.chance(0.18) {
                continue;
            }
            let s = if bd.rng.chance(0.15) {
                crying
            } else {
                obsidian
            };
            let z = if tilt && y > h / 2 { 1 } else { 0 };
            bd.set((x - w / 2, y + 1, z), s);
        }
    }
    // 둘레의 네더랙·마그마·석재
    for _ in 0..30 {
        let (x, z) = (bd.rng.range(-w, w), bd.rng.range(-3, 3));
        let g = bd.ground_local(x, z);
        let s = match bd.rng.below(5) {
            0 => b("magma_block"),
            1 | 2 => block::NETHERRACK,
            _ => around,
        };
        bd.set((x, g, z), s);
    }
    if variant == "jungle" || variant == "swamp" {
        for x in -w..=w {
            if bd.rng.chance(0.3) {
                bd.set((x, h, -1), bp("vine", &[("south", "true")]));
            }
        }
    }
    bd.set((w / 2 + 1, 1, 2), b("gold_block"));
    bd.chest((-w / 2 - 1, 1, 2), "north", "chests/ruined_portal");
}

pub fn shipwreck(bd: &mut Builder) {
    let planks = b(if bd.rng.chance(0.5) {
        "spruce_planks"
    } else {
        "oak_planks"
    });
    let log = b("spruce_log");
    let (len, half) = (bd.rng.range(14, 20), 3);
    let tilt = if bd.rng.chance(0.4) { 1 } else { 0 };
    for z in 0..len {
        // 뱃머리·고물 쪽은 좁게
        let taper = if z < 3 {
            3 - z
        } else if z > len - 4 {
            z - (len - 4)
        } else {
            0
        };
        let r = (half - taper).max(1);
        for x in -r..=r {
            let y_off = if x > 0 { tilt } else { 0 };
            bd.set((x, y_off, z), planks);
            if x.abs() == r {
                bd.fill((x, 1 + y_off, z), (x, 3 + y_off, z), planks);
            } else {
                bd.fill((x, 1 + y_off, z), (x, 3 + y_off, z), block::WATER);
            }
        }
    }
    if bd.rng.chance(0.6) {
        bd.fill((0, 1, len / 2), (0, 9, len / 2), log);
        bd.fill((-2, 7, len / 2), (2, 7, len / 2), b("spruce_fence"));
    }
    bd.chest((0, 1, 2), "south", "chests/shipwreck_supply");
    bd.chest((0, 1, len - 3), "north", "chests/shipwreck_treasure");
    bd.chest((1, 1, len / 2 + 2), "west", "chests/shipwreck_map");
}

pub fn ocean_ruin(bd: &mut Builder) {
    let warm = bd.structure.ends_with("warm");
    let mix = if warm {
        [
            (b("sandstone"), 4),
            (b("cut_sandstone"), 2),
            (b("chiseled_sandstone"), 1),
        ]
    } else {
        [
            (b("stone_bricks"), 4),
            (b("mossy_stone_bricks"), 2),
            (b("cracked_stone_bricks"), 2),
        ]
    };
    let s = bd.rng.range(3, 5);
    for x in -s..=s {
        for z in -s..=s {
            let edge = x.abs() == s || z.abs() == s;
            let g = bd.ground_local(x, z);
            bd.set((x, g, z), mix[0].0);
            if edge && bd.rng.chance(0.7) {
                let h = bd.rng.range(1, 4);
                for y in 1..=h {
                    let st = bd.rng.weighted(&mix);
                    bd.set((x, g + y, z), st);
                }
            }
        }
    }
    let g = bd.ground_local(0, 0);
    let big = bd.rng.chance(0.3);
    bd.chest(
        (0, g + 1, 0),
        "north",
        if big {
            "chests/underwater_ruin_big"
        } else {
            "chests/underwater_ruin_small"
        },
    );
}

pub fn buried_treasure(bd: &mut Builder) {
    let g = bd.ground_local(0, 0);
    if bd.origin.1 + g < SEA_LEVEL - 1 {
        return;
    }
    bd.chest((0, g - 3, 0), "north", "chests/buried_treasure");
}

pub fn trail_ruins(bd: &mut Builder) {
    // 땅에 반쯤 묻힌 테라코타·진흙 벽돌 유적, 의심스러운 자갈
    let mix = [
        (b("mud_bricks"), 4),
        (b("terracotta"), 2),
        (b("light_blue_terracotta"), 1),
        (b("suspicious_gravel"), 1),
    ];
    for i in 0..bd.rng.range(3, 6) {
        let (cx, cz) = (bd.rng.range(-10, 10), bd.rng.range(-10, 10));
        let (w, d) = (bd.rng.range(3, 6), bd.rng.range(3, 6));
        let g = bd.ground_local(cx, cz);
        let top = g - 1 + (i % 2);
        bd.walls((cx, top - 4, cz), (cx + w, top, cz + d), mix[0].0);
        for x in cx..=cx + w {
            for z in cz..=cz + d {
                let s = bd.rng.weighted(&mix);
                bd.set((x, top - 4, z), s);
            }
        }
        bd.fill(
            (cx + 1, top - 3, cz + 1),
            (cx + w - 1, top, cz + d - 1),
            b("gravel"),
        );
    }
}

pub fn monument(bd: &mut Builder) {
    let pr = b("prismarine");
    let bricks = b("prismarine_bricks");
    let dark = b("dark_prismarine");
    let lantern = b("sea_lantern");
    let n = 14;
    let g = bd.ground_local(0, 0);
    // 계단식 신전, 안은 물
    for layer in 0..8 {
        let r = n - layer;
        bd.fill(
            (-r, g + layer, -r),
            (r, g + layer, r),
            if layer % 2 == 0 { bricks } else { pr },
        );
        if layer > 0 {
            bd.fill(
                (-r + 1, g + layer, -r + 1),
                (r - 1, g + layer, r - 1),
                block::WATER,
            );
        }
    }
    bd.fill((-n, g, -n), (n, g, n), dark);
    for (x, z) in [
        (-n + 2, -n + 2),
        (n - 2, -n + 2),
        (-n + 2, n - 2),
        (n - 2, n - 2),
    ] {
        bd.fill((x, g, z), (x, g + 10, z), pr);
        bd.set((x, g + 11, z), lantern);
    }
    bd.fill((-2, g + 1, -2), (2, g + 1, 2), lantern);
    bd.fill((-1, g + 2, -1), (1, g + 3, 1), b("gold_block"));
    bd.fill((-1, g + 1, -n), (1, g + 4, -n + 2), block::WATER);
}

/// 26.3 버려진 야영지: 천막, 모닥불, 통나무 의자, 짐 상자. 바이옴마다 나무 재료가 다릅니다.
pub fn abandoned_camp(bd: &mut Builder) {
    let style = match bd
        .structure
        .strip_prefix("abandoned_camp_")
        .unwrap_or("forest")
    {
        "birch_forest" | "old_growth_birch_forest" => "birch",
        "cherry_grove" => "cherry",
        "dappled_forest" => "poplar",
        "pale_garden" => "pale",
        "savanna" | "wooded_badlands" => "savanna",
        "snowy_taiga"
        | "taiga"
        | "old_growth_pine_taiga"
        | "old_growth_spruce_taiga"
        | "windswept_forest" => "taiga",
        "bamboo_jungle" | "sparse_jungle" => "jungle",
        "swamp" => "mangrove",
        _ => "oak",
    };
    let pal = palette(style);
    let wool = b(bd
        .rng
        .pick(&["white_wool", "brown_wool", "green_wool", "light_gray_wool"]));
    bd.clear((-4, 1, -4), (4, 4, 4));
    bd.foundation(-4, 4, -4, 4, 0, b("dirt"), 4);
    bd.fill((-4, 0, -4), (4, 0, 4), b("coarse_dirt"));
    // 천막: A자 모양 양털
    for z in 0..4 {
        for (dx, y) in [(-2, 1), (-1, 2), (0, 3), (1, 2), (2, 1)] {
            bd.set((dx, y, z), wool);
        }
    }
    bd.fill((-1, 1, 3), (1, 2, 3), wool);
    bd.set((0, 1, 3), wool);
    bd.set(
        (-1, 1, 1),
        bp("white_bed", &[("facing", "south"), ("part", "foot")]),
    );
    bd.set(
        (-1, 1, 2),
        bp("white_bed", &[("facing", "south"), ("part", "head")]),
    );
    // 꺼진 모닥불과 둘레의 통나무 의자
    bd.set((0, 1, -3), bp("campfire", &[("lit", "false")]));
    for (x, z, axis) in [(-2, -3, "z"), (2, -3, "z"), (0, -5, "x")] {
        bd.set(
            (x, 1, z),
            crate::world::blockset::with(pal.log, &[("axis", axis)]),
        );
    }
    bd.chest((1, 1, 1), "west", "chests/abandoned_camp_common_chest");
    // 가끔 땅에 묻힌 비밀 상자
    if bd.rng.chance(0.35) {
        bd.chest((3, -2, 2), "north", "chests/abandoned_camp_secret_chest");
        bd.set((3, 0, 2), b("rooted_dirt"));
    }
    bd.set((3, 1, -2), bp("lantern", &[("hanging", "false")]));
}
