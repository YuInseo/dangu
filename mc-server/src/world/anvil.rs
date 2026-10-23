//! 바닐라 Anvil 월드 형식: 영역 파일(`r.X.Z.mca`)과 청크 NBT.
//!
//! 영역 파일 = 32×32 청크. 앞 4KiB는 청크마다 (섹터 위치 3바이트, 섹터 수 1바이트),
//! 다음 4KiB는 저장 시각, 그 뒤 4KiB 섹터들에 `길이(4) | 압축 종류(1) | 데이터`.
//! 우리는 zlib(2)로 쓰고, zlib·gzip(1)·무압축(3)을 읽습니다 (LZ4(4)는 아직 못 읽음).
//!
//! 청크 NBT는 26.3과 같은 모양(DataVersion 5023)으로 씁니다: 섹션마다 블록 팔레트(이름 +
//! 속성)와 바이옴 팔레트, 블록 엔티티(상자 내용·전리품 표·생성기), 높이 맵.
//! 그래서 바닐라 월드를 읽을 수 있고, 우리가 쓴 월드도 바닐라 도구로 열립니다.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::Dimension;
use super::biome::{self, Biome};
use super::column::{Column, SectionView};
use crate::data::{block, blocks};
use crate::packets::world::Item;
use crate::protocol::nbt::Tag;
use crate::protocol::zlib;

/// 26.3의 데이터 판.
pub const DATA_VERSION: i32 = 5023;
const SECTOR: u64 = 4096;
/// 압축을 푼 청크 하나의 상한 (거대한 파일로 메모리를 다 쓰지 않도록).
const MAX_CHUNK: usize = 64 * 1024 * 1024;

/// 차원의 영역 폴더 (바닐라 배치: overworld는 `region/`, 네더 `DIM-1/region/`, 엔드 `DIM1/region/`).
pub fn region_dir(world: &Path, dim: Dimension) -> PathBuf {
    match dim {
        Dimension::Overworld => world.join("region"),
        Dimension::Nether => world.join("DIM-1").join("region"),
        Dimension::End => world.join("DIM1").join("region"),
    }
}

fn region_path(world: &Path, dim: Dimension, cx: i32, cz: i32) -> PathBuf {
    region_dir(world, dim).join(format!("r.{}.{}.mca", cx >> 5, cz >> 5))
}

fn slot(cx: i32, cz: i32) -> u64 {
    ((cx & 31) + (cz & 31) * 32) as u64
}

/// 청크 NBT 읽기. 없으면 Ok(None).
pub fn read_chunk(world: &Path, dim: Dimension, cx: i32, cz: i32) -> io::Result<Option<Tag>> {
    let path = region_path(world, dim, cx, cz);
    let mut f = match File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut loc = [0u8; 4];
    f.seek(SeekFrom::Start(slot(cx, cz) * 4))?;
    if f.read_exact(&mut loc).is_err() {
        return Ok(None);
    }
    let offset = u32::from_be_bytes([0, loc[0], loc[1], loc[2]]) as u64;
    let count = loc[3] as u64;
    if offset < 2 || count == 0 {
        return Ok(None);
    }
    f.seek(SeekFrom::Start(offset * SECTOR))?;
    let mut head = [0u8; 5];
    f.read_exact(&mut head)?;
    let len = u32::from_be_bytes(head[..4].try_into().unwrap()) as usize;
    if len == 0 || len > (count * SECTOR) as usize {
        return Err(io::Error::other(format!(
            "{}: 청크 길이가 잘못됨",
            path.display()
        )));
    }
    let mut data = vec![0u8; len - 1];
    f.read_exact(&mut data)?;
    let raw = match head[4] & 0x7F {
        1 => zlib::gunzip(&data, MAX_CHUNK).map_err(|e| io::Error::other(format!("{e:?}")))?,
        2 => zlib::decompress(&data, MAX_CHUNK).map_err(|e| io::Error::other(format!("{e:?}")))?,
        3 => data,
        4 => return Err(io::Error::other("LZ4로 압축된 청크는 아직 읽지 못합니다")),
        t => return Err(io::Error::other(format!("모르는 압축 종류 {t}"))),
    };
    let (_, tag) = Tag::read_named(&raw)
        .ok_or_else(|| io::Error::other(format!("{}: 청크 NBT가 잘못됨", path.display())))?;
    Ok(Some(tag))
}

