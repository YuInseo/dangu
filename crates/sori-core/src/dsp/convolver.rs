//! 균일 분할 FFT 컨볼루션(overlap-save). 임펄스 응답이 수 초여도 블록 하나당 비용이 일정하다.
//!
//! 블록 B개씩 받아 2B짜리 FFT로 옮기고, 임펄스 응답을 B씩 자른 조각들의 스펙트럼과 주파수
//! 영역에서 곱해 더한다. 지연은 정확히 B 샘플이고, 체인이 그만큼 앞에서 버리고 끝에서 밀어낸다.

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

pub struct Convolver {
    block: usize,
    fwd: Arc<dyn RealToComplex<f64>>,
    inv: Arc<dyn ComplexToReal<f64>>,
    /// 채널마다 조각들의 스펙트럼: parts[c][p][bin]
    parts: Vec<Vec<Vec<Complex<f64>>>>,
    /// 주파수 영역 지연선: fdl[c][p] (가장 최근 입력이 head)
    fdl: Vec<Vec<Vec<Complex<f64>>>>,
    head: usize,
    /// 채널마다 직전 블록과 이번 블록(2B)
    window: Vec<Vec<f64>>,
    /// 모은 입력(인터리브), 낸 출력(인터리브)
    inbox: Vec<f64>,
    outbox: Vec<f64>,
    fill: usize,
    acc: Vec<Complex<f64>>,
    tmp_in: Vec<f64>,
    tmp_out: Vec<f64>,
    spec: Vec<Complex<f64>>,
}

impl Convolver {
    /// `ir[c]`: 채널마다의 임펄스 응답(같은 길이). 모노면 한 개를 두 채널에 쓴다.
    pub fn new(ir: &[Vec<f64>], block: usize) -> Self {
        let channels = 2;
        let n = 2 * block;
        let mut planner = RealFftPlanner::<f64>::new();
        let fwd = planner.plan_fft_forward(n);
        let inv = planner.plan_fft_inverse(n);
        let len = ir.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let count = len.div_ceil(block);
        let bins = block + 1;
        let mut parts = Vec::with_capacity(channels);
        for c in 0..channels {
            let h = &ir[c.min(ir.len() - 1)];
            let mut ps = Vec::with_capacity(count);
            for p in 0..count {
                let mut buf = vec![0.0; n];
                let start = p * block;
                let end = (start + block).min(h.len());
                if start < end {
                    buf[..end - start].copy_from_slice(&h[start..end]);
                }
                let mut out = vec![Complex::new(0.0, 0.0); bins];
                fwd.process(&mut buf, &mut out).expect("FFT");
                ps.push(out);
            }
            parts.push(ps);
        }
        Convolver {
            block,
            fwd,
            inv,
            parts,
            fdl: vec![vec![vec![Complex::new(0.0, 0.0); bins]; count]; channels],
            head: 0,
            window: vec![vec![0.0; n]; channels],
            inbox: vec![0.0; block * channels],
            outbox: vec![0.0; block * channels],
            fill: 0,
            acc: vec![Complex::new(0.0, 0.0); bins],
            tmp_in: vec![0.0; n],
            tmp_out: vec![0.0; n],
            spec: vec![Complex::new(0.0, 0.0); bins],
        }
    }

    /// 지연(샘플). 체인이 보정한다.
    pub fn latency(&self) -> usize {
        self.block
    }

    fn run_block(&mut self) {
        let b = self.block;
        let n = 2 * b;
        let count = self.parts[0].len();
        self.head = (self.head + count - 1) % count;
        let scale = 1.0 / n as f64;
        let mut out = vec![0.0; b * 2];
        for c in 0..2 {
            // 창: [직전 블록 | 이번 블록]
            let w = &mut self.window[c];
            w.copy_within(b.., 0);
            for i in 0..b {
                w[b + i] = self.inbox[i * 2 + c];
            }
            self.tmp_in.copy_from_slice(w);
            self.fwd.process(&mut self.tmp_in, &mut self.spec).expect("FFT");
            self.fdl[c][self.head].copy_from_slice(&self.spec);
            for a in &mut self.acc {
                *a = Complex::new(0.0, 0.0);
            }
            for p in 0..count {
                let x = &self.fdl[c][(self.head + p) % count];
                let h = &self.parts[c][p];
                for ((a, xv), hv) in self.acc.iter_mut().zip(x).zip(h) {
                    *a += xv * hv;
                }
            }
            // 실수 신호의 역변환: DC와 나이퀴스트 칸의 허수부는 0이어야 한다.
            self.acc[0].im = 0.0;
            let last = self.acc.len() - 1;
            self.acc[last].im = 0.0;
            let mut spec = self.acc.clone();
            self.inv.process(&mut spec, &mut self.tmp_out).expect("IFFT");
            for i in 0..b {
                out[i * 2 + c] = self.tmp_out[b + i] * scale;
            }
        }
        self.outbox.extend_from_slice(&out);
    }

