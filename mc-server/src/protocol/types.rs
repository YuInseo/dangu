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

pub fn read_u8(buf: &mut &[u8]) -> Result<u8, DecodeError> {
    Ok(take(buf, 1)?[0])
}

pub fn read_bool(buf: &mut &[u8]) -> Result<bool, DecodeError> {
    match read_u8(buf)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(DecodeError::InvalidValue("boolean")),
    }
}

pub fn write_bool(out: &mut Vec<u8>, v: bool) {
    out.push(v as u8);
}

pub fn write_i32(out: &mut Vec<u8>, v: i32) {
    out.extend_from_slice(&v.to_be_bytes());
}

pub fn read_f32(buf: &mut &[u8]) -> Result<f32, DecodeError> {
    let b = take(buf, 4)?;
    Ok(f32::from_be_bytes(b.try_into().unwrap()))
}

pub fn write_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_be_bytes());
}

pub fn read_f64(buf: &mut &[u8]) -> Result<f64, DecodeError> {
    let b = take(buf, 8)?;
    Ok(f64::from_be_bytes(b.try_into().unwrap()))
}

pub fn write_f64(out: &mut Vec<u8>, v: f64) {
    out.extend_from_slice(&v.to_be_bytes());
}

/// UUID: 상위 64비트, 하위 64비트 순서의 128비트 빅엔디언.
pub fn read_uuid(buf: &mut &[u8]) -> Result<u128, DecodeError> {
    let b = take(buf, 16)?;
    Ok(u128::from_be_bytes(b.try_into().unwrap()))
}

pub fn write_uuid(out: &mut Vec<u8>, v: u128) {
    out.extend_from_slice(&v.to_be_bytes());
}

/// `Prefixed Array of Long`: VarInt 개수 + i64들.
pub fn write_long_array(out: &mut Vec<u8>, values: &[i64]) {
    write_varint(out, values.len() as i32);
    for &v in values {
        write_i64(out, v);
    }
}

/// `Prefixed Array of Byte`: VarInt 길이 + 바이트.
pub fn write_byte_array(out: &mut Vec<u8>, bytes: &[u8]) {
    write_varint(out, bytes.len() as i32);
    out.extend_from_slice(bytes);
}

/// 26.3의 `BitSet`: Java `BitSet.toByteArray()`와 같이 리틀엔디언 바이트, 뒤쪽 0은 잘라
/// 내고, VarInt 길이를 앞에 붙입니다. (26.2까지는 long 배열이었습니다.)
pub fn write_bitset(out: &mut Vec<u8>, bits: u64) {
    let bytes = bits.to_le_bytes();
    let len = 8 - (bits.leading_zeros() / 8) as usize;
    write_byte_array(out, &bytes[..len]);
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
    fn bitset_matches_java_to_byte_array() {
        let mut out = Vec::new();
        write_bitset(&mut out, 0);
        assert_eq!(out, [0]);

        let mut out = Vec::new();
        write_bitset(&mut out, 0b1_0000_0001);
        assert_eq!(out, [2, 0x01, 0x01]);
    }

    #[test]
    fn uuid_is_big_endian() {
        let mut out = Vec::new();
        write_uuid(&mut out, 0x0011_2233_4455_6677_8899_aabb_ccdd_eeff);
        assert_eq!(out[0], 0x00);
        assert_eq!(out[15], 0xff);
        assert_eq!(
            read_uuid(&mut out.as_slice()),
            Ok(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff)
        );
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