/// 청크 NBT 쓰기. 자리가 모자라면 파일 끝에 새로 붙입니다 (바닐라처럼 빈 섹터가 생길 수 있음).
pub fn write_chunk(world: &Path, dim: Dimension, cx: i32, cz: i32, tag: &Tag) -> io::Result<()> {
    let dir = region_dir(world, dim);
    std::fs::create_dir_all(&dir)?;
    let path = region_path(world, dim, cx, cz);
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    if f.metadata()?.len() < 2 * SECTOR {
        f.set_len(2 * SECTOR)?;
    }
    let mut raw = Vec::new();
    tag.write_named("", &mut raw);
    let data = zlib::compress(&raw);
    let mut body = Vec::with_capacity(data.len() + 5);
    body.extend_from_slice(&((data.len() + 1) as u32).to_be_bytes());
    body.push(2);
    body.extend_from_slice(&data);
    let need = (body.len() as u64).div_ceil(SECTOR);
    if need > 255 {
        return Err(io::Error::other("청크가 1MiB를 넘습니다"));
    }

    let mut loc = [0u8; 4];
    f.seek(SeekFrom::Start(slot(cx, cz) * 4))?;
    f.read_exact(&mut loc)?;
    let old_offset = u32::from_be_bytes([0, loc[0], loc[1], loc[2]]) as u64;
    let old_count = loc[3] as u64;
    let offset = if old_offset >= 2 && need <= old_count {
        old_offset
    } else {
        f.metadata()?.len().div_ceil(SECTOR).max(2)
    };
    body.resize((need * SECTOR) as usize, 0);
    f.seek(SeekFrom::Start(offset * SECTOR))?;
    f.write_all(&body)?;
    let entry = ((offset as u32) << 8) | need as u32;
    f.seek(SeekFrom::Start(slot(cx, cz) * 4))?;
    f.write_all(&entry.to_be_bytes())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as u32);
    f.seek(SeekFrom::Start(SECTOR + slot(cx, cz) * 4))?;
    f.write_all(&now.to_be_bytes())?;
    Ok(())
}

// ---- 청크 ↔ NBT ----

/// 팔레트 인덱스들을 long에 채웁니다 (경계를 넘지 않게, 1.16+).
fn pack(values: &[u32], bits: u32) -> Vec<i64> {
    let per = 64 / bits as usize;
    let mut out = vec![0i64; values.len().div_ceil(per)];
    for (i, &v) in values.iter().enumerate() {
        out[i / per] |= (v as i64) << ((i % per) * bits as usize);
    }
    out
}

fn unpack(longs: &[i64], bits: u32, n: usize) -> Vec<u32> {
    let per = (64 / bits.max(1)) as usize;
    let mask = (1u64 << bits) - 1;
    (0..n)
        .map(|i| {
            longs.get(i / per).map_or(0, |l| {
                ((*l as u64 >> ((i % per) * bits as usize)) & mask) as u32
            })
        })
        .collect()
}

fn bits_for(n: usize, min: u32) -> u32 {
    if n <= 1 {
        0
    } else {
        (u32::BITS - (n as u32 - 1).leading_zeros()).max(min)
    }
}

fn state_tag(state: u32) -> Tag {
    let name = blocks::name(blocks::block_of(state));
    let props = blocks::properties(state);
    let mut f = vec![("Name".into(), Tag::String(format!("minecraft:{name}")))];
    if !props.is_empty() {
        f.push((
            "Properties".into(),
            Tag::Compound(
                props
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), Tag::String(v.to_owned())))
                    .collect(),
            ),
        ));
    }
    Tag::Compound(f)
}

