//! 64비트 샘플레이트 변환기.
//!
//! 두 샘플레이트의 비를 기약분수 L/M으로 놓고, L배로 늘려 거른 뒤 M배로 줄이는 것을
//! 다상(polyphase) FIR 하나로 한다. 필터는 카이저 창 싱크이고, 정밀도(비트)를 감쇠(dB)로 바꿔
//! 길이를 정한다. 28비트(VHQ)면 저지대역 감쇠가 168dB, 33비트면 199dB이다.
//!
//! 비가 정확한 분수라 시간이 흐르지 않는다(누적 오차 없음). 곡이 바뀌어도 같은 비면
//! 기록을 이어 쓰므로 곡 사이가 이음매 없이 붙는다.

use crate::dsp::fir::{kaiser_beta, kaiser_taps, lowpass};

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    /// 통과 대역을 나이퀴스트의 91%까지 평탄하게, 가파르게 자른다.
    Precise,
    /// 통과 대역을 80%까지만 두고 완만하게. 앞울림(pre-ringing)이 짧다.
    Smooth,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quality {
    /// 정밀도(비트). 감쇠 = 비트 × 6.02dB.
    pub bits: u32,
    pub filter: Filter,
}

impl Default for Quality {
    fn default() -> Self {
        Quality { bits: 28, filter: Filter::Precise }
    }
}

