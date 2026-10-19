//! 화면의 유튜브 영상(엔진 밖 소리): 알림·잠금 화면의 단추가 화면으로 돌아가고, 상태 흐름에 함께 실린다.

use std::io::{BufRead, BufReader};

use serde_json::{Value, json};
use sori_core::player::output::NullOutput;
use sori_core::{Config, Store};
use sori_server::App;

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

fn post(url: &str, body: Value) -> Value {
    let mut r = client().post(url).header("content-type", "application/json").send(body.to_string()).unwrap();
    serde_json::from_slice(&r.body_mut().read_to_vec().unwrap()).unwrap_or(Value::Null)
}

/// 상태 흐름(SSE)을 열면 곧바로 오는 지금 상태
fn state(base: &str) -> Value {
    let r = client().get(&format!("{base}/api/player/events")).call().unwrap();
    let lines = BufReader::new(r.into_body().into_reader()).lines();
    for line in lines {
        let line = line.unwrap();
        if let Some(d) = line.strip_prefix("data:") {
            return serde_json::from_str(d.trim()).unwrap();
        }
    }
    panic!("상태가 오지 않았다");
}

fn app() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let music = dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let store = Store::open(Config::new(&music, dir.path().join("data")));
    let app = App::new(store, String::new()).with_output(Box::new(|| Box::new(NullOutput::new(48_000, false))));
    (dir, start(app))
}

#[test]
fn external_sound_takes_the_buttons() {
    let (_dir, base) = app();
    let control = |body: Value| post(&format!("{base}/api/player/control"), body);
    let external = |body: Value| post(&format!("{base}/api/player/external"), body);

    // 아무것도 걸려 있지 않으면 단추는 엔진으로
    assert_eq!(control(json!({ "cmd": "pause", "media": true }))["external"], Value::Null);
    assert!(state(&base).get("external").is_none());

    // 화면이 유튜브 영상을 튼다고 알린다
    external(json!({ "title": "이 별로부터", "artist": "Yebit 예빛", "cover": "https://i.ytimg.com/vi/x/hqdefault.jpg", "playing": true }));
    let st = state(&base);
    assert_eq!(
        (st["external"]["title"].as_str(), st["external"]["playing"].as_bool(), st["external"]["cmdSeq"].as_u64()),
        (Some("이 별로부터"), Some(true), Some(0)),
        "{st}"
    );

    // 앱 화면의 단추(그리고 다른 기기의 리모컨)는 늘 엔진으로
    assert_eq!(control(json!({ "cmd": "pause" }))["external"], Value::Null);
    assert_eq!(state(&base)["external"]["cmdSeq"], 0);

    // 알림의 재생·일시정지 → 화면으로(번호가 오르고, 알림은 곧바로 멈춤으로)
    assert_eq!(control(json!({ "cmd": "toggle", "media": true }))["external"], true);
    let st = state(&base);
    assert_eq!(
        (st["external"]["cmd"].as_str(), st["external"]["cmdSeq"].as_u64(), st["external"]["playing"].as_bool()),
        (Some("pause"), Some(1), Some(false))
    );

    // 화면이 멈췄다고 다시 알려도 명령 번호는 그대로
    external(json!({ "title": "이 별로부터", "artist": "Yebit 예빛", "playing": false }));
    assert_eq!(state(&base)["external"]["cmdSeq"], 1);

    // 다음 곡 단추(헤드셋)도 화면으로
    control(json!({ "cmd": "next", "media": true }));
    assert_eq!(state(&base)["external"]["cmd"], "next");

    // 그림 주소는 https만, 제목은 200자까지. 화면의 표는 밖으로 내보내지 않는다.
    external(json!({ "title": "가".repeat(500), "cover": "http://example.com/x.jpg", "playing": false, "owner": "page-1" }));
    let st = state(&base);
    assert_eq!(st["external"]["title"].as_str().unwrap().chars().count(), 200);
    assert_eq!(st["external"]["cover"], Value::Null);
    assert!(st["external"].get("owner").is_none(), "{st}");

    // 다른 화면의 늦은 지우기는 지금 것을 지우지 않는다
    let del = |q: &str| {
        let mut r = client().delete(&format!("{base}/api/player/external{q}")).call().unwrap();
        assert_eq!(r.status().as_u16(), 200, "{}", r.body_mut().read_to_string().unwrap_or_default());
    };
    del("?owner=page-0");
    assert!(state(&base).get("external").is_some());
    del("?owner=page-1");
    assert!(state(&base).get("external").is_none());
    assert_eq!(control(json!({ "cmd": "toggle", "media": true }))["external"], Value::Null);

    // 다시 걸려도 명령 번호는 거꾸로 가지 않는다(화면은 처음 본 번호까지를 지난 것으로 친다)
    external(json!({ "title": "다시", "playing": true }));
    assert_eq!(state(&base)["external"]["cmdSeq"], 2);
    control(json!({ "cmd": "pause", "media": true }));
    assert_eq!(state(&base)["external"]["cmdSeq"], 3);

    // 이상한 표는 받지 않는다
    let mut r = client()
        .post(&format!("{base}/api/player/external"))
        .header("content-type", "application/json")
        .send(json!({ "title": "x", "owner": "a b/../c" }).to_string())
        .unwrap();
    assert_eq!(r.status().as_u16(), 400, "{}", r.body_mut().read_to_string().unwrap_or_default());
}

/// 영상을 건 화면이 사라지면(상태 흐름이 닫히면) 알림도 그 영상을 내려놓는다.
#[test]
fn external_goes_with_its_page() {
    let (_dir, base) = app();
    let external = |body: Value| post(&format!("{base}/api/player/external"), body);
    let open = || {
        let r = client().get(&format!("{base}/api/player/events?owner=page-7")).call().unwrap();
        let mut lines = BufReader::new(r.into_body().into_reader()).lines();
        // 첫 상태가 오면 흐름이 열린 것
        while !lines.next().unwrap().unwrap().starts_with("data:") {}
        lines
    };
    let gone_within = |secs: u64| {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        while std::time::Instant::now() < until {
            if state(&base).get("external").is_none() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        false
    };

    // 같은 화면의 흐름이 둘(다시 붙는 중): 하나가 닫혀도 남는다
    let first = open();
    let second = open();
    external(json!({ "title": "배경에서", "playing": true, "owner": "page-7" }));
    assert_eq!(state(&base)["external"]["title"], "배경에서");
    drop(first);
    assert!(!gone_within(2), "흐름이 남았는데 지워졌다");

    // 마지막 흐름이 닫히면 지운다
    drop(second);
    assert!(gone_within(20), "화면이 사라졌는데 알림이 남았다");

    // 표 없이 건 것(옛 화면)은 흐름과 상관없이 남는다
    let third = open();
    external(json!({ "title": "표 없음", "playing": false }));
    drop(third);
    assert!(!gone_within(2));
}
