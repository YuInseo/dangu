//! 몹: 소·돼지·양·닭(돌아다니고, 맞으면 도망)과 좀비·거미(밤에 나와 쫓아와 때림).
//!
//! 바닐라 AI(길찾기, 목표 선택기)를 옮기지는 않습니다. 몹마다 "목표 지점으로 걷고, 한 칸
//! 턱은 뛰어 오르고, 벽이면 멈춘다"는 단순한 규칙으로 움직입니다.

use super::entities::{Entity, Kind, Vec3, dist2, physics};
use super::{rand_f64, rand_u32};
use crate::data::entity_type;
use crate::data::items::by_name;
use crate::packets::world::Item;
use crate::server::{Event, PlayerState, Server, TICKS_PER_DAY};
use crate::world::{Dimension, terrain};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobKind {
    Cow,
    Pig,
    Sheep,
    Chicken,
    Zombie,
    Spider,
}

impl MobKind {
    pub const PASSIVE: [MobKind; 4] =
        [MobKind::Cow, MobKind::Pig, MobKind::Sheep, MobKind::Chicken];
    pub const HOSTILE: [MobKind; 2] = [MobKind::Zombie, MobKind::Spider];

    pub fn type_id(self) -> i32 {
        match self {
            MobKind::Cow => entity_type::COW,
            MobKind::Pig => entity_type::PIG,
            MobKind::Sheep => entity_type::SHEEP,
            MobKind::Chicken => entity_type::CHICKEN,
            MobKind::Zombie => entity_type::ZOMBIE,
            MobKind::Spider => entity_type::SPIDER,
        }
    }

    pub fn max_health(self) -> f32 {
        match self {
            MobKind::Cow => 10.0,
            MobKind::Pig | MobKind::Sheep => 8.0,
            MobKind::Chicken => 4.0,
            MobKind::Zombie => 20.0,
            MobKind::Spider => 16.0,
        }
    }

    pub fn hostile(self) -> bool {
        Self::HOSTILE.contains(&self)
    }

    /// 걷는 속도 (블록/틱).
    fn speed(self) -> f64 {
        match self {
            MobKind::Chicken => 0.06,
            MobKind::Spider => 0.13,
            MobKind::Zombie => 0.1,
            _ => 0.07,
        }
    }

    fn attack_damage(self) -> f32 {
        match self {
            MobKind::Zombie => 3.0,
            MobKind::Spider => 2.0,
            _ => 0.0,
        }
    }

