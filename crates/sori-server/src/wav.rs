//! 64비트 PCM → WAV 바이트. 브라우저가 못 여는 형식(ALAC, DSD, 32비트 …)을 보낼 때 쓴다.
//!
//! 원본이 정수 PCM이고 비트 수가 출력보다 작거나 같으면 디더 없이 그대로 옮긴다(비트퍼펙트).
//! 그 밖(DSD를 푼 것, DSP를 거친 것)은 삼각 분포(TPDF) 디더를 넣어 반올림한다.

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

pub struct Quantizer {
    bits: u16,
    dither: bool,
    state: u64,
}

impl Quantizer {
    pub fn new(bits: u16, dither: bool) -> Self {
        Quantizer { bits, dither, state: 0x9E37_79B9_7F4A_7C15 }
    }

    fn uniform(&mut self) -> f64 {
        // xorshift64*: 디더에는 이 정도면 충분하다.
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        (self.state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn push(&mut self, samples: &[f64], out: &mut Vec<u8>) {
        let scale = (1i64 << (self.bits - 1)) as f64;
        let max = scale - 1.0;
        let bytes = (self.bits / 8) as usize;
        out.reserve(samples.len() * bytes);
        // 이미 이 비트 수의 정수 격자 위에 있는 샘플(16/24비트 원본)에 디더를 넣으면 원본이 망가진다.
        let exact = samples.iter().all(|&s| {
            let v = s * scale;
            v == v.trunc()
        });
        let dither = self.dither && !exact;
        for &s in samples {
            let mut v = s * scale;
            if dither {
                v += self.uniform() - self.uniform();
            }
            let q = v.round().clamp(-scale, max) as i64 as i32;
            out.extend_from_slice(&q.to_le_bytes()[..bytes]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_without_dither() {
        let mut q = Quantizer::new(24, false);
        let mut out = Vec::new();
        q.push(&[0.5, -1.0, 1.0 / 8_388_608.0], &mut out);
        assert_eq!(&out[0..3], &[0x00, 0x00, 0x40]);
        assert_eq!(&out[3..6], &[0x00, 0x00, 0x80]);
        assert_eq!(&out[6..9], &[0x01, 0x00, 0x00]);
    }

    #[test]
    fn dither_skips_exact_integers() {
        let mut q = Quantizer::new(24, true);
        let mut out = Vec::new();
        q.push(&[0.5, 3.0 / 8_388_608.0], &mut out);
        assert_eq!(&out[..], &[0x00, 0x00, 0x40, 0x03, 0x00, 0x00]);
        // 격자 밖의 값에는 디더가 들어간다(1LSB 안에서).
        out.clear();
        q.push(&[0.3; 64], &mut out);
        let vals: Vec<i32> =
            out.chunks(3).map(|b| i32::from_le_bytes([b[0], b[1], b[2], if b[2] & 0x80 != 0 { 0xFF } else { 0 }])).collect();
        let ideal = 0.3 * 8_388_608.0;
        assert!(vals.iter().all(|&v| (v as f64 - ideal).abs() <= 1.5));
        assert!(vals.iter().any(|&v| v != vals[0]));
    }

    #[test]
    fn header_sizes() {
        assert_eq!(header(44_100, 2, 16, 1000).len(), 44);
        assert_eq!(header(48_000, 6, 24, 1000).len(), 68);
    }
}
