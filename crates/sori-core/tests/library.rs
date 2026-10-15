//! 시험용 라이브러리(music/Sori Samples)를 실제로 스캔해 본다.
mod common;

use std::sync::Arc;
use std::time::Duration;

use sori_core::library::Library;
use sori_core::model::Album;
use sori_core::{Config, Store};

fn scanned() -> Option<(Arc<Store>, tempfile::TempDir)> {
    let music = common::samples()?;
    let data = tempfile::tempdir().unwrap();
    let store = Store::open(Config::new(music, data.path()));
    // 분석은 따로 시험한다.
    store.set_analysis_speed(sori_core::model::AnalysisSpeed::Off);
    store.start_scan();
    assert!(store.wait_for_scan(Duration::from_secs(120)));
    Some((store, data))
}

fn album<'a>(lib: &'a Library, title: &str) -> &'a Album {
    lib.albums.values().find(|a| a.title == title).unwrap_or_else(|| panic!("앨범 없음: {title}"))
}

#[test]
fn scans_the_sample_library() {
    let Some((store, _data)) = scanned() else { return };
    let lib = store.library();
    assert_eq!(store.scan_status().error, None);
    assert_eq!(lib.summary.tracks, 22);
    let mut titles: Vec<&str> = lib.albums.values().map(|a| a.title.as_str()).collect();
    titles.sort();
    let mut want =
        vec!["Field Notes - Rain Studies", "Glass Garden", "Lo-Fi Sketches", "Night Drive", "Paper Moon", "Sori Sampler", "아침의 차"];
    want.sort();
    assert_eq!(titles, want);

    // 디스크 두 장짜리 컴필레이션은 한 앨범
    let a = album(&lib, "Sori Sampler");
    assert_eq!(a.artist, "Various Artists");
    assert_eq!(a.disc_count, 2);
    let discs: Vec<u32> = a.track_ids.iter().map(|id| lib.views[id].disc_no.unwrap()).collect();
    assert_eq!(discs, vec![1, 1, 2, 2]);
    assert!(a.cover.is_some(), "부모 폴더의 cover.jpg");

    // 태그 없는 WAV는 폴더와 파일 이름에서
    let a = album(&lib, "Field Notes - Rain Studies");
    let ts: Vec<(Option<u32>, String)> = a.track_ids.iter().map(|id| (lib.views[id].track_no, lib.views[id].title.clone())).collect();
    assert_eq!(ts, vec![(Some(1), "Rain on Tin".to_string()), (Some(2), "Gutter Song".to_string())]);
    assert!(a.cover.is_none());

    // 포맷과 등급
    assert_eq!(album(&lib, "Night Drive").format.sample_rate, Some(96_000));
    assert_eq!(album(&lib, "Night Drive").format.bit_depth, Some(24));
    assert_eq!(album(&lib, "Glass Garden").format.codec, "ALAC");
    assert_eq!(album(&lib, "Paper Moon").format.codec, "AIFF");
    assert_eq!(album(&lib, "Lo-Fi Sketches").format.codec, "MP3");
    assert!(!album(&lib, "Lo-Fi Sketches").format.lossless);
    assert_eq!(album(&lib, "아침의 차").format.bit_depth, Some(16));

    // 표지: 박힌 것과 폴더의 것
    for t in ["Night Drive", "Glass Garden", "Lo-Fi Sketches", "아침의 차", "Paper Moon"] {
        let c = album(&lib, t).cover.clone().unwrap_or_else(|| panic!("표지 없음: {t}"));
        assert!(c.color.starts_with('#') && c.color.len() == 7, "{t}: {}", c.color);
        let f = sori_core::covers::cover_file(&store.covers_dir(), &album(&lib, t).id, &c.v, sori_core::covers::CoverSize::Sm);
        assert!(f.exists(), "{}", f.display());
    }

    // 아티스트: 앨범 아티스트와 참여곡
    let neon = lib.ordered_artists().find(|a| a.name == "Neon Harbor").unwrap();
    assert_eq!(neon.album_ids.len(), 2);
    assert_eq!(neon.appears_on.len(), 1, "컴필레이션의 한 곡");
    assert!(!lib.ordered_artists().any(|a| a.name == "Various Artists"));

    // 검색
    assert_eq!(lib.search("아침").albums.first().map(|a| a.title.as_str()), Some("아침의 차"));
    assert_eq!(lib.search("neon night").albums.first().map(|a| a.title.as_str()), Some("Night Drive"));
    assert!(lib.search("chrome rain").tracks.iter().any(|t| t.title == "Chrome Rain"));

    // 길이: 합성한 곡은 모두 0초보다 길다
    assert!(lib.views.values().all(|t| t.duration > 1.0));

    // 다시 스캔하면 바뀐 게 없으니 그대로 쓴다
    let before = store.library();
    store.start_scan();
    assert!(store.wait_for_scan(Duration::from_secs(60)));
    let st = store.scan_status();
    assert_eq!(st.added, Some(0));
    assert_eq!(st.removed, Some(0));
    let after = store.library();
    for (id, a) in &before.albums {
        assert_eq!(a.cover, after.albums[id].cover);
    }
}

#[test]
fn reopens_from_disk_without_rescanning() {
    let Some((store, data)) = scanned() else { return };
    let n = store.library().summary.tracks;
    let music = store.music_dir();
    drop(store);
    let again = Store::open(Config::new(music, data.path()));
    assert_eq!(again.library().summary.tracks, n);
}

/// 유튜브에서 받은 WebM(Opus): 태그 읽는 쪽이 모르는 형식도 목록에 오르고, 길이·형식이 맞다.
#[test]
fn webm_files_are_listed() {
    if !common::has_ffmpeg() {
        return;
    }
    let music = tempfile::tempdir().unwrap();
    let dir = music.path().join("Audio");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("마음이 조급할 때 듣는 브금.webm");
    let ok = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-c:a", "libopus", "-f", "webm"])
        .arg(&p)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        return;
    }
    let data = tempfile::tempdir().unwrap();
    let store = Store::open(Config::new(music.path().to_path_buf(), data.path()));
    store.set_analysis_speed(sori_core::model::AnalysisSpeed::Off);
    store.start_scan();
    assert!(store.wait_for_scan(Duration::from_secs(60)));
    let lib = store.library();
    let t = lib.views.values().next().expect("WebM이 목록에 없다");
    assert_eq!(t.title, "마음이 조급할 때 듣는 브금");
    assert_eq!(t.album, "Audio", "앨범 태그가 없으면 폴더 이름");
    assert_eq!(t.format.codec, "Opus");
    assert_eq!(t.format.sample_rate, Some(48_000));
    assert!((t.duration - 3.0).abs() < 0.1, "길이 {}", t.duration);
    // 그리고 틀 수 있다
    let (path, _) = store.track_file(&t.id).unwrap();
    let (spec, pcm) = sori_core::decode::decode_all(&path).unwrap();
    assert!((pcm.len() as f64 / spec.channels as f64 / spec.rate as f64 - 3.0).abs() < 0.05);
}
