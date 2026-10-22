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
        // 아주 오래된 클라이언트나 일부 스캐너. 답하지 않고 닫습니다.
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
            session::run(conn, net, ip).await
        }
    }
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
