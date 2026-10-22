//! Configuration 상태 (1.20.2~). 레지스트리와 태그를 맞추고 Play로 넘어갑니다.
//!
//! 클라이언트와 서버가 같은 "known pack"(`minecraft:core` 26.3)을 가졌다고 합의하면,
//! Registry Data에 항목 이름만 보내고 내용(NBT)은 생략할 수 있습니다 — 클라이언트가
//! 자기 jar의 데이터로 채웁니다. 이 서버는 그 방식만 씁니다.

use crate::data::{SYNCED_REGISTRIES, TAGS};
use crate::protocol::DecodeError;
use crate::protocol::types::{read_string, write_bool, write_string};
use crate::protocol::varint::{read_varint, write_varint};

// Clientbound — ViaVersion `ClientboundConfigurationPackets26_3`의 순서.
// (그 파일의 주석 번호는 POST_EFFECTS가 끼어들기 전 것이라 0x0A부터 하나씩 어긋나 있습니다.)
pub const CUSTOM_PAYLOAD_ID: i32 = 0x01;
pub const DISCONNECT_ID: i32 = 0x02;
pub const FINISH_CONFIGURATION_ID: i32 = 0x03;
pub const REGISTRY_DATA_ID: i32 = 0x07;
pub const UPDATE_ENABLED_FEATURES_ID: i32 = 0x0D;
pub const UPDATE_TAGS_ID: i32 = 0x0E;
pub const SELECT_KNOWN_PACKS_ID: i32 = 0x0F;
/// `Add Resource Pack`: UUID, 주소, SHA-1(16진수 40자), 강제, 안내 문구(선택).
pub const ADD_RESOURCE_PACK_ID: i32 = 0x09;

// Serverbound
pub const CLIENT_INFORMATION_ID: i32 = 0x00;
pub const SB_CUSTOM_PAYLOAD_ID: i32 = 0x02;
pub const FINISH_CONFIGURATION_ACK_ID: i32 = 0x03;
pub const SB_KEEP_ALIVE_ID: i32 = 0x04;
pub const SB_SELECT_KNOWN_PACKS_ID: i32 = 0x07;
/// `Resource Pack Response`: UUID, 결과.
pub const SB_RESOURCE_PACK_ID: i32 = 0x06;

pub fn encode_add_resource_pack(
    uuid: u128,
    url: &str,
    hash: &str,
    forced: bool,
    prompt: Option<&crate::protocol::nbt::Tag>,
) -> Vec<u8> {
    let mut out = Vec::new();
    crate::protocol::types::write_uuid(&mut out, uuid);
    crate::protocol::types::write_string(&mut out, url);
    crate::protocol::types::write_string(&mut out, hash);
    out.push(forced as u8);
    match prompt {
        Some(t) => {
            out.push(1);
            out.extend_from_slice(&crate::text::to_nbt(t));
        }
        None => out.push(0),
    }
    out
}

/// (UUID, 결과). 결과: 0 적용됨, 1 거절, 2 내려받기 실패, 3 수락, 4 내려받음, 5 주소 잘못,
/// 6 다시 읽기 실패, 7 버림.
pub fn decode_resource_pack_response(mut buf: &[u8]) -> Result<(u128, i32), DecodeError> {
    let uuid = crate::protocol::types::read_uuid(&mut buf)?;
    let result = crate::protocol::varint::read_varint(&mut buf)?;
    Ok((uuid, result))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownPack {
    pub namespace: String,
    pub id: String,
    pub version: String,
}

/// 우리가 가진 유일한 팩.
pub fn core_pack() -> KnownPack {
    KnownPack {
        namespace: "minecraft".into(),
        id: "core".into(),
        version: crate::protocol::VERSION_NAME.into(),
    }
}

pub fn encode_known_packs(packs: &[KnownPack]) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, packs.len() as i32);
    for p in packs {
        write_string(&mut out, &p.namespace);
        write_string(&mut out, &p.id);
        write_string(&mut out, &p.version);
    }
    out
}

