//! 네이티브 재생 엔진. 브라우저를 거치지 않고 이 기기의 오디오 장치로 직접 내보낸다.
//!
//! 렌더 스레드 하나가 곡을 풀고(64비트), 장치의 샘플레이트로 바꾸고, 링 버퍼에 채운다.
//! 다음 곡을 미리 열어 두었다가 지금 곡이 끝나는 샘플 바로 다음에 이어 붙이므로
//! 곡 사이에 틈이 없다(갭리스). 장치가 실제로 어디를 틀고 있는지는 "타임라인"으로 따진다:
//! 링에 넣은 위치마다 어느 곡의 몇 초인지 적어 두고, 장치가 꺼내 간 위치와 맞춰 본다.
//! 그래서 화면의 "지금 재생 중"은 디코딩이 아니라 귀에 들리는 순간에 바뀐다.

#[cfg(feature = "device")]
pub mod device;
pub mod output;

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::decode::{PcmSource, Spec};
use crate::dsp::chain::{Chain, ClipMeter, Clips};
use crate::dsp::ir::IrStore;
use crate::dsp::leveling::leveling_gain;
use crate::dsp::request::{DspRequest, parse_request};
use crate::dsp::resample::Resampler;
use crate::dsp::stream::{quality_of, to_stereo};
use crate::model::Loudness;
use output::{OutSpec, Output, Shared};

