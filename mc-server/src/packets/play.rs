//! Play 상태에서 쓰는 패킷만.

use crate::protocol::DecodeError;
use crate::protocol::nbt::Tag;
use crate::protocol::types::{
    read_f64, read_i64, read_string, write_bool, write_f32, write_f64, write_i32, write_i64,
    write_string,
};
use crate::protocol::varint::{read_varint, write_varint};
use crate::text::to_nbt;
use crate::world::Dimension;

// Clientbound — ViaVersion `ClientboundPackets26_3`.
pub const CHUNK_BATCH_FINISHED_ID: i32 = 0x0B;
pub const CHUNK_BATCH_START_ID: i32 = 0x0C;
pub const COMMANDS_ID: i32 = 0x10;
pub const DISCONNECT_ID: i32 = 0x20;
pub const FORGET_LEVEL_CHUNK_ID: i32 = 0x26;
pub const GAME_EVENT_ID: i32 = 0x27;
pub const KEEP_ALIVE_ID: i32 = 0x2D;
pub const LEVEL_CHUNK_WITH_LIGHT_ID: i32 = 0x2E;
pub const LOGIN_ID: i32 = 0x32;
pub const PLAYER_ABILITIES_ID: i32 = 0x41;
pub const PLAYER_POSITION_ID: i32 = 0x49;
pub const RESPAWN_ID: i32 = 0x54;
pub const SET_CHUNK_CACHE_CENTER_ID: i32 = 0x60;
pub const SET_DEFAULT_SPAWN_POSITION_ID: i32 = 0x63;
pub const SYSTEM_CHAT_ID: i32 = 0x7C;

// Serverbound — ViaVersion `ServerboundPackets26_3`.
pub const SB_ACCEPT_TELEPORTATION_ID: i32 = 0x00;
pub const SB_CHAT_COMMAND_ID: i32 = 0x07;
pub const SB_CHAT_ID: i32 = 0x09;
pub const SB_KEEP_ALIVE_ID: i32 = 0x1C;
pub const SB_MOVE_PLAYER_POS_ID: i32 = 0x1E;
pub const SB_MOVE_PLAYER_POS_ROT_ID: i32 = 0x1F;
pub const SB_PLAYER_LOADED_ID: i32 = 0x2C;

/// `Game Event`의 "청크 기다리기 시작". 이걸 받아야 클라이언트가 로딩 화면을 끝낼 준비를 합니다.
pub const GAME_EVENT_LEVEL_CHUNKS_LOAD_START: u8 = 13;

pub const GAMEMODE_CREATIVE: i32 = 1;

/// Login과 Respawn이 함께 쓰는 "어느 월드에 어떤 모드로" 부분 (`CommonPlayerSpawnInfo`).
#[derive(Debug, Clone, Copy)]
pub struct SpawnInfo {
    pub dimension: Dimension,
    pub hashed_seed: i64,
    pub gamemode: i32,
}

impl SpawnInfo {
    fn write(&self, out: &mut Vec<u8>) {
        write_varint(out, self.dimension.type_id());
        write_string(out, self.dimension.level_name());
        write_i64(out, self.hashed_seed);
        // 26.3: 게임 모드가 byte → VarInt, 이전 게임 모드가 byte(-1 = 없음) → 압축형
        // Optional VarInt(0 = 없음, 그 외 값+1)로 바뀌었습니다.
        write_varint(out, self.gamemode);
        write_varint(out, 0);
        write_bool(out, false); // debug 월드
        write_bool(out, false); // flat 월드 (true면 지평선이 y=0 근처로 내려감)
        write_bool(out, false); // 마지막 사망 위치 없음
        write_varint(out, 0); // 포털 쿨다운
        write_varint(out, self.dimension.sea_level());
    }
}

pub struct Login {
    pub entity_id: i32,
    pub view_distance: i32,
    pub spawn: SpawnInfo,
}

impl Login {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_i32(&mut out, self.entity_id);
        write_bool(&mut out, false); // hardcore
        write_varint(&mut out, Dimension::ALL.len() as i32);
        for d in Dimension::ALL {
            write_string(&mut out, d.level_name());
        }
        write_varint(&mut out, 100); // max players (클라이언트는 무시)
        write_varint(&mut out, self.view_distance);
        write_varint(&mut out, self.view_distance); // simulation distance
        write_bool(&mut out, false); // reduced debug info
        write_bool(&mut out, true); // respawn screen
        write_bool(&mut out, false); // limited crafting
        self.spawn.write(&mut out);
        write_bool(&mut out, false); // 26.2: 암호화된(정품) 연결인가 — 오프라인이므로 false
        write_bool(&mut out, false); // enforces secure chat
        out
    }
}

pub fn encode_respawn(spawn: &SpawnInfo) -> Vec<u8> {
    let mut out = Vec::new();
    spawn.write(&mut out);
    out.push(0); // 유지할 데이터 없음
    out
}

/// `Player Position` (서버 → 클라이언트 텔레포트). 절대 좌표, 속도 0.
pub fn encode_player_position(
    teleport_id: i32,
    pos: (f64, f64, f64),
    yaw: f32,
    pitch: f32,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, teleport_id);
    write_f64(&mut out, pos.0);
    write_f64(&mut out, pos.1);
    write_f64(&mut out, pos.2);
    for _ in 0..3 {
        write_f64(&mut out, 0.0);
    }
    write_f32(&mut out, yaw);
    write_f32(&mut out, pitch);
    out.extend_from_slice(&0u32.to_be_bytes()); // 상대 좌표 플래그 없음
    out
}

