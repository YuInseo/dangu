//! 좋아요와 플레이리스트 API. 저장은 코어(`sori_core::collections`)가 하고, 여기서는 ID를
//! 라이브러리의 곡·앨범으로 바꿔 화면이 바로 그릴 수 있게 준다.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use sori_core::collections::{Playlist, valid_id};
use sori_core::library::Library;
use sori_core::model::{AlbumCard, Track};

use crate::App;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/favorites", get(favorites))
        .route("/api/favorites/ids", get(favorite_ids))
        .route("/api/favorites/tracks/{id}", put(like_track).delete(unlike_track))
        .route("/api/favorites/albums/{id}", put(like_album).delete(unlike_album))
        .route("/api/playlists", get(playlists).post(create))
        .route("/api/playlists/{id}", get(playlist).patch(update).delete(remove))
        .route("/api/playlists/{id}/tracks", post(add))
}

fn bad(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
}

fn missing() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "플레이리스트를 찾을 수 없습니다" }))).into_response()
}

fn tracks_of(lib: &Library, ids: &[String]) -> Vec<Track> {
    ids.iter().filter_map(|id| lib.views.get(id).cloned()).collect()
}

/// 목록에 보일 모양: 곡 수, 길이, 표지 넷(서로 다른 앨범)
fn summary(lib: &Library, p: &Playlist) -> Value {
    let tracks = tracks_of(lib, &p.tracks);
    let duration: f64 = tracks.iter().map(|t| t.duration).sum();
    let mut covers: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for t in &tracks {
        if covers.len() >= 4 {
            break;
        }
        if seen.insert(t.album_id.clone()) {
            covers.push(json!({ "albumId": t.album_id, "cover": t.cover, "title": t.album, "artist": t.album_artist }));
        }
    }
    json!({
        "id": p.id,
        "name": p.name,
        "created": p.created,
        "updated": p.updated,
        "count": tracks.len(),
        "duration": duration,
        "covers": covers,
    })
}

async fn favorites(State(app): State<App>) -> Json<Value> {
    let lib = app.store.library();
    let tracks: Vec<Track> = app.collections.liked_tracks().iter().filter_map(|l| lib.views.get(&l.id).cloned()).collect();
    let albums: Vec<AlbumCard> = app.collections.liked_albums().iter().filter_map(|l| lib.albums.get(&l.id).map(AlbumCard::from)).collect();
    Json(json!({ "tracks": tracks, "albums": albums }))
}

async fn favorite_ids(State(app): State<App>) -> Json<Value> {
    let ids = |v: Vec<sori_core::collections::Liked>| v.into_iter().map(|l| l.id).collect::<Vec<_>>();
    Json(json!({
        "tracks": ids(app.collections.liked_tracks()),
        "albums": ids(app.collections.liked_albums()),
        "generation": app.collections.generation(),
    }))
}

fn like(app: &App, id: &str, album: bool, on: bool) -> Response {
    if !valid_id(id) {
        return bad("이상한 ID입니다");
    }
    if album {
        app.collections.like_album(id, on);
    } else {
        app.collections.like_track(id, on);
    }
    Json(json!({ "liked": on })).into_response()
}

async fn like_track(State(app): State<App>, Path(id): Path<String>) -> Response {
    like(&app, &id, false, true)
}
async fn unlike_track(State(app): State<App>, Path(id): Path<String>) -> Response {
    like(&app, &id, false, false)
}
async fn like_album(State(app): State<App>, Path(id): Path<String>) -> Response {
    like(&app, &id, true, true)
}
async fn unlike_album(State(app): State<App>, Path(id): Path<String>) -> Response {
    like(&app, &id, true, false)
}

async fn playlists(State(app): State<App>) -> Json<Value> {
    let lib = app.store.library();
    Json(Value::Array(app.collections.playlists().iter().map(|p| summary(&lib, p)).collect()))
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
    #[serde(default)]
    tracks: Vec<String>,
}

async fn create(State(app): State<App>, Json(b): Json<CreateBody>) -> Response {
    match app.collections.create_playlist(&b.name, b.tracks) {
        Ok(p) => (StatusCode::CREATED, Json(summary(&app.store.library(), &p))).into_response(),
        Err(e) => bad(&e.to_string()),
    }
}

async fn playlist(State(app): State<App>, Path(id): Path<String>) -> Response {
    let Some(p) = app.collections.playlist(&id) else { return missing() };
    let lib = app.store.library();
    let mut v = summary(&lib, &p);
    let tracks = tracks_of(&lib, &p.tracks);
    v["missing"] = json!(p.tracks.len() - tracks.len());
    // 적어 둔 ID 전부. 화면은 순서를 바꾸거나 뺄 때 이것을 고쳐 보낸다(지금 없는 곡도 지키도록).
    v["ids"] = json!(p.tracks);
    v["tracks"] = json!(tracks);
    Json(v).into_response()
}

#[derive(Deserialize)]
struct UpdateBody {
    name: Option<String>,
    tracks: Option<Vec<String>>,
}

async fn update(State(app): State<App>, Path(id): Path<String>, Json(b): Json<UpdateBody>) -> Response {
    match app.collections.update_playlist(&id, b.name.as_deref(), b.tracks) {
        Ok(Some(p)) => Json(summary(&app.store.library(), &p)).into_response(),
        Ok(None) => missing(),
        Err(e) => bad(&e.to_string()),
    }
}

#[derive(Deserialize)]
struct AddBody {
    ids: Vec<String>,
}

async fn add(State(app): State<App>, Path(id): Path<String>, Json(b): Json<AddBody>) -> Response {
    match app.collections.add_to_playlist(&id, b.ids) {
        Some(p) => Json(summary(&app.store.library(), &p)).into_response(),
        None => missing(),
    }
}

async fn remove(State(app): State<App>, Path(id): Path<String>) -> Response {
    if app.collections.delete_playlist(&id) { Json(json!({ "ok": true })).into_response() } else { missing() }
}
