//! 백그라운드 오디오 분석.
//!
//! 곡마다 한 번 끝까지 풀어서 EBU R128 통합 라우드니스, 트루 피크, 라우드니스 범위,
//! 그리고 진행 막대에 그릴 파형을 얻는다. 볼륨 레벨링은 이 값으로 게인을 정하고
//! 트루 피크로 클리핑을 막는다.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ebur128::{EbuR128, Mode};
use serde::{Deserialize, Serialize};

use crate::decode;

pub const WAVE_BINS: usize = 200;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalysisEntry {
    /// 분석한 파일의 수정 시각. 바뀌면 다시 분석한다.
    pub m: i64,
    /// 통합 라우드니스, LUFS
    pub i: f64,
    /// 트루 피크, dBTP
    pub p: f64,
    /// 라우드니스 범위, LU
    pub r: f64,
    /// 파형: WAVE_BINS개의 0–255, base64
    pub w: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AnalysisFile {
    pub v: u32,
    pub tracks: HashMap<String, AnalysisEntry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Measured {
    pub integrated: f64,
    pub true_peak: f64,
    pub lra: f64,
    pub waveform: Vec<u8>,
}

/// 구간 에너지 → 0–255. 조용한 곳도 보이게 dB 눈금(-45dB ~ 0dB)으로 편다.
pub fn waveform_from_energy(sums: &[(f64, u64)], bins: usize) -> Vec<u8> {
    let n = sums.len();
    let mut out = vec![0u8; bins];
    if n == 0 {
        return out;
    }
    for (b, slot) in out.iter_mut().enumerate() {
        let s = b * n / bins;
        let e = ((b + 1) * n / bins).max(s + 1).min(n);
        let (mut sum, mut count) = (0.0, 0u64);
        for &(es, c) in &sums[s..e] {
            sum += es;
            count += c;
        }
        let rms = if count == 0 { 0.0 } else { (sum / count as f64).sqrt() };
        let db = 20.0 * rms.max(1e-6).log10();
        *slot = (((db + 45.0) / 45.0).clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    out
}

pub fn analyze_file(path: &Path, cancel: &AtomicBool) -> anyhow::Result<Measured> {
    let mut src = decode::open(path)?;
    let spec = src.spec().clone();
    let ch = spec.channels;
    let mut meter = EbuR128::new(ch as u32, spec.rate, Mode::I | Mode::LRA | Mode::TRUE_PEAK)
        .map_err(|e| anyhow::anyhow!("라우드니스 측정기를 만들 수 없습니다: {e:?}"))?;
    // 20ms 창마다 모노 에너지를 모았다가 끝에서 200칸으로 묶는다. 길이를 몰라도 된다.
    let window = (spec.rate as usize / 50).max(1);
    let mut sums: Vec<(f64, u64)> = Vec::new();
    let (mut acc, mut cnt) = (0.0f64, 0u64);
    let mut buf = Vec::with_capacity(1 << 16);
    loop {
        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!("중단됨");
        }
        buf.clear();
        // 패킷 몇 개를 모아 측정기에 넘긴다(호출 횟수를 줄인다).
        let mut frames = 0;
        while buf.len() < 1 << 15 {
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            frames += n;
        }
        if frames == 0 {
            break;
        }
        meter.add_frames_f64(&buf).map_err(|e| anyhow::anyhow!("측정 실패: {e:?}"))?;
        for f in buf.chunks_exact(ch) {
            let m = f.iter().sum::<f64>() / ch as f64;
            acc += m * m;
            cnt += 1;
            if cnt as usize == window {
                sums.push((acc, cnt));
                acc = 0.0;
                cnt = 0;
            }
        }
    }
    if cnt > 0 {
        sums.push((acc, cnt));
    }
    let integrated = meter.loudness_global().unwrap_or(f64::NEG_INFINITY);
    let lra = meter.loudness_range().unwrap_or(0.0);
    let peak = (0..ch as u32).filter_map(|c| meter.true_peak(c).ok()).fold(0.0f64, f64::max);
    Ok(Measured {
        // 무음은 -70 LUFS(게이트 바닥), 피크는 -120dB로 적는다.
        integrated: if integrated.is_finite() { (integrated * 10.0).round() / 10.0 } else { -70.0 },
        true_peak: if peak > 0.0 { (20.0 * peak.log10() * 10.0).round() / 10.0 } else { -120.0 },
        lra: if lra.is_finite() { (lra * 10.0).round() / 10.0 } else { 0.0 },
        waveform: waveform_from_energy(&sums, WAVE_BINS),
    })
}

pub fn to_entry(mtime: i64, m: &Measured) -> AnalysisEntry {
    AnalysisEntry { m: mtime, i: m.integrated, p: m.true_peak, r: m.lra, w: STANDARD.encode(&m.waveform) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_scale() {
        // 0dBFS 사인파의 RMS는 -3dB → (42/45)*255 ≈ 238
        let full: Vec<(f64, u64)> = vec![(0.5 * 100.0, 100); 400];
        let w = waveform_from_energy(&full, 200);
        assert_eq!(w.len(), 200);
        assert!((236..=240).contains(&w[10]), "{}", w[10]);
        let silent: Vec<(f64, u64)> = vec![(0.0, 100); 10];
        assert!(waveform_from_energy(&silent, 200).iter().all(|&v| v == 0));
    }
}