fn item_tag(slot: usize, item: Item) -> Tag {
    let mut f = vec![
        ("Slot".into(), Tag::Byte(slot as i8)),
        (
            "id".into(),
            Tag::String(format!("minecraft:{}", crate::data::items::name(item.id))),
        ),
        ("count".into(), Tag::Int(item.count)),
    ];
    if let Some(custom) = item.custom.and_then(crate::game::custom_items::get) {
        f.push((
            "components".into(),
            Tag::Compound(vec![(
                "minecraft:custom_data".into(),
                Tag::Compound(vec![(
                    crate::game::custom_items::TAG_KEY.into(),
                    Tag::String(custom.id.to_string()),
                )]),
            )]),
        ));
    }
    Tag::Compound(f)
}

fn item_from_tag(t: &Tag) -> Option<(usize, Item)> {
    let slot = t.get("Slot")?.as_i64()? as usize;
    let name = t.get("id")?.as_str()?;
    let id = crate::data::items::by_name(name)?;
    let count = t
        .get("count")
        .or_else(|| t.get("Count"))
        .and_then(Tag::as_i64)
        .unwrap_or(1) as i32;
    let custom = t
        .get("components")
        .and_then(|c| c.get("minecraft:custom_data"))
        .and_then(|c| {
            c.get(crate::game::custom_items::TAG_KEY)
                .or_else(|| c.get(crate::game::custom_items::LEGACY_TAG_KEY))
        })
        .and_then(Tag::as_str)
        .and_then(crate::game::custom_items::index_of);
    Some((
        slot,
        Item {
            id,
            count,
            custom,
            serial: 0,
        },
    ))
}

/// 블록 엔티티 이름 (월드 파일용). 클라이언트용 번호표와 같은 규칙.
fn block_entity_name(state: u32) -> Option<&'static str> {
    let n = blocks::name(blocks::block_of(state));
    Some(match n {
        "chest" | "trapped_chest" | "barrel" | "furnace" | "smoker" | "blast_furnace"
        | "ender_chest" | "bell" | "lectern" | "enchanting_table" | "brewing_stand" | "hopper"
        | "dispenser" | "dropper" | "jukebox" | "decorated_pot" | "trial_spawner" | "vault"
        | "beacon" | "conduit" | "crafter" | "potent_sulfur" => n,
        "spawner" => "mob_spawner",
        "campfire" | "soul_campfire" => "campfire",
        "suspicious_sand" | "suspicious_gravel" => "brushable_block",
        _ if n.ends_with("hanging_sign") => "hanging_sign",
        _ if n.ends_with("_sign") => "sign",
        _ if n.ends_with("_banner") => "banner",
        _ if n.ends_with("shulker_box") => "shulker_box",
        _ => return None,
    })
}

/// 상자 칸 가져오기: (차원, 위치) → 칸들 (없으면 None).
pub type ItemsOf<'a> = &'a dyn Fn((i32, i32, i32)) -> Option<Vec<Option<Item>>>;

