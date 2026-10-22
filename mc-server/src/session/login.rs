//! Login: 이름 받기 → (정품이면) 암호화와 세션 서버 확인 → 압축 켜기 → Login Finished.

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};

use super::Profile;
use crate::connection::{next, unknown};
use crate::crypto::{random, sha1};
use crate::packets::login::{self, Hello};
use crate::protocol::conn::Conn;
use crate::protocol::frame::FrameError;
use crate::server::Server;
use crate::uuid::{format_uuid, offline_uuid, random_uuid};

/// 끝까지 가면 프로필, 도중에 사유를 보내고 끊었으면 `None`.
pub async fn run<S>(
    conn: &mut Conn<S>,
    server: &Server,
    ip: &str,
) -> Result<Option<Profile>, FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let packet = next(conn).await?.ok_or(eof())?;
    if packet.id != login::HELLO_ID {
        return Err(unknown("login", packet.id));
    }
    let hello = match Hello::decode(&packet.body) {
        Ok(h) => h,
        Err(e) => {
            // 조용히 끊지 않고 이유를 보여 줌
            let reason =
                format!("접속 정보를 읽지 못했습니다 ({e}). 이름은 영문·숫자·_ 16자까지입니다.");
            conn.write_packet(login::DISCONNECT_ID, &login::encode_disconnect(&reason))
                .await?;
            return Ok(None);
        }
    };

    let profile = if server.config.online_mode {
        match authenticate(conn, server, &hello.name).await? {
            Ok(profile) => profile,
            Err(reason) => {
                conn.write_packet(login::DISCONNECT_ID, &login::encode_disconnect(&reason))
                    .await?;
                return Ok(None);
            }
        }
    } else {
        offline_profile(server, &hello.name).await
    };
    println!(
        "{} 로그인 ({}, {})",
        profile.name,
        format_uuid(profile.uuid),
        if server.config.online_mode {
            "정품"
        } else {
            "오프라인"
        }
    );

    if let Some(reason) = refusal(server, &profile, ip) {
        println!("{} 접속 거부: {reason}", profile.name);
        conn.write_packet(login::DISCONNECT_ID, &login::encode_disconnect(&reason))
            .await?;
        return Ok(None);
    }

    // 압축은 암호화 다음, Login Finished 전에 켭니다 (Set Compression 자체는 압축하지 않음).
    if let Some(threshold) = server.config.compression_threshold {
        conn.write_packet(
            login::SET_COMPRESSION_ID,
            &login::encode_set_compression(threshold),
        )
        .await?;
        conn.set_compression(Some(threshold));
    }

    let finished = login::encode_login_finished(
        profile.uuid,
        &profile.name,
        &profile.properties,
        random_uuid(),
    );
    conn.write_packet(login::LOGIN_FINISHED_ID, &finished)
        .await?;

    expect(conn, login::LOGIN_ACKNOWLEDGED_ID).await?;
    Ok(Some(profile))
}

/// `id` 패킷이 올 때까지 읽습니다. 그 사이 온 다른 Login 패킷(묻지 않은 플러그인 응답·쿠키 응답
/// 같은 것, 일부 클라이언트가 보냄)은 몇 개까지 건너뜁니다.
async fn expect<S>(
    conn: &mut Conn<S>,
    id: i32,
) -> Result<crate::protocol::frame::RawPacket, FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    for _ in 0..32 {
        let packet = next(conn).await?.ok_or(eof())?;
        if packet.id == id {
            return Ok(packet);
        }
        if packet.id == login::HELLO_ID {
            return Err(unknown("login", packet.id));
        }
    }
    Err(unknown("login", id))
}

