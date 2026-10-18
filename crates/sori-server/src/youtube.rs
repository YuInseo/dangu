//! 유튜브: 검색·영상·댓글·채널·재생목록(API 키), 구독과 구독 피드(키 없이 RSS), 링크 열기.
//! 영상은 화면이 유튜브 공식 플레이어로 튼다. 여기서는 영상이나 소리를 받지 않는다.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use sori_core::youtube::client::{ApiError, NeedKey};
use sori_core::youtube::{self, Subscription};

use crate::App;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/youtube", get(status))
        .route("/api/youtube/key", post(set_key).delete(clear_key))
        .route("/api/youtube/popular", get(popular))
        .route("/api/youtube/search", get(search))
        .route("/api/youtube/video/{id}", get(video))
        .route("/api/youtube/comments/{id}", get(comments))
        .route("/api/youtube/replies/{id}", get(replies))
        .route("/api/youtube/channel/{id}", get(channel))
        .route("/api/youtube/channel/{id}/videos", get(channel_videos))
        .route("/api/youtube/channel/{id}/playlists", get(channel_playlists))
        .route("/api/youtube/playlist/{id}", get(playlist))
        .route("/api/youtube/resolve", post(resolve))
        .route("/api/youtube/subscriptions", get(subscriptions).post(subscribe))
        .route("/api/youtube/subscriptions/import", post(import))
        .route("/api/youtube/subscriptions/{id}", delete(unsubscribe))
        .route("/api/youtube/feed", get(feed))
}

fn error(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

/// 막히는 일을 따로 돌리고, 오류를 화면이 알아듣게 옮긴다(키 없음 412, 유튜브가 거절 그 상태, 닿지 못함 502).
async fn ask<T: Serialize + Send + Sync + 'static>(work: impl FnOnce() -> anyhow::Result<Arc<T>> + Send + 'static) -> Response {
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(v)) => Json(v.as_ref()).into_response(),
        Ok(Err(e)) => {
            if e.is::<NeedKey>() {
                return (StatusCode::PRECONDITION_FAILED, Json(json!({ "error": e.to_string(), "needKey": true }))).into_response();
            }
            if let Some(a) = e.downcast_ref::<ApiError>() {
                let status = match a.status {
                    400 => StatusCode::BAD_REQUEST,
                    403 => StatusCode::FORBIDDEN,
                    404 => StatusCode::NOT_FOUND,
                    429 => StatusCode::TOO_MANY_REQUESTS,
                    _ => StatusCode::BAD_GATEWAY,
                };
                return error(status, &a.message);
            }
            tracing::warn!("유튜브: {e:#}");
            error(StatusCode::BAD_GATEWAY, &e.to_string())
        }
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "작업이 멈췄습니다"),
    }
}

async fn status(State(app): State<App>) -> Json<serde_json::Value> {
    let yt = &app.youtube;
    Json(json!({ "hasKey": yt.has_key(), "keyHint": yt.key_hint(), "subscriptions": yt.subscriptions().len() }))
}

#[derive(Deserialize)]
struct KeyBody {
    key: String,
}

async fn set_key(State(app): State<App>, Json(b): Json<KeyBody>) -> Response {
    let yt = app.youtube.clone();
    match tokio::task::spawn_blocking(move || yt.set_key(&b.key)).await {
        Ok(Ok(())) => Json(json!({ "hasKey": true, "keyHint": app.youtube.key_hint() })).into_response(),
        Ok(Err(e)) => error(StatusCode::BAD_REQUEST, &e.to_string()),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "작업이 멈췄습니다"),
    }
}

async fn clear_key(State(app): State<App>) -> Json<serde_json::Value> {
    app.youtube.clear_key();
    Json(json!({ "hasKey": app.youtube.has_key() }))
}

#[derive(Deserialize)]
struct PageQuery {
    page: Option<String>,
    region: Option<String>,
    q: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    order: Option<String>,
}

impl PageQuery {
    fn page(&self) -> Option<String> {
        self.page.clone().filter(|p| !p.is_empty() && p.len() < 200)
    }
    fn region(&self) -> String {
        self.region.clone().unwrap_or_else(|| "KR".into())
    }
}

async fn popular(State(app): State<App>, Query(q): Query<PageQuery>) -> Response {
    let yt = app.youtube.clone();
    ask(move || yt.popular(&q.region(), q.page().as_deref())).await
}

async fn search(State(app): State<App>, Query(q): Query<PageQuery>) -> Response {
    let text = q.q.clone().unwrap_or_default();
    if text.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "찾을 말을 넣어 주세요");
    }
    let yt = app.youtube.clone();
    ask(move || yt.search(&text, q.kind.as_deref().unwrap_or(""), q.page().as_deref(), &q.region())).await
}

async fn video(State(app): State<App>, Path(id): Path<String>) -> Response {
    let yt = app.youtube.clone();
    let subscribed = app.youtube.clone();
    match tokio::task::spawn_blocking(move || yt.video(&id)).await {
        Ok(Ok(v)) => {
            let mut j = serde_json::to_value(v.as_ref()).unwrap_or_default();
            if let Some(cid) = v.channel.as_ref().map(|c| c.id.clone()) {
                j["subscribed"] = json!(subscribed.is_subscribed(&cid));
            }
            Json(j).into_response()
        }
        Ok(Err(e)) => ask(move || Err::<Arc<()>, _>(e)).await,
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "작업이 멈췄습니다"),
    }
}

