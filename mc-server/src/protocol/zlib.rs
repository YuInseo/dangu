//! zlib (RFC 1950) 위의 DEFLATE (RFC 1951). 압축 패킷에 씁니다.
//!
//! 풀기는 세 블록 형식(저장·고정 허프만·동적 허프만)을 모두 처리합니다. 압축은 LZ77(해시
//! 체인) + 고정 허프만만 씁니다 — 청크 데이터처럼 반복이 많은 데이터에선 이걸로도 잘 줄고,
//! 동적 허프만 표를 만드는 비용이 없습니다.

/// 풀었을 때 허용하는 최대 크기 (바닐라와 같은 8 MiB).
pub const MAX_INFLATED: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZlibError {
    BadHeader,
    BadData(&'static str),
    TooLarge,
    Checksum,
}

impl std::fmt::Display for ZlibError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

// ---- 공통 표 ----

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

// ---- 풀기 ----

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
    nbits: u32,
}

impl BitReader<'_> {
    fn bits(&mut self, n: u32) -> Result<u32, ZlibError> {
        while self.nbits < n {
            let byte = *self
                .data
                .get(self.pos)
                .ok_or(ZlibError::BadData("unexpected end"))?;
            self.pos += 1;
            self.bit |= (byte as u32) << self.nbits;
            self.nbits += 8;
        }
        let v = self.bit & ((1u64 << n) - 1) as u32;
        self.bit >>= n;
        self.nbits -= n;
        Ok(v)
    }

    fn align(&mut self) {
        self.bit = 0;
        self.nbits = 0;
    }
}

/// 정규 허프만 부호: 길이별 개수와 길이순으로 정렬한 기호.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Self, ZlibError> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut left = 1i32;
        for &c in &counts[1..] {
            left = (left << 1) - c as i32;
            if left < 0 {
                return Err(ZlibError::BadData("over-subscribed code"));
            }
        }
        let mut offs = [0u16; 16];
        for i in 1..15 {
            offs[i + 1] = offs[i] + counts[i];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode(&self, br: &mut BitReader) -> Result<u16, ZlibError> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= br.bits(1)? as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return Ok(self.symbols[(index + code - first) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(ZlibError::BadData("bad huffman code"))
    }
}

fn fixed_tables() -> (Huffman, Huffman) {
    let mut lit = [0u8; 288];
    lit[..144].fill(8);
    lit[144..256].fill(9);
    lit[256..280].fill(7);
    lit[280..].fill(8);
    (
        Huffman::new(&lit).unwrap(),
        Huffman::new(&[5u8; 30]).unwrap(),
    )
}

fn dynamic_tables(br: &mut BitReader) -> Result<(Huffman, Huffman), ZlibError> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let nlen = br.bits(5)? as usize + 257;
    let ndist = br.bits(5)? as usize + 1;
    let ncode = br.bits(4)? as usize + 4;
    if nlen > 286 || ndist > 30 {
        return Err(ZlibError::BadData("bad counts"));
    }
    let mut code_lengths = [0u8; 19];
    for &i in &ORDER[..ncode] {
        code_lengths[i] = br.bits(3)? as u8;
    }
    let code = Huffman::new(&code_lengths)?;
    let mut lengths = vec![0u8; nlen + ndist];
    let mut i = 0;
    while i < nlen + ndist {
        let sym = code.decode(br)?;
        let (value, repeat) = match sym {
            0..=15 => (sym as u8, 1),
            16 => (
                *lengths
                    .get(i.wrapping_sub(1))
                    .ok_or(ZlibError::BadData("repeat at start"))?,
                3 + br.bits(2)? as usize,
            ),
            17 => (0, 3 + br.bits(3)? as usize),
            _ => (0, 11 + br.bits(7)? as usize),
        };
        if i + repeat > lengths.len() {
            return Err(ZlibError::BadData("too many lengths"));
        }
        lengths[i..i + repeat].fill(value);
        i += repeat;
    }
    Ok((
        Huffman::new(&lengths[..nlen])?,
        Huffman::new(&lengths[nlen..])?,
    ))
}

