//! Play 상태에서 쓰는 패킷만.

use crate::data::registry_id;
use crate::protocol::DecodeError;
use crate::protocol::nbt::Tag;
use crate::protocol::types::{
    read_f32, read_f64, read_i64, read_string, write_bool, write_f32, write_f64, write_i32,
    write_i64, write_string, write_uuid,
};
use crate::protocol::varint::{read_varint, write_varint};
use crate::text::to_nbt;
use crate::world::Dimension;

// Clientbound — ViaVersion `ClientboundPackets26_3`.
pub const CHUNK_BATCH_FINISHED_ID: i32 = 0x0B;
pub const CHUNK_BATCH_START_ID: i32 = 0x0C;
pub const COMMANDS_ID: i32 = 0x10;
pub const DISCONNECT_ID: i32 = 0x20;
pub const DISGUISED_CHAT_ID: i32 = 0x21;
pub const FORGET_LEVEL_CHUNK_ID: i32 = 0x26;
pub const GAME_EVENT_ID: i32 = 0x27;
pub const KEEP_ALIVE_ID: i32 = 0x2D;
pub const LEVEL_CHUNK_WITH_LIGHT_ID: i32 = 0x2E;
pub const LOGIN_ID: i32 = 0x32;
pub const ADD_ENTITY_ID: i32 = 0x01;
pub const PLAYER_ABILITIES_ID: i32 = 0x41;
pub const PLAYER_INFO_REMOVE_ID: i32 = 0x46;
pub const PLAYER_INFO_UPDATE_ID: i32 = 0x47;
pub const PLAYER_POSITION_ID: i32 = 0x49;
pub const REMOVE_ENTITIES_ID: i32 = 0x4E;
pub const RESPAWN_ID: i32 = 0x54;
pub const ROTATE_HEAD_ID: i32 = 0x55;
pub const SET_CHUNK_CACHE_CENTER_ID: i32 = 0x60;
pub const SET_DEFAULT_SPAWN_POSITION_ID: i32 = 0x63;
pub const SET_TIME_ID: i32 = 0x73;
pub const SYSTEM_CHAT_ID: i32 = 0x7C;
pub const TELEPORT_ENTITY_ID: i32 = 0x80;

// Serverbound — ViaVersion `ServerboundPackets26_3`.
pub const SB_ACCEPT_TELEPORTATION_ID: i32 = 0x00;
pub const SB_CHAT_COMMAND_ID: i32 = 0x07;
pub const SB_CHAT_ID: i32 = 0x09;
pub const SB_KEEP_ALIVE_ID: i32 = 0x1C;
pub const SB_MOVE_PLAYER_POS_ID: i32 = 0x1E;
pub const SB_MOVE_PLAYER_POS_ROT_ID: i32 = 0x1F;
pub const SB_MOVE_PLAYER_ROT_ID: i32 = 0x20;
pub const SB_MOVE_PLAYER_STATUS_ONLY_ID: i32 = 0x21;
pub const SB_PLAYER_INPUT_ID: i32 = 0x2B;
pub const SB_PLAYER_LOADED_ID: i32 = 0x2C;

/// `Game Event`의 "게임 모드 바꾸기". 값은 게임 모드 번호(실수).
pub const GAME_EVENT_CHANGE_GAME_MODE: u8 = 3;
/// `Game Event`의 "청크 기다리기 시작". 이걸 받아야 클라이언트가 로딩 화면을 끝낼 준비를 합니다.
pub const GAME_EVENT_LEVEL_CHUNKS_LOAD_START: u8 = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    Survival = 0,
    Creative = 1,
    Adventure = 2,
    Spectator = 3,
}

impl GameMode {
    pub const ALL: [GameMode; 4] = [
        GameMode::Survival,
        GameMode::Creative,
        GameMode::Adventure,
        GameMode::Spectator,
    ];

    pub fn id(self) -> i32 {
        self as i32
    }

