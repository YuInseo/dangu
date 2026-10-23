//! Play 상태 중 블록·아이템·손에 관한 패킷.

use crate::protocol::DecodeError;
use crate::protocol::types::{
    read_bool, read_f32, read_i64, read_u8, write_bool, write_i32, write_i64,
};
use crate::protocol::varint::{read_varint, write_varint};

// Clientbound — ViaVersion `ClientboundPackets26_3`.
pub const BLOCK_CHANGED_ACK_ID: i32 = 0x04;
pub const BLOCK_UPDATE_ID: i32 = 0x08;
pub const LEVEL_EVENT_ID: i32 = 0x2F;
pub const SET_EQUIPMENT_ID: i32 = 0x68;
pub const SET_HELD_SLOT_ID: i32 = 0x6B;
pub const SET_PLAYER_INVENTORY_ID: i32 = 0x6E;
pub const SWING_ANIMATION_ID: i32 = 0x7B;

// Serverbound — ViaVersion `ServerboundPackets26_3`.
pub const SB_PICK_ITEM_FROM_BLOCK_ID: i32 = 0x24;
pub const SB_PICK_ITEM_FROM_ENTITY_ID: i32 = 0x25;
pub const SB_PLAYER_ACTION_ID: i32 = 0x29;
/// 26.3: 휘두르기(왼클릭·채굴 중 매 틱). 내용 없음.
pub const SB_PUNCH_ID: i32 = 0x2E;
pub const SB_SET_CARRIED_ITEM_ID: i32 = 0x36;
pub const SB_SET_CREATIVE_MODE_SLOT_ID: i32 = 0x39;
pub const SB_USE_ITEM_ON_ID: i32 = 0x42;

/// `Level Event`: 블록 부서지는 소리와 조각 (data = 블록 상태).
pub const LEVEL_EVENT_BLOCK_DESTROYED: i32 = 2001;

pub type BlockPos = (i32, i32, i32);

pub fn pack_position((x, y, z): BlockPos) -> i64 {
    ((x as i64 & 0x3FF_FFFF) << 38) | ((z as i64 & 0x3FF_FFFF) << 12) | (y as i64 & 0xFFF)
}

/// x 26비트 | z 26비트 | y 12비트, 모두 부호 있음.
pub fn unpack_position(v: i64) -> BlockPos {
    let x = (v >> 38) as i32;
    let z = ((v << 26) >> 38) as i32;
    let y = ((v << 52) >> 52) as i32;
    (x, y, z)
}

fn read_position(buf: &mut &[u8]) -> Result<BlockPos, DecodeError> {
    Ok(unpack_position(read_i64(buf)?))
}

/// 블록의 면. `Direction` 순서: 아래, 위, 북(-z), 남(+z), 서(-x), 동(+x).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Face {
    pub fn from_id(id: i32) -> Result<Face, DecodeError> {
        Ok(match id {
            0 => Face::Down,
            1 => Face::Up,
            2 => Face::North,
            3 => Face::South,
            4 => Face::West,
            5 => Face::East,
            _ => return Err(DecodeError::InvalidValue("block face")),
        })
    }

    pub fn offset(self) -> BlockPos {
        match self {
            Face::Down => (0, -1, 0),
            Face::Up => (0, 1, 0),
            Face::North => (0, 0, -1),
            Face::South => (0, 0, 1),
            Face::West => (-1, 0, 0),
            Face::East => (1, 0, 0),
        }
    }

    pub fn opposite(self) -> Face {
        match self {
            Face::Down => Face::Up,
            Face::Up => Face::Down,
            Face::North => Face::South,
            Face::South => Face::North,
            Face::West => Face::East,
            Face::East => Face::West,
        }
    }

    /// 블록 상태의 `facing` 값 이름.
    pub fn name(self) -> &'static str {
        match self {
            Face::Down => "down",
            Face::Up => "up",
            Face::North => "north",
            Face::South => "south",
            Face::West => "west",
            Face::East => "east",
        }
    }

    pub fn axis(self) -> &'static str {
        match self {
            Face::Down | Face::Up => "y",
            Face::North | Face::South => "z",
            Face::West | Face::East => "x",
        }
    }

    pub fn is_horizontal(self) -> bool {
        !matches!(self, Face::Down | Face::Up)
    }

    /// 플레이어가 바라보는 수평 방향 (yaw 0 = 남쪽, 90 = 서쪽).
    pub fn from_yaw(yaw: f32) -> Face {
        match ((yaw.rem_euclid(360.0) + 45.0) / 90.0) as i32 % 4 {
            0 => Face::South,
            1 => Face::West,
            2 => Face::North,
            _ => Face::East,
        }
    }

    /// 위아래까지 포함한 바라보는 방향.
    pub fn from_look(yaw: f32, pitch: f32) -> Face {
        if pitch > 45.0 {
            Face::Down
        } else if pitch < -45.0 {
            Face::Up
        } else {
            Face::from_yaw(yaw)
        }
    }
}