    /// 죽으면 떨어뜨리는 것: (아이템 이름, 최소, 최대).
    fn drops(self) -> &'static [(&'static str, i32, i32)] {
        match self {
            MobKind::Cow => &[("leather", 0, 2), ("beef", 1, 3)],
            MobKind::Pig => &[("porkchop", 1, 3)],
            MobKind::Sheep => &[("white_wool", 1, 1), ("mutton", 1, 2)],
            MobKind::Chicken => &[("feather", 0, 2), ("chicken", 1, 1)],
            MobKind::Zombie => &[("rotten_flesh", 0, 2)],
            MobKind::Spider => &[("string", 0, 2)],
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            MobKind::Cow => "소",
            MobKind::Pig => "돼지",
            MobKind::Sheep => "양",
            MobKind::Chicken => "닭",
            MobKind::Zombie => "좀비",
            MobKind::Spider => "거미",
        }
    }

    pub fn by_name(name: &str) -> Option<MobKind> {
        Some(match name {
            "cow" => MobKind::Cow,
            "pig" => MobKind::Pig,
            "sheep" => MobKind::Sheep,
            "chicken" => MobKind::Chicken,
            "zombie" => MobKind::Zombie,
            "spider" => MobKind::Spider,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Mob {
    pub kind: MobKind,
    pub health: f32,
    /// 걸어갈 곳과 포기할 틱.
    goal: Option<(Vec3, u32)>,
    /// 맞고 도망치는 남은 틱 (순한 몹).
    panic: u32,
    attack_cooldown: u32,
    /// 죽은 뒤 지난 틱 (쓰러지는 모습을 보여 준 뒤 지웁니다).
    pub dead_ticks: u32,
    pub hurt_until: u32,
}

impl Mob {
    pub fn new(kind: MobKind) -> Self {
        Self {
            kind,
            health: kind.max_health(),
            goal: None,
            panic: 0,
            attack_cooldown: 0,
            dead_ticks: 0,
            hurt_until: 0,
        }
    }

    pub fn dead(&self) -> bool {
        self.health <= 0.0
    }
}

const SEARCH_RANGE: f64 = 16.0;
const ATTACK_RANGE: f64 = 1.6;
const DESPAWN_RANGE: f64 = 128.0;
const MAX_MOBS: usize = 80;

pub fn tick(
    server: &Server,
    e: &mut Entity,
    players: &[PlayerState],
    now: u32,
    events: &mut Vec<Event>,
) {
    let Kind::Mob(m) = &mut e.kind else { return };
    if m.dead() {
        m.dead_ticks += 1;
        return;
    }
    let kind = m.kind;
    m.attack_cooldown = m.attack_cooldown.saturating_sub(1);
    m.panic = m.panic.saturating_sub(1);

    // 아무도 가까이 없으면 사라집니다 (적대 몹만, 바닐라처럼).
    let near = players
        .iter()
        .filter(|p| p.dimension == e.dimension)
        .map(|p| dist2(p.position, e.pos))
        .fold(f64::MAX, f64::min);
    if kind.hostile() && near > DESPAWN_RANGE * DESPAWN_RANGE {
        m.health = 0.0;
        m.dead_ticks = 100;
        return;
    }

    // 어디로 갈까
    let mut speed = kind.speed();
    let mut target: Option<Vec3> = None;
    if kind.hostile() {
        let prey = players
            .iter()
            .filter(|p| p.dimension == e.dimension && !p.dead && p.gamemode.takes_damage())
            .filter(|p| dist2(p.position, e.pos) < SEARCH_RANGE * SEARCH_RANGE)
            .min_by(|a, b| dist2(a.position, e.pos).total_cmp(&dist2(b.position, e.pos)));
        if let Some(p) = prey {
            target = Some(p.position);
            let (dx, dy, dz) = (
                p.position.0 - e.pos.0,
                p.position.1 - e.pos.1,
                p.position.2 - e.pos.2,
            );
            if dx * dx + dz * dz < ATTACK_RANGE * ATTACK_RANGE
                && dy.abs() < 2.0
                && m.attack_cooldown == 0
            {
                m.attack_cooldown = 20;
                events.push(Event::Damage {
                    target: p.uuid,
                    amount: kind.attack_damage(),
                    damage_type: "mob_attack",
                    attacker: Some((e.id, e.pos)),
                    attacker_name: kind.display_name().to_owned(),
                });
            }
        }
    }
    if target.is_none() {
        if m.panic > 0 {
            speed *= 2.0;
        }
        let need_new = match m.goal {
            None => rand_u32().is_multiple_of(80) || m.panic > 0,
            Some((g, until)) => now > until || dist2((g.0, e.pos.1, g.2), e.pos) < 0.5,
        };
        if need_new && (m.goal.is_none() || m.panic > 0 || rand_u32().is_multiple_of(2)) {
            let r = if m.panic > 0 { 8.0 } else { 6.0 };
            let g = (
                e.pos.0 + (rand_f64() * 2.0 - 1.0) * r,
                e.pos.1,
                e.pos.2 + (rand_f64() * 2.0 - 1.0) * r,
            );
            m.goal = Some((g, now + 100));
        } else if need_new {
            m.goal = None;
        }
        target = m.goal.map(|(g, _)| g);
    }

    // 걷기: 앞 칸이 막혔으면 한 칸 턱은 뛰고, 벽이면 x나 z 한쪽으로만 비켜 갑니다.
    if let Some(t) = target {
        let (dx, dz) = (t.0 - e.pos.0, t.2 - e.pos.2);
        let len = (dx * dx + dz * dz).sqrt();
        if len > 0.3 {
            let dir = (dx / len, dz / len);
            let tries = [dir, (dir.0.signum(), 0.0), (0.0, dir.1.signum())];
            let mut moved = false;
            for (ux, uz) in tries {
                if ux == 0.0 && uz == 0.0 {
                    continue;
                }
                match step(server, e.dimension, e.pos, ux, uz) {
                    s @ (Step::Free | Step::Jump) => {
                        if s == Step::Jump && e.on_ground {
                            e.vel.1 = 0.42;
                        }
                        e.vel.0 = ux * speed;
                        e.vel.2 = uz * speed;
                        e.yaw = (-ux.atan2(uz)).to_degrees() as f32;
                        moved = true;
                        break;
                    }
                    Step::Wall => {}
                }
            }
            if !moved {
                e.vel.0 = 0.0;
                e.vel.2 = 0.0;
                m.goal = None;
            }
        }
    }
    physics(server, e, 0.08, 0.91);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Free,
    /// 한 칸 턱: 뛰면 넘어감
    Jump,
    Wall,
}

/// (ux, uz) 쪽으로 몸 반쪽(0.3) 너머, 곧 들어갈 칸을 봅니다.
fn step(server: &Server, dimension: Dimension, pos: Vec3, ux: f64, uz: f64) -> Step {
    let world = &server.world;
    let here = (pos.0.floor() as i32, pos.2.floor() as i32);
    let ahead = (
        (pos.0 + ux * 0.6).floor() as i32,
        (pos.2 + uz * 0.6).floor() as i32,
    );
    if ahead == here {
        return Step::Free;
    }
    let feet = pos.1.floor() as i32;
    let solid = |dy: i32| world.is_solid(dimension, ahead.0, feet + dy, ahead.1);
    match (solid(0), solid(1), solid(2)) {
        (false, false, _) => Step::Free,
        (true, false, false) => Step::Jump,
        _ => Step::Wall,
    }
}

/// 몹을 때립니다. 죽였으면 true.
pub fn damage(
    server: &Server,
    id: i32,
    amount: f32,
    attacker_pos: Option<Vec3>,
    attacker: Option<i32>,
) -> bool {
    let entities = &server.entities;
    let mut map = entities.map.lock().unwrap();
    let Some(e) = map.get_mut(&id) else {
        return false;
    };
    let now = *entities.tick.lock().unwrap();
    let Kind::Mob(m) = &mut e.kind else {
        return false;
    };
    if m.dead() || now < m.hurt_until {
        return false;
    }
    m.hurt_until = now + 10; // 0.5초 무적
    m.health -= amount;
    m.panic = 60;
    m.goal = None;
    if let Some(from) = attacker_pos {
        let (dx, dz) = (e.pos.0 - from.0, e.pos.2 - from.2);
        let len = (dx * dx + dz * dz).sqrt().max(0.01);
        e.vel = (dx / len * 0.4, 0.36, dz / len * 0.4);
    }
    let (dimension, pos, kind, dead) = (e.dimension, e.pos, m.kind, m.dead());
    drop(map);

    server.publish(Event::Hurt {
        dimension,
        entity_id: id,
        damage_type: crate::data::registry_id(
            "damage_type",
            if attacker.is_some() {
                "player_attack"
            } else {
                "generic"
            },
        ),
        attacker,
    });
    if dead {
        server.publish(Event::EntityAnimation {
            dimension,
            entity_id: id,
            event: crate::packets::entity::ENTITY_EVENT_DEATH,
        });
        for &(name, lo, hi) in kind.drops() {
            let count = lo + (rand_u32() % (hi - lo + 1) as u32) as i32;
            if let (Some(item), true) = (by_name(name), count > 0) {
                let at = (
                    pos.0.floor() as i32,
                    pos.1.floor() as i32,
                    pos.2.floor() as i32,
                );
                entities.drop_at_block(server, dimension, at, Item { id: item, count });
            }
        }
    }
    dead
}

pub fn spawn(server: &Server, kind: MobKind, dimension: Dimension, pos: Vec3) {
    server.entities.insert(
        server,
        Entity {
            id: server.allocate_entity_id(),
            uuid: u128::from_le_bytes(crate::crypto::random::bytes::<16>()),
            dimension,
            pos,
            vel: (0.0, 0.0, 0.0),
            yaw: (rand_f64() * 360.0) as f32,
            pitch: 0.0,
            on_ground: false,
            age: 0,
            kind: Kind::Mob(Mob::new(kind)),
        },
    );
}

/// 플레이어 주변에 몹을 채웁니다: 낮에는 순한 몹, 밤에는 적대 몹도 (오버월드만).
pub fn natural_spawning(server: &Server, players: &[PlayerState], now: u32) {
    if !now.is_multiple_of(100) || !server.config.spawn_mobs {
        return;
    }
    let mobs: Vec<(Dimension, Vec3, bool)> = server
        .entities
        .map
        .lock()
        .unwrap()
        .values()
        .filter_map(|e| match &e.kind {
            Kind::Mob(m) if !m.dead() => Some((e.dimension, e.pos, m.kind.hostile())),
            _ => None,
        })
        .collect();
    if mobs.len() >= MAX_MOBS {
        return;
    }
    // 하루 시각 13000~23000이 밤 (바닐라에서 적대 몹이 밖에 나오는 때)
    let night = (13000..23000).contains(&server.time().1.rem_euclid(TICKS_PER_DAY));
    for p in players
        .iter()
        .filter(|p| p.dimension == Dimension::Overworld && !p.dead)
    {
        let around = |hostile: bool| {
            mobs.iter()
                .filter(|(d, pos, h)| {
                    *d == p.dimension && *h == hostile && dist2(*pos, p.position) < 64.0 * 64.0
                })
                .count()
        };
        let (kinds, distance): (&[MobKind], (f64, f64)) = if night && around(true) < 4 {
            (&MobKind::HOSTILE, (24.0, 40.0))
        } else if around(false) < 6 {
            (&MobKind::PASSIVE, (12.0, 32.0))
        } else {
            continue;
        };
        let angle = rand_f64() * std::f64::consts::TAU;
        let r = distance.0 + rand_f64() * (distance.1 - distance.0);
        let (x, z) = (
            (p.position.0 + angle.cos() * r).floor() as i32,
            (p.position.2 + angle.sin() * r).floor() as i32,
        );
        let Some(y) = terrain::standing_y(Dimension::Overworld, server.seed, x, z) else {
            continue;
        };
        // 플레이어가 그 자리를 바꿨을 수 있으니 실제 블록으로 확인
        if !server.world.is_solid(p.dimension, x, y - 1, z)
            || server.world.is_solid(p.dimension, x, y, z)
        {
            continue;
        }
        let kind = kinds[rand_u32() as usize % kinds.len()];
        spawn(
            server,
            kind,
            p.dimension,
            (x as f64 + 0.5, y as f64, z as f64 + 0.5),
        );
    }
}
