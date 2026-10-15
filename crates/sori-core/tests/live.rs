//! 라이브 아카이브: 가짜 archive.org(이 컴퓨터 안의 작은 HTTP 서버)에 대고 시험한다.
//! 주소를 옮기고(302), 범위 요청을 받고, 일부러 연결을 끊어도, 받으면서 푼 소리가 파일을
//! 직접 푼 것과 비트 단위로 같아야 한다.

mod common;
#[path = "support/fake_archive.rs"]
mod fake_archive;

use std::collections::HashMap;
use std::path::Path;

use fake_archive::{Fake, serve};

use sori_core::decode::{self, PcmSource};
use sori_core::live::{Archive, Quality, Sort, StreamInfo};

fn read_all(src: &mut dyn PcmSource) -> Vec<f64> {
    let mut out = Vec::new();
    while src.read(&mut out).unwrap() > 0 {}
    out
}

fn sample_flac() -> Option<std::path::PathBuf> {
    let p = common::samples()?.join("Neon Harbor/2023 - Night Drive/01 Harbor Lights.flac");
    assert!(p.is_file());
    Some(p)
}

fn fake_with(path: &Path, cut: Option<usize>) -> (Fake, Archive) {
    let bytes = std::fs::read(path).unwrap();
    let fake = serve(HashMap::from([("/items/show-1/d1t01.flac".to_string(), bytes)]), cut);
    let archive = Archive::new(&fake.base);
    (fake, archive)
}

#[test]
fn streamed_decode_matches_the_file() {
    let Some(path) = sample_flac() else { return };
    let (_fake, archive) = fake_with(&path, None);
    let mut remote = archive.open("show-1", "d1t01.flac").unwrap();
    assert!(remote.remote());
    let (spec, want) = decode::decode_all(&path).unwrap();
    assert_eq!(remote.spec(), &spec);
    let got = read_all(remote.as_mut());
    assert_eq!(got.len(), want.len());
    assert!(got == want, "받으면서 푼 소리가 파일과 다릅니다");
}

#[test]
fn streamed_seek_matches_the_file() {
    let Some(path) = sample_flac() else { return };
    let (fake, archive) = fake_with(&path, None);
    let mut remote = archive.open("show-1", "d1t01.flac").unwrap();
    let mut local = decode::open(&path).unwrap();
    let rate = local.spec().rate as u64;
    let frames = local.spec().frames.unwrap();
    // 앞으로, 뒤로, 끝 가까이로 건너뛴다.
    for t in [frames / 2 + 123, rate / 3, frames.saturating_sub(rate / 2), 7] {
        let a = remote.seek(t).unwrap();
        let b = local.seek(t).unwrap();
        assert_eq!(a, b);
        let (mut x, mut y) = (Vec::new(), Vec::new());
        while x.len() < 8192 && remote.read(&mut x).unwrap() > 0 {}
        while y.len() < 8192 && local.read(&mut y).unwrap() > 0 {}
        let n = x.len().min(y.len());
        assert!(n > 0);
        assert!(x[..n] == y[..n], "{t}프레임으로 건너뛴 뒤가 다릅니다");
    }
    // 처음 한 번 옮겨 가고(302), 그다음은 옮겨 간 주소로 범위 요청을 한다. 짧은 파일이면 처음
    // 받은 것에 다 들어 있어 더 묻지 않을 수도 있다(멀리 건너뛰기는 아래 시험이 따로 본다).
    assert!(fake.requests() >= 2);
}

