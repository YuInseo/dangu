//! 라이브 아카이브: archive.org를 대신 묻는다. 화면(브라우저)이 직접 묻지 않으므로 앱 안에서도,
//! 같은 와이파이의 다른 기기에서도 똑같이 된다. DSP를 걸 때는 받으면서 64비트로 풀어 보낸다.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use sori_core::dsp::chain::valid_sid;
use sori_core::dsp::request::parse_request;
use sori_core::live::{self, Sort};

use crate::App;
use crate::media::{Dsp, pcm_stream};
use crate::saves::job_json;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/live/search", get(search))
        .route("/api/live/show/{id}", get(show))
        .route("/api/live/probe", get(probe))
        .route("/api/live/stream", get(stream))
        .route("/api/live/resolve", post(resolve))
        .route("/api/live/save", post(save_show))
        .route("/api/live/saves", get(saves_list))
        .route("/api/live/saves/{id}", get(save_status).delete(save_dismiss))
        .route("/api/live/saves/{id}/cancel", post(save_cancel))
}

const UNREACHABLE: &str = "archive.org에 닿지 못했습니다";

fn error(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

/// archive.org를 묻는 일은 막히는(blocking) 일이라 따로 돌린다.
async fn ask<T: Send + 'static>(work: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> Result<T, Response> {
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => {
            tracing::warn!("라이브 아카이브: {e:#}");
            Err(error(StatusCode::BAD_GATEWAY, UNREACHABLE))
        }
        Err(_) => Err(error(StatusCode::INTERNAL_SERVER_ERROR, "작업이 멈췄습니다")),
    }
}

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
    sort: Option<String>,
    page: Option<u32>,
    rows: Option<u32>,
}

async fn search(State(app): State<App>, Query(q): Query<SearchQuery>) -> Response {
    let archive = app.live.clone();
    let text = q.q.unwrap_or_default();
    let sort = Sort::parse(q.sort.as_deref().unwrap_or_default());
    let page = q.page.unwrap_or(1).clamp(1, 10_000);
    let rows = q.rows.unwrap_or(48).clamp(1, 100);
    match ask(move || archive.search(text.trim(), sort, page, rows)).await {
        Ok(r) => Json(r.as_ref().clone()).into_response(),
        Err(res) => res,
    }
}

async fn show(State(app): State<App>, Path(id): Path<String>) -> Response {
    if !live::is_valid_id(&id) {
        return error(StatusCode::NOT_FOUND, "공연을 찾을 수 없습니다");
    }
    let archive = app.live.clone();
    match ask(move || archive.show(&id)).await {
        Ok(s) => match s.as_ref() {
            Some(s) => Json(s.clone()).into_response(),
            None => error(StatusCode::NOT_FOUND, "공연을 찾을 수 없습니다"),
        },
        Err(res) => res,
    }
}

#[derive(Deserialize)]
struct FileQuery {
    id: Option<String>,
    file: Option<String>,
}

/// 라이브 아카이브 FLAC의 실제 샘플레이트·비트 깊이. 신호 경로가 묻는다.
async fn probe(State(app): State<App>, Query(q): Query<FileQuery>) -> Response {
    let (id, file) = (q.id.unwrap_or_default(), q.file.unwrap_or_default());
    let archive = app.live.clone();
    match ask(move || archive.probe(&id, &file)).await {
        Ok(Some(info)) => {
            let mut res = Json(info).into_response();
            res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400"));
            res
        }
        Ok(None) => error(StatusCode::NOT_FOUND, "읽을 수 없습니다"),
        Err(res) => res,
    }
}

#[derive(Deserialize)]
struct StreamQuery {
    id: Option<String>,
    file: Option<String>,
    dsp: Option<String>,
    sid: Option<String>,
    t: Option<f64>,
}

/// 라이브 아카이브 곡을 DSP 체인에 태워 보낸다. DSP가 꺼져 있으면 브라우저가 archive.org에서
/// 직접 받으므로 이 길을 쓰지 않는다.
async fn stream(State(app): State<App>, Query(q): Query<StreamQuery>) -> Response {
    let (id, file) = (q.id.unwrap_or_default(), q.file.unwrap_or_default());
    if !live::is_valid_id(&id) || !live::is_valid_file(&file) {
        return (StatusCode::BAD_REQUEST, "잘못된 요청").into_response();
    }
    let Some(req) = q.dsp.as_deref().and_then(parse_request) else {
        return (StatusCode::BAD_REQUEST, "DSP 요청이 없습니다").into_response();
    };
    let sid = q.sid.filter(|s| valid_sid(s));
    let at = q.t.unwrap_or(0.0).max(0.0);
    let archive = app.live.clone();
    let dsp = Dsp { req, loudness: None, sid, app };
    pcm_stream(move || archive.open(&id, &file), at, Some(dsp), StatusCode::BAD_GATEWAY).await
}

