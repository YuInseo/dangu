//! 소리결 DSP의 부속: 임펄스 응답 올리기, 클리핑 기록, 엔진이 할 수 있는 것.

use axum::extract::{DefaultBodyLimit, Multipart, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use sori_core::dsp::chain::valid_sid;

use crate::App;

async fn list(State(app): State<App>) -> Json<serde_json::Value> {
    Json(json!({ "irs": app.irs.list() }))
}

async fn upload(State(app): State<App>, mut form: Multipart) -> Response {
    loop {
        let field = match form.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("올린 파일을 읽지 못했습니다: {e}") }))).into_response();
            }
        };
        if field.name() != Some("file") {
            continue;
        }
        let name = field.file_name().unwrap_or("ir.wav").to_string();
        let bytes = match field.bytes().await {
            Ok(b) => b,
            Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
        };
        let irs = app.irs.clone();
        let saved = tokio::task::spawn_blocking(move || irs.save(&name, &bytes)).await;
        return match saved {
            Ok(Ok(ir)) => Json(json!({ "ir": ir })).into_response(),
            Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "저장하지 못했습니다" }))).into_response(),
        };
    }
    (StatusCode::BAD_REQUEST, Json(json!({ "error": "file 필드가 없습니다" }))).into_response()
}

#[derive(Deserialize)]
struct IdQuery {
    id: String,
}

async fn remove(State(app): State<App>, Query(q): Query<IdQuery>) -> Response {
    if app.irs.delete(&q.id) {
        Json(json!({ "ok": true })).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "없는 임펄스 응답입니다" }))).into_response()
    }
}

#[derive(Deserialize)]
struct SidQuery {
    sid: String,
}

async fn clips(State(app): State<App>, Query(q): Query<SidQuery>) -> Response {
    if !valid_sid(&q.sid) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "잘못된 세션" }))).into_response();
    }
    Json(app.clips.get(&q.sid)).into_response()
}

/// 소리 엔진은 DSP를 전부 직접(64비트) 한다. 화면의 신호 경로 표시가 쓴다.
async fn caps() -> Json<serde_json::Value> {
    Json(json!({ "soxr": true, "bs2b": true, "afir": true, "ebur128": true, "ffmpeg": true, "engine": "sori" }))
}

pub fn routes() -> Router<App> {
    Router::new()
        .route(
            "/api/dsp/ir",
            get(list).post(upload).delete(remove).layer(DefaultBodyLimit::max(sori_core::dsp::ir::IR_MAX_BYTES + 1024 * 1024)),
        )
        .route("/api/dsp/clips", get(clips))
        .route("/api/dsp/caps", get(caps))
}