    /// 명령어에 쓰는 이름 (`/gamemode creative`).
    pub fn name(self) -> &'static str {
        match self {
            GameMode::Survival => "survival",
            GameMode::Creative => "creative",
            GameMode::Adventure => "adventure",
            GameMode::Spectator => "spectator",
        }
    }

    /// 피해를 입는 모드 (서바이벌·모험).
    pub fn takes_damage(self) -> bool {
        matches!(self, GameMode::Survival | GameMode::Adventure)
    }

    pub fn from_id(id: i32) -> Option<GameMode> {
        GameMode::ALL.into_iter().find(|m| m.id() == id)
    }

    pub fn from_name(name: &str) -> Option<GameMode> {
        GameMode::ALL.into_iter().find(|m| m.name() == name)
    }

    pub fn display_name(self) -> &'static str {
        match self {
            GameMode::Survival => "서바이벌",
            GameMode::Creative => "크리에이티브",
            GameMode::Adventure => "모험",
            GameMode::Spectator => "관전",
        }
    }

    /// `Player Abilities` 플래그: 0x01 무적, 0x02 나는 중, 0x04 비행 가능, 0x08 즉시 부수기.
    fn ability_flags(self) -> u8 {
        match self {
            GameMode::Survival | GameMode::Adventure => 0,
            GameMode::Creative => 0x01 | 0x04 | 0x08,
            GameMode::Spectator => 0x01 | 0x02 | 0x04,
        }
    }
}

/// Login과 Respawn이 함께 쓰는 "어느 월드에 어떤 모드로" 부분 (`CommonPlayerSpawnInfo`).
#[derive(Debug, Clone, Copy)]
pub struct SpawnInfo {
    pub dimension: Dimension,
    pub hashed_seed: i64,
    pub gamemode: GameMode,
    /// F3+F4 메뉴가 "이전 모드"로 보여 주는 것.
    pub previous: Option<GameMode>,
}

