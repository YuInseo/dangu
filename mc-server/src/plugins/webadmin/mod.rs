//! 웹 관리 화면: 길드 레벨별 효과·강화·작물 단계·창고 막을 인스턴스, 퀘스트, 장비(강화 확률·잠재능력·
//! 주문서·보스)를 브라우저에서 고칩니다.
//!
//! - `MC_ADMIN_PORT` (기본 25580, 0이면 끔), `MC_ADMIN_BIND` (기본 `127.0.0.1` — 이 컴퓨터에서만).
//!   밖에서 쓰려면 SSH 터널(`ssh -L 25580:127.0.0.1:25580 서버`)을 권합니다.
//! - 토큰: `MC_ADMIN_TOKEN`, 없으면 `world/admin_token.txt`(`MC_ADMIN_TOKEN_FILE`)에 만들어 둡니다.
//!   켤 때 `http://127.0.0.1:25580/#token=…` 주소를 찍습니다 (토큰은 `#` 뒤라 서버 기록에 남지 않음).
//! - 저장하면 설정 파일(`guild_config.json`, `quests.json`)을 바꾸고, 모든 인스턴스(담비 자식
//!   포함)가 0.5초 안에 다시 읽습니다. 서버를 다시 켤 필요 없음.
//!
//! | 주소 | |
//! |---|---|
//! | `GET /` | 관리 화면 (page.html) |
//! | `GET /api/guild-config` · `PUT` | 길드 설정 JSON |
//! | `GET /api/guild-config/default` | 기본값 |
//! | `GET /api/quests` · `PUT` | 퀘스트 JSON |
//! | `GET /api/quests/default` | 기본값 |
//! | `GET /api/equipment` · `PUT` | 장비 JSON (강화 확률·잠재능력·주문서·보스·조합법·장비 능력치) |
//! | `GET /api/equipment/default` | 기본값 |
//! | `GET /api/ranks` · `PUT` | 후원 등급 JSON (특혜·홈 수·tp 대기·잠수 보상) |
//! | `GET /api/ranks/default` | 기본값 |
//! | `GET /api/rules` · `PUT` | 서버 규칙 JSON (금지 아이템) |
//! | `GET /api/rules/default` | 기본값 |
//! | `GET /api/menu` · `PUT` | 퀵메뉴 JSON (이동할 곳·워프·던전·링크·길라잡이) |
//! | `GET /api/menu/default` | 기본값 |
//! | `GET /api/chat?after=<자리>` | 채팅 기록의 새 줄 (처음엔 끝부분) — 화면이 1초마다 물음 |
//! | `GET /api/shops` · `PUT` | 상점 JSON (재화·물건·살 값·팔 값) |
//! | `GET /api/shops/default` | 기본값 |
//! | `GET /api/players` · `POST` | 사람마다 등급·플라이(시·분·초)·자동심기 ([`players`]) |
//!
//! `/api/`는 모두 `Authorization: Bearer <토큰>`이 있어야 합니다. `PUT`에 `X-Dry-Run: 1`을 붙이면
//! 확인만 하고 저장하지 않습니다.

mod players;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::json::{self, Json};
use crate::plugins::equip::config as equip_config;
use crate::plugins::guild::config as guild_config;
use crate::plugins::quest::config as quest_config;
use crate::plugins::store::data_path;
use crate::plugins::{menu, rank, rules, shop};

const PAGE: &str = include_str!("page.html");
const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 2 * 1024 * 1024;

