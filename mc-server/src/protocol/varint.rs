//! VarInt / VarLong.
//!
//! 한 바이트에 7비트씩, 낮은 자리부터 담고 최상위 비트(0x80)를 "뒤에 더 있음" 표시로
//! 씁니다. 음수는 지그재그 없이 2의 보수 그대로 — 그래서 `-1`은 VarInt로 5바이트
//! (`ff ff ff ff 0f`), VarLong으로 10바이트를 차지합니다.

use super::DecodeError;

const SEGMENT_BITS: u8 = 0x7F;
const CONTINUE_BIT: u8 = 0x80;

/// VarInt가 차지할 수 있는 최대 바이트 수 (32비트 / 7비트, 올림).
pub const VARINT_MAX_LEN: usize = 5;
/// VarLong이 차지할 수 있는 최대 바이트 수 (64비트 / 7비트, 올림).
pub const VARLONG_MAX_LEN: usize = 10;

/// 버퍼 앞에서 VarInt를 하나 읽고, 읽은 만큼 `buf`를 앞으로 당깁니다.
pub fn read_varint(buf: &mut &[u8]) -> Result<i32, DecodeError> {
    let mut value: u32 = 0;
    for i in 0..VARINT_MAX_LEN {
        let (&byte, rest) = buf.split_first().ok_or(DecodeError::UnexpectedEof)?;
        *buf = rest;
        value |= ((byte & SEGMENT_BITS) as u32) << (7 * i);
        if byte & CONTINUE_BIT == 0 {
            return Ok(value as i32);
        }
    }
    Err(DecodeError::VarIntTooLong)
}

/// 버퍼 앞에서 VarLong을 하나 읽고, 읽은 만큼 `buf`를 앞으로 당깁니다.
pub fn read_varlong(buf: &mut &[u8]) -> Result<i64, DecodeError> {
    let mut value: u64 = 0;
    for i in 0..VARLONG_MAX_LEN {
        let (&byte, rest) = buf.split_first().ok_or(DecodeError::UnexpectedEof)?;
        *buf = rest;
        value |= ((byte & SEGMENT_BITS) as u64) << (7 * i);
        if byte & CONTINUE_BIT == 0 {
            return Ok(value as i64);
        }
    }
    Err(DecodeError::VarLongTooLong)
}

/// VarInt를 `out` 뒤에 붙입니다.
pub fn write_varint(out: &mut Vec<u8>, value: i32) {
    // 부호 비트가 산술 시프트로 번지지 않도록 부호 없는 값으로 다룹니다.
    let mut v = value as u32;
    loop {
        if v & !(SEGMENT_BITS as u32) == 0 {
            out.push(v as u8);
            return;
        }
        out.push((v as u8 & SEGMENT_BITS) | CONTINUE_BIT);
        v >>= 7;
    }
}

/// VarLong을 `out` 뒤에 붙입니다.
pub fn write_varlong(out: &mut Vec<u8>, value: i64) {
    let mut v = value as u64;
    loop {
        if v & !(SEGMENT_BITS as u64) == 0 {
            out.push(v as u8);
            return;
        }
        out.push((v as u8 & SEGMENT_BITS) | CONTINUE_BIT);
        v >>= 7;
    }
}

/// `value`를 VarInt로 썼을 때의 바이트 수. 패킷 길이 접두사를 계산할 때 씁니다.
pub fn varint_len(value: i32) -> usize {
    let v = value as u32;
    match v {
        0..=0x7F => 1,
        0x80..=0x3FFF => 2,
        0x4000..=0x1F_FFFF => 3,
        0x20_0000..=0xFFF_FFFF => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // https://minecraft.wiki/w/Java_Edition_protocol/Data_types 의 예시 표.
    const VARINT_CASES: &[(i32, &[u8])] = &[
        (0, &[0x00]),
        (1, &[0x01]),
        (2, &[0x02]),
        (127, &[0x7f]),
        (128, &[0x80, 0x01]),
        (255, &[0xff, 0x01]),
        (25565, &[0xdd, 0xc7, 0x01]),
        (2097151, &[0xff, 0xff, 0x7f]),
        (2147483647, &[0xff, 0xff, 0xff, 0xff, 0x07]),
        (-1, &[0xff, 0xff, 0xff, 0xff, 0x0f]),
        (-2147483648, &[0x80, 0x80, 0x80, 0x80, 0x08]),
    ];

    const VARLONG_CASES: &[(i64, &[u8])] = &[
        (0, &[0x00]),
        (1, &[0x01]),
        (127, &[0x7f]),
        (128, &[0x80, 0x01]),
        (255, &[0xff, 0x01]),
        (2147483647, &[0xff, 0xff, 0xff, 0xff, 0x07]),
        (
            9223372036854775807,
            &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f],
        ),
        (
            -1,
            &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01],
        ),
        (
            -2147483648,
            &[0x80, 0x80, 0x80, 0x80, 0xf8, 0xff, 0xff, 0xff, 0xff, 0x01],
        ),
        (
            -9223372036854775808,
            &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01],
        ),
    ];

    #[test]
    fn varint_round_trip() {
        for &(value, bytes) in VARINT_CASES {
            let mut out = Vec::new();
            write_varint(&mut out, value);
            assert_eq!(out, bytes, "write {value}");
            assert_eq!(varint_len(value), bytes.len(), "len {value}");

            let mut buf = bytes;
            assert_eq!(read_varint(&mut buf), Ok(value), "read {value}");
            assert!(buf.is_empty());
        }
    }

    #[test]
    fn varlong_round_trip() {
        for &(value, bytes) in VARLONG_CASES {
            let mut out = Vec::new();
            write_varlong(&mut out, value);
            assert_eq!(out, bytes, "write {value}");

            let mut buf = bytes;
            assert_eq!(read_varlong(&mut buf), Ok(value), "read {value}");
            assert!(buf.is_empty());
        }
    }

    #[test]
    fn read_leaves_the_rest() {
        let mut buf: &[u8] = &[0xdd, 0xc7, 0x01, 0xAA, 0xBB];
        assert_eq!(read_varint(&mut buf), Ok(25565));
        assert_eq!(buf, &[0xAA, 0xBB]);
    }

    #[test]
    fn rejects_truncated_and_oversized() {
        let mut buf: &[u8] = &[0x80, 0x80];
        assert_eq!(read_varint(&mut buf), Err(DecodeError::UnexpectedEof));

        let mut buf: &[u8] = &[0xff; 6];
        assert_eq!(read_varint(&mut buf), Err(DecodeError::VarIntTooLong));

        let mut buf: &[u8] = &[0xff; 11];
        assert_eq!(read_varlong(&mut buf), Err(DecodeError::VarLongTooLong));
    }
}