impl SpawnInfo {
    fn write(&self, out: &mut Vec<u8>) {
        write_varint(out, self.dimension.type_id());
        write_string(out, self.dimension.level_name());
        write_i64(out, self.hashed_seed);
        // 26.3: 게임 모드가 byte → VarInt, 이전 게임 모드가 byte(-1 = 없음) → 압축형
        // Optional VarInt(0 = 없음, 그 외 값+1)로 바뀌었습니다.
        write_varint(out, self.gamemode.id());
        write_varint(out, self.previous.map_or(0, |m| m.id() + 1));
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
    pub online_mode: bool,
    /// 하드코어: 하트 모양이 바뀌고, 죽으면 "관전하기"만 남습니다.
    pub hardcore: bool,
}

impl Login {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_i32(&mut out, self.entity_id);
        write_bool(&mut out, self.hardcore);
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
        write_bool(&mut out, self.online_mode); // 26.2: 암호화된(정품) 연결인가
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

pub fn encode_abilities(mode: GameMode) -> Vec<u8> {
    let mut out = vec![mode.ability_flags()];
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

pub fn encode_spawn_position(dimension: Dimension, pos: (i32, i32, i32)) -> Vec<u8> {
    let mut out = Vec::new();
    write_string(&mut out, dimension.level_name());
    write_i64(&mut out, super::world::pack_position(pos));
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

/// `Move Player Pos Rot`의 시선 (좌표 24바이트 뒤).
pub fn decode_rotation_after_position(buf: &[u8]) -> Result<(f32, f32), DecodeError> {
    decode_rotation(buf.get(24..).ok_or(DecodeError::UnexpectedEof)?)
}

/// `Move Player Rot`: 시선만.
pub fn decode_rotation(mut buf: &[u8]) -> Result<(f32, f32), DecodeError> {
    Ok((read_f32(&mut buf)?, read_f32(&mut buf)?))
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

// ---- 탭 목록과 다른 플레이어 ----

/// `Player Info Update`의 동작 비트 (EnumSet, 8개라 1바이트).
const INFO_ADD_PLAYER: u8 = 0x01;
const INFO_UPDATE_GAME_MODE: u8 = 0x04;
const INFO_UPDATE_LISTED: u8 = 0x08;
const INFO_UPDATE_LATENCY: u8 = 0x10;

pub struct ListedPlayer<'a> {
    pub uuid: u128,
    pub name: &'a str,
    /// 스킨 등. 서명이 있어야 다른 클라이언트가 그 스킨을 믿고 그립니다.
    pub properties: &'a [crate::auth::Property],
    pub gamemode: GameMode,
}

/// 탭 목록에 넣기. 플레이어 엔티티를 띄우려면 먼저 이게 가 있어야 합니다.
pub fn encode_player_info_add(players: &[ListedPlayer<'_>]) -> Vec<u8> {
    let mut out =
        vec![INFO_ADD_PLAYER | INFO_UPDATE_GAME_MODE | INFO_UPDATE_LISTED | INFO_UPDATE_LATENCY];
    write_varint(&mut out, players.len() as i32);
    for p in players {
        write_uuid(&mut out, p.uuid);
        write_string(&mut out, p.name);
        crate::packets::login::write_properties(&mut out, p.properties); // 비어 있으면 기본 스킨
        write_varint(&mut out, p.gamemode.id());
        write_bool(&mut out, true); // 목록에 보이기
        write_varint(&mut out, 0); // 핑(ms)
    }
    out
}

pub fn encode_player_info_gamemode(uuid: u128, mode: GameMode) -> Vec<u8> {
    let mut out = vec![INFO_UPDATE_GAME_MODE];
    write_varint(&mut out, 1);
    write_uuid(&mut out, uuid);
    write_varint(&mut out, mode.id());
    out
}

pub fn encode_player_info_remove(uuid: u128) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, 1);
    write_uuid(&mut out, uuid);
    out
}

/// 각도(도) → 1/256 회전 단위 바이트.
fn angle(degrees: f32) -> u8 {
    ((degrees / 360.0 * 256.0).rem_euclid(256.0)) as u8
}

pub fn encode_add_player_entity(
    entity_id: i32,
    uuid: u128,
    pos: (f64, f64, f64),
    yaw: f32,
    pitch: f32,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, entity_id);
    write_uuid(&mut out, uuid);
    write_varint(&mut out, crate::data::entity_type::PLAYER);
    write_f64(&mut out, pos.0);
    write_f64(&mut out, pos.1);
    write_f64(&mut out, pos.2);
    out.push(0); // 속도: LpVec3의 영벡터는 바이트 하나(0)
    out.push(angle(pitch));
    out.push(angle(yaw));
    out.push(angle(yaw)); // 머리 방향
    write_varint(&mut out, 0); // 추가 데이터
    out
}

pub fn encode_remove_entities(ids: &[i32]) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, ids.len() as i32);
    for &id in ids {
        write_varint(&mut out, id);
    }
    out
}

/// `Teleport Entity`: 절대 좌표로 옮깁니다. 26.3에서 형식이 바뀐 Move Entity 계열 대신
/// 이것만 씁니다 — 매번 조금 크지만 단순하고, 오차가 쌓이지 않습니다.
pub fn encode_teleport_entity(
    entity_id: i32,
    pos: (f64, f64, f64),
    yaw: f32,
    pitch: f32,
    on_ground: bool,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, entity_id);
    write_f64(&mut out, pos.0);
    write_f64(&mut out, pos.1);
    write_f64(&mut out, pos.2);
    for _ in 0..3 {
        write_f64(&mut out, 0.0);
    }
    write_f32(&mut out, yaw);
    write_f32(&mut out, pitch);
    out.extend_from_slice(&0u32.to_be_bytes());
    write_bool(&mut out, on_ground);
    out
}

