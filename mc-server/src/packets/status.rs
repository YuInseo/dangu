//! Status 상태: 멀티플레이어 목록의 서버 정보와 핑.
//!
//! 순서: (C→S) Status Request → (S→C) Status Response → (C→S) Ping Request →
//! (S→C) Pong Response, 그리고 서버가 연결을 닫습니다.

use crate::json::escape;
use crate::protocol::types::{read_i64, write_i64, write_string};
use crate::protocol::{DecodeError, PROTOCOL_VERSION, VERSION_NAME};

/// Serverbound `Status Request` (데이터 없음).
pub const STATUS_REQUEST_ID: i32 = 0x00;
/// Serverbound `Ping Request`.
pub const PING_REQUEST_ID: i32 = 0x01;
/// Clientbound `Status Response`.
pub const STATUS_RESPONSE_ID: i32 = 0x00;
/// Clientbound `Pong Response`.
pub const PONG_RESPONSE_ID: i32 = 0x01;

/// 서버 목록에 보일 내용.
#[derive(Debug, Clone)]
pub struct ServerInfo {
    /// MOTD 첫 줄. 프로토콜에 "서버 이름" 필드는 따로 없어서 MOTD 첫 줄을 이름처럼 씁니다.
    pub name: String,
    /// MOTD 둘째 줄.
    pub description: String,
    pub max_players: u32,
}

impl ServerInfo {
    /// Status Response의 JSON 문자열. serde 없이 손으로 씁니다 — 필드가 고정이고,
    /// 사람이 넣는 값은 모두 [`escape`]를 거칩니다.
    pub fn to_json(&self, online_players: u32) -> String {
        format!(
            concat!(
                r#"{{"version":{{"name":"{version}","protocol":{protocol}}},"#,
                r#""players":{{"max":{max},"online":{online},"sample":[]}},"#,
                r#""description":{{"text":"","extra":["#,
                r#"{{"text":"{name}","color":"gold","bold":true}},"#,
                r#"{{"text":"\n"}},"#,
                r#"{{"text":"{description}","color":"gray"}}"#,
                r#"]}},"#,
                r#""enforcesSecureChat":false}}"#,
            ),
            version = escape(VERSION_NAME),
            protocol = PROTOCOL_VERSION,
            max = self.max_players,
            online = online_players,
            name = escape(&self.name),
            description = escape(&self.description),
        )
    }
}

pub fn encode_status_response(info: &ServerInfo, online_players: u32) -> Vec<u8> {
    let mut out = Vec::new();
    write_string(&mut out, &info.to_json(online_players));
    out
}

/// Ping Request의 페이로드. 클라이언트가 고른 아무 값이고, Pong으로 그대로 돌려줍니다.
pub fn decode_ping(mut buf: &[u8]) -> Result<i64, DecodeError> {
    read_i64(&mut buf)
}

pub fn encode_pong(payload: i64) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    write_i64(&mut out, payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_has_the_expected_fields() {
        let json = ServerInfo {
            name: "Hoya \"MC\"".into(),
            description: "순수 Rust 커스텀 서버입니다".into(),
            max_players: 100,
        }
        .to_json(0);

        assert!(json.contains(r#""version":{"name":"26.3","protocol":777}"#));
        assert!(json.contains(r#""players":{"max":100,"online":0,"sample":[]}"#));
        assert!(json.contains(r#""text":"Hoya \"MC\"""#));
        assert!(json.contains("순수 Rust 커스텀 서버입니다"));
    }

    #[test]
    fn pong_echoes_payload() {
        let payload = 0x1122334455667788;
        assert_eq!(decode_ping(&encode_pong(payload)), Ok(payload));
    }
}
