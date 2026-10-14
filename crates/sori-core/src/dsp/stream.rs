//! 곡 하나를 DSP 요청대로 처리하는 흐름(브라우저 존의 스트림).
//!   원본 → 스테레오 → 볼륨 레벨링 → 체인(헤드룸, 필터들) → 샘플레이트 변환 → 클리핑 측정
//! 비트 깊이 변환(디더)은 부르는 쪽이 마지막에 한다.

use crate::decode::Spec;
use crate::dsp::chain::{Chain, ClipMeter};
use crate::dsp::ir::IrStore;
use crate::dsp::leveling::{LevelingResult, leveling_gain};
use crate::dsp::request::{DspRequest, SrcFilter};
use crate::dsp::resample::{Filter, Quality, Resampler};
use crate::model::Loudness;

/// 원본 채널 → 스테레오. 모노는 양쪽에, 3채널 이상은 ITU 다운믹스.
pub fn to_stereo(input: &[f64], ch: usize, out: &mut Vec<f64>) {
    if ch == 2 {
        out.extend_from_slice(input);
        return;
    }
    let c = std::f64::consts::FRAC_1_SQRT_2;
    for f in input.chunks_exact(ch.max(1)) {
        match ch {
            1 => out.extend_from_slice(&[f[0], f[0]]),
            n if n >= 6 => out.extend_from_slice(&[f[0] + c * f[2] + c * f[4], f[1] + c * f[2] + c * f[5]]),
            _ => out.extend_from_slice(&[f[0], f[1]]),
        }
    }
}

pub fn quality_of(req: &DspRequest) -> Quality {
    match req.src {
        Some(s) => Quality { bits: s.p, filter: if s.f == SrcFilter::Smooth { Filter::Smooth } else { Filter::Precise } },
        None => Quality::default(),
    }
}

pub struct Processor {
    ch_in: usize,
    gain: f64,
    pub level: Option<LevelingResult>,
    chain: Chain,
    rs: Option<Resampler>,
    clip: Option<ClipMeter>,
    pub out_rate: u32,
    scratch: Vec<f64>,
    mid: Vec<f64>,
}

impl Processor {
    pub fn new(req: &DspRequest, spec: &Spec, loudness: Option<&Loudness>, at: f64, irs: Option<&IrStore>) -> Processor {
        let level = req.lv.map(|lv| leveling_gain(loudness, lv.m, lv.t, lv.u));
        let gain = level.map(|l| 10f64.powf(l.gain / 20.0)).unwrap_or(1.0);
        let chain = Chain::new(req, spec.rate, irs);
        // DSD를 푼 것(176.4kHz)이나 192kHz를 넘는 것은 브라우저가 받는 샘플레이트로 내린다.
        let target = req.src.map(|s| s.r).unwrap_or(if spec.rate > 192_000 { 176_400 } else { spec.rate });
        let rs = (target != spec.rate).then(|| Resampler::new(spec.rate, target, 2, quality_of(req)));
        let clip = req.clip_indicator().then(|| ClipMeter::new(target, at));
        Processor { ch_in: spec.channels.max(1), gain, level, chain, rs, clip, out_rate: target, scratch: Vec::new(), mid: Vec::new() }
    }

    pub fn warnings(&self) -> &[String] {
        &self.chain.warnings
    }

    /// 원본(인터리브, 원본 채널 수) → 출력(스테레오, out_rate)을 `out` 뒤에 붙인다.
    pub fn push(&mut self, input: &[f64], out: &mut Vec<f64>) {
        self.scratch.clear();
        to_stereo(input, self.ch_in, &mut self.scratch);
        if self.gain != 1.0 {
            for v in &mut self.scratch {
                *v *= self.gain;
            }
        }
        self.chain.process(&mut self.scratch);
        self.emit(out);
    }

    fn emit(&mut self, out: &mut Vec<f64>) {
        let start = out.len();
        match &mut self.rs {
            Some(rs) => rs.process(&self.scratch, out),
            None => out.extend_from_slice(&self.scratch),
        }
        if let Some(c) = &mut self.clip {
            c.scan(&out[start..], 2);
        }
    }

    /// 끝: 컨볼루션과 리샘플러의 꼬리를 밀어낸다.
    pub fn finish(&mut self, out: &mut Vec<f64>) {
        self.scratch.clear();
        self.chain.flush(&mut self.scratch);
        let start = out.len();
        match &mut self.rs {
            Some(rs) => {
                rs.process(&self.scratch, out);
                rs.flush(out);
            }
            None => out.extend_from_slice(&self.scratch),
        }
        if let Some(c) = &mut self.clip {
            c.scan(&out[start..], 2);
        }
        let _ = &self.mid;
    }

    /// 지금까지 잡힌 클리핑(곡에서의 초)과 최대값. 꺼내면 비워진다.
    pub fn take_clips(&mut self) -> Option<(Vec<f64>, f64)> {
        let c = self.clip.as_mut()?;
        if c.times.is_empty() {
            return None;
        }
        Some((std::mem::take(&mut c.times), c.peak))
    }
}