pub struct Resampler {
    pub in_rate: u32,
    pub out_rate: u32,
    l: u64,
    m: u64,
    taps: usize,
    /// 위상마다 뒤집어 둔 계수: phases[p * taps + j]
    phases: Vec<f64>,
    channels: usize,
    /// 채널별 입력 기록. hist[c][0]의 절대 번호가 `base`.
    hist: Vec<Vec<f64>>,
    base: u64,
    /// 다음에 낼 출력 샘플 번호. 필터 지연만큼 앞당겨 시작한다(출력 0번 = 입력 0번).
    k: u64,
    /// 지연 보정으로 건너뛴 출력 수
    d: u64,
    /// 들어온 입력 샘플 수(채널당)
    fed: u64,
    delay_in: f64,
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32, channels: usize, q: Quality) -> Self {
        let g = gcd(in_rate as u64, out_rate as u64);
        let (l, m) = (out_rate as u64 / g, in_rate as u64 / g);
        let nyq = in_rate.min(out_rate) as f64 / 2.0;
        let atten = (q.bits as f64 * 6.02).clamp(60.0, 210.0);
        let (pass, stop) = match q.filter {
            Filter::Precise => (0.91 * nyq, nyq),
            Filter::Smooth => (0.80 * nyq, nyq),
        };
        // 필터는 입력 샘플레이트 기준으로 설계하고(위상 하나의 길이), L배 길이의 원형을 만든다.
        // 원형의 길이는 2·M·q + 1로 맞춘다. 그래야 지연((길이-1)/2)이 출력 샘플 q개에 딱 떨어져서
        // 출력 0번을 입력 0번과 같은 시각에 놓을 수 있다.
        let want = kaiser_taps(atten, stop - pass, in_rate as f64).max(8) as u64 * l;
        let q = (want - 1).div_ceil(2 * m);
        let len = (2 * m * q + 1) as usize;
        let per_phase = len.div_ceil(l as usize);
        let fs_up = in_rate as f64 * l as f64;
        let proto = lowpass(len, (pass + stop) / 2.0, fs_up, kaiser_beta(atten), l as f64);
        let mut phases = vec![0.0; l as usize * per_phase];
        for p in 0..l as usize {
            for j in 0..per_phase {
                let idx = j * l as usize + p;
                let v = proto.get(idx).copied().unwrap_or(0.0);
                // 뒤집어 둔다: 기록(오래된 것 → 새것)과 바로 내적한다.
                phases[p * per_phase + (per_phase - 1 - j)] = v;
            }
        }
        let delay_in = (proto.len() - 1) as f64 / 2.0 / l as f64;
        // 원형 필터의 가운데(지연)가 입력 0번에 오도록 출력 번호를 q부터 센다.
        let d = q;
        Resampler {
            in_rate,
            out_rate,
            l,
            m,
            taps: per_phase,
            phases,
            channels,
            hist: vec![vec![0.0; per_phase - 1]; channels],
            base: 0,
            k: d,
            d,
            fed: 0,
            delay_in,
        }
    }

    pub fn is_identity(&self) -> bool {
        self.l == self.m
    }

    /// 필터 지연(입력 샘플 단위). 위치를 계산할 때 뺀다.
    pub fn delay_in(&self) -> f64 {
        self.delay_in
    }

    pub fn taps(&self) -> usize {
        self.taps
    }

    /// 인터리브된 입력을 받아 낼 수 있는 만큼 출력에 붙인다.
    pub fn process(&mut self, input: &[f64], out: &mut Vec<f64>) {
        let ch = self.channels;
        let frames = input.len() / ch;
        for (c, h) in self.hist.iter_mut().enumerate() {
            h.reserve(frames);
            h.extend(input.iter().skip(c).step_by(ch).copied());
        }
        self.fed += frames as u64;
        self.drain(out);
    }

    fn drain(&mut self, out: &mut Vec<f64>) {
        let n_taps = self.taps;
        // 기록에는 앞에 (taps-1)개의 0이 깔려 있으므로, 입력 n번은 hist의 n + taps-1 - base 자리.
        loop {
            let pos = self.k * self.m;
            let n = pos / self.l;
            let p = (pos % self.l) as usize;
            if n >= self.fed {
                break;
            }
            let end = (n - self.base) as usize + n_taps; // n번까지 포함하는 창의 끝(배제)
            let coef = &self.phases[p * n_taps..(p + 1) * n_taps];
            for h in &self.hist {
                let x = &h[end - n_taps..end];
                let mut acc = 0.0;
                for (a, b) in x.iter().zip(coef) {
                    acc += a * b;
                }
                out.push(acc);
            }
            self.k += 1;
        }
        // 다음 출력에 필요한 것만 남긴다.
        let next_n = (self.k * self.m) / self.l;
        let keep_from = next_n.saturating_sub(self.base); // hist에서 (keep_from)부터 taps-1개 앞이 필요
        let drop = keep_from as usize;
        if drop > 4096 {
            for h in &mut self.hist {
                h.drain(..drop);
            }
            self.base += drop as u64;
        }
    }

    /// 꼬리를 밀어낸다. 지금까지 넣은 입력 N개에 해당하는 출력 N×L/M개가 다 나오게 한다.
    /// 이걸 부른 뒤에 다시 쓰려면 `reset`을 부른다.
    pub fn flush(&mut self, out: &mut Vec<f64>) {
        let produced = self.k - self.d;
        let total = (self.fed * self.l).div_ceil(self.m);
        let need = total.saturating_sub(produced) as usize;
        let start = out.len();
        let pad = vec![0.0; self.channels * (self.taps + 2)];
        self.process(&pad, out);
        out.truncate((start + need * self.channels).min(out.len()));
    }

    pub fn reset(&mut self) {
        for h in &mut self.hist {
            h.clear();
            h.resize(self.taps - 1, 0.0);
        }
        self.base = 0;
        self.k = self.d;
        self.fed = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn tone(rate: u32, secs: f64, f: f64) -> Vec<f64> {
        let n = (rate as f64 * secs) as usize;
        (0..n)
            .flat_map(|i| {
                let v = 0.5 * (2.0 * PI * f * i as f64 / rate as f64).sin();
                [v, -v]
            })
            .collect()
    }

    /// 출력에서 사인 성분(최소제곱으로 맞춘 것)을 빼고 남은 것의 크기(dB, 신호 대비).
    fn residual_db(x: &[f64], rate: u32, f: f64, skip: usize) -> f64 {
        let l: Vec<f64> = x.as_chunks::<2>().0.iter().map(|c| c[0]).skip(skip).collect();
        let l = &l[..l.len() - skip];
        let w = |i: usize| 2.0 * PI * f * i as f64 / rate as f64;
        let (mut ss, mut sc, mut cc, mut xs, mut xc) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (i, v) in l.iter().enumerate() {
            let (s, c) = w(i).sin_cos();
            ss += s * s;
            sc += s * c;
            cc += c * c;
            xs += v * s;
            xc += v * c;
        }
        let det = ss * cc - sc * sc;
        let a = (xs * cc - xc * sc) / det;
        let b = (xc * ss - xs * sc) / det;
        let n = l.len() as f64;
        let err: f64 = l
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let (s, c) = w(i).sin_cos();
                (v - a * s - b * c).powi(2)
            })
            .sum::<f64>()
            / n;
        let amp = (a * a + b * b).sqrt();
        10.0 * err.log10() - 20.0 * (amp / 2f64.sqrt()).log10()
    }

    #[test]
    fn ratios_and_lengths() {
        for (a, b) in [(44_100, 48_000), (96_000, 48_000), (48_000, 44_100), (176_400, 48_000), (44_100, 88_200)] {
            let mut r = Resampler::new(a, b, 2, Quality { bits: 20, filter: Filter::Precise });
            let input = tone(a, 1.0, 1000.0);
            let mut out = Vec::new();
            // 조각조각 넣어도 한 번에 넣은 것과 같아야 한다.
            for chunk in input.chunks(2 * 997) {
                r.process(chunk, &mut out);
            }
            r.flush(&mut out);
            let frames = out.len() / 2;
            assert!((frames as i64 - b as i64).abs() <= 1, "{a}->{b}: {frames}");
        }
    }

    #[test]
    fn clean_1k_tone() {
        for (a, b) in [(44_100u32, 48_000u32), (96_000, 44_100), (48_000, 96_000)] {
            let mut r = Resampler::new(a, b, 2, Quality::default());
            let mut out = Vec::new();
            r.process(&tone(a, 0.5, 1000.0), &mut out);
            let db = residual_db(&out, b, 1000.0, r.taps() * 2);
            assert!(db < -140.0, "{a}->{b}: 잔여 {db:.1} dB");
        }
    }

    #[test]
    fn output_is_time_aligned() {
        // 출력 0번이 입력 0번과 같은 시각이어야 한다(지연 보정). 느린 사인의 위상으로 본다.
        let a = 44_100;
        let b = 48_000;
        let mut r = Resampler::new(a, b, 2, Quality::default());
        let mut out = Vec::new();
        r.process(&tone(a, 0.5, 100.0), &mut out);
        let mid = 12_000;
        let want = 0.5 * (2.0 * PI * 100.0 * mid as f64 / b as f64).sin();
        assert!((out[mid * 2] - want).abs() < 1e-6, "{} vs {want}", out[mid * 2]);
    }

    #[test]
    fn chunked_equals_whole() {
        let input = tone(44_100, 0.3, 997.0);
        let mut a = Resampler::new(44_100, 48_000, 2, Quality::default());
        let mut b = Resampler::new(44_100, 48_000, 2, Quality::default());
        let (mut oa, mut ob) = (Vec::new(), Vec::new());
        a.process(&input, &mut oa);
        for c in input.chunks(2 * 31) {
            b.process(c, &mut ob);
        }
        assert_eq!(oa.len(), ob.len());
        assert!(oa.iter().zip(&ob).all(|(x, y)| x == y));
    }
}
