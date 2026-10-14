//! WAV 머리. 브라우저가 못 여는 형식(ALAC, DSD …)과 DSP를 거친 소리를 보낼 때 쓴다.
//! 샘플은 `sori_core::dsp::quantize`가 정수로 바꾼다.

/// 44바이트짜리 표준 머리. 3채널 이상이면 WAVE_FORMAT_EXTENSIBLE.
pub fn header(rate: u32, channels: u16, bits: u16, data_len: u64) -> Vec<u8> {
    let block = channels as u32 * (bits as u32 / 8);
    let data_len = data_len.min(u32::MAX as u64 - 80) as u32;
    let ext = channels > 2;
    let fmt_len: u32 = if ext { 40 } else { 16 };
    let mut h = Vec::with_capacity(80);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(4 + 8 + fmt_len + 8 + data_len).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&fmt_len.to_le_bytes());
    h.extend_from_slice(&(if ext { 0xFFFEu16 } else { 1 }).to_le_bytes());
    h.extend_from_slice(&channels.to_le_bytes());
    h.extend_from_slice(&rate.to_le_bytes());
    h.extend_from_slice(&(rate * block).to_le_bytes());
    h.extend_from_slice(&(block as u16).to_le_bytes());
    h.extend_from_slice(&bits.to_le_bytes());
    if ext {
        h.extend_from_slice(&22u16.to_le_bytes());
        h.extend_from_slice(&bits.to_le_bytes());
        let mask: u32 = match channels {
            3 => 0x7,
            4 => 0x33,
            5 => 0x37,
            6 => 0x3F,
            8 => 0x63F,
            _ => 0,
        };
        h.extend_from_slice(&mask.to_le_bytes());
        // KSDATAFORMAT_SUBTYPE_PCM
        h.extend_from_slice(&[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71]);
    }
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data_len.to_le_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_sizes() {
        assert_eq!(header(44_100, 2, 16, 1000).len(), 44);
        assert_eq!(header(48_000, 6, 24, 1000).len(), 68);
    }
}
