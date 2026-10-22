//! 몹: 무엇이 있고, 매 틱 어떻게 움직이고 싸우는지.
//!
//! - 순한 몹(소·돼지·양·닭): 돌아다니다 맞으면 도망.
//! - 근접 적대 몹(좀비·허스크·거미·동굴 거미·좀벌레·위더 스켈레톤): 쫓아와 때림.
//! - 스켈레톤·스트레이: 거리를 두고 화살을 쏨. 블레이즈: 떠서 화염구 세 발씩.
//! - 크리퍼: 다가와 1.5초 부푼 뒤 터짐 (위력 3).
//! - 중립(엔더맨·좀비 피글린): 맞으면 때린 사람을 쫓음.
//! - 주민: 집(마을) 근처를 거닐고, 우클릭하면 거래 ([`super::villagers`]).
//!
//! 걷기는 A* 길찾기([`super::path`])로 고른 칸을 따라가고, 한 칸 턱은 뛰어오릅니다. 좀비·
//! 스켈레톤·스트레이는 낮에 하늘 아래 있으면 불탑니다. 팬텀은 없습니다 (잠을 안 자도 생기지 않음).

use super::entities::{Entity, Kind, TickOut, Vec3};
use super::villagers::Villager;
use super::{rand_f64, rand_u32};
use crate::data::items::by_name;
use crate::packets::entity::Meta;
use crate::packets::world::{BlockPos, Item};
use crate::server::{Event, Server};
use crate::world::Dimension;

mod ai;
mod movement;

use ai::*;
pub use ai::{is_night, tick};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobKind {
    Cow,
    Pig,
    Sheep,
    Chicken,
    Zombie,
    Husk,
    Spider,
    CaveSpider,
    Skeleton,
    Stray,
    WitherSkeleton,
    Creeper,
    Silverfish,
    Enderman,
    ZombifiedPiglin,
    Blaze,
    Villager,
}

/// 어떻게 싸우나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fight {
    /// 싸우지 않음 (맞으면 도망).
    Flee,
    Melee,
    /// 화살.
    Bow,
    Explode,
    /// 떠서 화염구.
    Fireballs,
}

impl MobKind {
    pub const ALL: [MobKind; 17] = [
        MobKind::Cow,
        MobKind::Pig,
        MobKind::Sheep,
        MobKind::Chicken,
        MobKind::Zombie,
        MobKind::Husk,
        MobKind::Spider,
        MobKind::CaveSpider,
        MobKind::Skeleton,
        MobKind::Stray,
        MobKind::WitherSkeleton,
        MobKind::Creeper,
        MobKind::Silverfish,
        MobKind::Enderman,
        MobKind::ZombifiedPiglin,
        MobKind::Blaze,
        MobKind::Villager,
    ];
    pub const PASSIVE: [MobKind; 4] =
        [MobKind::Cow, MobKind::Pig, MobKind::Sheep, MobKind::Chicken];

