//! 창(인벤토리, 작업대, 상자) 패킷.

use super::world::{Item, write_slot};
use crate::protocol::DecodeError;
use crate::protocol::nbt::Tag;
use crate::protocol::types::read_u8;
use crate::protocol::varint::{read_varint, write_varint};
use crate::text::to_nbt;

// Clientbound
pub const CONTAINER_SET_CONTENT_ID: i32 = 0x12;
/// `Container Set Data`: 창 번호, 속성(i16), 값(i16). 화로의 불·진행 화살표.
pub const CONTAINER_SET_DATA_ID: i32 = 0x13;
pub const CONTAINER_SET_SLOT_ID: i32 = 0x14;
pub const OPEN_SCREEN_ID: i32 = 0x3C;
/// `Merchant Offers`: 주민 거래 창의 거래 목록.
pub const MERCHANT_OFFERS_ID: i32 = 0x35;

// Serverbound
pub const SB_CONTAINER_CLICK_ID: i32 = 0x12;
pub const SB_CONTAINER_CLOSE_ID: i32 = 0x13;
/// `Select Trade`: 거래 창 왼쪽 목록에서 고름 (번호 VarInt).
pub const SB_SELECT_TRADE_ID: i32 = 0x34;

/// 창 밖을 눌렀을 때의 칸 번호 (손에 든 것을 던짐).
pub const OUTSIDE: i16 = -999;
/// `Container Click` 모드 0 = 보통 클릭 (집기·놓기).
pub const MODE_PICKUP: i32 = 0;
/// Shift+클릭.
pub const MODE_QUICK_MOVE: i32 = 1;
/// `Container Click` 모드 4 = 던지기 (Q).
pub const MODE_THROW: i32 = 4;

pub fn encode_open_screen(window: i32, menu: i32, title: &Tag) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, window);
    write_varint(&mut out, menu);
    out.extend_from_slice(&to_nbt(title));
    out
}

/// 거래 하나 (창에 보이는 대로).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MerchantOffer {
    /// (아이템 ID, 개수)
    pub wants: (i32, i32),
    pub extra: Option<(i32, i32)>,
    pub gives: Item,
    pub disabled: bool,
    pub uses: i32,
    pub max_uses: i32,
    pub xp: i32,
    pub price_multiplier: f32,
}

/// `ItemCost`: 아이템 ID, 개수, 꼭 맞아야 하는 컴포넌트 목록(없음).
fn write_item_cost(out: &mut Vec<u8>, (id, count): (i32, i32)) {
    write_varint(out, id);
    write_varint(out, count);
    write_varint(out, 0);
}

pub fn encode_merchant_offers(
    window: i32,
    offers: &[MerchantOffer],
    level: i32,
    xp: i32,
    regular_villager: bool,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, window);
    write_varint(&mut out, offers.len() as i32);
    for o in offers {
        write_item_cost(&mut out, o.wants);
        write_slot(&mut out, Some(o.gives));
        match o.extra {
            Some(e) => {
                out.push(1);
                write_item_cost(&mut out, e);
            }
            None => out.push(0),
        }
        out.push(o.disabled as u8);
        out.extend_from_slice(&o.uses.to_be_bytes());
        out.extend_from_slice(&o.max_uses.to_be_bytes());
        out.extend_from_slice(&o.xp.to_be_bytes());
        out.extend_from_slice(&0i32.to_be_bytes()); // 특별 가격 (평판 없음)
        out.extend_from_slice(&o.price_multiplier.to_be_bytes());
        out.extend_from_slice(&0i32.to_be_bytes()); // 수요
    }
    write_varint(&mut out, level);
    write_varint(&mut out, xp);
    out.push(regular_villager as u8);
    out.push(1); // 다시 채울 수 있음
    out
}

pub fn decode_select_trade(mut buf: &[u8]) -> Result<i32, DecodeError> {
    read_varint(&mut buf)
}

pub fn encode_set_content(
    window: i32,
    state_id: i32,
    items: &[Option<Item>],
    carried: Option<Item>,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, window);
    write_varint(&mut out, state_id);
    write_varint(&mut out, items.len() as i32);
    for &i in items {
        write_slot(&mut out, i);
    }
    write_slot(&mut out, carried);
    out
}

pub fn encode_set_slot(window: i32, state_id: i32, slot: i16, item: Option<Item>) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, window);
    write_varint(&mut out, state_id);
    out.extend_from_slice(&slot.to_be_bytes());
    write_slot(&mut out, item);
    out
}

