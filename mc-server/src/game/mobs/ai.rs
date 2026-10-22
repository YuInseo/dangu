//! 몹의 머리: 매 틱 누구를 노릴지, 때릴지·쏠지·부풀지·도망칠지 정합니다.

use crate::game::entities::{Entity, Kind, TickOut, Vec3, dist2, physics};
use crate::game::projectiles::{self, Projectile, ProjectileKind};
use crate::game::{rand_f64, rand_u32};
use crate::packets::entity::Meta;
use crate::server::{Difficulty, Event, PlayerState, Server, TICKS_PER_DAY};
use crate::world::Dimension;
use crate::world::blockset::name_of;

use super::movement::*;
use super::*;

pub(super) const SEARCH_RANGE: f64 = 16.0;
pub(super) const ATTACK_RANGE: f64 = 1.6;
pub(super) const DESPAWN_RANGE: f64 = 128.0;
/// 스켈레톤이 멈춰 서서 쏘는 거리.
pub(super) const BOW_RANGE: f64 = 12.0;
/// 크리퍼가 부풀기 시작하는 거리, 멈추는 거리, 터지기까지 틱.
pub(super) const CREEPER_START: f64 = 3.0;
pub(super) const CREEPER_CANCEL: f64 = 7.0;
pub(super) const CREEPER_FUSE: u32 = 30;
pub(super) const CREEPER_POWER: f32 = 3.0;

pub fn tick(server: &Server, e: &mut Entity, players: &[PlayerState], now: u32, out: &mut TickOut) {
    let Kind::Mob(m) = &mut e.kind else { return };
    let mut m = std::mem::replace(m, Mob::new(MobKind::Pig));
    think(server, e, &mut m, players, now, out);
    e.kind = Kind::Mob(m);
}