/// 청크를 NBT로.
pub fn to_nbt(col: &Column, cx: i32, cz: i32, items: ItemsOf) -> Tag {
    let dim = col.dimension;
    let min_section = dim.min_y() >> 4;
    let mut sections = Vec::new();
    for s in 0..dim.section_count() {
        let view = col.section(s);
        let (palette, data): (Vec<u32>, Option<Vec<i64>>) = match view {
            SectionView::Uniform(v) => (vec![v], None),
            SectionView::Full(a) => {
                let mut palette: Vec<u32> = Vec::new();
                let mut index = std::collections::HashMap::new();
                let idx: Vec<u32> = a
                    .iter()
                    .map(|&b| {
                        *index.entry(b).or_insert_with(|| {
                            palette.push(b as u32);
                            palette.len() as u32 - 1
                        })
                    })
                    .collect();
                if palette.len() == 1 {
                    (palette, None)
                } else {
                    let bits = bits_for(palette.len(), 4);
                    (palette, Some(pack(&idx, bits)))
                }
            }
        };
        let mut bs = vec![(
            "palette".into(),
            Tag::List(palette.iter().map(|&s| state_tag(s)).collect()),
        )];
        if let Some(d) = data {
            bs.push(("data".into(), Tag::LongArray(d)));
        }
        // 바이옴
        let cells: Vec<u8> = col.section_biomes(s).to_vec();
        let mut bpal: Vec<u8> = Vec::new();
        for &b in &cells {
            if !bpal.contains(&b) {
                bpal.push(b);
            }
        }
        let mut bio = vec![(
            "palette".into(),
            Tag::List(
                bpal.iter()
                    .map(|&b| Tag::String(format!("minecraft:{}", Biome(b).name())))
                    .collect(),
            ),
        )];
        if bpal.len() > 1 {
            let idx: Vec<u32> = cells
                .iter()
                .map(|b| bpal.iter().position(|p| p == b).unwrap() as u32)
                .collect();
            bio.push((
                "data".into(),
                Tag::LongArray(pack(&idx, bits_for(bpal.len(), 1))),
            ));
        }
        sections.push(Tag::Compound(vec![
            ("Y".into(), Tag::Byte((min_section + s as i32) as i8)),
            ("block_states".into(), Tag::Compound(bs)),
            ("biomes".into(), Tag::Compound(bio)),
        ]));
    }

    // 블록 엔티티
    let mut bes = Vec::new();
    for s in 0..dim.section_count() {
        if let SectionView::Uniform(v) = col.section(s)
            && block_entity_name(v).is_none()
        {
            continue;
        }
        let y0 = dim.min_y() + s as i32 * 16;
        for y in y0..y0 + 16 {
            for z in 0..16 {
                for x in 0..16 {
                    let state = col.get(x, y, z);
                    let Some(name) = block_entity_name(state) else {
                        continue;
                    };
                    let pos = (cx * 16 + x as i32, y, cz * 16 + z as i32);
                    let mut f = vec![
                        ("id".into(), Tag::String(format!("minecraft:{name}"))),
                        ("x".into(), Tag::Int(pos.0)),
                        ("y".into(), Tag::Int(pos.1)),
                        ("z".into(), Tag::Int(pos.2)),
                        ("keepPacked".into(), Tag::Byte(0)),
                    ];
                    if let Some(slots) = items(pos) {
                        let list: Vec<Tag> = slots
                            .iter()
                            .enumerate()
                            .filter_map(|(i, s)| s.map(|it| item_tag(i, it)))
                            .collect();
                        f.push(("Items".into(), Tag::List(list)));
                    } else if let Some((_, table)) = col.loot.iter().find(|(p, _)| *p == pos) {
                        // 아직 안 연 구조물 상자: 바닐라처럼 전리품 표 이름만
                        f.push((
                            "LootTable".into(),
                            Tag::String(format!("minecraft:{table}")),
                        ));
                    }
                    if let Some((_, e)) = col.spawners.iter().find(|(p, _)| *p == pos) {
                        f.push((
                            "SpawnData".into(),
                            Tag::Compound(vec![(
                                "entity".into(),
                                Tag::Compound(vec![(
                                    "id".into(),
                                    Tag::String(format!("minecraft:{e}")),
                                )]),
                            )]),
                        ));
                    }
                    bes.push(Tag::Compound(f));
                }
            }
        }
    }

    Tag::Compound(vec![
        ("DataVersion".into(), Tag::Int(DATA_VERSION)),
        ("xPos".into(), Tag::Int(cx)),
        ("yPos".into(), Tag::Int(min_section)),
        ("zPos".into(), Tag::Int(cz)),
        ("Status".into(), Tag::String("minecraft:full".into())),
        ("LastUpdate".into(), Tag::Long(0)),
        ("InhabitedTime".into(), Tag::Long(0)),
        ("isLightOn".into(), Tag::Byte(0)),
        ("sections".into(), Tag::List(sections)),
        ("block_entities".into(), Tag::List(bes)),
        ("Heightmaps".into(), Tag::Compound(heightmaps(col))),
        ("block_ticks".into(), Tag::List(Vec::new())),
        ("fluid_ticks".into(), Tag::List(Vec::new())),
        (
            "structures".into(),
            Tag::Compound(vec![
                ("References".into(), Tag::Compound(Vec::new())),
                ("starts".into(), Tag::Compound(Vec::new())),
            ]),
        ),
    ])
}

