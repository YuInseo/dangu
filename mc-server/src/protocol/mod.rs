//! 바이트 수준의 프로토콜 코덱. 소켓은 모르고, 바이트 슬라이스와 `Vec<u8>`만 다룹니다.
//! 소켓과 맞닿는 부분은 [`frame`]에 있습니다.

pub mod frame;
pub mod types;
pub mod varint;

use std::fmt;

/// 26.3 (Wilderness Bound) 정식판의 프로토콜 번호.
pub const PROTOCOL_VERSION: i32 = 777;
/// 서버 목록에 그대로 찍히는 버전 이름.
pub const VERSION_NAME: &str = "26.3";

/// 바이트를 값으로 바꾸다 실패한 이유.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// 값을 다 읽기 전에 바이트가 끝났습니다.
    UnexpectedEof,
    /// VarInt가 5바이트를 넘었습니다.
    VarIntTooLong,
    /// VarLong이 10바이트를 넘었습니다.
    VarLongTooLong,
    /// 문자열 길이가 음수이거나 허용치를 넘었습니다.
    StringTooLong { len: i32, max: usize },
    /// 문자열이 UTF-8이 아닙니다.
    InvalidUtf8,
    /// 패킷 길이 접두사가 0 이하이거나 허용치를 넘었습니다.
    BadPacketLength(i32),
    /// 지금 상태에서 모르는 패킷 ID입니다.
    UnknownPacket { state: &'static str, id: i32 },
    /// 필드 값이 명세 범위 밖입니다.
    InvalidValue(&'static str),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of packet"),
            Self::VarIntTooLong => write!(f, "VarInt is longer than 5 bytes"),
            Self::VarLongTooLong => write!(f, "VarLong is longer than 10 bytes"),
            Self::StringTooLong { len, max } => {
                write!(f, "string length {len} is out of range (max {max})")
            }
            Self::InvalidUtf8 => write!(f, "string is not valid UTF-8"),
            Self::BadPacketLength(len) => write!(f, "bad packet length {len}"),
            Self::UnknownPacket { state, id } => {
                write!(f, "unknown packet 0x{id:02X} in {state} state")
            }
            Self::InvalidValue(what) => write!(f, "invalid value: {what}"),
        }
    }
}

impl std::error::Error for DecodeError {}
