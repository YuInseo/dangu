//! DSD → PCM: 시그마-델타로 만든 1kHz 사인을 DSF로 써서 다시 풀어 본다.

use std::io::Write;

use sori_core::decode::{self, DSD_PCM_RATE};

/// 2차 시그마-델타 변조기. 비트 1 = +1, 0 = -1.
fn modulate(x: &[f64]) -> Vec<u8> {
    let (mut v1, mut v2) = (0.0f64, 0.0f64);
    let mut bits = Vec::with_capacity(x.len());
    let mut y = 1.0;
    for &s in x {
        v1 += s - y;
        v2 += v1 - y;
        y = if v2 >= 0.0 { 1.0 } else { -1.0 };
        bits.push(if y > 0.0 { 1u8 } else { 0 });
    }
    bits
}

fn write_dsf(path: &std::path::Path, rate: u32, channels: &[Vec<u8>]) {
    let samples = channels[0].len() as u64;
    let block = 4096usize;
    let bytes_per_ch = samples.div_ceil(8) as usize;
    let blocks = bytes_per_ch.div_ceil(block);
    // 채널마다 LSB가 먼저 오게 바이트로 묶는다.
    let packed: Vec<Vec<u8>> = channels
        .iter()
        .map(|bits| {
            let mut v = vec![0u8; blocks * block];
            for (i, &b) in bits.iter().enumerate() {
                v[i / 8] |= b << (i % 8);
            }
            v
        })
        .collect();
    let mut data = Vec::with_capacity(blocks * block * channels.len());
    for k in 0..blocks {
        for ch in &packed {
            data.extend_from_slice(&ch[k * block..(k + 1) * block]);
        }
    }
    let total = 28 + 52 + 12 + data.len() as u64;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"DSD ").unwrap();
    f.write_all(&28u64.to_le_bytes()).unwrap();
    f.write_all(&total.to_le_bytes()).unwrap();
    f.write_all(&0u64.to_le_bytes()).unwrap();
    f.write_all(b"fmt ").unwrap();
    f.write_all(&52u64.to_le_bytes()).unwrap();
    for v in [1u32, 0, 2, channels.len() as u32, rate, 1] {
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    f.write_all(&samples.to_le_bytes()).unwrap();
    f.write_all(&(block as u32).to_le_bytes()).unwrap();
    f.write_all(&0u32.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&(12 + data.len() as u64).to_le_bytes()).unwrap();
    f.write_all(&data).unwrap();
}

fn tone(rate: u32, secs: f64, freq: f64, amp: f64) -> Vec<f64> {
    let n = (rate as f64 * secs) as usize;
    (0..n).map(|i| amp * (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin()).collect()
}

/// 1kHz 성분의 크기와 위상(사인 기준).
fn component(x: &[f64], rate: f64, freq: f64) -> (f64, f64) {
    let (mut s, mut c) = (0.0, 0.0);
    for (i, &v) in x.iter().enumerate() {
        let w = 2.0 * std::f64::consts::PI * freq * i as f64 / rate;
        s += v * w.sin();
        c += v * w.cos();
    }
    let n = x.len() as f64;
    ((s * s + c * c).sqrt() * 2.0 / n, c.atan2(s))
}

#[test]
fn dsd64_and_dsd128_become_clean_pcm() {
    for rate in [2_822_400u32, 5_644_800] {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("tone.dsf");
        let left = modulate(&tone(rate, 1.0, 1000.0, 0.25));
        let right = modulate(&tone(rate, 1.0, 1000.0, -0.25));
        write_dsf(&p, rate, &[left, right]);

        let (spec, pcm) = decode::decode_all(&p).unwrap();
        assert_eq!(spec.rate, DSD_PCM_RATE);
        assert_eq!(spec.channels, 2);
        assert_eq!(spec.dsd_rate, Some(rate));
        let frames = pcm.len() / 2;
        let expect = (rate / DSD_PCM_RATE) as usize;
        let want = rate as usize / expect;
        assert!(frames.abs_diff(want) <= 200, "프레임 {frames} vs {want}");

        // 가운데 0.5초: 25% 변조 → +6dB → 진폭 0.5, 위상은 원래 신호와 맞아야 한다(지연 보정).
        let l: Vec<f64> = pcm.as_chunks::<2>().0.iter().map(|f| f[0]).skip(44_100).take(88_200).collect();
        let r: Vec<f64> = pcm.as_chunks::<2>().0.iter().map(|f| f[1]).skip(44_100).take(88_200).collect();
        let (amp, phase) = component(&l, DSD_PCM_RATE as f64, 1000.0);
        assert!((amp - 0.5).abs() < 0.01, "{rate}: 진폭 {amp}");
        // 0.25초 지점(44_100 프레임)에서 시작했으니 사인의 위상은 2π·1000·0.25 = 0(mod 2π)
        assert!(phase.abs() < 0.01, "{rate}: 위상 {phase} rad (지연 보정이 어긋남)");
        let (amp_r, _) = component(&r, DSD_PCM_RATE as f64, 1000.0);
        assert!((amp_r - 0.5).abs() < 0.01);

        // 가청 대역(20kHz 아래)에 남은 것(잡음+왜곡)이 -60dB 아래. 시험용 2차 변조기의 잡음은
        // 20kHz 위에서 가파르게 커지므로 거기까지 재면 변환기가 아니라 변조기를 재게 된다.
        let resid: Vec<f64> = l
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let w = 2.0 * std::f64::consts::PI * 1000.0 * i as f64 / DSD_PCM_RATE as f64;
                v - amp * (w + phase).sin()
            })
            .collect();
        use sori_core::dsp::fir::{kaiser_beta, kaiser_taps, lowpass};
        let fs = DSD_PCM_RATE as f64;
        let h = lowpass(kaiser_taps(100.0, 4_000.0, fs), 22_000.0, fs, kaiser_beta(100.0), 1.0);
        let inband: Vec<f64> = (h.len()..resid.len()).map(|n| h.iter().enumerate().map(|(k, c)| c * resid[n - k]).sum()).collect();
        let power = inband.iter().map(|v| v * v).sum::<f64>() / inband.len() as f64;
        let resid_db = 10.0 * power.log10() - 20.0 * (amp / 2f64.sqrt()).log10();
        assert!(resid_db < -60.0, "{rate}: 잔여 {resid_db:.1} dB");

        // 시크: 0.5초로 가면 그 자리의 값이 처음부터 푼 것과 같아야 한다(필터가 데워진 뒤).
        let mut src = decode::open(&p).unwrap();
        src.seek(88_200).unwrap();
        let mut got = Vec::new();
        while got.len() < 4000 {
            if src.read(&mut got).unwrap() == 0 {
                break;
            }
        }
        let want = &pcm[88_200 * 2..88_200 * 2 + 4000];
        let err = got[..4000].iter().zip(want).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        assert!(err < 1e-6, "{rate}: 시크 뒤 차이 {err}");
    }
}
