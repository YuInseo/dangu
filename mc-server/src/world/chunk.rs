//! `Level Chunk with Light` 패킷 본문 만들기.
//!
//! ```text
//! i32 x | i32 z
//! heightmaps: VarInt 개수 × (VarInt 종류, Prefixed Array of Long)
//! data:       VarInt 바이트 길이 × 섹션들
//!             섹션 = i16 공기 아닌 블록 수 | i16 유체 블록 수(26.1~) | 블록 팔레트 | 바이옴 팔레트
//! block entities: VarInt 개수 (항상 0)
//! light:      BitSet ×4 (26.3부터 바이트 BitSet) | 하늘빛 배열들 | 블록빛 배열들
//! ```
//!
//! 팔레트 컨테이너는 1.21.5부터 데이터 배열의 길이 접두사가 없습니다 — 길이는 비트 수와
//! 항목 수로 정해지기 때문입니다.

use std::sync::OnceLock;

use super::biome::Biome;
use super::column::SectionView;
use super::terrain::Column;
use crate::data::BLOCK_ENTITY_TYPES;
use crate::data::{BLOCK_STATE_COUNT, block, blocks};
use crate::protocol::nbt::Tag;
use crate::protocol::types::{write_bitset, write_byte_array, write_i32, write_long_array};
use crate::protocol::varint::write_varint;

const HEIGHTMAP_WORLD_SURFACE: i32 = 1;
const HEIGHTMAP_MOTION_BLOCKING: i32 = 4;

/// 직접(전역) 팔레트의 비트 수: 블록 상태 수를 담을 수 있는 최소 비트.
const GLOBAL_BLOCK_BITS: u8 = (u32::BITS - (BLOCK_STATE_COUNT - 1).leading_zeros()) as u8;

pub fn encode(col: &Column, cx: i32, cz: i32) -> Vec<u8> {
    let dim = col.dimension;
    let mut out = Vec::with_capacity(64 * 1024);
    write_i32(&mut out, cx);
    write_i32(&mut out, cz);

    // ---- 높이 맵 ----
    let mut world_surface = [0i64; 256];
    let mut motion_blocking = [0i64; 256];
    for z in 0..16 {
        for x in 0..16 {
            let i = z * 16 + x;
            for y in (dim.min_y()..dim.min_y() + dim.height()).rev() {
                let state = col.get(x, y, z);
                let rel = (y - dim.min_y() + 1) as i64;
                if world_surface[i] == 0 && state != block::AIR {
                    world_surface[i] = rel;
                }
                if motion_blocking[i] == 0 && blocks_motion(state) {
                    motion_blocking[i] = rel;
                    break;
                }
            }
        }
    }
    let height_bits = (u32::BITS - (dim.height() as u32).leading_zeros()) as u8;
    write_varint(&mut out, 2);
    write_varint(&mut out, HEIGHTMAP_WORLD_SURFACE);
    write_long_array(&mut out, &pack(&world_surface, height_bits));
    write_varint(&mut out, HEIGHTMAP_MOTION_BLOCKING);
    write_long_array(&mut out, &pack(&motion_blocking, height_bits));

    // ---- 섹션 ----
    let mut sections = Vec::with_capacity(16 * 1024);
    for s in 0..dim.section_count() {
        write_section(&mut sections, col.section(s), col.section_biomes(s));
    }
    write_byte_array(&mut out, &sections);

    write_block_entities(&mut out, col, cx, cz);

    write_light(&mut out, col);
    out
}

fn blocks_motion(state: u32) -> bool {
    !crate::world::features::soft(state) || state == block::WATER
}

fn is_fluid(state: u32) -> bool {
    matches!(state, block::WATER | block::LAVA)
}

/// 하늘빛이 이 블록을 지날 때 줄어드는 양. 15면 완전히 막힘. 상태마다 한 번 계산해 둡니다.
fn sky_opacity(state: u32) -> u8 {
    static T: OnceLock<Vec<u8>> = OnceLock::new();
    T.get_or_init(|| (0..BLOCK_STATE_COUNT).map(opacity_of).collect())
        .get(state as usize)
        .copied()
        .unwrap_or(15)
}

