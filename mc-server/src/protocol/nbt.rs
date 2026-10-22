//! NBT 읽고 쓰기. 1.20.2부터 네트워크 NBT는 루트 태그에 이름이 없고(`태그 타입 | 내용`),
//! 파일(.mca, level.dat)은 루트에 이름이 있습니다(대개 빈 문자열).
//!
//! 문자열은 Java의 "modified UTF-8"입니다. NUL은 두 바이트로, BMP 밖 문자는 대리쌍(각 3바이트)으로
//! 씁니다. 읽을 때도 같은 규칙으로 풉니다.

const TAG_END: u8 = 0;
const TAG_BYTE: u8 = 1;
const TAG_SHORT: u8 = 2;
const TAG_INT: u8 = 3;
const TAG_LONG: u8 = 4;
const TAG_FLOAT: u8 = 5;
const TAG_DOUBLE: u8 = 6;
const TAG_BYTE_ARRAY: u8 = 7;
const TAG_STRING: u8 = 8;
const TAG_LIST: u8 = 9;
const TAG_COMPOUND: u8 = 10;
const TAG_INT_ARRAY: u8 = 11;
const TAG_LONG_ARRAY: u8 = 12;

/// 중첩 깊이 제한 (바닐라와 같은 512). 악의적인 입력이 스택을 넘치게 하지 않도록.
const MAX_DEPTH: u32 = 512;

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    /// 같은 타입의 태그 목록. 비어 있으면 TAG_End 목록으로 씁니다.
    List(Vec<Tag>),
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => TAG_BYTE,
            Tag::Short(_) => TAG_SHORT,
            Tag::Int(_) => TAG_INT,
            Tag::Long(_) => TAG_LONG,
            Tag::Float(_) => TAG_FLOAT,
            Tag::Double(_) => TAG_DOUBLE,
            Tag::ByteArray(_) => TAG_BYTE_ARRAY,
            Tag::String(_) => TAG_STRING,
            Tag::List(_) => TAG_LIST,
            Tag::Compound(_) => TAG_COMPOUND,
            Tag::IntArray(_) => TAG_INT_ARRAY,
            Tag::LongArray(_) => TAG_LONG_ARRAY,
        }
    }

    /// 이름 없는 루트로 씁니다 (네트워크).
    pub fn write_root(&self, out: &mut Vec<u8>) {
        out.push(self.id());
        self.write_payload(out);
    }

    /// 이름 있는 루트로 씁니다 (파일).
    pub fn write_named(&self, name: &str, out: &mut Vec<u8>) {
        out.push(self.id());
        write_nbt_string(out, name);
        self.write_payload(out);
    }

    fn write_payload(&self, out: &mut Vec<u8>) {
        match self {
            Tag::Byte(v) => out.push(*v as u8),
            Tag::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
            Tag::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
            Tag::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
            Tag::Float(v) => out.extend_from_slice(&v.to_be_bytes()),
            Tag::Double(v) => out.extend_from_slice(&v.to_be_bytes()),
            Tag::ByteArray(v) => {
                out.extend_from_slice(&(v.len() as i32).to_be_bytes());
                out.extend(v.iter().map(|&b| b as u8));
            }
            Tag::String(s) => write_nbt_string(out, s),
            Tag::List(items) => {
                let elem = items.first().map_or(TAG_END, Tag::id);
                debug_assert!(
                    items.iter().all(|t| t.id() == elem),
                    "NBT list of mixed types"
                );
                out.push(elem);
                out.extend_from_slice(&(items.len() as i32).to_be_bytes());
                for item in items {
                    item.write_payload(out);
                }
            }
            Tag::Compound(fields) => {
                for (name, tag) in fields {
                    out.push(tag.id());
                    write_nbt_string(out, name);
                    tag.write_payload(out);
                }
                out.push(TAG_END);
            }
            Tag::IntArray(v) => {
                out.extend_from_slice(&(v.len() as i32).to_be_bytes());
                for x in v {
                    out.extend_from_slice(&x.to_be_bytes());
                }
            }
            Tag::LongArray(v) => {
                out.extend_from_slice(&(v.len() as i32).to_be_bytes());
                for x in v {
                    out.extend_from_slice(&x.to_be_bytes());
                }
            }
        }
    }

    // ---- 읽기 ----

    /// 이름 있는 루트 (파일). (이름, 태그)
    pub fn read_named(bytes: &[u8]) -> Option<(String, Tag)> {
        let mut r = Reader(bytes);
        let kind = r.u8()?;
        let name = r.string()?;
        let tag = r.payload(kind, 0)?;
        Some((name, tag))
    }

    /// 이름 없는 루트 (네트워크).
    pub fn read_root(bytes: &[u8]) -> Option<Tag> {
        let mut r = Reader(bytes);
        let kind = r.u8()?;
        r.payload(kind, 0)
    }

    // ---- 꺼내기 ----

    pub fn get(&self, key: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(f) => f.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// 정수 계열 무엇이든 i64로.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Tag::Byte(v) => Some(*v as i64),
            Tag::Short(v) => Some(*v as i64),
            Tag::Int(v) => Some(*v as i64),
            Tag::Long(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Tag::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_list(&self) -> &[Tag] {
        match self {
            Tag::List(v) => v,
            _ => &[],
        }
    }

    pub fn as_longs(&self) -> &[i64] {
        match self {
            Tag::LongArray(v) => v,
            _ => &[],
        }
    }

    pub fn fields(&self) -> &[(String, Tag)] {
        match self {
            Tag::Compound(f) => f,
            _ => &[],
        }
    }
}

fn write_nbt_string(out: &mut Vec<u8>, s: &str) {
    let mut bytes = Vec::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\0' => bytes.extend_from_slice(&[0xC0, 0x80]),
            c if (c as u32) > 0xFFFF => {
                // 대리쌍을 각각 3바이트 UTF-8로 (modified UTF-8)
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    let u = *u as u32;
                    bytes.extend_from_slice(&[
                        0xE0 | (u >> 12) as u8,
                        0x80 | ((u >> 6) & 0x3F) as u8,
                        0x80 | (u & 0x3F) as u8,
                    ]);
                }
            }
            c => {
                let mut buf = [0; 4];
                bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    let len = bytes.len().min(u16::MAX as usize);
    out.extend_from_slice(&(len as u16).to_be_bytes());
    out.extend_from_slice(&bytes[..len]);
}

/// modified UTF-8 → String. 잘못된 바이트는 대체 문자로.
fn decode_mutf8(b: &[u8]) -> String {
    let mut units: Vec<u16> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as u32;
        if c < 0x80 {
            units.push(c as u16);
            i += 1;
        } else if c & 0xE0 == 0xC0 && i + 1 < b.len() {
            units.push((((c & 0x1F) << 6) | (b[i + 1] as u32 & 0x3F)) as u16);
            i += 2;
        } else if c & 0xF0 == 0xE0 && i + 2 < b.len() {
            units.push(
                (((c & 0x0F) << 12) | ((b[i + 1] as u32 & 0x3F) << 6) | (b[i + 2] as u32 & 0x3F))
                    as u16,
            );
            i += 3;
        } else {
            units.push(0xFFFD);
            i += 1;
        }
    }
    String::from_utf16_lossy(&units)
}

