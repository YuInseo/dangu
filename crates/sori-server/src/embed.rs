//! 화면(ui/dist)을 실행 파일 안에 넣어 둔다. 설치할 것이 파일 하나뿐이다.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../ui/dist"]
#[allow_missing = true]
struct Ui;

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "없는 API입니다").into_response();
    }
    if let Some(f) = (!path.is_empty()).then(|| Ui::get(path)).flatten() {
        // 이름에 해시가 붙은 파일은 영원히 캐시해도 된다.
        let cache = if path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
        return ([(header::CONTENT_TYPE, f.metadata.mimetype().to_string()), (header::CACHE_CONTROL, cache.to_string())], f.data)
            .into_response();
    }
    // 화면의 주소(/albums/…)는 모두 index.html이 받아서 그린다.
    match Ui::get("index.html") {
        Some(f) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], f.data).into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "화면이 빌드되지 않았습니다. ui 폴더에서 npm run build를 하세요.",
        )
            .into_response(),
    }
}