pub(super) fn think(
    server: &Server,
    e: &mut Entity,
    m: &mut Mob,
    players: &[PlayerState],
    now: u32,
    out: &mut TickOut,
) {
    if m.dead() {
        m.dead_ticks += 1;
        return;
    }
    if m.npc.is_some() {
        stop(e);
        physics(server, e, 0.08, 0.91);
        return;
    }
    let kind = m.kind;
    m.attack_cooldown = m.attack_cooldown.saturating_sub(1);
    m.panic = m.panic.saturating_sub(1);
    if m.angry_at.is_some_and(|(_, until)| now > until) {
        m.angry_at = None;
    }

    // 아무도 가까이 없거나 평화로움이면 적대 몹은 사라집니다 (바닐라처럼).
    let near = players
        .iter()
        .filter(|p| p.dimension == e.dimension)
        .map(|p| dist2(p.position, e.pos))
        .fold(f64::MAX, f64::min);
    let peaceful = server.difficulty() == Difficulty::Peaceful;
    if kind.hostile() && (peaceful || near > DESPAWN_RANGE * DESPAWN_RANGE) {
        m.health = 0.0;
        m.dead_ticks = 100;
        return;
    }

    // 햇빛과 불
    let in_water = name_of(server.world.get(
        e.dimension,
        e.pos.0.floor() as i32,
        e.pos.1.floor() as i32,
        e.pos.2.floor() as i32,
    )) == "water";
    if in_water {
        m.fire_ticks = 0;
    } else if kind.burns_in_daylight()
        && (now + e.id as u32).is_multiple_of(20)
        && in_daylight(server, e)
    {
        m.fire_ticks = m.fire_ticks.max(160);
    }
    if m.fire_ticks > 0 {
        m.fire_ticks -= 1;
        if m.fire_ticks.is_multiple_of(20)
            && hurt(server, e, m, 1.0, None, None, "on_fire", now, out)
        {
            return;
        }
    }
    if m.flags() != m.flags_sent {
        m.flags_sent = m.flags();
        out.events.push(Event::EntityData {
            dimension: e.dimension,
            id: e.id,
            meta: vec![Meta::Byte(0, m.flags_sent)],
        });
    }

    // 양은 풀을 뜯어 털이 다시 자람, 주민은 가끔 물건을 다시 채움
    if m.sheared && rand_u32().is_multiple_of(1000) {
        let below = (
            e.pos.0.floor() as i32,
            e.pos.1.floor() as i32 - 1,
            e.pos.2.floor() as i32,
        );
        if server.world.get(e.dimension, below.0, below.1, below.2)
            == crate::data::block::GRASS_BLOCK
        {
            server.set_block(e.dimension, below, crate::data::block::DIRT);
            m.sheared = false;
            out.events.push(Event::EntityData {
                dimension: e.dimension,
                id: e.id,
                meta: vec![Meta::Byte(18, 0)],
            });
        }
    }
    if let Some(v) = &mut m.villager
        && (now + e.id as u32).is_multiple_of(6000)
    {
        v.restock();
    }

    // 누구를 노리나
    let night = is_night(server);
    let aggressive = match kind {
        MobKind::Spider => night || e.pos.1 < 50.0,
        _ => kind.hostile() && !kind.neutral(),
    };
    let prey = players
        .iter()
        .filter(|p| p.dimension == e.dimension && !p.dead && p.gamemode.takes_damage())
        .filter(|p| {
            let angry = m.angry_at.is_some_and(|(id, _)| id == p.entity_id);
            angry || aggressive && dist2(p.position, e.pos) < SEARCH_RANGE * SEARCH_RANGE
        })
        .min_by(|a, b| dist2(a.position, e.pos).total_cmp(&dist2(b.position, e.pos)));

    let speed = kind.speed();
    let mut gravity = 0.08;
    match (kind.fight(), prey) {
        (Fight::Melee, Some(p)) => {
            walk(server, e, m, p.position, speed, now, out);
            melee(server, e, m, p, out);
        }
        (Fight::Bow, Some(p)) => {
            let d = dist2(p.position, e.pos).sqrt();
            let eye = (e.pos.0, e.pos.1 + 1.6, e.pos.2);
            let target = (p.position.0, p.position.1 + 0.6, p.position.2);
            let sees = line_of_sight(server, e.dimension, eye, target);
            if d > BOW_RANGE || !sees {
                walk(server, e, m, p.position, speed, now, out);
            } else {
                stop(e);
                face(e, p.position);
            }
            if sees && d < SEARCH_RANGE && m.attack_cooldown == 0 {
                m.attack_cooldown = match server.difficulty() {
                    Difficulty::Hard => 20,
                    _ => 40,
                };
                let vel = projectiles::aim_arrow(eye, target, 1.6, 0.08);
                let arrow = Projectile {
                    kind: ProjectileKind::Arrow,
                    owner: e.id,
                    owner_name: kind.display_name(),
                    damage: 3.0 + (rand_u32() % 2) as f32,
                    stuck: 0,
                };
                out.spawns
                    .push(projectiles::launch(server, e, arrow, eye, vel));
                sound(e, "entity.skeleton.shoot", out);
            }
        }
        (Fight::Explode, Some(p)) => {
            let d = dist2(p.position, e.pos).sqrt();
            let swelling = m.fuse.is_some_and(|_| d < CREEPER_CANCEL) || d < CREEPER_START;
            if swelling {
                stop(e);
                face(e, p.position);
                if m.fuse.is_none() {
                    sound(e, "entity.creeper.primed", out);
                    swell(e, 1, out);
                }
                let f = m.fuse.unwrap_or(0) + 1;
                m.fuse = Some(f);
                if f >= CREEPER_FUSE {
                    out.explosions.push((
                        e.dimension,
                        (e.pos.0, e.pos.1 + 0.5, e.pos.2),
                        CREEPER_POWER,
                        false,
                    ));
                    m.health = 0.0;
                    m.dead_ticks = 100;
                    return;
                }
            } else {
                unswell(e, m, out);
                walk(server, e, m, p.position, speed, now, out);
            }
        }
        (Fight::Fireballs, Some(p)) => {
            gravity = 0.0;
            let want_y = p.position.1 + 1.5;
            e.vel.1 = (e.vel.1 + if e.pos.1 < want_y { 0.03 } else { -0.03 }).clamp(-0.15, 0.15);
            let d = dist2(p.position, e.pos).sqrt();
            if d > 6.0 {
                walk(server, e, m, p.position, speed, now, out);
            } else {
                stop(e);
                face(e, p.position);
            }
            melee(server, e, m, p, out);
            let eye = (e.pos.0, e.pos.1 + 1.2, e.pos.2);
            let target = (p.position.0, p.position.1 + 1.0, p.position.2);
            if d < SEARCH_RANGE
                && m.attack_cooldown == 0
                && line_of_sight(server, e.dimension, eye, target)
            {
                if m.shots == 0 {
                    m.shots = 3;
                }
                m.shots -= 1;
                m.attack_cooldown = if m.shots == 0 { 100 } else { 6 };
                let vel = projectiles::aim_straight(eye, target, 0.8, 0.1);
                let ball = Projectile {
                    kind: ProjectileKind::SmallFireball,
                    owner: e.id,
                    owner_name: kind.display_name(),
                    damage: 5.0,
                    stuck: 0,
                };
                out.spawns
                    .push(projectiles::launch(server, e, ball, eye, vel));
                sound(e, "entity.blaze.shoot", out);
            }
        }
        _ => {
            unswell(e, m, out);
            if kind == MobKind::Blaze {
                gravity = 0.02;
            }
            if m.trading {
                stop(e);
            } else {
                wander(server, e, m, now, out);
            }
        }
    }
    physics(server, e, gravity, 0.91);
}

/// 낮이고 머리 위로 하늘이 트였나 (오버월드).
pub(super) fn in_daylight(server: &Server, e: &Entity) -> bool {
    if e.dimension != Dimension::Overworld || is_night(server) {
        return false;
    }
    let (x, z) = (e.pos.0.floor() as i32, e.pos.2.floor() as i32);
    let head = e.pos.1.floor() as i32 + 1;
    (head + 1..head + 64).all(|y| {
        let s = server.world.get(e.dimension, x, y, z);
        crate::world::features::soft(s)
            && s != crate::data::block::WATER
            && !name_of(s).ends_with("leaves")
    })
}

