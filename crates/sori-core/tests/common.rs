#![allow(dead_code)]
use std::path::PathBuf;
use std::process::Command;

pub fn samples() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../music/Sori Samples");
    if p.is_dir() {
        Some(p.canonicalize().unwrap())
    } else {
        // CI에서는 건너뛰지 않는다. 라이브러리를 못 만들었으면 시험이 실패해야 한다.
        assert!(std::env::var_os("SORI_REQUIRE_SAMPLES").is_none(), "시험용 라이브러리가 없습니다: {}", p.display());
        eprintln!("시험용 라이브러리가 없어 건너뜁니다 (node ui/scripts/make-sample-library.mjs)");
        None
    }
}

pub fn has_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// ffmpeg로 풀어 32비트 정수(왼쪽 정렬)로 받는다.
pub fn ffmpeg_s32(path: &std::path::Path) -> Vec<i32> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-f", "s32le", "-acodec", "pcm_s32le", "-"])
        .output()
        .expect("ffmpeg");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    out.stdout.as_chunks::<4>().0.iter().map(|b| i32::from_le_bytes(*b)).collect()
}

pub fn ffmpeg_f64(path: &std::path::Path) -> Vec<f64> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-f", "f64le", "-acodec", "pcm_f64le", "-"])
        .output()
        .expect("ffmpeg");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    out.stdout.as_chunks::<8>().0.iter().map(|b| f64::from_le_bytes(*b)).collect()
}