async fn comments(State(app): State<App>, Path(id): Path<String>, Query(q): Query<PageQuery>) -> Response {
    let yt = app.youtube.clone();
    ask(move || yt.comments(&id, q.order.as_deref().unwrap_or("relevance"), q.page().as_deref())).await
}

async fn replies(State(app): State<App>, Path(id): Path<String>, Query(q): Query<PageQuery>) -> Response {
    let yt = app.youtube.clone();
    ask(move || yt.replies(&id, q.page().as_deref())).await
}

async fn channel(State(app): State<App>, Path(id): Path<String>) -> Response {
    let yt = app.youtube.clone();
    let check = app.youtube.clone();
    let cid = id.clone();
    match tokio::task::spawn_blocking(move || yt.channel(&id)).await {
        Ok(Ok(c)) => {
            let mut j = serde_json::to_value(c.as_ref()).unwrap_or_default();
            j["subscribed"] = json!(check.is_subscribed(&cid));
            Json(j).into_response()
        }
        Ok(Err(e)) => ask(move || Err::<Arc<()>, _>(e)).await,
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "작업이 멈췄습니다"),
    }
}

/// 채널의 영상 다음 쪽. `uploads`(UU…)를 모르면 채널 ID의 UC를 UU로 바꾼 것이 올린 영상 재생목록이다.
async fn channel_videos(State(app): State<App>, Path(id): Path<String>, Query(q): Query<PageQuery>) -> Response {
    if !youtube::is_channel_id(&id) {
        return error(StatusCode::BAD_REQUEST, "채널 주소가 이상합니다");
    }
    let uploads = format!("UU{}", &id[2..]);
    let yt = app.youtube.clone();
    ask(move || yt.channel_videos(&uploads, q.page().as_deref())).await
}

async fn channel_playlists(State(app): State<App>, Path(id): Path<String>, Query(q): Query<PageQuery>) -> Response {
    let yt = app.youtube.clone();
    ask(move || yt.channel_playlists(&id, q.page().as_deref())).await
}

async fn playlist(State(app): State<App>, Path(id): Path<String>, Query(q): Query<PageQuery>) -> Response {
    let yt = app.youtube.clone();
    ask(move || yt.playlist(&id, q.page().as_deref())).await
}

#[derive(Deserialize)]
struct LinkBody {
    url: String,
}

/// 붙여 넣은 유튜브 링크 → 영상·채널·재생목록. @핸들은 API 키가 있어야 채널을 찾는다.
async fn resolve(State(app): State<App>, Json(b): Json<LinkBody>) -> Response {
    let Some(link) = youtube::parse_link(&b.url) else {
        return error(StatusCode::BAD_REQUEST, "유튜브 영상·채널·재생목록 링크가 아닙니다");
    };
    let yt = app.youtube.clone();
    ask(move || yt.resolve(link).map(Arc::new)).await
}

async fn subscriptions(State(app): State<App>) -> Json<Vec<Subscription>> {
    Json(app.youtube.subscriptions())
}

#[derive(Deserialize)]
struct SubscribeBody {
    id: String,
    #[serde(default)]
    title: String,
    thumb: Option<String>,
}

async fn subscribe(State(app): State<App>, Json(b): Json<SubscribeBody>) -> Response {
    if !youtube::is_channel_id(&b.id) {
        return error(StatusCode::BAD_REQUEST, "채널 주소가 이상합니다");
    }
    let thumb = b.thumb.filter(|t| t.starts_with("https://") && t.len() < 1000);
    let title: String = b.title.trim().chars().take(200).collect();
    let fresh = app.youtube.subscribe(Subscription { id: b.id, title, thumb, added_at: 0 });
    Json(json!({ "subscribed": true, "new": fresh, "count": app.youtube.subscriptions().len() })).into_response()
}

async fn unsubscribe(State(app): State<App>, Path(id): Path<String>) -> Json<serde_json::Value> {
    let removed = app.youtube.unsubscribe(&id);
    Json(json!({ "subscribed": false, "removed": removed, "count": app.youtube.subscriptions().len() }))
}

/// 구글 테이크아웃의 subscriptions.csv 본문
async fn import(State(app): State<App>, body: String) -> Response {
    if body.len() > 4 << 20 {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "파일이 너무 큽니다");
    }
    let list = youtube::parse_takeout_csv(&body);
    if list.is_empty() {
        return error(StatusCode::BAD_REQUEST, "구독 목록을 찾지 못했습니다. 테이크아웃의 subscriptions.csv인지 확인해 주세요");
    }
    let found = list.len();
    let added = app.youtube.import(list);
    Json(json!({ "found": found, "added": added, "count": app.youtube.subscriptions().len() })).into_response()
}

#[derive(Deserialize)]
struct FeedQuery {
    page: Option<usize>,
}

/// 구독한 채널의 새 영상(날짜순). 키 없이 된다.
async fn feed(State(app): State<App>, Query(q): Query<FeedQuery>) -> Response {
    let yt = app.youtube.clone();
    let page = q.page.unwrap_or(0).min(100);
    ask(move || Ok(Arc::new(yt.subscription_feed(page, 30)))).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn link_kinds_are_tagged() {
        let j = serde_json::to_value(sori_core::youtube::Link::Video("ArmDp-zijuc".into())).unwrap();
        assert_eq!(j, serde_json::json!({ "kind": "video", "id": "ArmDp-zijuc" }));
        let j = serde_json::to_value(sori_core::youtube::Link::Channel("UCMki_UkHb4qSc0qyEcOHHJw".into())).unwrap();
        assert_eq!(j["kind"], "channel");
    }
}
