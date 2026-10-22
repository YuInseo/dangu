//! 위에서 내려다본 지도를 PNG로: `cargo run --release --example mapview -- <시드> <x> <z> <반지름> <파일> [biome]`
//! 블록 모드는 맨 위 블록 색(높이로 음영), biome 모드는 바이옴 색.

use mc_server::protocol::zlib;
use mc_server::world::{Dimension, biome, blockset, terrain};

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn png(w: usize, h: usize, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(h * (w * 3 + 1));
    for y in 0..h {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * w * 3..(y + 1) * w * 3]);
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = kind.to_vec();
        c.extend_from_slice(data);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &zlib::compress(&raw));
    chunk(b"IEND", &[]);
    out
}

fn color(name: &str) -> [u8; 3] {
    let table: &[(&str, [u8; 3])] = &[
        ("water", [52, 96, 200]),
        ("lava", [230, 90, 20]),
        ("grass_block", [96, 160, 64]),
        ("sand", [219, 207, 163]),
        ("red_sand", [190, 102, 33]),
        ("sandstone", [200, 185, 130]),
        ("snow", [245, 250, 255]),
        ("ice", [160, 190, 250]),
        ("stone", [125, 125, 125]),
        ("gravel", [140, 130, 128]),
        ("dirt_path", [150, 120, 70]),
        ("dirt", [134, 96, 67]),
        ("podzol", [91, 63, 24]),
        ("mycelium", [111, 99, 105]),
        ("mud", [60, 57, 61]),
        ("terracotta", [160, 90, 60]),
        ("cobblestone", [110, 110, 110]),
        ("oak_leaves", [60, 120, 40]),
        ("birch_leaves", [110, 150, 70]),
        ("spruce_leaves", [50, 90, 60]),
        ("jungle_leaves", [40, 140, 30]),
        ("acacia_leaves", [100, 140, 40]),
        ("dark_oak_leaves", [40, 90, 30]),
        ("cherry_leaves", [240, 170, 200]),
        ("pale_oak_leaves", [180, 190, 180]),
        ("red_poplar", [190, 50, 40]),
        ("orange_poplar", [220, 120, 30]),
        ("yellow_poplar", [230, 200, 50]),
        ("mangrove_leaves", [70, 130, 40]),
        ("planks", [170, 130, 80]),
        ("log", [100, 75, 45]),
        ("stairs", [150, 110, 70]),
        ("slab", [160, 120, 75]),
        ("farmland", [100, 70, 40]),
        ("wheat", [200, 190, 80]),
        ("carrots", [200, 140, 40]),
        ("potatoes", [190, 180, 90]),
        ("beetroots", [150, 60, 60]),
        ("wool", [230, 230, 230]),
        ("obsidian", [30, 20, 50]),
        ("purpur", [170, 120, 170]),
        ("end_stone", [220, 220, 160]),
        ("netherrack", [110, 50, 50]),
        ("nylium", [130, 30, 30]),
        ("soul", [80, 60, 50]),
        ("basalt", [70, 70, 75]),
        ("blackstone", [40, 35, 40]),
        ("glowstone", [250, 220, 120]),
        ("prismarine", [90, 160, 150]),
        ("mushroom_block", [180, 60, 50]),
        ("cactus", [80, 130, 50]),
        ("glass", [200, 230, 240]),
        ("bricks", [130, 130, 130]),
        ("bookshelf", [120, 90, 60]),
        ("packed_ice", [140, 170, 240]),
        ("moss", [90, 130, 50]),
        ("leaf_litter", [170, 110, 50]),
    ];
    table
        .iter()
        .filter(|(k, _)| name.contains(k))
        .max_by_key(|(k, _)| k.len())
        .map_or([200, 0, 200], |(_, c)| *c)
}

fn biome_color(name: &str) -> [u8; 3] {
    let h = name
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33) ^ b as u32);
    if name.contains("ocean") {
        return [30, 60 + (h % 60) as u8, 160 + (h % 80) as u8];
    }
    [
        (h & 0xFF) as u8 / 2 + 64,
        ((h >> 8) & 0xFF) as u8 / 2 + 64,
        ((h >> 16) & 0xFF) as u8 / 2 + 64,
    ]
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let seed: u64 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(20260915);
    let cx0: i32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let cz0: i32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let r: i32 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(128);
    let out = a.get(5).cloned().unwrap_or_else(|| "map.png".into());
    let biome_mode = a.get(6).is_some_and(|m| m == "biome");
    let size = (2 * r) as usize;
    let mut rgb = vec![0u8; size * size * 3];
    if biome_mode {
        let scale = 16;
        for py in 0..size {
            for px in 0..size {
                let (x, z) = (cx0 + (px as i32 - r) * scale, cz0 + (py as i32 - r) * scale);
                let h = biome::surface_height(seed, x, z);
                let c = biome_color(biome::surface_biome(seed, x, z, h).name());
                rgb[(py * size + px) * 3..][..3].copy_from_slice(&c);
            }
        }
    } else {
        let (c0x, c1x) = ((cx0 - r) >> 4, (cx0 + r - 1) >> 4);
        let (c0z, c1z) = ((cz0 - r) >> 4, (cz0 + r - 1) >> 4);
        for ccx in c0x..=c1x {
            for ccz in c0z..=c1z {
                let col = terrain::generate(Dimension::Overworld, seed, ccx, ccz);
                for lz in 0..16 {
                    for lx in 0..16 {
                        let (x, z) = (ccx * 16 + lx as i32, ccz * 16 + lz as i32);
                        let (px, py) = (x - (cx0 - r), z - (cz0 - r));
                        if px < 0 || py < 0 || px >= 2 * r || py >= 2 * r {
                            continue;
                        }
                        let mut y = 319;
                        while y > -64 {
                            let s = col.get(lx, y, lz);
                            let n = blockset::name_of(s);
                            if n != "air"
                                && n != "short_grass"
                                && n != "tall_grass"
                                && n != "fern"
                                && n != "snow"
                            {
                                break;
                            }
                            y -= 1;
                        }
                        let n = blockset::name_of(col.get(lx, y, lz));
                        let mut c = color(n);
                        let shade = ((y - 64) as f32 * 1.2).clamp(-40.0, 60.0);
                        for v in &mut c {
                            *v = (*v as f32 + shade).clamp(0.0, 255.0) as u8;
                        }
                        rgb[(py as usize * size + px as usize) * 3..][..3].copy_from_slice(&c);
                    }
                }
            }
        }
    }
    let k: usize = std::env::var("SCALE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let big = size * k;
    let mut up = vec![0u8; big * big * 3];
    for y in 0..big {
        for x in 0..big {
            let src = ((y / k) * size + x / k) * 3;
            up[(y * big + x) * 3..][..3].copy_from_slice(&rgb[src..src + 3]);
        }
    }
    std::fs::write(&out, png(big, big, &up)).unwrap();
    println!("{out}");
}
