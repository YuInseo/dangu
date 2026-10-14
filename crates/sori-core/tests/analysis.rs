//! 라우드니스 분석이 ffmpeg의 ebur128과 같은 값을 내는지.
mod common;

use std::process::Command;
use std::sync::atomic::AtomicBool;

fn ffmpeg_loudness(path: &std::path::Path) -> (f64, f64) {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(path)
        .args(["-filter_complex", "ebur128=peak=true:framelog=quiet", "-f", "null", "-"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    let summary = &err[err.rfind("Summary:").unwrap()..];
    let grab = |key: &str| -> f64 {
        let i = summary.find(key).unwrap() + key.len();
        summary[i..].split_whitespace().next().unwrap().parse().unwrap()
    };
    (grab("I:"), grab("Peak:"))
}

#[test]
fn loudness_matches_ffmpeg() {
    let (Some(s), true) = (common::samples(), common::has_ffmpeg()) else { return };
    for f in [
        "Neon Harbor/2023 - Night Drive/01 Harbor Lights.flac",
        "Cassette Club/Lo-Fi Sketches/01 Tape Hiss Morning.mp3",
        "Mira Sol/Glass Garden/01 Greenhouse.m4a",
    ] {
        let p = s.join(f);
        let m = sori_core::analysis::analyze_file(&p, &AtomicBool::new(false)).unwrap();
        let (i, peak) = ffmpeg_loudness(&p);
        assert!((m.integrated - i).abs() <= 0.15, "{f}: 통합 라우드니스 {} vs ffmpeg {i}", m.integrated);
        assert!((m.true_peak - peak).abs() <= 0.35, "{f}: 트루 피크 {} vs ffmpeg {peak}", m.true_peak);
        assert_eq!(m.waveform.len(), 200);
        assert!(m.waveform.iter().filter(|&&v| v > 0).count() > 150, "{f}: 파형이 비었다");
    }
}
