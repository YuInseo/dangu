//! 연결 하나의 상태 기계: Handshaking → Status | Login.

use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio::time::timeout;

use crate::packets::handshake::{HANDSHAKE_ID, Handshake, Intent};
use crate::packets::login;
use crate::packets::status::{self, ServerInfo};
use crate::protocol::frame::{FrameError, RawPacket, read_packet, write_packet};
use crate::protocol::{DecodeError, PROTOCOL_VERSION, VERSION_NAME};

/// 아무 패킷도 오지 않을 때 연결을 붙잡고 있는 시간.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// 레거시(1.6 이하) 서버 목록 핑의 첫 바이트. 현재 프레임 형식과 섞이지 않습니다.
const LEGACY_PING: u8 = 0xFE;

pub async fn handle<S>(stream: S, info: &ServerInfo) -> Result<(), FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut stream = BufReader::new(stream);

    if peek_byte(&mut stream).await? == Some(LEGACY_PING) {
        // 아주 오래된 클라이언트나 일부 스캐너. 답하지 않고 닫습니다.
        return Ok(());
    }

    let Some(packet) = next(&mut stream).await? else {
        return Ok(());
    };
    if packet.id != HANDSHAKE_ID {
        return Err(unknown("handshaking", packet.id));
    }
    let handshake = Handshake::decode(&packet.body)?;

    match handshake.intent {
        // 버전이 달라도 Status에는 답합니다 — 클라이언트는 우리 버전 이름을 빨간 글씨로
        // 보여 주며 "호환되지 않음"을 표시합니다.
        Intent::Status => handle_status(&mut stream, info).await,
        Intent::Login | Intent::Transfer => {
            if handshake.protocol_version != PROTOCOL_VERSION {
                let reason = version_mismatch_message(handshake.protocol_version);
                write_packet(
                    &mut stream,
                    login::DISCONNECT_ID,
                    &login::encode_disconnect(&reason),
                )
                .await?;
                return Ok(());
            }
            // 다음 단계: Login Start → (암호화) → Login Success → Configuration.
            let reason = "로그인은 아직 구현되지 않았습니다.";
            write_packet(
                &mut stream,
                login::DISCONNECT_ID,
                &login::encode_disconnect(reason),
            )
            .await?;
            Ok(())
        }
    }
}

async fn handle_status<S>(stream: &mut S, info: &ServerInfo) -> Result<(), FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let Some(packet) = next(stream).await? else {
            return Ok(());
        };
        match packet.id {
            status::STATUS_REQUEST_ID => {
                write_packet(
                    stream,
                    status::STATUS_RESPONSE_ID,
                    &status::encode_status_response(info),
                )
                .await?;
            }
            status::PING_REQUEST_ID => {
                let payload = status::decode_ping(&packet.body)?;
                write_packet(
                    stream,
                    status::PONG_RESPONSE_ID,
                    &status::encode_pong(payload),
                )
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

async fn next<R: AsyncRead + Unpin>(r: &mut R) -> Result<Option<RawPacket>, FrameError> {
    timeout(READ_TIMEOUT, read_packet(r))
        .await
        .map_err(|_| FrameError::Io(std::io::ErrorKind::TimedOut.into()))?
}

async fn peek_byte<R: AsyncRead + Unpin>(r: &mut BufReader<R>) -> Result<Option<u8>, FrameError> {
    use tokio::io::AsyncBufReadExt;
    let buf = timeout(READ_TIMEOUT, r.fill_buf())
        .await
        .map_err(|_| FrameError::Io(std::io::ErrorKind::TimedOut.into()))??;
    Ok(buf.first().copied())
}

fn unknown(state: &'static str, id: i32) -> FrameError {
    DecodeError::UnknownPacket { state, id }.into()
}
