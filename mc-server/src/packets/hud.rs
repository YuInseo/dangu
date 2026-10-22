//! 화면에 띄우는 것과 자잘한 상태 패킷: 제목, 액션 바, 탭 목록 머리·꼬리, 서버 정보,
//! 난이도, 핑, 카메라, 명령어 자동 완성.

use super::play::GameMode;
use crate::protocol::DecodeError;
use crate::protocol::nbt::Tag;
use crate::protocol::types::{
    read_bool, read_i64, read_string, read_uuid, write_bool, write_f32, write_i32, write_i64,
    write_string, write_uuid,
};
use crate::protocol::varint::{read_varint, write_varint};
use crate::text::to_nbt;

pub const BOSS_EVENT_ID: i32 = 0x09;
pub const CHANGE_DIFFICULTY_ID: i32 = 0x0A;
pub const CLEAR_TITLES_ID: i32 = 0x0E;
pub const COMMAND_SUGGESTIONS_ID: i32 = 0x0F;
pub const PONG_RESPONSE_ID: i32 = 0x3F;
pub const SERVER_DATA_ID: i32 = 0x58;
pub const SET_ACTION_BAR_TEXT_ID: i32 = 0x59;
pub const SET_CAMERA_ID: i32 = 0x5F;
pub const SET_SUBTITLE_TEXT_ID: i32 = 0x72;
pub const SET_TITLE_TEXT_ID: i32 = 0x74;
pub const SET_TITLES_ANIMATION_ID: i32 = 0x75;
pub const TAB_LIST_ID: i32 = 0x7D;

pub const SB_CHANGE_DIFFICULTY_ID: i32 = 0x04;
pub const SB_CHANGE_GAME_MODE_ID: i32 = 0x05;
pub const SB_CLIENT_INFORMATION_ID: i32 = 0x0E;
pub const SB_COMMAND_SUGGESTION_ID: i32 = 0x0F;
pub const SB_LOCK_DIFFICULTY_ID: i32 = 0x1D;
pub const SB_PING_REQUEST_ID: i32 = 0x26;
pub const SB_PLAYER_ABILITIES_ID: i32 = 0x28;
pub const SB_SPECTATOR_ACTION_ID: i32 = 0x3F;
pub const SB_TELEPORT_TO_ENTITY_ID: i32 = 0x40;

/// `Change Difficulty`: 난이도(VarInt)와 잠금.
pub fn encode_difficulty(difficulty: u8, locked: bool) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, difficulty as i32);
    write_bool(&mut out, locked);
    out
}

/// 보스바 색 (`BossEvent.BossBarColor` 순서).
#[derive(Debug, Clone, Copy)]
pub enum BossColor {
    Pink = 0,
    Blue = 1,
    Red = 2,
    Green = 3,
    Yellow = 4,
    Purple = 5,
    White = 6,
}

/// 보스바 띄우기: 제목, 채운 정도(0..1), 색, 칸 나눔 없음, 하늘 어둡게 등 없음.
pub fn encode_boss_add(uuid: u128, title: &Tag, progress: f32, color: BossColor) -> Vec<u8> {
    let mut out = Vec::new();
    write_uuid(&mut out, uuid);
    write_varint(&mut out, 0);
    out.extend_from_slice(&to_nbt(title));
    write_f32(&mut out, progress);
    write_varint(&mut out, color as i32);
    write_varint(&mut out, 0);
    out.push(0);
    out
}

pub fn encode_boss_remove(uuid: u128) -> Vec<u8> {
    let mut out = Vec::new();
    write_uuid(&mut out, uuid);
    write_varint(&mut out, 1);
    out
}

pub fn encode_boss_title(uuid: u128, title: &Tag) -> Vec<u8> {
    let mut out = Vec::new();
    write_uuid(&mut out, uuid);
    write_varint(&mut out, 3);
    out.extend_from_slice(&to_nbt(title));
    out
}

pub fn encode_text(text: &Tag) -> Vec<u8> {
    to_nbt(text)
}

