//! 라이브 아카이브 공연을 라이브러리에 저장하는 일(받기 목록).
//!
//! 한 번에 한 공연씩 차례로 받고(archive.org에 무리를 주지 않게), 끝나면 라이브러리를 다시
//! 읽는다. 어디에 저장했는지는 기억해 둔다: 같은 공연을 다시 누르면 같은 폴더에 이어 받고,
//! 같은 날 같은 곳의 다른 녹음은 다른 폴더(다른 앨범)가 된다. 취소하면 그 공연에서 받은 것을
//! 지운다. 받다가 실패한 것은 남겨 두었다가 다시 누르면 이어 받는다.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sori_core::Store;
use sori_core::live::save::{self, Cancelled, Plan, Progress};
use sori_core::live::{Archive, LiveShowDetail};

const FILE: &str = "live-saves.json";
/// 끝난 것은 이만큼만 목록에 남긴다.
const KEEP: usize = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Saving,
    Done,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn active(self) -> bool {
        matches!(self, JobState::Queued | JobState::Saving)
    }
}

#[derive(Clone, Debug)]
pub struct Job {
    pub id: String,
    pub state: JobState,
    pub progress: Progress,
    pub error: Option<String>,
    pub plan: Plan,
    /// 받기 전에 이미 다 받아 둔 공연이었나(취소해도 지우지 않는다)
    was_complete: bool,
    cancel: Arc<AtomicBool>,
}

/// 저장한 곳. 앱을 다시 켜도 기억한다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Saved {
    dir: PathBuf,
    complete: bool,
}

#[derive(Default)]
struct State {
    jobs: Vec<Job>,
    saved: HashMap<String, Saved>,
    running: bool,
}

pub struct Saves {
    file: PathBuf,
    state: Mutex<State>,
}

impl Saves {
    pub fn open(data_dir: &Path) -> Saves {
        let file = data_dir.join(FILE);
        let saved = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Saves { file, state: Mutex::new(State { saved, ..Default::default() }) }
    }

    /// 상태를 쥔 채로 부른다(먼저 계산한 것이 나중에 덮어쓰지 않게).
    fn persist(&self, saved: &HashMap<String, Saved>) {
        if let Ok(bytes) = serde_json::to_vec_pretty(saved)
            && let Err(e) = sori_core::config::write_atomic(&self.file, &bytes)
        {
            tracing::warn!("저장 목록을 적지 못했습니다: {e}");
        }
    }

