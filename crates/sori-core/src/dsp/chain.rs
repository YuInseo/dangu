//! DSP 체인. 룬의 MUSE와 같은 순서:
//!   (볼륨 레벨링: 곡마다 따로) → 헤드룸 → [사용자가 순서를 정한 필터들]
//! 샘플레이트 변환과 비트 깊이 변환은 체인 밖(재생 엔진·스트림)에서 한다.
//! 모든 계산은 64비트, 스테레오 인터리브.

use std::collections::{HashMap, VecDeque};

use parking_lot::Mutex;
use serde::Serialize;

use crate::dsp::biquad::{Coeffs, State, coeffs};
use crate::dsp::convolver::{Convolver, peak_normalize};
use crate::dsp::crossfeed::Bs2b;
use crate::dsp::ir::IrStore;
use crate::dsp::request::{Band, BandType, Channel, CrossfeedPreset, DspRequest, Filter, Step};

fn db(g: f64) -> f64 {
    10f64.powf(g / 20.0)
}

fn mask(ch: Channel) -> [bool; 2] {
    match ch {
        Channel::L => [true, false],
        Channel::R => [false, true],
        Channel::LR => [true, true],
    }
}

/// 통과·차단 필터가 아니면 게인이 0인 밴드는 아무것도 하지 않는다.
fn effective(b: &Band) -> bool {
    b.on && (matches!(b.kind, BandType::Lowpass | BandType::Highpass | BandType::Bandpass | BandType::Notch) || b.gain != 0.0)
}

enum Op {
    Gain([f64; 2]),
    Biquad { c: Coeffs, on: [bool; 2], st: [State; 2] },
    Crossfeed(Box<Bs2b>),
    Delay { on: [bool; 2], line: [VecDeque<f64>; 2] },
    Mix { ll: f64, lr: f64, rl: f64, rr: f64 },
    Convolve(Box<Convolver>),
}

pub struct Chain {
    ops: Vec<Op>,
    latency: usize,
    /// 지연 보정으로 아직 버려야 할 프레임
    drop: usize,
    pub rate: u32,
    pub warnings: Vec<String>,
}

impl Chain {
    /// 아무것도 하지 않는 체인
    pub fn empty(rate: u32) -> Chain {
        Chain { ops: Vec::new(), latency: 0, drop: 0, rate, warnings: Vec::new() }
    }