    /// 레지스트리 이름 (`cow`, `zombie` …).
    pub fn name(self) -> &'static str {
        match self {
            MobKind::Cow => "cow",
            MobKind::Pig => "pig",
            MobKind::Sheep => "sheep",
            MobKind::Chicken => "chicken",
            MobKind::Zombie => "zombie",
            MobKind::Husk => "husk",
            MobKind::Spider => "spider",
            MobKind::CaveSpider => "cave_spider",
            MobKind::Skeleton => "skeleton",
            MobKind::Stray => "stray",
            MobKind::WitherSkeleton => "wither_skeleton",
            MobKind::Creeper => "creeper",
            MobKind::Silverfish => "silverfish",
            MobKind::Enderman => "enderman",
            MobKind::ZombifiedPiglin => "zombified_piglin",
            MobKind::Blaze => "blaze",
            MobKind::Villager => "villager",
        }
    }

    pub fn by_name(name: &str) -> Option<MobKind> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    pub fn type_id(self) -> i32 {
        crate::data::entity_id(self.name()).unwrap_or(crate::data::entity_type::PIG)
    }

    pub fn max_health(self) -> f32 {
        match self {
            MobKind::Cow => 10.0,
            MobKind::Pig | MobKind::Sheep | MobKind::Silverfish => 8.0,
            MobKind::Chicken => 4.0,
            MobKind::Spider => 16.0,
            MobKind::CaveSpider => 12.0,
            MobKind::Enderman => 40.0,
            _ => 20.0,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            MobKind::Cow => "소",
            MobKind::Pig => "돼지",
            MobKind::Sheep => "양",
            MobKind::Chicken => "닭",
            MobKind::Zombie => "좀비",
            MobKind::Husk => "허스크",
            MobKind::Spider => "거미",
            MobKind::CaveSpider => "동굴 거미",
            MobKind::Skeleton => "스켈레톤",
            MobKind::Stray => "스트레이",
            MobKind::WitherSkeleton => "위더 스켈레톤",
            MobKind::Creeper => "크리퍼",
            MobKind::Silverfish => "좀벌레",
            MobKind::Enderman => "엔더맨",
            MobKind::ZombifiedPiglin => "좀비 피글린",
            MobKind::Blaze => "블레이즈",
            MobKind::Villager => "주민",
        }
    }

    /// 평화로움에서 사라지고, 멀어지면 사라지는 몹.
    pub fn hostile(self) -> bool {
        !Self::PASSIVE.contains(&self) && self != MobKind::Villager
    }

    /// 먼저 덤비지 않고 맞아야 싸우는 몹.
    fn neutral(self) -> bool {
        matches!(self, MobKind::Enderman | MobKind::ZombifiedPiglin)
    }

    fn fight(self) -> Fight {
        match self {
            MobKind::Cow | MobKind::Pig | MobKind::Sheep | MobKind::Chicken | MobKind::Villager => {
                Fight::Flee
            }
            MobKind::Skeleton | MobKind::Stray => Fight::Bow,
            MobKind::Creeper => Fight::Explode,
            MobKind::Blaze => Fight::Fireballs,
            _ => Fight::Melee,
        }
    }

    fn burns_in_daylight(self) -> bool {
        matches!(self, MobKind::Zombie | MobKind::Skeleton | MobKind::Stray)
    }

    /// 걷는 속도 (블록/틱).
    fn speed(self) -> f64 {
        match self {
            MobKind::Chicken | MobKind::Villager => 0.06,
            MobKind::Spider | MobKind::CaveSpider | MobKind::Silverfish => 0.13,
            MobKind::Enderman => 0.12,
            MobKind::Creeper | MobKind::Skeleton | MobKind::Stray => 0.09,
            MobKind::Zombie | MobKind::Husk | MobKind::ZombifiedPiglin => 0.1,
            MobKind::WitherSkeleton | MobKind::Blaze => 0.1,
            _ => 0.07,
        }
    }

    /// 근접 공격력 (보통 난이도).
    fn attack_damage(self) -> f32 {
        match self {
            MobKind::Zombie | MobKind::Husk => 3.0,
            MobKind::Spider | MobKind::CaveSpider => 2.0,
            MobKind::Silverfish => 1.0,
            MobKind::Enderman => 7.0,
            MobKind::ZombifiedPiglin => 5.0,
            MobKind::WitherSkeleton => 8.0,
            MobKind::Blaze => 6.0,
            MobKind::Skeleton | MobKind::Stray => 2.0,
            _ => 0.0,
        }
    }

    /// (반쪽 폭, 키). 길찾기가 한 칸 너비를 가정하므로 반쪽 폭은 0.45까지만.
    pub fn size(self) -> (f64, f64) {
        let (w, h) = match self {
            MobKind::Cow => (0.9, 1.4),
            MobKind::Pig => (0.9, 0.9),
            MobKind::Sheep => (0.9, 1.3),
            MobKind::Chicken => (0.4, 0.7),
            MobKind::Spider => (1.4, 0.9),
            MobKind::CaveSpider => (0.7, 0.5),
            MobKind::Silverfish => (0.4, 0.3),
            MobKind::WitherSkeleton => (0.7, 2.4),
            MobKind::Creeper => (0.6, 1.7),
            MobKind::Enderman => (0.6, 2.9),
            MobKind::Blaze => (0.6, 1.8),
            _ => (0.6, 1.95),
        };
        ((w / 2.0f64).min(0.45), h)
    }

    /// 죽으면 떨어뜨리는 것: (아이템 이름, 최소, 최대).
    fn drops(self) -> &'static [(&'static str, i32, i32)] {
        match self {
            MobKind::Cow => &[("leather", 0, 2), ("beef", 1, 3)],
            MobKind::Pig => &[("porkchop", 1, 3)],
            MobKind::Sheep => &[("white_wool", 1, 1), ("mutton", 1, 2)],
            MobKind::Chicken => &[("feather", 0, 2), ("chicken", 1, 1)],
            MobKind::Zombie | MobKind::Husk => &[("rotten_flesh", 0, 2)],
            MobKind::Spider | MobKind::CaveSpider => &[("string", 0, 2), ("spider_eye", 0, 1)],
            MobKind::Skeleton | MobKind::Stray => &[("bone", 0, 2), ("arrow", 0, 2)],
            MobKind::WitherSkeleton => &[("bone", 0, 2), ("coal", 0, 1)],
            MobKind::Creeper => &[("gunpowder", 0, 2)],
            MobKind::Enderman => &[("ender_pearl", 0, 1)],
            MobKind::ZombifiedPiglin => &[("rotten_flesh", 0, 1), ("gold_nugget", 0, 1)],
            MobKind::Blaze => &[("blaze_rod", 0, 1)],
            MobKind::Silverfish | MobKind::Villager => &[],
        }
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
    /// 길찾기로 고른 칸들과 그 목적지, 다시 찾을 틱.
    path: Vec<BlockPos>,
    path_goal: Option<BlockPos>,
    repath_at: u32,
    /// 불타는 남은 틱.
    fire_ticks: u32,
    /// 크리퍼가 부푼 틱.
    fuse: Option<u32>,
    /// 블레이즈: 남은 화염구 수.
    shots: u32,
    /// 때린 플레이어 (엔티티 번호)와 화가 풀리는 틱.
    angry_at: Option<(i32, u32)>,
    /// 돌아다니는 중심 (주민의 마을).
    pub home: Option<BlockPos>,
    pub villager: Option<Villager>,
    pub sheared: bool,
    /// 누가 거래 창을 열어 둠 (그동안 서 있음).
    pub trading: bool,
    /// 움직이지 않고 맞지 않는 NPC (역할: `lottery` 등).
    pub npc: Option<&'static str>,
    /// 머리 위 이름표.
    pub custom_name: Option<String>,
    /// 마지막으로 보낸 상태 플래그 (불붙음).
    flags_sent: u8,
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
            path: Vec::new(),
            path_goal: None,
            repath_at: 0,
            fire_ticks: 0,
            fuse: None,
            shots: 0,
            angry_at: None,
            home: None,
            villager: (kind == MobKind::Villager).then(|| {
                let p = super::villagers::PROFESSIONS
                    [rand_u32() as usize % super::villagers::PROFESSIONS.len()];
                Villager::new(p, "plains")
            }),
            sheared: false,
            trading: false,
            npc: None,
            custom_name: None,
            flags_sent: 0,
        }
    }

    pub fn dead(&self) -> bool {
        self.health <= 0.0
    }

    fn flags(&self) -> u8 {
        if self.fire_ticks > 0 { 0x01 } else { 0 }
    }

    /// 띄울 때 보낼 모습.
    pub fn meta(&self) -> Vec<Meta> {
        let mut m = Vec::new();
        if self.flags() != 0 {
            m.push(Meta::Byte(0, self.flags()));
        }
        if self.fuse.is_some() {
            m.push(Meta::Int(16, 1));
        }
        if self.sheared {
            m.push(Meta::Byte(18, 0x10));
        }
        if let Some(v) = &self.villager {
            m.push(villager_meta(v));
        }
        if let Some(name) = &self.custom_name {
            m.push(Meta::Name(2, name.clone()));
            m.push(Meta::Bool(3, true));
        }
        if self.npc.is_some() {
            m.push(Meta::Byte(15, 0x01)); // AI 없음 (클라이언트도 고개를 돌리지 않음)
        }
        m
    }
}

