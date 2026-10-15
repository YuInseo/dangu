//! 좋아요와 플레이리스트. 곡·앨범 ID로만 적어 두고, 보여 줄 때 라이브러리에서 찾는다.
//!
//! 서버에 두므로 같은 소리에 연결한 기기(휴대폰, PC의 화면)가 모두 같은 것을 본다. 파일을 옮겨
//! ID가 바뀐 곡은 보여 줄 때만 빠지고 기록은 남는다(다시 돌아오면 다시 보인다).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::model::now_ms;

/// 플레이리스트 하나에 넣을 수 있는 곡 수
pub const MAX_TRACKS: usize = 10_000;
pub const MAX_PLAYLISTS: usize = 1_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub created: i64,
    pub updated: i64,
    pub tracks: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Liked {
    pub id: String,
    /// 좋아요를 누른 때(ms)
    pub at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    #[serde(default)]
    v: u32,
    /// 최근 것이 앞
    #[serde(default)]
    liked_tracks: Vec<Liked>,
    #[serde(default)]
    liked_albums: Vec<Liked>,
    #[serde(default)]
    playlists: Vec<Playlist>,
}

pub struct Collections {
    path: PathBuf,
    data: RwLock<File>,
    generation: AtomicU64,
}

/// 곡·앨범 ID 모양(라이브러리의 ID는 base64url 12자)
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn clean_name(name: &str) -> Option<String> {
    let n: String = name.trim().chars().filter(|c| !c.is_control()).take(100).collect();
    (!n.is_empty()).then_some(n)
}

fn new_id() -> String {
    let mut rng = rand::rng();
    (0..10).map(|_| char::from(b"abcdefghijkmnopqrstuvwxyz23456789"[rng.random_range(0..33)])).collect()
}

impl Collections {
    pub fn open(data_dir: &Path) -> Collections {
        let path = data_dir.join("collections.json");
        let data: File = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Collections { path, data: RwLock::new(data), generation: AtomicU64::new(1) }
    }