/// zlib 스트림을 풉니다. `limit`보다 커지면 거부합니다 (압축 폭탄 방지).
pub fn decompress(data: &[u8], limit: usize) -> Result<Vec<u8>, ZlibError> {
    if data.len() < 6
        || !u16::from_be_bytes([data[0], data[1]]).is_multiple_of(31)
        || data[0] & 0x0F != 8
        || data[1] & 0x20 != 0
    {
        return Err(ZlibError::BadHeader);
    }
    let (out, used) = inflate(&data[2..], limit)?;
    let tail = data
        .get(2 + used..2 + used + 4)
        .ok_or(ZlibError::BadData("missing checksum"))?;
    if u32::from_be_bytes(tail.try_into().unwrap()) != adler32(&out) {
        return Err(ZlibError::Checksum);
    }
    Ok(out)
}

/// 날 deflate 스트림을 풉니다. (풀린 바이트, 읽은 압축 바이트 수)
pub fn inflate(data: &[u8], limit: usize) -> Result<(Vec<u8>, usize), ZlibError> {
    let mut br = BitReader {
        data,
        pos: 0,
        bit: 0,
        nbits: 0,
    };
    let mut out = Vec::new();
    loop {
        let last = br.bits(1)? == 1;
        match br.bits(2)? {
            0 => {
                br.align();
                let d = br.data;
                let p = br.pos;
                if d.len() < p + 4 {
                    return Err(ZlibError::BadData("stored header"));
                }
                let len = u16::from_le_bytes([d[p], d[p + 1]]) as usize;
                let nlen = u16::from_le_bytes([d[p + 2], d[p + 3]]) as usize;
                if len != !nlen & 0xFFFF || d.len() < p + 4 + len {
                    return Err(ZlibError::BadData("stored length"));
                }
                out.extend_from_slice(&d[p + 4..p + 4 + len]);
                br.pos = p + 4 + len;
            }
            t @ (1 | 2) => {
                let (lit, dist) = if t == 1 {
                    fixed_tables()
                } else {
                    dynamic_tables(&mut br)?
                };
                loop {
                    let sym = lit.decode(&mut br)? as usize;
                    if sym < 256 {
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let i = sym - 257;
                        if i >= 29 {
                            return Err(ZlibError::BadData("bad length symbol"));
                        }
                        let len = LEN_BASE[i] as usize + br.bits(LEN_EXTRA[i] as u32)? as usize;
                        let d = dist.decode(&mut br)? as usize;
                        if d >= 30 {
                            return Err(ZlibError::BadData("bad distance symbol"));
                        }
                        let distance =
                            DIST_BASE[d] as usize + br.bits(DIST_EXTRA[d] as u32)? as usize;
                        if distance > out.len() {
                            return Err(ZlibError::BadData("distance too far"));
                        }
                        let start = out.len() - distance;
                        for k in 0..len {
                            out.push(out[start + k]);
                        }
                    }
                    if out.len() > limit {
                        return Err(ZlibError::TooLarge);
                    }
                }
            }
            _ => return Err(ZlibError::BadData("reserved block type")),
        }
        if out.len() > limit {
            return Err(ZlibError::TooLarge);
        }
        if last {
            break;
        }
    }
    // 바이트 경계까지 읽은 자리
    Ok((out, br.pos))
}

/// CRC-32 (gzip, PNG와 같은 다항식).
pub fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *e = c;
        }
        t
    });
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

/// gzip 파일 풀기 (level.dat).
pub fn gunzip(data: &[u8], limit: usize) -> Result<Vec<u8>, ZlibError> {
    if data.len() < 18 || data[0] != 0x1F || data[1] != 0x8B || data[2] != 8 {
        return Err(ZlibError::BadHeader);
    }
    let flags = data[3];
    let mut p = 10;
    if flags & 0x04 != 0 {
        let xlen = u16::from_le_bytes([data[p], data[p + 1]]) as usize;
        p += 2 + xlen;
    }
    for bit in [0x08, 0x10] {
        if flags & bit != 0 {
            while p < data.len() && data[p] != 0 {
                p += 1;
            }
            p += 1;
        }
    }
    if flags & 0x02 != 0 {
        p += 2;
    }
    let (out, used) = inflate(data.get(p..).ok_or(ZlibError::BadHeader)?, limit)?;
    let tail = data
        .get(p + used..p + used + 8)
        .ok_or(ZlibError::BadData("missing trailer"))?;
    if u32::from_le_bytes(tail[..4].try_into().unwrap()) != crc32(&out) {
        return Err(ZlibError::Checksum);
    }
    Ok(out)
}

