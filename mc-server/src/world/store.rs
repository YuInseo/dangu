//! 플레이어가 바꾼 블록. 지형은 시드로 언제든 다시 만들 수 있으므로, 바뀐 블록만
//! 기억했다가 청크를 만들 때 덮어씁니다. 디스크에도 그것만 저장합니다.
//!
//! 저장 파일 (`blocks.dat`, 빅엔디언):
//! ```text
//! "DGW1" | u32 개수 | 개수 × (u8 차원, i32 x, i32 y, i32 z, u32 블록 상태)
//! ```

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::Dimension;
use super::terrain::{self, Column};

const MAGIC: &[u8; 4] = b"DGW1";
const FILE_NAME: &str = "blocks.dat";

type ChunkKey = (Dimension, i32, i32);
/// 청크 안 좌표 (x 0..16, 월드 y, z 0..16).
type LocalPos = (u8, i32, u8);

pub struct WorldStore {
    seed: u64,
    dir: Option<PathBuf>,
    changes: Mutex<HashMap<ChunkKey, HashMap<LocalPos, u32>>>,
    dirty: AtomicBool,
    /// 블록 하나를 물을 때마다 청크를 새로 만들지 않도록, 최근에 만든 지형을 몇 개 둡니다.
    terrain: Mutex<TerrainCache>,
}

const TERRAIN_CACHE_CHUNKS: usize = 48;

#[derive(Default)]
struct TerrainCache {
    columns: HashMap<ChunkKey, (Arc<Column>, u64)>,
    clock: u64,
}

impl TerrainCache {
    fn get(&mut self, seed: u64, key: ChunkKey) -> Arc<Column> {
        self.clock += 1;
        let clock = self.clock;
        if let Some((col, used)) = self.columns.get_mut(&key) {
            *used = clock;
            return col.clone();
        }
        if self.columns.len() >= TERRAIN_CACHE_CHUNKS {
            // 가장 오래 안 쓴 것 버리기
            let oldest = *self
                .columns
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .unwrap()
                .0;
            self.columns.remove(&oldest);
        }
        let col = Arc::new(terrain::generate(key.0, seed, key.1, key.2));
        self.columns.insert(key, (col.clone(), clock));
        col
    }
}

fn split(x: i32, z: i32) -> ((i32, i32), (u8, u8)) {
    ((x >> 4, z >> 4), ((x & 15) as u8, (z & 15) as u8))
}

