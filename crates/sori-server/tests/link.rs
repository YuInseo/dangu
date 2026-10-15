//! 다른 기기와 잇기(선택): 앱은 기본으로 이 기기 안(127.0.0.1)에만 열리고, 켜면 같은 와이파이에
//! PIN으로 잠근 채 열리며, 찾기에 답하고, 끄면 열려 있던 연결까지 끊긴다.

use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sori_core::{Config, Store};
use sori_server::{App, link};

fn start(app: App, addr: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    let addr: std::net::SocketAddr = addr.parse().unwrap();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let (otx, orx) = tokio::sync::oneshot::channel();
            tokio::spawn(sori_server::serve(app, addr, Some(otx)));
            tx.send(orx.await.unwrap()).unwrap();
            std::future::pending::<()>().await;
        });
    });
    format!("http://127.0.0.1:{}", rx.recv().unwrap().port())
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .new_agent()
}

fn get(a: &ureq::Agent, url: &str, cookie: Option<&str>) -> (u16, Value) {
    let mut req = a.get(url);
    if let Some(c) = cookie {
        req = req.header("cookie", c);
    }
    let mut r = req.call().unwrap();
    let s = r.status().as_u16();
    (s, serde_json::from_slice(&r.body_mut().read_to_vec().unwrap()).unwrap_or(Value::Null))
}

fn post(a: &ureq::Agent, url: &str, body: Value) -> (u16, Value) {
    let mut r = a.post(url).header("content-type", "application/json").send(body.to_string()).unwrap();
    let s = r.status().as_u16();
    (s, serde_json::from_slice(&r.body_mut().read_to_vec().unwrap()).unwrap_or(Value::Null))
}

#[test]
fn lan_sharing_is_optional_locked_and_revocable() {
    let dir = tempfile::tempdir().unwrap();
    let music = dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let app = App::new(Store::open(Config::new(&music, dir.path().join("data"))), String::new());
    let id = app.link.id.clone();
    // 앱처럼 이 기기 안에만 연다.
    let base = start(app, "127.0.0.1:0");
    let a = agent();

    let (_, info) = get(&a, &format!("{base}/api/link"), None);
    assert_eq!(info["mode"], "off", "기본은 꺼짐: {info}");
    assert_eq!(info["local"], true);

    // 다른 출처의 화면이 "소리인가"를 물을 수 있다.
    let r = a.get(&format!("{base}/api/hello")).call().unwrap();
    assert_eq!(r.headers().get("access-control-allow-origin").unwrap(), "*");

    // 켠다: LAN 포트, PIN
    let (s, on) = post(&a, &format!("{base}/api/link/lan"), json!({ "enabled": true }));
    assert_eq!(s, 200, "{on}");
    assert_eq!(on["mode"], "on");
    let port = on["port"].as_u64().unwrap();
    let pin = on["pin"].as_str().unwrap().to_string();
    assert_eq!(pin.len(), 6);
    let lan = format!("http://127.0.0.1:{port}");

    // LAN 쪽은 PIN 없이는 아무것도 못 본다. hello는 잠겼다고 답한다.
    assert_eq!(get(&a, &format!("{lan}/api/library"), None).0, 401);
    let (_, hello) = get(&a, &format!("{lan}/api/hello"), None);
    assert_eq!(hello["locked"], true);
    assert_eq!(hello["id"], id.as_str());

    // 틀린 PIN은 돌려보내고, 맞는 PIN이면 쿠키를 준다.
    let wrong = a.post(&format!("{lan}/api/login")).send_form([("password", "000000x"), ("next", "/")]).unwrap();
    assert!(wrong.headers().get("set-cookie").is_none());
    let ok = a.post(&format!("{lan}/api/login")).send_form([("password", pin.as_str()), ("next", "/")]).unwrap();
    let cookie = ok.headers().get("set-cookie").unwrap().to_str().unwrap().split(';').next().unwrap().to_string();
    let (s, libinfo) = get(&a, &format!("{lan}/api/library"), Some(&cookie));
    assert_eq!(s, 200);
    assert!(libinfo["deviceName"].is_string());

    // LAN 쪽에서는 PIN을 볼 수도, 공유를 끌 수도 없다.
    let (_, remote_info) = get(&a, &format!("{lan}/api/link"), Some(&cookie));
    assert_eq!(remote_info["local"], false);
    assert!(remote_info["pin"].is_null());
    let mut r = a
        .post(&format!("{lan}/api/link/lan"))
        .header("cookie", &cookie)
        .header("content-type", "application/json")
        .send(r#"{"enabled":false}"#)
        .unwrap();
    assert_eq!(r.status().as_u16(), 403, "{}", r.body_mut().read_to_string().unwrap());

    // 찾기: 다른 소리가 물으면 답한다(나 자신을 묻는 쪽은 자기 답을 뺀다).
    let found = link::discover("someone-else", Duration::from_millis(900));
    assert!(found.iter().any(|f| f.id == id && f.locked && f.url.ends_with(&format!(":{port}"))), "{found:?}");
    assert!(link::discover(&id, Duration::from_millis(500)).iter().all(|f| f.id != id));

    // 열려 있는 이벤트 스트림(SSE)을 하나 붙잡아 둔다.
    let sse = a.get(&format!("{lan}/api/events")).header("cookie", &cookie).call().unwrap();
    let mut body = sse.into_body().into_reader();
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; 256];
        let t = Instant::now();
        while let Ok(n) = body.read(&mut buf) {
            if n == 0 {
                return Some(t.elapsed());
            }
        }
        Some(t.elapsed())
    });

    // 끈다: 새 연결도, 열려 있던 스트림도 끝난다.
    std::thread::sleep(Duration::from_millis(200));
    let (_, off) = post(&a, &format!("{base}/api/link/lan"), json!({ "enabled": false }));
    assert_eq!(off["mode"], "off");
    let waited = reader.join().unwrap().unwrap();
    assert!(waited < Duration::from_secs(5), "스트림이 끊기지 않았습니다");
    std::thread::sleep(Duration::from_millis(200));
    assert!(a.get(&format!("{lan}/api/hello")).call().map(|r| r.status().as_u16() != 200).unwrap_or(true), "끈 뒤에도 열려 있습니다");

    // 기억한 주소
    let (_, saved) = post(&a, &format!("{base}/api/link/saved"), json!({ "name": "거실 PC", "url": "192.168.0.10:4390" }));
    assert_eq!(saved["saved"][0], json!({ "name": "거실 PC", "url": "http://192.168.0.10:4390" }));
    let (s, _) = post(&a, &format!("{base}/api/link/saved"), json!({ "url": "http://a b" }));
    assert_eq!(s, 400);
    let mut r = a.delete(&format!("{base}/api/link/saved?url=http%3A%2F%2F192.168.0.10%3A4390")).call().unwrap();
    let after: Value = serde_json::from_slice(&r.body_mut().read_to_vec().unwrap()).unwrap();
    assert_eq!(after["saved"], json!([]));
}

#[test]
fn settings_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let music = dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let data = dir.path().join("data");
    let l = link::Link::open(&data);
    assert_eq!(l.mode(), link::Mode::Off);
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("link.json"), r#"{"lan":false,"pin":"123456","saved":[{"name":"PC","url":"http://10.0.0.2:4390"}]}"#).unwrap();
    let l = link::Link::open(&data);
    assert_eq!(l.saved()[0].name, "PC");
    let _ = Arc::new(l);
}
