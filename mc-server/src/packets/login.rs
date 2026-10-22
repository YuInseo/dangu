//! Login 상태. 지금은 버전이 맞지 않는 클라이언트를 돌려보내는 데만 씁니다.

use crate::json::{escape, text_component};
use crate::protocol::types::write_string;

/// Clientbound `Disconnect (login)` (`login_disconnect`).
pub const DISCONNECT_ID: i32 = 0x00;

/// Login 단계의 Disconnect 사유는 아직 NBT가 아니라 JSON 텍스트 컴포넌트입니다.
pub fn encode_disconnect(reason: &str) -> Vec<u8> {
    let mut out = Vec::new();
    write_string(&mut out, &text_component(&escape(reason)));
    out
}
