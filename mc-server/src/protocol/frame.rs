//! 패킷 프레임: `VarInt 길이 | VarInt 패킷 ID | 데이터`.
//!
//! 압축(Set Compression)과 암호화를 켜기 전의 평문 프레임만 다룹니다. 둘 다 Login
//! 단계 이후에 붙으므로 Handshake·Status에는 필요 없습니다.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::DecodeError;
use super::varint::{VARINT_MAX_LEN, read_varint, varint_len, write_varint};

/// 길이 접두사의 상한. 바닐라와 같이 3바이트 VarInt로 담을 수 있는 최대값입니다.
pub const MAX_PACKET_LEN: i32 = 2_097_151;

/// 소켓에서 패킷 하나를 읽다 실패한 이유.
#[derive(Debug)]
pub enum FrameError {
    Io(std::io::Error),
    Decode(DecodeError),
}

impl From<std::io::Error> for FrameError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<DecodeError> for FrameError {
    fn from(e: DecodeError) -> Self {
        Self::Decode(e)
    }
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Decode(e) => write!(f, "decode: {e}"),
        }
    }
}

impl std::error::Error for FrameError {}

/// 디코딩된 프레임 하나. `body`는 패킷 ID 뒤의 데이터입니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPacket {
    pub id: i32,
    pub body: Vec<u8>,
}

/// 스트림에서 VarInt를 한 바이트씩 읽습니다. 길이 접두사처럼 "몇 바이트를 읽어야
/// 할지 아직 모를 때" 씁니다. 연결이 패킷 사이에서 깔끔하게 끊기면 `Ok(None)`.
async fn read_varint_stream<R: AsyncRead + Unpin>(r: &mut R) -> Result<Option<i32>, FrameError> {
    let mut bytes = [0u8; VARINT_MAX_LEN];
    for i in 0..VARINT_MAX_LEN {
        let byte = match r.read_u8().await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && i == 0 => {
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        };
        bytes[i] = byte;
        if byte & 0x80 == 0 {
            return Ok(Some(read_varint(&mut &bytes[..=i])?));
        }
    }
    Err(DecodeError::VarIntTooLong.into())
}

/// 패킷 하나를 읽습니다. 상대가 패킷 경계에서 연결을 닫았으면 `Ok(None)`.
pub async fn read_packet<R: AsyncRead + Unpin>(r: &mut R) -> Result<Option<RawPacket>, FrameError> {
    let Some(len) = read_varint_stream(r).await? else {
        return Ok(None);
    };
    if !(1..=MAX_PACKET_LEN).contains(&len) {
        return Err(DecodeError::BadPacketLength(len).into());
    }

    let mut frame = vec![0u8; len as usize];
    r.read_exact(&mut frame).await?;

    let mut buf = frame.as_slice();
    let id = read_varint(&mut buf)?;
    let body = buf.to_vec();
    Ok(Some(RawPacket { id, body }))
}

/// `id`와 `body`를 한 프레임으로 묶은 바이트. 소켓 없이 테스트할 수 있도록 따로 둡니다.
pub fn encode_packet(id: i32, body: &[u8]) -> Vec<u8> {
    let inner_len = varint_len(id) + body.len();
    let mut out = Vec::with_capacity(varint_len(inner_len as i32) + inner_len);
    write_varint(&mut out, inner_len as i32);
    write_varint(&mut out, id);
    out.extend_from_slice(body);
    out
}

/// 패킷 하나를 써서 내보냅니다.
pub async fn write_packet<W: AsyncWrite + Unpin>(
    w: &mut W,
    id: i32,
    body: &[u8],
) -> std::io::Result<()> {
    w.write_all(&encode_packet(id, body)).await?;
    w.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frame_round_trip() {
        let bytes = encode_packet(0x01, &[1, 2, 3]);
        assert_eq!(bytes, [4, 0x01, 1, 2, 3]);

        let mut r = bytes.as_slice();
        let p = read_packet(&mut r).await.unwrap().unwrap();
        assert_eq!(
            p,
            RawPacket {
                id: 1,
                body: vec![1, 2, 3]
            }
        );
        assert!(read_packet(&mut r).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn rejects_oversized_length() {
        // 2097152 = 0x200000 → 80 80 80 01
        let mut r: &[u8] = &[0x80, 0x80, 0x80, 0x01];
        assert!(matches!(
            read_packet(&mut r).await,
            Err(FrameError::Decode(DecodeError::BadPacketLength(2_097_152)))
        ));
    }
}
