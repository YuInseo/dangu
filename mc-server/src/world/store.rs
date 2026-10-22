//! 월드 저장소: 메모리에 올린 청크들과 바닐라 Anvil 영역 파일(`region/r.X.Z.mca`).
//!
//! 청크가 필요하면 영역 파일에서 읽고, 없으면 지형 생성기로 만듭니다. 만든 청크와 바뀐
//! 청크는 "저장할 것"으로 표시했다가 [`WorldStore::save`]에서 영역 파일에 씁니다. 메모리에는
//! 최근에 쓴 청크를 [`CACHE_CHUNKS`]개까지만 두고, 오래된 것은 (저장한 뒤) 내립니다.
//!
//! 예전 형식(`blocks.dat`: 바뀐 블록 목록)은 처음 열 때 영역 파일로 옮기고 `blocks.dat.old`로
//! 이름을 바꿉니다.

use std::collections::HashMap;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use super::Dimension;
use super::anvil;
use super::terrain::{self, Column};
use crate::game::containers::Containers;

const LEGACY_MAGIC: &[u8; 4] = b"DGW1";
const LEGACY_FILE: &str = "blocks.dat";

/// 메모리에 둘 청크 수 (섹션 압축으로 청크 하나가 대략 50~150KB).
pub const CACHE_CHUNKS: usize = 1536;

type ChunkKey = (Dimension, i32, i32);

struct Entry {
    col: Arc<Column>,
    dirty: bool,
    used: u64,
}

#[derive(Default)]
struct Cache {
    map: HashMap<ChunkKey, Entry>,
    clock: u64,
}

pub struct WorldStore {
    seed: u64,
    dir: Option<PathBuf>,
    cache: Mutex<Cache>,
    /// 영역 파일은 한 번에 하나씩 씁니다.
    io: Mutex<()>,
    /// 상자 내용: 저장할 때 블록 엔티티에 넣고, 바닐라 월드를 읽을 때 채웁니다.
    containers: OnceLock<Arc<Containers>>,
    /// 켠 뒤 바뀐 블록 수.
    changed: AtomicUsize,
}

fn split(x: i32, z: i32) -> ((i32, i32), (usize, usize)) {
    ((x >> 4, z >> 4), ((x & 15) as usize, (z & 15) as usize))
}

impl WorldStore {
    /// `dir`이 있으면 거기서 읽고 거기에 저장합니다. 없으면 메모리에만 둡니다.
    pub fn open(seed: u64, dir: Option<PathBuf>) -> io::Result<Self> {
        let store = Self {
            seed,
            dir,
            cache: Mutex::new(Cache::default()),
            io: Mutex::new(()),
            containers: OnceLock::new(),
            changed: AtomicUsize::new(0),
        };
        store.migrate_legacy()?;
        Ok(store)
    }

    /// 상자 내용을 함께 저장하도록 연결합니다.
    pub fn attach_containers(&self, containers: Arc<Containers>) {
        let _ = self.containers.set(containers);
    }