    pub fn new(req: &DspRequest, rate: u32, irs: Option<&IrStore>) -> Chain {
        let fs = rate as f64;
        let mut ops = Vec::new();
        let mut warnings = Vec::new();
        if let Some(hr) = req.hr
            && hr != 0.0
        {
            ops.push(Op::Gain([db(hr); 2]));
        }
        let mut latency = 0;
        for f in &req.f {
            match f {
                Filter::Crossfeed { p, fc, fd } => {
                    let (cut, feed) = match p {
                        CrossfeedPreset::Custom => (fc.unwrap_or(700.0), fd.unwrap_or(4.5)),
                        CrossfeedPreset::Cmoy => Bs2b::preset("cmoy"),
                        CrossfeedPreset::Jmeier => Bs2b::preset("jmeier"),
                        CrossfeedPreset::Default => Bs2b::preset("default"),
                    };
                    ops.push(Op::Crossfeed(Box::new(Bs2b::new(cut, feed, fs))));
                }
                Filter::Peq { pre, b } | Filter::Headphone { pre, b, .. } => {
                    if *pre != 0.0 {
                        ops.push(Op::Gain([db(*pre); 2]));
                    }
                    for band in b.iter().filter(|b| effective(b)) {
                        ops.push(Op::Biquad { c: coeffs(band, fs), on: [true, true], st: [State::default(); 2] });
                    }
                }
                Filter::Procedural { s } => {
                    for step in s {
                        match step {
                            Step::Volume { ch, gain } => {
                                let m = mask(*ch);
                                ops.push(Op::Gain([if m[0] { db(*gain) } else { 1.0 }, if m[1] { db(*gain) } else { 1.0 }]));
                            }
                            Step::Delay { ch, ms } => {
                                let n = (ms * fs / 1000.0).round() as usize;
                                if n > 0 {
                                    let m = mask(*ch);
                                    let line = [
                                        VecDeque::from(vec![0.0; if m[0] { n } else { 0 }]),
                                        VecDeque::from(vec![0.0; if m[1] { n } else { 0 }]),
                                    ];
                                    ops.push(Op::Delay { on: m, line });
                                }
                            }
                            Step::Invert { ch } => {
                                let m = mask(*ch);
                                ops.push(Op::Gain([if m[0] { -1.0 } else { 1.0 }, if m[1] { -1.0 } else { 1.0 }]));
                            }
                            Step::Mute { ch } => {
                                let m = mask(*ch);
                                ops.push(Op::Gain([if m[0] { 0.0 } else { 1.0 }, if m[1] { 0.0 } else { 1.0 }]));
                            }
                            Step::Mix { ll, lr, rl, rr } => ops.push(Op::Mix { ll: *ll, lr: *lr, rl: *rl, rr: *rr }),
                            Step::Eq { ch, band } => {
                                if band.on {
                                    ops.push(Op::Biquad { c: coeffs(band, fs), on: mask(*ch), st: [State::default(); 2] });
                                }
                            }
                        }
                    }
                }
                Filter::Convolution { ir, g, n, name } => match irs.map(|s| s.load(ir, rate)) {
                    Some(Ok(data)) => {
                        let mut chans: Vec<Vec<f64>> = data.as_ref().clone();
                        if *n {
                            let k = peak_normalize(&chans);
                            for c in &mut chans {
                                for v in c.iter_mut() {
                                    *v *= k;
                                }
                            }
                        }
                        let cv = Convolver::new(&chans, 1024);
                        latency += cv.latency();
                        ops.push(Op::Convolve(Box::new(cv)));
                        if *g != 0.0 {
                            ops.push(Op::Gain([db(*g); 2]));
                        }
                    }
                    _ => warnings.push(format!("임펄스 응답 {}을(를) 열 수 없어 건너뜁니다", name.as_deref().unwrap_or(ir))),
                },
            }
        }
        Chain { ops, latency, drop: latency, rate, warnings }
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn latency(&self) -> usize {
        self.latency
    }

    /// 스테레오 인터리브를 제자리에서 처리한다. 지연 보정 때문에 처음에는 길이가 줄 수 있다.
    pub fn process(&mut self, buf: &mut Vec<f64>) {
        for op in &mut self.ops {
            match op {
                Op::Gain(g) => {
                    for f in buf.chunks_exact_mut(2) {
                        f[0] *= g[0];
                        f[1] *= g[1];
                    }
                }
                Op::Biquad { c, on, st } => {
                    for f in buf.chunks_exact_mut(2) {
                        if on[0] {
                            f[0] = st[0].run(c, f[0]);
                        }
                        if on[1] {
                            f[1] = st[1].run(c, f[1]);
                        }
                    }
                }
                Op::Crossfeed(x) => x.process(buf),
                Op::Delay { on, line } => {
                    for f in buf.chunks_exact_mut(2) {
                        for c in 0..2 {
                            if on[c] {
                                line[c].push_back(f[c]);
                                f[c] = line[c].pop_front().unwrap_or(0.0);
                            }
                        }
                    }
                }
                Op::Mix { ll, lr, rl, rr } => {
                    for f in buf.chunks_exact_mut(2) {
                        let (l, r) = (f[0], f[1]);
                        f[0] = *ll * l + *lr * r;
                        f[1] = *rl * l + *rr * r;
                    }
                }
                Op::Convolve(cv) => cv.process(buf),
            }
        }
        if self.drop > 0 {
            let n = self.drop.min(buf.len() / 2);
            buf.drain(..n * 2);
            self.drop -= n;
        }
    }

    /// 꼬리를 밀어낸다(컨볼루션 지연만큼). 스트림이 끝날 때 부른다.
    pub fn flush(&mut self, out: &mut Vec<f64>) {
        if self.latency == 0 {
            return;
        }
        let mut pad = vec![0.0; self.latency * 2];
        self.process(&mut pad);
        out.extend_from_slice(&pad);
    }

    pub fn reset(&mut self) {
        for op in &mut self.ops {
            match op {
                Op::Biquad { st, .. } => *st = [State::default(); 2],
                Op::Crossfeed(x) => x.reset(),
                Op::Delay { line, .. } => {
                    for l in line {
                        let n = l.len();
                        l.clear();
                        l.resize(n, 0.0);
                    }
                }
                Op::Convolve(cv) => cv.reset(),
                _ => {}
            }
        }
        self.drop = self.latency;
    }
}

/// 마지막 양자화 바로 앞에서 0dBFS를 넘는 순간을 센다(한 샘플이라도). 곡의 몇 초인지로 적는다.
pub struct ClipMeter {
    rate: f64,
    /// 곡에서의 시작 위치(초)
    offset: f64,
    frame: u64,
    pub times: Vec<f64>,
    pub peak: f64,
}

impl ClipMeter {
    pub fn new(rate: u32, offset: f64) -> Self {
        ClipMeter { rate: rate as f64, offset, frame: 0, times: Vec::new(), peak: 0.0 }
    }

