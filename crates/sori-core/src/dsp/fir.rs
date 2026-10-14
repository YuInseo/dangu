//! 카이저 창 FIR 설계와 정수배 데시메이터. DSD→PCM 변환과 리샘플러가 같이 쓴다.

use std::f64::consts::PI;

/// 0차 수정 베셀 함수 I₀. 급수가 충분히 작아질 때까지 더한다.
pub fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let half = x / 2.0;
    for k in 1..200 {
        term *= (half / k as f64) * (half / k as f64);
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

/// 원하는 저지대역 감쇠(dB)에 맞는 카이저 β.
pub fn kaiser_beta(atten_db: f64) -> f64 {
    if atten_db > 50.0 {
        0.1102 * (atten_db - 8.7)
    } else if atten_db >= 21.0 {
        0.5842 * (atten_db - 21.0).powf(0.4) + 0.07886 * (atten_db - 21.0)
    } else {
        0.0
    }
}

/// 전이 대역 폭(Hz)과 감쇠(dB)를 맞추는 데 필요한 탭 수(홀수로 올림).
pub fn kaiser_taps(atten_db: f64, transition_hz: f64, fs: f64) -> usize {
    let n = ((atten_db - 7.95) * fs / (2.285 * 2.0 * PI * transition_hz)).ceil() as usize + 1;
    n | 1
}

/// 차단 주파수 `cutoff_hz`(-6dB 지점)의 선형 위상 저역 통과 필터. 계수 합은 `gain`.
pub fn lowpass(taps: usize, cutoff_hz: f64, fs: f64, beta: f64, gain: f64) -> Vec<f64> {
    let fc = cutoff_hz / fs;
    let m = (taps - 1) as f64 / 2.0;
    let i0b = bessel_i0(beta);
    let mut h: Vec<f64> = (0..taps)
        .map(|n| {
            let x = n as f64 - m;
            let sinc = if x == 0.0 { 2.0 * fc } else { (2.0 * PI * fc * x).sin() / (PI * x) };
            let r = if m == 0.0 { 0.0 } else { x / m };
            let w = bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0b;
            sinc * w
        })
        .collect();
    let sum: f64 = h.iter().sum();
    for v in &mut h {
        *v *= gain / sum;
    }
    h
}

/// 주파수 응답의 크기(dB). 테스트와 그래프에 쓴다.
pub fn response_db(h: &[f64], f: f64, fs: f64) -> f64 {
    let w = 2.0 * PI * f / fs;
    let (mut re, mut im) = (0.0, 0.0);
    for (n, &c) in h.iter().enumerate() {
        re += c * (w * n as f64).cos();
        im -= c * (w * n as f64).sin();
    }
    20.0 * (re * re + im * im).sqrt().max(1e-300).log10()
}

/// 정수배로 줄이는 FIR. 채널마다 기록을 두 배 길이 버퍼에 써서 순환 없이 내적한다.
pub struct Decimator {
    taps: Vec<f64>,
    factor: usize,
    channels: usize,
    hist: Vec<Vec<f64>>,
    pos: usize,
    phase: usize,
}

impl Decimator {
    pub fn new(taps: Vec<f64>, factor: usize, channels: usize) -> Self {
        let n = taps.len();
        // 계수를 뒤집어 두면 기록(오래된 것 → 새것)과 바로 내적할 수 있다.
        let mut taps = taps;
        taps.reverse();
        Decimator { taps, factor, channels, hist: vec![vec![0.0; 2 * n]; channels], pos: 0, phase: 0 }
    }

    pub fn reset(&mut self) {
        for h in &mut self.hist {
            h.fill(0.0);
        }
        self.pos = 0;
        self.phase = 0;
    }

    /// 필터의 그룹 지연(출력 샘플 단위).
    pub fn delay_out(&self) -> f64 {
        (self.taps.len() - 1) as f64 / 2.0 / self.factor as f64
    }

    /// 인터리브된 입력 → 인터리브된 출력(뒤에 붙인다).
    pub fn process(&mut self, input: &[f64], out: &mut Vec<f64>) {
        let n = self.taps.len();
        let ch = self.channels;
        for frame in input.chunks_exact(ch) {
            for (c, &x) in frame.iter().enumerate() {
                let h = &mut self.hist[c];
                h[self.pos] = x;
                h[self.pos + n] = x;
            }
            self.pos = (self.pos + 1) % n;
            self.phase += 1;
            if self.phase == self.factor {
                self.phase = 0;
                for c in 0..ch {
                    let h = &self.hist[c][self.pos..self.pos + n];
                    let mut acc = 0.0;
                    for (a, b) in h.iter().zip(&self.taps) {
                        acc += a * b;
                    }
                    out.push(acc);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowpass_meets_spec() {
        let fs = 352_800.0;
        let taps = kaiser_taps(120.0, 30_000.0, fs);
        let h = lowpass(taps, 45_000.0, fs, kaiser_beta(120.0), 1.0);
        assert!(response_db(&h, 1_000.0, fs).abs() < 0.001);
        assert!(response_db(&h, 20_000.0, fs).abs() < 0.001);
        assert!(response_db(&h, 30_000.0, fs).abs() < 0.01, "{}", response_db(&h, 30_000.0, fs));
        for f in [60_000.0, 80_000.0, 120_000.0, 170_000.0] {
            assert!(response_db(&h, f, fs) < -115.0, "{f}: {}", response_db(&h, f, fs));
        }
    }

    #[test]
    fn decimator_passes_dc() {
        let h = lowpass(31, 10_000.0, 96_000.0, kaiser_beta(100.0), 1.0);
        let mut d = Decimator::new(h, 2, 2);
        let input = vec![0.5; 2 * 400];
        let mut out = Vec::new();
        d.process(&input, &mut out);
        assert_eq!(out.len(), 400);
        assert!((out[out.len() - 1] - 0.5).abs() < 1e-12);
    }
}
