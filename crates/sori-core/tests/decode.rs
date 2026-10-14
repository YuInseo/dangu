//! 디코딩이 원본과 비트 단위로 같은지, 시크가 샘플 단위로 정확한지.
mod common;

use std::path::Path;

use sori_core::decode;

fn first(dir: &Path, sub: &str) -> std::path::PathBuf {
    let d = dir.join(sub);
    let mut v: Vec<_> =
        std::fs::read_dir(&d).unwrap().flatten().map(|e| e.path()).filter(|p| !p.to_string_lossy().ends_with(".jpg")).collect();
    v.sort();
    v.remove(0)
}

fn assert_bit_exact(path: &Path) {
    let (spec, ours) = decode::decode_all(path).unwrap();
    let theirs = common::ffmpeg_s32(path);
    assert_eq!(ours.len(), theirs.len(), "{}: 샘플 수", path.display());
    let mut diff = 0usize;
    for (a, b) in ours.iter().zip(&theirs) {
        // f64 → 32비트 정수: 2^31을 곱하면 정확히 원래 정수로 돌아와야 한다.
        let ai = (a * 2_147_483_648.0) as i64;
        if ai != *b as i64 {
            diff += 1;
        }
    }
    assert_eq!(diff, 0, "{}: {}개 샘플이 다르다 ({spec:?})", path.display(), diff);
}

#[test]
fn lossless_formats_are_bit_exact() {
    let (Some(s), true) = (common::samples(), common::has_ffmpeg()) else { return };
    assert_bit_exact(&first(&s, "Neon Harbor/2023 - Night Drive")); // FLAC 24/96
    assert_bit_exact(&first(&s, "Mira Sol/Glass Garden")); // ALAC 24/48
    assert_bit_exact(&first(&s, "Neon Harbor/2019 - Paper Moon")); // AIFF
    assert_bit_exact(&first(&s, "Field Notes - Rain Studies")); // WAV
    assert_bit_exact(&first(&s, "하늘 사중주/아침의 차")); // FLAC 16/44.1
}

#[test]
fn mp3_is_gapless_and_close_to_ffmpeg() {
    let (Some(s), true) = (common::samples(), common::has_ffmpeg()) else { return };
    let p = first(&s, "Cassette Club/Lo-Fi Sketches");
    let (spec, ours) = decode::decode_all(&p).unwrap();
    let theirs = common::ffmpeg_f64(&p);
    // 인코더 지연과 패딩을 둘 다 잘라 내므로 길이가 같아야 한다.
    let frames_ours = ours.len() / spec.channels;
    let frames_theirs = theirs.len() / spec.channels;
    assert!((frames_ours as i64 - frames_theirs as i64).abs() <= 1, "{frames_ours} vs {frames_theirs}");
    let n = ours.len().min(theirs.len());
    let (mut sig, mut err) = (0.0, 0.0);
    for i in 0..n {
        sig += theirs[i] * theirs[i];
        err += (ours[i] - theirs[i]).powi(2);
    }
    let snr = 10.0 * (sig / err.max(1e-30)).log10();
    assert!(snr > 90.0, "MP3 디코더 차이가 크다: {snr:.1} dB");
}

#[test]
fn seeking_is_sample_accurate() {
    let Some(s) = common::samples() else { return };
    for sub in ["Neon Harbor/2023 - Night Drive", "Mira Sol/Glass Garden", "Field Notes - Rain Studies", "Neon Harbor/2019 - Paper Moon"] {
        let p = first(&s, sub);
        let (spec, all) = decode::decode_all(&p).unwrap();
        let ch = spec.channels;
        for target in [0u64, 1, 4_095, 4_096, 100_001, (all.len() / ch / 2) as u64] {
            let mut src = decode::open(&p).unwrap();
            let at = src.seek(target).unwrap();
            assert_eq!(at, target);
            let mut got = Vec::new();
            while got.len() < 2048 * ch {
                if src.read(&mut got).unwrap() == 0 {
                    break;
                }
            }
            let want = &all[target as usize * ch..(target as usize * ch + 2048 * ch).min(all.len())];
            assert_eq!(&got[..want.len()], want, "{sub} @ {target}");
        }
    }
}
