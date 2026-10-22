//! Login 상태. 오프라인 모드만 지원합니다 — 암호화(Encryption Request)와 압축(Set
//! Compression)은 보내지 않으므로, 이후 모든 패킷이 평문·비압축입니다.

use crate::json::{escape, text_component};
use crate::protocol::DecodeError;
use crate::protocol::types::{read_string, read_uuid, write_string, write_uuid};
use crate::protocol::varint::write_varint;

// Serverbound
pub const HELLO_ID: i32 = 0x00;
pub const LOGIN_ACKNOWLEDGED_ID: i32 = 0x03;

// Clientbound
/// `Disconnect (login)` (`login_disconnect`).
pub const DISCONNECT_ID: i32 = 0x00;
pub const LOGIN_FINISHED_ID: i32 = 0x02;

/// Login 단계의 Disconnect 사유는 아직 NBT가 아니라 JSON 텍스트 컴포넌트입니다.
pub fn encode_disconnect(reason: &str) -> Vec<u8> {
    let mut out = Vec::new();
    write_string(&mut out, &text_component(&escape(reason)));
    out
}

/// Serverbound `Hello` (Login Start).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub name: String,
    /// 클라이언트가 주장하는 UUID. 오프라인 모드에서는 믿지 않습니다.
    pub uuid: u128,
}

impl Hello {
    pub fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        let name = read_string(&mut buf, 16)?;
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(DecodeError::InvalidValue("player name"));
        }
        let uuid = read_uuid(&mut buf)?;
        Ok(Self { name, uuid })
    }
}

/// Clientbound `Login Finished`. 26.2부터 끝에 세션 UUID가 붙었습니다.
pub fn encode_login_finished(uuid: u128, name: &str, session: u128) -> Vec<u8> {
    let mut out = Vec::new();
    write_uuid(&mut out, uuid);
    write_string(&mut out, name);
    write_varint(&mut out, 0); // 프로필 속성(스킨 등) 없음
    write_uuid(&mut out, session);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_rejects_bad_names() {
        let mut body = Vec::new();
        write_string(&mut body, "bad name!");
        write_uuid(&mut body, 0);
        assert!(Hello::decode(&body).is_err());

        let mut body = Vec::new();
        write_string(&mut body, "Steve_01");
        write_uuid(&mut body, 7);
        assert_eq!(
            Hello::decode(&body),
            Ok(Hello {
                name: "Steve_01".into(),
                uuid: 7
            })
        );
    }
}