/// 클라이언트가 클릭한 결과라고 알려 온 것. 아이템은 "해시된 슬롯"으로 오는데, 컴포넌트
/// 해시는 쓰지 않고 아이템 ID와 개수만 봅니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Click {
    pub window: i32,
    pub slot: i16,
    pub button: i8,
    pub mode: i32,
    pub changed: Vec<(i16, Option<Hashed>)>,
    pub carried: Option<Hashed>,
}

/// 해시된 슬롯: 아이템 ID와 개수, 그리고 컴포넌트가 붙었다면 `custom_data`의 해시.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hashed {
    pub id: i32,
    pub count: i32,
    /// 컴포넌트가 하나라도 붙어 있음 (바닐라 기본값과 다름).
    pub has_components: bool,
    pub custom_data_hash: Option<i32>,
}

impl Hashed {
    pub fn plain(self) -> Item {
        Item::new(self.id, self.count)
    }
}

fn read_i16(buf: &mut &[u8]) -> Result<i16, DecodeError> {
    Ok(i16::from_be_bytes([read_u8(buf)?, read_u8(buf)?]))
}

/// Optional HashedStack: 있음 여부, 아이템 ID, 개수, 추가 컴포넌트(종류, i32 해시)들, 뺀 컴포넌트들.
fn read_hashed_slot(buf: &mut &[u8]) -> Result<Option<Hashed>, DecodeError> {
    if read_u8(buf)? == 0 {
        return Ok(None);
    }
    let id = read_varint(buf)?;
    let count = read_varint(buf)?;
    let added = read_varint(buf)?;
    if !(0..=256).contains(&added) {
        return Err(DecodeError::InvalidValue("component count"));
    }
    let mut custom_data_hash = None;
    for _ in 0..added {
        let kind = read_varint(buf)?;
        if buf.len() < 4 {
            return Err(DecodeError::UnexpectedEof);
        }
        let hash = i32::from_be_bytes(buf[..4].try_into().unwrap());
        if kind == crate::data::component::CUSTOM_DATA {
            custom_data_hash = Some(hash);
        }
        *buf = &buf[4..];
    }
    let removed = read_varint(buf)?;
    if !(0..=256).contains(&removed) {
        return Err(DecodeError::InvalidValue("component count"));
    }
    for _ in 0..removed {
        read_varint(buf)?;
    }
    Ok((count > 0).then_some(Hashed {
        id,
        count,
        has_components: added + removed > 0,
        custom_data_hash,
    }))
}

pub fn decode_click(mut buf: &[u8]) -> Result<Click, DecodeError> {
    let window = read_varint(&mut buf)?;
    read_varint(&mut buf)?; // state id — 아래 바뀐 칸을 그대로 받아들이므로 쓰지 않습니다
    let slot = read_i16(&mut buf)?;
    let button = read_u8(&mut buf)? as i8;
    let mode = read_varint(&mut buf)?;
    let n = read_varint(&mut buf)?;
    if !(0..=128).contains(&n) {
        return Err(DecodeError::InvalidValue("changed slot count"));
    }
    let mut changed = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let s = read_i16(&mut buf)?;
        changed.push((s, read_hashed_slot(&mut buf)?));
    }
    let carried = read_hashed_slot(&mut buf)?;
    Ok(Click {
        window,
        slot,
        button,
        mode,
        changed,
        carried,
    })
}

pub fn decode_close(mut buf: &[u8]) -> Result<i32, DecodeError> {
    read_varint(&mut buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_click_with_hashed_slots() {
        let mut b = Vec::new();
        write_varint(&mut b, 0); // window
        write_varint(&mut b, 5); // state
        b.extend_from_slice(&36i16.to_be_bytes());
        b.push(0); // button
        write_varint(&mut b, 0); // mode
        write_varint(&mut b, 1);
        b.extend_from_slice(&36i16.to_be_bytes());
        b.push(0); // 빈 칸
        // 손에: 돌 10개, 컴포넌트 해시 하나
        b.push(1);
        write_varint(&mut b, 1);
        write_varint(&mut b, 10);
        write_varint(&mut b, 1);
        write_varint(&mut b, 3);
        b.extend_from_slice(&0x1234_5678i32.to_be_bytes());
        write_varint(&mut b, 0);
        let c = decode_click(&b).unwrap();
        assert_eq!(c.changed, vec![(36, None)]);
        assert_eq!(
            c.carried,
            Some(Hashed {
                id: 1,
                count: 10,
                has_components: true,
                custom_data_hash: None
            })
        );
    }
}
