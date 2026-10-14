//! Bauer 스테레오→바이노럴(bs2b) 크로스피드. libbs2b와 같은 식이다.
//!
//! 반대쪽 채널을 저역 통과(1차)로 거르고 줄여 섞고, 같은 쪽은 고역을 살짝 올려 둘을 더한다.
//! 프리셋: 기본(700Hz, 4.5dB), Chu Moy(700Hz, 6dB), Jan Meier(650Hz, 9.5dB).

use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct Bs2b {
    a0_lo: f64,
    b1_lo: f64,
    a0_hi: f64,
    a1_hi: f64,
    b1_hi: f64,
    gain: f64,
    lo: [f64; 2],
    hi: [f64; 2],
    asis: [f64; 2],
}

impl Bs2b {
    /// `fcut`: 차단 주파수(Hz, 300–2000), `feed`: 섞는 양(dB, 1–15)
    pub fn new(fcut: f64, feed: f64, rate: f64) -> Self {
        let fc_lo = fcut.clamp(300.0, 2000.0);
        let level = feed.clamp(1.0, 15.0);
        let gb_lo = level * -5.0 / 6.0 - 3.0;
        let gb_hi = level / 6.0 - 3.0;
        let g_lo = 10f64.powf(gb_lo / 20.0);
        let g_hi = 1.0 - 10f64.powf(gb_hi / 20.0);
        let fc_hi = fc_lo * 2f64.powf((gb_lo - 20.0 * g_hi.log10()) / 12.0);
        let x = (-2.0 * PI * fc_lo / rate).exp();
        let (b1_lo, a0_lo) = (x, g_lo * (1.0 - x));
        let x = (-2.0 * PI * fc_hi / rate).exp();
        let (b1_hi, a0_hi, a1_hi) = (x, 1.0 - g_hi * (1.0 - x), -x);
        Bs2b { a0_lo, b1_lo, a0_hi, a1_hi, b1_hi, gain: 1.0 / (1.0 - g_hi + g_lo), lo: [0.0; 2], hi: [0.0; 2], asis: [0.0; 2] }
    }

    pub fn preset(name: &str) -> (f64, f64) {
        match name {
            "cmoy" => (700.0, 6.0),
            "jmeier" => (650.0, 9.5),
            _ => (700.0, 4.5),
        }
    }

    /// 스테레오 인터리브
    pub fn process(&mut self, buf: &mut [f64]) {
        for f in buf.chunks_exact_mut(2) {
            for (c, &x) in f.iter().enumerate() {
                self.lo[c] = self.a0_lo * x + self.b1_lo * self.lo[c];
                self.hi[c] = self.a0_hi * x + self.a1_hi * self.asis[c] + self.b1_hi * self.hi[c];
                self.asis[c] = x;
            }
            f[0] = (self.hi[0] + self.lo[1]) * self.gain;
            f[1] = (self.hi[1] + self.lo[0]) * self.gain;
        }
    }

    pub fn reset(&mut self) {
        self.lo = [0.0; 2];
        self.hi = [0.0; 2];
        self.asis = [0.0; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_passes_at_unity_in_the_bass() {
        // 두 채널이 같은 저음(모노)이면 섞어도 크기가 거의 그대로여야 한다.
        let mut x = Bs2b::new(700.0, 4.5, 48_000.0);
        let mut buf: Vec<f64> = (0..96_000)
            .flat_map(|i| {
                let v = (2.0 * PI * 50.0 * i as f64 / 48_000.0).sin() * 0.5;
                [v, v]
            })
            .collect();
        x.process(&mut buf);
        let peak = buf[48_000..].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        assert!((20.0 * (peak / 0.5).log10()).abs() < 0.3, "{}", 20.0 * (peak / 0.5).log10());
    }

    #[test]
    fn hard_left_bleeds_into_right() {
        let mut x = Bs2b::new(650.0, 9.5, 48_000.0);
        let mut buf: Vec<f64> = (0..48_000).flat_map(|i| [(2.0 * PI * 200.0 * i as f64 / 48_000.0).sin(), 0.0]).collect();
        x.process(&mut buf);
        let right = buf[24_000..].chunks(2).fold(0.0f64, |m, f| m.max(f[1].abs()));
        assert!(right > 0.05, "오른쪽으로 새어 들어가야 한다: {right}");
    }
}
