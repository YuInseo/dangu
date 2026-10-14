//! 프로세스에 하나뿐인 라이브러리.
//!
//! 처음 열 때 디스크의 색인을 읽어 바로 보여 주고, 뒤에서 조용히 다시 스캔해 그 사이
//! 더해지거나 빠진 파일을 맞춘다. 스캔이 끝나면 아직 분석하지 않은 곡을 분석한다.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

use crate::analysis::{AnalysisFile, analyze_file, to_entry};
use crate::config::{Config, write_atomic};
use crate::library::Library;
use crate::model::*;
use crate::scan::{INDEX_VERSION, IndexFile, scan_library};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// 백그라운드 오디오 분석 속도. 룬의 "Background audio analysis speed".
    pub analysis_speed: AnalysisSpeed,
    /// 설정 화면에서 고른 음악 폴더. 비어 있으면 실행할 때 받은 폴더.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music_dir: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { analysis_speed: AnalysisSpeed::Throttled, music_dir: None }
    }
}

struct State {
    lib: Arc<Library>,
    index: Option<Arc<IndexFile>>,
    status: ScanStatus,
    analysis: AnalysisFile,
    analysis_status: AnalysisStatus,
    settings: Settings,
}

pub struct Store {
    pub config: Config,
    state: RwLock<State>,
    scanning: AtomicBool,
    rescan: AtomicBool,
    run: Mutex<Option<Arc<AtomicBool>>>,
    generation: AtomicU64,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &std::path::Path) -> Option<T> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

impl Store {
    /// 디스크에 있는 것만 읽는다. 스캔은 `start_scan`이 시작한다.
    pub fn open(config: Config) -> Arc<Store> {
        let _ = std::fs::create_dir_all(&config.data_dir);
        let settings: Settings = read_json(&config.data_file("settings.json")).unwrap_or_default();
        let analysis: AnalysisFile = read_json(&config.data_file("analysis.json"))
            .filter(|a: &AnalysisFile| a.v == 1)
            .unwrap_or(AnalysisFile { v: 1, ..Default::default() });
        let index: Option<IndexFile> = read_json(&config.data_file("library.json")).filter(|i: &IndexFile| i.version == INDEX_VERSION);
        let lib = index.as_ref().map(|i| Library::build(i, Some(&analysis))).unwrap_or_default();
        let speed = settings.analysis_speed;
        Arc::new(Store {
            config,
            state: RwLock::new(State {
                lib: Arc::new(lib),
                index: index.map(Arc::new),
                status: ScanStatus::default(),
                analysis,
                analysis_status: AnalysisStatus {
                    speed,
                    state: if speed == AnalysisSpeed::Off { AnalysisState::Off } else { AnalysisState::Idle },
                    done: 0,
                    total: 0,
                },
                settings,
            }),
            scanning: AtomicBool::new(false),
            rescan: AtomicBool::new(false),
            run: Mutex::new(None),
            generation: AtomicU64::new(1),
        })
    }

    pub fn library(&self) -> Arc<Library> {
        self.state.read().lib.clone()
    }