/// 하루 13000~23000 (바닐라에서 적대 몹이 밖에 나오는 때).
pub fn is_night(server: &Server) -> bool {
    (13000..23000).contains(&server.time().1.rem_euclid(TICKS_PER_DAY))
}

/// 눈에서 눈까지 단단한 블록이 없나.
pub(super) fn line_of_sight(server: &Server, dim: Dimension, from: Vec3, to: Vec3) -> bool {
    let d = dist2(from, to).sqrt();
    let n = (d / 0.5).ceil() as i32;
    (1..n).all(|i| {
        let t = i as f64 / n as f64;
        !server.world.is_solid(
            dim,
            (from.0 + (to.0 - from.0) * t).floor() as i32,
            (from.1 + (to.1 - from.1) * t).floor() as i32,
            (from.2 + (to.2 - from.2) * t).floor() as i32,
        )
    })
}

pub(super) fn melee(server: &Server, e: &Entity, m: &mut Mob, p: &PlayerState, out: &mut TickOut) {
    let (dx, dy, dz) = (
        p.position.0 - e.pos.0,
        p.position.1 - e.pos.1,
        p.position.2 - e.pos.2,
    );
    if dx * dx + dz * dz < ATTACK_RANGE * ATTACK_RANGE && dy.abs() < 2.0 && m.attack_cooldown == 0 {
        m.attack_cooldown = 20;
        out.events.push(Event::Damage {
            target: p.uuid,
            amount: server.difficulty().scale_mob_damage(m.kind.attack_damage()) * m.damage_mult,
            damage_type: "mob_attack",
            attacker: Some((e.id, e.pos)),
            attacker_name: m
                .custom_name
                .clone()
                .unwrap_or_else(|| m.kind.display_name().to_owned()),
        });
    }
}

pub(super) fn sound(e: &Entity, name: &str, out: &mut TickOut) {
    if let Some(sound) = crate::data::sound_id(name) {
        out.events.push(Event::Sound {
            dimension: e.dimension,
            pos: e.pos,
            sound,
            source: 5, // hostile
            volume: 1.0,
            pitch: 1.0,
        });
    }
}

pub(super) fn swell(e: &Entity, dir: i32, out: &mut TickOut) {
    out.events.push(Event::EntityData {
        dimension: e.dimension,
        id: e.id,
        meta: vec![Meta::Int(16, dir)],
    });
}

/// 크리퍼가 멀어진 상대를 놓치면 천천히 줄어듭니다.
pub(super) fn unswell(e: &Entity, m: &mut Mob, out: &mut TickOut) {
    if let Some(f) = m.fuse {
        if f <= 1 {
            m.fuse = None;
            swell(e, -1, out);
        } else {
            m.fuse = Some(f - 1);
        }
    }
}

pub(super) fn stop(e: &mut Entity) {
    e.vel.0 = 0.0;
    e.vel.2 = 0.0;
}

pub(super) fn face(e: &mut Entity, at: Vec3) {
    let (dx, dz) = (at.0 - e.pos.0, at.2 - e.pos.2);
    if dx * dx + dz * dz > 1e-6 {
        e.yaw = (-dx.atan2(dz)).to_degrees() as f32;
    }
}

/// 할 일이 없을 때: 가끔 근처 아무 데나 (주민은 집 근처), 맞았으면 멀리 도망.
pub(super) fn wander(server: &Server, e: &mut Entity, m: &mut Mob, now: u32, out: &mut TickOut) {
    let mut speed = m.kind.speed();
    if m.panic > 0 {
        speed *= 2.0;
    }
    let need_new = match m.goal {
        None => rand_u32().is_multiple_of(80) || m.panic > 0,
        Some((g, until)) => now > until || dist2((g.0, e.pos.1, g.2), e.pos) < 0.5,
    };
    if need_new && (m.goal.is_none() || m.panic > 0 || rand_u32().is_multiple_of(2)) {
        let r = if m.panic > 0 { 8.0 } else { 6.0 };
        let center = match (m.home, m.panic) {
            (Some(h), 0) if dist2((h.0 as f64, e.pos.1, h.2 as f64), e.pos) > 16.0 * 16.0 => {
                (h.0 as f64 + 0.5, e.pos.1, h.2 as f64 + 0.5)
            }
            _ => e.pos,
        };
        let g = (
            center.0 + (rand_f64() * 2.0 - 1.0) * r,
            e.pos.1,
            center.2 + (rand_f64() * 2.0 - 1.0) * r,
        );
        m.goal = Some((g, now + 140));
        m.path.clear();
    } else if need_new {
        m.goal = None;
    }
    match m.goal {
        Some((g, _)) => {
            if !walk(server, e, m, g, speed, now, out) {
                m.goal = None;
            }
        }
        None => stop(e),
    }
}