pub fn encode_rotate_head(entity_id: i32, yaw: f32) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, entity_id);
    out.push(angle(yaw));
    out
}

// ---- 채팅 ----

/// `chat_type` 레지스트리의 항목. 번역 키가 형식을 정합니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatKind {
    /// `<이름> 내용`
    Chat,
    /// `* 이름 내용`
    Emote,
    /// `이름 whispers to you: 내용` (받는 쪽)
    WhisperIncoming,
    /// `You whisper to 대상: 내용` (보낸 쪽)
    WhisperOutgoing,
}

impl ChatKind {
    pub fn registry_key(self) -> &'static str {
        match self {
            ChatKind::Chat => "chat",
            ChatKind::Emote => "emote_command",
            ChatKind::WhisperIncoming => "msg_command_incoming",
            ChatKind::WhisperOutgoing => "msg_command_outgoing",
        }
    }
}

/// `Disguised Chat`: 서명 없는 플레이어 채팅. 클라이언트는 `chat_type` 형식으로 그리고,
/// 채팅 설정(숨기기 등)도 일반 채팅처럼 적용합니다. 서명이 없으니 신고 대상은 아닙니다.
pub fn encode_disguised_chat(
    kind: ChatKind,
    content: &Tag,
    sender: &Tag,
    target: Option<&Tag>,
) -> Vec<u8> {
    let mut out = to_nbt(content);
    // Holder<ChatType>: 레지스트리 참조는 ID+1, 0은 직접 정의.
    write_varint(&mut out, registry_id("chat_type", kind.registry_key()) + 1);
    sender.write_root(&mut out);
    match target {
        Some(t) => {
            write_bool(&mut out, true);
            t.write_root(&mut out);
        }
        None => write_bool(&mut out, false),
    }
    out
}

// ---- 시간 ----

/// `Set Time` (26.1~): 월드 나이와 시계별 시각. 오버월드·엔드 시계 둘 다 같은 시각으로,
/// 속도 1(틱마다 1씩 흐름)로 보냅니다. 클라이언트는 받은 뒤 스스로 시간을 흘립니다.
pub fn encode_set_time(game_time: i64, day_time: i64) -> Vec<u8> {
    let mut out = Vec::new();
    write_i64(&mut out, game_time);
    let clocks = ["overworld", "the_end"];
    write_varint(&mut out, clocks.len() as i32);
    for clock in clocks {
        write_varint(&mut out, registry_id("world_clock", clock));
        crate::protocol::varint::write_varlong(&mut out, day_time);
        write_f32(&mut out, 0.0); // partial tick
        write_f32(&mut out, 1.0); // rate
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_info_layout() {
        let mut out = Vec::new();
        SpawnInfo {
            dimension: Dimension::Overworld,
            hashed_seed: 0,
            gamemode: GameMode::Creative,
            previous: None,
        }
        .write(&mut out);
        // type id(1) + 문자열(1+19) + seed(8) + gamemode(1) + prev(1) + bool×3 + cooldown(1) + sea(1)
        assert_eq!(out.len(), 1 + 20 + 8 + 1 + 1 + 3 + 1 + 1);
        assert_eq!(out[1 + 20 + 8], 1); // 크리에이티브
        assert_eq!(out[1 + 20 + 8 + 1], 0); // 이전 모드 없음
    }

    #[test]
    fn angles_wrap() {
        assert_eq!(angle(0.0), 0);
        assert_eq!(angle(90.0), 64);
        assert_eq!(angle(-90.0), 192);
        assert_eq!(angle(360.0), 0);
    }

    #[test]
    fn rotation_after_position() {
        let mut body = vec![0u8; 24];
        body.extend_from_slice(&90f32.to_be_bytes());
        body.extend_from_slice(&(-10f32).to_be_bytes());
        body.push(1);
        assert_eq!(decode_rotation_after_position(&body), Ok((90.0, -10.0)));
    }
}