// ---------------------------------------------------------------------------
// 링크로 열기, 라이브러리에 저장

const NOT_A_LINK: &str =
    "archive.org 라이브 음악 아카이브의 공연 링크만 열 수 있습니다. 유튜브·타이달 같은 곳의 음원은 저작권 때문에 받지 않습니다";
const NO_SHOW: &str = "공연을 찾을 수 없습니다";

#[derive(Deserialize)]
struct LinkBody {
    url: String,
}

/// 붙여 넣은 링크 → 공연 ID. 라이브 음악 아카이브의 공연만.
async fn resolve(State(app): State<App>, Json(b): Json<LinkBody>) -> Response {
    let Some(id) = live::parse_link(&b.url) else { return error(StatusCode::BAD_REQUEST, NOT_A_LINK) };
    let archive = app.live.clone();
    let lookup = id.clone();
    match ask(move || archive.show(&lookup)).await {
        Ok(s) => match s.as_ref() {
            Some(s) if s.save_block.as_deref() == Some(live::NOT_LMA) => error(StatusCode::UNPROCESSABLE_ENTITY, live::NOT_LMA),
            Some(s) => Json(json!({ "id": id, "saveable": s.saveable, "saveBlock": s.save_block })).into_response(),
            None => error(StatusCode::NOT_FOUND, NO_SHOW),
        },
        Err(res) => res,
    }
}

#[derive(Deserialize)]
struct SaveBody {
    id: String,
}

/// 공연을 라이브러리에 저장한다(받기 목록에 넣는다). 아티스트가 내려받기를 허락한 공연만.
async fn save_show(State(app): State<App>, Json(b): Json<SaveBody>) -> Response {
    if !live::is_valid_id(&b.id) {
        return error(StatusCode::NOT_FOUND, NO_SHOW);
    }
    let archive = app.live.clone();
    let id = b.id.clone();
    let show = match ask(move || archive.show(&id)).await {
        Ok(s) => s,
        Err(res) => return res,
    };
    let Some(show) = show.as_ref().clone() else { return error(StatusCode::NOT_FOUND, NO_SHOW) };
    if !show.saveable {
        return error(StatusCode::FORBIDDEN, show.save_block.as_deref().unwrap_or(live::STREAM_ONLY));
    }
    let a = app.clone();
    let res = tokio::task::spawn_blocking(move || a.saves.enqueue(&show, &a.store.music_dir(), a.live.clone(), a.store.clone())).await;
    match res {
        Ok(Ok(job)) => Json(job_json(&job, &app.store)).into_response(),
        Ok(Err(e)) => error(StatusCode::INTERNAL_SERVER_ERROR, &format!("{e:#}")),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "작업이 멈췄습니다"),
    }
}

async fn saves_list(State(app): State<App>) -> Json<serde_json::Value> {
    let jobs: Vec<_> = app.saves.jobs().iter().map(|j| job_json(j, &app.store)).collect();
    Json(json!({ "jobs": jobs }))
}

/// 이 공연의 저장 상태. 받기 목록에 없어도 전에 다 받아 둔 공연이면 "done".
async fn save_status(State(app): State<App>, Path(id): Path<String>) -> Response {
    if let Some(j) = app.saves.get(&id) {
        return Json(job_json(&j, &app.store)).into_response();
    }
    match app.saves.saved_dir(&id) {
        Some(dir) if dir.is_dir() => Json(json!({
            "id": id,
            "state": "done",
            "dir": dir.to_string_lossy(),
            "albumId": crate::saves::album_of(&app.store, &dir),
        }))
        .into_response(),
        _ => error(StatusCode::NOT_FOUND, "저장하지 않은 공연입니다"),
    }
}

async fn save_cancel(State(app): State<App>, Path(id): Path<String>) -> Response {
    match app.saves.cancel(&id) {
        Some(j) => Json(job_json(&j, &app.store)).into_response(),
        None => error(StatusCode::NOT_FOUND, "받는 중이 아닙니다"),
    }
}

async fn save_dismiss(State(app): State<App>, Path(id): Path<String>) -> Response {
    if app.saves.dismiss(&id) { StatusCode::NO_CONTENT.into_response() } else { error(StatusCode::NOT_FOUND, "목록에 없습니다") }
}