fn opacity_of(state: u32) -> u8 {
    let n = blocks::name(blocks::block_of(state));
    const CLEAR: &[&str] = &[
        "air",
        "grass",
        "fern",
        "flower",
        "sapling",
        "torch",
        "rail",
        "fence",
        "pane",
        "bars",
        "door",
        "carpet",
        "button",
        "lever",
        "pressure_plate",
        "sign",
        "vine",
        "ladder",
        "lantern",
        "chain",
        "glass",
        "cobweb",
        "bush",
        "roots",
        "sprouts",
        "sugar_cane",
        "bamboo",
        "petals",
        "wildflowers",
        "leaf_litter",
        "dead_bush",
        "lily_pad",
        "wheat",
        "carrots",
        "potatoes",
        "beetroots",
        "spike",
        "dripstone",
        "campfire",
        "bed",
        "chest",
        "end_rod",
        "rod",
        "candle",
        "mushroom",
        "fungus",
        "tulip",
        "orchid",
        "allium",
        "bluet",
        "daisy",
        "poppy",
        "dandelion",
        "cornflower",
        "lily_of_the_valley",
        "eyeblossom",
        "chorus",
        "cactus",
        "frame",
        "pot",
        "moss_carpet",
        "hanging_moss",
        "kelp",
        "seagrass",
        "berry",
        "melon_stem",
        "pumpkin_stem",
        "azalea",
        "cave_vines",
        "weeping",
        "twisting",
        "snow",
        "scaffolding",
        "sensor",
        "shrieker",
        "lectern",
        "anvil",
        "bell",
        "brewing_stand",
        "cauldron",
        "composter",
        "slab",
        "stairs",
        "wall",
        "trapdoor",
        "nether_wart",
        "farmland",
        "dirt_path",
        "light",
    ];
    if n == "snow_block" || n.ends_with("_wall") && n.contains("brick") && false {
        return 15;
    }
    if n == "water"
        || n.ends_with("leaves")
        || n == "ice"
        || n == "frosted_ice"
        || n == "slime_block"
        || n == "honey_block"
    {
        return 1;
    }
    if n == "lava" {
        return 15;
    }
    if CLEAR.iter().any(|c| n.contains(c)) && n != "snow_block" && n != "glass_bottle" {
        return 0;
    }
    15
}

/// 상태 → 블록 엔티티 종류 번호 (없으면 -1). 상자·생성기·표지판 등은 이 목록이 있어야
/// 클라이언트가 그립니다.
fn block_entity_type(state: u32) -> i16 {
    static T: OnceLock<Vec<i16>> = OnceLock::new();
    T.get_or_init(|| {
        let id = |name: &str| {
            BLOCK_ENTITY_TYPES
                .iter()
                .position(|t| *t == name)
                .map_or(-1, |i| i as i16)
        };
        (0..BLOCK_STATE_COUNT)
            .map(|s| {
                let n = blocks::name(blocks::block_of(s));
                let t = match n {
                    "chest"
                    | "trapped_chest"
                    | "ender_chest"
                    | "barrel"
                    | "furnace"
                    | "smoker"
                    | "blast_furnace"
                    | "bell"
                    | "lectern"
                    | "enchanting_table"
                    | "brewing_stand"
                    | "beacon"
                    | "conduit"
                    | "hopper"
                    | "dispenser"
                    | "dropper"
                    | "jukebox"
                    | "end_portal"
                    | "end_gateway"
                    | "daylight_detector"
                    | "comparator"
                    | "sculk_sensor"
                    | "calibrated_sculk_sensor"
                    | "sculk_catalyst"
                    | "sculk_shrieker"
                    | "chiseled_bookshelf"
                    | "decorated_pot"
                    | "crafter"
                    | "trial_spawner"
                    | "vault"
                    | "creaking_heart"
                    | "potent_sulfur"
                    | "structure_block"
                    | "jigsaw" => n,
                    "spawner" => "mob_spawner",
                    "campfire" | "soul_campfire" => "campfire",
                    "beehive" | "bee_nest" => "beehive",
                    "suspicious_sand" | "suspicious_gravel" => "brushable_block",
                    "command_block" | "chain_command_block" | "repeating_command_block" => {
                        "command_block"
                    }
                    _ if n.ends_with("hanging_sign") => "hanging_sign",
                    _ if n.ends_with("_sign") => "sign",
                    _ if n.ends_with("_banner") => "banner",
                    _ if n.ends_with("shulker_box") => "shulker_box",
                    _ if n.ends_with("_skull") || n.ends_with("_head") => "skull",
                    _ if n.ends_with("_shelf") && n != "shelf_mushroom" => "shelf",
                    _ if n.contains("copper_chest") => "chest",
                    _ if n.contains("copper_golem_statue") => "copper_golem_statue",
                    _ => "",
                };
                if t.is_empty() { -1 } else { id(t) }
            })
            .collect()
    })
    .get(state as usize)
    .copied()
    .unwrap_or(-1)
}