/// 주민 종류·직업·레벨 (`Villager` 19번).
pub fn villager_meta(v: &Villager) -> Meta {
    let (t, p, l) = v.data();
    Meta::Villager(19, t, p, l)
}

/// 몹에게 피해. 엔티티 목록을 잠근 채로 부르므로 떨어뜨릴 것은 `out`에 남깁니다. 죽었으면 true.
#[allow(clippy::too_many_arguments)]
fn hurt(
    _server: &Server,
    e: &mut Entity,
    m: &mut Mob,
    amount: f32,
    attacker_pos: Option<Vec3>,
    attacker: Option<i32>,
    damage_type: &str,
    now: u32,
    out: &mut TickOut,
) -> bool {
    if m.dead() || m.npc.is_some() {
        return false;
    }
    if attacker.is_some() || attacker_pos.is_some() {
        if now < m.hurt_until {
            return false;
        }
        m.hurt_until = now + 10; // 0.5초 무적
    }
    m.health -= amount;
    if let Some(a) = attacker {
        m.angry_at = Some((a, now + 600));
    }
    if m.kind.fight() == Fight::Flee {
        m.panic = 60;
        m.goal = None;
    }
    if let Some(from) = attacker_pos {
        let (dx, dz) = (e.pos.0 - from.0, e.pos.2 - from.2);
        let len = (dx * dx + dz * dz).sqrt().max(0.01);
        e.vel = (dx / len * 0.4, 0.36, dz / len * 0.4);
    }
    out.events.push(Event::Hurt {
        dimension: e.dimension,
        entity_id: e.id,
        damage_type: crate::data::registry_id("damage_type", damage_type),
        attacker,
    });
    if !m.dead() {
        return false;
    }
    out.events.push(Event::EntityAnimation {
        dimension: e.dimension,
        entity_id: e.id,
        event: crate::packets::entity::ENTITY_EVENT_DEATH,
    });
    let at = (
        e.pos.0.floor() as i32,
        e.pos.1.floor() as i32,
        e.pos.2.floor() as i32,
    );
    for &(name, lo, hi) in m.kind.drops() {
        let count = lo + (rand_u32() % (hi - lo + 1) as u32) as i32;
        if let (Some(item), true) = (by_name(name), count > 0) {
            out.drops.push((e.dimension, at, Item::new(item, count)));
        }
    }
    true
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
    let now = entities.now();
    let mut out = TickOut::default();
    let damage_type = if attacker.is_some() {
        "player_attack"
    } else if attacker_pos.is_some() {
        "explosion"
    } else {
        "generic"
    };
    let dead = entities
        .update(id, |e| {
            let Kind::Mob(m) = &mut e.kind else {
                return false;
            };
            let mut m = std::mem::replace(m, Mob::new(MobKind::Pig));
            let dead = hurt(
                server,
                e,
                &mut m,
                amount,
                attacker_pos,
                attacker,
                damage_type,
                now,
                &mut out,
            );
            e.kind = Kind::Mob(m);
            dead
        })
        .unwrap_or(false);
    for ev in out.events {
        server.publish(ev);
    }
    for (dim, at, item) in out.drops {
        entities.drop_at_block(server, dim, at, item);
    }
    dead
}