impl WorldStore {
    /// `dir`이 있으면 거기서 읽고 거기에 저장합니다. 없으면 메모리에만 둡니다.
    pub fn open(seed: u64, dir: Option<PathBuf>) -> io::Result<Self> {
        let store = Self {
            seed,
            dir,
            changes: Mutex::new(HashMap::new()),
            dirty: AtomicBool::new(false),
            terrain: Mutex::new(TerrainCache::default()),
        };
        if let Some(dir) = &store.dir {
            let path = dir.join(FILE_NAME);
            match std::fs::File::open(&path) {
                Ok(mut f) => {
                    let mut bytes = Vec::new();
                    f.read_to_end(&mut bytes)?;
                    store.load(&bytes)?;
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(store)
    }

    fn load(&self, bytes: &[u8]) -> io::Result<()> {
        let bad = || io::Error::new(io::ErrorKind::InvalidData, "blocks.dat가 손상되었습니다");
        if bytes.get(..4) != Some(MAGIC) {
            return Err(bad());
        }
        let count =
            u32::from_be_bytes(bytes.get(4..8).ok_or_else(bad)?.try_into().unwrap()) as usize;
        let records = bytes.get(8..).ok_or_else(bad)?;
        if records.len() != count * 17 {
            return Err(bad());
        }
        let int = |r: &[u8], at: usize| i32::from_be_bytes(r[at..at + 4].try_into().unwrap());
        let mut changes = self.changes.lock().unwrap();
        for r in records.chunks(17) {
            let dim = Dimension::from_index(r[0]).ok_or_else(bad)?;
            let (x, y, z, state) = (int(r, 1), int(r, 5), int(r, 9), int(r, 13) as u32);
            if state >= crate::data::BLOCK_STATE_COUNT {
                return Err(bad());
            }
            let ((cx, cz), (lx, lz)) = split(x, z);
            changes
                .entry((dim, cx, cz))
                .or_default()
                .insert((lx, y, lz), state);
        }
        Ok(())
    }

    /// 한 블록의 지금 상태. 바뀐 적 없으면 지형 생성기에 묻습니다.
    pub fn get(&self, dim: Dimension, x: i32, y: i32, z: i32) -> u32 {
        let ((cx, cz), (lx, lz)) = split(x, z);
        if let Some(state) = self
            .changes
            .lock()
            .unwrap()
            .get(&(dim, cx, cz))
            .and_then(|c| c.get(&(lx, y, lz)))
        {
            return *state;
        }
        self.terrain
            .lock()
            .unwrap()
            .get(self.seed, (dim, cx, cz))
            .get(lx as usize, y, lz as usize)
    }

    /// 그 자리가 구조물이 만든(플레이어가 바꾸지 않은) 상자라면 전리품 표.
    pub fn loot_at(&self, dim: Dimension, x: i32, y: i32, z: i32) -> Option<&'static str> {
        let ((cx, cz), (lx, lz)) = split(x, z);
        if self
            .changes
            .lock()
            .unwrap()
            .get(&(dim, cx, cz))
            .is_some_and(|c| c.contains_key(&(lx, y, lz)))
        {
            return None;
        }
        let col = self.terrain.lock().unwrap().get(self.seed, (dim, cx, cz));
        col.loot
            .iter()
            .find(|(p, _)| *p == (x, y, z))
            .map(|(_, t)| *t)
    }

    pub fn set(&self, dim: Dimension, x: i32, y: i32, z: i32, state: u32) {
        let ((cx, cz), (lx, lz)) = split(x, z);
        self.changes
            .lock()
            .unwrap()
            .entry((dim, cx, cz))
            .or_default()
            .insert((lx, y, lz), state);
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// 청크 하나의 바뀐 블록들. 청크 생성을 다른 스레드로 넘길 때 복사해 갑니다.
    pub fn changes_in(&self, dim: Dimension, cx: i32, cz: i32) -> Vec<(LocalPos, u32)> {
        self.changes
            .lock()
            .unwrap()
            .get(&(dim, cx, cz))
            .map(|c| c.iter().map(|(&p, &s)| (p, s)).collect())
            .unwrap_or_default()
    }

    /// 바뀐 블록을 덮어쓴 청크.
    pub fn column(
        seed: u64,
        dim: Dimension,
        cx: i32,
        cz: i32,
        changes: &[(LocalPos, u32)],
    ) -> Column {
        let mut col = terrain::generate(dim, seed, cx, cz);
        for &((x, y, z), state) in changes {
            col.set(x as usize, y, z as usize, state);
        }
        col
    }

    /// 바뀐 것이 있으면 저장합니다. 임시 파일에 다 쓴 뒤 바꿔치기하므로, 도중에 꺼져도
    /// 이전 파일은 남습니다.
    pub fn save(&self) -> io::Result<bool> {
        let Some(dir) = &self.dir else {
            return Ok(false);
        };
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return Ok(false);
        }
        let bytes = self.serialize();
        if let Err(e) = write_atomically(dir, &bytes) {
            self.dirty.store(true, Ordering::Relaxed); // 다음에 다시
            return Err(e);
        }
        Ok(true)
    }

    fn serialize(&self) -> Vec<u8> {
        let changes = self.changes.lock().unwrap();
        let count: usize = changes.values().map(HashMap::len).sum();
        let mut out = Vec::with_capacity(8 + count * 17);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(count as u32).to_be_bytes());
        for (&(dim, cx, cz), blocks) in changes.iter() {
            for (&(lx, y, lz), &state) in blocks {
                out.push(dim.index());
                out.extend_from_slice(&(cx * 16 + lx as i32).to_be_bytes());
                out.extend_from_slice(&y.to_be_bytes());
                out.extend_from_slice(&(cz * 16 + lz as i32).to_be_bytes());
                out.extend_from_slice(&state.to_be_bytes());
            }
        }
        out
    }

    /// 엔티티가 서거나 부딪히는 블록인가 (공기·물·풀처럼 덮어써지는 블록이 아닌 것).
    pub fn is_solid(&self, dim: Dimension, x: i32, y: i32, z: i32) -> bool {
        !crate::data::blocks::is_replaceable(self.get(dim, x, y, z))
    }

    pub fn changed_block_count(&self) -> usize {
        self.changes
            .lock()
            .unwrap()
            .values()
            .map(HashMap::len)
            .sum()
    }
}

fn write_atomically(dir: &Path, bytes: &[u8]) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{FILE_NAME}.tmp"));
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(&tmp, dir.join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::block;

    #[test]
    fn changes_override_terrain_and_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("dangu-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let store = WorldStore::open(5, Some(dir.clone())).unwrap();
        assert_eq!(store.get(Dimension::Overworld, -1, -64, -1), block::BEDROCK);
        store.set(Dimension::Overworld, -1, -64, -1, block::GLOWSTONE);
        store.set(Dimension::Nether, 20, 100, 30, block::OBSIDIAN);
        assert_eq!(
            store.get(Dimension::Overworld, -1, -64, -1),
            block::GLOWSTONE
        );
        assert!(store.save().unwrap());
        assert!(!store.save().unwrap(), "nothing new to save");

        let reopened = WorldStore::open(5, Some(dir.clone())).unwrap();
        assert_eq!(reopened.changed_block_count(), 2);
        assert_eq!(
            reopened.get(Dimension::Overworld, -1, -64, -1),
            block::GLOWSTONE
        );
        assert_eq!(
            reopened.get(Dimension::Nether, 20, 100, 30),
            block::OBSIDIAN
        );
        let col = WorldStore::column(
            5,
            Dimension::Nether,
            1,
            1,
            &reopened.changes_in(Dimension::Nether, 1, 1),
        );
        assert_eq!(col.get(4, 100, 14), block::OBSIDIAN);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_garbage() {
        let store = WorldStore::open(1, None).unwrap();
        assert!(store.load(b"nope").is_err());
        assert!(store.load(b"DGW1\0\0\0\x01short").is_err());
    }
}
