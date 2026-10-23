//! 여러 인스턴스·담비 자식 프로세스가 함께 쓰는 JSON 파일 하나 (길드, 퀘스트 진행, 설정).
//!
//! - 읽기: 0.5초에 한 번만 파일 시각을 보고, 바뀌었으면 다시 읽습니다.
//! - 쓰기: 잠금 파일(`<파일>.lock`)을 잡고 → 다시 읽고 → 고치고 → 임시 파일에 쓴 뒤 바꿔치기.
//! - 경로가 없으면 메모리에만 (테스트).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use crate::json::{self, Json};

const RECHECK: Duration = Duration::from_millis(500);

pub struct JsonFile<T> {
    path: Option<PathBuf>,
    state: Mutex<State<T>>,
    encode: fn(&T) -> Json,
    decode: fn(&Json) -> Option<T>,
}

struct State<T> {
    data: T,
    mtime: Option<SystemTime>,
    checked: Option<Instant>,
}

impl<T: Clone + PartialEq + Default> JsonFile<T> {
    pub fn in_memory(encode: fn(&T) -> Json, decode: fn(&Json) -> Option<T>) -> Self {
        Self {
            path: None,
            state: Mutex::new(State {
                data: T::default(),
                mtime: None,
                checked: None,
            }),
            encode,
            decode,
        }
    }

    pub fn open(path: PathBuf, encode: fn(&T) -> Json, decode: fn(&Json) -> Option<T>) -> Self {
        let f = Self {
            path: Some(path),
            ..Self::in_memory(encode, decode)
        };
        f.refresh(&mut f.state.lock().unwrap(), true);
        f
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn mtime(path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path).and_then(|m| m.modified()).ok()
    }

    fn refresh(&self, s: &mut State<T>, force: bool) {
        let Some(path) = &self.path else { return };
        if !force && s.checked.is_some_and(|t| t.elapsed() < RECHECK) {
            return;
        }
        s.checked = Some(Instant::now());
        let m = Self::mtime(path);
        // 파일 시각이 거칠게 기록되는 곳에서는 짧은 사이의 두 번 쓰기가 같은 시각일 수 있으므로,
        // 막 바뀐 파일(2초 안)은 시각이 같아도 다시 읽습니다.
        // (시계가 어긋나 파일 시각이 "미래"인 것도 막 바뀐 것으로)
        let fresh = m.is_some_and(|t| t.elapsed().map_or(true, |age| age < Duration::from_secs(2)));
        if m.is_some() && (m != s.mtime || fresh) {
            if let Some(d) = std::fs::read_to_string(path)
                .ok()
                .and_then(|t| json::parse(&t).ok())
                .and_then(|j| (self.decode)(&j))
            {
                s.data = d;
            }
            s.mtime = m;
        }
    }

    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let mut s = self.state.lock().unwrap();
        self.refresh(&mut s, false);
        f(&s.data)
    }

    /// 고치고, 바뀌었으면 저장합니다.
    pub fn write<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut s = self.state.lock().unwrap();
        let _lock = self.path.as_deref().map(FileLock::take);
        self.refresh(&mut s, true);
        let before = s.data.clone();
        let r = f(&mut s.data);
        if s.data != before
            && let Some(path) = &self.path
        {
            let dir = path.parent().unwrap_or(Path::new("."));
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let text = (self.encode)(&s.data).pretty();
            if crate::game::playerdata::write_atomically(dir, &name, text.as_bytes()).is_ok() {
                s.mtime = Self::mtime(path);
            }
        }
        r
    }
}

/// `<파일>.lock`을 만들어 잡고, 놓을 때 지웁니다. 30초 넘게 남은 것은 죽은 프로세스의 것으로 봅니다.
struct FileLock(PathBuf);

impl FileLock {
    fn take(path: &Path) -> FileLock {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(".lock");
        let lock = path.with_file_name(name);
        if let Some(d) = lock.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        for _ in 0..400 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock)
            {
                Ok(_) => return FileLock(lock),
                Err(_) => {
                    let stale = std::fs::metadata(&lock)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .is_some_and(|age| age > Duration::from_secs(30));
                    if stale {
                        let _ = std::fs::remove_file(&lock);
                    } else {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            }
        }
        FileLock(lock)
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// 서버 데이터 폴더 아래 파일 (`MC_DATA`, 기본 `world/`). 모든 인스턴스가 같이 씁니다.
pub fn data_path(env: &str, file: &str) -> PathBuf {
    std::env::var_os(env).map(PathBuf::from).unwrap_or_else(|| {
        std::env::var_os("MC_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("world"))
            .join(file)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::ptr_arg)] // JsonFile<Vec<i64>>의 fn(&T) 모양에 맞춤
    fn enc(v: &Vec<i64>) -> Json {
        Json::Array(v.iter().map(|&n| Json::Number(n as f64)).collect())
    }
    fn dec(j: &Json) -> Option<Vec<i64>> {
        Some(
            j.as_array()?
                .iter()
                .filter_map(|n| n.as_f64().map(|n| n as i64))
                .collect(),
        )
    }

    #[test]
    fn two_handles_on_one_file_see_each_other() {
        let dir = std::env::temp_dir().join(format!("hoya-jsonfile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("x.json");
        let a = JsonFile::open(path.clone(), enc, dec);
        let b = JsonFile::open(path.clone(), enc, dec);
        a.write(|v| v.push(1));
        b.write(|v| v.push(2));
        std::thread::sleep(RECHECK + Duration::from_millis(50));
        assert_eq!(a.read(|v| v.clone()), vec![1, 2]);
        assert!(!dir.join("x.json.lock").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
