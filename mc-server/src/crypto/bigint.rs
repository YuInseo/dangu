//! RSA에 필요한 만큼의 부호 없는 큰 정수. 32비트 조각을 낮은 자리부터 담습니다.
//! 모듈러 거듭제곱은 몽고메리 곱셈으로 나눗셈 없이 합니다.

use std::cmp::Ordering;

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct BigUint {
    limbs: Vec<u32>,
}

impl BigUint {
    fn normalize(mut self) -> Self {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
        self
    }

    pub fn zero() -> Self {
        Self::default()
    }

    pub fn from_u64(v: u64) -> Self {
        Self {
            limbs: vec![v as u32, (v >> 32) as u32],
        }
        .normalize()
    }

    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        let mut limbs = Vec::with_capacity(bytes.len().div_ceil(4));
        for chunk in bytes.rchunks(4) {
            let mut b = [0u8; 4];
            b[4 - chunk.len()..].copy_from_slice(chunk);
            limbs.push(u32::from_be_bytes(b));
        }
        Self { limbs }.normalize()
    }

    /// 앞에 0을 채워 정확히 `len`바이트로. 값이 더 크면 패닉.
    pub fn to_be_bytes(&self, len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        for (i, limb) in self.limbs.iter().enumerate() {
            for (j, b) in limb.to_le_bytes().iter().enumerate() {
                let pos = i * 4 + j;
                if pos < len {
                    out[len - 1 - pos] = *b;
                } else {
                    assert_eq!(*b, 0, "value does not fit in {len} bytes");
                }
            }
        }
        out
    }

    /// 앞자리 0 없는 빅엔디언 바이트.
    pub fn to_be_bytes_minimal(&self) -> Vec<u8> {
        let len = self.bits().div_ceil(8).max(1);
        self.to_be_bytes(len)
    }

    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    pub fn is_odd(&self) -> bool {
        self.limbs.first().is_some_and(|l| l & 1 == 1)
    }

    pub fn bits(&self) -> usize {
        match self.limbs.last() {
            None => 0,
            Some(&top) => self.limbs.len() * 32 - top.leading_zeros() as usize,
        }
    }

    pub fn bit(&self, i: usize) -> bool {
        self.limbs
            .get(i / 32)
            .is_some_and(|l| (l >> (i % 32)) & 1 == 1)
    }

    pub fn add(&self, other: &Self) -> Self {
        let n = self.limbs.len().max(other.limbs.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u64;
        for i in 0..n {
            let s = *self.limbs.get(i).unwrap_or(&0) as u64
                + *other.limbs.get(i).unwrap_or(&0) as u64
                + carry;
            out.push(s as u32);
            carry = s >> 32;
        }
        out.push(carry as u32);
        Self { limbs: out }.normalize()
    }

    /// `self - other`. `self >= other`이어야 합니다.
    pub fn sub(&self, other: &Self) -> Self {
        assert!(*self >= *other, "BigUint underflow");
        let mut out = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i64;
        for i in 0..self.limbs.len() {
            let mut d = self.limbs[i] as i64 - *other.limbs.get(i).unwrap_or(&0) as i64 - borrow;
            borrow = 0;
            if d < 0 {
                d += 1 << 32;
                borrow = 1;
            }
            out.push(d as u32);
        }
        Self { limbs: out }.normalize()
    }

    pub fn mul(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        let mut out = vec![0u32; self.limbs.len() + other.limbs.len()];
        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in other.limbs.iter().enumerate() {
                let t = a as u64 * b as u64 + out[i + j] as u64 + carry;
                out[i + j] = t as u32;
                carry = t >> 32;
            }
            out[i + other.limbs.len()] = carry as u32;
        }
        Self { limbs: out }.normalize()
    }

    fn shl1(&mut self) {
        let mut carry = 0;
        for l in self.limbs.iter_mut() {
            let next = *l >> 31;
            *l = (*l << 1) | carry;
            carry = next;
        }
        if carry != 0 {
            self.limbs.push(carry);
        }
    }

    pub fn shr(&self, bits: usize) -> Self {
        let (words, rest) = (bits / 32, bits % 32);
        if words >= self.limbs.len() {
            return Self::zero();
        }
        let src = &self.limbs[words..];
        let mut out = Vec::with_capacity(src.len());
        for i in 0..src.len() {
            let lo = src[i] >> rest;
            let hi = if rest == 0 {
                0
            } else {
                src.get(i + 1).map_or(0, |h| h << (32 - rest))
            };
            out.push(lo | hi);
        }
        Self { limbs: out }.normalize()
    }

    /// (몫, 나머지). 한 비트씩 하는 긴 나눗셈 — 키 만들 때 몇 번만 쓰므로 충분합니다.
    pub fn divrem(&self, d: &Self) -> (Self, Self) {
        assert!(!d.is_zero(), "division by zero");
        if self < d {
            return (Self::zero(), self.clone());
        }
        let mut q = vec![0u32; self.limbs.len()];
        let mut r = Self::zero();
        for i in (0..self.bits()).rev() {
            r.shl1();
            if self.bit(i) {
                if r.limbs.is_empty() {
                    r.limbs.push(1);
                } else {
                    r.limbs[0] |= 1;
                }
            }
            if r >= *d {
                r = r.sub(d);
                q[i / 32] |= 1 << (i % 32);
            }
        }
        (Self { limbs: q }.normalize(), r)
    }

    pub fn rem(&self, d: &Self) -> Self {
        self.divrem(d).1
    }

    pub fn rem_u32(&self, d: u32) -> u32 {
        let mut r = 0u64;
        for &l in self.limbs.iter().rev() {
            r = ((r << 32) | l as u64) % d as u64;
        }
        r as u32
    }

    /// (self · other) mod m
    pub fn mulmod(&self, other: &Self, m: &Self) -> Self {
        self.mul(other).rem(m)
    }

    /// self⁻¹ mod m (확장 유클리드). 서로소가 아니면 None.
    pub fn modinv(&self, m: &Self) -> Option<Self> {
        // (값, 음수인가)로 부호를 따로 들고 다닙니다.
        let (mut old_r, mut r) = (self.rem(m), m.clone());
        let (mut old_s, mut s) = ((Self::from_u64(1), false), (Self::zero(), false));
        while !r.is_zero() {
            let (q, rem) = old_r.divrem(&r);
            old_r = std::mem::replace(&mut r, rem);
            let qs = (q.mul(&s.0), s.1);
            let next = signed_sub(&old_s, &qs);
            old_s = std::mem::replace(&mut s, next);
        }
        if old_r != Self::from_u64(1) {
            return None;
        }
        let v = old_s.0.rem(m);
        Some(if old_s.1 && !v.is_zero() {
            m.sub(&v)
        } else {
            v
        })
    }

    /// self^exp mod m (m은 홀수).
    pub fn modpow(&self, exp: &Self, m: &Self) -> Self {
        Montgomery::new(m).pow(&self.rem(m), exp)
    }
}

