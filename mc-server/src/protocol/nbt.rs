//! 네트워크 NBT 쓰기. 1.20.2부터 네트워크 NBT는 루트 태그에 이름이 없습니다
//! (`태그 타입 | 내용`). 문자열은 Java의 "modified UTF-8"이지만, 우리가 쓰는 문자열엔
//! NUL과 BMP 밖 문자가 없으므로 UTF-8과 같습니다 — 그런 문자가 오면 바꿔 씁니다.

const TAG_END: u8 = 0;
const TAG_BYTE: u8 = 1;
const TAG_STRING: u8 = 8;
const TAG_LIST: u8 = 9;
const TAG_COMPOUND: u8 = 10;

/// 쓸 수 있는 태그. 텍스트 컴포넌트에 필요한 만큼만 있습니다.
#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    String(String),
    /// 같은 타입의 태그 목록. 비어 있으면 TAG_End 목록으로 씁니다.
    List(Vec<Tag>),
    Compound(Vec<(String, Tag)>),
}

impl Tag {
    fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => TAG_BYTE,
            Tag::String(_) => TAG_STRING,
            Tag::List(_) => TAG_LIST,
            Tag::Compound(_) => TAG_COMPOUND,
        }
    }

    /// 이름 없는 루트로 씁니다.
    pub fn write_root(&self, out: &mut Vec<u8>) {
        out.push(self.id());
        self.write_payload(out);
    }

    fn write_payload(&self, out: &mut Vec<u8>) {
        match self {
            Tag::Byte(v) => out.push(*v as u8),
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
        }
    }
}

fn write_nbt_string(out: &mut Vec<u8>, s: &str) {
    let mut bytes = Vec::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\0' => bytes.extend_from_slice(&[0xC0, 0x80]),
            c if (c as u32) > 0xFFFF => bytes.extend_from_slice("\u{FFFD}".as_bytes()),
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

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    fn string(&mut self) -> Option<String> {
        let len = u16::from_be_bytes(self.take(2)?.try_into().ok()?) as usize;
        Some(String::from_utf8_lossy(self.take(len)?).into_owned())
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

    use super::*;

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
    fn empty_list_is_end_typed() {
        let mut out = Vec::new();
        Tag::List(vec![]).write_root(&mut out);
        assert_eq!(out, [9, 0, 0, 0, 0, 0]);
    }
}
