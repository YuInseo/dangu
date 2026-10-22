//! 상태별 패킷 정의. 각 패킷은 `decode(&[u8])` 또는 `encode() -> Vec<u8>`를 갖고,
//! 프레임(길이·ID)은 [`crate::protocol::frame`]이 붙입니다.

pub mod handshake;
pub mod login;
pub mod status;
