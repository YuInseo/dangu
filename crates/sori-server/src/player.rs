//! 네이티브 존(이 기기의 오디오 장치)을 조종하는 API. 화면은 리모컨처럼 명령을 보내고,
//! 상태는 SSE로 받는다. 같은 와이파이의 휴대폰에서 컴퓨터의 소리를 조종할 수도 있다.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sori_core::player::{PlayItem, Status};

use crate::App;

/// 엔진 밖에서 나는 소리(화면의 유튜브 영상). 화면이 알려 두면 알림·잠금 화면이 그것을 보이고,
/// 그 단추(재생·일시정지·다음·이전)는 명령 번호를 올려 화면으로 돌려보낸다.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct External {
    pub title: String,
    pub artist: String,
    /// 그림 주소(https)
    pub cover: Option<String>,
    pub playing: bool,
    /// 알림 단추에서 온 마지막 명령과 그 번호. 화면은 번호가 바뀌면 따른다.
    pub cmd: Option<String>,
    pub cmd_seq: u64,
    /// 알린 화면의 표(그 화면의 상태 흐름이 모두 닫히면 지운다). 밖으로는 내보내지 않는다.
    #[serde(skip_serializing)]
    pub owner: String,
}

#[derive(Default)]
pub struct ExternalSlot {
    now: Mutex<Option<External>>,
    /// 화면(표)마다 열려 있는 상태 흐름의 수
    open: Mutex<HashMap<String, usize>>,
    version: AtomicU64,
    /// 알림 단추 명령의 번호. 영상이 지워졌다가 다시 걸려도 거꾸로 가지 않는다.
    seq: AtomicU64,
}

impl ExternalSlot {
    pub fn get(&self) -> Option<External> {
        self.now.lock().clone()
    }

    /// 바뀔 때마다 오른다(상태 흐름이 알아채게)
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// 화면이 알린다. 알림 단추의 명령 번호는 그대로 둔다.
    pub fn set(&self, mut e: External) {
        e.title = e.title.chars().take(200).collect();
        e.artist = e.artist.chars().take(200).collect();
        e.cover = e.cover.filter(|c| c.starts_with("https://") && c.len() < 1000);
        e.owner = e.owner.chars().take(64).collect();
        let mut g = self.now.lock();
        match g.as_ref() {
            Some(old) => (e.cmd, e.cmd_seq) = (old.cmd.clone(), old.cmd_seq),
            None => (e.cmd, e.cmd_seq) = (None, self.seq.load(Ordering::Acquire)),
        }
        if g.as_ref() != Some(&e) {
            *g = Some(e);
            self.version.fetch_add(1, Ordering::AcqRel);
        }
    }

    /// `owner`가 있으면 그 화면이 건 것만 지운다(늦게 온 옛 화면의 지우기가 새 화면의 것을 지우지 않게).
    pub fn clear(&self, owner: Option<&str>) {
        let mut g = self.now.lock();
        if g.as_ref().is_some_and(|e| owner.is_none_or(|o| e.owner == o)) {
            *g = None;
            self.version.fetch_add(1, Ordering::AcqRel);
        }
    }

    /// 알림 단추의 명령을 화면으로 넘긴다. 바깥 소리가 없으면 false.
    pub fn command(&self, cmd: &str) -> bool {
        let mut g = self.now.lock();
        let Some(e) = g.as_mut() else { return false };
        let cmd = match cmd {
            "toggle" if e.playing => "pause",
            "toggle" => "play",
            c => c,
        };
        e.cmd = Some(cmd.to_string());
        e.cmd_seq = self.seq.fetch_add(1, Ordering::AcqRel) + 1;
        // 화면이 따르기 전에도 알림이 곧바로 바뀌게
        match cmd {
            "play" => e.playing = true,
            "pause" => e.playing = false,
            _ => {}
        }
        drop(g);
        self.version.fetch_add(1, Ordering::AcqRel);
        true
    }

    /// 화면의 상태 흐름이 열려 있는 동안 그 화면이 살아 있다고 본다. 마지막 흐름이 닫히면(화면을 닫았거나
    /// 새로 읽었거나 웹뷰가 사라졌다) 그 화면이 건 영상을 지운다. 알림이 없는 영상을 붙들지 않게.
    pub fn join(self: &Arc<Self>, owner: String) -> Presence {
        *self.open.lock().entry(owner.clone()).or_default() += 1;
        Presence { slot: self.clone(), owner }
    }
}

