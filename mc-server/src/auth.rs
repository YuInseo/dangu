//! 정품 인증: Mojang 세션 서버에 "이 사람이 이 서버 해시로 접속했는가"를 묻습니다.
//! 응답의 프로필 속성(`textures`)이 곧 스킨·망토입니다.
//!
//! HTTPS(TLS)만은 rustls를 씁니다. TLS와 인증서 검증을 직접 짜면 보안 구멍이 되기 쉬워서,
//! 이 서버에서 유일하게 외부 구현에 맡기는 부분입니다. 게임 프로토콜의 암호(RSA·AES·SHA-1)는
//! [`crate::crypto`]에서 직접 구현합니다.

use std::future::Future;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::json::{self, Json};

/// 프로필 속성 하나. 스킨은 `textures`(base64 JSON)와 Mojang 서명.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameProfile {
    pub uuid: u128,
    pub name: String,
    pub properties: Vec<Property>,
}

pub type ProfileFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<GameProfile>, String>> + Send + 'a>>;

/// 세션 서버. 테스트에서는 가짜로 바꿔 끼웁니다.
pub trait SessionService: Send + Sync {
    /// 정품 인증. 클라이언트가 Mojang에 접속을 알리지 않았으면 `Ok(None)`.
    fn has_joined<'a>(&'a self, name: &'a str, server_hash: &'a str) -> ProfileFuture<'a>;

    /// 이름으로 정품 프로필(스킨 포함)을 찾습니다. 오프라인 모드에서 스킨을 입힐 때 씁니다.
    fn profile_by_name<'a>(&'a self, name: &'a str) -> ProfileFuture<'a>;
}

pub struct MojangSessionService;

const SESSION_HOST: &str = "sessionserver.mojang.com";
const API_HOST: &str = "api.mojang.com";

impl SessionService for MojangSessionService {
    fn has_joined<'a>(&'a self, name: &'a str, server_hash: &'a str) -> ProfileFuture<'a> {
        let path = format!(
            "/session/minecraft/hasJoined?username={}&serverId={}",
            name, server_hash
        );
        Box::pin(async move {
            let (status, body) = https_get(SESSION_HOST, path).await?;
            match status {
                200 => parse_profile(&body).map(Some),
                204 => Ok(None),
                s => Err(format!("세션 서버 응답 {s}")),
            }
        })
    }

    fn profile_by_name<'a>(&'a self, name: &'a str) -> ProfileFuture<'a> {
        let path = format!("/users/profiles/minecraft/{name}");
        Box::pin(async move {
            let (status, body) = https_get(API_HOST, path).await?;
            if status != 200 {
                return Ok(None);
            }
            let id = json::parse(&body)?
                .get("id")
                .and_then(Json::as_str)
                .ok_or("id 없음")?
                .to_owned();
            let (status, body) = https_get(
                SESSION_HOST,
                format!("/session/minecraft/profile/{id}?unsigned=false"),
            )
            .await?;
            if status != 200 {
                return Ok(None);
            }
            parse_profile(&body).map(Some)
        })
    }
}

/// 플레이어 채팅 키에 서명하는 Mojang 공개 키들 (`playerCertificateKeys`).
pub async fn fetch_chat_keys() -> Result<Vec<crate::crypto::rsa::PublicKey>, String> {
    let (status, body) = https_get("api.minecraftservices.com", "/publickeys".into()).await?;
    if status != 200 {
        return Err(format!("publickeys 응답 {status}"));
    }
    parse_chat_keys(&body)
}

pub fn parse_chat_keys(body: &str) -> Result<Vec<crate::crypto::rsa::PublicKey>, String> {
    let v = json::parse(body)?;
    let keys = v
        .get("playerCertificateKeys")
        .and_then(Json::as_array)
        .ok_or("playerCertificateKeys 없음")?;
    Ok(keys
        .iter()
        .filter_map(|k| k.get("publicKey")?.as_str())
        .filter_map(crate::crypto::base64::decode)
        .filter_map(|der| crate::crypto::rsa::PublicKey::from_der(&der))
        .collect())
}

