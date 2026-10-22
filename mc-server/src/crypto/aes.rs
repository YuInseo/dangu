//! AES-128 암호화(FIPS-197)와 CFB8 모드. 마인크래프트 연결 암호화는 공유 비밀을 키와 IV로
//! 함께 쓰는 AES-128-CFB8입니다. CFB는 양방향 모두 블록 암호화만 쓰므로 복호화 라운드는
//! 없습니다.

const SBOX: [u8; 256] = {
    // GF(2^8)의 역원 + 아핀 변환으로 컴파일 시간에 만듭니다.
    let mut sbox = [0u8; 256];
    let mut p: u8 = 1;
    let mut q: u8 = 1;
    loop {
        // p ← p·3, q ← q/3
        p = p ^ (p << 1) ^ if p & 0x80 != 0 { 0x1B } else { 0 };
        q ^= q << 1;
        q ^= q << 2;
        q ^= q << 4;
        if q & 0x80 != 0 {
            q ^= 0x09;
        }
        let x = q ^ q.rotate_left(1) ^ q.rotate_left(2) ^ q.rotate_left(3) ^ q.rotate_left(4);
        sbox[p as usize] = x ^ 0x63;
        if p == 1 {
            break;
        }
    }
    sbox[0] = 0x63;
    sbox
};

fn xtime(x: u8) -> u8 {
    (x << 1) ^ if x & 0x80 != 0 { 0x1B } else { 0 }
}

pub struct Aes128 {
    round_keys: [[u8; 16]; 11],
}

impl Aes128 {
    pub fn new(key: &[u8; 16]) -> Self {
        let mut w = [[0u8; 4]; 44];
        for i in 0..4 {
            w[i].copy_from_slice(&key[i * 4..i * 4 + 4]);
        }
        let mut rcon = 1u8;
        for i in 4..44 {
            let mut t = w[i - 1];
            if i % 4 == 0 {
                t = [
                    SBOX[t[1] as usize] ^ rcon,
                    SBOX[t[2] as usize],
                    SBOX[t[3] as usize],
                    SBOX[t[0] as usize],
                ];
                rcon = xtime(rcon);
            }
            for j in 0..4 {
                w[i][j] = w[i - 4][j] ^ t[j];
            }
        }
        let mut round_keys = [[0u8; 16]; 11];
        for (r, rk) in round_keys.iter_mut().enumerate() {
            for c in 0..4 {
                rk[c * 4..c * 4 + 4].copy_from_slice(&w[r * 4 + c]);
            }
        }
        Self { round_keys }
    }

    pub fn encrypt_block(&self, block: &mut [u8; 16]) {
        xor(block, &self.round_keys[0]);
        for round in 1..=10 {
            for b in block.iter_mut() {
                *b = SBOX[*b as usize];
            }
            // ShiftRows (열 우선 배치: 인덱스 = 열*4 + 행)
            let s = *block;
            for c in 0..4 {
                for r in 0..4 {
                    block[c * 4 + r] = s[((c + r) % 4) * 4 + r];
                }
            }
            if round != 10 {
                for c in 0..4 {
                    let col = [
                        block[c * 4],
                        block[c * 4 + 1],
                        block[c * 4 + 2],
                        block[c * 4 + 3],
                    ];
                    let all = col[0] ^ col[1] ^ col[2] ^ col[3];
                    for r in 0..4 {
                        block[c * 4 + r] = col[r] ^ all ^ xtime(col[r] ^ col[(r + 1) % 4]);
                    }
                }
            }
            xor(block, &self.round_keys[round]);
        }
    }
}

fn xor(a: &mut [u8; 16], b: &[u8; 16]) {
    for (x, y) in a.iter_mut().zip(b) {
        *x ^= y;
    }
}

/// AES-128-CFB8 한 방향. 보내는 쪽과 받는 쪽은 각자 상태를 따로 가집니다.
pub struct Cfb8 {
    aes: Aes128,
    iv: [u8; 16],
}

impl Cfb8 {
    /// 마인크래프트는 공유 비밀을 키와 IV로 함께 씁니다.
    pub fn new(shared_secret: &[u8; 16]) -> Self {
        Self {
            aes: Aes128::new(shared_secret),
            iv: *shared_secret,
        }
    }

    fn next_mask(&self) -> u8 {
        let mut b = self.iv;
        self.aes.encrypt_block(&mut b);
        b[0]
    }

    fn push(&mut self, cipher_byte: u8) {
        self.iv.copy_within(1.., 0);
        self.iv[15] = cipher_byte;
    }

    pub fn encrypt(&mut self, data: &mut [u8]) {
        for b in data {
            *b ^= self.next_mask();
            self.push(*b);
        }
    }

    pub fn decrypt(&mut self, data: &mut [u8]) {
        for b in data {
            let c = *b;
            *b ^= self.next_mask();
            self.push(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn sbox_spot_checks() {
        assert_eq!(SBOX[0x00], 0x63);
        assert_eq!(SBOX[0x01], 0x7c);
        assert_eq!(SBOX[0x53], 0xed);
        assert_eq!(SBOX[0xff], 0x16);
    }

    #[test]
    fn fips197_vector() {
        let key: [u8; 16] = unhex("000102030405060708090a0b0c0d0e0f")
            .try_into()
            .unwrap();
        let mut block: [u8; 16] = unhex("00112233445566778899aabbccddeeff")
            .try_into()
            .unwrap();
        Aes128::new(&key).encrypt_block(&mut block);
        assert_eq!(block.to_vec(), unhex("69c4e0d86a7b0430d8cdb78070b4c55a"));
    }

    #[test]
    fn nist_cfb8_vector() {
        // NIST SP 800-38A F.3.7 CFB8-AES128.Encrypt (키와 IV가 달라 Cfb8::new 대신 직접 구성)
        let key: [u8; 16] = unhex("2b7e151628aed2a6abf7158809cf4f3c")
            .try_into()
            .unwrap();
        let iv: [u8; 16] = unhex("000102030405060708090a0b0c0d0e0f")
            .try_into()
            .unwrap();
        let mut enc = Cfb8 {
            aes: Aes128::new(&key),
            iv,
        };
        let mut data = unhex("6bc1bee22e409f96e93d7e117393172aae2d");
        enc.encrypt(&mut data);
        assert_eq!(data, unhex("3b79424c9c0dd436bace9e0ed4586a4f32b9"));

        let mut dec = Cfb8 {
            aes: Aes128::new(&key),
            iv,
        };
        dec.decrypt(&mut data);
        assert_eq!(data, unhex("6bc1bee22e409f96e93d7e117393172aae2d"));
    }
}