#[test]
fn far_seeks_restart_with_range_requests() {
    use std::io::{Read, Seek, SeekFrom};
    // 앞서 받는 창(16MB)보다 큰 파일: 멀리 건너뛰면 그 위치부터 새로 받고, 뒤로 멀리 가도 그렇다.
    let size = 40usize << 20;
    let bytes: Vec<u8> = (0..size).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
    let fake = serve(HashMap::from([("/items/big/blob.bin".to_string(), bytes.clone())]), None);
    let agent = sori_core::live::http::agent();
    let mut src = sori_core::live::http::HttpSource::open(&agent, &format!("{}/download/big/blob.bin", fake.base)).unwrap();
    assert_eq!(src.len(), size as u64);
    let check = |src: &mut sori_core::live::http::HttpSource, at: usize| {
        src.seek(SeekFrom::Start(at as u64)).unwrap();
        let mut buf = vec![0u8; 100_000];
        src.read_exact(&mut buf).unwrap();
        assert!(buf == bytes[at..at + buf.len()], "{at}에서 읽은 것이 다릅니다");
    };
    check(&mut src, 0);
    let before = fake.requests();
    check(&mut src, 33 << 20);
    check(&mut src, 5 << 20);
    check(&mut src, size - 100_000);
    assert!(fake.requests() >= before + 2, "멀리 건너뛸 때 범위 요청으로 새로 받아야 합니다: {}", fake.requests() - before);
    // 끝에서는 0바이트
    src.seek(SeekFrom::End(0)).unwrap();
    assert_eq!(src.read(&mut [0u8; 16]).unwrap(), 0);
}

#[test]
fn dropped_connections_are_resumed() {
    let Some(path) = sample_flac() else { return };
    let (fake, archive) = fake_with(&path, Some(700_000));
    let mut remote = archive.open("show-1", "d1t01.flac").unwrap();
    let (_, want) = decode::decode_all(&path).unwrap();
    let got = read_all(remote.as_mut());
    assert!(got == want, "끊긴 연결을 이어 받은 소리가 파일과 다릅니다");
    let size = std::fs::metadata(&path).unwrap().len() as usize;
    assert!(fake.requests() > size / 700_000, "연결이 끊길 때마다 이어 받아야 합니다");
}

#[test]
fn probe_reads_streaminfo_from_the_head() {
    let Some(path) = sample_flac() else { return };
    let (_fake, archive) = fake_with(&path, None);
    let info = archive.probe("show-1", "d1t01.flac").unwrap().unwrap();
    let spec = decode::open(&path).unwrap().spec().clone();
    assert_eq!(
        info,
        StreamInfo {
            sample_rate: spec.rate,
            channels: spec.channels as u32,
            bit_depth: spec.bits.unwrap(),
            total_samples: spec.frames.unwrap()
        }
    );
    // 이상한 이름은 묻지도 않는다.
    assert_eq!(archive.probe("show-1", "../secret.flac").unwrap(), None);
    assert_eq!(archive.probe("show-1", "d1t01.mp3").unwrap(), None);
}

#[test]
fn search_and_show_through_http() {
    let search = serde_json::json!({
        "response": { "numFound": 1, "docs": [{ "identifier": "show-1", "title": "T", "creator": "Band", "date": "2001-02-03T00:00:00Z", "format": ["Flac"] }] }
    });
    let meta = serde_json::json!({
        "metadata": { "title": "T", "creator": "Band", "date": "2001-02-03" },
        "files": [{ "name": "d1t01.flac", "format": "Flac", "title": "One", "track": "1", "length": "1:00" }]
    });
    let fake = serve(
        HashMap::from([
            ("/advancedsearch.php".to_string(), serde_json::to_vec(&search).unwrap()),
            ("/metadata/show-1".to_string(), serde_json::to_vec(&meta).unwrap()),
            ("/metadata/empty".to_string(), b"{}".to_vec()),
        ]),
        None,
    );
    let archive = Archive::new(&fake.base);
    let r = archive.search("band", Sort::Popular, 1, 48).unwrap();
    assert_eq!(r.total, 1);
    assert_eq!(r.shows[0].quality, Quality::Flac);
    assert_eq!(r.shows[0].date.as_deref(), Some("2001-02-03"));
    let s = archive.show("show-1").unwrap();
    let s = s.as_ref().as_ref().unwrap();
    assert_eq!(s.tracks[0].url, format!("{}/download/show-1/d1t01.flac", fake.base));
    assert_eq!(s.tracks[0].duration, 60.0);
    assert!(archive.show("empty").unwrap().is_none());
    assert!(archive.show("../x").unwrap().is_none());
    // 두 번째는 캐시에서
    let before = fake.requests();
    archive.show("show-1").unwrap();
    archive.search("band", Sort::Popular, 1, 48).unwrap();
    assert_eq!(fake.requests(), before);
    // 서버가 404면 오류
    assert!(Archive::new(&format!("{}/nowhere", fake.base)).search("band", Sort::Recent, 1, 48).is_err());
}