/// 무엇을 틀지. 화면이 보낸다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PlaySource {
    /// 라이브러리의 곡
    Library { id: String },
    /// 라이브 아카이브의 곡(공연 ID와 그 안의 파일). 주소는 서버가 만든다.
    Live { id: String, file: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayItem {
    /// 대기열 칸의 ID. 같은 곡을 두 번 넣어도 칸은 다르다.
    pub uid: String,
    pub source: PlaySource,
    /// 이 곡에 걸 DSP 요청(화면의 `encodeRequest`). 없으면 원본 그대로.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dsp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

/// 곡의 이름표. 잠금 화면·알림이 보여 준다(엔진은 읽지 않는다).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    /// 표지 주소(이 서버 기준, 예: `/api/cover/…`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
}

impl PlayItem {
    /// 네트워크에서 받는 곡이면 여는 일을 재생 스레드 밖에서 한다.
    pub fn is_remote(&self) -> bool {
        matches!(self.source, PlaySource::Live { .. })
    }
}

/// 곡을 연 결과. 라우드니스는 볼륨 레벨링이 쓴다.
pub struct Opened {
    pub source: Box<dyn PcmSource>,
    pub loudness: Option<Loudness>,
}

pub type Resolver = Arc<dyn Fn(&PlayItem) -> anyhow::Result<Opened> + Send + Sync>;

fn open_library(store: &crate::Store, id: &str) -> anyhow::Result<Opened> {
    let (path, _) = store.track_file(id).ok_or_else(|| anyhow::anyhow!("라이브러리에 없는 곡입니다"))?;
    let loudness = store.library().views.get(id).and_then(|t| t.loudness.clone());
    Ok(Opened { source: crate::decode::open(&path)?, loudness })
}

/// 라이브러리의 곡만 여는 방법.
pub fn library_resolver(store: Arc<crate::Store>) -> Resolver {
    Arc::new(move |item: &PlayItem| match &item.source {
        PlaySource::Library { id } => open_library(&store, id),
        PlaySource::Live { .. } => anyhow::bail!("라이브 아카이브를 열 수 없습니다"),
    })
}

/// 라이브러리의 곡과 라이브 아카이브의 곡(받으면서 푼다)을 여는 기본 방법.
pub fn resolver(store: Arc<crate::Store>, archive: Arc<crate::live::Archive>) -> Resolver {
    Arc::new(move |item: &PlayItem| match &item.source {
        PlaySource::Library { id } => open_library(&store, id),
        PlaySource::Live { id, file } => Ok(Opened { source: archive.open(id, file)?, loudness: None }),
    })
}
pub type OutputFactory = Box<dyn FnOnce() -> Box<dyn Output> + Send>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Idle,
    Loading,
    Playing,
    Paused,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Event {
    /// 다음 곡으로 넘어갔다(장치가 그 곡의 첫 샘플을 틀기 시작했다).
    Advanced { uid: String },
    /// 대기열 끝까지 다 틀었다.
    Ended { uid: String },
    Error {
        uid: Option<String>,
        message: String,
        /// 오디오 장치가 문제다(곡이 아니라). 화면은 다른 재생 방법으로 바꿀 수 있다.
        output: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub rate: u32,
    pub channels: usize,
    pub bits: Option<u32>,
    pub dsd_rate: Option<u32>,
    pub gain_db: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub status: Status,
    pub uid: Option<String>,
    /// 지금 곡에서의 위치(초)
    pub position: f64,
    pub duration: f64,
    /// 선형 볼륨(0–1)과 음소거
    pub volume: f64,
    pub muted: bool,
    pub error: Option<String>,
    pub output: Option<OutSpec>,
    pub source: Option<SourceInfo>,
    /// 지금 들리는 곡의 이름표
    pub meta: Option<Meta>,
    pub resampling: Option<(u32, u32)>,
    pub underruns: u64,
    /// 바뀔 때마다 오른다
    pub seq: u64,
    /// 지금까지의 사건(번호, 사건). 화면은 마지막으로 본 번호 다음 것만 처리한다.
    pub events: Vec<(u64, Event)>,
}

enum Cmd {
    Load {
        item: PlayItem,
        autoplay: bool,
        at: f64,
        force: bool,
    },
    /// 곡마다의 DSP 요청이 바뀌었다(설정을 고쳤다).
    Dsp(Vec<(String, Option<String>)>),
    Preload(Option<PlayItem>),
    /// 다음에 틀 곡들(순서대로)과 앞서 튼 곡들(가까운 것부터). 화면이 잠들어도 엔진이 혼자
    /// 이어 튼다. `repeat_one`이면 지금 곡을 이음매 없이 되풀이한다.
    Queue {
        upcoming: Vec<PlayItem>,
        previous: Vec<PlayItem>,
        repeat_one: bool,
    },
    Next,
    Prev,
    Play,
    Pause,
    Stop,
    Seek(f64),
    Volume {
        volume: f64,
        muted: bool,
    },
    /// 뒤에서 하던 일(네트워크 곡 열기·시크)이 끝났다.
    Job(Box<JobDone>),
    Shutdown,
}

/// 렌더 스레드를 막지 않으려고 따로 하는 일.
#[derive(Clone, Copy, Debug)]
enum JobKind {
    /// 열고(필요하면 그 위치로 가서) 지금 곡으로 건다.
    Load { at: f64 },
    /// 다음 곡으로 미리 열어 둔다.
    Preload,
    /// 지금 곡을 그 위치로(곡을 잠시 빌려 간다).
    Seek { t: f64 },
}

struct JobDone {
    token: u64,
    kind: JobKind,
    deck: anyhow::Result<Deck>,
    /// 시크했다면 그 결과(간 프레임)
    seeked: Option<anyhow::Result<u64>>,
}

/// 링에 넣은 구간 하나: 이 절대 프레임부터는 이 곡의 이 위치다.
#[derive(Clone, Debug)]
struct Segment {
    uid: String,
    start: u64,
    /// 구간 시작이 곡의 몇 초인가
    offset: f64,
    duration: f64,
    source: Option<SourceInfo>,
    meta: Option<Meta>,
}

struct Deck {
    item: PlayItem,
    source: Box<dyn PcmSource>,
    spec: Spec,
    req: Option<DspRequest>,
    loudness: Option<Loudness>,
    gain: f64,
    gain_db: f64,
    /// 이 곡에서 읽은 프레임(원본 샘플레이트)
    frames_read: u64,
    duration: f64,
}

impl Deck {
    /// 요청과 라우드니스로 볼륨 레벨링 게인을 다시 정한다.
    fn apply_request(&mut self, dsp: Option<String>) {
        self.item.dsp = dsp;
        self.req = self.item.dsp.as_deref().and_then(parse_request);
        let db = self.req.as_ref().and_then(|r| r.lv).map(|lv| leveling_gain(self.loudness.as_ref(), lv.m, lv.t, lv.u).gain).unwrap_or(0.0);
        self.gain_db = db;
        self.gain = 10f64.powf(db / 20.0);
    }

    /// 체인(헤드룸·필터)을 정하는 부분만. 레벨링은 곡마다 게인으로 따로 건다.
    fn chain_key(&self) -> Option<String> {
        let r = self.req.as_ref()?;
        (r.hr.is_some() || !r.f.is_empty() || r.ci.is_some())
            .then(|| serde_json::to_string(&(r.hr, &r.f, r.ci, r.src.map(|s| (s.f, s.p)))).unwrap_or_default())
    }

    fn info(&self) -> SourceInfo {
        SourceInfo {
            rate: self.spec.rate,
            channels: self.spec.channels,
            bits: self.spec.bits,
            dsd_rate: self.spec.dsd_rate,
            gain_db: self.gain_db,
        }
    }
}

fn open_deck(resolver: &Resolver, item: &PlayItem) -> anyhow::Result<Deck> {
    let opened = resolver(item)?;
    let spec = opened.source.spec().clone();
    let duration = spec.duration().unwrap_or(0.0);
    let mut d = Deck {
        item: item.clone(),
        source: opened.source,
        req: None,
        loudness: opened.loudness,
        gain: 1.0,
        gain_db: 0.0,
        spec,
        frames_read: 0,
        duration,
    };
    d.apply_request(item.dsp.clone());
    Ok(d)
}

/// 곡을 `t`초로. 실제로 간 프레임을 돌려준다.
fn seek_deck(d: &mut Deck, t: f64) -> anyhow::Result<u64> {
    let frame = (t.max(0.0) * d.spec.rate as f64).round() as u64;
    let frame = d.spec.frames.map(|f| frame.min(f)).unwrap_or(frame);
    let at = d.source.seek(frame)?;
    d.frames_read = at;
    Ok(at)
}

pub struct Player {
    tx: Sender<Cmd>,
    pub clips: Arc<Clips>,
    state: Arc<Mutex<PlayerState>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Player {
    pub fn new(factory: OutputFactory, resolver: Resolver) -> Player {
        Player::with_dsp(factory, resolver, None, Arc::new(Clips::default()))
    }

    pub fn with_dsp(factory: OutputFactory, resolver: Resolver, irs: impl Into<Option<Arc<IrStore>>>, clips: Arc<Clips>) -> Player {
        let irs = irs.into();
        let (tx, rx) = channel();
        let state = Arc::new(Mutex::new(PlayerState {
            status: Status::Idle,
            uid: None,
            position: 0.0,
            duration: 0.0,
            volume: 1.0,
            muted: false,
            error: None,
            output: None,
            source: None,
            meta: None,
            resampling: None,
            underruns: 0,
            seq: 0,
            events: Vec::new(),
        }));
        let st = state.clone();
        let cl = clips.clone();
        let jobs = tx.clone();
        let thread = std::thread::Builder::new()
            .name("sori-render".into())
            .spawn(move || {
                let mut r = Render::new(factory(), resolver, rx, jobs, st);
                r.irs = irs;
                r.clips = cl;
                r.run()
            })
            .expect("렌더 스레드를 만들 수 없습니다");
        Player { tx, clips, state, thread: Some(thread) }
    }

    /// 곡(대기열 칸)마다 DSP 요청을 바꾼다. 지금 곡이면 지금 위치에서 바로 다시 건다.
    pub fn set_dsp(&self, items: Vec<(String, Option<String>)>) {
        let _ = self.tx.send(Cmd::Dsp(items));
    }

    pub fn state(&self) -> PlayerState {
        self.state.lock().clone()
    }

    pub fn load(&self, item: PlayItem, autoplay: bool, at: f64, force: bool) {
        let _ = self.tx.send(Cmd::Load { item, autoplay, at, force });
    }
    pub fn preload(&self, item: Option<PlayItem>) {
        let _ = self.tx.send(Cmd::Preload(item));
    }
    /// 다음에 틀 곡들과 앞서 튼 곡들. 엔진이 혼자 이어 틀고, 다음·이전 단추(알림, 헤드셋)도 이것으로 간다.
    pub fn set_queue(&self, upcoming: Vec<PlayItem>, previous: Vec<PlayItem>, repeat_one: bool) {
        let _ = self.tx.send(Cmd::Queue { upcoming, previous, repeat_one });
    }
    pub fn next(&self) {
        let _ = self.tx.send(Cmd::Next);
    }
    pub fn prev(&self) {
        let _ = self.tx.send(Cmd::Prev);
    }
    pub fn play(&self) {
        let _ = self.tx.send(Cmd::Play);
    }
    pub fn pause(&self) {
        let _ = self.tx.send(Cmd::Pause);
    }
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
    pub fn seek(&self, t: f64) {
        let _ = self.tx.send(Cmd::Seek(t));
    }
    pub fn set_volume(&self, volume: f64, muted: bool) {
        let _ = self.tx.send(Cmd::Volume { volume: volume.clamp(0.0, 1.0), muted });
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// 링 버퍼 길이(초). 길수록 끊김에 강하고, 시크·곡 바꾸기는 링을 비우므로 늦어지지 않는다.
const RING_SECONDS: f64 = 1.0;
/// 엔진이 들고 있는 대기열의 길이(앞뒤 각각)
const QUEUE_MAX: usize = 64;
/// 한 번에 링에 넣는 양(초)
const CHUNK_SECONDS: f64 = 0.02;
/// 넣은 소리를 장치가 이만큼 가져가지 않으면 장치를 다시 연다.
const STALL: Duration = Duration::from_secs(3);
/// 그래도 안 되면 몇 번까지 다시 열어 볼지
const MAX_RECOVERIES: u32 = 1;
/// 내 기기의 곡을 틀겠다고 하고 이만큼 지나도 소리가 나가지 않으면 알린다(곡을 풀지 못하는 등).
const START_TIMEOUT: Duration = Duration::from_secs(10);

struct Render {
    output: Box<dyn Output>,
    resolver: Resolver,
    rx: Receiver<Cmd>,
    /// 뒤에서 하는 일이 끝나면 여기로 알린다.
    jobs: Sender<Cmd>,
    job_seq: u64,
    /// 지금 곡을 열거나 시크하는 일(이것이 끝나야 지금 곡이 있다)
    job: Option<u64>,
    /// 그 일의 곡(그사이 DSP가 바뀌면 여기에 적어 둔다)
    job_item: Option<PlayItem>,
    /// 그 일이 끝나면 갈 위치(그사이 사용자가 또 시크했다)
    queued_seek: Option<f64>,
    /// 다음 곡을 미리 여는 일
    preload_job: Option<(u64, String)>,
    /// 다음에 틀 곡들, 앞서 튼 곡들(가까운 것부터)
    upcoming: VecDeque<PlayItem>,
    previous: VecDeque<PlayItem>,
    repeat_one: bool,
    state: Arc<Mutex<PlayerState>>,
    shared: Option<Arc<Shared>>,
    producer: Option<rtrb::Producer<f64>>,
    spec: Option<OutSpec>,
    /// 링에 넣은 절대 프레임 수
    written: u64,
    current: Option<Deck>,
    next: Option<Deck>,
    next_item: Option<PlayItem>,
    resampler: Option<Resampler>,
    /// 링에 아직 못 넣은 출력(장치 샘플레이트)
    pending: Vec<f64>,
    segments: VecDeque<Segment>,
    /// 사용자가 바라는 것
    want_play: bool,
    /// 지금 곡을 다 읽었고 다음 곡이 없다
    draining: bool,
    volume: f64,
    muted: bool,
    /// 화면에 마지막으로 알린 곡
    reported_uid: Option<String>,
    event_seq: u64,
    scratch: Vec<f64>,
    mapped: Vec<f64>,
    stereo: Vec<f64>,
    irs: Option<Arc<IrStore>>,
    clips: Arc<Clips>,
    /// 장치 샘플레이트에서 도는 체인과 그 열쇠(요청의 헤드룸·필터 부분)
    chain: Option<(String, Chain)>,
    clip: Option<ClipMeter>,
    /// 장치가 마지막으로 가져간 위치와 그때(지켜보기)
    watch_read: u64,
    watch_since: Instant,
    /// 연달아 장치를 다시 연 횟수(소리를 가져가면 0으로)
    recoveries: u32,
    /// 여는 중(Loading)이 시작된 때
    loading_since: Option<Instant>,
}

impl Render {
    fn new(output: Box<dyn Output>, resolver: Resolver, rx: Receiver<Cmd>, jobs: Sender<Cmd>, state: Arc<Mutex<PlayerState>>) -> Self {
        Render {
            output,
            resolver,
            rx,
            jobs,
            job_seq: 0,
            job: None,
            job_item: None,
            queued_seek: None,
            preload_job: None,
            upcoming: VecDeque::new(),
            previous: VecDeque::new(),
            repeat_one: false,
            state,
            shared: None,
            producer: None,
            spec: None,
            written: 0,
            current: None,
            next: None,
            next_item: None,
            resampler: None,
            pending: Vec::new(),
            segments: VecDeque::new(),
            want_play: false,
            draining: false,
            volume: 1.0,
            muted: false,
            reported_uid: None,
            event_seq: 0,
            scratch: Vec::new(),
            mapped: Vec::new(),
            stereo: Vec::new(),
            irs: None,
            clips: Arc::new(Clips::default()),
            chain: None,
            clip: None,
            watch_read: 0,
            watch_since: Instant::now(),
            recoveries: 0,
            loading_since: None,
        }
    }

    fn emit(&mut self, e: Event) {
        self.event_seq += 1;
        let mut s = self.state.lock();
        s.events.push((self.event_seq, e));
        if s.events.len() > 32 {
            s.events.remove(0);
        }
        s.seq += 1;
    }

    fn set_status(&mut self, status: Status, error: Option<String>) {
        let mut s = self.state.lock();
        if s.status != status || s.error != error {
            s.status = status;
            s.error = error;
            s.seq += 1;
        }
    }

    fn run(mut self) {
        loop {
            // 명령: 링이 넉넉하면 짧게, 할 일이 없으면 길게 기다린다.
            let busy = self.want_play && self.current.is_some() && !self.draining;
            let wait = if busy { Duration::from_millis(4) } else { Duration::from_millis(30) };
            match self.rx.recv_timeout(wait) {
                Ok(Cmd::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(cmd) => {
                    self.handle(cmd);
                    while let Ok(cmd) = self.rx.try_recv() {
                        if matches!(cmd, Cmd::Shutdown) {
                            self.output.close();
                            return;
                        }
                        self.handle(cmd);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            if let Err(e) = self.fill() {
                let uid = self.current.as_ref().map(|d| d.item.uid.clone());
                tracing::warn!("재생 오류: {e:#}");
                self.current = None;
                self.want_play = false;
                if let Some(s) = &self.shared {
                    s.paused.store(true, Ordering::Relaxed);
                }
                self.set_status(Status::Error, Some(e.to_string()));
                self.emit(Event::Error { uid, message: e.to_string(), output: false });
            }
            self.update_state();
            self.watchdog();
        }
        self.output.close();
    }

    /// 뒤에서 곡을 열거나(deck이 없으면) 시크한다. 끝나면 `Cmd::Job`이 온다.
    fn spawn_job(&mut self, kind: JobKind, item: PlayItem, deck: Option<Deck>) -> u64 {
        self.job_seq += 1;
        let token = self.job_seq;
        let resolver = self.resolver.clone();
        let tx = self.jobs.clone();
        let work = move || {
            let mut seeked = None;
            let deck = match deck {
                Some(d) => Ok(d),
                None => open_deck(&resolver, &item),
            }
            .map(|mut d| {
                match kind {
                    JobKind::Load { at } if at > 0.0 => seeked = Some(seek_deck(&mut d, at)),
                    JobKind::Seek { t } => seeked = Some(seek_deck(&mut d, t)),
                    _ => {}
                }
                d
            });
            let _ = tx.send(Cmd::Job(Box::new(JobDone { token, kind, deck, seeked })));
        };
        if let Err(e) = std::thread::Builder::new().name("sori-open".into()).spawn(work) {
            tracing::warn!("여는 스레드를 만들 수 없습니다: {e}");
        }
        token
    }

    fn cancel_job(&mut self) {
        self.job = None;
        self.job_item = None;
        self.queued_seek = None;
    }

    fn load_failed(&mut self, uid: Option<String>, e: &anyhow::Error) {
        self.current = None;
        self.want_play = false;
        if let Some(s) = &self.shared {
            s.paused.store(true, Ordering::Relaxed);
        }
        self.set_status(Status::Error, Some(e.to_string()));
        self.emit(Event::Error { uid, message: e.to_string(), output: false });
    }

    /// 오디오 장치를 쓸 수 없다. 멈추고 화면에 알린다.
    fn output_failed(&mut self, uid: Option<String>, message: String) {
        tracing::warn!("{message}");
        self.current = None;
        self.want_play = false;
        self.recoveries = 0;
        self.set_status(Status::Error, Some(message.clone()));
        self.emit(Event::Error { uid, message, output: true });
    }

    /// 장치가 소리를 가져가는지 지켜본다. 넣은 소리가 있는데 몇 초 동안 한 프레임도 가져가지 않거나
    /// (안드로이드에서 출력이 시작하지 않는 때) 장치가 오류를 알리면(이어폰·블루투스로 바뀌며 끊기는 때)
    /// 장치를 다시 열고 들리던 자리부터 잇는다. 다시 열어도 안 되면 오류를 낸다.
    fn watchdog(&mut self) {
        self.start_watchdog();
        let Some(shared) = self.shared.clone() else { return };
        let failed = shared.failed.load(Ordering::Relaxed);
        let read = shared.read.load(Ordering::Acquire);
        let waiting = self.want_play && self.job.is_none() && !shared.paused.load(Ordering::Relaxed) && self.written > read;
        if read != self.watch_read {
            self.watch_read = read;
            self.watch_since = Instant::now();
            self.recoveries = 0;
        } else if !waiting {
            self.watch_since = Instant::now();
        }
        if !failed && self.watch_since.elapsed() < STALL {
            return;
        }
        self.watch_since = Instant::now();
        self.recover_output(failed);
    }

    /// 내 기기의 곡인데 오래 "여는 중"이면(소리를 하나도 만들지 못한다) 알린다. 네트워크 곡은
    /// 받는 쪽이 따로 기다리고 포기한다.
    fn start_watchdog(&mut self) {
        let loading = self.state.lock().status == Status::Loading;
        let local = self.job.is_none() && self.current.as_ref().is_some_and(|d| !d.source.remote());
        if !(loading && self.want_play && local) {
            self.loading_since = None;
            return;
        }
        let since = *self.loading_since.get_or_insert_with(Instant::now);
        if since.elapsed() < START_TIMEOUT {
            return;
        }
        self.loading_since = None;
        let uid = self.current.as_ref().map(|d| d.item.uid.clone());
        if let Some(s) = &self.shared {
            s.paused.store(true, Ordering::Relaxed);
        }
        self.output_failed(uid, format!("재생을 시작하지 못했습니다: {}초 동안 소리가 나가지 않았습니다", START_TIMEOUT.as_secs()));
    }

    fn recover_output(&mut self, failed: bool) {
        let heard = self.segment_at_read().map(|s| s.uid.clone());
        let pos = self.playing_position().unwrap_or(0.0);
        let device = self.spec.as_ref().map(|s| s.device.clone()).unwrap_or_default();
        let uid = self.current.as_ref().map(|d| d.item.uid.clone());
        self.recoveries += 1;
        tracing::warn!(device, failed, attempt = self.recoveries, "오디오 장치가 소리를 가져가지 않습니다. 다시 엽니다");
        self.output.close();
        self.producer = None;
        self.shared = None;
        if self.recoveries > MAX_RECOVERIES {
            let why = if failed { "장치가 오류를 냈습니다" } else { "소리를 가져가지 않습니다" };
            self.output_failed(uid, format!("이 기기의 오디오 출력이 응답하지 않습니다({device}: {why})"));
            return;
        }
        if let Err(e) = self.ensure_output() {
            self.output_failed(uid, format!("출력 장치를 열 수 없습니다: {e}"));
            return;
        }
        // 들리던 자리부터. 장치가 아직 앞 곡의 끝을 틀던 중이었으면 지금 곡의 처음부터.
        if self.current.is_none() {
            return;
        }
        let t = if heard.is_some() && heard == uid { pos } else { 0.0 };
        self.draining = false;
        self.resampler = None;
        self.chain = None;
        if self.current.as_ref().is_some_and(|d| d.source.remote()) {
            self.begin_remote_seek(t);
        } else {
            self.seek_current(t);
            self.sync_chain(t);
            if let Some(s) = &self.shared {
                s.paused.store(!self.want_play, Ordering::Relaxed);
            }
        }
        if self.want_play {
            self.set_status(Status::Loading, None);
        }
    }

    /// 다음 곡을 미리 연다. 네트워크 곡은 뒤에서.
    fn preload(&mut self, it: PlayItem) {
        if self.next.as_ref().is_some_and(|n| n.item.uid == it.uid)
            || self.current.as_ref().is_some_and(|c| c.item.uid == it.uid)
            || self.preload_job.as_ref().is_some_and(|(_, uid)| *uid == it.uid)
            || self.job_item.as_ref().is_some_and(|j| j.uid == it.uid)
        {
            return;
        }
        self.next = None;
        self.preload_job = None;
        if it.is_remote() {
            let uid = it.uid.clone();
            let token = self.spawn_job(JobKind::Preload, it, None);
            self.preload_job = Some((token, uid));
        } else {
            self.next = open_deck(&self.resolver, &it).ok();
            // 지금 곡을 이미 다 읽고 기다리던 중이면 바로 이어 붙인다.
            if self.draining {
                self.draining = false;
            }
        }
    }

    /// 네트워크 곡: 지금 것을 멈추고 뒤에서 연다. 다 열리면 `finish_load`.
    fn begin_remote_load(&mut self, item: PlayItem, at: f64) {
        if let Err(e) = self.ensure_output() {
            self.output_failed(Some(item.uid), format!("출력 장치를 열 수 없습니다: {e}"));
            return;
        }
        self.flush_ring();
        self.resampler = None;
        self.current = None;
        self.draining = false;
        self.chain = None;
        if let Some(s) = &self.shared {
            s.paused.store(true, Ordering::Relaxed);
        }
        // 다음 곡으로 미리 열던 것이 바로 이 곡이면 그것을 기다린다.
        let token = match self.preload_job.take() {
            Some((t, uid)) if uid == item.uid && at == 0.0 => t,
            other => {
                self.preload_job = other;
                self.spawn_job(JobKind::Load { at }, item.clone(), None)
            }
        };
        self.job = Some(token);
        self.job_item = Some(item.clone());
        self.reported_uid = Some(item.uid.clone());
        let mut s = self.state.lock();
        s.uid = Some(item.uid);
        s.position = at;
        s.duration = 0.0;
        s.source = None;
        s.seq += 1;
    }

    /// 네트워크 곡의 시크: 곡을 뒤로 넘겨 시크하고 돌려받는다.
    fn begin_remote_seek(&mut self, t: f64) {
        let Some(d) = self.current.take() else { return };
        if let Some(s) = &self.shared {
            s.paused.store(true, Ordering::Relaxed);
        }
        let item = d.item.clone();
        self.job_item = Some(item.clone());
        self.job = Some(self.spawn_job(JobKind::Seek { t }, item, Some(d)));
        let mut s = self.state.lock();
        s.position = t;
        s.seq += 1;
    }

    /// 곡이 열렸다: 지금 곡으로 건다.
    fn finish_load(&mut self, deck: Deck, item: PlayItem, at: f64, seeked: Option<anyhow::Result<u64>>) {
        if let Err(e) = self.ensure_output() {
            self.output_failed(Some(item.uid), format!("출력 장치를 열 수 없습니다: {e}"));
            return;
        }
        self.flush_ring();
        self.resampler = None;
        self.current = Some(deck);
        self.draining = false;
        self.chain = None;
        match seeked {
            Some(r) => {
                let d = self.current.as_ref().unwrap();
                if let Err(e) = &r {
                    tracing::warn!("시크 실패: {e}");
                }
                self.start_segment(d.frames_read as f64 / d.spec.rate as f64);
            }
            None if at > 0.0 => self.seek_current(at),
            None => self.start_segment(0.0),
        }
        self.sync_chain(at);
        self.reported_uid = Some(item.uid.clone());
        {
            let mut s = self.state.lock();
            s.uid = Some(item.uid.clone());
            s.position = at;
            s.duration = self.current.as_ref().map(|d| d.duration).unwrap_or(0.0);
            s.source = self.current.as_ref().map(|d| d.info());
            s.seq += 1;
        }
        // 다음 곡이 이 곡이었다면 비었으니, 기억해 둔 다음 곡을 다시 연다.
        if self.next.is_none()
            && let Some(n) = self.next_item.clone()
            && n.uid != item.uid
        {
            self.preload(n);
        }
        let play = self.want_play;
        if let Some(s) = &self.shared {
            s.paused.store(!play, Ordering::Relaxed);
        }
        if !play {
            self.set_status(Status::Paused, None);
        }
    }

    /// 뒤에서 하던 일이 끝났다.
    fn finish_job(&mut self, done: JobDone) {
        let JobDone { token, kind, deck, seeked } = done;
        if let Some((t, uid)) = self.preload_job.clone()
            && t == token
            && self.job != Some(token)
        {
            self.preload_job = None;
            match deck {
                Ok(mut d) => {
                    let wanted = self.next_item.as_ref().filter(|n| n.uid == uid).map(|n| n.dsp.clone());
                    let playing = self.current.as_ref().is_some_and(|c| c.item.uid == uid);
                    if let Some(dsp) = wanted
                        && !playing
                    {
                        if d.item.dsp != dsp {
                            d.apply_request(dsp);
                        }
                        self.next = Some(d);
                        // 지금 곡을 이미 다 읽고 기다리던 중이면 바로 이어 붙인다.
                        if self.draining {
                            self.draining = false;
                            if let Some(s) = &self.shared {
                                s.drained.store(false, Ordering::Relaxed);
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!("다음 곡을 미리 열지 못했습니다: {e:#}"),
            }
            return;
        }
        if self.job != Some(token) {
            return; // 그사이 다른 곡을 걸었거나 멈췄다
        }
        if self.preload_job.as_ref().is_some_and(|(t, _)| *t == token) {
            self.preload_job = None;
        }
        self.job = None;
        let item = self.job_item.take();
        let mut d = match deck {
            Ok(d) => d,
            Err(e) => {
                let uid = item.map(|i| i.uid);
                self.queued_seek = None;
                self.load_failed(uid, &e);
                return;
            }
        };
        // 여는 사이에 DSP 설정이 바뀌었으면 따른다.
        let item = item.unwrap_or_else(|| d.item.clone());
        if d.item.dsp != item.dsp {
            d.apply_request(item.dsp.clone());
        }
        match kind {
            JobKind::Seek { t } => {
                let offset = match &seeked {
                    Some(Ok(at)) => *at as f64 / d.spec.rate as f64,
                    Some(Err(e)) => {
                        tracing::warn!("시크 실패: {e}");
                        d.frames_read as f64 / d.spec.rate as f64
                    }
                    None => t,
                };
                self.current = Some(d);
                self.start_segment(offset);
                self.sync_chain(offset);
                if let Some(s) = &self.shared {
                    s.paused.store(!self.want_play, Ordering::Relaxed);
                }
                let mut s = self.state.lock();
                s.source = self.current.as_ref().map(|d| d.info());
                s.seq += 1;
            }
            JobKind::Load { at } => self.finish_load(d, item, at, seeked),
            JobKind::Preload => self.finish_load(d, item, 0.0, None),
        }
        if let Some(t) = self.queued_seek.take() {
            self.handle(Cmd::Seek(t));
        }
    }

    /// 지금 곡의 요청에 맞는 체인을 준비한다. 열쇠가 같으면 이어 쓴다(곡 사이에도 필터 상태가 이어진다).
    fn sync_chain(&mut self, offset: f64) {
        let rate = self.out_rate();
        let Some(d) = &self.current else { return };
        let key = d.chain_key();
        let ci = d.req.as_ref().is_some_and(|r| r.clip_indicator());
        match (key, &self.chain) {
            (None, _) => self.chain = None,
            (Some(k), Some((old, c))) if *old == k && c.rate == rate => {}
            (Some(k), _) => {
                let chain = Chain::new(d.req.as_ref().unwrap(), rate, self.irs.as_deref());
                for w in &chain.warnings {
                    tracing::warn!("{w}");
                }
                self.chain = Some((k, chain));
            }
        }
        self.clip = ci.then(|| ClipMeter::new(rate, offset));
        if ci {
            self.clips.reset_after(&format!("native.{}", d.item.uid), offset);
        }
    }

    /// 장치를 연다(이미 열려 있으면 그대로). 장치가 고른 샘플레이트로 연다.
    fn ensure_output(&mut self) -> anyhow::Result<()> {
        if self.producer.is_some() && !self.shared.as_ref().is_some_and(|s| s.failed.load(Ordering::Relaxed)) {
            return Ok(());
        }
        self.output.close();
        let (_rates, channels) = self.output.probe()?;
        let shared = Shared::new(channels.max(1));
        // 장치 샘플레이트를 아직 모르므로 넉넉히(192kHz 기준) 잡는다.
        let cap = ((192_000.0 * RING_SECONDS) as usize) * channels.max(1);
        let (prod, cons) = rtrb::RingBuffer::new(cap);
        let spec = self.output.open(None, cons, shared.clone())?;
        shared.set_gain(if self.muted { 0.0 } else { self.volume });
        tracing::info!(device = %spec.device, rate = spec.rate, channels = spec.channels, "출력 장치를 열었습니다");
        self.written = 0;
        self.segments.clear();
        self.producer = Some(prod);
        self.shared = Some(shared);
        self.spec = Some(spec.clone());
        self.resampler = None;
        self.pending.clear();
        let mut s = self.state.lock();
        s.output = Some(spec);
        s.seq += 1;
        Ok(())
    }

    fn out_rate(&self) -> u32 {
        self.spec.as_ref().map(|s| s.rate).unwrap_or(48_000)
    }

    fn out_channels(&self) -> usize {
        self.spec.as_ref().map(|s| s.channels).unwrap_or(2)
    }

    /// 링을 비운다: 지금까지 넣은 것은 콜백이 버린다.
    fn flush_ring(&mut self) {
        if let Some(s) = &self.shared {
            s.flush_to.store(self.written, Ordering::Release);
            s.drained.store(false, Ordering::Relaxed);
        }
        self.pending.clear();
        self.segments.clear();
    }

    fn start_segment(&mut self, offset: f64) {
        let Some(d) = &self.current else { return };
        let seg = Segment {
            uid: d.item.uid.clone(),
            start: self.written + (self.pending.len() / self.out_channels()) as u64,
            offset,
            duration: d.duration,
            source: Some(d.info()),
            meta: d.item.meta.clone(),
        };
        self.segments.push_back(seg);
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Load { item, autoplay, at, force } => {
                // 이미 이 곡을 틀고 있으면(다음 곡으로 저절로 넘어간 뒤 화면이 따라온 경우) 그대로 둔다.
                let playing_uid = self.segment_at_read().map(|s| s.uid.clone());
                if !force && at == 0.0 && playing_uid.as_deref() == Some(item.uid.as_str()) && self.job.is_none() {
                    if autoplay {
                        self.handle(Cmd::Play);
                    }
                    return;
                }
                self.set_status(Status::Loading, None);
                self.cancel_job();
                self.want_play = autoplay;
                if self.next.as_ref().is_some_and(|n| n.item.uid == item.uid) && at == 0.0 {
                    let deck = self.next.take().unwrap();
                    self.finish_load(deck, item, at, None);
                } else if item.is_remote() {
                    self.begin_remote_load(item, at);
                } else {
                    match open_deck(&self.resolver, &item) {
                        Ok(d) => self.finish_load(d, item, at, None),
                        Err(e) => self.load_failed(Some(item.uid), &e),
                    }
                }
            }
            Cmd::Queue { upcoming, previous, repeat_one } => {
                self.upcoming = upcoming.into_iter().take(QUEUE_MAX).collect();
                self.previous = previous.into_iter().take(QUEUE_MAX).collect();
                self.repeat_one = repeat_one;
                let front = self.upcoming.front().cloned();
                self.handle(Cmd::Preload(front));
            }
            Cmd::Next => {
                let Some(n) = self.upcoming.pop_front() else { return };
                if let Some(c) = self.playing_item() {
                    self.previous.push_front(c);
                    self.previous.truncate(QUEUE_MAX);
                }
                self.jump(n);
            }
            Cmd::Prev => {
                // 3초 넘게 들었으면 이 곡의 처음으로, 아니면 앞 곡으로(다른 플레이어와 같은 규칙).
                let pos = self.playing_position().unwrap_or(0.0);
                if pos > 3.0 || self.previous.is_empty() {
                    self.handle(Cmd::Seek(0.0));
                    return;
                }
                let p = self.previous.pop_front().unwrap();
                if let Some(c) = self.playing_item() {
                    self.upcoming.push_front(c);
                    self.upcoming.truncate(QUEUE_MAX);
                }
                self.jump(p);
            }
            Cmd::Preload(item) => {
                self.next_item = item.clone();
                match item {
                    None => {
                        self.next = None;
                        self.preload_job = None;
                    }
                    Some(it) => self.preload(it),
                }
            }
            Cmd::Play => {
                if self.current.is_none() && self.segments.is_empty() && self.job.is_none() {
                    return;
                }
                self.want_play = true;
                // 아직 여는 중이면 장치는 열린 뒤에 튼다.
                if let Some(s) = &self.shared
                    && self.job.is_none()
                {
                    s.paused.store(false, Ordering::Relaxed);
                }
                self.set_status(Status::Loading, None);
            }
            Cmd::Pause => {
                self.want_play = false;
                if let Some(s) = &self.shared {
                    s.paused.store(true, Ordering::Relaxed);
                }
                if self.current.is_some() || !self.segments.is_empty() || self.job.is_some() {
                    self.set_status(Status::Paused, None);
                }
            }
            Cmd::Stop => {
                self.cancel_job();
                self.preload_job = None;
                self.want_play = false;
                self.flush_ring();
                self.current = None;
                self.next = None;
                self.next_item = None;
                self.resampler = None;
                if let Some(s) = &self.shared {
                    s.paused.store(true, Ordering::Relaxed);
                }
                self.reported_uid = None;
                let mut s = self.state.lock();
                s.status = Status::Idle;
                s.uid = None;
                s.position = 0.0;
                s.duration = 0.0;
                s.source = None;
                s.seq += 1;
            }
            Cmd::Seek(t) => {
                // 아직 여는(시크하는) 중이면 끝난 뒤 그 위치로 간다.
                if self.job.is_some() {
                    self.queued_seek = Some(t.max(0.0));
                    let mut s = self.state.lock();
                    s.position = t.max(0.0);
                    s.seq += 1;
                    return;
                }
                // 장치가 아직 앞 곡의 끝을 틀고 있는데 렌더는 다음 곡으로 넘어갔다면, 앞 곡을 다시 연다.
                let playing = self.segment_at_read().map(|s| s.uid.clone());
                let cur = self.current.as_ref().map(|d| d.item.uid.clone());
                if playing.is_some() && playing != cur {
                    let item = self.find_item(playing.as_deref()).unwrap_or(PlayItem {
                        uid: playing.clone().unwrap(),
                        source: PlaySource::Library { id: String::new() },
                        dsp: None,
                        meta: None,
                    });
                    if let Some(c) = self.current.take() {
                        self.next_item = Some(c.item.clone());
                        self.next = Some(c);
                    }
                    let want = self.want_play;
                    self.handle(Cmd::Load { item, autoplay: want, at: t.max(0.0), force: true });
                    return;
                }
                if self.current.is_some() {
                    self.flush_ring();
                    self.resampler = None;
                    self.draining = false;
                    if let Some((_, c)) = &mut self.chain {
                        c.reset();
                    }
                    if self.current.as_ref().is_some_and(|d| d.source.remote()) {
                        self.begin_remote_seek(t.max(0.0));
                    } else {
                        self.seek_current(t.max(0.0));
                        self.sync_chain(t.max(0.0));
                    }
                    if self.want_play {
                        self.set_status(Status::Loading, None);
                    }
                }
            }
            Cmd::Dsp(items) => {
                let mut redo = false;
                for (uid, dsp) in items {
                    if let Some(j) = self.job_item.as_mut().filter(|j| j.uid == uid) {
                        j.dsp = dsp.clone();
                    }
                    if let Some(n) = self.next.as_mut().filter(|n| n.item.uid == uid) {
                        n.apply_request(dsp.clone());
                    }
                    if let Some(n) = self.next_item.as_mut().filter(|n| n.uid == uid) {
                        n.dsp = dsp.clone();
                    }
                    if let Some(c) = self.current.as_mut().filter(|c| c.item.uid == uid)
                        && c.item.dsp != dsp
                    {
                        c.apply_request(dsp);
                        redo = true;
                    }
                }
                // 지금 곡: 장치가 틀고 있는 위치에서 새 설정으로 다시 건다(링을 비우니 바로 들린다).
                if redo && self.current.is_some() {
                    let pos = self.playing_position().unwrap_or(0.0);
                    self.flush_ring();
                    self.resampler = None;
                    self.draining = false;
                    self.chain = None;
                    if self.current.as_ref().is_some_and(|d| d.source.remote()) {
                        self.begin_remote_seek(pos);
                    } else {
                        self.seek_current(pos);
                        self.sync_chain(pos);
                    }
                    let mut s = self.state.lock();
                    s.source = self.current.as_ref().map(|d| d.info()).or(s.source.clone());
                    s.seq += 1;
                }
            }
            Cmd::Volume { volume, muted } => {
                self.volume = volume;
                self.muted = muted;
                // 화면과 같은 눈금: 선형 진폭(0.5 → -6dB). 화면은 dB로 보여 준다.
                if let Some(s) = &self.shared {
                    s.set_gain(if muted { 0.0 } else { volume });
                }
                let mut s = self.state.lock();
                s.volume = volume;
                s.muted = muted;
                s.seq += 1;
            }
            Cmd::Job(done) => self.finish_job(*done),
            Cmd::Shutdown => {}
        }
    }

    /// 장치가 지금 틀고 있는(또는 열고 있는) 곡.
    fn playing_item(&self) -> Option<PlayItem> {
        let uid = self.segment_at_read().map(|s| s.uid.clone()).or_else(|| self.job_item.as_ref().map(|j| j.uid.clone()))?;
        self.find_item(Some(&uid))
    }

    /// 다음·이전 단추: 그 곡을 바로 틀고, 화면에는 넘어갔다고 알린다.
    fn jump(&mut self, item: PlayItem) {
        let uid = item.uid.clone();
        self.next_item = self.upcoming.front().cloned();
        self.handle(Cmd::Load { item, autoplay: true, at: 0.0, force: true });
        let front = self.upcoming.front().cloned();
        self.handle(Cmd::Preload(front));
        self.emit(Event::Advanced { uid });
    }

    fn find_item(&self, uid: Option<&str>) -> Option<PlayItem> {
        [self.current.as_ref(), self.next.as_ref()]
            .into_iter()
            .flatten()
            .find(|d| Some(d.item.uid.as_str()) == uid)
            .map(|d| d.item.clone())
            .or_else(|| self.next_item.clone().filter(|n| Some(n.uid.as_str()) == uid))
            .or_else(|| self.job_item.clone().filter(|n| Some(n.uid.as_str()) == uid))
    }

    fn seek_current(&mut self, t: f64) {
        let Some(d) = &mut self.current else { return };
        if let Err(e) = seek_deck(d, t) {
            tracing::warn!("시크 실패: {e}");
        }
        let offset = d.frames_read as f64 / d.spec.rate as f64;
        self.start_segment(offset);
    }

    /// 장치가 지금 틀고 있는 구간.
    fn segment_at_read(&self) -> Option<&Segment> {
        let read = self.shared.as_ref()?.read.load(Ordering::Acquire);
        self.segments.iter().rev().find(|s| s.start <= read).or(self.segments.front())
    }

    /// 장치가 지금 틀고 있는 곡의 위치(초).
    fn playing_position(&self) -> Option<f64> {
        let read = self.shared.as_ref()?.read.load(Ordering::Acquire);
        let seg = self.segment_at_read()?;
        Some(seg.offset + read.saturating_sub(seg.start) as f64 / self.out_rate() as f64)
    }

    /// 링에 빈자리가 있는 만큼 채운다.
    fn fill(&mut self) -> anyhow::Result<()> {
        if !self.want_play || self.current.is_none() {
            return Ok(());
        }
        let ch_out = self.out_channels();
        let out_rate = self.out_rate();
        let chunk = ((out_rate as f64 * CHUNK_SECONDS) as usize).max(64) * ch_out;
        for _ in 0..64 {
            // 먼저 남은 것부터
            if !self.pending.is_empty() {
                let prod = self.producer.as_mut().unwrap();
                let (pushed, _) = prod.push_partial_slice(&self.pending);
                let n = pushed.len();
                self.pending.drain(..n);
                self.written += (n / ch_out) as u64;
                if !self.pending.is_empty() {
                    return Ok(());
                }
            }
            let free = self.producer.as_ref().unwrap().slots();
            if free < chunk {
                return Ok(());
            }
            if self.draining {
                return Ok(());
            }
            // 네트워크 곡을 아직 다 못 받았으면 기다리지 않고 돌아간다(명령은 계속 받는다).
            if self.current.as_ref().is_some_and(|d| !d.source.ready()) {
                return Ok(());
            }
            self.render_some()?;
        }
        Ok(())
    }

    /// 지금 곡에서 패킷 몇 개를 풀어 출력 형식으로 바꿔 `pending`에 붙인다.
    ///   원본 → (DSP면 스테레오로) → 볼륨 레벨링 → 장치 샘플레이트로 → 체인 → 클리핑 측정 → 장치 채널
    fn render_some(&mut self) -> anyhow::Result<()> {
        let ch_out = self.out_channels();
        let out_rate = self.out_rate();
        let Some(d) = self.current.as_mut() else { return Ok(()) };
        self.scratch.clear();
        let n = d.source.read(&mut self.scratch)?;
        if n == 0 {
            return self.end_of_track();
        }
        d.frames_read += n as u64;
        let ch_in = d.spec.channels.max(1);
        let in_rate = d.spec.rate;
        let quality = d.req.as_ref().map(quality_of).unwrap_or_default();
        let dsp = self.chain.is_some() || self.clip.is_some();
        // 1. 채널: DSP는 스테레오에서 돈다. 아니면 바로 장치 채널로.
        let work_ch = if dsp { 2 } else { ch_out };
        self.mapped.clear();
        if dsp {
            to_stereo(&self.scratch, ch_in, &mut self.mapped);
        } else {
            self.mapped.reserve(n * ch_out);
            map_channels(&self.scratch, ch_in, &mut self.mapped, ch_out);
        }
        // 2. 볼륨 레벨링(곡마다)
        if d.gain != 1.0 {
            for s in &mut self.mapped {
                *s *= d.gain;
            }
        }
        // 3. 장치 샘플레이트로
        self.stereo.clear();
        if in_rate == out_rate {
            std::mem::swap(&mut self.stereo, &mut self.mapped);
        } else {
            let rs = self.resampler.get_or_insert_with(|| Resampler::new(in_rate, out_rate, work_ch, quality));
            if rs.in_rate != in_rate {
                *rs = Resampler::new(in_rate, out_rate, work_ch, quality);
            }
            rs.process(&self.mapped, &mut self.stereo);
        }
        // 4. 체인(헤드룸·필터)과 클리핑 측정
        if let Some((_, c)) = &mut self.chain {
            c.process(&mut self.stereo);
        }
        if let Some(m) = &mut self.clip {
            m.scan(&self.stereo, 2);
            if !m.times.is_empty() {
                let uid = self.current.as_ref().map(|d| d.item.uid.clone()).unwrap_or_default();
                self.clips.record(&format!("native.{uid}"), &m.times, m.peak);
                m.times.clear();
            }
        }
        // 5. 장치 채널로
        if dsp && ch_out != 2 {
            let mut v = Vec::with_capacity(self.stereo.len() / 2 * ch_out);
            map_channels(&self.stereo, 2, &mut v, ch_out);
            self.pending.extend_from_slice(&v);
        } else {
            self.pending.extend_from_slice(&self.stereo);
        }
        Ok(())
    }

    fn end_of_track(&mut self) -> anyhow::Result<()> {
        // 한 곡 반복: 처음으로 돌아가 이음매 없이 잇는다(리샘플러·체인이 그대로 이어진다).
        if self.repeat_one
            && let Some(d) = self.current.as_mut()
            && seek_deck(d, 0.0).is_ok()
        {
            self.sync_chain(0.0);
            self.start_segment(0.0);
            return Ok(());
        }
        let out_rate = self.out_rate();
        let next_rate = self.next.as_ref().map(|n| n.spec.rate);
        // 리샘플러: 다음 곡이 같은 샘플레이트면 기록을 이어 쓴다(이음매 없음). 다르면 꼬리를 뺀다.
        if let Some(rs) = &mut self.resampler
            && next_rate != Some(rs.in_rate)
        {
            rs.flush(&mut self.pending);
            self.resampler = None;
        }
        match self.next.take() {
            Some(next) => {
                let uid = next.item.uid.clone();
                // DSP를 켜고 끄는 경계에서는 채널 배치가 바뀌므로 리샘플러를 새로 만든다.
                let was = self.chain.is_some() || self.clip.is_some();
                if let Some(prev) = self.current.replace(next) {
                    self.previous.push_front(prev.item);
                    self.previous.truncate(QUEUE_MAX);
                }
                self.next_item = None;
                // 엔진이 들고 있는 대기열에서 다음 곡을 꺼내고, 그다음 곡을 미리 연다.
                if self.upcoming.front().is_some_and(|u| u.uid == uid) {
                    self.upcoming.pop_front();
                }
                if let Some(n) = self.upcoming.front().cloned() {
                    self.next_item = Some(n.clone());
                    self.preload(n);
                }
                self.sync_chain(0.0);
                let now = self.chain.is_some() || self.clip.is_some();
                if was != now
                    && let Some(rs) = &mut self.resampler
                {
                    rs.flush(&mut self.pending);
                    self.resampler = None;
                }
                self.start_segment(0.0);
                tracing::debug!(uid, out_rate, "다음 곡을 이어 붙였습니다");
            }
            None => {
                self.draining = true;
                if let Some(s) = &self.shared {
                    // 남은 것을 다 넣은 뒤에 비는 것은 끊김이 아니다.
                    if self.pending.is_empty() {
                        s.drained.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
        Ok(())
    }

    /// 장치가 어디를 틀고 있는지 보고 화면에 알릴 것을 고친다.
    fn update_state(&mut self) {
        let Some(shared) = self.shared.clone() else { return };
        // 다 넣은 뒤 pending까지 비면 링이 비는 것은 정상이다.
        if self.draining && self.pending.is_empty() {
            shared.drained.store(true, Ordering::Relaxed);
        }
        let read = shared.read.load(Ordering::Acquire);
        let rate = self.out_rate() as f64;
        // 다 지나간 구간은 버린다(마지막 것은 남긴다).
        while self.segments.len() > 1 && self.segments[1].start <= read {
            self.segments.pop_front();
        }
        let seg = self.segments.front().cloned();
        let mut events = Vec::new();
        // 지금 들리는 구간과 그 안의 위치
        let mut patch: Option<(Segment, f64)> = None;
        if let Some(seg) = &seg {
            let pos = if read >= seg.start { seg.offset + (read - seg.start) as f64 / rate } else { seg.offset };
            let pos = if seg.duration > 0.0 { pos.min(seg.duration) } else { pos };
            if self.reported_uid.as_deref() != Some(seg.uid.as_str()) && read >= seg.start {
                self.reported_uid = Some(seg.uid.clone());
                events.push(Event::Advanced { uid: seg.uid.clone() });
            }
            patch = Some((seg.clone(), pos));
        }
        // 대기열 끝: 넣은 것을 장치가 다 틀었다.
        let ended = self.draining
            && self.pending.is_empty()
            && read >= self.written
            && self.want_play
            && self.preload_job.is_none()
            && self.job.is_none();
        if ended {
            let uid = seg.as_ref().map(|s| s.uid.clone()).unwrap_or_default();
            self.want_play = false;
            self.draining = false;
            self.current = None;
            shared.paused.store(true, Ordering::Relaxed);
            events.push(Event::Ended { uid });
        }
        let playing_now = self.want_play && seg.as_ref().is_some_and(|s| read > s.start);
        let underruns = shared.underruns.load(Ordering::Relaxed);
        let resampling = self.resampler.as_ref().map(|r| (r.in_rate, r.out_rate));
        {
            let mut s = self.state.lock();
            let mut changed = false;
            if let Some((seg, pos)) = patch {
                if s.uid.as_deref() != Some(seg.uid.as_str()) || s.source != seg.source || s.duration != seg.duration || s.meta != seg.meta
                {
                    s.uid = Some(seg.uid);
                    s.duration = seg.duration;
                    s.source = seg.source;
                    s.meta = seg.meta;
                    changed = true;
                }
                if (s.position - pos).abs() > 0.2 {
                    changed = true;
                }
                s.position = pos;
            }
            if ended {
                s.status = Status::Paused;
                changed = true;
            } else if playing_now && s.status == Status::Loading {
                s.status = Status::Playing;
                changed = true;
            }
            if s.underruns != underruns || s.resampling != resampling {
                s.underruns = underruns;
                s.resampling = resampling;
                changed = true;
            }
            if changed {
                s.seq += 1;
            }
        }
        for e in events {
            self.emit(e);
        }
    }
}

/// 원본 채널 → 장치 채널. 모노는 양쪽으로, 장치보다 많으면 앞 둘에 섞고, 적으면 0을 채운다.
fn map_channels(input: &[f64], ch_in: usize, out: &mut Vec<f64>, ch_out: usize) {
    if ch_in == ch_out {
        out.extend_from_slice(input);
        return;
    }
    for f in input.chunks_exact(ch_in) {
        match (ch_in, ch_out) {
            (1, _) => {
                for _ in 0..ch_out.min(2) {
                    out.push(f[0]);
                }
                for _ in 2..ch_out {
                    out.push(0.0);
                }
            }
            (_, 1) => out.push(f.iter().sum::<f64>() / ch_in as f64),
            (n, 2) if n >= 6 => {
                // 5.1 → 스테레오(ITU 다운믹스): L + 0.707C + 0.707Ls, R + 0.707C + 0.707Rs
                let c = std::f64::consts::FRAC_1_SQRT_2;
                out.push(f[0] + c * f[2] + c * f[4]);
                out.push(f[1] + c * f[2] + c * f[5]);
            }
            _ => {
                out.extend(f.iter().copied().chain(std::iter::repeat(0.0)).take(ch_out));
            }
        }
    }
}
