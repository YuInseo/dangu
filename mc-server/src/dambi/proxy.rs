//! 프록시 루프: 플레이어 TCP 연결을 받아 핸드셰이크와 Login Start만 읽고, 갈 인스턴스를 골라
//! 그 뒤로는 바이트를 그대로 잇습니다 (암호화·압축·정품 인증은 인스턴스가 직접).
//!
//! 인스턴스가 진짜 IP를 알 수 있게, 넘겨주는 핸드셰이크의 주소 칸을
//! `원래주소\0클라이언트IP`로 바꿉니다 (BungeeCord가 쓰던 방식). 인스턴스는 127.0.0.1에서만
//! 들으므로 이 칸을 위조해 들어올 수 없습니다.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use super::master::Master;
use crate::packets::handshake::{HANDSHAKE_ID, Handshake, Intent};
use crate::packets::{login, status};
use crate::protocol::types::{read_string, write_string};
use crate::protocol::varint::{read_varint, write_varint};

const FIRST_PACKETS_TIMEOUT: Duration = Duration::from_secs(10);
/// 핸드셰이크·Login Start는 작습니다. 이보다 크면 이상한 접속.
const MAX_EARLY_PACKET: usize = 4096;

pub async fn serve(listener: TcpListener, master: Arc<Master>) -> std::io::Result<()> {
    loop {
        let (stream, peer) = listener.accept().await?;
        stream.set_nodelay(true)?;
        let m = Arc::clone(&master);
        tokio::spawn(async move {
            if let Err(e) = handle(stream, peer.ip().to_string(), &m).await {
                eprintln!("[프록시] {peer}: {e}");
            }
        });
    }
}

/// 프레임 하나 (`길이 VarInt` + 패킷). (패킷 바이트, 읽은 원본 바이트 전부).
async fn read_frame(s: &mut TcpStream) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let mut raw = Vec::new();
    let mut len: usize = 0;
    for i in 0..3 {
        let b = s.read_u8().await?;
        raw.push(b);
        len |= ((b & 0x7F) as usize) << (7 * i);
        if b & 0x80 == 0 {
            break;
        }
        if i == 2 {
            return Err(std::io::Error::other("길이 VarInt가 너무 김"));
        }
    }
    if len == 0 || len > MAX_EARLY_PACKET {
        return Err(std::io::Error::other(format!("이상한 패킷 길이 {len}")));
    }
    let mut packet = vec![0; len];
    s.read_exact(&mut packet).await?;
    raw.extend_from_slice(&packet);
    Ok((packet, raw))
}

async fn write_frame(s: &mut TcpStream, id: i32, body: &[u8]) -> std::io::Result<()> {
    let mut packet = Vec::new();
    write_varint(&mut packet, id);
    packet.extend_from_slice(body);
    let mut frame = Vec::new();
    write_varint(&mut frame, packet.len() as i32);
    frame.extend_from_slice(&packet);
    s.write_all(&frame).await
}

fn split_id(packet: &[u8]) -> std::io::Result<(i32, &[u8])> {
    let mut buf = packet;
    let id = read_varint(&mut buf).map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    Ok((id, buf))
}

/// 인스턴스에 넘길 핸드셰이크 (주소 칸에 진짜 IP를 덧붙임).
pub fn forwarded_handshake(h: &Handshake, ip: &str) -> Vec<u8> {
    let mut body = Vec::new();
    write_varint(&mut body, h.protocol_version);
    write_string(&mut body, &format!("{}\0{ip}", h.server_address));
    body.extend_from_slice(&h.server_port.to_be_bytes());
    write_varint(
        &mut body,
        match h.intent {
            Intent::Status => 1,
            Intent::Login => 2,
            Intent::Transfer => 3,
        },
    );
    body
}

async fn handle(mut client: TcpStream, ip: String, master: &Master) -> std::io::Result<()> {
    let (packet, _) = timeout(FIRST_PACKETS_TIMEOUT, read_frame(&mut client))
        .await
        .map_err(|_| std::io::Error::other("핸드셰이크 시간 초과"))??;
    let (id, body) = split_id(&packet)?;
    if id != HANDSHAKE_ID {
        return Ok(());
    }
    let handshake = Handshake::decode(body).map_err(|e| std::io::Error::other(format!("{e:?}")))?;

    if handshake.intent == Intent::Status {
        // 서버 목록: 마스터가 직접 (모든 인스턴스 접속자 합)
        loop {
            let (packet, _) = timeout(FIRST_PACKETS_TIMEOUT, read_frame(&mut client))
                .await
                .map_err(|_| std::io::Error::other("상태 요청 시간 초과"))??;
            let (id, body) = split_id(&packet)?;
            match id {
                status::STATUS_REQUEST_ID => {
                    let resp = status::encode_status_response(&master.info, master.online_total());
                    write_frame(&mut client, status::STATUS_RESPONSE_ID, &resp).await?;
                }
                status::PING_REQUEST_ID => {
                    let payload = status::decode_ping(body)
                        .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
                    write_frame(
                        &mut client,
                        status::PONG_RESPONSE_ID,
                        &status::encode_pong(payload),
                    )
                    .await?;
                    return Ok(());
                }
                _ => return Ok(()),
            }
        }
    }

    // Login Start: 이름 (+ UUID). 이름으로 갈 곳을 고릅니다.
    let (packet, login_raw) = timeout(FIRST_PACKETS_TIMEOUT, read_frame(&mut client))
        .await
        .map_err(|_| std::io::Error::other("로그인 시간 초과"))??;
    let (id, mut body) = split_id(&packet)?;
    if id != login::HELLO_ID {
        return Ok(());
    }
    let name = read_string(&mut body, 16).map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    let (instance, port) = match master.route(&name) {
        Ok(r) => r,
        Err(reason) => {
            write_frame(
                &mut client,
                login::DISCONNECT_ID,
                &login::encode_disconnect(&reason),
            )
            .await?;
            return Ok(());
        }
    };
    let mut backend = match TcpStream::connect(("127.0.0.1", port)).await {
        Ok(b) => b,
        Err(e) => {
            let reason = format!("'{instance}'에 연결하지 못했습니다: {e}");
            write_frame(
                &mut client,
                login::DISCONNECT_ID,
                &login::encode_disconnect(&reason),
            )
            .await?;
            return Ok(());
        }
    };
    backend.set_nodelay(true)?;
    println!("[프록시] {name} ({ip}) → {instance}");
    write_frame(
        &mut backend,
        HANDSHAKE_ID,
        &forwarded_handshake(&handshake, &ip),
    )
    .await?;
    backend.write_all(&login_raw).await?;
    // 여기서부터는 그냥 잇기. 한쪽이 닫으면 끝.
    let _ = tokio::io::copy_bidirectional(&mut client, &mut backend).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarded_handshake_carries_the_real_ip() {
        let h = Handshake {
            protocol_version: crate::protocol::PROTOCOL_VERSION,
            server_address: "play.dambi.kr".into(),
            server_port: 25565,
            intent: Intent::Transfer,
        };
        let body = forwarded_handshake(&h, "203.0.113.7");
        let back = Handshake::decode(&body).unwrap();
        assert_eq!(back.server_address, "play.dambi.kr\u{0}203.0.113.7");
        assert_eq!(back.intent, Intent::Transfer);
        assert_eq!(back.server_port, 25565);
    }
}
