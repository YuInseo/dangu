//! 서명된 채팅 패킷.

use super::play::ChatKind;
use crate::chat::{ChatSession, SIGNATURE_LEN, Signature};
use crate::protocol::DecodeError;
use crate::protocol::nbt::Tag;
use crate::protocol::types::{
    read_i64, read_string, read_u8, read_uuid, write_bool, write_byte_array, write_i64,
    write_string, write_uuid,
};
use crate::protocol::varint::{read_varint, write_varint};

// Clientbound
pub const PLAYER_CHAT_ID: i32 = 0x42;
// Serverbound
pub const SB_CHAT_ACK_ID: i32 = 0x06;
pub const SB_CHAT_COMMAND_SIGNED_ID: i32 = 0x08;
pub const SB_CHAT_SESSION_UPDATE_ID: i32 = 0x0A;

/// `Player Info Update`의 "채팅 세션 알림" 동작 비트.
const INFO_INITIALIZE_CHAT: u8 = 0x02;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub message: String,
    pub timestamp: i64,
    pub salt: i64,
    pub signature: Option<Signature>,
    pub offset: i32,
    /// 읽음 비트 20개 (리틀엔디언 3바이트).
    pub acknowledged: u32,
}

fn bytes(buf: &mut &[u8], max: usize) -> Result<Vec<u8>, DecodeError> {
    let n = read_varint(buf)?;
    let n = usize::try_from(n)
        .ok()
        .filter(|&n| n <= max && n <= buf.len())
        .ok_or(DecodeError::InvalidValue("byte array"))?;
    let (h, r) = buf.split_at(n);
    *buf = r;
    Ok(h.to_vec())
}

pub fn decode_chat(mut buf: &[u8]) -> Result<ChatMessage, DecodeError> {
    let message = read_string(&mut buf, 256)?;
    let timestamp = read_i64(&mut buf)?;
    let salt = read_i64(&mut buf)?;
    let signature = if read_u8(&mut buf)? != 0 {
        let s: Signature = buf
            .get(..SIGNATURE_LEN)
            .ok_or(DecodeError::UnexpectedEof)?
            .try_into()
            .unwrap();
        buf = &buf[SIGNATURE_LEN..];
        Some(s)
    } else {
        None
    };
    let offset = read_varint(&mut buf)?;
    let ack = buf.get(..3).ok_or(DecodeError::UnexpectedEof)?;
    let acknowledged = u32::from_le_bytes([ack[0], ack[1], ack[2], 0]);
    Ok(ChatMessage {
        message,
        timestamp,
        salt,
        signature,
        offset,
        acknowledged,
    })
}

pub fn decode_chat_ack(mut buf: &[u8]) -> Result<i32, DecodeError> {
    read_varint(&mut buf)
}

pub fn decode_session_update(mut buf: &[u8]) -> Result<ChatSession, DecodeError> {
    let session_id = read_uuid(&mut buf)?;
    let expires_at = read_i64(&mut buf)?;
    let key_der = bytes(&mut buf, 512)?;
    let key_signature = bytes(&mut buf, 4096)?;
    Ok(ChatSession {
        session_id,
        expires_at,
        key_der,
        key_signature,
    })
}

/// 탭 목록의 그 사람에게 채팅 세션을 붙입니다 (받는 쪽이 서명을 확인할 수 있도록).
pub fn encode_info_chat_session(uuid: u128, session: &ChatSession) -> Vec<u8> {
    let mut out = vec![INFO_INITIALIZE_CHAT];
    write_varint(&mut out, 1);
    write_uuid(&mut out, uuid);
    write_bool(&mut out, true);
    write_uuid(&mut out, session.session_id);
    write_i64(&mut out, session.expires_at);
    write_byte_array(&mut out, &session.key_der);
    write_byte_array(&mut out, &session.key_signature);
    out
}

/// 서명째 전하는 플레이어 채팅.
pub struct SignedChat<'a> {
    pub global_index: i32,
    pub sender: u128,
    pub index: i32,
    pub signature: &'a Signature,
    pub content: &'a str,
    pub timestamp: i64,
    pub salt: i64,
    /// 서명에 들어간 "읽은 메시지" — 받는 쪽 캐시를 쓰지 않고 서명 전체로 보냅니다.
    pub last_seen: &'a [Signature],
    pub kind: ChatKind,
    pub sender_name: &'a Tag,
}

pub fn encode_player_chat(c: &SignedChat<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, c.global_index);
    write_uuid(&mut out, c.sender);
    write_varint(&mut out, c.index);
    write_bool(&mut out, true);
    out.extend_from_slice(c.signature);
    write_string(&mut out, c.content);
    write_i64(&mut out, c.timestamp);
    write_i64(&mut out, c.salt);
    write_varint(&mut out, c.last_seen.len() as i32);
    for s in c.last_seen {
        write_varint(&mut out, 0); // 0 = 캐시 번호 대신 서명 전체
        out.extend_from_slice(s);
    }
    write_bool(&mut out, false); // 서명 안 된 대체 내용 없음
    write_varint(&mut out, 0); // 필터: 그대로 보임
    write_varint(
        &mut out,
        crate::data::registry_id("chat_type", c.kind.registry_key()) + 1,
    );
    c.sender_name.write_root(&mut out);
    write_bool(&mut out, false); // 대상 없음
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_signed_chat() {
        let mut b = Vec::new();
        write_string(&mut b, "안녕");
        write_i64(&mut b, 1_700_000_000_123);
        write_i64(&mut b, -5);
        b.push(1);
        b.extend_from_slice(&[9u8; 256]);
        write_varint(&mut b, 1);
        b.extend_from_slice(&[0x00, 0x00, 0x08]); // 19번 비트
        b.push(0);
        let m = decode_chat(&b).unwrap();
        assert_eq!(m.message, "안녕");
        assert_eq!(m.signature, Some([9u8; 256]));
        assert_eq!((m.offset, m.acknowledged), (1, 1 << 19));
    }
}
