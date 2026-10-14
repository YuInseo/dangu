//! 바이쿼드 필터. RBJ "Audio EQ Cookbook" 공식 그대로(화면의 곡선과 같은 식).

use crate::dsp::request::{Band, BandType};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

/// a0으로 나눠 둔 계수.
pub fn coeffs(band: &Band, fs: f64) -> Coeffs {
    let f = band.freq.clamp(1.0, fs / 2.0 - 1.0);
    let w0 = 2.0 * PI * f / fs;
    let (sin, cos) = w0.sin_cos();
    let q = band.q.max(0.01);
    let alpha = sin / (2.0 * q);
    let a = 10f64.powf(band.gain / 40.0);
    let (b0, b1, b2, a0, a1, a2) = match band.kind {
        BandType::Peak => (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a),
        BandType::Lowshelf => {
            let beta = 2.0 * a.sqrt() * alpha;
            (
                a * (a + 1.0 - (a - 1.0) * cos + beta),
                2.0 * a * (a - 1.0 - (a + 1.0) * cos),
                a * (a + 1.0 - (a - 1.0) * cos - beta),
                a + 1.0 + (a - 1.0) * cos + beta,
                -2.0 * (a - 1.0 + (a + 1.0) * cos),
                a + 1.0 + (a - 1.0) * cos - beta,
            )
        }
        BandType::Highshelf => {
            let beta = 2.0 * a.sqrt() * alpha;
            (
                a * (a + 1.0 + (a - 1.0) * cos + beta),
                -2.0 * a * (a - 1.0 + (a + 1.0) * cos),
                a * (a + 1.0 + (a - 1.0) * cos - beta),
                a + 1.0 - (a - 1.0) * cos + beta,
                2.0 * (a - 1.0 - (a + 1.0) * cos),
                a + 1.0 - (a - 1.0) * cos - beta,
            )
        }
        BandType::Lowpass => ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        BandType::Highpass => ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        BandType::Bandpass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
        BandType::Notch => (1.0, -2.0 * cos, 1.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
    };
    Coeffs { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
}

/// 주파수 f에서의 크기(dB).
pub fn magnitude_db(c: &Coeffs, f: f64, fs: f64) -> f64 {
    let w = 2.0 * PI * f / fs;
    let (s1, c1) = w.sin_cos();
    let (s2, c2) = (2.0 * w).sin_cos();
    let nr = c.b0 + c.b1 * c1 + c.b2 * c2;
    let ni = -(c.b1 * s1 + c.b2 * s2);
    let dr = 1.0 + c.a1 * c1 + c.a2 * c2;
    let di = -(c.a1 * s1 + c.a2 * s2);
    10.0 * ((nr * nr + ni * ni).max(1e-300) / (dr * dr + di * di).max(1e-300)).log10()
}

/// 전치 직접형 II(TDF-II). 상태는 채널마다 따로.
#[derive(Clone, Copy, Debug, Default)]
pub struct State {
    z1: f64,
    z2: f64,
}

impl State {
    #[inline]
    pub fn run(&mut self, c: &Coeffs, x: f64) -> f64 {
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band(kind: BandType, freq: f64, gain: f64, q: f64) -> Band {
        Band { kind, freq, gain, q, on: true }
    }

    #[test]
    fn responses() {
        let fs = 48_000.0;
        let p = coeffs(&band(BandType::Peak, 1000.0, 6.0, 1.0), fs);
        assert!((magnitude_db(&p, 1000.0, fs) - 6.0).abs() < 1e-9);
        assert!(magnitude_db(&p, 20.0, fs).abs() < 0.05);
        let lp = coeffs(&band(BandType::Lowpass, 1000.0, 0.0, std::f64::consts::FRAC_1_SQRT_2), fs);
        assert!((magnitude_db(&lp, 1000.0, fs) + 3.0103).abs() < 0.01);
        let ls = coeffs(&band(BandType::Lowshelf, 100.0, 4.0, 0.7), fs);
        assert!((magnitude_db(&ls, 10.0, fs) - 4.0).abs() < 0.05);
    }

    #[test]
    fn filtering_a_tone_matches_the_curve() {
        let fs = 48_000.0;
        let c = coeffs(&band(BandType::Peak, 3000.0, -9.0, 2.0), fs);
        let mut st = State::default();
        let n = 48_000;
        let mut peak = 0.0f64;
        for i in 0..n {
            let x = (2.0 * PI * 3000.0 * i as f64 / fs).sin();
            let y = st.run(&c, x);
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        assert!((20.0 * peak.log10() + 9.0).abs() < 0.01, "{}", 20.0 * peak.log10());
    }
}
