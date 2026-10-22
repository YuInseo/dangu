//! 소리, 입자, 재사용 대기 표시.

use crate::protocol::types::{
    write_bool, write_f32, write_f64, write_i32, write_i64, write_string,
};
use crate::protocol::varint::write_varint;

pub const COOLDOWN_ID: i32 = 0x16;
pub const EXPLODE_ID: i32 = 0x24;
pub const LEVEL_PARTICLES_ID: i32 = 0x30;
pub const SOUND_ID: i32 = 0x77;

/// 소리 분류 (`SoundSource`).
pub const SOURCE_PLAYER: i32 = 7;
pub const SOURCE_WEATHER: i32 = 3;

/// `Sound`: 레지스트리 소리(ID+1), 분류, 위치(1/8블록 정수), 크기, 높낮이, 씨앗.
pub fn encode_sound(
    sound: i32,
    source: i32,
    pos: (f64, f64, f64),
    volume: f32,
    pitch: f32,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, sound + 1);
    write_varint(&mut out, source);
    for c in [pos.0, pos.1, pos.2] {
        write_i32(&mut out, (c * 8.0) as i32);
    }
    write_f32(&mut out, volume);
    write_f32(&mut out, pitch);
    write_i64(&mut out, crate::game::rand_u32() as i64);
    out
}

/// 26.3 `Level Particles`: 입자가 맨 앞, 속도는 축마다, 개수는 VarInt, 끝에 흩뿌림 방식.
/// 내용 없는 입자만 씁니다.
pub fn encode_particles(
    particle: i32,
    pos: (f64, f64, f64),
    spread: (f32, f32, f32),
    speed: f32,
    count: i32,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_varint(&mut out, particle);
    write_bool(&mut out, false); // 거리 제한 무시 안 함
    write_bool(&mut out, false); // 입자 설정과 상관없이 보이기: 아니오
    write_f64(&mut out, pos.0);
    write_f64(&mut out, pos.1);
    write_f64(&mut out, pos.2);
    write_f32(&mut out, spread.0);
    write_f32(&mut out, spread.1);
    write_f32(&mut out, spread.2);
    for _ in 0..3 {
        write_f32(&mut out, speed);
    }
    write_varint(&mut out, count);
    write_varint(&mut out, 0); // 기본 흩뿌림
    out
}

/// `Explode`: 가운데, 반지름, 부서진 블록 수(표시용), 이 플레이어를 미는 속도, 입자, 소리,
/// 블록 조각 입자 목록(없음), 26.3의 "소리 내기".
pub fn encode_explosion(
    center: (f64, f64, f64),
    radius: f32,
    knockback: Option<(f64, f64, f64)>,
) -> Vec<u8> {
    let mut out = Vec::new();
    write_f64(&mut out, center.0);
    write_f64(&mut out, center.1);
    write_f64(&mut out, center.2);
    write_f32(&mut out, radius);
    write_i32(&mut out, 0);
    write_bool(&mut out, knockback.is_some());
    if let Some(k) = knockback {
        write_f64(&mut out, k.0);
        write_f64(&mut out, k.1);
        write_f64(&mut out, k.2);
    }
    let particle = if radius >= 2.0 {
        "explosion_emitter"
    } else {
        "explosion"
    };
    write_varint(&mut out, crate::data::particle_id(particle).unwrap_or(0));
    write_varint(
        &mut out,
        crate::data::sound_id("entity.generic.explode").map_or(0, |s| s + 1),
    );
    write_varint(&mut out, 0);
    write_bool(&mut out, true);
    out
}

/// `Cooldown`: 재사용 대기 그룹(아이템 ID)과 틱. 핫바의 아이템이 회색으로 차오릅니다.
pub fn encode_cooldown(group: &str, ticks: i32) -> Vec<u8> {
    let mut out = Vec::new();
    write_string(&mut out, group);
    write_varint(&mut out, ticks);
    out
}