/// gzip으로 압축 (level.dat).
pub fn gzip(data: &[u8]) -> Vec<u8> {
    let z = compress(data);
    let mut out = vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 0xFF];
    out.extend_from_slice(&z[2..z.len() - 4]);
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

// ---- 압축 ----

struct BitWriter {
    out: Vec<u8>,
    bit: u64,
    nbits: u32,
}

impl BitWriter {
    fn put(&mut self, value: u32, n: u32) {
        self.bit |= (value as u64) << self.nbits;
        self.nbits += n;
        while self.nbits >= 8 {
            self.out.push(self.bit as u8);
            self.bit >>= 8;
            self.nbits -= 8;
        }
    }

    /// 허프만 부호는 최상위 비트부터 씁니다.
    fn put_code(&mut self, code: u32, len: u32) {
        self.put(code.reverse_bits() >> (32 - len), len);
    }

    fn flush(&mut self) {
        if self.nbits > 0 {
            self.out.push(self.bit as u8);
        }
        self.bit = 0;
        self.nbits = 0;
    }
}

fn put_literal(w: &mut BitWriter, sym: u32) {
    match sym {
        0..=143 => w.put_code(0x30 + sym, 8),
        144..=255 => w.put_code(0x190 + sym - 144, 9),
        256..=279 => w.put_code(sym - 256, 7),
        _ => w.put_code(0xC0 + sym - 280, 8),
    }
}

fn put_match(w: &mut BitWriter, len: usize, dist: usize) {
    let li = LEN_BASE.partition_point(|&b| b as usize <= len) - 1;
    put_literal(w, 257 + li as u32);
    w.put((len - LEN_BASE[li] as usize) as u32, LEN_EXTRA[li] as u32);
    let di = DIST_BASE.partition_point(|&b| b as usize <= dist) - 1;
    w.put_code(di as u32, 5);
    w.put(
        (dist - DIST_BASE[di] as usize) as u32,
        DIST_EXTRA[di] as u32,
    );
}

const WINDOW: usize = 32 * 1024;
const HASH_BITS: u32 = 15;
const MAX_CHAIN: usize = 32;

