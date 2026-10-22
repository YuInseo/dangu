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

#[cfg(test)]
mod tests {
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