pub fn parse_profile(body: &str) -> Result<GameProfile, String> {
    let v = json::parse(body)?;
    let id = v.get("id").and_then(Json::as_str).ok_or("id 없음")?;
    let uuid = u128::from_str_radix(&id.replace('-', ""), 16).map_err(|_| "id 형식")?;
    let name = v
        .get("name")
        .and_then(Json::as_str)
        .ok_or("name 없음")?
        .to_owned();
    let properties = v
        .get("properties")
        .and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|p| {
            Some(Property {
                name: p.get("name")?.as_str()?.to_owned(),
                value: p.get("value")?.as_str()?.to_owned(),
                signature: p.get("signature").and_then(Json::as_str).map(str::to_owned),
            })
        })
        .collect();
    Ok(GameProfile {
        uuid,
        name,
        properties,
    })
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots =
                rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            Arc::new(
                rustls::ClientConfig::builder_with_provider(provider)
                    .with_safe_default_protocol_versions()
                    .expect("TLS versions")
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            )
        })
        .clone()
}

/// HTTPS GET 하나 (HTTP/1.1, `Connection: close`). 블로킹 소켓이라 별도 스레드에서 돕니다.
async fn https_get(host: &'static str, path: String) -> Result<(u16, String), String> {
    tokio::task::spawn_blocking(move || {
        let server_name = rustls::pki_types::ServerName::try_from(host).map_err(|e| e.to_string())?;
        let conn = rustls::ClientConnection::new(tls_config(), server_name).map_err(|e| e.to_string())?;
        let tcp = TcpStream::connect((host, 443)).map_err(|e| format!("{host}: {e}"))?;
        tcp.set_read_timeout(Some(Duration::from_secs(10))).ok();
        tcp.set_write_timeout(Some(Duration::from_secs(10))).ok();
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: dangu-mc\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
        );
        tls.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
        let mut response = Vec::new();
        match tls.read_to_end(&mut response) {
            Ok(_) => {}
            // 상대가 close_notify 없이 끊어도 받은 만큼은 씁니다.
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && !response.is_empty() => {}
            Err(e) => return Err(e.to_string()),
        }
        parse_http_response(&response)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 상태 코드와 본문. `Transfer-Encoding: chunked`도 풉니다.
pub fn parse_http_response(raw: &[u8]) -> Result<(u16, String), String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("HTTP 머리 끝이 없습니다")?;
    let head = std::str::from_utf8(&raw[..split]).map_err(|_| "HTTP 머리가 UTF-8이 아닙니다")?;
    let mut lines = head.split("\r\n");
    let status: u16 = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or("상태 줄이 이상합니다")?;
    let chunked = lines.any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("transfer-encoding:") && l.contains("chunked")
    });
    let mut body = raw[split + 4..].to_vec();
    if chunked {
        let mut out = Vec::new();
        let mut rest = &body[..];
        loop {
            let line_end = rest
                .windows(2)
                .position(|w| w == b"\r\n")
                .ok_or("chunk 크기 줄")?;
            let size_text = std::str::from_utf8(&rest[..line_end]).map_err(|_| "chunk 크기")?;
            let size = usize::from_str_radix(size_text.split(';').next().unwrap().trim(), 16)
                .map_err(|_| "chunk 크기")?;
            rest = &rest[line_end + 2..];
            if size == 0 {
                break;
            }
            out.extend_from_slice(rest.get(..size).ok_or("chunk가 잘렸습니다")?);
            rest = rest.get(size + 2..).unwrap_or_default();
        }
        body = out;
    }
    Ok((
        status,
        String::from_utf8(body).map_err(|_| "본문이 UTF-8이 아닙니다")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_chunked_responses() {
        let plain = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}";
        assert_eq!(parse_http_response(plain).unwrap(), (200, "{}".into()));
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        assert_eq!(
            parse_http_response(chunked).unwrap(),
            (200, "{\"a\":1}".into())
        );
        let empty = b"HTTP/1.1 204 No Content\r\n\r\n";
        assert_eq!(parse_http_response(empty).unwrap().0, 204);
    }

    #[test]
    fn parses_publickeys_response() {
        let key = crate::crypto::rsa::PrivateKey::generate(512);
        let b64 = crate::crypto::base64::encode(&key.public.to_der());
        let body = format!(
            r#"{{"profilePropertyKeys":[],"playerCertificateKeys":[{{"publicKey":"{b64}"}},{{"publicKey":"!!"}}]}}"#
        );
        assert_eq!(parse_chat_keys(&body).unwrap(), vec![key.public]);
    }

    #[test]
    fn parses_profile_with_textures() {
        let p = parse_profile(
            r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","properties":[{"name":"textures","value":"dGV4","signature":"c2ln"}]}"#,
        )
        .unwrap();
        assert_eq!(p.uuid, 0x069a79f444e94726a5befca90e38aaf5);
        assert_eq!(p.properties[0].signature.as_deref(), Some("c2ln"));
    }
}
