//! 소리를 내보내는 곳. 장치(cpal)든, 시험용 가짜든 같은 모양이다.
//!
//! 렌더 스레드가 64비트 샘플을 링 버퍼에 채우고, 출력 쪽(오디오 콜백)이 꺼내 간다.
//! 볼륨은 콜백에서 건다. 그래야 손을 대자마자 들린다(링 버퍼만큼 늦지 않는다).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rtrb::Consumer;
use serde::Serialize;

/// 렌더 스레드와 콜백이 같이 보는 것.
pub struct Shared {
    /// 콜백이 링에서 꺼낸 프레임 수(들려준 것 + 버린 것). 처음부터 센 절대값.
    pub read: AtomicU64,
    /// 이 절대 프레임 전까지는 버린다(시크·곡 바꾸기 때 링을 비우는 방법).
    pub flush_to: AtomicU64,
    pub paused: AtomicBool,
    /// 목표 게인(선형, f64 비트). 콜백이 버퍼 하나 동안 부드럽게 옮겨 간다.
    pub gain: AtomicU64,
    /// 링이 비어 무음을 낸 횟수
    pub underruns: AtomicU64,
    /// 렌더 스레드가 끝까지 다 썼다(더 올 것이 없다). 이때의 빈 링은 언더런이 아니다.
    pub drained: AtomicBool,
    /// 장치가 사라지는 등 출력이 깨졌다.
    pub failed: AtomicBool,
    pub channels: usize,
}

impl Shared {
    pub fn new(channels: usize) -> Arc<Shared> {
        Arc::new(Shared {
            read: AtomicU64::new(0),
            flush_to: AtomicU64::new(0),
            paused: AtomicBool::new(true),
            gain: AtomicU64::new(1.0f64.to_bits()),
            underruns: AtomicU64::new(0),
            drained: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            channels,
        })
    }

