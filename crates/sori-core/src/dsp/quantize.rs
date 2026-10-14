//! 64비트 → 정수 PCM. 이미 그 비트 수의 격자 위에 있는 샘플(16/24비트 원본을 손대지 않은 것)은
//! 디더 없이 그대로 옮기고, 그 밖은 삼각 분포(TPDF) 디더를 넣어 반올림한다.

pub struct Quantizer {
    bits: u32,
    dither: bool,
    state: u64,
}

impl Quantizer {
    pub fn new(bits: u32, dither: bool) -> Self {
        Quantizer { bits, dither, state: 0x9E37_79B9_7F4A_7C15 }
    }

    pub fn bits(&self) -> u32 {
        self.bits
    }

    #[inline]
    fn uniform(&mut self) -> f64 {
        // xorshift64*: 디더에는 이 정도면 충분하다.
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        (self.state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    /// 정수로 바꿔 `f`에 하나씩 넘긴다. 범위를 넘는 값은 잘린다(클리핑).
    #[inline]
    pub fn each(&mut self, samples: &[f64], mut f: impl FnMut(i32)) {
        let scale = (1i64 << (self.bits - 1)) as f64;
        let max = scale - 1.0;
        let exact = !self.dither
            || samples.iter().all(|&s| {
                let v = s * scale;
                v == v.trunc()
            });
        for &s in samples {
            let mut v = s * scale;
            if !exact {
                v += self.uniform() - self.uniform();
            }
            f(v.round().clamp(-scale, max) as i32);
        }
    }

    /// 리틀엔디언 바이트로(16/24/32비트).
    pub fn push_le(&mut self, samples: &[f64], out: &mut Vec<u8>) {
        let bytes = (self.bits / 8) as usize;
        out.reserve(samples.len() * bytes);
        self.each(samples, |q| out.extend_from_slice(&q.to_le_bytes()[..bytes]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_values_pass_through() {
        let mut q = Quantizer::new(24, true);
        let mut out = Vec::new();
        q.push_le(&[0.5, -1.0, 1.0 / 8_388_608.0], &mut out);
        assert_eq!(&out[0..3], &[0x00, 0x00, 0x40]);
        assert_eq!(&out[3..6], &[0x00, 0x00, 0x80]);
        assert_eq!(&out[6..9], &[0x01, 0x00, 0x00]);
    }

    #[test]
    fn dither_stays_within_one_lsb() {
        let mut q = Quantizer::new(16, true);
        let mut v = Vec::new();
        q.each(&[0.3; 256], |x| v.push(x));
        let ideal = 0.3 * 32_768.0;
        assert!(v.iter().all(|&x| (x as f64 - ideal).abs() <= 1.5));
        assert!(v.iter().any(|&x| x != v[0]));
        // 1을 넘는 값은 잘린다
        let mut w = Vec::new();
        Quantizer::new(16, false).each(&[1.5, -1.5], |x| w.push(x));
        assert_eq!(w, vec![32_767, -32_768]);
    }
}