pub fn add(p: BlockPos, d: BlockPos) -> BlockPos {
    (p.0 + d.0, p.1 + d.1, p.2 + d.2)
}

/// `Player Action`. 26.3에 "부수는 방향 바꾸기"(1)가 끼어들어 그 뒤 번호가 하나씩 밀렸습니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerAction {
    StartDestroy,
    ChangeDestroyDirection,
    AbortDestroy,
    StopDestroy,
    /// 아이템 버리기, 손 바꾸기 등 — 아직 쓰지 않음.
    Other(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerActionPacket {
    pub action: PlayerAction,
    pub pos: BlockPos,
    pub sequence: i32,
}

pub fn decode_player_action(mut buf: &[u8]) -> Result<PlayerActionPacket, DecodeError> {
    let action = match read_varint(&mut buf)? {
        0 => PlayerAction::StartDestroy,
        1 => PlayerAction::ChangeDestroyDirection,
        2 => PlayerAction::AbortDestroy,
        3 => PlayerAction::StopDestroy,
        n => PlayerAction::Other(n),
    };
    let pos = read_position(&mut buf)?;
    read_u8(&mut buf)?; // 면
    // 방향 바꾸기에는 순번이 없습니다.
    let sequence = if buf.is_empty() {
        0
    } else {
        read_varint(&mut buf)?
    };
    Ok(PlayerActionPacket {
        action,
        pos,
        sequence,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UseItemOn {
    pub off_hand: bool,
    pub pos: BlockPos,
    pub face: Face,
    /// 누른 자리의 블록 안 좌표 (0..1).
    pub cursor: (f32, f32, f32),
    pub sequence: i32,
}

pub fn decode_use_item_on(mut buf: &[u8]) -> Result<UseItemOn, DecodeError> {
    let off_hand = read_varint(&mut buf)? == 1;
    let pos = read_position(&mut buf)?;
    let face = Face::from_id(read_varint(&mut buf)?)?;
    let cursor = (
        read_f32(&mut buf)?,
        read_f32(&mut buf)?,
        read_f32(&mut buf)?,
    );
    read_bool(&mut buf)?; // 블록 안에서 눌렀나
    read_bool(&mut buf)?; // 월드 경계
    let sequence = read_varint(&mut buf)?;
    Ok(UseItemOn {
        off_hand,
        pos,
        face,
        cursor,
        sequence,
    })
}

/// 아이템 한 묶음. 바닐라 아이템이면 컴포넌트가 없고, 우리만의 아이템이면 `custom`이
/// [`crate::game::custom_items`]의 번호를 가리켜 이름·모델·설명 등 컴포넌트가 붙습니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Item {
    pub id: i32,
    pub count: i32,
    pub custom: Option<u16>,
    /// 한 장 한 장 다른 아이템(복권 등)의 일련번호. 0이면 없음. 이름·설명은
    /// [`crate::game::custom_items::set_look`]로 번호마다 정합니다.
    pub serial: u32,
}

impl Item {
    pub fn new(id: i32, count: i32) -> Self {
        Self {
            id,
            count,
            custom: None,
            serial: 0,
        }
    }

    /// 개수만 바꾼 것.
    pub fn with_count(self, count: i32) -> Self {
        Self { count, ..self }
    }

    /// 같은 칸에 쌓일 수 있는가 (종류와 사용자 정의가 같음).
    pub fn stacks_with(&self, other: &Item) -> bool {
        self.id == other.id && self.custom == other.custom && self.serial == other.serial
    }
}

/// `Slot`: 개수 VarInt, 0이 아니면 아이템 ID와 컴포넌트 추가·제거 개수, 그리고 컴포넌트들.
pub fn write_slot(out: &mut Vec<u8>, item: Option<Item>) {
    match item {
        Some(item) if item.count > 0 => {
            write_varint(out, item.count);
            write_varint(out, item.id);
            let components = item
                .custom
                .map(|c| crate::game::custom_items::components_with(c, item.serial))
                .unwrap_or_default();
            write_varint(out, components.len() as i32);
            write_varint(out, 0);
            for (kind, data) in components {
                write_varint(out, kind);
                out.extend_from_slice(&data);
            }
        }
        _ => write_varint(out, 0),
    }
}

/// `Set Creative Mode Slot`: 슬롯 번호(인벤토리 메뉴 기준)와 아이템. 크리에이티브 창에서 온
/// 아이템은 컴포넌트가 길이 접두사와 함께 오므로(`UntrustedSlot`), 우리 아이템 표시
/// (`custom_data`)를 찾아 사용자 정의 아이템으로 되돌립니다.
pub fn decode_creative_slot(mut buf: &[u8]) -> Result<(i16, Option<Item>), DecodeError> {
    let slot = i16::from_be_bytes([read_u8(&mut buf)?, read_u8(&mut buf)?]);
    let count = read_varint(&mut buf)?;
    if count <= 0 {
        return Ok((slot, None));
    }
    let id = read_varint(&mut buf)?;
    let mut custom = None;
    let added = read_varint(&mut buf)?;
    if !(0..=256).contains(&added) {
        return Err(DecodeError::InvalidValue("component count"));
    }
    for _ in 0..added {
        let kind = read_varint(&mut buf)?;
        let len = read_varint(&mut buf)?;
        let len = usize::try_from(len)
            .ok()
            .filter(|&l| l <= buf.len())
            .ok_or(DecodeError::UnexpectedEof)?;
        let (data, rest) = buf.split_at(len);
        buf = rest;
        if kind == crate::data::component::CUSTOM_DATA {
            use crate::game::custom_items::{LEGACY_TAG_KEY, TAG_KEY};
            custom = crate::protocol::nbt::find_string(data, TAG_KEY)
                .or_else(|| crate::protocol::nbt::find_string(data, LEGACY_TAG_KEY))
                .and_then(|name| crate::game::custom_items::index_of(&name));
        }
    }
    Ok((
        slot,
        Some(Item {
            id,
            count,
            custom,
            serial: 0,
        }),
    ))
}

pub fn decode_carried_item(buf: &[u8]) -> Result<i16, DecodeError> {
    let b = buf.get(..2).ok_or(DecodeError::UnexpectedEof)?;
    Ok(i16::from_be_bytes([b[0], b[1]]))
}

pub fn decode_pick_item_from_block(mut buf: &[u8]) -> Result<BlockPos, DecodeError> {
    read_position(&mut buf)
}

pub fn encode_block_update(pos: BlockPos, state: u32) -> Vec<u8> {
    let mut out = Vec::new();
    write_i64(&mut out, pack_position(pos));
    write_varint(&mut out, state as i32);
    out
}

pub fn encode_block_changed_ack(sequence: i32) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, sequence);
    out
}

pub fn encode_level_event(event: i32, pos: BlockPos, data: i32) -> Vec<u8> {
    let mut out = Vec::new();
    write_i32(&mut out, event);
    write_i64(&mut out, pack_position(pos));
    write_i32(&mut out, data);
    write_bool(&mut out, false);
    out
}

/// `Set Equipment`: 주 손(0)과 보조 손(1). 이어지는 항목이 있으면 슬롯 바이트의 최상위 비트를 켭니다.
pub fn encode_hands(entity_id: i32, main: Option<Item>, off: Option<Item>) -> Vec<u8> {
    encode_equipment(entity_id, main, off, None)
}

/// `Set Equipment`: 두 손과 (있으면) 갑옷 넷 — 발(2)·다리(3)·몸(4)·머리(5).
pub fn encode_equipment(
    entity_id: i32,
    main: Option<Item>,
    off: Option<Item>,
    armor: Option<[Option<Item>; 4]>,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, entity_id);
    let mut entries = vec![(0u8, main), (1, off)];
    if let Some(a) = armor {
        entries.extend((2u8..).zip(a));
    }
    let last = entries.len() - 1;
    for (i, (slot, item)) in entries.into_iter().enumerate() {
        out.push(if i < last { slot | 0x80 } else { slot });
        write_slot(&mut out, item);
    }
    out
}

