//! 라이브러리를 JSON으로. 화면의 한 쪽이 요청 한 번으로 그려지도록 쪽마다 묶어서 준다.

use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, Stream};
use serde::Deserialize;
use serde_json::{Value, json};
use sori_core::collate::compare;
use sori_core::ids::{genre_id, norm};
use sori_core::library::{Library, compare_artist, sort_name};
use sori_core::model::*;
use sori_core::quality::tier_of;

use crate::App;

type Q = Query<HashMap<String, String>>;

fn not_found(what: &str) -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": format!("{what}을(를) 찾을 수 없습니다") }))).into_response()
}

/// 같은 문자열이면 같은 숫자(FNV-1a). 화면의 `hash`와 같은 값이다.
pub fn fnv(s: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for u in s.encode_utf16() {
        h ^= u as u32;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

/// 날짜가 같으면 같은 순서. 하루 동안은 "오늘의 발견"이 새로고침마다 바뀌지 않는다.
fn daily_pick<'a>(albums: impl Iterator<Item = &'a Album>, n: usize) -> Vec<AlbumCard> {
    let day = today();
    let mut v: Vec<(&Album, u32)> = albums.map(|a| (a, fnv(&format!("{day}{}", a.id)))).collect();
    v.sort_by_key(|x| x.1);
    v.into_iter().take(n).map(|(a, _)| AlbumCard::from(a)).collect()
}

fn today() -> String {
    let days = now_ms().div_euclid(86_400_000);
    // 1970-01-01부터 날 수 → 그레고리력 (Howard Hinnant의 공식)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

async fn library(State(app): State<App>) -> Json<Value> {
    let s = &app.store;
    let lib = s.library();
    Json(json!({
        "summary": lib.summary,
        "status": s.scan_status(),
        "analysis": s.analysis_status(),
        "generation": s.generation(),
        "musicDir": s.music_dir().to_string_lossy(),
        "dataDir": s.config.data_dir.to_string_lossy(),
        "version": sori_core::VERSION,
        "auth": !app.password.is_empty(),
        "platform": std::env::consts::OS,
        "host": app.host.as_ref().map(|h| h.name()),
        "deviceName": app.link.name(),
    }))
}

async fn home(State(app): State<App>) -> Json<Value> {
    let lib = app.store.library();
    let mut recent: Vec<&Album> = lib.albums.values().collect();
    recent.sort_by(|a, b| b.added_at.cmp(&a.added_at).then_with(|| a.id.cmp(&b.id)));
    let recent: Vec<AlbumCard> = recent.into_iter().take(18).map(AlbumCard::from).collect();
    let hires: Vec<&Album> = lib.album_list.iter().map(|id| &lib.albums[id]).filter(|a| tier_of(&a.format) == QualityTier::Hires).collect();
    Json(json!({
        "summary": lib.summary,
        "recent": recent,
        "hiresCount": hires.len(),
        "hires": daily_pick(hires.into_iter(), 18),
        "picks": daily_pick(lib.album_list.iter().map(|id| &lib.albums[id]), 12),
    }))
}

async fn albums(State(app): State<App>, Query(q): Q) -> Json<Value> {
    let lib = app.store.library();
    let tier = q.get("tier").map(String::as_str).unwrap_or("all");
    let sort = q.get("sort").map(String::as_str).unwrap_or("artist");
    let mut list: Vec<&Album> = lib
        .album_list
        .iter()
        .map(|id| &lib.albums[id])
        .filter(|a| match tier {
            "hires" => tier_of(&a.format) == QualityTier::Hires,
            // "무손실"은 Hi-Res까지 포함한다.
            "lossless" => a.format.lossless,
            "lossy" => !a.format.lossless,
            _ => true,
        })
        .collect();
    match sort {
        "title" => list.sort_by(|a, b| compare(sort_name(&a.title), sort_name(&b.title))),
        "year" => list.sort_by(|a, b| b.year.unwrap_or(0).cmp(&a.year.unwrap_or(0)).then_with(|| compare(&a.title, &b.title))),
        "added" => list.sort_by_key(|a| std::cmp::Reverse(a.added_at)),
        _ => list.sort_by(|a, b| compare_artist(&a.artist, &b.artist).then(a.year.unwrap_or(0).cmp(&b.year.unwrap_or(0)))),
    }
    Json(json!({ "albums": list.into_iter().map(AlbumCard::from).collect::<Vec<_>>() }))
}

fn tracks_of(lib: &Library, ids: &[String]) -> Vec<Track> {
    ids.iter().filter_map(|id| lib.views.get(id).cloned()).collect()
}

async fn album(State(app): State<App>, Path(id): Path<String>) -> Response {
    let lib = app.store.library();
    let Some(album) = lib.albums.get(&id) else { return not_found("앨범") };
    let more: Vec<AlbumCard> = lib
        .artists
        .get(&album.artist_id)
        .map(|a| a.album_ids.iter().filter(|x| **x != id).map(|x| AlbumCard::from(&lib.albums[x])).collect())
        .unwrap_or_default();
    let genres: Vec<Value> = album.genres.iter().map(|g| json!({ "id": genre_id(g), "name": g })).collect();
    Json(json!({ "album": album, "tracks": tracks_of(&lib, &album.track_ids), "more": more, "genres": genres })).into_response()
}

async fn artists(State(app): State<App>) -> Json<Value> {
    let lib = app.store.library();
    Json(json!({ "artists": lib.ordered_artists().collect::<Vec<_>>() }))
}

async fn artist(State(app): State<App>, Path(id): Path<String>) -> Response {
    let lib = app.store.library();
    let Some(artist) = lib.artists.get(&id) else { return not_found("아티스트") };
    let albums: Vec<&Album> = artist.album_ids.iter().map(|a| &lib.albums[a]).collect();
    // 연도 순으로 앨범을 이어 붙인 것이 "이 아티스트 전부 재생".
    let mut by_year = albums.clone();
    by_year.sort_by_key(|a| a.year.unwrap_or(0));
    let own: Vec<Track> = by_year.iter().flat_map(|a| tracks_of(&lib, &a.track_ids)).collect();
    Json(json!({
        "artist": artist,
        "albums": albums.iter().map(|a| AlbumCard::from(*a)).collect::<Vec<_>>(),
        "own": own,
        "appears": tracks_of(&lib, &artist.appears_on),
    }))
    .into_response()
}

async fn tracks(State(app): State<App>, Query(q): Q) -> Json<Value> {
    let lib = app.store.library();
    let offset: usize = q.get("offset").and_then(|v| v.parse().ok()).unwrap_or(0);
    let limit: usize = q.get("limit").and_then(|v| v.parse().ok()).unwrap_or(200).clamp(1, 500);
    let query = norm(q.get("q").map(String::as_str).unwrap_or(""));
    let terms: Vec<&str> = query.split(' ').filter(|t| !t.is_empty()).collect();
    let matched: Vec<&Track> = lib
        .ordered_tracks()
        .filter(|t| {
            terms.is_empty() || {
                let h = norm(&format!("{} {} {}", t.title, t.artist, t.album));
                terms.iter().all(|w| h.contains(w))
            }
        })
        .collect();
    let total = matched.len();
    let page: Vec<&Track> = matched.into_iter().skip(offset).take(limit).collect();
    Json(json!({ "total": total, "offset": offset, "tracks": page }))
}

async fn genres(State(app): State<App>) -> Json<Value> {
    let lib = app.store.library();
    Json(json!({ "genres": lib.ordered_genres().collect::<Vec<_>>() }))
}

async fn genre(State(app): State<App>, Path(id): Path<String>) -> Response {
    let lib = app.store.library();
    let Some(g) = lib.genres.get(&id) else { return not_found("장르") };
    let albums: Vec<AlbumCard> = g.album_ids.iter().map(|a| AlbumCard::from(&lib.albums[a])).collect();
    Json(json!({ "genre": g, "albums": albums })).into_response()
}

async fn search(State(app): State<App>, Query(q): Q) -> Json<Value> {
    let lib = app.store.library();
    Json(serde_json::to_value(lib.search(q.get("q").map(String::as_str).unwrap_or(""))).unwrap_or(Value::Null))
}

async fn scan_status(State(app): State<App>) -> Json<Value> {
    Json(json!({ "status": app.store.scan_status(), "summary": app.store.library().summary }))
}

async fn scan_start(State(app): State<App>) -> Json<Value> {
    Json(json!({ "status": app.store.start_scan() }))
}

async fn analysis_status(State(app): State<App>) -> Json<AnalysisStatus> {
    Json(app.store.analysis_status())
}

#[derive(Deserialize)]
struct SpeedBody {
    speed: AnalysisSpeed,
}

async fn analysis_set(State(app): State<App>, Json(b): Json<SpeedBody>) -> Json<AnalysisStatus> {
    app.store.set_analysis_speed(b.speed);
    Json(app.store.analysis_status())
}

async fn waveform(State(app): State<App>, Path(id): Path<String>) -> Response {
    match app.store.waveform(&id) {
        Some(w) => ([(axum::http::header::CACHE_CONTROL, "private, max-age=3600")], Json(json!({ "w": w }))).into_response(),
        // 아직 분석 전이면 404. 화면은 기억하지 않고 다음에 다시 묻는다.
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "아직 분석하지 않았습니다" }))).into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MusicDirBody {
    music_dir: Option<String>,
}