    /// 라이브러리가 바뀔 때마다 오른다. 화면은 이 값이 바뀌면 다시 불러온다.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    pub fn scan_status(&self) -> ScanStatus {
        self.state.read().status.clone()
    }

    pub fn analysis_status(&self) -> AnalysisStatus {
        self.state.read().analysis_status.clone()
    }

    pub fn settings(&self) -> Settings {
        self.state.read().settings.clone()
    }

    pub fn music_dir(&self) -> PathBuf {
        self.state.read().settings.music_dir.as_ref().map(PathBuf::from).unwrap_or_else(|| self.config.music_dir.clone())
    }

    pub fn covers_dir(&self) -> PathBuf {
        self.config.data_dir.join("covers")
    }

    /// 곡 ID → 실제 파일. 음악 폴더 밖으로는 나가지 않는다(색인에 있는 상대 경로만 쓴다).
    pub fn track_file(&self, id: &str) -> Option<(PathBuf, StoredTrack)> {
        let lib = self.library();
        let t = lib.tracks.get(id)?.clone();
        Some((self.music_dir().join(&t.path), t))
    }

    pub fn waveform(&self, id: &str) -> Option<String> {
        let s = self.state.read();
        let t = s.lib.tracks.get(id)?;
        s.analysis.tracks.get(id).filter(|a| a.m == t.mtime).map(|a| a.w.clone())
    }

    fn save_settings(&self, settings: &Settings) {
        if let Ok(bytes) = serde_json::to_vec_pretty(settings) {
            let _ = write_atomic(&self.config.data_file("settings.json"), &bytes);
        }
    }

    pub fn set_music_dir(self: &Arc<Self>, dir: Option<String>) -> anyhow::Result<()> {
        if let Some(d) = &dir
            && !std::path::Path::new(d).is_dir()
        {
            anyhow::bail!("폴더가 없습니다: {d}");
        }
        let settings = {
            let mut s = self.state.write();
            s.settings.music_dir = dir;
            s.settings.clone()
        };
        self.save_settings(&settings);
        self.start_scan();
        Ok(())
    }

    pub fn start_scan(self: &Arc<Self>) -> ScanStatus {
        if self.scanning.swap(true, Ordering::SeqCst) {
            // 스캔 중에 다시 부르면 끝난 뒤 한 번 더 돈다(그 사이 바뀐 것을 놓치지 않게).
            self.rescan.store(true, Ordering::SeqCst);
            return self.scan_status();
        }
        let started_at = now_ms();
        self.state.write().status =
            ScanStatus { state: ScanState::Scanning, phase: Some(ScanPhase::Listing), started_at: Some(started_at), ..Default::default() };
        let me = self.clone();
        std::thread::Builder::new()
            .name("sori-scan".into())
            .spawn(move || {
                loop {
                    me.rescan.store(false, Ordering::SeqCst);
                    me.scan_once(started_at);
                    if !me.rescan.load(Ordering::SeqCst) {
                        break;
                    }
                }
                me.scanning.store(false, Ordering::SeqCst);
                me.start_analysis();
            })
            .expect("스캔 스레드를 만들 수 없습니다");
        self.scan_status()
    }

    fn scan_once(self: &Arc<Self>, started_at: i64) {
        let root = self.music_dir();
        let prev = self.state.read().index.clone();
        let report = |phase: ScanPhase, processed: usize, total: usize| {
            let mut s = self.state.write();
            s.status.phase = Some(phase);
            s.status.processed = processed;
            s.status.total = total;
        };
        let result = scan_library(&root, &self.covers_dir(), prev.as_deref(), &report);
        match result {
            Ok(r) => {
                self.state.write().status.phase = Some(ScanPhase::Saving);
                if let Ok(bytes) = serde_json::to_vec(&r.index) {
                    let _ = write_atomic(&self.config.data_file("library.json"), &bytes);
                }
                let n = r.index.tracks.len();
                let index = Arc::new(r.index);
                let lib = {
                    let s = self.state.read();
                    Library::build(&index, Some(&s.analysis))
                };
                let mut s = self.state.write();
                s.index = Some(index);
                s.lib = Arc::new(lib);
                s.status = ScanStatus {
                    state: ScanState::Idle,
                    phase: None,
                    processed: n,
                    total: n,
                    started_at: Some(started_at),
                    finished_at: Some(now_ms()),
                    error: None,
                    added: Some(r.added),
                    removed: Some(r.removed),
                };
                drop(s);
                self.generation.fetch_add(1, Ordering::Relaxed);
                tracing::info!(tracks = n, added = r.added, removed = r.removed, failed = r.failed, "스캔 끝");
            }
            Err(e) => {
                let mut s = self.state.write();
                s.status = ScanStatus {
                    state: ScanState::Error,
                    started_at: Some(started_at),
                    finished_at: Some(now_ms()),
                    error: Some(e.to_string()),
                    ..Default::default()
                };
                tracing::warn!("스캔 실패: {e}");
            }
        }
    }

    /// 스캔이 끝날 때까지 기다린다(테스트와 명령줄 도구용).
    pub fn wait_for_scan(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while self.scanning.load(Ordering::SeqCst) {
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        true
    }

    pub fn wait_for_analysis(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self.analysis_status().state != AnalysisState::Running && !self.scanning.load(Ordering::SeqCst) {
                return true;
            }
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn set_analysis_speed(self: &Arc<Self>, speed: AnalysisSpeed) {
        let settings = {
            let mut s = self.state.write();
            s.settings.analysis_speed = speed;
            s.analysis_status.speed = speed;
            s.settings.clone()
        };
        self.save_settings(&settings);
        self.start_analysis();
    }

    fn save_analysis(&self) {
        let bytes = {
            let s = self.state.read();
            serde_json::to_vec(&s.analysis)
        };
        if let Ok(bytes) = bytes {
            let _ = write_atomic(&self.config.data_file("analysis.json"), &bytes);
        }
    }

    fn rebuild(&self) {
        let lib = {
            let s = self.state.read();
            match &s.index {
                Some(i) => Library::build(i, Some(&s.analysis)),
                None => return,
            }
        };
        self.state.write().lib = Arc::new(lib);
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// 아직 분석하지 않았거나 파일이 바뀐 곡을 분석한다. 이미 돌고 있으면 멈추고 다시 센다.
    pub fn start_analysis(self: &Arc<Self>) {
        if let Some(prev) = self.run.lock().take() {
            prev.store(true, Ordering::SeqCst);
        }
        let (speed, pending) = {
            let s = self.state.read();
            let pending: VecDeque<(String, PathBuf, i64)> = s
                .index
                .as_ref()
                .map(|i| {
                    i.tracks
                        .iter()
                        .filter(|t| s.analysis.tracks.get(&t.id).map(|a| a.m) != Some(t.mtime))
                        .map(|t| (t.id.clone(), self.music_dir().join(&t.path), t.mtime))
                        .collect()
                })
                .unwrap_or_default();
            (s.settings.analysis_speed, pending)
        };
        let total = pending.len();
        {
            let mut s = self.state.write();
            s.analysis_status = AnalysisStatus {
                speed,
                state: if speed == AnalysisSpeed::Off {
                    AnalysisState::Off
                } else if total == 0 {
                    AnalysisState::Idle
                } else {
                    AnalysisState::Running
                },
                done: 0,
                total,
            };
        }
        if speed == AnalysisSpeed::Off || total == 0 {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *self.run.lock() = Some(cancel.clone());
        let workers = match speed {
            AnalysisSpeed::Fast => (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2) / 2).clamp(1, 4),
            _ => 1,
        };
        let queue = Arc::new(Mutex::new(pending));
        let last_flush = Arc::new(Mutex::new(Instant::now()));
        let mut handles = Vec::new();
        for w in 0..workers {
            let (me, queue, cancel, last_flush) = (self.clone(), queue.clone(), cancel.clone(), last_flush.clone());
            handles.push(
                std::thread::Builder::new()
                    .name(format!("sori-analysis-{w}"))
                    .spawn(move || {
                        loop {
                            if cancel.load(Ordering::Relaxed) {
                                return;
                            }
                            let Some((id, path, mtime)) = queue.lock().pop_front() else { return };
                            let result = analyze_file(&path, &cancel);
                            if cancel.load(Ordering::Relaxed) {
                                return;
                            }
                            {
                                let mut s = me.state.write();
                                if let Ok(m) = &result {
                                    s.analysis.tracks.insert(id, to_entry(mtime, m));
                                }
                                s.analysis_status.done += 1;
                            }
                            // 몇 초마다 화면에 반영하고 디스크에 남긴다.
                            let flush = {
                                let mut lf = last_flush.lock();
                                if lf.elapsed() > Duration::from_secs(4) {
                                    *lf = Instant::now();
                                    true
                                } else {
                                    false
                                }
                            };
                            if flush {
                                me.rebuild();
                                me.save_analysis();
                            }
                            if speed == AnalysisSpeed::Throttled {
                                std::thread::sleep(Duration::from_millis(250));
                            }
                        }
                    })
                    .expect("분석 스레드를 만들 수 없습니다"),
            );
        }
        let me = self.clone();
        std::thread::spawn(move || {
            for h in handles {
                let _ = h.join();
            }
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            me.rebuild();
            me.save_analysis();
            let mut s = me.state.write();
            s.analysis_status.state = AnalysisState::Idle;
        });
    }

    pub fn stop(&self) {
        if let Some(c) = self.run.lock().take() {
            c.store(true, Ordering::SeqCst);
        }
    }
}
