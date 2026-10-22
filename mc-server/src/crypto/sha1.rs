//! SHA-1 (FIPS 180-4). 정품 인증의 서버 해시에만 씁니다.

pub struct Sha1 {
    h: [u32; 5],
    buf: Vec<u8>,
    len: u64,
}

impl Default for Sha1 {
    fn default() -> Self {
        Self {
            h: [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0],
            buf: Vec::new(),
            len: 0,
        }
    }
}

impl Sha1 {
    pub fn update(&mut self, data: &[u8]) {
        self.len += data.len() as u64;
        self.buf.extend_from_slice(data);
        let full = self.buf.len() / 64 * 64;
        for i in (0..full).step_by(64) {
            let block: [u8; 64] = self.buf[i..i + 64].try_into().unwrap();
            self.compress(&block);
        }
        self.buf.drain(..full);
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = self.h;
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (h, v) in self.h.iter_mut().zip([a, b, c, d, e]) {
            *h = h.wrapping_add(v);
        }
    }

    pub fn finish(mut self) -> [u8; 20] {
        let bit_len = self.len * 8;
        let mut tail = vec![0x80u8];
        while (self.buf.len() + tail.len()) % 64 != 56 {
            tail.push(0);
        }
        tail.extend_from_slice(&bit_len.to_be_bytes());
        let (len, buf) = (self.len, std::mem::take(&mut self.buf));
        self.update(&[buf, tail].concat());
        self.len = len;
        let mut out = [0u8; 20];
        for (i, h) in self.h.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&h.to_be_bytes());
        }
        out
    }
}

pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut s = Sha1::default();
    s.update(data);
    s.finish()
}

/// 마인크래프트식 16진 다이제스트: 해시를 부호 있는 큰 정수로 보고 16진수로 (음수면 `-`,
/// 앞자리 0은 없음). Java `new BigInteger(hash).toString(16)`과 같습니다.
pub fn minecraft_hex_digest(hash: &[u8; 20]) -> String {
    let negative = hash[0] & 0x80 != 0;
    let mut bytes = *hash;
    if negative {
        // 2의 보수의 절댓값: 반전 후 1 더하기
        let mut carry = true;
        for b in bytes.iter_mut().rev() {
            *b = !*b;
            if carry {
                let (v, c) = b.overflowing_add(1);
                *b = v;
                carry = c;
            }
        }
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let trimmed = hex.trim_start_matches('0');
    let digits = if trimmed.is_empty() { "0" } else { trimmed };
    if negative {
        format!("-{digits}")
    } else {
        digits.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn known_vectors() {
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(hex(&sha1(long)), "84983e441c3bd26ebaae4aa1f95129e5e54670f1");
        let mut s = Sha1::default();
        for _ in 0..1000 {
            s.update(&[b'a'; 1000]);
        }
        assert_eq!(hex(&s.finish()), "34aa973cd4c4daa4f61eeb2bdbad27316534016f");
    }

    #[test]
    fn minecraft_digests() {
        // wiki.vg "Protocol Encryption"의 예시
        assert_eq!(
            minecraft_hex_digest(&sha1(b"Notch")),
            "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48"
        );
        assert_eq!(
            minecraft_hex_digest(&sha1(b"jeb_")),
            "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1"
        );
        assert_eq!(
            minecraft_hex_digest(&sha1(b"simon")),
            "88e16a1019277b15d58faf0541e11910eb756f6"
        );
    }
}