    /// 받기 목록에 넣는다. 이미 받는 중이면 그것을 돌려준다.
    pub fn enqueue(
        self: &Arc<Self>,
        show: &LiveShowDetail,
        music_dir: &Path,
        archive: Arc<Archive>,
        store: Arc<Store>,
    ) -> anyhow::Result<Job> {
        let id = show.show.id.clone();
        let mut s = self.state.lock();
        if let Some(j) = s.jobs.iter().find(|j| j.id == id && j.state.active()) {
            return Ok(j.clone());
        }
        let owners: HashMap<PathBuf, String> = s.saved.iter().map(|(k, v)| (v.dir.clone(), k.clone())).collect();
        let plan = save::plan(show, &save::root(music_dir), |d| owners.get(d).cloned())?;
        let was_complete = s.saved.get(&id).is_some_and(|v| v.complete && v.dir == plan.dir);
        s.saved.insert(id.clone(), Saved { dir: plan.dir.clone(), complete: was_complete });
        let saved = s.saved.clone();
        let job = Job {
            id: id.clone(),
            state: JobState::Queued,
            progress: Progress { files_total: plan.files.len(), bytes_total: plan.total_bytes(), ..Default::default() },
            error: None,
            plan,
            was_complete,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        s.jobs.retain(|j| j.id != id);
        s.jobs.push(job.clone());
        let start = !s.running;
        s.running = true;
        self.persist(&saved);
        drop(s);
        if start {
            let me = self.clone();
            if let Err(e) = std::thread::Builder::new().name("sori-live-save".into()).spawn(move || me.work(&archive, &store)) {
                self.state.lock().running = false;
                return Err(e.into());
            }
        }
        Ok(job)
    }

    /// 차례로 받는다. 받을 것이 없으면 끝난다(다음에 넣으면 다시 뜬다).
    fn work(&self, archive: &Archive, store: &Arc<Store>) {
        loop {
            let next = {
                let mut s = self.state.lock();
                match s.jobs.iter_mut().find(|j| j.state == JobState::Queued) {
                    Some(j) => {
                        j.state = JobState::Saving;
                        j.clone()
                    }
                    None => {
                        s.running = false;
                        return;
                    }
                }
            };
            tracing::info!(id = next.id, dir = %next.plan.dir.display(), "공연을 저장합니다");
            let res = save::run(archive, &next.plan, &next.cancel, |p| {
                if let Some(j) = self.state.lock().jobs.iter_mut().find(|j| j.id == next.id) {
                    j.progress = p;
                }
            });
            let cancelled = res.as_ref().is_err_and(|e| e.is::<Cancelled>());
            if cancelled && !next.was_complete {
                forget_files(&next.plan);
            }
            let mut s = self.state.lock();
            if let Some(j) = s.jobs.iter_mut().find(|j| j.id == next.id) {
                match &res {
                    Ok(()) => j.state = JobState::Done,
                    Err(_) if cancelled => j.state = JobState::Cancelled,
                    Err(e) => {
                        tracing::warn!(id = next.id, "공연을 저장하지 못했습니다: {e:#}");
                        j.state = JobState::Failed;
                        j.error = Some(format!("{e:#}"));
                    }
                }
            }
            match &res {
                Ok(()) => {
                    s.saved.insert(next.id.clone(), Saved { dir: next.plan.dir.clone(), complete: true });
                }
                Err(_) if cancelled && !next.was_complete => {
                    s.saved.remove(&next.id);
                }
                Err(_) => {}
            }
            // 끝난 것이 너무 많으면 오래된 것부터 뺀다.
            while s.jobs.len() > KEEP {
                let Some(i) = s.jobs.iter().position(|j| !j.state.active()) else { break };
                s.jobs.remove(i);
            }
            let saved = s.saved.clone();
            self.persist(&saved);
            drop(s);
            if res.is_ok() {
                store.start_scan();
            }
        }
    }

    pub fn cancel(&self, id: &str) -> Option<Job> {
        let mut s = self.state.lock();
        let j = s.jobs.iter_mut().find(|j| j.id == id)?;
        match j.state {
            JobState::Queued => j.state = JobState::Cancelled,
            JobState::Saving => j.cancel.store(true, Ordering::Relaxed),
            _ => {}
        }
        let job = j.clone();
        // 기다리던 것은 아직 받은 게 없다: 폴더만 잊는다.
        if job.state == JobState::Cancelled && !job.was_complete {
            s.saved.remove(id);
            let saved = s.saved.clone();
            self.persist(&saved);
        }
        Some(job)
    }

    /// 끝난 것을 목록에서 뺀다(받은 파일은 그대로).
    pub fn dismiss(&self, id: &str) -> bool {
        let mut s = self.state.lock();
        let before = s.jobs.len();
        s.jobs.retain(|j| j.id != id || j.state.active());
        s.jobs.len() != before
    }

    pub fn jobs(&self) -> Vec<Job> {
        self.state.lock().jobs.clone()
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.state.lock().jobs.iter().find(|j| j.id == id).cloned()
    }

    /// 다 받아 둔 공연의 폴더
    pub fn saved_dir(&self, id: &str) -> Option<PathBuf> {
        self.state.lock().saved.get(id).filter(|v| v.complete).map(|v| v.dir.clone())
    }
}

/// 취소한 공연에서 받은 곡을 지운다. 계획에 있는 이름만 지우고, 비게 된 폴더만 지운다.
fn forget_files(plan: &Plan) {
    for f in &plan.files {
        let _ = std::fs::remove_file(plan.dir.join(&f.name));
    }
    // 공연 폴더, 밴드 폴더, Live Archive: 비었을 때만 지워진다.
    let mut dir = Some(plan.dir.as_path());
    for _ in 0..3 {
        let Some(d) = dir else { break };
        if std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// 저장한 공연이 라이브러리의 어느 앨범이 됐는지. 그 폴더 안의 곡을 찾는다.
pub fn album_of(store: &Store, dir: &Path) -> Option<String> {
    let rel = dir.strip_prefix(store.music_dir()).ok()?;
    let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
    let prefix = format!("{rel}/");
    let lib = store.library();
    lib.tracks.values().find(|t| t.path.starts_with(&prefix)).map(|t| t.album_id.clone())
}

pub fn job_json(job: &Job, store: &Store) -> Value {
    let p = job.progress;
    json!({
        "id": job.id,
        "band": job.plan.band,
        "album": job.plan.album,
        "state": job.state,
        "filesDone": p.files_done,
        "filesTotal": p.files_total,
        "bytesDone": p.bytes_done,
        "bytesTotal": p.bytes_total,
        "error": job.error,
        "dir": job.plan.dir.to_string_lossy(),
        "albumId": if job.state == JobState::Done { album_of(store, &job.plan.dir) } else { None },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_where_shows_were_saved() {
        let dir = tempfile::tempdir().unwrap();
        let s = Saves::open(dir.path());
        {
            let mut st = s.state.lock();
            st.saved.insert("a".into(), Saved { dir: "/m/x".into(), complete: true });
            st.saved.insert("b".into(), Saved { dir: "/m/y".into(), complete: false });
            let saved = st.saved.clone();
            drop(st);
            s.persist(&saved);
        }
        let again = Saves::open(dir.path());
        assert_eq!(again.saved_dir("a"), Some(PathBuf::from("/m/x")));
        assert_eq!(again.saved_dir("b"), None, "다 받지 못한 것은 저장한 것이 아니다");
        assert_eq!(again.saved_dir("c"), None);
    }

    #[test]
    fn cancelling_removes_only_what_was_saved_for_the_show() {
        let root = tempfile::tempdir().unwrap();
        let show_dir = root.path().join("Live Archive/Band/2020-01-01 Hall");
        std::fs::create_dir_all(&show_dir).unwrap();
        std::fs::write(show_dir.join("01 A.flac"), b"x").unwrap();
        let plan = Plan {
            id: "x".into(),
            band: "Band".into(),
            album: "2020-01-01 Hall".into(),
            date: None,
            dir: show_dir.clone(),
            files: vec![save::PlanFile {
                file: "a.flac".into(),
                title: "A".into(),
                track: 1,
                name: "01 A.flac".into(),
                size: None,
                sha1: None,
            }],
        };
        // 밴드 폴더에 다른 공연이 있으면 밴드 폴더는 남는다.
        std::fs::create_dir_all(root.path().join("Live Archive/Band/other")).unwrap();
        forget_files(&plan);
        assert!(!show_dir.exists());
        assert!(root.path().join("Live Archive/Band/other").is_dir());
        // 이제 비면 위로 올라가며 지운다(음악 폴더 자체는 아니다).
        std::fs::remove_dir(root.path().join("Live Archive/Band/other")).unwrap();
        std::fs::create_dir_all(&show_dir).unwrap();
        forget_files(&plan);
        assert!(!root.path().join("Live Archive").exists());
        assert!(root.path().is_dir());
    }
}
