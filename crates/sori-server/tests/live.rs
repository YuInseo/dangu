//! 라이브 아카이브를 서버 끝에서 끝까지: 가짜 archive.org → 소리 서버 → 화면이 부르는 API.
//! 검색·공연·프로브, 브라우저 존의 DSP 스트림(64비트 체인 → 24비트 WAV), 그리고 이 기기 존에서
//! 받으면서 트는 것까지 본다.

#[path = "../../sori-core/tests/support/fake_archive.rs"]
mod fake_archive;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sori_core::dsp::request::{Dither, DspRequest, Out, encode_request};
use sori_core::live::Archive;
use sori_core::player::output::NullOutput;
use sori_core::{Config, Store, decode};
use sori_server::App;

fn sample() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../music/Sori Samples/Neon Harbor/2023 - Night Drive/01 Harbor Lights.flac");
    if p.is_file() {
        return Some(p);
    }
    assert!(std::env::var_os("SORI_REQUIRE_SAMPLES").is_none(), "시험용 라이브러리가 없습니다");
    None
}

fn fake(flac: &[u8]) -> fake_archive::Fake {
    let search = json!({
        "response": { "numFound": 1, "docs": [{ "identifier": "show-1", "title": "Live", "creator": "Band", "date": "2001-02-03T00:00:00Z", "format": ["24bit Flac"] }] }
    });
    let meta = json!({
        "metadata": { "title": "Live", "creator": "Band", "date": "2001-02-03", "description": "a<br>b" },
        "files": [{ "name": "d1t01.flac", "format": "24bit Flac", "title": "One", "track": "1", "length": "0:30" }]
    });
    fake_archive::serve(
        HashMap::from([
            ("/advancedsearch.php".to_string(), serde_json::to_vec(&search).unwrap()),
            ("/metadata/show-1".to_string(), serde_json::to_vec(&meta).unwrap()),
            ("/metadata/missing".to_string(), b"{}".to_vec()),
            ("/items/show-1/d1t01.flac".to_string(), flac.to_vec()),
        ]),
        None,
    )
}

fn app(archive: &str, dir: &tempfile::TempDir) -> App {
    let music = dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let store = Store::open(Config::new(&music, dir.path().join("data")));
    let mut app = App::new(store, String::new());
    app.live = Arc::new(Archive::new(archive));
    app
}

/// 소리 서버를 남는 포트에 띄우고 주소를 돌려준다.
fn start(app: App) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let (otx, orx) = tokio::sync::oneshot::channel();
            tokio::spawn(sori_server::serve(app, "127.0.0.1:0".parse().unwrap(), Some(otx)));
            tx.send(orx.await.unwrap()).unwrap();
            std::future::pending::<()>().await;
        });
    });
    format!("http://{}", rx.recv().unwrap())
}

fn client() -> ureq::Agent {
    ureq::Agent::config_builder().http_status_as_error(false).build().new_agent()
}

fn get(url: &str) -> (u16, Vec<u8>) {
    let mut r = client().get(url).call().unwrap();
    let status = r.status().as_u16();
    (status, r.body_mut().with_config().limit(1 << 30).read_to_vec().unwrap())
}