/// 이름 없는 루트 컴파운드에서 문자열 `key`의 값을 찾습니다 (맨 윗단계만).
/// 잘못된 NBT면 None.
pub fn find_string(bytes: &[u8], key: &str) -> Option<String> {
    let mut r = Reader(bytes);
    if r.u8()? != TAG_COMPOUND {
        return None;
    }
    loop {
        let kind = r.u8()?;
        if kind == TAG_END {
            return None;
        }
        let name = r.string()?;
        if kind == TAG_STRING && name == key {
            return r.string();
        }
        r.skip(kind, 0)?;
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.0.len() < n {
            return None;
        }
        let (h, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(h)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn i16(&mut self) -> Option<i16> {
        Some(i16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_be_bytes(self.take(8)?.try_into().ok()?))
    }

    fn len(&mut self, unit: usize) -> Option<usize> {
        let n = usize::try_from(self.i32()?).ok()?;
        // 남은 바이트보다 많다고 주장하면 거부 (거대한 할당 방지)
        (n.checked_mul(unit)? <= self.0.len()).then_some(n)
    }

    fn string(&mut self) -> Option<String> {
        let len = u16::from_be_bytes(self.take(2)?.try_into().ok()?) as usize;
        Some(decode_mutf8(self.take(len)?))
    }

    fn payload(&mut self, kind: u8, depth: u32) -> Option<Tag> {
        if depth > MAX_DEPTH {
            return None;
        }
        Some(match kind {
            TAG_BYTE => Tag::Byte(self.u8()? as i8),
            TAG_SHORT => Tag::Short(self.i16()?),
            TAG_INT => Tag::Int(self.i32()?),
            TAG_LONG => Tag::Long(self.i64()?),
            TAG_FLOAT => Tag::Float(f32::from_bits(self.i32()? as u32)),
            TAG_DOUBLE => Tag::Double(f64::from_bits(self.i64()? as u64)),
            TAG_BYTE_ARRAY => {
                let n = self.len(1)?;
                Tag::ByteArray(self.take(n)?.iter().map(|&b| b as i8).collect())
            }
            TAG_STRING => Tag::String(self.string()?),
            TAG_LIST => {
                let elem = self.u8()?;
                let n = self.len(if elem == TAG_END { 0 } else { 1 })?;
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    v.push(self.payload(elem, depth + 1)?);
                }
                Tag::List(v)
            }
            TAG_COMPOUND => {
                let mut f = Vec::new();
                loop {
                    let k = self.u8()?;
                    if k == TAG_END {
                        break;
                    }
                    let name = self.string()?;
                    f.push((name, self.payload(k, depth + 1)?));
                }
                Tag::Compound(f)
            }
            TAG_INT_ARRAY => {
                let n = self.len(4)?;
                Tag::IntArray((0..n).map(|_| self.i32()).collect::<Option<_>>()?)
            }
            TAG_LONG_ARRAY => {
                let n = self.len(8)?;
                Tag::LongArray((0..n).map(|_| self.i64()).collect::<Option<_>>()?)
            }
            _ => return None,
        })
    }

    /// 내용 하나를 건너뜁니다. 깊이 제한으로 악의적인 중첩을 막습니다.
    fn skip(&mut self, kind: u8, depth: u32) -> Option<()> {
        if depth > 32 {
            return None;
        }
        match kind {
            1 => self.take(1).map(|_| ()),
            2 => self.take(2).map(|_| ()),
            3 | 5 => self.take(4).map(|_| ()),
            4 | 6 => self.take(8).map(|_| ()),
            7 => {
                let n = usize::try_from(self.i32()?).ok()?;
                self.take(n).map(|_| ())
            }
            8 => self.string().map(|_| ()),
            9 => {
                let elem = self.u8()?;
                let n = self.i32()?.max(0);
                for _ in 0..n {
                    self.skip(elem, depth + 1)?;
                }
                Some(())
            }
            10 => loop {
                let k = self.u8()?;
                if k == TAG_END {
                    return Some(());
                }
                self.string()?;
                self.skip(k, depth + 1)?;
            },
            11 => {
                let n = usize::try_from(self.i32()?).ok()?;
                self.take(n * 4).map(|_| ())
            }
            12 => {
                let n = usize::try_from(self.i32()?).ok()?;
                self.take(n * 8).map(|_| ())
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_string_after_other_tags() {
        let tag = Tag::Compound(vec![
            ("x".into(), Tag::Byte(1)),
            ("list".into(), Tag::List(vec![Tag::String("a".into())])),
            ("dangu:item".into(), Tag::String("sword".into())),
        ]);
        let mut out = Vec::new();
        tag.write_root(&mut out);
        assert_eq!(find_string(&out, "dangu:item").as_deref(), Some("sword"));
        assert_eq!(find_string(&out, "nope"), None);
        assert_eq!(find_string(&out[..5], "dangu:item"), None);
    }

    #[test]
    fn plain_string_root() {
        let mut out = Vec::new();
        Tag::String("hi".into()).write_root(&mut out);
        assert_eq!(out, [8, 0, 2, b'h', b'i']);
    }

    #[test]
    fn compound_root() {
        let mut out = Vec::new();
        Tag::Compound(vec![("bold".into(), Tag::Byte(1))]).write_root(&mut out);
        assert_eq!(out, [10, 1, 0, 4, b'b', b'o', b'l', b'd', 1, 0]);
    }

    #[test]
    fn every_type_round_trips() {
        let tag = Tag::Compound(vec![
            ("b".into(), Tag::Byte(-3)),
            ("s".into(), Tag::Short(-300)),
            ("i".into(), Tag::Int(1 << 20)),
            ("l".into(), Tag::Long(-1 << 40)),
            ("f".into(), Tag::Float(1.5)),
            ("d".into(), Tag::Double(-2.25)),
            ("ba".into(), Tag::ByteArray(vec![1, -1])),
            ("str".into(), Tag::String("한글 \0 😀".into())),
            ("list".into(), Tag::List(vec![Tag::Int(1), Tag::Int(2)])),
            ("empty".into(), Tag::List(vec![])),
            ("ia".into(), Tag::IntArray(vec![7, -7])),
            ("la".into(), Tag::LongArray(vec![i64::MAX, i64::MIN])),
            ("c".into(), Tag::Compound(vec![("x".into(), Tag::Byte(0))])),
        ]);
        let mut out = Vec::new();
        tag.write_named("", &mut out);
        assert_eq!(Tag::read_named(&out), Some((String::new(), tag.clone())));
        let mut net = Vec::new();
        tag.write_root(&mut net);
        assert_eq!(Tag::read_root(&net), Some(tag));
    }

    #[test]
    fn rejects_lies_about_length() {
        // 리스트가 20억 개라고 주장
        let bytes = [10, 0, 0, 9, 0, 1, b'x', 1, 0x7F, 0xFF, 0xFF, 0xFF];
        assert_eq!(Tag::read_named(&bytes), None);
    }
}