pub fn encode_held_slot(slot: i32) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, slot);
    out
}

/// `Set Player Inventory`: 인벤토리 번호(0..8 = 핫바)에 아이템을 넣습니다.
pub fn encode_player_inventory(slot: i32, item: Option<Item>) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, slot);
    write_slot(&mut out, item);
    out
}

/// 26.3 `Swing Animation`: 손, 동작 종류(1 = whack), 길이(틱).
pub fn encode_swing(entity_id: i32, off_hand: bool) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, entity_id);
    write_varint(&mut out, off_hand as i32);
    write_varint(&mut out, 1);
    write_varint(&mut out, 6);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_round_trip() {
        for p in [
            (0, 0, 0),
            (-1, -64, -1),
            (18357644, 831, -20882616),
            (-30000000, 319, 29999999),
        ] {
            assert_eq!(unpack_position(pack_position(p)), p);
        }
    }

    #[test]
    fn yaw_to_facing() {
        assert_eq!(Face::from_yaw(0.0), Face::South);
        assert_eq!(Face::from_yaw(90.0), Face::West);
        assert_eq!(Face::from_yaw(-90.0), Face::East);
        assert_eq!(Face::from_yaw(180.0), Face::North);
        assert_eq!(Face::from_yaw(44.0), Face::South);
    }

    #[test]
    fn decodes_use_item_on() {
        let mut body = Vec::new();
        write_varint(&mut body, 0);
        write_i64(&mut body, pack_position((5, 70, -3)));
        write_varint(&mut body, 1);
        for f in [0.5f32, 1.0, 0.25] {
            body.extend_from_slice(&f.to_be_bytes());
        }
        body.extend_from_slice(&[0, 0]);
        write_varint(&mut body, 42);
        let u = decode_use_item_on(&body).unwrap();
        assert_eq!(
            (u.pos, u.face, u.sequence, u.off_hand),
            ((5, 70, -3), Face::Up, 42, false)
        );
    }
}