// ---------------------------------------------------------------------------
// 라이브러리에 저장(내려받기)

mod saving {
    use super::*;
    use sha1::{Digest, Sha1};
    use sori_core::live::LiveShowDetail;
    use sori_core::live::save::{self, Cancelled, Progress};
    use std::sync::atomic::{AtomicBool, Ordering};

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// 두 장짜리 공연(장마다 트랙 번호를 1부터). 두 곡 다 같은 시험용 FLAC.
    fn fake_show(flac: &[u8], sha1: &str, cut: Option<usize>) -> (Fake, Archive, LiveShowDetail) {
        let meta = serde_json::json!({
            "metadata": {
                "title": "Neon Harbor Live at Test Hall", "creator": "Neon Harbor", "date": "2023-06-07",
                "venue": "Test Hall", "coverage": "Seoul", "collection": ["NeonHarbor", "etree"]
            },
            "files": [
                { "name": "nh-d2t01.flac", "format": "Flac", "title": "Encore: Harbor Lights?", "track": "01", "size": flac.len().to_string(), "sha1": sha1 },
                { "name": "nh-d1t01.flac", "format": "Flac", "title": "Harbor Lights", "track": "01", "size": flac.len().to_string(), "sha1": sha1 },
                { "name": "nh.ffp", "format": "Flac FingerPrint" }
            ]
        });
        let fake = serve(
            HashMap::from([
                ("/metadata/show-2".to_string(), serde_json::to_vec(&meta).unwrap()),
                ("/items/show-2/nh-d1t01.flac".to_string(), flac.to_vec()),
                ("/items/show-2/nh-d2t01.flac".to_string(), flac.to_vec()),
            ]),
            cut,
        );
        let archive = Archive::new(&fake.base);
        let show = archive.show("show-2").unwrap().as_ref().clone().unwrap();
        (fake, archive, show)
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        v.sort();
        v
    }