fn signed_sub(a: &(BigUint, bool), b: &(BigUint, bool)) -> (BigUint, bool) {
    // a - b = a + (-b)
    let neg_b = (b.0.clone(), !b.1);
    if a.1 == neg_b.1 {
        (a.0.add(&neg_b.0), a.1)
    } else if a.0 >= neg_b.0 {
        (a.0.sub(&neg_b.0), a.1)
    } else {
        (neg_b.0.sub(&a.0), neg_b.1)
    }
}

impl PartialOrd for BigUint {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BigUint {
    fn cmp(&self, other: &Self) -> Ordering {
        self.limbs.len().cmp(&other.limbs.len()).then_with(|| {
            for (a, b) in self.limbs.iter().rev().zip(other.limbs.iter().rev()) {
                match a.cmp(b) {
                    Ordering::Equal => continue,
                    o => return o,
                }
            }
            Ordering::Equal
        })
    }
}

/// 홀수 모듈러 n에 대한 몽고메리 연산 (R = 2^(32k)).
struct Montgomery {
    n: Vec<u32>,
    /// -n⁻¹ mod 2³²
    n0inv: u32,
    /// R² mod n
    r2: BigUint,
}

impl Montgomery {
    fn new(n: &BigUint) -> Self {
        assert!(n.is_odd(), "Montgomery modulus must be odd");
        let n0 = n.limbs[0];
        // 뉴턴 반복으로 n0⁻¹ mod 2³²
        let mut inv: u32 = 1;
        for _ in 0..5 {
            inv = inv.wrapping_mul(2u32.wrapping_sub(n0.wrapping_mul(inv)));
        }
        let k = n.limbs.len();
        let mut r2_limbs = vec![0u32; 2 * k];
        r2_limbs.push(1);
        let r2 = BigUint { limbs: r2_limbs }.rem(n);
        Self {
            n: n.limbs.clone(),
            n0inv: inv.wrapping_neg(),
            r2,
        }
    }

