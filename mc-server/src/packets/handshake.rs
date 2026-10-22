//! Handshaking 상태. 클라이언트가 보내는 첫 패킷이자 유일한 패킷입니다.

use crate::protocol::DecodeError;
use crate::protocol::types::{read_string, read_u16};
use crate::protocol::varint::read_varint;

/// Serverbound `Handshake` (`intention`).
pub const HANDSHAKE_ID: i32 = 0x00;

/// 핸드셰이크 뒤에 클라이언트가 가려는 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// 1 — 서버 목록 핑.
    Status,
    /// 2 — 실제 접속.
    Login,
    /// 3 — 다른 서버에서 Transfer 패킷으로 넘어온 접속.
    Transfer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handshake {
    pub protocol_version: i32,
    /// 클라이언트가 입력한 주소. SRV 레코드를 거쳤다면 풀린 뒤의 호스트명입니다.
    pub server_address: String,
    pub server_port: u16,
    pub intent: Intent,
}

impl Handshake {
    pub fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        let protocol_version = read_varint(&mut buf)?;
        // 바닐라는 255자지만, 프록시·모드 로더(Forge `\0FML3\0`, BungeeCord IP 전달, TCPShield 등)가
        // 덧붙이는 것까지 받습니다.
        let server_address = read_string(&mut buf, 32767)?;
        let server_port = read_u16(&mut buf)?;
        let intent = match read_varint(&mut buf)? {
            1 => Intent::Status,
            2 => Intent::Login,
            3 => Intent::Transfer,
            _ => return Err(DecodeError::InvalidValue("handshake intent")),
        };
        // 뒤에 남는 바이트는 무시합니다 (몇몇 클라이언트·프록시가 덧붙임).
        Ok(Self {
            protocol_version,
            server_address,
            server_port,
            intent,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::write_string;
    use crate::protocol::varint::write_varint;

    #[test]
    fn decodes_status_handshake() {
        let mut body = Vec::new();
        write_varint(&mut body, 777);
        write_string(&mut body, "localhost");
        body.extend_from_slice(&25565u16.to_be_bytes());
        write_varint(&mut body, 1);

        assert_eq!(
            Handshake::decode(&body),
            Ok(Handshake {
                protocol_version: 777,
                server_address: "localhost".into(),
                server_port: 25565,
                intent: Intent::Status,
            })
        );
    }

    #[test]
    fn tolerates_long_addresses_and_extra_bytes() {
        let mut body = Vec::new();
        write_varint(&mut body, 777);
        let long = format!("play.example.com\0{}", "x".repeat(400));
        write_string(&mut body, &long);
        body.extend_from_slice(&25565u16.to_be_bytes());
        write_varint(&mut body, 2);
        body.extend_from_slice(&[1, 2, 3]);
        let h = Handshake::decode(&body).unwrap();
        assert_eq!(h.server_address, long);
        assert_eq!(h.intent, Intent::Login);
    }

    #[test]
    fn rejects_unknown_intent() {
        let mut body = Vec::new();
        write_varint(&mut body, 777);
        write_string(&mut body, "a");
        body.extend_from_slice(&[0, 0]);
        write_varint(&mut body, 9);
        assert!(Handshake::decode(&body).is_err());
    }
}