fn heightmaps(col: &Column) -> Vec<(String, Tag)> {
    let dim = col.dimension;
    let bits = u32::BITS - (dim.height() as u32).leading_zeros();
    let mut surface = vec![0u32; 256];
    let mut motion = vec![0u32; 256];
    for z in 0..16 {
        for x in 0..16 {
            for y in (dim.min_y()..dim.min_y() + dim.height()).rev() {
                let s = col.get(x, y, z);
                if s == block::AIR {
                    continue;
                }
                let rel = (y - dim.min_y() + 1) as u32;
                if surface[z * 16 + x] == 0 {
                    surface[z * 16 + x] = rel;
                }
                if !super::features::soft(s) || s == block::WATER {
                    motion[z * 16 + x] = rel;
                    break;
                }
            }
        }
    }
    vec![
        ("WORLD_SURFACE".into(), Tag::LongArray(pack(&surface, bits))),
        (
            "MOTION_BLOCKING".into(),
            Tag::LongArray(pack(&motion, bits)),
        ),
    ]
}

/// 상자 하나: (위치, (칸, 아이템)들).
pub type ChestItems = ((i32, i32, i32), Vec<(usize, Item)>);

/// 읽어 낸 청크와, 그 안 상자들의 내용.
pub struct Loaded {
    pub column: Column,
    pub items: Vec<ChestItems>,
    /// 이름을 몰라 공기로 바꾼 블록 수 (다른 판의 월드).
    pub unknown_blocks: usize,
}