    /// 바뀔 때마다 오른다(화면이 다시 읽을 때를 안다).
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    fn save(&self, data: &File) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        match serde_json::to_vec(data) {
            Ok(bytes) => {
                if let Err(e) = crate::config::write_atomic(&self.path, &bytes) {
                    tracing::warn!("좋아요·플레이리스트를 저장하지 못했습니다: {e}");
                }
            }
            Err(e) => tracing::warn!("좋아요·플레이리스트를 저장하지 못했습니다: {e}"),
        }
    }

    pub fn liked_tracks(&self) -> Vec<Liked> {
        self.data.read().liked_tracks.clone()
    }

    pub fn liked_albums(&self) -> Vec<Liked> {
        self.data.read().liked_albums.clone()
    }

    fn set_liked(list: &mut Vec<Liked>, id: &str, on: bool) -> bool {
        let had = list.iter().any(|l| l.id == id);
        if on && !had {
            list.insert(0, Liked { id: id.to_string(), at: now_ms() });
        } else if !on && had {
            list.retain(|l| l.id != id);
        }
        had != on
    }

    /// 바뀌었으면 true
    pub fn like_track(&self, id: &str, on: bool) -> bool {
        let mut d = self.data.write();
        let changed = Self::set_liked(&mut d.liked_tracks, id, on);
        if changed {
            self.save(&d);
        }
        changed
    }

    pub fn like_album(&self, id: &str, on: bool) -> bool {
        let mut d = self.data.write();
        let changed = Self::set_liked(&mut d.liked_albums, id, on);
        if changed {
            self.save(&d);
        }
        changed
    }

    /// 최근에 고친 것이 앞
    pub fn playlists(&self) -> Vec<Playlist> {
        let mut v = self.data.read().playlists.clone();
        v.sort_by_key(|p| std::cmp::Reverse(p.updated));
        v
    }

    pub fn playlist(&self, id: &str) -> Option<Playlist> {
        self.data.read().playlists.iter().find(|p| p.id == id).cloned()
    }

    pub fn create_playlist(&self, name: &str, tracks: Vec<String>) -> anyhow::Result<Playlist> {
        let name = clean_name(name).ok_or_else(|| anyhow::anyhow!("이름이 비었습니다"))?;
        let tracks: Vec<String> = tracks.into_iter().filter(|t| valid_id(t)).take(MAX_TRACKS).collect();
        let mut d = self.data.write();
        if d.playlists.len() >= MAX_PLAYLISTS {
            anyhow::bail!("플레이리스트가 너무 많습니다");
        }
        let now = now_ms();
        let p = Playlist { id: new_id(), name, created: now, updated: now, tracks };
        d.playlists.push(p.clone());
        self.save(&d);
        Ok(p)
    }

    /// 이름이나 곡 목록(순서 바꾸기·빼기)을 바꾼다.
    pub fn update_playlist(&self, id: &str, name: Option<&str>, tracks: Option<Vec<String>>) -> anyhow::Result<Option<Playlist>> {
        let mut d = self.data.write();
        let Some(p) = d.playlists.iter_mut().find(|p| p.id == id) else { return Ok(None) };
        if let Some(n) = name {
            p.name = clean_name(n).ok_or_else(|| anyhow::anyhow!("이름이 비었습니다"))?;
        }
        if let Some(t) = tracks {
            p.tracks = t.into_iter().filter(|t| valid_id(t)).take(MAX_TRACKS).collect();
        }
        p.updated = now_ms();
        let out = p.clone();
        self.save(&d);
        Ok(Some(out))
    }

    /// 끝에 곡들을 붙인다. 이미 있는 곡도 다시 넣을 수 있다(같은 곡 두 번).
    pub fn add_to_playlist(&self, id: &str, ids: Vec<String>) -> Option<Playlist> {
        let mut d = self.data.write();
        let p = d.playlists.iter_mut().find(|p| p.id == id)?;
        let room = MAX_TRACKS.saturating_sub(p.tracks.len());
        p.tracks.extend(ids.into_iter().filter(|t| valid_id(t)).take(room));
        p.updated = now_ms();
        let out = p.clone();
        self.save(&d);
        Some(out)
    }

    pub fn delete_playlist(&self, id: &str) -> bool {
        let mut d = self.data.write();
        let before = d.playlists.len();
        d.playlists.retain(|p| p.id != id);
        let changed = d.playlists.len() != before;
        if changed {
            self.save(&d);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn likes_and_playlists_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let c = Collections::open(dir.path());
        let g0 = c.generation();
        assert!(c.like_track("aaaaaaaaaaaa", true));
        assert!(!c.like_track("aaaaaaaaaaaa", true), "두 번 눌러도 하나");
        assert!(c.like_track("bbbbbbbbbbbb", true));
        assert!(c.like_album("album1", true));
        assert!(c.generation() > g0);
        assert_eq!(c.liked_tracks().iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), ["bbbbbbbbbbbb", "aaaaaaaaaaaa"], "최근 것이 앞");

        let p = c.create_playlist("  밤 드라이브  ", vec!["t1".into(), "../x".into(), "t2".into()]).unwrap();
        assert_eq!(p.name, "밤 드라이브");
        assert_eq!(p.tracks, ["t1", "t2"], "이상한 ID는 뺀다");
        assert!(c.create_playlist("   ", vec![]).is_err());
        let p = c.add_to_playlist(&p.id, vec!["t3".into(), "t1".into()]).unwrap();
        assert_eq!(p.tracks, ["t1", "t2", "t3", "t1"]);
        let p = c.update_playlist(&p.id, Some("새벽"), Some(vec!["t3".into(), "t1".into()])).unwrap().unwrap();
        assert_eq!((p.name.as_str(), p.tracks.len()), ("새벽", 2));
        assert!(c.update_playlist("nope", Some("x"), None).unwrap().is_none());

        // 다시 열어도 그대로
        let c2 = Collections::open(dir.path());
        assert_eq!(c2.liked_tracks().len(), 2);
        assert_eq!(c2.liked_albums()[0].id, "album1");
        assert_eq!(c2.playlist(&p.id).unwrap().tracks, ["t3", "t1"]);
        assert!(c2.like_track("aaaaaaaaaaaa", false));
        assert!(c2.delete_playlist(&p.id));
        assert!(!c2.delete_playlist(&p.id));
        assert!(Collections::open(dir.path()).playlists().is_empty());
    }
}