/// 켤 곳과 토큰 (`MC_ADMIN_PORT=0`이면 None).
pub fn settings() -> Option<(String, String)> {
    let port: u16 = std::env::var("MC_ADMIN_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(25580);
    if port == 0 {
        return None;
    }
    let host = std::env::var("MC_ADMIN_BIND").unwrap_or_else(|_| "127.0.0.1".into());
    Some((format!("{host}:{port}"), token()))
}

/// `MC_ADMIN_TOKEN`, 없으면 파일에서 읽고, 파일도 없으면 새로 만들어 씁니다.
pub fn token() -> String {
    if let Ok(t) = std::env::var("MC_ADMIN_TOKEN")
        && !t.trim().is_empty()
    {
        return t.trim().to_owned();
    }
    let path: PathBuf = data_path("MC_ADMIN_TOKEN_FILE", "admin_token.txt");
    if let Ok(t) = std::fs::read_to_string(&path)
        && t.trim().len() >= 16
    {
        return t.trim().to_owned();
    }
    let t: String = crate::crypto::random::bytes::<16>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if std::fs::write(&path, &t).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
    }
    t
}

/// 서버를 켭니다 (돌아오지 않음).
pub async fn serve(listener: TcpListener, token: String) -> std::io::Result<()> {
    let token: Arc<str> = token.into();
    loop {
        let (s, _) = listener.accept().await?;
        let token = token.clone();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(Duration::from_secs(20), connection(s, &token)).await;
        });
    }
}