/// 블록 엔티티 목록: 묶은 xz(바이트), y(i16), 종류, NBT(생성기만 SpawnData).
fn write_block_entities(out: &mut Vec<u8>, col: &Column, cx: i32, cz: i32) {
    let dim = col.dimension;
    let mut list: Vec<(u8, i16, i16, Tag)> = Vec::new();
    for y in dim.min_y()..dim.min_y() + dim.height() {
        for z in 0..16 {
            for x in 0..16 {
                let s = col.get(x, y, z);
                if s == block::AIR || s == block::STONE || s == block::WATER {
                    continue;
                }
                let t = block_entity_type(s);
                if t < 0 {
                    continue;
                }
                let world = (cx * 16 + x as i32, y, cz * 16 + z as i32);
                let nbt = match col.spawners.iter().find(|(p, _)| *p == world) {
                    Some((_, entity)) => Tag::Compound(vec![(
                        "SpawnData".into(),
                        Tag::Compound(vec![(
                            "entity".into(),
                            Tag::Compound(vec![(
                                "id".into(),
                                Tag::String(format!("minecraft:{entity}")),
                            )]),
                        )]),
                    )]),
                    None => Tag::Compound(Vec::new()),
                };
                list.push((((x as u8) << 4) | z as u8, y as i16, t, nbt));
            }
        }
    }
    write_varint(out, list.len() as i32);
    for (xz, y, t, nbt) in list {
        out.push(xz);
        out.extend_from_slice(&y.to_be_bytes());
        write_varint(out, t as i32);
        nbt.write_root(out);
    }
}

/// 값들을 `bits`비트씩 long에 채웁니다. 값은 long 경계를 넘지 않습니다(남는 비트는 버림).
fn pack(values: &[i64], bits: u8) -> Vec<i64> {
    let per_long = 64 / bits as usize;
    let mut longs = vec![0i64; values.len().div_ceil(per_long)];
    for (i, &v) in values.iter().enumerate() {
        longs[i / per_long] |= v << ((i % per_long) * bits as usize);
    }
    longs
}

