//! RSA: 정품 인증용 키 만들기와 PKCS#1 v1.5 복호화, 채팅 서명용 PKCS#1 v1.5 서명 검증,
//! 그리고 X.509 `SubjectPublicKeyInfo` DER 인코딩·해석.

use super::bigint::BigUint;
use super::random;

const SMALL_PRIMES: [u32; 54] = [
    3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97,
    101, 103, 107, 109, 113, 127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179, 181, 191, 193,
    197, 199, 211, 223, 227, 229, 233, 239, 241, 251, 257,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKey {
    pub n: BigUint,
    pub e: BigUint,
}

pub struct PrivateKey {
    pub public: PublicKey,
    p: BigUint,
    q: BigUint,
    dp: BigUint,
    dq: BigUint,
    qinv: BigUint,
}

fn random_odd(bits: usize) -> BigUint {
    let mut bytes = vec![0u8; bits / 8];
    random::fill(&mut bytes);
    bytes[0] |= 0xC0; // 두 소수의 곱이 정확히 2·bits 비트가 되도록 위 두 비트를 켭니다
    *bytes.last_mut().unwrap() |= 1;
    BigUint::from_be_bytes(&bytes)
}

/// 밀러-라빈. 오판 확률 4^-rounds.
fn is_probable_prime(n: &BigUint, rounds: usize) -> bool {
    for &p in &SMALL_PRIMES {
        if n.rem_u32(p) == 0 {
            return *n == BigUint::from_u64(p as u64);
        }
    }
    let one = BigUint::from_u64(1);
    let n1 = n.sub(&one);
    let mut s = 0;
    while !n1.bit(s) {
        s += 1;
    }
    let d = n1.shr(s);
    'witness: for _ in 0..rounds {
        let a = random_odd(n.bits() - 8)
            .rem(&n1)
            .add(&BigUint::from_u64(2))
            .rem(&n1);
        let a = if a.bits() < 2 {
            BigUint::from_u64(2)
        } else {
            a
        };
        let mut x = a.modpow(&d, n);
        if x == one || x == n1 {
            continue;
        }
        for _ in 1..s {
            x = x.mulmod(&x, n);
            if x == n1 {
                continue 'witness;
            }
        }
        return false;
    }
    true
}

fn random_prime(bits: usize, e: &BigUint) -> BigUint {
    loop {
        let candidate = random_odd(bits);
        if is_probable_prime(&candidate, 24)
            && candidate.sub(&BigUint::from_u64(1)).rem(e) != BigUint::zero()
        {
            return candidate;
        }
    }
}

impl PrivateKey {
    /// `bits`비트 키 (바닐라 서버는 1024).
    pub fn generate(bits: usize) -> Self {
        let e = BigUint::from_u64(65537);
        loop {
            let p = random_prime(bits / 2, &e);
            let q = random_prime(bits / 2, &e);
            if p == q {
                continue;
            }
            let one = BigUint::from_u64(1);
            let (p1, q1) = (p.sub(&one), q.sub(&one));
            let phi = p1.mul(&q1);
            let Some(d) = e.modinv(&phi) else { continue };
            let Some(qinv) = q.modinv(&p) else { continue };
            let n = p.mul(&q);
            return Self {
                dp: d.rem(&p1),
                dq: d.rem(&q1),
                qinv,
                public: PublicKey { n, e },
                p,
                q,
            };
        }
    }

    /// c^d mod n (중국인의 나머지 정리로).
    fn raw_decrypt(&self, c: &BigUint) -> BigUint {
        let m1 = c.modpow(&self.dp, &self.p);
        let m2 = c.modpow(&self.dq, &self.q);
        let m2p = m2.rem(&self.p);
        let diff = if m1 >= m2p {
            m1.sub(&m2p)
        } else {
            m1.add(&self.p).sub(&m2p)
        };
        let h = self.qinv.mulmod(&diff, &self.p);
        m2.add(&h.mul(&self.q))
    }

    /// PKCS#1 v1.5 (블록 형식 2) 복호화. 형식이 틀리면 None.
    pub fn decrypt(&self, ciphertext: &[u8]) -> Option<Vec<u8>> {
        let k = self.public.n.bits().div_ceil(8);
        if ciphertext.len() != k {
            return None;
        }
        let c = BigUint::from_be_bytes(ciphertext);
        if c >= self.public.n {
            return None;
        }
        let em = self.raw_decrypt(&c).to_be_bytes(k);
        if em[0] != 0 || em[1] != 2 {
            return None;
        }
        let sep = em[2..].iter().position(|&b| b == 0)? + 2;
        if sep < 10 {
            return None; // 패딩은 최소 8바이트
        }
        Some(em[sep + 1..].to_vec())
    }
}

impl PublicKey {
    fn len(&self) -> usize {
        self.n.bits().div_ceil(8)
    }

    /// PKCS#1 v1.5 (블록 형식 2) 암호화. 서버는 쓰지 않고, 테스트의 가짜 클라이언트가 씁니다.
    pub fn encrypt(&self, message: &[u8]) -> Vec<u8> {
        let k = self.len();
        assert!(message.len() + 11 <= k, "message too long");
        let mut em = vec![0u8, 2];
        while em.len() < k - message.len() - 1 {
            let [b] = random::bytes::<1>();
            if b != 0 {
                em.push(b);
            }
        }
        em.push(0);
        em.extend_from_slice(message);
        BigUint::from_be_bytes(&em)
            .modpow(&self.e, &self.n)
            .to_be_bytes(k)
    }