    /// a·b·R⁻¹ mod n (CIOS). a, b < n, 길이 k로 맞춘 조각 배열.
    /// 여러 배열을 같은 번호로 함께 다루므로 인덱스 반복이 더 읽기 쉽습니다.
    #[allow(clippy::needless_range_loop)]
    fn mul(&self, a: &[u32], b: &[u32]) -> Vec<u32> {
        let k = self.n.len();
        let mut t = vec![0u32; k + 2];
        for i in 0..k {
            let mut carry = 0u64;
            for j in 0..k {
                let s = t[j] as u64 + a[j] as u64 * b[i] as u64 + carry;
                t[j] = s as u32;
                carry = s >> 32;
            }
            let s = t[k] as u64 + carry;
            t[k] = s as u32;
            t[k + 1] = (s >> 32) as u32;

            let m = t[0].wrapping_mul(self.n0inv);
            let mut carry = (t[0] as u64 + m as u64 * self.n[0] as u64) >> 32;
            for j in 1..k {
                let s = t[j] as u64 + m as u64 * self.n[j] as u64 + carry;
                t[j - 1] = s as u32;
                carry = s >> 32;
            }
            let s = t[k] as u64 + carry;
            t[k - 1] = s as u32;
            t[k] = t[k + 1] + (s >> 32) as u32;
            t[k + 1] = 0;
        }
        // t < 2n 이므로 많아야 한 번 빼면 됩니다.
        let ge = t[k] != 0 || {
            let mut ge = true;
            for j in (0..k).rev() {
                if t[j] != self.n[j] {
                    ge = t[j] > self.n[j];
                    break;
                }
            }
            ge
        };
        if ge {
            let mut borrow = 0i64;
            for j in 0..k {
                let mut d = t[j] as i64 - self.n[j] as i64 - borrow;
                borrow = 0;
                if d < 0 {
                    d += 1 << 32;
                    borrow = 1;
                }
                t[j] = d as u32;
            }
        }
        t.truncate(k);
        t
    }

    fn pad(&self, v: &BigUint) -> Vec<u32> {
        let mut l = v.limbs.clone();
        l.resize(self.n.len(), 0);
        l
    }

    fn pow(&self, base: &BigUint, exp: &BigUint) -> BigUint {
        let k = self.n.len();
        let base_m = self.mul(&self.pad(base), &self.pad(&self.r2));
        let mut one = vec![0u32; k];
        one[0] = 1;
        let mut acc = self.mul(&one, &self.pad(&self.r2)); // R mod n

        // 4비트 창
        let mut table = vec![acc.clone(), base_m.clone()];
        for i in 2..16 {
            table.push(self.mul(&table[i - 1], &base_m));
        }
        let bits = exp.bits();
        let mut i = bits.div_ceil(4) * 4;
        while i > 0 {
            i -= 4;
            for _ in 0..4 {
                acc = self.mul(&acc, &acc);
            }
            let w = (0..4).fold(0usize, |w, b| w | ((exp.bit(i + b) as usize) << b));
            if w != 0 {
                acc = self.mul(&acc, &table[w]);
            }
        }
        BigUint {
            limbs: self.mul(&acc, &one),
        }
        .normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(v: u64) -> BigUint {
        BigUint::from_u64(v)
    }

    #[test]
    fn arithmetic_matches_u128() {
        let a = 0xdead_beef_1234_5678u64;
        let b = 0x0fed_cba9_8765_4321u64;
        assert_eq!(
            n(a).mul(&n(b)).to_be_bytes(16),
            (a as u128 * b as u128).to_be_bytes().to_vec()
        );
        let (q, r) = n(a).divrem(&n(12345));
        assert_eq!((q, r), (n(a / 12345), n(a % 12345)));
        assert_eq!(n(a).sub(&n(b)), n(a - b));
        assert_eq!(n(a).shr(13), n(a >> 13));
        assert_eq!(n(a).rem_u32(97), (a % 97) as u32);
    }

    #[test]
    fn modpow_and_modinv() {
        // 2^100 mod 1_000_000_007
        let m = n(1_000_000_007);
        let expect = (0..100).fold(1u128, |acc, _| acc * 2 % 1_000_000_007) as u64;
        assert_eq!(n(2).modpow(&n(100), &m), n(expect));
        assert_eq!(n(3).modinv(&n(11)), Some(n(4)));
        assert_eq!(n(6).modinv(&n(9)), None);
        // 큰 수: (a^e)^d = a mod p 형태의 페르마 검사, p = 2^127 - 1
        let p = BigUint::from_be_bytes(&((1u128 << 127) - 1).to_be_bytes());
        let a = n(123456789);
        assert_eq!(a.modpow(&p.sub(&n(1)), &p), n(1));
    }

    #[test]
    fn bytes_round_trip() {
        let bytes = [0x01, 0x02, 0x03, 0x04, 0x05];
        let v = BigUint::from_be_bytes(&bytes);
        assert_eq!(v.to_be_bytes(5), bytes);
        assert_eq!(v.to_be_bytes(7), [0, 0, 1, 2, 3, 4, 5]);
        assert_eq!(v.to_be_bytes_minimal(), bytes);
    }
}
