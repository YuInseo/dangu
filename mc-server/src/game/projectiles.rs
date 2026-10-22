//! 날아가는 것: 스켈레톤 화살, 블레이즈 화염구.
//!
//! 한 틱 이동을 0.5칸 이하 토막으로 나눠 플레이어 몸(폭 0.6, 키 1.8)이나 단단한 블록에 닿는지
//! 봅니다. 화살은 블록에 꽂혀 1분 뒤 사라지고(몹이 쏜 화살이라 줍지 못함), 화염구는 닿으면
//! 사라집니다.

use super::entities::{Entity, Kind, Vec3};
use crate::server::{Event, PlayerState, Server};

/// 꽂힌 화살이 남아 있는 틱.
const STUCK_LIFETIME: u32 = 1200;
const FIREBALL_LIFETIME: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectileKind {
    Arrow,
    SmallFireball,
}

impl ProjectileKind {
    pub fn name(self) -> &'static str {
        match self {
            ProjectileKind::Arrow => "arrow",
            ProjectileKind::SmallFireball => "small_fireball",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Projectile {
    pub kind: ProjectileKind,
    /// 쏜 몹 (맞은 사람에게 누가 쐈는지 보여 줌).
    pub owner: i32,
    pub owner_name: &'static str,
    pub damage: f32,
    /// 꽂힌 뒤 지난 틱 (0 = 날아가는 중).
    pub stuck: u32,
}

/// 쏩니다: `from`에서 `vel`로.
pub fn launch(server: &Server, e: &Entity, p: Projectile, from: Vec3, vel: Vec3) -> Entity {
    let (yaw, pitch) = rotation(vel);
    Entity {
        id: server.allocate_entity_id(),
        uuid: u128::from_le_bytes(crate::crypto::random::bytes::<16>()),
        dimension: e.dimension,
        pos: from,
        vel,
        yaw,
        pitch,
        on_ground: false,
        collided: false,
        age: 0,
        kind: Kind::Projectile(p),
    }
}

/// 화살 방향 (바닐라 `AbstractArrow`처럼 yaw = atan2(x, z)).
fn rotation(v: Vec3) -> (f32, f32) {
    let h = (v.0 * v.0 + v.2 * v.2).sqrt();
    (
        v.0.atan2(v.2).to_degrees() as f32,
        v.1.atan2(h).to_degrees() as f32,
    )
}

/// 맞은 사람 몸 안인가.
fn inside(p: &PlayerState, at: Vec3) -> bool {
    (at.0 - p.position.0).abs() < 0.4
        && (at.2 - p.position.2).abs() < 0.4
        && (p.position.1 - 0.1..p.position.1 + 1.9).contains(&at.1)
}

/// 1틱. 없애야 하면 true.
pub fn tick(
    server: &Server,
    e: &mut Entity,
    players: &[PlayerState],
    events: &mut Vec<Event>,
) -> bool {
    let Kind::Projectile(p) = &mut e.kind else {
        return false;
    };
    if p.stuck > 0 {
        p.stuck += 1;
        return p.stuck > STUCK_LIFETIME;
    }
    if p.kind == ProjectileKind::SmallFireball && e.age > FIREBALL_LIFETIME {
        return true;
    }
    let speed = (e.vel.0.powi(2) + e.vel.1.powi(2) + e.vel.2.powi(2)).sqrt();
    let steps = (speed / 0.5).ceil().max(1.0) as i32;
    let d = (
        e.vel.0 / steps as f64,
        e.vel.1 / steps as f64,
        e.vel.2 / steps as f64,
    );
    for _ in 0..steps {
        let next = (e.pos.0 + d.0, e.pos.1 + d.1, e.pos.2 + d.2);
        if let Some(hit) = players
            .iter()
            .filter(|pl| pl.dimension == e.dimension && !pl.dead && pl.gamemode.takes_damage())
            .find(|pl| inside(pl, next))
        {
            let (damage_type, amount) = match p.kind {
                ProjectileKind::Arrow => ("arrow", p.damage),
                ProjectileKind::SmallFireball => ("fireball", p.damage),
            };
            events.push(Event::Damage {
                target: hit.uuid,
                amount: server.difficulty().scale_mob_damage(amount),
                damage_type,
                attacker: Some((p.owner, e.pos)),
                attacker_name: p.owner_name.to_owned(),
            });
            return true;
        }
        let cell = (
            next.0.floor() as i32,
            next.1.floor() as i32,
            next.2.floor() as i32,
        );
        if server.world.is_solid(e.dimension, cell.0, cell.1, cell.2) {
            if p.kind == ProjectileKind::SmallFireball {
                return true;
            }
            p.stuck = 1;
            e.vel = (0.0, 0.0, 0.0);
            e.on_ground = true;
            return false;
        }
        e.pos = next;
    }
    if p.kind == ProjectileKind::Arrow {
        e.vel = (e.vel.0 * 0.99, e.vel.1 * 0.99 - 0.05, e.vel.2 * 0.99);
        (e.yaw, e.pitch) = rotation(e.vel);
    }
    e.pos.1 < (e.dimension.min_y() - 64) as f64
}

/// `from`에서 `to`를 노리는 화살 속도 (바닐라 스켈레톤처럼 거리만큼 위로 올려 쏨).
pub fn aim_arrow(from: Vec3, to: Vec3, power: f64, spread: f64) -> Vec3 {
    let (dx, dy, dz) = (to.0 - from.0, to.1 - from.1, to.2 - from.2);
    let h = (dx * dx + dz * dz).sqrt();
    let v = (dx, dy + h * 0.2, dz);
    scale(v, power, spread)
}

pub fn aim_straight(from: Vec3, to: Vec3, power: f64, spread: f64) -> Vec3 {
    scale((to.0 - from.0, to.1 - from.1, to.2 - from.2), power, spread)
}

fn scale(v: Vec3, power: f64, spread: f64) -> Vec3 {
    let len = (v.0 * v.0 + v.1 * v.1 + v.2 * v.2).sqrt().max(1e-6);
    let jitter = || (super::rand_f64() - 0.5) * spread;
    (
        (v.0 / len + jitter()) * power,
        (v.1 / len + jitter()) * power,
        (v.2 / len + jitter()) * power,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_aim_rises_with_distance() {
        let v = aim_arrow((0.0, 0.0, 0.0), (20.0, 0.0, 0.0), 1.6, 0.0);
        assert!(v.1 > 0.0 && v.0 > 1.0);
        let (yaw, pitch) = rotation((0.0, 0.0, 1.0));
        assert_eq!((yaw, pitch), (0.0, 0.0));
    }
}