async fn set_music_dir(State(app): State<App>, Json(b): Json<MusicDirBody>) -> Response {
    let dir = b.music_dir.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
    match app.store.set_music_dir(dir) {
        Ok(()) => Json(json!({ "ok": true, "musicDir": app.store.music_dir().to_string_lossy() })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

/// 음악이 있을 만한 폴더와 곡 수. 휴대폰에는 폴더 고르기 창이 없어서 이것으로 고른다.
async fn music_dir_suggestions() -> Json<serde_json::Value> {
    let found = tokio::task::spawn_blocking(|| {
        let mut roots: Vec<std::path::PathBuf> = Vec::new();
        if cfg!(target_os = "android") {
            // 내장 저장공간과 SD 카드(/storage/XXXX-XXXX)
            roots.push("/storage/emulated/0".into());
            if let Ok(rd) = std::fs::read_dir("/storage") {
                roots.extend(rd.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n != "emulated" && n != "self")));
            }
        } else if cfg!(windows)
            && let Some(home) = std::env::var_os("USERPROFILE")
        {
            roots.push(home.into());
        }
        let found = sori_core::scan::suggest_music_dirs(&roots, 999, std::time::Duration::from_secs(4));
        // 사람이 읽는 이름: 저장공간 전체, SD 카드 안의 폴더는 앞에 "SD 카드"
        let label = |p: &std::path::Path| {
            let root = roots.iter().position(|r| p == r.as_path());
            let under = roots.iter().position(|r| p.starts_with(r));
            let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            match (root, under) {
                (Some(0), _) if cfg!(windows) => "내 폴더 전체".to_string(),
                (Some(0), _) => "내장 저장공간 전체".to_string(),
                (Some(_), _) => "SD 카드 전체".to_string(),
                (None, Some(i)) if i > 0 => format!("SD 카드 · {name}"),
                _ => name,
            }
        };
        found.into_iter().take(12).map(|(p, n)| json!({ "path": p.to_string_lossy(), "name": label(&p), "count": n })).collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    Json(json!(found))
}

/// 앱이면 폴더 고르기 창을 띄우고, 고른 폴더로 바꾼다.
async fn pick_music_dir(State(app): State<App>) -> Response {
    let Some(host) = app.host.clone() else {
        return (StatusCode::NOT_IMPLEMENTED, Json(json!({ "error": "이 서버에서는 폴더 고르기 창을 띄울 수 없습니다" }))).into_response();
    };
    let start = Some(app.store.music_dir());
    let picked = tokio::task::spawn_blocking(move || host.pick_folder(start)).await.ok().flatten();
    let Some(dir) = picked else { return Json(json!({ "ok": false })).into_response() };
    match app.store.set_music_dir(Some(dir.to_string_lossy().to_string())) {
        Ok(()) => Json(json!({ "ok": true, "musicDir": app.store.music_dir().to_string_lossy() })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

/// 라이브러리와 스캔·분석 상태가 바뀔 때마다 알린다. 화면은 폴링 대신 이걸 듣는다.
async fn events(State(app): State<App>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let s = stream::unfold((app, String::new()), |(app, last)| async move {
        loop {
            let snap = json!({
                "generation": app.store.generation(),
                "collections": app.collections.generation(),
                "status": app.store.scan_status(),
                "analysis": app.store.analysis_status(),
            })
            .to_string();
            if snap != last {
                let ev = Event::default().event("library").data(snap.clone());
                return Some((Ok(ev), (app, snap)));
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    });
    Sse::new(s).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/library", get(library))
        .route("/api/home", get(home))
        .route("/api/albums", get(albums))
        .route("/api/albums/{id}", get(album))
        .route("/api/artists", get(artists))
        .route("/api/artists/{id}", get(artist))
        .route("/api/tracks", get(tracks))
        .route("/api/genres", get(genres))
        .route("/api/genres/{id}", get(genre))
        .route("/api/search", get(search))
        .route("/api/scan", get(scan_status).post(scan_start))
        .route("/api/analysis", get(analysis_status).post(analysis_set))
        .route("/api/waveform/{id}", get(waveform))
        .route("/api/settings/music-dir", post(set_music_dir))
        .route("/api/settings/pick-music-dir", post(pick_music_dir))
        .route("/api/settings/music-dirs", get(music_dir_suggestions))
        .route("/api/events", get(events))
        .route("/api/open", post(open_url))
}

#[derive(Deserialize)]
struct OpenBody {
    url: String,
}

/// 앱이면 주소를 앱 밖에서 연다(브라우저, 유튜브 앱). 브라우저로 쓰는 서버면 501: 화면이 직접 연다.
async fn open_url(State(app): State<App>, Json(b): Json<OpenBody>) -> Response {
    let url = b.url.trim().to_string();
    if !(url.starts_with("https://") || url.starts_with("http://"))
        || url.len() > 4000
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "열 수 없는 주소입니다" }))).into_response();
    }
    let Some(host) = app.host.clone() else {
        return (StatusCode::NOT_IMPLEMENTED, Json(json!({ "error": "브라우저에서 엽니다" }))).into_response();
    };
    match tokio::task::spawn_blocking(move || host.open_url(&url)).await {
        Ok(Ok(())) => Json(json!({ "ok": true })).into_response(),
        Ok(Err(e)) => (StatusCode::NOT_IMPLEMENTED, Json(json!({ "error": e.to_string() }))).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "작업이 멈췄습니다" }))).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_matches_js() {
        // 화면의 hash('abc')와 같은 값
        assert_eq!(fnv("abc"), 440_920_331);
        assert_eq!(fnv(""), 2_166_136_261);
    }

    #[test]
    fn today_is_a_date() {
        let d = today();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"));
    }
}
