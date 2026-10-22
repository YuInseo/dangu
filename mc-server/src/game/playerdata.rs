//! 플레이어 저장: 위치, 차원, 게임 모드, 체력·배고픔, 인벤토리. 다시 접속하면 그대로.
//!
//! `players/<uuid>.dat` (빅엔디언):
//! ```text
//! "DGP1" | u8 차원 | f64 x y z | f32 yaw pitch | u8 게임 모드 | f32 체력 | u8 배고픔 | f32 포만도
//!        | u8 고른 칸 | u8 아이템 수 | 아이템마다 (u8 칸, i32 아이템, u8 개수)
//! ```

use std::io::{self, Write};
use std::path::Path;

use crate::packets::play::GameMode;
use crate::packets::world::Item;
use crate::world::Dimension;

const MAGIC: &[u8; 4] = b"DGP1";

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerData {
    pub dimension: Dimension,
    pub position: (f64, f64, f64),
    pub yaw: f32,
    pub pitch: f32,
    pub gamemode: GameMode,
    pub health: f32,
    pub food: i32,
    pub saturation: f32,
    pub held: usize,
    pub inventory: [Option<Item>; 46],
}

fn path(dir: &Path, uuid: u128) -> std::path::PathBuf {
    dir.join("players")
        .join(format!("{}.dat", crate::uuid::format_uuid(uuid)))
}

pub fn load(dir: &Path, uuid: u128) -> Option<PlayerData> {
    let b = std::fs::read(path(dir, uuid)).ok()?;
    decode(&b)
}

pub fn save(dir: &Path, uuid: u128, d: &PlayerData) -> io::Result<()> {
    write_atomically(
        &dir.join("players"),
        &format!("{}.dat", crate::uuid::format_uuid(uuid)),
        &encode(d),
    )
}

fn encode(d: &PlayerData) -> Vec<u8> {
    let mut o = MAGIC.to_vec();
    o.push(d.dimension.index());
    for v in [d.position.0, d.position.1, d.position.2] {
        o.extend_from_slice(&v.to_be_bytes());
    }
    for v in [d.yaw, d.pitch] {
        o.extend_from_slice(&v.to_be_bytes());
    }
    o.push(d.gamemode.id() as u8);
    o.extend_from_slice(&d.health.to_be_bytes());
    o.push(d.food.clamp(0, 20) as u8);
    o.extend_from_slice(&d.saturation.to_be_bytes());
    o.push(d.held as u8);
    let items: Vec<_> = d
        .inventory
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.map(|s| (i, s)))
        .collect();
    o.push(items.len() as u8);
    for (i, item) in items {
        o.push(i as u8);
        o.extend_from_slice(&item.id.to_be_bytes());
        o.push(item.count.clamp(0, 255) as u8);
    }
    o
}

fn decode(b: &[u8]) -> Option<PlayerData> {
    let mut r = b;
    let mut take = |n: usize| -> Option<&[u8]> {
        if r.len() < n {
            return None;
        }
        let (h, rest) = r.split_at(n);
        r = rest;
        Some(h)
    };
    if take(4)? != MAGIC {
        return None;
    }
    let f64_ = |b: &[u8]| f64::from_be_bytes(b.try_into().unwrap());
    let f32_ = |b: &[u8]| f32::from_be_bytes(b.try_into().unwrap());
    let dimension = Dimension::from_index(take(1)?[0])?;
    let position = (f64_(take(8)?), f64_(take(8)?), f64_(take(8)?));
    let (yaw, pitch) = (f32_(take(4)?), f32_(take(4)?));
    let gamemode = *GameMode::ALL.get(take(1)?[0] as usize)?;
    let health = f32_(take(4)?);
    let food = take(1)?[0] as i32;
    let saturation = f32_(take(4)?);
    let held = (take(1)?[0] as usize).min(8);
    let n = take(1)?[0] as usize;
    let mut inventory = [None; 46];
    for _ in 0..n {
        let slot = take(1)?[0] as usize;
        let id = i32::from_be_bytes(take(4)?.try_into().unwrap());
        let count = take(1)?[0] as i32;
        *inventory.get_mut(slot)? = Some(Item { id, count });
    }
    let finite = position.0.is_finite()
        && position.1.is_finite()
        && position.2.is_finite()
        && health.is_finite();
    finite.then_some(PlayerData {
        dimension,
        position,
        yaw,
        pitch,
        gamemode,
        health,
        food,
        saturation,
        held,
        inventory,
    })
}

/// 임시 파일에 다 쓴 뒤 바꿔치기 (도중에 꺼져도 이전 파일은 남음).
pub fn write_atomically(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{name}.tmp"));
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(&tmp, dir.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut inventory = [None; 46];
        inventory[36] = Some(Item { id: 1, count: 64 });
        inventory[45] = Some(Item { id: 900, count: 1 });
        let d = PlayerData {
            dimension: Dimension::End,
            position: (1.5, 70.0, -3.25),
            yaw: 90.0,
            pitch: -10.0,
            gamemode: GameMode::Survival,
            health: 13.5,
            food: 17,
            saturation: 2.0,
            held: 3,
            inventory,
        };
        assert_eq!(decode(&encode(&d)), Some(d));
        assert_eq!(decode(b"junk"), None);
    }
}