    pub fn set_gain(&self, g: f64) {
        self.gain.store(g.to_bits(), Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SampleKind {
    F32,
    F64,
    I16,
    I24,
    I32,
}

impl SampleKind {
    pub fn bits(self) -> u32 {
        match self {
            SampleKind::F32 => 32,
            SampleKind::F64 => 64,
            SampleKind::I16 => 16,
            SampleKind::I24 => 24,
            SampleKind::I32 => 32,
        }
    }
    pub fn is_float(self) -> bool {
        matches!(self, SampleKind::F32 | SampleKind::F64)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutSpec {
    pub rate: u32,
    pub channels: usize,
    pub format: SampleKind,
    /// 장치 이름
    pub device: String,
    /// "WASAPI 공유 모드", "AAudio" …
    pub api: String,
    /// 운영체제가 소리를 섞는가(공유 모드). 그렇다면 비트퍼펙트가 아니다.
    pub mixer: bool,
}

pub trait Output {
    /// 장치를 연다. `rate`를 주면 그 샘플레이트로 열어 보고, 안 되면 장치가 고른 것으로 연다.
    fn open(&mut self, rate: Option<u32>, consumer: Consumer<f64>, shared: Arc<Shared>) -> anyhow::Result<OutSpec>;
    fn close(&mut self);
    /// 장치가 받아 주는 샘플레이트와 채널 수(열기 전에 묻는다).
    fn probe(&mut self) -> anyhow::Result<(Vec<u32>, usize)>;
}

/// 콜백 쪽 상태. 콜백 하나가 하나를 가진다.
pub struct Callback {
    consumer: Consumer<f64>,
    shared: Arc<Shared>,
    read: u64,
    gain: f64,
}

impl Callback {
    pub fn new(consumer: Consumer<f64>, shared: Arc<Shared>) -> Self {
        let gain = f64::from_bits(shared.gain.load(Ordering::Relaxed));
        Callback { consumer, shared, read: 0, gain }
    }

    /// `out`(인터리브, f64)을 채운다. 링이 모자라면 나머지는 무음.
    /// 실제 소리(링에서 꺼낸 것)로 채운 프레임 수를 돌려준다.
    pub fn fill(&mut self, out: &mut [f64]) -> usize {
        let ch = self.shared.channels;
        let frames = out.len() / ch;
        let flush_to = self.shared.flush_to.load(Ordering::Acquire);
        while self.read < flush_to {
            let avail = self.consumer.slots() / ch;
            let n = (avail as u64).min(flush_to - self.read) as usize;
            if n == 0 {
                break;
            }
            if let Ok(c) = self.consumer.read_chunk(n * ch) {
                c.commit_all();
            }
            self.read += n as u64;
        }
        if self.shared.paused.load(Ordering::Relaxed) || self.read < flush_to {
            out.fill(0.0);
            self.shared.read.store(self.read, Ordering::Release);
            return 0;
        }
        let avail = self.consumer.slots() / ch;
        let n = frames.min(avail);
        if n > 0 {
            let (got, _) = self.consumer.pop_partial_slice(&mut out[..n * ch]);
            debug_assert_eq!(got.len(), n * ch);
        }
        out[n * ch..].fill(0.0);
        if n < frames && !self.shared.drained.load(Ordering::Relaxed) {
            self.shared.underruns.fetch_add(1, Ordering::Relaxed);
        }
        // 볼륨: 버퍼 하나에 걸쳐 직선으로 옮겨 간다(지퍼 잡음 없이).
        let target = f64::from_bits(self.shared.gain.load(Ordering::Relaxed));
        if (target - self.gain).abs() < 1e-12 {
            if target != 1.0 {
                for s in &mut out[..n * ch] {
                    *s *= target;
                }
            }
        } else {
            let step = (target - self.gain) / n.max(1) as f64;
            for f in 0..n {
                let g = self.gain + step * (f + 1) as f64;
                for s in &mut out[f * ch..(f + 1) * ch] {
                    *s *= g;
                }
            }
            self.gain = target;
        }
        self.read += n as u64;
        self.shared.read.store(self.read, Ordering::Release);
        n
    }
}

/// 시험용 출력. 장치 대신 스레드 하나가 꺼내 간다. `realtime`이 아니면 최대한 빨리.
pub struct NullOutput {
    pub rates: Vec<u32>,
    pub channels: usize,
    pub preferred: u32,
    pub realtime: bool,
    pub captured: Arc<parking_lot::Mutex<Vec<f64>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl NullOutput {
    pub fn new(preferred: u32, realtime: bool) -> Self {
        NullOutput {
            rates: vec![44_100, 48_000, 88_200, 96_000, 176_400, 192_000],
            channels: 2,
            preferred,
            realtime,
            captured: Arc::new(parking_lot::Mutex::new(Vec::new())),
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }
}

impl Output for NullOutput {
    fn open(&mut self, rate: Option<u32>, consumer: Consumer<f64>, shared: Arc<Shared>) -> anyhow::Result<OutSpec> {
        self.close();
        let rate = rate.filter(|r| self.rates.contains(r)).unwrap_or(self.preferred);
        let stop = Arc::new(AtomicBool::new(false));
        self.stop = stop.clone();
        let captured = self.captured.clone();
        let realtime = self.realtime;
        let ch = self.channels;
        self.thread = Some(std::thread::spawn(move || {
            let mut cb = Callback::new(consumer, shared.clone());
            let frames = 512;
            let mut buf = vec![0.0; frames * ch];
            let started = std::time::Instant::now();
            let mut done = 0u64;
            while !stop.load(Ordering::Relaxed) {
                let played = cb.fill(&mut buf);
                if played > 0 {
                    captured.lock().extend_from_slice(&buf[..played * ch]);
                }
                done += frames as u64;
                if realtime {
                    let due = std::time::Duration::from_secs_f64(done as f64 / rate as f64);
                    if let Some(wait) = due.checked_sub(started.elapsed()) {
                        std::thread::sleep(wait);
                    }
                } else if played == 0 {
                    std::thread::sleep(std::time::Duration::from_micros(200));
                }
            }
        }));
        Ok(OutSpec { rate, channels: ch, format: SampleKind::F64, device: "시험용 출력".into(), api: "null".into(), mixer: false })
    }

    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    fn probe(&mut self) -> anyhow::Result<(Vec<u32>, usize)> {
        Ok((self.rates.clone(), self.channels))
    }
}

impl Drop for NullOutput {
    fn drop(&mut self) {
        self.close();
    }
}