/// 밴·IP 밴·화이트리스트 확인. 처음 들어온 `initial_ops`는 여기서 OP 4가 됩니다.
fn refusal(server: &Server, profile: &Profile, ip: &str) -> Option<String> {
    let initial = server
        .config
        .initial_ops
        .iter()
        .any(|n| n.eq_ignore_ascii_case(&profile.name));
    let (is_op, stale_name) = server.perms.read(|d| {
        let op = d.ops.iter().find(|o| o.uuid == profile.uuid);
        (op.is_some(), op.is_some_and(|o| o.name != profile.name))
    });
    if (initial && !is_op) || stale_name {
        server.perms.update(
            |d| match d.ops.iter_mut().find(|o| o.uuid == profile.uuid) {
                // 이름이 바뀌었으면 기록도 따라갑니다.
                Some(op) => op.name.clone_from(&profile.name),
                None => d.ops.push(crate::perms::Op {
                    uuid: profile.uuid,
                    name: profile.name.clone(),
                    level: 4,
                }),
            },
        );
    }
    let level = server.perms.op_level(profile.uuid);
    server.perms.read(|d| {
        if let Some(b) = d.banned(profile.uuid) {
            return Some(format!("이 서버에서 차단되었습니다.\n사유: {}", b.reason));
        }
        if let Some(b) = d.ip_banned(ip) {
            return Some(format!("이 IP는 차단되었습니다.\n사유: {}", b.reason));
        }
        if d.whitelist_enabled && level == 0 && !d.whitelisted(profile.uuid) {
            return Some("화이트리스트에 없습니다.".to_owned());
        }
        None
    })
}

/// 오프라인: 바닐라처럼 이름에서 UUID를 만들고, 설정에 따라 같은 이름의 정품 스킨을 빌립니다.
async fn offline_profile(server: &Server, name: &str) -> Profile {
    let properties = if server.config.offline_skins {
        match server.sessions.profile_by_name(name).await {
            Ok(Some(p)) => p.properties,
            Ok(None) => Vec::new(),
            Err(e) => {
                eprintln!("{name}의 스킨을 가져오지 못했습니다: {e}");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    Profile {
        name: name.to_owned(),
        uuid: offline_uuid(name),
        properties: Arc::new(properties),
    }
}

/// 정품 인증. 실패하면 클라이언트에게 보일 사유.
async fn authenticate<S>(
    conn: &mut Conn<S>,
    server: &Server,
    name: &str,
) -> Result<Result<Profile, String>, FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let key = server.key();
    let public_der = key.public.to_der();
    let token = random::bytes::<4>();
    conn.write_packet(
        login::ENCRYPTION_REQUEST_ID,
        &login::encode_encryption_request(&public_der, &token, true),
    )
    .await?;

    let packet = expect(conn, login::ENCRYPTION_RESPONSE_ID).await?;
    let (secret_enc, token_enc) = login::decode_encryption_response(&packet.body)?;
    let Some(echoed) = key.decrypt(&token_enc) else {
        return Ok(Err("암호화 응답을 풀 수 없습니다.".into()));
    };
    if echoed != token {
        return Ok(Err("검증 토큰이 다릅니다.".into()));
    }
    let Some(secret) = key
        .decrypt(&secret_enc)
        .and_then(|s| <[u8; 16]>::try_from(s).ok())
    else {
        return Ok(Err("공유 비밀이 올바르지 않습니다.".into()));
    };
    // 이 다음 바이트부터 양방향 모두 암호화됩니다.
    conn.enable_encryption(&secret);

    // 서버 ID("") + 공유 비밀 + 공개 키 DER의 SHA-1
    let mut h = sha1::Sha1::default();
    h.update(&secret);
    h.update(&public_der);
    let server_hash = sha1::minecraft_hex_digest(&h.finish());

    match server.sessions.has_joined(name, &server_hash).await {
        Ok(Some(p)) => Ok(Ok(Profile {
            name: p.name,
            uuid: p.uuid,
            properties: Arc::new(p.properties),
        })),
        Ok(None) => Ok(Err(
            "정품 인증에 실패했습니다. 런처를 다시 시작해 보세요.".into()
        )),
        Err(e) => {
            eprintln!("세션 서버 오류: {e}");
            Ok(Err(
                "인증 서버에 연결할 수 없습니다. 잠시 뒤 다시 시도하세요.".into(),
            ))
        }
    }
}

fn eof() -> FrameError {
    FrameError::Io(std::io::ErrorKind::UnexpectedEof.into())
}