    /// 예전 `blocks.dat`(바뀐 블록 목록)를 영역 파일로 옮깁니다.
    fn migrate_legacy(&self) -> io::Result<()> {
        let Some(dir) = &self.dir else { return Ok(()) };
        let path = dir.join(LEGACY_FILE);
        let mut bytes = Vec::new();
        match std::fs::File::open(&path) {
            Ok(mut f) => f.read_to_end(&mut bytes)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        let changes = parse_legacy(&bytes)?;
        let n = changes.len();
        for (dim, x, y, z, state) in changes {
            self.set(dim, x, y, z, state);
        }
        self.save()?;
        std::fs::rename(&path, dir.join(format!("{LEGACY_FILE}.old")))?;
        println!(
            "{}: 예전 형식의 바뀐 블록 {n}개를 영역 파일로 옮겼습니다",
            dir.display()
        );
        Ok(())
    }

    /// 청크 하나 (메모리 → 영역 파일 → 지형 생성 순으로).
    pub fn chunk(&self, dim: Dimension, cx: i32, cz: i32) -> Arc<Column> {
        let key = (dim, cx, cz);
        {
            let mut c = self.cache.lock().unwrap();
            c.clock += 1;
            let clock = c.clock;
            if let Some(e) = c.map.get_mut(&key) {
                e.used = clock;
                return e.col.clone();
            }
        }
        let (col, fresh) = self.load_or_generate(dim, cx, cz);
        self.insert(key, col, fresh)
    }

    fn load_or_generate(&self, dim: Dimension, cx: i32, cz: i32) -> (Column, bool) {
        if let Some(dir) = &self.dir {
            match anvil::read_chunk(dir, dim, cx, cz) {
                Ok(Some(tag)) => {
                    if let Some(loaded) = anvil::from_nbt(&tag, dim) {
                        if loaded.unknown_blocks > 0 {
                            eprintln!(
                                "청크 {cx},{cz}: 모르는 블록 {}개를 공기로 읽었습니다",
                                loaded.unknown_blocks
                            );
                        }
                        if let Some(containers) = self.containers.get() {
                            for (pos, items) in loaded.items {
                                if !containers.contains(dim, pos) {
                                    let mut slots =
                                        vec![None; crate::game::containers::CHEST_SLOTS];
                                    for (i, it) in items {
                                        if let Some(s) = slots.get_mut(i) {
                                            *s = Some(it);
                                        }
                                    }
                                    containers.fill(dim, pos, slots);
                                }
                            }
                        }
                        return (loaded.column, false);
                    }
                    eprintln!("청크 {cx},{cz}: 읽지 못해 새로 만듭니다");
                }
                Ok(None) => {}
                Err(e) => eprintln!("청크 {cx},{cz}: {e} — 새로 만듭니다"),
            }
        }
        (terrain::generate(dim, self.seed, cx, cz), true)
    }

    /// 넣고(다른 스레드가 먼저 넣었으면 그것을), 넘치면 오래된 청크를 내립니다.
    fn insert(&self, key: ChunkKey, col: Column, dirty: bool) -> Arc<Column> {
        let (result, evicted) = {
            let mut c = self.cache.lock().unwrap();
            c.clock += 1;
            let clock = c.clock;
            let result = c
                .map
                .entry(key)
                .or_insert_with(|| Entry {
                    col: Arc::new(col),
                    // 저장할 곳이 없으면 굳이 표시하지 않음
                    dirty: dirty && self.dir.is_some(),
                    used: clock,
                })
                .col
                .clone();
            let mut evicted = Vec::new();
            if c.map.len() > CACHE_CHUNKS {
                let mut by_age: Vec<(ChunkKey, u64)> =
                    c.map.iter().map(|(k, e)| (*k, e.used)).collect();
                by_age.sort_by_key(|(_, u)| *u);
                let excess = c.map.len() - CACHE_CHUNKS * 9 / 10;
                for (k, _) in by_age.into_iter().take(excess) {
                    if let Some(e) = c.map.remove(&k)
                        && e.dirty
                    {
                        evicted.push((k, e.col));
                    }
                }
            }
            (result, evicted)
        };
        for (k, col) in evicted {
            if let Err(e) = self.write(k, &col) {
                eprintln!("청크 {k:?} 저장 실패: {e}");
            }
        }
        result
    }

    fn write(&self, (dim, cx, cz): ChunkKey, col: &Column) -> io::Result<()> {
        let Some(dir) = &self.dir else { return Ok(()) };
        let containers = self.containers.get();
        let items = |pos: (i32, i32, i32)| -> Option<Vec<Option<crate::packets::world::Item>>> {
            let c = containers?;
            c.contains(dim, pos).then(|| c.get(dim, pos))
        };
        let tag = anvil::to_nbt(col, cx, cz, &items);
        let _guard = self.io.lock().unwrap();
        anvil::write_chunk(dir, dim, cx, cz, &tag)
    }

    /// 이미 메모리에 있는 청크만 (없으면 읽거나 만들지 않음 — 무작위 틱처럼 자주 묻는 곳).
    pub fn chunk_if_loaded(&self, dim: Dimension, cx: i32, cz: i32) -> Option<Arc<Column>> {
        self.cache
            .lock()
            .unwrap()
            .map
            .get(&(dim, cx, cz))
            .map(|e| e.col.clone())
    }

    /// 청크 하나를 직접 고칩니다 (생성기·전리품 목록 등 블록 밖의 것).
    pub fn edit_chunk(&self, dim: Dimension, cx: i32, cz: i32, f: impl FnOnce(&mut Column)) {
        let key = (dim, cx, cz);
        let persist = self.dir.is_some();
        loop {
            self.chunk(dim, cx, cz);
            let mut c = self.cache.lock().unwrap();
            if let Some(e) = c.map.get_mut(&key) {
                f(Arc::make_mut(&mut e.col));
                e.dirty = persist;
                return;
            }
        }
    }

    /// 한 블록의 지금 상태.
    pub fn get(&self, dim: Dimension, x: i32, y: i32, z: i32) -> u32 {
        let ((cx, cz), (lx, lz)) = split(x, z);
        self.chunk(dim, cx, cz).get(lx, y, lz)
    }

    pub fn set(&self, dim: Dimension, x: i32, y: i32, z: i32, state: u32) {
        let ((cx, cz), (lx, lz)) = split(x, z);
        let key = (dim, cx, cz);
        let persist = self.dir.is_some();
        loop {
            // 먼저 올려 둠 (자물쇠 밖에서 읽거나 만들도록)
            self.chunk(dim, cx, cz);
            let mut c = self.cache.lock().unwrap();
            c.clock += 1;
            let clock = c.clock;
            if let Some(e) = c.map.get_mut(&key) {
                Arc::make_mut(&mut e.col).set(lx, y, lz, state);
                e.dirty = persist;
                e.used = clock;
                break;
            }
            // 그 사이 다른 스레드가 내렸으면 다시 (드묾)
        }
        self.changed.fetch_add(1, Ordering::Relaxed);
    }

    /// 저장할 청크들을 영역 파일에 씁니다. 쓴 것이 있으면 true.
    pub fn save(&self) -> io::Result<bool> {
        if self.dir.is_none() {
            return Ok(false);
        }
        let dirty: Vec<(ChunkKey, Arc<Column>)> = {
            let mut c = self.cache.lock().unwrap();
            c.map
                .iter_mut()
                .filter(|(_, e)| e.dirty)
                .map(|(k, e)| {
                    e.dirty = false;
                    (*k, e.col.clone())
                })
                .collect()
        };
        let mut first_err = None;
        for (k, col) in &dirty {
            if let Err(e) = self.write(*k, col) {
                // 다음 저장 때 다시
                if let Some(entry) = self.cache.lock().unwrap().map.get_mut(k) {
                    entry.dirty = true;
                }
                first_err.get_or_insert(e);
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(!dirty.is_empty()),
        }
    }

    /// 스폰 둘레를 미리 만들어 저장합니다 (처음 켤 때). 여러 스레드로 만듭니다.
    pub fn pregenerate(
        &self,
        dim: Dimension,
        center: (i32, i32),
        radius: i32,
    ) -> io::Result<usize> {
        let mut keys: Vec<(i32, i32)> = Vec::new();
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                keys.push((center.0 + dx, center.1 + dz));
            }
        }
        keys.sort_by_key(|(x, z)| (x - center.0).pow(2) + (z - center.1).pow(2));
        let threads = std::thread::available_parallelism()
            .map_or(2, |n| n.get())
            .min(8);
        let total = keys.len();
        let seed = self.seed;
        let mut done = 0;
        for batch in keys.chunks(256) {
            let columns: Vec<((i32, i32), Column)> = std::thread::scope(|s| {
                let parts: Vec<_> = batch
                    .chunks(batch.len().div_ceil(threads))
                    .map(|part| {
                        s.spawn(move || {
                            part.iter()
                                .map(|&(x, z)| ((x, z), terrain::generate(dim, seed, x, z)))
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect();
                parts.into_iter().flat_map(|h| h.join().unwrap()).collect()
            });
            done += columns.len();
            for ((x, z), col) in columns {
                self.insert((dim, x, z), col, true);
            }
            self.save()?;
            println!("  미리 만들기: {done}/{total} 청크");
        }
        Ok(total)
    }

    /// 엔티티가 서거나 부딪히는 블록인가 (공기·물·풀·꽃·횃불처럼 충돌 모양이 없는 것이 아닌 것).
    pub fn is_solid(&self, dim: Dimension, x: i32, y: i32, z: i32) -> bool {
        crate::data::blocks::has_collision(self.get(dim, x, y, z))
    }

    /// 켠 뒤 바뀐 블록 수.
    pub fn changed_block_count(&self) -> usize {
        self.changed.load(Ordering::Relaxed)
    }

    /// 메모리에 올라 있는 청크 수.
    pub fn loaded_chunks(&self) -> usize {
        self.cache.lock().unwrap().map.len()
    }

    /// 구조물이 만든(아직 아무도 연 적 없는) 상자라면 전리품 표.
    pub fn loot_at(&self, dim: Dimension, x: i32, y: i32, z: i32) -> Option<&'static str> {
        let ((cx, cz), _) = split(x, z);
        let col = self.chunk(dim, cx, cz);
        col.loot
            .iter()
            .find(|(p, _)| *p == (x, y, z))
            .map(|(_, t)| *t)
    }
}

/// 예전 `blocks.dat`: "DGW1" | u32 개수 | 개수 × (u8 차원, i32 x, y, z, u32 상태).
type LegacyChange = (Dimension, i32, i32, i32, u32);

fn parse_legacy(bytes: &[u8]) -> io::Result<Vec<LegacyChange>> {
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "blocks.dat가 손상되었습니다");
    if bytes.get(..4) != Some(LEGACY_MAGIC) {
        return Err(bad());
    }
    let count = u32::from_be_bytes(bytes.get(4..8).ok_or_else(bad)?.try_into().unwrap()) as usize;
    let records = bytes.get(8..).ok_or_else(bad)?;
    if records.len() != count * 17 {
        return Err(bad());
    }
    let int = |r: &[u8], at: usize| i32::from_be_bytes(r[at..at + 4].try_into().unwrap());
    records
        .chunks(17)
        .map(|r| {
            let dim = Dimension::from_index(r[0]).ok_or_else(bad)?;
            let state = int(r, 13) as u32;
            if state >= crate::data::BLOCK_STATE_COUNT {
                return Err(bad());
            }
            Ok((dim, int(r, 1), int(r, 5), int(r, 9), state))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::block;

    #[test]
    fn changes_survive_a_restart_in_region_files() {
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
        assert!(dir.join("region/r.-1.-1.mca").exists());
        assert!(dir.join("DIM-1/region/r.0.0.mca").exists());

        let reopened = WorldStore::open(5, Some(dir.clone())).unwrap();
        assert_eq!(
            reopened.get(Dimension::Overworld, -1, -64, -1),
            block::GLOWSTONE
        );
        assert_eq!(
            reopened.get(Dimension::Nether, 20, 100, 30),
            block::OBSIDIAN
        );
        assert_eq!(
            reopened.chunk(Dimension::Nether, 1, 1).get(4, 100, 14),
            block::OBSIDIAN
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn legacy_block_list_is_migrated() {
        let dir = std::env::temp_dir().join(format!("dangu-legacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut bytes = LEGACY_MAGIC.to_vec();
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.push(0);
        for v in [3i32, 70, -9] {
            bytes.extend_from_slice(&v.to_be_bytes());
        }
        bytes.extend_from_slice(&block::GLOWSTONE.to_be_bytes());
        std::fs::write(dir.join(LEGACY_FILE), &bytes).unwrap();
        let store = WorldStore::open(5, Some(dir.clone())).unwrap();
        assert_eq!(store.get(Dimension::Overworld, 3, 70, -9), block::GLOWSTONE);
        assert!(dir.join("blocks.dat.old").exists());
        assert!(parse_legacy(b"nope").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