pub struct Presence {
    slot: Arc<ExternalSlot>,
    owner: String,
}

impl Drop for Presence {
    fn drop(&mut self) {
        let mut open = self.slot.open.lock();
        let gone = match open.get_mut(&self.owner) {
            Some(n) if *n > 1 => {
                *n -= 1;
                false
            }
            _ => {
                open.remove(&self.owner);
                true
            }
        };
        drop(open);
        if gone {
            self.slot.clear(Some(&self.owner));
        }
    }
}

/// 화면의 유튜브 영상이 소리를 내고 있거나, 엔진이 쉬는 동안 영상이 걸려 있으면 알림 단추는 영상으로 간다.
fn external_owns(app: &App, engine: &sori_core::player::Player) -> bool {
    app.external.get().is_some_and(|e| e.playing || !matches!(engine.state().status, Status::Playing | Status::Loading))
}

/// 화면의 표: 짧은 글자·숫자·`-`만
fn valid_owner(o: &str) -> bool {
    !o.is_empty() && o.len() <= 64 && o.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

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
    /// 휴대폰의 알림·잠금 화면·헤드셋 단추에서 왔다. 화면의 영상이 알림을 차지하고 있으면 그리로 간다.
    /// 앱 화면의 단추와 다른 기기의 리모컨은 늘 엔진으로.
    #[serde(default)]
    media: bool,
    #[serde(default)]
    t: f64,
    #[serde(default)]
    volume: Option<f64>,
    #[serde(default)]
    muted: Option<bool>,
}

async fn control(State(app): State<App>, Json(c): Json<Cmd>) -> Response {
    let Some(p) = &app.player else { return unavailable() };
    if c.media
        && matches!(c.cmd.as_str(), "play" | "pause" | "toggle" | "next" | "prev")
        && external_owns(&app, p)
        && app.external.command(&c.cmd)
    {
        return Json(json!({ "ok": true, "external": true })).into_response();
    }
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

/// 화면이 지금 트는 유튜브 영상을 알린다(제목·채널·그림·재생 중인지). 지우면 알림은 엔진으로 돌아간다.
async fn set_external(State(app): State<App>, Json(e): Json<External>) -> Response {
    if !e.owner.is_empty() && !valid_owner(&e.owner) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "owner" }))).into_response();
    }
    app.external.set(e);
    Json(json!({ "ok": true })).into_response()
}

#[derive(Deserialize)]
struct OwnerQuery {
    owner: Option<String>,
}

async fn clear_external(State(app): State<App>, Query(q): Query<OwnerQuery>) -> Json<serde_json::Value> {
    app.external.clear(q.owner.as_deref());
    Json(json!({ "ok": true }))
}

/// 상태가 바뀌거나, 재생 중이면 0.25초마다 보낸다. 화면의 유튜브 영상이 걸려 있으면 `external`로 함께.
/// `owner`: 영상을 거는 화면의 표. 이 흐름이 닫히면 그 화면이 건 영상도 내린다.
async fn events(State(app): State<App>, Query(q): Query<OwnerQuery>) -> Response {
    let Some(p) = app.player.clone() else { return unavailable() };
    let ext = app.external.clone();
    let presence = q.owner.filter(|o| valid_owner(o)).map(|o| ext.join(o));
    let start = (p, ext, presence, (u64::MAX, u64::MAX), std::time::Instant::now());
    let s = stream::unfold(start, |(p, ext, presence, last, at)| async move {
        loop {
            let st = p.state();
            let now = (st.seq, ext.version());
            let moving = st.status == Status::Playing;
            if now != last || (moving && at.elapsed() >= Duration::from_millis(250)) {
                let mut j = serde_json::to_value(&st).unwrap_or_default();
                if let Some(e) = ext.get() {
                    j["external"] = serde_json::to_value(e).unwrap_or_default();
                }
                let ev = SseEvent::default().event("player").data(j.to_string());
                return Some((Ok::<_, Infallible>(ev), (p, ext, presence, now, std::time::Instant::now())));
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
        .route("/api/player/external", post(set_external).delete(clear_external))
}
