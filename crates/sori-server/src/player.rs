//! 네이티브 존(이 기기의 오디오 장치)을 조종하는 API. 화면은 리모컨처럼 명령을 보내고,
//! 상태는 SSE로 받는다. 같은 와이파이의 휴대폰에서 컴퓨터의 소리를 조종할 수도 있다.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream;
use serde::Deserialize;
use serde_json::json;
use sori_core::player::{PlayItem, Status};

use crate::App;

fn unavailable() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "이 서버에는 직접 트는 오디오 장치가 없습니다" }))).into_response()
}

async fn info(State(app): State<App>) -> Json<serde_json::Value> {
    match &app.player {
        Some(p) => Json(json!({ "available": true, "state": p.state() })),
        None => Json(json!({ "available": false })),
    }
}

#[derive(Deserialize)]
struct LoadBody {
    item: PlayItem,
    #[serde(default)]
    autoplay: bool,
    #[serde(default)]
    at: f64,
    #[serde(default)]
    force: bool,
}

async fn load(State(app): State<App>, Json(b): Json<LoadBody>) -> Response {
    let Some(p) = &app.player else { return unavailable() };
    p.load(b.item, b.autoplay, b.at.max(0.0), b.force);
    Json(json!({ "ok": true })).into_response()
}

#[derive(Deserialize)]
struct PreloadBody {
    item: Option<PlayItem>,
}

async fn preload(State(app): State<App>, Json(b): Json<PreloadBody>) -> Response {
    let Some(p) = &app.player else { return unavailable() };
    p.preload(b.item);
    Json(json!({ "ok": true })).into_response()
}

#[derive(Deserialize)]
struct Cmd {
    cmd: String,
    #[serde(default)]
    t: f64,
    #[serde(default)]
    volume: Option<f64>,
    #[serde(default)]
    muted: Option<bool>,
}

async fn control(State(app): State<App>, Json(c): Json<Cmd>) -> Response {
    let Some(p) = &app.player else { return unavailable() };
    match c.cmd.as_str() {
        "play" => p.play(),
        "pause" => p.pause(),
        "toggle" => {
            if p.state().status == Status::Playing || p.state().status == Status::Loading {
                p.pause()
            } else {
                p.play()
            }
        }
        "stop" => p.stop(),
        // 알림·잠금 화면·헤드셋 단추: 엔진이 들고 있는 대기열로 간다.
        "next" => p.next(),
        "prev" => p.prev(),
        "seek" => p.seek(c.t.max(0.0)),
        "volume" => {
            let s = p.state();
            p.set_volume(c.volume.unwrap_or(s.volume), c.muted.unwrap_or(s.muted));
        }
        other => return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("모르는 명령: {other}") }))).into_response(),
    }
    Json(json!({ "ok": true })).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueueBody {
    #[serde(default)]
    upcoming: Vec<PlayItem>,
    #[serde(default)]
    previous: Vec<PlayItem>,
    #[serde(default)]
    repeat_one: bool,
}

/// 다음에 틀 곡들과 앞서 튼 곡들. 화면이 잠들어도 엔진이 혼자 이어 튼다.
async fn queue(State(app): State<App>, Json(b): Json<QueueBody>) -> Response {
    let Some(p) = &app.player else { return unavailable() };
    p.set_queue(b.upcoming, b.previous, b.repeat_one);
    Json(json!({ "ok": true })).into_response()
}

#[derive(Deserialize)]
struct DspItem {
    uid: String,
    dsp: Option<String>,
}

#[derive(Deserialize)]
struct DspBody {
    items: Vec<DspItem>,
}

/// 곡마다의 DSP 요청을 바꾼다(설정을 고쳤을 때). 지금 곡은 지금 위치에서 바로 다시 건다.
async fn dsp(State(app): State<App>, Json(b): Json<DspBody>) -> Response {
    let Some(p) = &app.player else { return unavailable() };
    p.set_dsp(b.items.into_iter().map(|i| (i.uid, i.dsp)).collect());
    Json(json!({ "ok": true })).into_response()
}

/// 상태가 바뀌거나, 재생 중이면 0.25초마다 보낸다.
async fn events(State(app): State<App>) -> Response {
    let Some(p) = app.player.clone() else { return unavailable() };
    let s = stream::unfold((p, u64::MAX, std::time::Instant::now()), |(p, last, at)| async move {
        loop {
            let st = p.state();
            let moving = st.status == Status::Playing;
            if st.seq != last || (moving && at.elapsed() >= Duration::from_millis(250)) {
                let ev = SseEvent::default().event("player").data(serde_json::to_string(&st).unwrap_or_default());
                return Some((Ok::<_, Infallible>(ev), (p, st.seq, std::time::Instant::now())));
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    });
    Sse::new(s).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))).into_response()
}

pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/player", get(info))
        .route("/api/player/load", post(load))
        .route("/api/player/preload", post(preload))
        .route("/api/player/queue", post(queue))
        .route("/api/player/control", post(control))
        .route("/api/player/dsp", post(dsp))
        .route("/api/player/events", get(events))
}
