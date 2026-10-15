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

/// ffmpeg로 푼 32비트 float PCM(48kHz, 인터리브)
fn ffmpeg_f32(path: &Path) -> Vec<f64> {
    let out = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "f32le", "-acodec", "pcm_f32le", "-"])
        .output()
        .unwrap();
    assert!(out.status.success(), "ffmpeg");
    out.stdout.chunks_exact(4).map(|b| f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))).collect()
}

fn snr_db(ours: &[f64], theirs: &[f64]) -> f64 {
    let (mut sig, mut err) = (0.0, 0.0);
    for (a, b) in ours.iter().zip(theirs) {
        sig += b * b;
        err += (a - b) * (a - b);
    }
    10.0 * (sig / err.max(1e-30)).log10()
}

/// 유튜브에서 받은 음악에 흔한 Opus(.opus, .webm): ffmpeg(libopus)와 같은 길이, 같은 소리.
#[test]
fn opus_decodes_like_ffmpeg() {
    if !common::has_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // 2.5초, 스테레오, 왼쪽 440Hz + 오른쪽 1.1kHz와 잡음 조금(SILK·CELT가 다 쓰이게 160kbps)
    for (name, extra) in [("a.opus", vec![]), ("a.webm", vec!["-f", "webm"])] {
        let p = dir.path().join(name);
        let st = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg("aevalsrc=0.4*sin(2*PI*440*t)+0.02*random(0)|0.4*sin(2*PI*1100*t):s=44100:d=2.5")
            .args(["-c:a", "libopus", "-b:a", "160k"])
            .args(&extra)
            .arg(&p)
            .status()
            .unwrap();
        if !st.success() {
            eprintln!("이 ffmpeg에는 libopus가 없습니다");
            return;
        }
        let (spec, ours) = decode::decode_all(&p).unwrap();
        assert_eq!((spec.rate, spec.channels), (48_000, 2), "{name}");
        let theirs = ffmpeg_f32(&p);
        if name.ends_with(".opus") {
            // Ogg: pre-skip과 끝 패딩을 잘라 원래 길이(2.5초 → 48kHz로 120000프레임) 그대로
            assert_eq!(ours.len(), theirs.len(), "{name}: 길이");
            assert_eq!(ours.len(), 120_000 * 2, "{name}: 원래 길이");
        } else {
            // WebM은 컨테이너가 끝 패딩을 늘 알려 주지는 않는다. 길이는 한 패킷(20ms) 안에서.
            assert!(ours.len().abs_diff(theirs.len()) <= 960 * 2 * 2, "{name}: 길이 {} vs {}", ours.len(), theirs.len());
        }
        let n = ours.len().min(theirs.len());
        let snr = snr_db(&ours[..n], &theirs[..n]);
        assert!(snr > 40.0, "{name}: ffmpeg와의 차이가 크다: SNR {snr:.1}dB");

        // 1초로 시크: 그 자리의 소리가 처음부터 푼 것의 그 자리와 같다(pre-skip만큼 밀린 시간축).
        let mut src = decode::open(&p).unwrap();
        let at = src.seek(48_000).unwrap();
        assert_eq!(at, 48_000, "{name}: 시크한 위치");
        let mut got = Vec::new();
        while got.len() < 9_600 * 2 && src.read(&mut got).unwrap() > 0 {}
        if name.ends_with(".opus") {
            // 디코더가 상태를 미리 쌓아 두므로(pre-roll) 시크한 첫 샘플부터 같은 소리다.
            let want = &ours[(48_000 * 2)..(48_000 * 2 + 9_600 * 2)];
            let snr = snr_db(&got[..9_600 * 2], want);
            assert!(snr > 60.0, "{name}: 시크한 자리의 소리가 다르다: SNR {snr:.1}dB");
        } else {
            // WebM의 시간은 밀리초로 반올림돼 적혀 있다: 2ms 안에 들어오면 된다.
            let best = (-240i64..=240)
                .map(|off| {
                    let from = ((48_000 + off) * 2) as usize;
                    (snr_db(&got[960..9_600 * 2], &ours[from + 960..from + 9_600 * 2]), off)
                })
                .fold((f64::MIN, 0), |a, b| if b.0 > a.0 { b } else { a });
            assert!(best.0 > 60.0 && best.1.abs() <= 96, "{name}: 시크한 자리가 2ms 넘게 어긋난다: {best:?}");
        }
    }
}
