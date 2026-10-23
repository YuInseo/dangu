//! 상자 내용. 블록 위치마다 27칸을 두고, 바뀐 블록과 같은 방식으로 저장합니다.
//!
//! 저장 파일 (`containers.dat`, 빅엔디언):
//! ```text
//! "DGC2" | u32 상자 수 | 상자마다 (u8 차원, i32 x, i32 y, i32 z, u8 칸 수, 칸마다 (u8 번호, i32 아이템, u8 개수, u8 길이 + 사용자 정의 id))
//! ```

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::packets::world::{BlockPos, Item};
use crate::world::Dimension;

pub const CHEST_SLOTS: usize = 27;
const MAGIC: &[u8; 4] = b"DGC2";
/// 사용자 정의 아이템이 생기기 전 형식 (아이템마다 사용자 정의 이름이 없음).
const MAGIC_V1: &[u8; 4] = b"DGC1";
const FILE_NAME: &str = "containers.dat";

type Key = (Dimension, BlockPos);

pub struct Containers {
    dir: Option<PathBuf>,
    map: Mutex<HashMap<Key, Vec<Option<Item>>>>,
    dirty: AtomicBool,
}

impl Containers {
    pub fn open(dir: Option<PathBuf>) -> io::Result<Self> {
        let c = Self {
            dir,
            map: Mutex::new(HashMap::new()),
            dirty: AtomicBool::new(false),
        };
        if let Some(dir) = &c.dir {
            match std::fs::read(dir.join(FILE_NAME)) {
                Ok(bytes) => c.load(&bytes)?,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(c)
    }

    fn load(&self, b: &[u8]) -> io::Result<()> {
        let bad = || {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "containers.dat가 손상되었습니다",
            )
        };
        let mut r = b;
        let mut take = |n: usize| -> io::Result<&[u8]> {
            if r.len() < n {
                return Err(bad());
            }
            let (h, rest) = r.split_at(n);
            r = rest;
            Ok(h)
        };
        let magic = take(4)?;
        let v2 = magic == MAGIC;
        if !v2 && magic != MAGIC_V1 {
            return Err(bad());
        }
        let int = |b: &[u8]| i32::from_be_bytes(b.try_into().unwrap());
        let count = int(take(4)?) as usize;
        let mut map = self.map.lock().unwrap();
        for _ in 0..count {
            let dim = Dimension::from_index(take(1)?[0]).ok_or_else(bad)?;
            let pos = (int(take(4)?), int(take(4)?), int(take(4)?));
            let n = take(1)?[0] as usize;
            let mut slots = vec![None; CHEST_SLOTS];
            for _ in 0..n {
                let slot = take(1)?[0] as usize;
                let id = int(take(4)?);
                let count = take(1)?[0] as i32;
                let (custom, serial) = if v2 {
                    super::custom_items::read_custom_tag(&mut |n| take(n).ok()).ok_or_else(bad)?
                } else {
                    (None, 0)
                };
                if slot >= CHEST_SLOTS {
                    return Err(bad());
                }
                slots[slot] = Some(Item {
                    id,
                    count,
                    custom,
                    serial,
                });
            }
            map.insert((dim, pos), slots);
        }
        Ok(())
    }

    /// 상자 내용 (없으면 빈 상자를 만듭니다).
    pub fn get(&self, dim: Dimension, pos: BlockPos) -> Vec<Option<Item>> {
        self.map
            .lock()
            .unwrap()
            .entry((dim, pos))
            .or_insert_with(|| vec![None; CHEST_SLOTS])
            .clone()
    }

    pub fn set_slot(&self, dim: Dimension, pos: BlockPos, slot: usize, item: Option<Item>) {
        if slot < CHEST_SLOTS {
            self.map
                .lock()
                .unwrap()
                .entry((dim, pos))
                .or_insert_with(|| vec![None; CHEST_SLOTS])[slot] = item;
            self.dirty.store(true, Ordering::Relaxed);
        }
    }

    /// 한 번이라도 열었거나 채운 상자인가.
    pub fn contains(&self, dim: Dimension, pos: BlockPos) -> bool {
        self.map.lock().unwrap().contains_key(&(dim, pos))
    }

    /// 처음 여는 구조물 상자를 전리품으로 채웁니다.
    pub fn fill(&self, dim: Dimension, pos: BlockPos, slots: Vec<Option<Item>>) {
        let mut slots = slots;
        slots.resize(CHEST_SLOTS, None);
        self.map.lock().unwrap().insert((dim, pos), slots);
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// 상자를 부술 때: 내용을 꺼내고 지웁니다.
    pub fn take_all(&self, dim: Dimension, pos: BlockPos) -> Vec<Item> {
        let removed = self.map.lock().unwrap().remove(&(dim, pos));
        self.dirty.store(true, Ordering::Relaxed);
        removed.unwrap_or_default().into_iter().flatten().collect()
    }

    pub fn save(&self) -> io::Result<bool> {
        let Some(dir) = &self.dir else {
            return Ok(false);
        };
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return Ok(false);
        }
        let map = self.map.lock().unwrap();
        let mut out = MAGIC.to_vec();
        let nonempty: Vec<_> = map
            .iter()
            .filter(|(_, s)| s.iter().any(Option::is_some))
            .collect();
        out.extend_from_slice(&(nonempty.len() as u32).to_be_bytes());
        for ((dim, pos), slots) in nonempty {
            out.push(dim.index());
            for v in [pos.0, pos.1, pos.2] {
                out.extend_from_slice(&v.to_be_bytes());
            }
            let filled: Vec<_> = slots
                .iter()
                .enumerate()
                .filter_map(|(i, s)| s.map(|s| (i, s)))
                .collect();
            out.push(filled.len() as u8);
            for (i, item) in filled {
                out.push(i as u8);
                out.extend_from_slice(&item.id.to_be_bytes());
                out.push(item.count.clamp(0, 255) as u8);
                super::custom_items::write_custom_tag(&mut out, item.custom, item.serial);
            }
        }
        drop(map);
        super::playerdata::write_atomically(dir, FILE_NAME, &out)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chest_contents_survive_restart() {
        let dir = std::env::temp_dir().join(format!("hoya-chest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Containers::open(Some(dir.clone())).unwrap();
        c.set_slot(Dimension::Nether, (1, 70, -2), 13, Some(Item::new(5, 32)));
        assert!(c.save().unwrap());
        let c2 = Containers::open(Some(dir.clone())).unwrap();
        assert_eq!(
            c2.get(Dimension::Nether, (1, 70, -2))[13],
            Some(Item::new(5, 32))
        );
        assert_eq!(
            c2.take_all(Dimension::Nether, (1, 70, -2)),
            vec![Item::new(5, 32)]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