pub fn spawn(server: &Server, kind: MobKind, dimension: Dimension, pos: Vec3) -> i32 {
    spawn_mob(server, Mob::new(kind), dimension, pos)
}

pub fn spawn_mob(server: &Server, mob: Mob, dimension: Dimension, pos: Vec3) -> i32 {
    let id = server.allocate_entity_id();
    server.entities.insert(
        server,
        Entity {
            id,
            uuid: u128::from_le_bytes(crate::crypto::random::bytes::<16>()),
            dimension,
            pos,
            vel: (0.0, 0.0, 0.0),
            yaw: (rand_f64() * 360.0) as f32,
            pitch: 0.0,
            on_ground: false,
            collided: false,
            age: 0,
            kind: Kind::Mob(mob),
        },
    );
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::block;
    use crate::game::projectiles;
    use crate::packets::status::ServerInfo;
    use crate::server::PlayerState;
    use crate::world::blockset::b;

    fn server() -> Server {
        Server::new(
            ServerInfo {
                name: "t".into(),
                description: "t".into(),
                max_players: 1,
            },
            3,
        )
    }

    /// 돌 바닥 위 빈 방 (x, z: -r..=r, 높이 4).
    fn room(s: &Server, y: i32, r: i32) {
        let d = Dimension::Overworld;
        for x in -r..=r {
            for z in -r..=r {
                s.world.set(d, x, y - 1, z, block::STONE);
                for dy in 0..6 {
                    s.world.set(d, x, y + dy, z, block::AIR);
                }
            }
        }
    }

    fn mob(s: &Server, id: i32) -> Mob {
        match s.entities.get(id).unwrap().kind {
            Kind::Mob(m) => m,
            _ => unreachable!(),
        }
    }

    #[test]
    fn every_kind_has_a_registry_entry() {
        for k in MobKind::ALL {
            assert!(crate::data::entity_id(k.name()).is_some(), "{}", k.name());
            assert_eq!(MobKind::by_name(k.name()), Some(k));
        }
        assert_eq!(MobKind::by_name("phantom"), None, "no phantoms");
    }

    #[test]
    fn zombie_walks_around_a_wall_to_reach_its_goal() {
        let s = server();
        let d = Dimension::Overworld;
        let y = 200;
        room(&s, y, 8);
        for z in -5..=5 {
            for dy in 0..3 {
                s.world.set(d, 0, y + dy, z, block::STONE);
            }
        }
        let id = spawn(&s, MobKind::Villager, d, (-4.5, y as f64, 0.5));
        s.entities.update(id, |e| {
            if let Kind::Mob(m) = &mut e.kind {
                m.goal = Some(((4.5, y as f64, 0.5), u32::MAX));
            }
        });
        for _ in 0..600 {
            s.entities.tick(&s);
            let e = s.entities.get(id).unwrap();
            if e.pos.0 > 3.5 {
                return;
            }
            // 목표를 다시 고르지 않게 계속 잡아 둠
            s.entities.update(id, |e| {
                if let Kind::Mob(m) = &mut e.kind {
                    m.goal = Some(((4.5, y as f64, 0.5), u32::MAX));
                }
            });
        }
        panic!("stuck at {:?}", s.entities.get(id).unwrap().pos);
    }

    #[test]
    fn creeper_swells_and_explodes() {
        let s = server();
        let d = Dimension::Overworld;
        let y = 200;
        room(&s, y, 6);
        let id = spawn(&s, MobKind::Creeper, d, (0.5, y as f64, 0.5));
        let mut out = TickOut::default();
        let player = PlayerState::test(d, (2.0, y as f64, 0.5));
        let mut e = s.entities.get(id).unwrap();
        for t in 0..CREEPER_FUSE + 1 {
            tick(&s, &mut e, std::slice::from_ref(&player), t, &mut out);
        }
        assert_eq!(out.explosions.len(), 1);
        assert!(matches!(&e.kind, Kind::Mob(m) if m.dead()));
        assert!(
            out.events.iter().any(
                |ev| matches!(ev, Event::EntityData { meta, .. } if meta == &[Meta::Int(16, 1)])
            )
        );
    }

    #[test]
    fn skeleton_shoots_and_arrow_hits() {
        let s = server();
        let d = Dimension::Overworld;
        let y = 200;
        room(&s, y, 12);
        let id = spawn(&s, MobKind::Skeleton, d, (0.5, y as f64, 0.5));
        let player = PlayerState::test(d, (8.5, y as f64, 0.5));
        // 화살에는 바닐라처럼 약간의 흩어짐이 있어, 몇 발 가운데 맞으면 됩니다.
        let mut events = Vec::new();
        for shot in 0..5 {
            let mut out = TickOut::default();
            let mut e = s.entities.get(id).unwrap();
            tick(
                &s,
                &mut e,
                std::slice::from_ref(&player),
                shot * 100,
                &mut out,
            );
            assert_eq!(out.spawns.len(), 1, "fired an arrow");
            let mut arrow = out.spawns.pop().unwrap();
            for _ in 0..40 {
                if projectiles::tick(&s, &mut arrow, std::slice::from_ref(&player), &mut events) {
                    break;
                }
            }
        }
        assert!(
            events.iter().any(|ev| matches!(
                ev,
                Event::Damage {
                    damage_type: "arrow",
                    ..
                }
            )),
            "arrow hit the player"
        );
    }

    #[test]
    fn zombie_burns_in_daylight_but_not_under_a_roof() {
        let s = server();
        let d = Dimension::Overworld;
        let y = 200;
        room(&s, y, 4);
        s.set_day_time(1000);
        let id = spawn(&s, MobKind::Zombie, d, (0.5, y as f64, 0.5));
        let mut e = s.entities.get(id).unwrap();
        assert!(in_daylight(&s, &e));
        s.world.set(d, 0, y + 4, 0, b("oak_planks"));
        assert!(!in_daylight(&s, &e));
        let mut out = TickOut::default();
        let far = PlayerState::test(d, (40.5, y as f64, 0.5));
        s.world.set(d, 0, y + 4, 0, block::AIR);
        for t in 0..40 {
            tick(&s, &mut e, std::slice::from_ref(&far), t, &mut out);
        }
        assert!(matches!(&e.kind, Kind::Mob(m) if m.fire_ticks > 0));
        assert!(mob(&s, id).health == 20.0, "the stored copy is untouched");
    }
}
