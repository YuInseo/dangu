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

use super::entities::{Entity, Kind, TickOut, Vec3, dist2, physics};
use super::projectiles::{self, Projectile, ProjectileKind};
use super::villagers::Villager;
use super::{path, rand_f64, rand_u32};
use crate::data::items::by_name;
use crate::packets::entity::Meta;
use crate::packets::world::{BlockPos, Item};
use crate::server::{Difficulty, Event, PlayerState, Server, TICKS_PER_DAY};
use crate::world::Dimension;
use crate::world::blockset::name_of;

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

const SEARCH_RANGE: f64 = 16.0;
const ATTACK_RANGE: f64 = 1.6;
const DESPAWN_RANGE: f64 = 128.0;
/// 스켈레톤이 멈춰 서서 쏘는 거리.
const BOW_RANGE: f64 = 12.0;
/// 크리퍼가 부풀기 시작하는 거리, 멈추는 거리, 터지기까지 틱.
const CREEPER_START: f64 = 3.0;
const CREEPER_CANCEL: f64 = 7.0;
const CREEPER_FUSE: u32 = 30;
const CREEPER_POWER: f32 = 3.0;

pub fn tick(server: &Server, e: &mut Entity, players: &[PlayerState], now: u32, out: &mut TickOut) {
    let Kind::Mob(m) = &mut e.kind else { return };
    let mut m = std::mem::replace(m, Mob::new(MobKind::Pig));
    think(server, e, &mut m, players, now, out);
    e.kind = Kind::Mob(m);
}