pub fn decode_known_packs(mut buf: &[u8]) -> Result<Vec<KnownPack>, DecodeError> {
    let n = read_varint(&mut buf)?;
    if !(0..=64).contains(&n) {
        return Err(DecodeError::InvalidValue("known pack count"));
    }
    (0..n)
        .map(|_| {
            Ok(KnownPack {
                namespace: read_string(&mut buf, 32767)?,
                id: read_string(&mut buf, 32767)?,
                version: read_string(&mut buf, 32767)?,
            })
        })
        .collect()
}

/// `Client Information`: 언어, 시야 거리, 채팅 설정, 채팅 색, 겉옷 레이어, 주 손, …
/// (Configuration과 Play에서 같은 형식)
pub fn apply_client_information(
    mut buf: &[u8],
    s: &mut crate::session::ClientSettings,
) -> Result<(), DecodeError> {
    read_string(&mut buf, 16)?;
    let view = *buf.first().ok_or(DecodeError::UnexpectedEof)?;
    buf = &buf[1..];
    s.view_distance = (view as i8).max(2) as u8;
    read_varint(&mut buf)?; // 채팅 모드
    buf = buf.get(1..).ok_or(DecodeError::UnexpectedEof)?; // 채팅 색
    s.skin_parts = *buf.first().ok_or(DecodeError::UnexpectedEof)?;
    buf = &buf[1..];
    s.main_hand = read_varint(&mut buf)?;
    Ok(())
}

/// `Client Information`에서 우리가 쓰는 것: 시야 거리(청크).
pub fn decode_view_distance(mut buf: &[u8]) -> Result<u8, DecodeError> {
    read_string(&mut buf, 16)?; // locale
    let b = buf.first().copied().ok_or(DecodeError::UnexpectedEof)?;
    Ok((b as i8).max(2) as u8)
}

pub fn encode_brand(brand: &str) -> Vec<u8> {
    let mut out = Vec::new();
    write_string(&mut out, "minecraft:brand");
    write_string(&mut out, brand);
    out
}

/// 바닐라 기능 플래그. 이게 없으면 바닐라 블록·아이템이 "비활성"으로 취급됩니다.
pub fn encode_enabled_features() -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, 1);
    write_string(&mut out, "minecraft:vanilla");
    out
}

/// 동기화 레지스트리마다 Registry Data 본문 하나씩. 항목 내용은 known pack에 맡깁니다.
pub fn encode_registries() -> Vec<Vec<u8>> {
    SYNCED_REGISTRIES
        .iter()
        .map(|(registry, entries)| {
            let mut out = Vec::new();
            write_string(&mut out, &format!("minecraft:{registry}"));
            write_varint(&mut out, entries.len() as i32);
            for e in entries.iter() {
                write_string(&mut out, &format!("minecraft:{e}"));
                write_bool(&mut out, false); // 내용 없음
            }
            out
        })
        .collect()
}

pub fn encode_tags() -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, TAGS.len() as i32);
    for (registry, tags) in TAGS {
        write_string(&mut out, &format!("minecraft:{registry}"));
        write_varint(&mut out, tags.len() as i32);
        for (name, ids) in tags.iter() {
            write_string(&mut out, &format!("minecraft:{name}"));
            write_varint(&mut out, ids.len() as i32);
            for &id in ids.iter() {
                write_varint(&mut out, id as i32);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_packs_round_trip() {
        let packs = vec![core_pack()];
        assert_eq!(decode_known_packs(&encode_known_packs(&packs)), Ok(packs));
    }

    #[test]
    fn registries_cover_what_play_needs() {
        let names: Vec<_> = SYNCED_REGISTRIES.iter().map(|(n, _)| *n).collect();
        for needed in [
            "dimension_type",
            "worldgen/biome",
            "damage_type",
            "chat_type",
        ] {
            assert!(names.contains(&needed), "{needed}");
        }
        assert_eq!(encode_registries().len(), SYNCED_REGISTRIES.len());
    }
}