fn hash3(d: &[u8], i: usize) -> usize {
    let v = (d[i] as u32) << 16 | (d[i + 1] as u32) << 8 | d[i + 2] as u32;
    (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

/// zlib 스트림으로 압축합니다.
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter {
        out: vec![0x78, 0x9C],
        bit: 0,
        nbits: 0,
    };
    w.put(1, 1); // 마지막 블록
    w.put(1, 2); // 고정 허프만

    let mut head = vec![usize::MAX; 1 << HASH_BITS];
    let mut prev = vec![usize::MAX; data.len()];
    let mut i = 0;
    let insert = |head: &mut Vec<usize>, prev: &mut Vec<usize>, i: usize| {
        if i + 2 < data.len() {
            let h = hash3(data, i);
            prev[i] = head[h];
            head[h] = i;
        }
    };
    while i < data.len() {
        let (mut best_len, mut best_dist) = (0, 0);
        if i + 2 < data.len() {
            let mut cand = head[hash3(data, i)];
            let max_len = (data.len() - i).min(258);
            for _ in 0..MAX_CHAIN {
                if cand == usize::MAX || i - cand > WINDOW {
                    break;
                }
                let len = data[cand..]
                    .iter()
                    .zip(&data[i..i + max_len])
                    .take_while(|(a, b)| a == b)
                    .count();
                if len > best_len {
                    best_len = len;
                    best_dist = i - cand;
                    if len == max_len {
                        break;
                    }
                }
                cand = prev[cand];
            }
        }
        if best_len >= 3 {
            put_match(&mut w, best_len, best_dist);
            for k in i..i + best_len {
                insert(&mut head, &mut prev, k);
            }
            i += best_len;
        } else {
            put_literal(&mut w, data[i] as u32);
            insert(&mut head, &mut prev, i);
            i += 1;
        }
    }
    put_literal(&mut w, 256);
    w.flush();
    w.out.extend_from_slice(&adler32(data).to_be_bytes());
    w.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gzip_round_trip_and_crc() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        let data = b"level.dat level.dat level.dat".repeat(50);
        assert_eq!(gunzip(&gzip(&data), 1 << 20).unwrap(), data);
    }

    #[test]
    fn adler_vector() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E60398);
    }

    #[test]
    fn round_trips() {
        let samples: Vec<Vec<u8>> = vec![
            vec![],
            b"a".to_vec(),
            b"hello hello hello hello world".to_vec(),
            (0..70000u32).map(|i| (i * 7 % 251) as u8).collect(),
            vec![0u8; 100_000],
            crate::crypto::random::bytes::<4096>().to_vec(),
        ];
        for s in samples {
            let c = compress(&s);
            assert_eq!(decompress(&c, MAX_INFLATED).unwrap(), s);
        }
        let zeros = compress(&vec![0u8; 100_000]);
        assert!(zeros.len() < 1000, "zeros compress well: {}", zeros.len());
    }

    #[test]
    fn inflates_python_zlib_output() {
        // python3: zlib.compress(b'minecraft ' * 20)  → 고정 허프만 블록
        let fixed = hex("789ccbcdcc4b4d2e4a4c2b51c81dd22c0051d64cf5");
        assert_eq!(
            decompress(&fixed, MAX_INFLATED).unwrap(),
            b"minecraft ".repeat(20)
        );
        // python3: zlib.compress(" ".join(f"word{i%37} item{i%11}" for i in range(60)).encode(), 9)
        // → 동적 허프만 블록
        let dynamic = hex(
            "78da4dd24b0a02411004d1ab7884aeaceadf7d74e1420411e6fa4a8e42ec62578fce3e9eaf6bbbdcdfb747bb1cdf0e77b8e5963bdde92e77b9bbbbbb877bb8a77bba977bb9b77b9fb7cec3f1bb1c64088e4840a220890e4a0c586202130b9ad8e0a8c1a3a0478247c9772978d4e1d18047131e2d78b4e1c9064f063c297a32e1c982273b871af0343e0f35c0242c054a87641032e158606caed4b8529021ae945ca9b85207e5b7ece6b2ff5fb3f86b36576a5c29b8d249a80f753bfd11",
        );
        let expect: Vec<String> = (0..60)
            .map(|i| format!("word{} item{}", i % 37, i % 11))
            .collect();
        assert_eq!(
            decompress(&dynamic, MAX_INFLATED).unwrap(),
            expect.join(" ").into_bytes()
        );
        // zlib.compress(b'abc', 0) → 저장 블록
        let stored = hex("7801010300fcff616263024d0127");
        assert_eq!(decompress(&stored, MAX_INFLATED).unwrap(), b"abc");
    }

    #[test]
    fn rejects_bombs_and_corruption() {
        let c = compress(&vec![0u8; 10_000]);
        assert_eq!(decompress(&c, 1000), Err(ZlibError::TooLarge));
        let mut bad = compress(b"some data here");
        let n = bad.len();
        bad[n - 1] ^= 1;
        assert_eq!(decompress(&bad, MAX_INFLATED), Err(ZlibError::Checksum));
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
}

/// 다른 압축기(zlib 모든 단계·전략, libdeflate)가 만든 스트림을 풀어 봅니다.
/// `MC_ZLIB_CORPUS=<폴더> cargo test zlib_corpus -- --ignored` (폴더: `N.z` + 원본 `N.raw`).
#[cfg(test)]
#[test]
#[ignore]
fn zlib_corpus() {
    let Ok(dir) = std::env::var("MC_ZLIB_CORPUS") else {
        return;
    };
    let mut bad = Vec::new();
    let mut n = 0;
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "z") {
            let z = std::fs::read(&p).unwrap();
            let raw = std::fs::read(p.with_extension("raw")).unwrap();
            n += 1;
            match decompress(&z, MAX_INFLATED) {
                Ok(out) if out == raw => {}
                other => bad.push((
                    p.display().to_string(),
                    format!("{:?}", other.map(|o| o.len())),
                )),
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{n}개 중 {}개 실패: {:?}",
        bad.len(),
        &bad[..bad.len().min(10)]
    );
    println!("{n}개 모두 풀림");
}