/// 제목이 나타나고(`fade_in`) 머물고(`stay`) 사라지는(`fade_out`) 틱.
pub fn encode_titles_animation(fade_in: i32, stay: i32, fade_out: i32) -> Vec<u8> {
    let mut out = Vec::new();
    for t in [fade_in, stay, fade_out] {
        write_i32(&mut out, t);
    }
    out
}

pub fn encode_clear_titles(reset: bool) -> Vec<u8> {
    vec![reset as u8]
}

pub fn encode_tab_list(header: &Tag, footer: &Tag) -> Vec<u8> {
    let mut out = to_nbt(header);
    out.extend(to_nbt(footer));
    out
}

/// `Server Data`: 채팅 신고 화면 등에 쓰는 MOTD와 (없는) 아이콘.
pub fn encode_server_data(motd: &Tag) -> Vec<u8> {
    let mut out = to_nbt(motd);
    write_bool(&mut out, false);
    out
}

pub fn encode_camera(entity_id: i32) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, entity_id);
    out
}

pub fn encode_pong(payload: i64) -> Vec<u8> {
    let mut out = Vec::new();
    write_i64(&mut out, payload);
    out
}

/// `Command Suggestions`: 바꿀 구간(`start`, `length`, 글자 수)과 후보들.
pub fn encode_suggestions(
    transaction: i32,
    start: i32,
    length: i32,
    matches: &[String],
) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, transaction);
    write_varint(&mut out, start);
    write_varint(&mut out, length);
    write_varint(&mut out, matches.len() as i32);
    for m in matches {
        write_string(&mut out, m);
        write_bool(&mut out, false); // 설명 없음
    }
    out
}

pub fn decode_varint_only(mut buf: &[u8]) -> Result<i32, DecodeError> {
    read_varint(&mut buf)
}

pub fn decode_bool_only(mut buf: &[u8]) -> Result<bool, DecodeError> {
    read_bool(&mut buf)
}

pub fn decode_i64_only(mut buf: &[u8]) -> Result<i64, DecodeError> {
    read_i64(&mut buf)
}

pub fn decode_uuid_only(mut buf: &[u8]) -> Result<u128, DecodeError> {
    read_uuid(&mut buf)
}

/// `Change Game Mode` (F3+F4 메뉴).
pub fn decode_change_game_mode(buf: &[u8]) -> Result<Option<GameMode>, DecodeError> {
    Ok(GameMode::from_id(decode_varint_only(buf)?))
}

/// `Command Suggestion`: (번호, `/` 없는 입력 중인 줄).
pub fn decode_command_suggestion(mut buf: &[u8]) -> Result<(i32, String), DecodeError> {
    let id = read_varint(&mut buf)?;
    Ok((id, read_string(&mut buf, 32500)?))
}

/// 26.3 `Spectator Action`: 관전 중 좌클릭한 엔티티 (0 = 허공, 아니면 ID+1).
pub fn decode_spectator_action(buf: &[u8]) -> Result<Option<i32>, DecodeError> {
    let v = decode_varint_only(buf)?;
    Ok((v != 0).then_some(v - 1))
}

/// Play 단계의 `Client Information` 중 쓰는 것: (시야 거리, 스킨 레이어, 주 손).
pub fn decode_client_information(mut buf: &[u8]) -> Result<(u8, u8, i32), DecodeError> {
    read_string(&mut buf, 16)?; // 언어
    let view = crate::protocol::types::read_u8(&mut buf)?;
    read_varint(&mut buf)?; // 채팅 표시
    read_bool(&mut buf)?; // 채팅 색
    let skin = crate::protocol::types::read_u8(&mut buf)?;
    let hand = read_varint(&mut buf)?;
    Ok((view, skin, hand))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectator_action_is_optional_varint() {
        assert_eq!(decode_spectator_action(&[0]).unwrap(), None);
        assert_eq!(decode_spectator_action(&[6]).unwrap(), Some(5));
    }

    #[test]
    fn suggestions_layout() {
        let body = encode_suggestions(3, 4, 2, &["ab".into()]);
        assert_eq!(body, [3, 4, 2, 1, 2, b'a', b'b', 0]);
    }
}
