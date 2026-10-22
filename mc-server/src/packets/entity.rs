//! 엔티티(아이템, 몹)와 체력·피해에 관한 Play 패킷.

use super::world::{Item, write_slot};
use crate::protocol::DecodeError;
use crate::protocol::nbt::Tag;
use crate::protocol::types::{write_bool, write_f32, write_f64, write_i32, write_uuid};
use crate::protocol::varint::{read_varint, write_varint};
use crate::text::to_nbt;

// Clientbound
pub const DAMAGE_EVENT_ID: i32 = 0x19;
pub const ENTITY_EVENT_ID: i32 = 0x22;
pub const HURT_ANIMATION_ID: i32 = 0x2B;
pub const PLAYER_COMBAT_KILL_ID: i32 = 0x45;
pub const SET_ENTITY_DATA_ID: i32 = 0x65;
pub const SET_ENTITY_MOTION_ID: i32 = 0x67;
pub const SET_HEALTH_ID: i32 = 0x6A;
pub const TAKE_ITEM_ENTITY_ID: i32 = 0x7F;

// Serverbound
pub const SB_ATTACK_ID: i32 = 0x01;
pub const SB_CLIENT_COMMAND_ID: i32 = 0x0C;
pub const SB_INTERACT_ID: i32 = 0x1A;
pub const SB_PLAYER_COMMAND_ID: i32 = 0x2A;
pub const SB_USE_ITEM_ID: i32 = 0x43;

/// `Entity Event` 번호.
pub const ENTITY_EVENT_DEATH: i8 = 3;
pub const ENTITY_EVENT_USE_ITEM_FINISHED: i8 = 9;

/// `Client Command`의 "부활".
pub const CLIENT_COMMAND_RESPAWN: i32 = 0;
/// `Player Command`의 달리기 시작·멈춤.
pub const PLAYER_COMMAND_START_SPRINT: i32 = 1;
pub const PLAYER_COMMAND_STOP_SPRINT: i32 = 2;

/// 1/256 회전 단위.
fn angle(degrees: f32) -> u8 {
    ((degrees / 360.0 * 256.0).rem_euclid(256.0)) as u8
}

/// LpVec3 (1.21.9~ 속도 표기): 3비트(배율 2비트 + 이어짐 1비트) + 15비트씩 x·y·z, 모두 48비트.
/// 가장 큰 성분을 올림한 정수가 배율이고, 각 성분은 배율로 나눈 [-1, 1] 값을 15비트로.
pub fn write_lp_vec3(out: &mut Vec<u8>, v: (f64, f64, f64)) {
    const MAX_Q: f64 = (i16::MAX - 1) as f64;
    let clean = |x: f64| {
        if x.is_nan() {
            0.0
        } else {
            x.clamp(-1.7e10, 1.7e10)
        }
    };
    let (x, y, z) = (clean(v.0), clean(v.1), clean(v.2));
    let max = x.abs().max(y.abs()).max(z.abs());
    if max < 1.0 / MAX_Q {
        out.push(0);
        return;
    }
    let scale = max.ceil() as u64;
    let big = scale & 3 != scale;
    let scale_bits = if big { (scale & 3) | 4 } else { scale };
    let pack = |c: f64| ((c / scale as f64 * 0.5 + 0.5) * MAX_Q).round() as u64;
    let packed = scale_bits | pack(x) << 3 | pack(y) << 18 | pack(z) << 33;
    out.push(packed as u8);
    out.push((packed >> 8) as u8);
    out.extend_from_slice(&((packed >> 16) as u32).to_be_bytes());
    if big {
        write_varint(out, (scale >> 2) as i32);
    }
}

/// `Add Entity` (속도 포함).
pub fn encode_add_entity(
    id: i32,
    uuid: u128,
    kind: i32,
    pos: (f64, f64, f64),
    vel: (f64, f64, f64),
    yaw: f32,
    pitch: f32,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, id);
    write_uuid(&mut out, uuid);
    write_varint(&mut out, kind);
    write_f64(&mut out, pos.0);
    write_f64(&mut out, pos.1);
    write_f64(&mut out, pos.2);
    write_lp_vec3(&mut out, vel);
    out.push(angle(pitch));
    out.push(angle(yaw));
    out.push(angle(yaw));
    write_varint(&mut out, 0);
    out
}