fn get_json(url: &str) -> (u16, Value) {
    let (s, b) = get(url);
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

fn post(url: &str, body: Value) -> Value {
    let mut r = client().post(url).header("content-type", "application/json").send(body.to_string()).unwrap();
    serde_json::from_slice(&r.body_mut().read_to_vec().unwrap()).unwrap()
}

#[test]
fn search_show_and_probe() {
    let Some(path) = sample() else { return };
    let flac = std::fs::read(&path).unwrap();
    let fake = fake(&flac);
    let dir = tempfile::tempdir().unwrap();
    let base = start(app(&fake.base, &dir));

    let (s, r) = get_json(&format!("{base}/api/live/search?q=Band&sort=popular&page=1&rows=48"));
    assert_eq!(s, 200);
    assert_eq!(r["total"], 1);
    assert_eq!(r["shows"][0]["id"], "show-1");
    assert_eq!(r["shows"][0]["quality"], "flac24");
    assert_eq!(r["shows"][0]["date"], "2001-02-03");

    let (s, show) = get_json(&format!("{base}/api/live/show/show-1"));
    assert_eq!(s, 200);
    assert_eq!(show["band"], "Band");
    assert_eq!(show["notes"], "a\nb");
    assert_eq!(show["tracks"][0]["title"], "One");
    assert_eq!(show["tracks"][0]["duration"], 30.0);
    assert_eq!(show["tracks"][0]["format"]["bitDepth"], 24);
    assert_eq!(show["tracks"][0]["url"], format!("{}/download/show-1/d1t01.flac", fake.base));

    let (s, e) = get_json(&format!("{base}/api/live/show/missing"));
    assert_eq!(s, 404);
    assert!(e["error"].is_string());
    assert_eq!(get_json(&format!("{base}/api/live/show/..%2Fx")).0, 404);

    let (s, info) = get_json(&format!("{base}/api/live/probe?id=show-1&file=d1t01.flac"));
    assert_eq!(s, 200);
    let spec = decode::open(&path).unwrap().spec().clone();
    assert_eq!(info, json!({ "sampleRate": spec.rate, "channels": spec.channels, "bitDepth": spec.bits, "totalSamples": spec.frames }));
    assert_eq!(get_json(&format!("{base}/api/live/probe?id=show-1&file=../x.flac")).0, 404);

    // archive.org에 닿지 못하면 502와 사람이 읽는 오류
    let dir2 = tempfile::tempdir().unwrap();
    let down = start(app("http://127.0.0.1:9", &dir2));
    let (s, e) = get_json(&format!("{down}/api/live/search?q=x"));
    assert_eq!(s, 502);
    assert_eq!(e["error"], "archive.org에 닿지 못했습니다");
}

#[test]
fn dsp_stream_runs_the_64bit_chain() {
    let Some(path) = sample() else { return };
    let flac = std::fs::read(&path).unwrap();
    let fake = fake(&flac);
    let dir = tempfile::tempdir().unwrap();
    let base = start(app(&fake.base, &dir));
    let req = DspRequest { v: 1, hr: Some(-3.0), ci: None, lv: None, src: None, out: Out { b: 24, d: Dither::None }, f: vec![] };
    let url = format!("{base}/api/live/stream?id=show-1&file=d1t01.flac&dsp={}", encode_request(&req));
    let (s, wav) = get(&url);
    assert_eq!(s, 200);
    let mut r = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
    let spec = r.spec();
    let (src, want) = decode::decode_all(&path).unwrap();
    assert_eq!(spec.sample_rate, src.rate);
    assert_eq!(spec.bits_per_sample, 24);
    let got: Vec<i32> = r.samples::<i32>().map(Result::unwrap).collect();
    assert_eq!(got.len(), want.len());
    let g = 10f64.powf(-3.0 / 20.0);
    let scale = (1 << 23) as f64;
    let worst = got.iter().zip(&want).map(|(&q, &x)| (q as f64 - x * g * scale).abs()).fold(0.0, f64::max);
    assert!(worst <= 0.5 + 1e-9, "24비트로 반올림한 것과 다름: {worst} LSB");
    // 요청이 없거나 이상한 이름이면 400
    assert_eq!(get(&format!("{base}/api/live/stream?id=show-1&file=d1t01.flac")).0, 400);
    assert_eq!(get(&format!("{base}/api/live/stream?id=show-1&file=../d1t01.flac&dsp={}", encode_request(&req))).0, 400);
}

#[test]
fn native_zone_streams_live_tracks() {
    let Some(path) = sample() else { return };
    let flac = std::fs::read(&path).unwrap();
    let fake = fake(&flac);
    let dir = tempfile::tempdir().unwrap();
    let out = NullOutput::new(96_000, false);
    let captured = out.captured.clone();
    let base = start(app(&fake.base, &dir).with_output(Box::new(move || Box::new(out))));
    let item = json!({ "uid": "q1", "source": { "kind": "live", "id": "show-1", "file": "d1t01.flac" } });
    assert_eq!(post(&format!("{base}/api/player/load"), json!({ "item": item, "autoplay": true }))["ok"], true);
    let t = Instant::now();
    let ended = loop {
        let (_, st) = get_json(&format!("{base}/api/player"));
        let events = st["state"]["events"].as_array().cloned().unwrap_or_default();
        if events.iter().any(|e| e[1]["type"] == "ended") {
            break st;
        }
        assert!(!events.iter().any(|e| e[1]["type"] == "error"), "{st}");
        assert!(t.elapsed() < Duration::from_secs(60), "끝나지 않았습니다: {st}");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(ended["state"]["source"]["rate"], 96_000);
    let (_, want) = decode::decode_all(&path).unwrap();
    let got = captured.lock().clone();
    assert!(got == want, "받으면서 튼 소리가 파일과 비트 단위로 같아야 합니다");
}

fn post_status(url: &str, body: Value) -> (u16, Value) {
    let mut r = client().post(url).header("content-type", "application/json").send(body.to_string()).unwrap();
    let status = r.status().as_u16();
    (status, serde_json::from_slice(&r.body_mut().read_to_vec().unwrap()).unwrap_or(Value::Null))
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(60);
    while !ok() {
        assert!(Instant::now() < until, "{what}: 시간이 지났습니다");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// 링크로 열고, 라이브러리에 저장하고, 라이브러리에 앨범으로 오르는 데까지.
#[test]
fn links_and_saving_to_the_library() {
    use sha1::{Digest, Sha1};
    let Some(path) = sample() else { return };
    let flac = std::fs::read(&path).unwrap();
    let sha1: String = Sha1::digest(&flac).iter().map(|b| format!("{b:02x}")).collect();
    let meta = |collection: Value, files: Value| {
        serde_json::to_vec(&json!({ "metadata": { "title": "Live", "creator": "Band", "date": "2001-02-03", "venue": "Hall", "collection": collection }, "files": files }))
            .unwrap()
    };
    let file =
        |name: &str, title: &str| json!({ "name": name, "format": "Flac", "title": title, "size": flac.len().to_string(), "sha1": sha1 });
    let fake = fake_archive::serve(
        HashMap::from([
            ("/metadata/show-2".to_string(), meta(json!(["Band", "etree"]), json!([file("b-t01.flac", "One"), file("b-t02.flac", "Two")]))),
            (
                "/metadata/gd".to_string(),
                meta(
                    json!(["GratefulDead", "etree", "stream_only"]),
                    json!([{ "name": "a.shn", "format": "Shorten", "private": "true" }, { "name": "a.mp3", "format": "VBR MP3" }]),
                ),
            ),
            ("/metadata/other".to_string(), meta(json!(["opensource_audio"]), json!([file("x.flac", "X")]))),
            ("/metadata/missing".to_string(), b"{}".to_vec()),
            ("/items/show-2/b-t01.flac".to_string(), flac.clone()),
            ("/items/show-2/b-t02.flac".to_string(), flac.clone()),
        ]),
        None,
    );
    let dir = tempfile::tempdir().unwrap();
    let base = start(app(&fake.base, &dir));

    // 링크로 열기: 라이브 음악 아카이브의 공연만
    let (s, r) = post_status(&format!("{base}/api/live/resolve"), json!({ "url": "https://archive.org/details/show-2?x=1" }));
    assert_eq!((s, &r["id"], &r["saveable"]), (200, &json!("show-2"), &json!(true)), "{r}");
    let (s, r) = post_status(&format!("{base}/api/live/resolve"), json!({ "url": "https://www.youtube.com/watch?v=abc" }));
    assert_eq!(s, 400);
    assert!(r["error"].as_str().unwrap().contains("저작권"), "{r}");
    let (s, _) = post_status(&format!("{base}/api/live/resolve"), json!({ "url": "https://archive.org/details/other" }));
    assert_eq!(s, 422, "라이브 음악 아카이브가 아닌 자료");
    let (s, _) = post_status(&format!("{base}/api/live/resolve"), json!({ "url": "archive.org/details/missing" }));
    assert_eq!(s, 404);

    // 아티스트가 듣기만 허락한 공연은 저장하지 않는다.
    let (s, show) = get_json(&format!("{base}/api/live/show/gd"));
    assert_eq!((s, &show["saveable"]), (200, &json!(false)));
    let (s, r) = post_status(&format!("{base}/api/live/save"), json!({ "id": "gd" }));
    assert_eq!(s, 403);
    assert!(r["error"].as_str().unwrap().contains("듣기만"), "{r}");
    assert_eq!(get_json(&format!("{base}/api/live/saves/gd")).0, 404);

    // 저장
    let (s, job) = post_status(&format!("{base}/api/live/save"), json!({ "id": "show-2" }));
    assert_eq!(s, 200, "{job}");
    assert_eq!(job["filesTotal"], 2);
    assert_eq!(job["album"], "2001-02-03 Hall");
    wait_for("저장", || get_json(&format!("{base}/api/live/saves/show-2")).1["state"] == "done");
    let show_dir = dir.path().join("music/Live Archive/Band/2001-02-03 Hall");
    let mut names: Vec<String> =
        std::fs::read_dir(&show_dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    names.sort();
    assert_eq!(names, ["01 One.flac", "02 Two.flac"]);

    // 다 받으면 라이브러리를 다시 읽고, 공연이 앨범 하나로 오른다.
    let mut album = String::new();
    wait_for("라이브러리", || {
        let (_, j) = get_json(&format!("{base}/api/live/saves/show-2"));
        album = j["albumId"].as_str().unwrap_or_default().to_string();
        !album.is_empty()
    });
    let (s, a) = get_json(&format!("{base}/api/albums/{album}"));
    assert_eq!(s, 200);
    assert_eq!(a["album"]["title"], "2001-02-03 Hall", "{a}");
    assert_eq!(a["album"]["artist"], "Band");
    assert_eq!(a["album"]["trackIds"].as_array().map(Vec::len), Some(2));
    let (_, list) = get_json(&format!("{base}/api/live/saves"));
    assert_eq!(list["jobs"][0]["state"], "done");

    // 앱을 다시 켜도 저장한 공연인 줄 안다.
    let again = start(app(&fake.base, &dir));
    let (s, j) = get_json(&format!("{again}/api/live/saves/show-2"));
    assert_eq!((s, &j["state"]), (200, &json!("done")), "{j}");
}