fn think(
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
fn in_daylight(server: &Server, e: &Entity) -> bool {
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
fn line_of_sight(server: &Server, dim: Dimension, from: Vec3, to: Vec3) -> bool {
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

fn melee(server: &Server, e: &Entity, m: &mut Mob, p: &PlayerState, out: &mut TickOut) {
    let (dx, dy, dz) = (
        p.position.0 - e.pos.0,
        p.position.1 - e.pos.1,
        p.position.2 - e.pos.2,
    );
    if dx * dx + dz * dz < ATTACK_RANGE * ATTACK_RANGE && dy.abs() < 2.0 && m.attack_cooldown == 0 {
        m.attack_cooldown = 20;
        out.events.push(Event::Damage {
            target: p.uuid,
            amount: server.difficulty().scale_mob_damage(m.kind.attack_damage()),
            damage_type: "mob_attack",
            attacker: Some((e.id, e.pos)),
            attacker_name: m.kind.display_name().to_owned(),
        });
    }
}

fn sound(e: &Entity, name: &str, out: &mut TickOut) {
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

fn swell(e: &Entity, dir: i32, out: &mut TickOut) {
    out.events.push(Event::EntityData {
        dimension: e.dimension,
        id: e.id,
        meta: vec![Meta::Int(16, dir)],
    });
}

/// 크리퍼가 멀어진 상대를 놓치면 천천히 줄어듭니다.
fn unswell(e: &Entity, m: &mut Mob, out: &mut TickOut) {
    if let Some(f) = m.fuse {
        if f <= 1 {
            m.fuse = None;
            swell(e, -1, out);
        } else {
            m.fuse = Some(f - 1);
        }
    }
}

fn stop(e: &mut Entity) {
    e.vel.0 = 0.0;
    e.vel.2 = 0.0;
}

fn face(e: &mut Entity, at: Vec3) {
    let (dx, dz) = (at.0 - e.pos.0, at.2 - e.pos.2);
    if dx * dx + dz * dz > 1e-6 {
        e.yaw = (-dx.atan2(dz)).to_degrees() as f32;
    }
}

/// 할 일이 없을 때: 가끔 근처 아무 데나 (주민은 집 근처), 맞았으면 멀리 도망.
fn wander(server: &Server, e: &mut Entity, m: &mut Mob, now: u32, out: &mut TickOut) {
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

fn block_of(p: Vec3) -> BlockPos {
    (
        p.0.floor() as i32,
        (p.1 + 0.01).floor() as i32,
        p.2.floor() as i32,
    )
}

/// `to` 쪽으로 한 틱 걷습니다: 길을 찾아 그 칸들을 따라가고, 턱 앞에서는 뜁니다. 길이 없고
/// 앞이 벽이면 false.
fn walk(
    server: &Server,
    e: &mut Entity,
    m: &mut Mob,
    to: Vec3,
    speed: f64,
    now: u32,
    out: &mut TickOut,
) -> bool {
    let goal = block_of(to);
    let feet = block_of(e.pos);
    let (gx, gz) = (to.0 - e.pos.0, to.2 - e.pos.2);
    let close = gx * gx + gz * gz < 2.0 && (to.1 - e.pos.1).abs() < 1.0;
    if close {
        m.path.clear();
    } else {
        let stale = m.path.is_empty()
            || m.path_goal.is_none_or(|g| {
                (g.0 - goal.0).abs() + (g.1 - goal.1).abs() + (g.2 - goal.2).abs() > 1
            });
        if stale && now >= m.repath_at && out.path_budget > 0 {
            out.path_budget -= 1;
            m.path = path::find(server, e.dimension, feet, goal, 1);
            m.path_goal = Some(goal);
            m.repath_at = now + 10 + rand_u32() % 20;
        }
    }
    // 다다른 칸은 버림
    while let Some(&wp) = m.path.first() {
        let (dx, dz) = (wp.0 as f64 + 0.5 - e.pos.0, wp.2 as f64 + 0.5 - e.pos.2);
        if dx * dx + dz * dz < 0.15 && (e.pos.1 - wp.1 as f64).abs() < 1.1 {
            m.path.remove(0);
        } else {
            break;
        }
    }
    let (aim, climb) = match m.path.first() {
        Some(&wp) => (
            (wp.0 as f64 + 0.5, wp.1 as f64, wp.2 as f64 + 0.5),
            wp.1 > feet.1,
        ),
        None => (to, false),
    };
    let (dx, dz) = (aim.0 - e.pos.0, aim.2 - e.pos.2);
    let len = (dx * dx + dz * dz).sqrt();
    if len < 0.05 {
        stop(e);
        return true;
    }
    let mut dir = (dx / len, dz / len);
    let mut jump = climb && len < 1.5;
    if m.path.is_empty() {
        // 길이 없으면 곧장: 한 칸 턱은 뛰고, 벽이면 x나 z 한쪽으로 비켜 봅니다.
        let tries = [dir, (dir.0.signum(), 0.0), (0.0, dir.1.signum())];
        let free = tries.into_iter().find_map(|(ux, uz)| {
            if ux == 0.0 && uz == 0.0 {
                return None;
            }
            match step(server, e.dimension, e.pos, ux, uz) {
                Step::Free => Some(((ux, uz), false)),
                Step::Jump => Some(((ux, uz), true)),
                Step::Wall => None,
            }
        });
        match free {
            Some((d, j)) => {
                dir = d;
                jump |= j;
            }
            None => {
                stop(e);
                return false;
            }
        }
    }
    if e.on_ground && (jump || e.collided) {
        e.vel.1 = 0.42;
    }
    let s = speed.min(len.max(0.02));
    e.vel.0 = dir.0 * s;
    e.vel.2 = dir.1 * s;
    e.yaw = (-dir.0.atan2(dir.1)).to_degrees() as f32;
    true
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
    use crate::packets::status::ServerInfo;
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
        let mut out = TickOut::default();
        let mut e = s.entities.get(id).unwrap();
        tick(&s, &mut e, std::slice::from_ref(&player), 0, &mut out);
        assert_eq!(out.spawns.len(), 1, "fired an arrow");
        let mut arrow = out.spawns.pop().unwrap();
        let mut events = Vec::new();
        for _ in 0..40 {
            if projectiles::tick(&s, &mut arrow, std::slice::from_ref(&player), &mut events) {
                break;
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