    /// PKCS#1 v1.5 서명 검증. `digest_info`는 해시 알고리즘의 DER 머리말.
    fn verify(&self, digest_info: &[u8], hash: &[u8], signature: &[u8]) -> bool {
        let k = self.len();
        if signature.len() != k {
            return false;
        }
        let s = BigUint::from_be_bytes(signature);
        if s >= self.n {
            return false;
        }
        let em = s.modpow(&self.e, &self.n).to_be_bytes(k);
        let t_len = digest_info.len() + hash.len();
        if k < t_len + 11 {
            return false;
        }
        let mut expected = vec![0u8, 1];
        expected.resize(k - t_len - 1, 0xFF);
        expected.push(0);
        expected.extend_from_slice(digest_info);
        expected.extend_from_slice(hash);
        em == expected
    }

    pub fn verify_sha256(&self, message: &[u8], signature: &[u8]) -> bool {
        const SHA256_INFO: [u8; 19] = [
            0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x01, 0x05, 0x00, 0x04, 0x20,
        ];
        self.verify(&SHA256_INFO, &super::sha256::sha256(message), signature)
    }

    pub fn verify_sha1(&self, message: &[u8], signature: &[u8]) -> bool {
        const SHA1_INFO: [u8; 15] = [
            0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04,
            0x14,
        ];
        self.verify(&SHA1_INFO, &super::sha1::sha1(message), signature)
    }

    /// X.509 `SubjectPublicKeyInfo` DER (Java `PublicKey.getEncoded()`와 같음).
    pub fn to_der(&self) -> Vec<u8> {
        let rsa_key = der_seq(&[der_int(&self.n), der_int(&self.e)].concat());
        let mut bit_string = vec![0u8]; // 남는 비트 0
        bit_string.extend_from_slice(&rsa_key);
        let alg = der_seq(&[RSA_OID.to_vec(), vec![0x05, 0x00]].concat());
        der_seq(&[alg, der(0x03, &bit_string)].concat())
    }

    pub fn from_der(bytes: &[u8]) -> Option<Self> {
        let mut r = DerReader(bytes);
        let mut spki = DerReader(r.read(0x30)?);
        let mut alg = DerReader(spki.read(0x30)?);
        if alg.read(0x06)? != &RSA_OID[2..] {
            return None;
        }
        let bits = spki.read(0x03)?;
        let mut key = DerReader(DerReader(bits.get(1..)?).read(0x30)?);
        let n = BigUint::from_be_bytes(key.read(0x02)?);
        let e = BigUint::from_be_bytes(key.read(0x02)?);
        Some(Self { n, e })
    }
}

/// OID 1.2.840.113549.1.1.1 (rsaEncryption), 태그·길이 포함.
const RSA_OID: [u8; 11] = [
    0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01,
];

fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = content.len();
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes: Vec<u8> = len
            .to_be_bytes()
            .into_iter()
            .skip_while(|&b| b == 0)
            .collect();
        out.push(0x80 | bytes.len() as u8);
        out.extend_from_slice(&bytes);
    }
    out.extend_from_slice(content);
    out
}

fn der_seq(content: &[u8]) -> Vec<u8> {
    der(0x30, content)
}

fn der_int(v: &BigUint) -> Vec<u8> {
    let mut bytes = v.to_be_bytes_minimal();
    if bytes[0] & 0x80 != 0 {
        bytes.insert(0, 0); // 양수 표시
    }
    der(0x02, &bytes)
}

struct DerReader<'a>(&'a [u8]);

impl<'a> DerReader<'a> {
    fn read(&mut self, tag: u8) -> Option<&'a [u8]> {
        let (&t, rest) = self.0.split_first()?;
        if t != tag {
            return None;
        }
        let (&l, mut rest) = rest.split_first()?;
        let len = if l < 0x80 {
            l as usize
        } else {
            let n = (l & 0x7f) as usize;
            if n > 4 || rest.len() < n {
                return None;
            }
            let len = rest[..n].iter().fold(0usize, |a, &b| a << 8 | b as usize);
            rest = &rest[n..];
            len
        };
        if rest.len() < len {
            return None;
        }
        let (content, rest) = rest.split_at(len);
        self.0 = rest;
        Some(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_encrypt_decrypt() {
        let key = PrivateKey::generate(1024);
        assert_eq!(key.public.n.bits(), 1024);
        let secret = random::bytes::<16>();
        let c = key.public.encrypt(&secret);
        assert_eq!(c.len(), 128);
        assert_eq!(key.decrypt(&c).as_deref(), Some(&secret[..]));
        // 깨진 암호문은 거부
        let mut bad = c.clone();
        bad[5] ^= 1;
        assert_ne!(key.decrypt(&bad).as_deref(), Some(&secret[..]));
    }

    #[test]
    fn der_round_trip() {
        let key = PrivateKey::generate(512);
        let der = key.public.to_der();
        assert_eq!(der[0], 0x30);
        assert_eq!(PublicKey::from_der(&der), Some(key.public.clone()));
    }

    #[test]
    fn verifies_a_known_sha256_signature() {
        // 작은 고정 키로 서명을 만들고 (s = m^d), 검증과 변조 거부를 확인합니다.
        let key = PrivateKey::generate(1024);
        let msg = b"chat body";
        let hash = super::super::sha256::sha256(msg);
        let k = 128;
        let mut em = vec![0u8, 1];
        let info = [
            0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x01, 0x05, 0x00, 0x04, 0x20,
        ];
        em.resize(k - info.len() - hash.len() - 1, 0xFF);
        em.push(0);
        em.extend_from_slice(&info);
        em.extend_from_slice(&hash);
        let sig = key.raw_decrypt(&BigUint::from_be_bytes(&em)).to_be_bytes(k);
        assert!(key.public.verify_sha256(msg, &sig));
        assert!(!key.public.verify_sha256(b"chat bodY", &sig));
    }
}