fn write_section(out: &mut Vec<u8>, blocks: SectionView<'_>, biomes: &[u8]) {
    let blocks = match blocks {
        SectionView::Uniform(state) => {
            // 한 가지 블록: 개수와 단일 값 팔레트
            let n = if state == block::AIR { 0 } else { 4096 };
            out.extend_from_slice(&(n as i16).to_be_bytes());
            out.extend_from_slice(&(if is_fluid(state) { 4096i16 } else { 0 }).to_be_bytes());
            out.push(0);
            write_varint(out, state as i32);
            write_biomes(out, biomes);
            return;
        }
        SectionView::Full(a) => a,
    };
    let non_air = blocks.iter().filter(|&&b| b as u32 != block::AIR).count() as i16;
    let fluids = blocks.iter().filter(|&&b| is_fluid(b as u32)).count() as i16;
    out.extend_from_slice(&non_air.to_be_bytes());
    out.extend_from_slice(&fluids.to_be_bytes());

    // 블록: 한 가지면 단일 값, 256가지 이하면 간접 팔레트, 그 이상이면 전역 ID.
    let mut palette: Vec<u16> = Vec::new();
    let mut index: std::collections::HashMap<u16, i64> = std::collections::HashMap::new();
    for &b in blocks.iter() {
        if let std::collections::hash_map::Entry::Vacant(e) = index.entry(b) {
            e.insert(palette.len() as i64);
            palette.push(b);
            if palette.len() > 256 {
                break;
            }
        }
    }
    if palette.len() == 1 {
        out.push(0);
        write_varint(out, palette[0] as i32);
    } else if palette.len() <= 256 {
        let bits = (u32::BITS - (palette.len() as u32 - 1).leading_zeros()).max(4) as u8;
        out.push(bits);
        write_varint(out, palette.len() as i32);
        for &p in &palette {
            write_varint(out, p as i32);
        }
        let indices: Vec<i64> = blocks.iter().map(|b| index[b]).collect();
        for l in pack(&indices, bits) {
            out.extend_from_slice(&l.to_be_bytes());
        }
    } else {
        out.push(GLOBAL_BLOCK_BITS);
        let ids: Vec<i64> = blocks.iter().map(|&b| b as i64).collect();
        for l in pack(&ids, GLOBAL_BLOCK_BITS) {
            out.extend_from_slice(&l.to_be_bytes());
        }
    }
    write_biomes(out, biomes);
}

fn write_biomes(out: &mut Vec<u8>, biomes: &[u8]) {
    // 바이옴 64칸: 한 가지면 단일 값, 아니면 간접 팔레트(1~3비트), 그 이상이면 전역.
    let ids: Vec<u16> = biomes.iter().map(|&b| Biome(b).network_id()).collect();
    let mut palette: Vec<u16> = Vec::new();
    for &b in &ids {
        if !palette.contains(&b) {
            palette.push(b);
        }
    }
    if palette.len() == 1 {
        out.push(0);
        write_varint(out, palette[0] as i32);
    } else if palette.len() <= 8 {
        let bits = (u32::BITS - (palette.len() as u32 - 1).leading_zeros()).max(1) as u8;
        out.push(bits);
        write_varint(out, palette.len() as i32);
        for &p in &palette {
            write_varint(out, p as i32);
        }
        let indices: Vec<i64> = ids
            .iter()
            .map(|b| palette.iter().position(|p| p == b).unwrap() as i64)
            .collect();
        for l in pack(&indices, bits) {
            out.extend_from_slice(&l.to_be_bytes());
        }
    } else {
        let count = crate::data::SYNCED_REGISTRIES
            .iter()
            .find(|(n, _)| *n == "worldgen/biome")
            .map_or(64, |(_, e)| e.len()) as u32;
        let bits = (u32::BITS - (count - 1).leading_zeros()) as u8;
        out.push(bits);
        let v: Vec<i64> = ids.iter().map(|&b| b as i64).collect();
        for l in pack(&v, bits) {
            out.extend_from_slice(&l.to_be_bytes());
        }
    }
}