    #[test]
    fn saves_a_show_into_the_library_resuming_dropped_connections() {
        let Some(path) = sample_flac() else { return };
        let flac = std::fs::read(&path).unwrap();
        // 1MB마다 연결이 끊긴다: 받은 데부터 이어 받아야 한다.
        let (fake, archive, show) = fake_show(&flac, &hex(&Sha1::digest(&flac)), Some(1 << 20));
        assert!(show.saveable, "{:?}", show.save_block);
        let music = tempfile::tempdir().unwrap();
        let root = save::root(music.path());
        let plan = save::plan(&show, &root, |_| None).unwrap();
        assert_eq!(plan.dir, music.path().join("Live Archive").join("Neon Harbor").join("2023-06-07 Test Hall"));

        let cancel = AtomicBool::new(false);
        let mut seen: Vec<Progress> = Vec::new();
        save::run(&archive, &plan, &cancel, |p| seen.push(p)).unwrap();
        let last = *seen.last().unwrap();
        assert_eq!((last.files_done, last.files_total), (2, 2));
        assert_eq!(last.bytes_done, 2 * flac.len() as u64);
        assert_eq!(last.bytes_total, Some(2 * flac.len() as u64));
        assert!(seen.windows(2).all(|w| w[1].bytes_done >= w[0].bytes_done), "진척은 줄지 않는다");
        assert!(fake.requests() > 2 * flac.len() / (1 << 20), "끊길 때마다 이어 받아야 합니다");
        // 숨은 이름(받는 중)은 남지 않고, 장이 뒤섞이지 않는다.
        assert_eq!(names(&plan.dir), ["01 Harbor Lights.flac", "02 Encore- Harbor Lights.flac"]);

        // 소리는 원본 그대로(태그만 붙었다)
        let (_, want) = decode::decode_all(&path).unwrap();
        let (_, got) = decode::decode_all(&plan.dir.join("01 Harbor Lights.flac")).unwrap();
        assert!(got == want, "저장한 곡의 소리가 원본과 다릅니다");

        // 라이브러리가 공연 하나를 한 앨범으로, 밴드 이름으로, 순서대로 읽는다.
        let covers = tempfile::tempdir().unwrap();
        let r = sori_core::scan::scan_library(music.path(), covers.path(), None, &|_, _, _| {}).unwrap();
        let mut tracks = r.index.tracks.clone();
        tracks.sort_by_key(|t| t.track_no);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].album_id, tracks[1].album_id, "한 앨범");
        assert_eq!(tracks[0].album, "2023-06-07 Test Hall");
        assert_eq!(tracks[0].artist, "Neon Harbor");
        assert_eq!(tracks[0].album_artist_tag.as_deref(), Some("Neon Harbor"));
        assert_eq!(tracks[0].year, Some(2023));
        assert_eq!(
            tracks.iter().map(|t| (t.track_no, t.title.as_str())).collect::<Vec<_>>(),
            [(Some(1), "Harbor Lights"), (Some(2), "Encore: Harbor Lights?")]
        );
        // 녹음한 사람이 적어 둔 장르는 둔다(없으면 Live).
        assert_eq!(tracks[0].genres, ["Synthwave"]);

        // 다시 저장하면 받은 것은 건너뛴다.
        let before = fake.requests();
        save::run(&archive, &plan, &cancel, |_| {}).unwrap();
        assert_eq!(fake.requests(), before, "이미 받은 곡을 다시 받았습니다");
    }

    #[test]
    fn a_damaged_download_is_not_kept() {
        let Some(path) = sample_flac() else { return };
        let flac = std::fs::read(&path).unwrap();
        let (_fake, archive, show) = fake_show(&flac, &"0".repeat(40), None);
        let music = tempfile::tempdir().unwrap();
        let plan = save::plan(&show, &save::root(music.path()), |_| None).unwrap();
        let err = format!("{:#}", save::run(&archive, &plan, &AtomicBool::new(false), |_| {}).unwrap_err());
        assert!(err.contains("SHA-1"), "{err}");
        assert!(names(&plan.dir).is_empty(), "손상된 파일이 남았습니다: {:?}", names(&plan.dir));
    }

    #[test]
    fn cancelling_stops_and_cleans_up() {
        let Some(path) = sample_flac() else { return };
        let flac = std::fs::read(&path).unwrap();
        let (_fake, archive, show) = fake_show(&flac, &hex(&Sha1::digest(&flac)), Some(1 << 20));
        let music = tempfile::tempdir().unwrap();
        let plan = save::plan(&show, &save::root(music.path()), |_| None).unwrap();
        let cancel = AtomicBool::new(false);
        let err = save::run(&archive, &plan, &cancel, |p| {
            if p.bytes_done > 0 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap_err();
        assert!(err.is::<Cancelled>(), "{err:#}");
        assert!(names(&plan.dir).is_empty(), "취소한 뒤 남은 파일: {:?}", names(&plan.dir));
    }

    #[test]
    fn shows_the_artist_did_not_share_are_not_saved() {
        let meta = serde_json::json!({
            "metadata": { "title": "T", "creator": "Grateful Dead", "date": "1977-05-08", "collection": ["GratefulDead", "etree", "stream_only"] },
            "files": [
                { "name": "gd77-05-08d1t01.shn", "format": "Shorten", "private": "true" },
                { "name": "gd77-05-08d1t01.mp3", "format": "VBR MP3", "title": "New Minglewood Blues" }
            ]
        });
        let fake = serve(HashMap::from([("/metadata/gd".to_string(), serde_json::to_vec(&meta).unwrap())]), None);
        let archive = Archive::new(&fake.base);
        let show = archive.show("gd").unwrap().as_ref().clone().unwrap();
        assert_eq!(show.tracks.len(), 1, "듣기는 된다");
        assert!(!show.saveable);
        let music = tempfile::tempdir().unwrap();
        let err = save::plan(&show, &save::root(music.path()), |_| None).unwrap_err().to_string();
        assert!(err.contains("듣기만"), "{err}");
        assert!(!music.path().join("Live Archive").exists());
    }
}
