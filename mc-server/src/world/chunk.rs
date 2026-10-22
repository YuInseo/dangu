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

use super::terrain::Column;
use crate::data::{BLOCK_STATE_COUNT, block};
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
        write_section(&mut sections, col.section(s), dim.biome_id());
    }
    write_byte_array(&mut out, &sections);

    write_varint(&mut out, 0); // 블록 엔티티

    write_light(&mut out, col);
    out
}

fn blocks_motion(state: u32) -> bool {
    !matches!(state, block::AIR | block::SHORT_GRASS)
}

fn is_fluid(state: u32) -> bool {
    matches!(state, block::WATER | block::LAVA)
}

/// 하늘빛이 이 블록을 지날 때 줄어드는 양. 15면 완전히 막힘.
fn sky_opacity(state: u32) -> u8 {
    match state {
        block::AIR | block::SHORT_GRASS => 0,
        block::WATER | block::OAK_LEAVES => 1,
        _ => 15,
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

fn write_section(out: &mut Vec<u8>, blocks: &[u32], biome: i32) {
    let non_air = blocks.iter().filter(|&&b| b != block::AIR).count() as i16;
    let fluids = blocks.iter().filter(|&&b| is_fluid(b)).count() as i16;
    out.extend_from_slice(&non_air.to_be_bytes());
    out.extend_from_slice(&fluids.to_be_bytes());

    // 블록: 한 가지면 단일 값, 256가지 이하면 간접 팔레트, 그 이상이면 전역 ID.
    let mut palette: Vec<u32> = Vec::new();
    for &b in blocks {
        if !palette.contains(&b) {
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
        let indices: Vec<i64> = blocks
            .iter()
            .map(|b| palette.iter().position(|p| p == b).unwrap() as i64)
            .collect();
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

    // 바이옴: 차원마다 하나뿐이므로 늘 단일 값.
    out.push(0);
    write_varint(out, biome);
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

            assert_eq!(read_varint(&mut buf).unwrap(), 0);
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