/// 빛 섹션은 블록 섹션보다 위아래로 하나씩 더 있습니다. 비트 i = (최하단 섹션 - 1 + i).
fn write_light(out: &mut Vec<u8>, col: &Column) {
    let dim = col.dimension;
    let light_sections = dim.section_count() + 2;
    let all = (1u64 << light_sections) - 1;

    let mut sky_mask = 0u64;
    let mut empty_sky_mask = 0u64;
    let mut sky_arrays: Vec<Vec<u8>> = Vec::new();

    if dim.has_skylight() {
        // 기둥마다 위에서 아래로 빛을 내립니다. 옆으로 퍼지는 빛은 계산하지 않습니다.
        let top = dim.min_y() + dim.height();
        let mut light = vec![0u8; light_sections * 4096];
        for z in 0..16 {
            for x in 0..16 {
                let mut level = 15u8;
                // 맨 위 여분 섹션은 늘 15
                for y in (dim.min_y() - 16..top + 16).rev() {
                    if y < top {
                        level = level.saturating_sub(sky_opacity(col.get(x, y, z)));
                    }
                    let ly = (y - (dim.min_y() - 16)) as usize;
                    light[ly * 256 + z * 16 + x] = level;
                }
            }
        }
        for s in 0..light_sections {
            let section = &light[s * 4096..(s + 1) * 4096];
            if section.iter().all(|&l| l == 0) {
                empty_sky_mask |= 1 << s;
                continue;
            }
            sky_mask |= 1 << s;
            let mut nibbles = vec![0u8; 2048];
            for (i, &l) in section.iter().enumerate() {
                nibbles[i / 2] |= l << ((i % 2) * 4);
            }
            sky_arrays.push(nibbles);
        }
    }

    write_bitset(out, sky_mask);
    write_bitset(out, 0); // 블록빛: 없음
    write_bitset(out, empty_sky_mask);
    write_bitset(out, all); // 모든 섹션의 블록빛이 0
    write_varint(out, sky_arrays.len() as i32);
    for a in &sky_arrays {
        write_byte_array(out, a);
    }
    write_varint(out, 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::{read_i64, read_u8};
    use crate::protocol::varint::read_varint;
    use crate::world::{Dimension, terrain};

    #[test]
    fn pack_does_not_straddle_longs() {
        // 9비트 → long 하나에 7개
        let longs = pack(&[1; 256], 9);
        assert_eq!(longs.len(), 37);
        assert_eq!(longs[0], (0..7).map(|i| 1i64 << (i * 9)).sum::<i64>());
    }

    /// 만든 본문을 명세대로 끝까지 읽어 보며 길이가 맞는지 확인합니다.
    #[test]
    fn chunk_body_parses_back() {
        for dim in Dimension::ALL {
            let col = terrain::generate(dim, 7, 0, 0);
            let body = encode(&col, 0, 0);
            let mut buf = body.as_slice();
            buf = &buf[8..];

            let heightmaps = read_varint(&mut buf).unwrap();
            for _ in 0..heightmaps {
                read_varint(&mut buf).unwrap();
                let n = read_varint(&mut buf).unwrap();
                for _ in 0..n {
                    read_i64(&mut buf).unwrap();
                }
            }

            let len = read_varint(&mut buf).unwrap() as usize;
            let (mut data, rest) = buf.split_at(len);
            buf = rest;
            for _ in 0..dim.section_count() {
                data = &data[4..];
                for entries in [4096usize, 64] {
                    let bits = read_u8(&mut data).unwrap() as usize;
                    if bits == 0 {
                        read_varint(&mut data).unwrap();
                        continue;
                    }
                    if bits <= 8 && entries == 4096 {
                        let n = read_varint(&mut data).unwrap();
                        for _ in 0..n {
                            read_varint(&mut data).unwrap();
                        }
                    }
                    let longs = entries.div_ceil(64 / bits);
                    data = &data[longs * 8..];
                }
            }
            assert!(data.is_empty(), "{dim:?}: section bytes left over");

            // 블록 엔티티: 이 시드의 원점 청크에는 없거나 있음 — 개수만큼 건너뜀
            let n = read_varint(&mut buf).unwrap();
            assert_eq!(n, 0, "no block entities expected at origin");
            let mut masks = [0u64; 4];
            for m in &mut masks {
                let n = read_varint(&mut buf).unwrap() as usize;
                let mut b = [0u8; 8];
                b[..n].copy_from_slice(&buf[..n]);
                buf = &buf[n..];
                *m = u64::from_le_bytes(b);
            }
            for mask in [masks[0], masks[1]] {
                let n = read_varint(&mut buf).unwrap();
                assert_eq!(n as u32, mask.count_ones());
                for _ in 0..n {
                    assert_eq!(read_varint(&mut buf).unwrap(), 2048);
                    buf = &buf[2048..];
                }
            }
            assert!(buf.is_empty(), "{dim:?}: trailing bytes");
            assert_eq!(masks[0] & masks[2], 0, "sky light both set and empty");
        }
    }
}
