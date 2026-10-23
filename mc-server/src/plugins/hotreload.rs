//! 핫 리로드: 서버를 다시 켜지 않고 바뀐 것을 반영합니다.
//!
//! - `contents/` (아이템·블록·이모지·그림)와 `world/equipment.json`(장비 그림이 나오는 곳)을 2초마다
//!   살펴, 바뀌고 나서 잠잠해지면 콘텐츠를 다시 읽고 리소스 팩을 새로 만들어 접속자에게 다시 보냅니다.
//!   담비 마스터면 마스터가 팩을 새로 만들고 모든 자식에게 알립니다.
//! - 그 밖의 `world/*.json` 설정(길드·퀘스트·장비 수치·등급·금지 아이템·퀵메뉴·상점)은 원래 0.5초마다
//!   다시 읽습니다 ([`super::store::JsonFile`]).
//! - `MC_HOT_RELOAD=false`면 끔. 손으로는 `/reload` (`/iareload`).
//! - 플러그인 코드(러스트) 자체는 다시 빌드해 켜야 합니다.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 살피는 간격.
const POLL: Duration = Duration::from_secs(2);

/// 지켜볼 곳들.
pub fn watched() -> Vec<PathBuf> {
    vec![
        super::contents::dir(),
        super::store::data_path("MC_EQUIPMENT", "equipment.json"),
    ]
}

fn walk(path: &Path, h: &mut DefaultHasher) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    path.hash(h);
    if meta.is_dir() {
        let Ok(rd) = std::fs::read_dir(path) else {
            return;
        };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            // 서버가 쓰는 배정 기억은 빼고
            if p.file_name().is_some_and(|n| n == ".cache") {
                continue;
            }
            walk(&p, h);
        }
    } else {
        meta.len().hash(h);
        if let Ok(t) = meta.modified() {
            t.hash(h);
        }
    }
}

/// 파일 이름·크기·고친 때를 모두 섞은 값 (하나라도 바뀌면 달라짐).
pub fn fingerprint(paths: &[PathBuf]) -> u64 {
    let mut h = DefaultHasher::new();
    for p in paths {
        walk(p, &mut h);
    }
    h.finish()
}

pub fn enabled() -> bool {
    std::env::var("MC_HOT_RELOAD").map_or(true, |v| !matches!(v.as_str(), "0" | "false" | "off"))
}

/// 바뀌면 (그리고 한 번 더 살펴 그대로면) `on_change`를 부르는 일을 띄웁니다.
pub fn spawn(on_change: impl Fn() + Send + Sync + 'static) {
    if !enabled() {
        return;
    }
    tokio::spawn(async move {
        let paths = watched();
        let fp = {
            let p = paths.clone();
            move || {
                let p = p.clone();
                async move {
                    tokio::task::spawn_blocking(move || fingerprint(&p))
                        .await
                        .unwrap_or(0)
                }
            }
        };
        let mut seen = fp().await;
        let mut pending: Option<u64> = None;
        let mut tick = tokio::time::interval(POLL);
        tick.tick().await;
        loop {
            tick.tick().await;
            let now = fp().await;
            match pending {
                // 바뀐 뒤 잠잠해졌음 (저장 도중이 아님)
                Some(p) if p == now => {
                    pending = None;
                    seen = now;
                    println!("[핫 리로드] 콘텐츠가 바뀌어 다시 읽고 리소스 팩을 새로 만듭니다");
                    on_change();
                }
                _ if now != seen => pending = Some(now),
                _ => pending = None,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_follows_files_but_not_the_cache() {
        let dir = std::env::temp_dir().join(format!("dangu-hot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("pack/configs")).unwrap();
        std::fs::create_dir_all(dir.join(".cache")).unwrap();
        std::fs::write(dir.join("pack/configs/a.yml"), "a: 1").unwrap();
        let paths = vec![dir.clone()];
        let a = fingerprint(&paths);
        assert_eq!(a, fingerprint(&paths));
        std::fs::write(dir.join(".cache/ids.json"), "{}").unwrap();
        assert_eq!(a, fingerprint(&paths), ".cache는 무시");
        std::fs::write(dir.join("pack/configs/a.yml"), "a: 22").unwrap();
        assert_ne!(a, fingerprint(&paths));
        let b = fingerprint(&paths);
        std::fs::write(dir.join("pack/configs/b.yml"), "").unwrap();
        assert_ne!(b, fingerprint(&paths));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