/// 엔티티 데이터 타입 번호 (26.3, ViaVersion `EntityDataTypes26_3`).
const DATA_ITEM: i32 = 7;
/// `ItemEntity`의 아이템은 기본 엔티티 데이터 8개(0..7) 다음인 8번.
const ITEM_ENTITY_STACK_INDEX: u8 = 8;

/// 엔티티 데이터 종류: byte, pose, 팔.
const DATA_BYTE: i32 = 0;
const DATA_POSE: i32 = 20;
const DATA_HUMANOID_ARM: i32 = 42;
/// 플레이어(`Avatar`) 데이터 위치: 0 상태 플래그, 6 자세, 15 주 손, 16 겉옷 레이어.
const FLAGS_INDEX: u8 = 0;
const POSE_INDEX: u8 = 6;
const MAIN_HAND_INDEX: u8 = 15;
const SKIN_PARTS_INDEX: u8 = 16;
const POSE_STANDING: i32 = 0;
const POSE_CROUCHING: i32 = 5;

/// 다른 사람에게 보이는 플레이어 모습: 웅크림·달리기, 겉옷 레이어, 주 손.
pub fn encode_player_data(
    id: i32,
    sneaking: bool,
    sprinting: bool,
    skin_parts: u8,
    main_arm: i32,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, id);
    out.push(FLAGS_INDEX);
    write_varint(&mut out, DATA_BYTE);
    out.push(if sneaking { 0x02 } else { 0 } | if sprinting { 0x08 } else { 0 });
    out.push(POSE_INDEX);
    write_varint(&mut out, DATA_POSE);
    write_varint(
        &mut out,
        if sneaking {
            POSE_CROUCHING
        } else {
            POSE_STANDING
        },
    );
    out.push(MAIN_HAND_INDEX);
    write_varint(&mut out, DATA_HUMANOID_ARM);
    write_varint(&mut out, main_arm);
    out.push(SKIN_PARTS_INDEX);
    write_varint(&mut out, DATA_BYTE);
    out.push(skin_parts);
    out.push(0xFF);
    out
}

/// 몹 엔티티 데이터 한 칸 (번호는 26.3 클래스 계층 순서: 0..8 `Entity`, 8..15 `LivingEntity`,
/// 15 `Mob` 플래그, 그다음 종류별).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Meta {
    /// 이름표 (선택적 텍스트 컴포넌트, 2번 칸).
    Name(u8, String),
    Byte(u8, u8),
    Int(u8, i32),
    Bool(u8, bool),
    /// 주민 종류·직업 (레지스트리 번호)과 레벨.
    Villager(u8, i32, i32, i32),
}

const DATA_INT: i32 = 1;
const DATA_OPTIONAL_COMPONENT: i32 = 6;
const DATA_BOOL: i32 = 8;
const DATA_VILLAGER: i32 = 18;

pub fn encode_entity_data(id: i32, meta: &[Meta]) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, id);
    for m in meta {
        match m.clone() {
            Meta::Name(i, name) => {
                out.push(i);
                write_varint(&mut out, DATA_OPTIONAL_COMPONENT);
                out.push(1);
                out.extend_from_slice(&crate::text::to_nbt(&crate::text::colored(&name, "gold")));
            }
            Meta::Byte(i, v) => {
                out.push(i);
                write_varint(&mut out, DATA_BYTE);
                out.push(v);
            }
            Meta::Int(i, v) => {
                out.push(i);
                write_varint(&mut out, DATA_INT);
                write_varint(&mut out, v);
            }
            Meta::Bool(i, v) => {
                out.push(i);
                write_varint(&mut out, DATA_BOOL);
                write_bool(&mut out, v);
            }
            Meta::Villager(i, kind, profession, level) => {
                out.push(i);
                write_varint(&mut out, DATA_VILLAGER);
                write_varint(&mut out, kind);
                write_varint(&mut out, profession);
                write_varint(&mut out, level);
            }
        }
    }
    out.push(0xFF);
    out
}

/// 떨어진 아이템 엔티티가 무엇인지.
pub fn encode_item_entity_data(id: i32, item: Item) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, id);
    out.push(ITEM_ENTITY_STACK_INDEX);
    write_varint(&mut out, DATA_ITEM);
    write_slot(&mut out, Some(item));
    out.push(0xFF); // 끝
    out
}