async fn connection(mut s: TcpStream, token: &str) -> std::io::Result<()> {
    let res = match read_request(&mut s).await? {
        Some(req) => {
            // 파일 잠금이 잠깐 기다릴 수 있으므로 블로킹 스레드에서
            let token = token.to_owned();
            tokio::task::spawn_blocking(move || handle(&req, &token))
                .await
                .unwrap_or_else(|_| Response::error(500, "내부 오류"))
        }
        None => Response::error(400, "잘못된 요청"),
    };
    s.write_all(&res.bytes()).await?;
    s.shutdown().await
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    /// `?` 뒤 (없으면 빈 글).
    pub query: String,
    /// 이름은 소문자.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// `?a=1&b=2`의 값.
    fn param(&self, name: &str) -> Option<&str> {
        self.query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v)
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

async fn read_request(s: &mut TcpStream) -> std::io::Result<Option<Request>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > MAX_HEAD {
            return Ok(None);
        }
        let n = s.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split_whitespace();
    let (Some(method), Some(target)) = (first.next(), first.next()) else {
        return Ok(None);
    };
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    if len > MAX_BODY {
        return Ok(None);
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < len {
        let n = s.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(len);
    Ok(Some(Request {
        method: method.to_owned(),
        path: target.split(['?', '#']).next().unwrap_or("/").to_owned(),
        query: target
            .split_once('?')
            .map(|(_, q)| q.split('#').next().unwrap_or("").to_owned())
            .unwrap_or_default(),
        headers,
        body,
    }))
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub kind: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    fn json(status: u16, j: &Json) -> Self {
        Self {
            status,
            kind: "application/json; charset=utf-8",
            body: j.pretty().into_bytes(),
        }
    }

    fn error(status: u16, msg: &str) -> Self {
        Self::json(
            status,
            &Json::Object(vec![("error".into(), Json::String(msg.into()))]),
        )
    }

    fn bytes(&self) -> Vec<u8> {
        let reason = match self.status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            404 => "Not Found",
            405 => "Method Not Allowed",
            _ => "Internal Server Error",
        };
        let mut out = format!(
            "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
             Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\
             X-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\n\
             Content-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; \
             style-src 'unsafe-inline'; connect-src 'self'\r\nConnection: close\r\n\r\n",
            self.status,
            self.kind,
            self.body.len()
        )
        .into_bytes();
        out.extend_from_slice(&self.body);
        out
    }
}

/// 길이가 같으면 끝까지 비교 (시간으로 토큰을 알아내지 못하게).
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// 요청 하나를 처리합니다 (네트워크 없이 테스트할 수 있게 따로).
pub fn handle(req: &Request, token: &str) -> Response {
    let path = req.path.as_str();
    if path == "/" || path == "/index.html" {
        if req.method != "GET" {
            return Response::error(405, "GET만 됩니다");
        }
        return Response {
            status: 200,
            kind: "text/html; charset=utf-8",
            body: PAGE.as_bytes().to_vec(),
        };
    }
    let Some(api) = path.strip_prefix("/api/") else {
        return Response::error(404, "없는 주소");
    };
    let authorized = req
        .header("authorization")
        .and_then(|h| h.strip_prefix("Bearer "))
        .is_some_and(|t| same(t.trim(), token));
    if !authorized {
        return Response::error(401, "토큰이 맞지 않습니다");
    }
    // `X-Dry-Run: 1`이면 확인만 (둘 다 통과할 때만 저장하려고 화면이 먼저 물어봄)
    let dry = req.header("x-dry-run") == Some("1");
    match (req.method.as_str(), api) {
        ("GET", "guild-config") => Response::json(200, &guild_config::encode(&guild_config::get())),
        ("GET", "guild-config/default") => {
            Response::json(200, &guild_config::encode(&guild_config::Config::default()))
        }
        ("PUT" | "POST", "guild-config") => save(&req.body, guild_config::decode, |c| {
            if dry {
                c.validate()?;
                return Ok(guild_config::encode(c));
            }
            guild_config::set(c.clone())?;
            Ok(guild_config::encode(&guild_config::get()))
        }),
        ("GET", "quests") => Response::json(200, &quest_config::encode(&quest_config::get())),
        ("GET", "quests/default") => Response::json(
            200,
            &quest_config::encode(&quest_config::QuestConfig::default()),
        ),
        ("PUT" | "POST", "quests") => save(&req.body, quest_config::decode, |c| {
            if dry {
                c.validate()?;
                return Ok(quest_config::encode(c));
            }
            quest_config::set(c.clone())?;
            Ok(quest_config::encode(&quest_config::get()))
        }),
        ("GET", "equipment") => Response::json(200, &equip_config::encode(&equip_config::get())),
        ("GET", "equipment/default") => Response::json(
            200,
            &equip_config::encode(&equip_config::EquipConfig::default()),
        ),
        ("PUT" | "POST", "equipment") => save(&req.body, equip_config::decode, |c| {
            if dry {
                c.validate()?;
                return Ok(equip_config::encode(c));
            }
            equip_config::set(c.clone())?;
            Ok(equip_config::encode(&equip_config::get()))
        }),
        ("GET", "ranks") => Response::json(200, &rank::encode_config(&rank::get_config())),
        ("GET", "ranks/default") => {
            Response::json(200, &rank::encode_config(&rank::RankConfig::default()))
        }
        ("PUT" | "POST", "ranks") => save(&req.body, rank::decode_config, |c| {
            if dry {
                c.validate()?;
                return Ok(rank::encode_config(c));
            }
            rank::set_config(c.clone())?;
            Ok(rank::encode_config(&rank::get_config()))
        }),
        ("GET", "rules") => Response::json(200, &rules::encode(&rules::get())),
        ("GET", "rules/default") => Response::json(200, &rules::encode(&rules::Rules::default())),
        ("PUT" | "POST", "rules") => save(&req.body, rules::decode, |r| {
            if dry {
                r.validate()?;
                return Ok(rules::encode(r));
            }
            rules::set(r.clone())?;
            Ok(rules::encode(&rules::get()))
        }),
        ("GET", "chat") => {
            let after = req.param("after").and_then(|a| a.parse::<u64>().ok());
            let (lines, next) = crate::plugins::chat::log::read_since(after);
            Response::json(
                200,
                &Json::Object(vec![
                    ("next".into(), Json::Number(next as f64)),
                    ("lines".into(), Json::Array(lines)),
                ]),
            )
        }
        ("GET", "players") => Response::json(200, &players::list()),
        ("POST" | "PUT", "players") => {
            let parsed = std::str::from_utf8(&req.body)
                .ok()
                .and_then(|t| json::parse(t).ok());
            match parsed.map(|j| players::act(&j)) {
                Some(Ok(msg)) => Response::json(
                    200,
                    &Json::Object(vec![
                        ("ok".into(), Json::Bool(true)),
                        ("message".into(), Json::String(msg)),
                    ]),
                ),
                Some(Err(e)) => Response::error(400, &e),
                None => Response::error(400, "JSON이 아닙니다"),
            }
        }
        ("GET", "shops") => Response::json(200, &shop::encode(&shop::get())),
        ("GET", "shops/default") => {
            Response::json(200, &shop::encode(&shop::ShopConfig::default()))
        }
        ("PUT" | "POST", "shops") => save(&req.body, shop::decode, |c| {
            if dry {
                c.validate()?;
                return Ok(shop::encode(c));
            }
            shop::set(c.clone())?;
            Ok(shop::encode(&shop::get()))
        }),
        ("GET", "menu") => Response::json(200, &menu::encode(&menu::get())),
        ("GET", "menu/default") => Response::json(200, &menu::encode(&menu::MenuConfig::default())),
        ("PUT" | "POST", "menu") => save(&req.body, menu::decode, |c| {
            if dry {
                c.validate()?;
                return Ok(menu::encode(c));
            }
            menu::set(c.clone())?;
            Ok(menu::encode(&menu::get()))
        }),
        (
            _,
            "shops"
            | "shops/default"
            | "menu"
            | "menu/default"
            | "ranks"
            | "ranks/default"
            | "rules"
            | "rules/default"
            | "guild-config"
            | "guild-config/default"
            | "quests"
            | "quests/default"
            | "equipment"
            | "equipment/default",
        ) => Response::error(405, "그 방법으로는 안 됩니다"),
        _ => Response::error(404, "없는 주소"),
    }
}

fn save<T>(
    body: &[u8],
    decode: fn(&Json) -> Option<T>,
    apply: impl FnOnce(&T) -> Result<Json, String>,
) -> Response {
    let parsed = std::str::from_utf8(body)
        .map_err(|_| "UTF-8이 아닙니다".to_owned())
        .and_then(|t| json::parse(t).map_err(|e| format!("JSON 오류: {e:?}")))
        .and_then(|j| decode(&j).ok_or_else(|| "모양이 맞지 않습니다".to_owned()));
    match parsed.and_then(|c| apply(&c)) {
        Ok(saved) => Response::json(
            200,
            &Json::Object(vec![
                ("ok".into(), Json::Bool(true)),
                ("saved".into(), saved),
            ]),
        ),
        Err(e) => Response::error(400, &e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(method: &str, path: &str, token: Option<&str>, body: &str) -> Request {
        let mut headers = Vec::new();
        if let Some(t) = token {
            headers.push(("authorization".into(), format!("Bearer {t}")));
        }
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        Request {
            method: method.into(),
            path: path.into(),
            query: query.into(),
            headers,
            body: body.as_bytes().to_vec(),
        }
    }

    fn body(r: &Response) -> Json {
        json::parse(std::str::from_utf8(&r.body).unwrap()).unwrap()
    }

    #[test]
    fn page_is_open_but_api_needs_the_token() {
        let r = handle(&req("GET", "/", None, ""), "secret-token-123");
        assert_eq!(r.status, 200);
        assert!(String::from_utf8_lossy(&r.body).contains("<html"));
        let r = handle(
            &req("GET", "/api/guild-config", None, ""),
            "secret-token-123",
        );
        assert_eq!(r.status, 401);
        let r = handle(
            &req("GET", "/api/guild-config", Some("wrong"), ""),
            "secret-token-123",
        );
        assert_eq!(r.status, 401);
        let r = handle(
            &req("GET", "/api/guild-config", Some("secret-token-123"), ""),
            "secret-token-123",
        );
        assert_eq!(r.status, 200);
        assert!(body(&r).get("levels").is_some());
        let r = handle(&req("GET", "/nope", None, ""), "t");
        assert_eq!(r.status, 404);
        let raw = String::from_utf8(r.bytes()).unwrap();
        assert!(raw.starts_with("HTTP/1.1 404") && raw.contains("Content-Security-Policy"));
    }

    #[test]
    fn saving_validates_and_applies() {
        let t = "tok";
        // 망가진 JSON, 확인을 통과하지 못하는 설정은 400
        let r = handle(&req("PUT", "/api/guild-config", Some(t), "{"), t);
        assert_eq!(r.status, 400);
        let r = handle(
            &req("PUT", "/api/guild-config", Some(t), r#"{"levels": []}"#),
            t,
        );
        assert_eq!(r.status, 400, "{:?}", body(&r));
        // 기본값에서 창고 막을 인스턴스만 바꿔 저장 (다른 테스트에 닿지 않는 값)
        let c = guild_config::Config {
            storage_blocked_instances: vec!["webadmin-test".into()],
            ..Default::default()
        };
        let text = guild_config::encode(&c).compact();
        let r = handle(&req("PUT", "/api/guild-config", Some(t), &text), t);
        assert_eq!(r.status, 200, "{:?}", body(&r));
        assert_eq!(
            guild_config::get().storage_blocked_instances,
            ["webadmin-test"]
        );
        guild_config::set(guild_config::Config::default()).unwrap();

        let r = handle(
            &req(
                "PUT",
                "/api/quests",
                Some(t),
                r#"{"normal":[{"id":"x","name":"x","objective":{"kind":"fly"},"target":1}]}"#,
            ),
            t,
        );
        assert_eq!(r.status, 400);
        let mut dry = req("PUT", "/api/guild-config", Some(t), &text);
        dry.headers.push(("x-dry-run".into(), "1".into()));
        let other = guild_config::Config {
            max_officers: 9,
            ..Default::default()
        };
        dry.body = guild_config::encode(&other).compact().into_bytes();
        assert_eq!(handle(&dry, t).status, 200);
        assert_eq!(
            guild_config::get().max_officers,
            2,
            "확인만 하고 바꾸지 않음"
        );
        let r = handle(&req("GET", "/api/quests/default", Some(t), ""), t);
        assert!(body(&r).get("weekly").is_some());
        let r = handle(&req("GET", "/api/equipment", Some(t), ""), t);
        assert!(body(&r).get("enhance").is_some());
        let r = handle(
            &req(
                "PUT",
                "/api/equipment",
                Some(t),
                r#"{"enhance": {"chances": [0.5]}}"#,
            ),
            t,
        );
        assert_eq!(r.status, 400, "확률 30개가 아니면 거절");
        // 등급·규칙: 확인만 하면 저장하지 않고, 잘못된 것은 거절
        let r = handle(&req("GET", "/api/ranks", Some(t), ""), t);
        assert!(body(&r).get("ranks").is_some());
        let mut dry = req(
            "PUT",
            "/api/ranks",
            Some(t),
            r#"{"ranks": [{"id": "a"}, {"id": "a"}]}"#,
        );
        dry.headers.push(("x-dry-run".into(), "1".into()));
        assert_eq!(handle(&dry, t).status, 400, "겹치는 id");
        let r = handle(&req("GET", "/api/rules/default", Some(t), ""), t);
        assert!(body(&r).get("banned_count").and_then(Json::as_f64).unwrap() > 30.0);
        let mut dry = req(
            "PUT",
            "/api/rules",
            Some(t),
            r#"{"banned_items": ["tnt", "no_such_item"]}"#,
        );
        dry.headers.push(("x-dry-run".into(), "1".into()));
        assert_eq!(handle(&dry, t).status, 400, "모르는 아이템");
        let r = handle(&req("DELETE", "/api/quests", Some(t), ""), t);
        assert_eq!(r.status, 405);
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab"));
    }
}
