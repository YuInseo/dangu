//! 비밀번호 하나짜리 잠금. `SORI_PASSWORD`가 없으면 아무것도 하지 않는다.
//!
//! 쿠키에는 비밀번호가 아니라 비밀번호로 서명한 고정 문자열이 들어간다. 비밀번호를
//! 바꾸면 모든 기기가 다시 로그인하게 된다.

use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::post;
use axum::{Form, Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use crate::App;

pub const SESSION_COOKIE: &str = "sori_session";

fn hmac(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut m = Hmac::<Sha256>::new_from_slice(key).expect("HMAC 키");
    m.update(msg);
    m.finalize().into_bytes().to_vec()
}

pub fn session_token(password: &str) -> String {
    URL_SAFE_NO_PAD.encode(hmac(password.as_bytes(), b"sori-session-v1"))
}

/// 길이와 내용이 같은지 시간 차이 없이 비교한다.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn password_matches(password: &str, input: &str) -> bool {
    same(&hmac(b"sori", password.as_bytes()), &hmac(b"sori", input.as_bytes()))
}

/// 로그인 뒤 돌아갈 곳. 바깥 주소로 튕기는 것을 막는다.
pub fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") && !n.starts_with("/\\") => n.to_string(),
        _ => "/".to_string(),
    }
}

fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|kv| {
        let (k, v) = kv.trim().split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}

pub async fn gate(State(app): State<App>, req: Request, next: Next) -> Response {
    let password = app.password.as_str();
    if password.is_empty() {
        return next.run(req).await;
    }
    let path = req.uri().path().to_string();
    let open =
        path == "/login" || path == "/api/login" || path.starts_with("/assets/") || path == "/icon.svg" || path == "/manifest.webmanifest";
    let ok = cookie(req.headers(), SESSION_COOKIE).is_some_and(|v| same(v.as_bytes(), session_token(password).as_bytes()));
    if open || ok {
        return next.run(req).await;
    }
    if path.starts_with("/api/") {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "로그인이 필요합니다" }))).into_response();
    }
    let target = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/");
    Redirect::to(&format!("/login?next={}", encode(target))).into_response()
}

pub fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Deserialize)]
struct LoginForm {
    password: String,
    next: Option<String>,
}

async fn login(State(app): State<App>, headers: HeaderMap, Form(f): Form<LoginForm>) -> Response {
    let next = safe_next(f.next.as_deref());
    let password = app.password.as_str();
    if password.is_empty() || !password_matches(password, &f.password) {
        return Redirect::to(&format!("/login?error=1&next={}", encode(&next))).into_response();
    }
    let secure = headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()) == Some("https");
    let c = format!(
        "{SESSION_COOKIE}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=31536000{}",
        session_token(password),
        if secure { "; Secure" } else { "" }
    );
    ([(header::SET_COOKIE, c)], Redirect::to(&next)).into_response()
}

async fn logout() -> Response {
    ([(header::SET_COOKIE, format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"))], Redirect::to("/login"))
        .into_response()
}

pub fn routes() -> Router<App> {
    Router::new().route("/api/login", post(login)).route("/api/logout", post(logout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_stays_inside() {
        assert_eq!(safe_next(Some("/albums?x=1")), "/albums?x=1");
        assert_eq!(safe_next(Some("//evil.com")), "/");
        assert_eq!(safe_next(Some("/\\evil.com")), "/");
        assert_eq!(safe_next(Some("https://evil.com")), "/");
        assert_eq!(safe_next(None), "/");
    }

    #[test]
    fn tokens() {
        assert!(password_matches("hunter2", "hunter2"));
        assert!(!password_matches("hunter2", "hunter3"));
        assert_eq!(session_token("a"), session_token("a"));
        assert_ne!(session_token("a"), session_token("b"));
    }
}
