//! VarInt 외의 기본 타입. 고정 길이 정수는 모두 빅엔디언입니다.

use super::DecodeError;
use super::varint::{read_varint, write_varint};

/// 문자열의 최대 길이(UTF-16 코드 유닛 기준)가 `max`일 때, 바이트 길이 상한.
/// 바닐라와 같이 "코드 유닛 하나가 UTF-8로 최대 3바이트"로 셈합니다.
fn max_bytes(max_chars: usize) -> usize {
    max_chars * 3
}

fn take<'a>(buf: &mut &'a [u8], n: usize) -> Result<&'a [u8], DecodeError> {
    if buf.len() < n {
        return Err(DecodeError::UnexpectedEof);
    }
    let (head, rest) = buf.split_at(n);
    *buf = rest;
    Ok(head)
}

/// `String (n)`: VarInt 바이트 길이 + UTF-8.
pub fn read_string(buf: &mut &[u8], max_chars: usize) -> Result<String, DecodeError> {
    let len = read_varint(buf)?;
    let too_long = DecodeError::StringTooLong {
        len,
        max: max_chars,
    };
    let byte_len = usize::try_from(len).map_err(|_| too_long.clone())?;
    if byte_len > max_bytes(max_chars) {
        return Err(too_long);
    }
    let bytes = take(buf, byte_len)?;
    let s = std::str::from_utf8(bytes).map_err(|_| DecodeError::InvalidUtf8)?;
    if s.encode_utf16().count() > max_chars {
        return Err(too_long);
    }
    Ok(s.to_owned())
}

pub fn write_string(out: &mut Vec<u8>, s: &str) {
    write_varint(out, s.len() as i32);
    out.extend_from_slice(s.as_bytes());
}

pub fn read_u16(buf: &mut &[u8]) -> Result<u16, DecodeError> {
    let b = take(buf, 2)?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}

pub fn read_i64(buf: &mut &[u8]) -> Result<i64, DecodeError> {
    let b = take(buf, 8)?;
    Ok(i64::from_be_bytes(b.try_into().unwrap()))
}

pub fn write_i64(out: &mut Vec<u8>, v: i64) {
    out.extend_from_slice(&v.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_round_trip() {
        let mut out = Vec::new();
        write_string(&mut out, "순수 Rust");
        let mut buf = out.as_slice();
        assert_eq!(read_string(&mut buf, 32).unwrap(), "순수 Rust");
        assert!(buf.is_empty());
    }

    #[test]
    fn string_limit_counts_utf16_units() {
        let mut out = Vec::new();
        write_string(&mut out, "가나다");
        assert!(read_string(&mut out.as_slice(), 3).is_ok());
        assert!(matches!(
            read_string(&mut out.as_slice(), 2),
            Err(DecodeError::StringTooLong { .. })
        ));
    }

    #[test]
    fn fixed_width_is_big_endian() {
        let mut buf: &[u8] = &[0x63, 0xdd];
        assert_eq!(read_u16(&mut buf), Ok(25565));

        let mut out = Vec::new();
        write_i64(&mut out, 0x0102030405060708);
        assert_eq!(out, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(read_i64(&mut out.as_slice()), Ok(0x0102030405060708));
    }
}
