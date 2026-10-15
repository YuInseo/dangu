//! 좋아요·플레이리스트 API를 시험용 라이브러리로 끝에서 끝까지.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use sori_core::{Config, Store};
use sori_server::App;

fn samples() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../music/Sori Samples");
    if p.is_dir() {
        return Some(p);
    }
    assert!(std::env::var_os("SORI_REQUIRE_SAMPLES").is_none(), "시험용 라이브러리가 없습니다");
    None
}

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

struct Http(ureq::Agent, String);

impl Http {
    fn call(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let url = format!("{}{path}", self.1);
        let res = match (method, body) {
            ("GET", _) => self.0.get(&url).call(),
            ("DELETE", _) => self.0.delete(&url).call(),
            ("PUT", _) => self.0.put(&url).send_empty(),
            (m, Some(b)) => {
                let r = if m == "POST" { self.0.post(&url) } else { self.0.patch(&url) };
                r.header("content-type", "application/json").send(b.to_string())
            }
            (m, None) => panic!("{m}"),
        };
        let mut r = res.unwrap();
        let s = r.status().as_u16();
        (s, serde_json::from_slice(&r.body_mut().read_to_vec().unwrap()).unwrap_or(Value::Null))
    }
}

#[test]
fn likes_and_playlists() {
    let Some(music) = samples() else { return };
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(Config::new(&music, dir.path().join("data")));
    store.start_scan();
    assert!(store.wait_for_scan(Duration::from_secs(120)));
    let base = start(App::new(store, String::new()));
    let h = Http(ureq::Agent::config_builder().http_status_as_error(false).build().new_agent(), base);

    let (_, all) = h.call("GET", "/api/tracks?limit=10", None);
    let ids: Vec<String> = all["tracks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect();
    let album = all["tracks"][0]["albumId"].as_str().unwrap().to_string();

    // 좋아요: 곡 둘, 앨범 하나. 최근 것이 앞.
    assert_eq!(h.call("PUT", &format!("/api/favorites/tracks/{}", ids[0]), None).1["liked"], true);
    h.call("PUT", &format!("/api/favorites/tracks/{}", ids[3]), None);
    h.call("PUT", &format!("/api/favorites/albums/{album}"), None);
    let (_, fav) = h.call("GET", "/api/favorites", None);
    assert_eq!(fav["tracks"].as_array().unwrap().len(), 2);
    assert_eq!(fav["tracks"][0]["id"], ids[3].as_str());
    assert_eq!(fav["albums"][0]["id"], album.as_str());
    let (_, fid) = h.call("GET", "/api/favorites/ids", None);
    assert_eq!(fid["tracks"], json!([ids[3], ids[0]]));
    h.call("DELETE", &format!("/api/favorites/tracks/{}", ids[3]), None);
    assert_eq!(h.call("GET", "/api/favorites/ids", None).1["tracks"], json!([ids[0]]));
    assert_eq!(h.call("PUT", "/api/favorites/tracks/..%2Fx", None).0, 400);

    // 플레이리스트: 만들기 → 더하기 → 순서 바꾸기·이름 → 읽기 → 지우기
    let (s, p) = h.call("POST", "/api/playlists", Some(json!({ "name": "밤 드라이브", "tracks": [ids[1], ids[2]] })));
    assert_eq!(s, 201, "{p}");
    let pid = p["id"].as_str().unwrap().to_string();
    assert_eq!(p["count"], 2);
    assert!(p["duration"].as_f64().unwrap() > 0.0);
    assert!(!p["covers"].as_array().unwrap().is_empty());
    let (_, p) = h.call("POST", &format!("/api/playlists/{pid}/tracks"), Some(json!({ "ids": [ids[4], "zzzzzzzzzzzz", "없는곡"] })));
    assert_eq!(p["count"], 3, "라이브러리에 없는 곡은 세지 않는다(이상한 ID는 아예 받지 않는다)");
    let (_, full) = h.call("GET", &format!("/api/playlists/{pid}"), None);
    assert_eq!(full["ids"], json!([ids[1], ids[2], ids[4], "zzzzzzzzzzzz"]), "적어 둔 ID는 지금 없는 곡까지 전부");
    assert_eq!(full["missing"], 1);
    assert_eq!(full["tracks"].as_array().unwrap().len(), 3);
    let (_, p) = h.call("PATCH", &format!("/api/playlists/{pid}"), Some(json!({ "name": "새벽", "tracks": [ids[4], ids[1]] })));
    assert_eq!(p["name"], "새벽");
    let (_, full) = h.call("GET", &format!("/api/playlists/{pid}"), None);
    assert_eq!(
        full["tracks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap()).collect::<Vec<_>>(),
        [ids[4].as_str(), ids[1].as_str()]
    );
    assert_eq!(full["missing"], 0);
    assert_eq!(full["ids"], json!([ids[4], ids[1]]));
    let (_, list) = h.call("GET", "/api/playlists", None);
    assert_eq!(list[0]["name"], "새벽");
    assert_eq!(h.call("POST", "/api/playlists", Some(json!({ "name": "  " }))).0, 400);
    assert_eq!(h.call("DELETE", &format!("/api/playlists/{pid}"), None).0, 200);
    assert_eq!(h.call("GET", &format!("/api/playlists/{pid}"), None).0, 404);
}