    pub fn scan(&mut self, buf: &[f64], channels: usize) {
        for (i, f) in buf.chunks_exact(channels).enumerate() {
            let m = f.iter().fold(0.0f64, |a, v| a.max(v.abs()));
            if m > 1.0 {
                let t = self.offset + (self.frame + i as u64) as f64 / self.rate;
                if self.times.last().is_none_or(|l| t - l > 0.05) {
                    self.times.push((t * 1000.0).round() / 1000.0);
                }
                self.peak = self.peak.max(m);
            }
        }
        self.frame += (buf.len() / channels) as u64;
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ClipReport {
    pub times: Vec<f64>,
    pub peak: Option<f64>,
}

struct Session {
    times: Vec<f64>,
    peak: f64,
    at: i64,
}

/// 재생 세션(존.대기열칸)마다의 클리핑 기록. 화면이 재생 위치에 맞춰 표시등을 켠다.
#[derive(Default)]
pub struct Clips {
    sessions: Mutex<HashMap<String, Session>>,
}

pub fn valid_sid(s: &str) -> bool {
    (4..=80).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

impl Clips {
    pub fn record(&self, sid: &str, times: &[f64], peak: f64) {
        if times.is_empty() {
            return;
        }
        let mut m = self.sessions.lock();
        let s = m.entry(sid.to_string()).or_insert(Session { times: Vec::new(), peak: 0.0, at: 0 });
        for &t in times {
            if s.times.last().is_none_or(|l| (t - l).abs() > 0.05) {
                s.times.push(t);
            }
        }
        if s.times.len() > 5000 {
            let cut = s.times.len() - 5000;
            s.times.drain(..cut);
        }
        s.peak = s.peak.max(peak);
        s.at = crate::model::now_ms();
        if m.len() > 200 {
            let cutoff = crate::model::now_ms() - 30 * 60_000;
            m.retain(|_, v| v.at >= cutoff);
        }
    }

    /// 탐색해서 새 스트림이 시작되면 그 위치 뒤의 기록은 다시 쌓인다.
    pub fn reset_after(&self, sid: &str, from: f64) {
        if let Some(s) = self.sessions.lock().get_mut(sid) {
            s.times.retain(|t| *t < from);
        }
    }

    pub fn get(&self, sid: &str) -> ClipReport {
        let m = self.sessions.lock();
        match m.get(sid) {
            Some(s) => {
                ClipReport { times: s.times.clone(), peak: (s.peak > 0.0).then(|| ((20.0 * s.peak.log10()) * 100.0).round() / 100.0) }
            }
            None => ClipReport::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::request::{Dither, Out};

    fn req(f: Vec<Filter>, hr: Option<f64>) -> DspRequest {
        DspRequest { v: 1, hr, ci: Some(1), lv: None, src: None, out: Out { b: 24, d: Dither::Tpdf }, f }
    }

    #[test]
    fn headroom_and_procedural() {
        let r = req(
            vec![Filter::Procedural {
                s: vec![
                    Step::Volume { ch: Channel::L, gain: -6.0 },
                    Step::Invert { ch: Channel::R },
                    Step::Delay { ch: Channel::R, ms: 1.0 },
                    Step::Mix { ll: 1.0, lr: 0.0, rl: 0.0, rr: 1.0 },
                ],
            }],
            Some(-3.0),
        );
        let mut c = Chain::new(&r, 48_000, None);
        let mut buf = vec![0.5; 2 * 200];
        c.process(&mut buf);
        let g = 10f64.powf(-3.0 / 20.0);
        assert!((buf[0] - 0.5 * g * 10f64.powf(-6.0 / 20.0)).abs() < 1e-12);
        // 오른쪽: 48샘플(1ms) 지연, 그 뒤에는 반전
        assert_eq!(buf[1], 0.0);
        assert!((buf[2 * 48 + 1] + 0.5 * g).abs() < 1e-12);
    }

    #[test]
    fn convolution_latency_is_compensated() {
        let dir = tempfile::tempdir().unwrap();
        let store = IrStore::new(dir.path());
        // 지연 없는 디랙 IR을 WAV로 만들어 올린다
        let mut wav = Vec::new();
        {
            let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 24, sample_format: hound::SampleFormat::Int };
            let mut w = hound::WavWriter::new(std::io::Cursor::new(&mut wav), spec).unwrap();
            w.write_sample(4_194_304i32).unwrap(); // 0.5
            for _ in 0..999 {
                w.write_sample(0i32).unwrap();
            }
            w.finalize().unwrap();
        }
        let info = store.save("dirac.wav", &wav).unwrap();
        let r = req(vec![Filter::Convolution { ir: info.id.clone(), g: 0.0, n: true, name: None }], None);
        let mut c = Chain::new(&r, 48_000, Some(&store));
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        let x: Vec<f64> = (0..10_000)
            .flat_map(|i| {
                let v = (i as f64 * 0.01).sin() * 0.25;
                [v, v]
            })
            .collect();
        let mut out = Vec::new();
        for chunk in x.chunks(2 * 777) {
            let mut b = chunk.to_vec();
            c.process(&mut b);
            out.extend_from_slice(&b);
        }
        c.flush(&mut out);
        // 정규화(최대 이득 0dB)했으니 디랙 0.5 → 1.0, 지연은 보정돼서 입력과 그대로 같다.
        assert_eq!(out.len(), x.len());
        let max = out.iter().zip(&x).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        assert!(max < 1e-9, "{max}");
    }

    #[test]
    fn clip_meter_and_sessions() {
        let mut m = ClipMeter::new(1000, 10.0);
        let mut buf = vec![0.1; 2 * 1000];
        buf[2 * 500] = 1.2;
        m.scan(&buf, 2);
        assert_eq!(m.times, vec![10.5]);
        let clips = Clips::default();
        clips.record("zone.abc", &m.times, m.peak);
        assert_eq!(clips.get("zone.abc").times, vec![10.5]);
        clips.reset_after("zone.abc", 10.0);
        assert!(clips.get("zone.abc").times.is_empty());
        assert!(valid_sid("abcd.ef-1_2"));
        assert!(!valid_sid("../x"));
    }
}