/// NBT를 청크로. 바닐라 월드도 읽습니다 (모르는 블록은 공기, 모르는 바이옴은 차원 기본값).
pub fn from_nbt(tag: &Tag, dim: Dimension) -> Option<Loaded> {
    let fallback = biome::id(match dim {
        Dimension::Overworld => "plains",
        Dimension::Nether => "nether_wastes",
        Dimension::End => "the_end",
    });
    let mut col = Column::new(dim, fallback);
    let min_section = dim.min_y() >> 4;
    let mut unknown = 0;
    let sections = tag.get("sections").or_else(|| tag.get("Sections"))?;
    for sec in sections.as_list() {
        let y = sec.get("Y")?.as_i64()? as i32;
        let s = y - min_section;
        if s < 0 || s as usize >= dim.section_count() {
            continue;
        }
        let s = s as usize;
        if let Some(bs) = sec.get("block_states") {
            let palette: Vec<u16> = bs
                .get("palette")
                .map(|p| {
                    p.as_list()
                        .iter()
                        .map(|e| {
                            let name = e
                                .get("Name")
                                .and_then(Tag::as_str)
                                .unwrap_or("minecraft:air");
                            let props: Vec<(&str, &str)> = e
                                .get("Properties")
                                .map(|p| {
                                    p.fields()
                                        .iter()
                                        .filter_map(|(k, v)| Some((k.as_str(), v.as_str()?)))
                                        .collect()
                                })
                                .unwrap_or_default();
                            blocks::state_from(name, &props).unwrap_or_else(|| {
                                unknown += 1;
                                block::AIR
                            }) as u16
                        })
                        .collect()
                })
                .unwrap_or_default();
            if palette.is_empty() {
                continue;
            }
            let blocks_vec: Vec<u16> = if palette.len() == 1 {
                vec![palette[0]; 4096]
            } else {
                let bits = bits_for(palette.len(), 4);
                let data = bs.get("data").map_or(&[][..], Tag::as_longs);
                unpack(data, bits, 4096)
                    .into_iter()
                    .map(|i| *palette.get(i as usize).unwrap_or(&0))
                    .collect()
            };
            col.put_section(s, &blocks_vec);
        }
        if let Some(bio) = sec.get("biomes") {
            let palette: Vec<Biome> = bio
                .get("palette")
                .map(|p| {
                    p.as_list()
                        .iter()
                        .map(|e| e.as_str().and_then(biome::by_name).unwrap_or(fallback))
                        .collect()
                })
                .unwrap_or_default();
            if !palette.is_empty() {
                let cells: Vec<Biome> = if palette.len() == 1 {
                    vec![palette[0]; 64]
                } else {
                    let data = bio.get("data").map_or(&[][..], Tag::as_longs);
                    unpack(data, bits_for(palette.len(), 1), 64)
                        .into_iter()
                        .map(|i| *palette.get(i as usize).unwrap_or(&fallback))
                        .collect()
                };
                col.set_section_biomes(s, &cells);
            }
        }
    }

    let mut items = Vec::new();
    let bes = tag
        .get("block_entities")
        .or_else(|| tag.get("TileEntities"));
    for be in bes.map(Tag::as_list).unwrap_or(&[]) {
        let pos = (
            be.get("x").and_then(Tag::as_i64)? as i32,
            be.get("y").and_then(Tag::as_i64)? as i32,
            be.get("z").and_then(Tag::as_i64)? as i32,
        );
        if let Some(list) = be.get("Items") {
            items.push((
                pos,
                list.as_list().iter().filter_map(item_from_tag).collect(),
            ));
        } else if let Some(table) = be.get("LootTable").and_then(Tag::as_str) {
            let table = table.strip_prefix("minecraft:").unwrap_or(table);
            if let Some((name, _)) = super::structure::data::LOOT_TABLES
                .iter()
                .find(|(n, _)| *n == table)
            {
                col.loot.push((pos, name));
            }
        }
        if let Some(id) = be
            .get("SpawnData")
            .and_then(|s| s.get("entity"))
            .and_then(|e| e.get("id"))
            .and_then(Tag::as_str)
        {
            let id = id.strip_prefix("minecraft:").unwrap_or(id);
            if let Some(name) = crate::data::ENTITY_NAMES.iter().find(|n| **n == id) {
                col.spawners.push((pos, name));
            }
        }
    }
    Some(Loaded {
        column: col,
        items,
        unknown_blocks: unknown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::terrain;

    #[test]
    fn chunks_survive_a_round_trip_through_region_files() {
        let dir = std::env::temp_dir().join(format!("hoya-anvil-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (dim, cx, cz) in [
            (Dimension::Overworld, 0, 0),
            (Dimension::Overworld, -33, 70),
            (Dimension::Nether, 3, -2),
        ] {
            let mut col = terrain::generate(dim, 11, cx, cz);
            col.set(5, 70, 5, block::STONE);
            let chest = (cx * 16 + 1, 80, cz * 16 + 1);
            col.set(1, 80, 1, crate::world::blockset::b("chest"));
            let items =
                |p: (i32, i32, i32)| (p == chest).then(|| vec![None, Some(Item::new(1, 5))]);
            let tag = to_nbt(&col, cx, cz, &items);
            write_chunk(&dir, dim, cx, cz, &tag).unwrap();
            let back = read_chunk(&dir, dim, cx, cz).unwrap().unwrap();
            let loaded = from_nbt(&back, dim).unwrap();
            assert_eq!(loaded.unknown_blocks, 0);
            for y in (dim.min_y()..dim.min_y() + dim.height()).step_by(3) {
                for (x, z) in [(0, 0), (5, 5), (15, 9), (1, 1)] {
                    assert_eq!(
                        loaded.column.get(x, y, z),
                        col.get(x, y, z),
                        "{dim:?} {x} {y} {z}"
                    );
                }
            }
            assert_eq!(loaded.column.biome_at(3, 64, 3), col.biome_at(3, 64, 3));
            assert_eq!(loaded.items, vec![(chest, vec![(1, Item::new(1, 5))])]);
        }
        // 다시 쓰면(더 커져도) 같은 자리 또는 끝에
        let col = terrain::generate(Dimension::Overworld, 12, 0, 0);
        write_chunk(
            &dir,
            Dimension::Overworld,
            0,
            0,
            &to_nbt(&col, 0, 0, &|_| None),
        )
        .unwrap();
        let back = from_nbt(
            &read_chunk(&dir, Dimension::Overworld, 0, 0)
                .unwrap()
                .unwrap(),
            Dimension::Overworld,
        )
        .unwrap();
        assert_eq!(back.column.get(8, 0, 8), col.get(8, 0, 8));
        assert!(
            read_chunk(&dir, Dimension::Overworld, 5, 5)
                .unwrap()
                .is_none()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
