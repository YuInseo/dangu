//! 연결 하나의 시작: Handshaking → Status | Login. Login 이후는 [`crate::session`].

use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::timeout;

use crate::network::Network;
use crate::packets::handshake::{HANDSHAKE_ID, Handshake, Intent};
use crate::packets::login;
use crate::packets::status;
use crate::protocol::conn::Conn;
use crate::protocol::frame::{FrameError, RawPacket};
use crate::protocol::{DecodeError, PROTOCOL_VERSION, VERSION_NAME};
use crate::server::Server;
use crate::session;

/// 아무 패킷도 오지 않을 때 연결을 붙잡고 있는 시간. Login에서 세션 서버를 기다리는 동안은
/// 클라이언트도 기다리므로 넉넉히 둡니다.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// 레거시(1.6 이하) 서버 목록 핑의 첫 바이트. 현재 프레임 형식과 섞이지 않습니다.
const LEGACY_PING: u8 = 0xFE;

pub async fn handle<S>(
    stream: S,
    net: &std::sync::Arc<Network>,
    ip: String,
) -> Result<(), FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut conn = Conn::new(stream);

    let first = timeout(READ_TIMEOUT, conn.r.peek_byte())
        .await
        .map_err(|_| timed_out())??;
    if first == Some(LEGACY_PING) {
        // 1.6 이하 형식의 서버 목록 핑 (오래된 런처·서버 목록 사이트·일부 클라이언트의 첫 시도).
        // 그 형식대로 답해 두면 버전이 맞지 않는다고 보여 줍니다.
        let info = &net.default_instance();
        let reply = legacy_ping_reply(
            &info.info.description,
            info.online_total(),
            info.info.max_players,
        );
        conn.write_raw(&reply).await?;
        return Ok(());
    }

    let Some(packet) = next(&mut conn).await? else {
        return Ok(());
    };
    if packet.id != HANDSHAKE_ID {
        return Err(unknown("handshaking", packet.id));
    }
    let handshake = Handshake::decode(&packet.body)?;

    match handshake.intent {
        // 버전이 달라도 Status에는 답합니다 — 클라이언트는 우리 버전 이름을 빨간 글씨로
        // 보여 주며 "호환되지 않음"을 표시합니다.
        Intent::Status => handle_status(&mut conn, &net.default_instance()).await,
        Intent::Login | Intent::Transfer => {
            if handshake.protocol_version != PROTOCOL_VERSION {
                let reason = version_mismatch_message(handshake.protocol_version);
                conn.write_packet(login::DISCONNECT_ID, &login::encode_disconnect(&reason))
                    .await?;
                return Ok(());
            }
            let (host, ip) = forwarded(&handshake.server_address, ip);
            session::run(conn, net, ip, (clean_host(&host), handshake.server_port)).await
        }
    }
}

/// 레거시 핑 답: `0xFF`, UTF-16BE 글자 수, `§1\0프로토콜\0버전\0설명\0접속자\0최대`.
pub(crate) fn legacy_ping_reply(motd: &str, online: u32, max: u32) -> Vec<u8> {
    let s = format!("§1\0{PROTOCOL_VERSION}\0{VERSION_NAME}\0{motd}\0{online}\0{max}");
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut out = vec![0xFF];
    out.extend_from_slice(&(units.len() as u16).to_be_bytes());
    for u in units {
        out.extend_from_slice(&u.to_be_bytes());
    }
    out
}

/// 클라이언트가 친 주소에서 덧붙은 것을 뗍니다: Forge `\0FML3\0`, 끝의 `.`(SRV로 푼 이름), 공백.
fn clean_host(host: &str) -> String {
    host.split('\0')
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('.')
        .to_owned()
}

/// 담비 마스터 프록시 뒤라면 핸드셰이크 주소 칸이 `원래주소\0진짜IP`입니다. 프록시 뒤에서만
/// (자식 인스턴스는 127.0.0.1에서만 들음) 믿습니다.
fn forwarded(address: &str, peer_ip: String) -> (String, String) {
    match address.split_once('\0') {
        Some((host, real)) if crate::dambi::child::active() && is_loopback(&peer_ip) => (
            host.to_owned(),
            real.split('\0').next().unwrap_or(real).to_owned(),
        ),
        Some((host, _)) => (host.to_owned(), peer_ip),
        None => (address.to_owned(), peer_ip),
    }
}

fn is_loopback(ip: &str) -> bool {
    ip.parse::<std::net::IpAddr>()
        .is_ok_and(|a| a.is_loopback())
}

async fn handle_status<S>(conn: &mut Conn<S>, server: &Server) -> Result<(), FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let Some(packet) = next(conn).await? else {
            return Ok(());
        };
        match packet.id {
            status::STATUS_REQUEST_ID => {
                let body = status::encode_status_response(&server.info, server.online_total());
                conn.write_packet(status::STATUS_RESPONSE_ID, &body).await?;
            }
            status::PING_REQUEST_ID => {
                let payload = status::decode_ping(&packet.body)?;
                conn.write_packet(status::PONG_RESPONSE_ID, &status::encode_pong(payload))
                    .await?;
                // 바닐라 서버도 Pong 뒤에 연결을 닫습니다.
                return Ok(());
            }
            id => return Err(unknown("status", id)),
        }
    }
}

pub fn version_mismatch_message(client_protocol: i32) -> String {
    if client_protocol < PROTOCOL_VERSION {
        format!("클라이언트가 오래되었습니다. {VERSION_NAME}을(를) 사용해 주세요.")
    } else {
        format!("서버가 오래되었습니다. 이 서버는 {VERSION_NAME}입니다.")
    }
}

/// 시간 제한을 둔 다음 패킷.
pub(crate) async fn next<S>(conn: &mut Conn<S>) -> Result<Option<RawPacket>, FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    timeout(READ_TIMEOUT, conn.read_packet())
        .await
        .map_err(|_| timed_out())?
}

fn timed_out() -> FrameError {
    FrameError::Io(std::io::ErrorKind::TimedOut.into())
}

pub(crate) fn unknown(state: &'static str, id: i32) -> FrameError {
    DecodeError::UnknownPacket { state, id }.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_legacy_ping() {
        assert_eq!(clean_host("play.example.com\0FML3\0"), "play.example.com");
        assert_eq!(clean_host("mc.example.com."), "mc.example.com");
        assert_eq!(clean_host(" localhost "), "localhost");
        let r = legacy_ping_reply("Dangu", 3, 100);
        assert_eq!(r[0], 0xFF);
        let n = u16::from_be_bytes([r[1], r[2]]) as usize;
        assert_eq!(r.len(), 3 + n * 2);
        let text: String =
            char::decode_utf16(r[3..].chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])))
                .map(|c| c.unwrap())
                .collect();
        assert!(
            text.starts_with("§1\0") && text.ends_with("\u{0}3\u{0}100"),
            "{text:?}"
        );
    }
}
