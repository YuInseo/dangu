#![allow(dead_code)]
use std::path::PathBuf;
use std::process::Command;

pub fn samples() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../music/Sori Samples");
    if p.is_dir() {
        Some(p.canonicalize().unwrap())
    } else {
        eprintln!("시험용 라이브러리가 없어 건너뜁니다 (cargo run -p sori-server -- sample)");
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
    out.stdout.chunks_exact(4).map(|b| i32::from_le_bytes(b.try_into().unwrap())).collect()
}

pub fn ffmpeg_f64(path: &std::path::Path) -> Vec<f64> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-f", "f64le", "-acodec", "pcm_f64le", "-"])
        .output()
        .expect("ffmpeg");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    out.stdout.chunks_exact(8).map(|b| f64::from_le_bytes(b.try_into().unwrap())).collect()
}
