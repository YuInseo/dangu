//! 상태별 패킷 정의. 각 패킷은 `decode(&[u8])` 또는 `encode() -> Vec<u8>`를 갖고,
//! 프레임(길이·ID)은 [`crate::protocol::frame`]이 붙입니다.
//!
//! 패킷 ID 출처: ViaVersion `protocols/v26_2to26_3/packet/*Packets26_3` (열거 순서 = ID).
//! 필드 형식: minecraft-data `pc/26.1` + ViaVersion 26.1→26.2→26.3 변환기의 차이.

pub mod commands;
pub mod configuration;
pub mod handshake;
pub mod login;
pub mod play;
pub mod status;
pub mod world;
