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
    // 처음 한 번 옮겨 가고, 그다음부터는 옮겨 간 주소로 바로 범위 요청을 한다.
    assert!(fake.requests() >= 3);
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