pub fn encode_motion(id: i32, vel: (f64, f64, f64)) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, id);
    write_lp_vec3(&mut out, vel);
    out
}

pub fn encode_take_item(item_entity: i32, collector: i32, count: i32) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, item_entity);
    write_varint(&mut out, collector);
    write_varint(&mut out, count);
    out
}

pub fn encode_set_health(health: f32, food: i32, saturation: f32) -> Vec<u8> {
    let mut out = Vec::new();
    write_f32(&mut out, health);
    write_varint(&mut out, food);
    write_f32(&mut out, saturation);
    out
}

/// `Damage Event`: 맞은 엔티티, 피해 종류(`damage_type` ID), 원인 엔티티(ID+1, 0 = 없음) 두 개.
pub fn encode_damage_event(id: i32, damage_type: i32, attacker: Option<i32>) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, id);
    write_varint(&mut out, damage_type);
    let a = attacker.map_or(0, |a| a + 1);
    write_varint(&mut out, a);
    write_varint(&mut out, a);
    write_bool(&mut out, false); // 위치 없음
    out
}

pub fn encode_entity_event(id: i32, event: i8) -> Vec<u8> {
    let mut out = Vec::new();
    write_i32(&mut out, id);
    out.push(event as u8);
    out
}

/// 사망 화면.
pub fn encode_combat_kill(player_entity: i32, message: &Tag) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, player_entity);
    out.extend_from_slice(&to_nbt(message));
    out
}

pub fn decode_entity_id(mut buf: &[u8]) -> Result<i32, DecodeError> {
    read_varint(&mut buf)
}

/// `Player Command`: (엔티티, 동작).
/// `Interact`: (대상, 손, 웅크림). 맞은 자리(LpVec3)는 쓰지 않습니다.
pub fn decode_interact(mut buf: &[u8]) -> Result<(i32, i32, bool), DecodeError> {
    let target = read_varint(&mut buf)?;
    let hand = read_varint(&mut buf)?;
    let sneaking = buf.last().is_some_and(|&b| b == 1);
    Ok((target, hand, sneaking))
}

pub fn decode_player_command(mut buf: &[u8]) -> Result<i32, DecodeError> {
    read_varint(&mut buf)?;
    read_varint(&mut buf)
}

/// `Use Item`: (보조 손인가, 순번).
pub fn decode_use_item(mut buf: &[u8]) -> Result<(bool, i32), DecodeError> {
    let hand = read_varint(&mut buf)?;
    Ok((hand == 1, read_varint(&mut buf)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ViaVersion `LowPrecisionVectorType.read`를 옮긴 것 — 쓴 값을 다시 읽어 봅니다.
    fn read_lp_vec3(b: &[u8]) -> (f64, f64, f64) {
        if b[0] == 0 {
            return (0.0, 0.0, 0.0);
        }
        let packed = (u32::from_be_bytes(b[2..6].try_into().unwrap()) as u64) << 16
            | (b[1] as u64) << 8
            | b[0] as u64;
        let mut scale = packed & 3;
        if b[0] & 4 != 0 {
            let mut rest = &b[6..];
            scale |= (read_varint(&mut rest).unwrap() as u64) << 2;
        }
        let un = |v: u64| ((v & 0x7FFF).min(32766) as f64 * 2.0) / 32766.0 - 1.0;
        (
            un(packed >> 3) * scale as f64,
            un(packed >> 18) * scale as f64,
            un(packed >> 33) * scale as f64,
        )
    }

    #[test]
    fn lp_vec3_round_trip() {
        for v in [
            (0.0, 0.0, 0.0),
            (0.1, -0.4, 0.25),
            (1.0, 2.0, -3.0),
            (0.0, 0.42, 0.0),
            (9.5, 0.0, -1.0),
        ] {
            let mut out = Vec::new();
            write_lp_vec3(&mut out, v);
            let r = read_lp_vec3(&out);
            for (a, b) in [(v.0, r.0), (v.1, r.1), (v.2, r.2)] {
                assert!((a - b).abs() < 1e-3 * a.abs().max(1.0), "{v:?} -> {r:?}");
            }
        }
        let mut out = Vec::new();
        write_lp_vec3(&mut out, (0.0, 0.0, 0.0));
        assert_eq!(out, [0]);
    }
}