/// 무적 + 비행 가능 + 즉시 부수기(크리에이티브).
pub fn encode_creative_abilities() -> Vec<u8> {
    let mut out = vec![0x01 | 0x04 | 0x08];
    write_f32(&mut out, 0.05);
    write_f32(&mut out, 0.1);
    out
}

pub fn encode_game_event(event: u8, value: f32) -> Vec<u8> {
    let mut out = vec![event];
    write_f32(&mut out, value);
    out
}

pub fn encode_chunk_xz_varint(cx: i32, cz: i32) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, cx);
    write_varint(&mut out, cz);
    out
}

/// `Forget Level Chunk`: 이 패킷만 z가 먼저입니다 (청크 좌표를 long 하나로 묶은 결과).
pub fn encode_forget_chunk(cx: i32, cz: i32) -> Vec<u8> {
    let mut out = Vec::new();
    write_i32(&mut out, cz);
    write_i32(&mut out, cx);
    out
}

pub fn encode_batch_finished(count: i32) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, count);
    out
}

/// 블록 좌표를 long 하나로: x 26비트 | z 26비트 | y 12비트.
pub fn pack_position(x: i32, y: i32, z: i32) -> i64 {
    ((x as i64 & 0x3FF_FFFF) << 38) | ((z as i64 & 0x3FF_FFFF) << 12) | (y as i64 & 0xFFF)
}

pub fn encode_spawn_position(dimension: Dimension, pos: (i32, i32, i32)) -> Vec<u8> {
    let mut out = Vec::new();
    write_string(&mut out, dimension.level_name());
    write_i64(&mut out, pack_position(pos.0, pos.1, pos.2));
    write_f32(&mut out, 0.0); // yaw
    write_f32(&mut out, 0.0); // pitch
    out
}

pub fn encode_keep_alive(id: i64) -> Vec<u8> {
    id.to_be_bytes().to_vec()
}

pub fn encode_system_chat(content: &Tag) -> Vec<u8> {
    let mut out = to_nbt(content);
    write_bool(&mut out, false); // 채팅창 (액션 바 아님)
    out
}

pub fn encode_disconnect(reason: &Tag) -> Vec<u8> {
    to_nbt(reason)
}

/// 인자 없는 명령어들만 선언합니다. 선언하지 않은 명령어는 클라이언트가 빨갛게 표시합니다.
///
/// 노드 플래그: 하위 2비트 = 종류(0 루트, 1 리터럴, 2 인자), 0x04 = 실행 가능.
pub fn encode_commands(literals: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, literals.len() as i32 + 1);
    // 0번: 루트
    out.push(0x00);
    write_varint(&mut out, literals.len() as i32);
    for i in 0..literals.len() {
        write_varint(&mut out, i as i32 + 1);
    }
    for name in literals {
        out.push(0x01 | 0x04);
        write_varint(&mut out, 0); // 자식 없음
        write_string(&mut out, name);
    }
    write_varint(&mut out, 0); // 루트 인덱스
    out
}

pub fn decode_keep_alive(mut buf: &[u8]) -> Result<i64, DecodeError> {
    read_i64(&mut buf)
}

/// 이동 패킷(`Move Player Pos`, `Pos Rot`)의 앞머리 좌표.
pub fn decode_position(mut buf: &[u8]) -> Result<(f64, f64, f64), DecodeError> {
    Ok((
        read_f64(&mut buf)?,
        read_f64(&mut buf)?,
        read_f64(&mut buf)?,
    ))
}

/// `Accept Teleportation`. 26.3부터 클라이언트가 도착한 좌표·시선을 함께 보냅니다
/// (그 대신 수락 직후의 이동 패킷은 더 보내지 않습니다).
pub fn decode_accept_teleportation(mut buf: &[u8]) -> Result<(i32, (f64, f64, f64)), DecodeError> {
    let id = read_varint(&mut buf)?;
    let pos = (
        read_f64(&mut buf)?,
        read_f64(&mut buf)?,
        read_f64(&mut buf)?,
    );
    Ok((id, pos))
}

/// `Chat Command` / `Chat`: 앞의 문자열만 읽습니다. 서명 등 나머지는 쓰지 않습니다.
pub fn decode_leading_string(mut buf: &[u8]) -> Result<String, DecodeError> {
    read_string(&mut buf, 256)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_packing() {
        // 위키 예시: (18357644, 831, -20882616)
        assert_eq!(
            pack_position(18357644, 831, -20882616) as u64,
            0b0100_0110_0000_0111_0110_0011_0010_1100_0001_0101_1011_0100_1000_0011_0011_1111
        );
    }

    #[test]
    fn spawn_info_layout() {
        let mut out = Vec::new();
        SpawnInfo {
            dimension: Dimension::Overworld,
            hashed_seed: 0,
            gamemode: 1,
        }
        .write(&mut out);
        // type id(1) + 문자열(1+19) + seed(8) + gamemode(1) + prev(1) + bool×3 + cooldown(1) + sea(1)
        assert_eq!(out.len(), 1 + 20 + 8 + 1 + 1 + 3 + 1 + 1);
        assert_eq!(out[1 + 20 + 8], 1); // 크리에이티브
        assert_eq!(out[1 + 20 + 8 + 1], 0); // 이전 모드 없음
    }
}
