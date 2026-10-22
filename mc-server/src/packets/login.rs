//! Login 상태. 오프라인 모드만 지원합니다 — 암호화(Encryption Request)와 압축(Set
//! Compression)은 보내지 않으므로, 이후 모든 패킷이 평문·비압축입니다.

use crate::auth::Property;
use crate::json::{escape, text_component};
use crate::protocol::DecodeError;
use crate::protocol::types::{
    read_string, read_uuid, write_bool, write_byte_array, write_string, write_uuid,
};
use crate::protocol::varint::{read_varint, write_varint};

// Serverbound
pub const HELLO_ID: i32 = 0x00;
pub const ENCRYPTION_RESPONSE_ID: i32 = 0x01;
pub const LOGIN_ACKNOWLEDGED_ID: i32 = 0x03;

// Clientbound
/// `Disconnect (login)` (`login_disconnect`).
pub const DISCONNECT_ID: i32 = 0x00;
pub const ENCRYPTION_REQUEST_ID: i32 = 0x01;
pub const LOGIN_FINISHED_ID: i32 = 0x02;
pub const SET_COMPRESSION_ID: i32 = 0x03;

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
pub fn encode_login_finished(
    uuid: u128,
    name: &str,
    properties: &[Property],
    session: u128,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_uuid(&mut out, uuid);
    write_string(&mut out, name);
    write_properties(&mut out, properties);
    write_uuid(&mut out, session);
    out
}

/// 프로필 속성 배열: 이름, 값, (있으면) 서명.
pub fn write_properties(out: &mut Vec<u8>, properties: &[Property]) {
    write_varint(out, properties.len() as i32);
    for p in properties {
        write_string(out, &p.name);
        write_string(out, &p.value);
        match &p.signature {
            Some(sig) => {
                write_bool(out, true);
                write_string(out, sig);
            }
            None => write_bool(out, false),
        }
    }
}

/// `Encryption Request`: 서버 ID(빈 문자열), 공개 키(DER), 검증 토큰, 세션 서버 인증 여부.
pub fn encode_encryption_request(
    public_key_der: &[u8],
    verify_token: &[u8],
    authenticate: bool,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_string(&mut out, "");
    write_byte_array(&mut out, public_key_der);
    write_byte_array(&mut out, verify_token);
    write_bool(&mut out, authenticate);
    out
}

/// `Encryption Response`: (RSA로 암호화한) 공유 비밀과 검증 토큰.
pub fn decode_encryption_response(mut buf: &[u8]) -> Result<(Vec<u8>, Vec<u8>), DecodeError> {
    let mut bytes = || -> Result<Vec<u8>, DecodeError> {
        let len = read_varint(&mut buf)?;
        if !(0..=1024).contains(&len) || buf.len() < len as usize {
            return Err(DecodeError::InvalidValue("byte array length"));
        }
        let (head, rest) = buf.split_at(len as usize);
        buf = rest;
        Ok(head.to_vec())
    };
    let secret = bytes()?;
    let token = bytes()?;
    Ok((secret, token))
}

pub fn encode_set_compression(threshold: usize) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, threshold as i32);
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