    /// 스테레오 인터리브. 같은 길이를 돌려준다. 출력은 정확히 B 샘플 늦다
    /// (처음 B 샘플은 0으로 시작하는 출력 줄에서 나온다).
    pub fn process(&mut self, buf: &mut [f64]) {
        let b = self.block;
        let frames = buf.len() / 2;
        let mut i = 0;
        while i < frames {
            let take = (b - self.fill).min(frames - i);
            self.inbox[self.fill * 2..(self.fill + take) * 2].copy_from_slice(&buf[i * 2..(i + take) * 2]);
            self.fill += take;
            if self.fill == b {
                self.run_block();
                self.fill = 0;
            }
            buf[i * 2..(i + take) * 2].copy_from_slice(&self.outbox[..take * 2]);
            self.outbox.drain(..take * 2);
            i += take;
        }
    }

    pub fn reset(&mut self) {
        for c in &mut self.fdl {
            for p in c {
                p.fill(Complex::new(0.0, 0.0));
            }
        }
        for w in &mut self.window {
            w.fill(0.0);
        }
        self.inbox.fill(0.0);
        self.outbox.clear();
        self.outbox.resize(self.block * 2, 0.0);
        self.fill = 0;
    }
}

/// 임펄스 응답의 최대 이득(주파수 응답의 가장 큰 크기)을 0dB로 맞추는 배율.
pub fn peak_normalize(ir: &[Vec<f64>]) -> f64 {
    let len = ir.iter().map(Vec::len).max().unwrap_or(1);
    let n = (len * 2).next_power_of_two().max(4096);
    let mut planner = RealFftPlanner::<f64>::new();
    let fwd = planner.plan_fft_forward(n);
    let mut peak = 0.0f64;
    for h in ir {
        let mut buf = vec![0.0; n];
        buf[..h.len()].copy_from_slice(h);
        let mut out = fwd.make_output_vec();
        fwd.process(&mut buf, &mut out).expect("FFT");
        peak = out.iter().map(|c| c.norm()).fold(peak, f64::max);
    }
    if peak > 1e-12 { 1.0 / peak } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 시간 영역 직접 계산과 비교한다.
    #[test]
    fn matches_direct_convolution() {
        let ir_l: Vec<f64> = (0..3000).map(|i| ((i as f64 * 0.37).sin()) * (-(i as f64) / 600.0).exp()).collect();
        let ir_r: Vec<f64> = (0..3000).map(|i| ((i as f64 * 0.11).cos()) * (-(i as f64) / 300.0).exp()).collect();
        let x: Vec<f64> = (0..20_000).map(|i| ((i * 7919) % 1000) as f64 / 500.0 - 1.0).collect();
        let mut buf = x.clone();
        let mut cv = Convolver::new(&[ir_l.clone(), ir_r.clone()], 512);
        // 조각조각 넣는다
        for chunk in buf.chunks_mut(2 * 333) {
            cv.process(chunk);
        }
        let lat = cv.latency();
        for (c, ir) in [(0usize, &ir_l), (1, &ir_r)] {
            for n in [lat, lat + 1, 4000, 9000, 9999] {
                if n >= x.len() / 2 {
                    continue;
                }
                let m = n - lat; // 출력 n번 = 입력 n-B번에서의 컨볼루션
                let mut want = 0.0;
                for (k, h) in ir.iter().enumerate() {
                    if k > m {
                        break;
                    }
                    want += h * x[(m - k) * 2 + c];
                }
                assert!((buf[n * 2 + c] - want).abs() < 1e-9, "채널 {c}, {n}: {} vs {want}", buf[n * 2 + c]);
            }
        }
    }

    #[test]
    fn dirac_is_identity_after_latency() {
        let mut ir = vec![0.0; 100];
        ir[0] = 1.0;
        let mut cv = Convolver::new(&[ir], 256);
        let x: Vec<f64> = (0..4096).map(|i| (i as f64 * 0.01).sin()).collect();
        let mut buf = x.clone();
        cv.process(&mut buf);
        for n in 300..2000 {
            assert!((buf[n * 2] - x[(n - 256) * 2]).abs() < 1e-12);
        }
        assert!((peak_normalize(&[vec![0.5, 0.0, 0.0]]) - 2.0).abs() < 1e-9);
    }
}
