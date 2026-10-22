//! 플레이어가 아닌 엔티티: 떨어진 아이템, 몹, 화살, 불붙은 TNT. 서버 전체에서 하나의 틱 루프가
//! 움직입니다.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use super::mobs::{self, Mob};
use super::projectiles::{self, Projectile};
use super::rand_f64;
use crate::data::entity_type;
use crate::packets::entity::Meta;
use crate::packets::world::{BlockPos, Item};
use crate::server::{Event, Server};
use crate::world::Dimension;

pub type Vec3 = (f64, f64, f64);

/// 아이템이 사라지기까지 (5분).
const ITEM_LIFETIME: u32 = 6000;
/// 플레이어가 던진 아이템을 다시 주울 수 있기까지.
pub const THROWN_PICKUP_DELAY: u32 = 40;
const PICKUP_RANGE: f64 = 1.5;
/// 줍기 제안에 답이 없으면 이만큼 뒤 다시 제안합니다.
const CLAIM_TIMEOUT: u32 = 20;

#[derive(Debug, Clone)]
pub enum Kind {
    Item {
        item: Item,
        pickup_delay: u32,
        claimed: Option<(u128, u32)>,
    },
    Mob(Mob),
    /// 불붙은 TNT: 남은 틱.
    Tnt {
        fuse: u32,
    },
    Projectile(Projectile),
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: i32,
    pub uuid: u128,
    pub dimension: Dimension,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    /// 지난 틱에 옆 벽에 막혔음 (몹이 뛰어오를지 고를 때).
    pub collided: bool,
    pub age: u32,
    pub kind: Kind,
}

/// 세션이 엔티티를 띄울 때 필요한 것.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub id: i32,
    pub uuid: u128,
    pub type_id: i32,
    pub dimension: Dimension,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub item: Option<Item>,
    /// 띄운 뒤 곧바로 보낼 엔티티 데이터 (몹 모습).
    pub meta: Vec<Meta>,
}

impl Entity {
    pub fn mob_kind(&self) -> Option<super::mobs::MobKind> {
        match &self.kind {
            Kind::Mob(m) => Some(m.kind),
            _ => None,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let (type_id, item) = match &self.kind {
            Kind::Item { item, .. } => (entity_type::ITEM, Some(*item)),
            Kind::Mob(m) => (m.kind.type_id(), None),
            Kind::Tnt { .. } => (
                crate::data::entity_id("tnt").unwrap_or(entity_type::ITEM),
                None,
            ),
            Kind::Projectile(p) => (
                crate::data::entity_id(p.kind.name()).unwrap_or(entity_type::ITEM),
                None,
            ),
        };
        let meta = match &self.kind {
            Kind::Mob(m) => m.meta(),
            _ => Vec::new(),
        };
        Snapshot {
            id: self.id,
            uuid: self.uuid,
            type_id,
            dimension: self.dimension,
            pos: self.pos,
            vel: self.vel,
            yaw: self.yaw,
            pitch: self.pitch,
            item,
            meta,
        }
    }

    /// (반쪽 폭, 키): 옆 벽에 막히는지 볼 때.
    fn size(&self) -> (f64, f64) {
        match &self.kind {
            Kind::Mob(m) => m.kind.size(),
            Kind::Tnt { .. } => (0.49, 0.98),
            _ => (0.125, 0.25),
        }
    }
}

/// 틱 하나 동안 몹·화살이 남긴 일: 엔티티 목록을 잠근 채로는 할 수 없는 것들.
#[derive(Default)]
pub struct TickOut {
    pub events: Vec<Event>,
    /// (차원, 중심, 위력, 부서진 블록을 모두 떨어뜨릴지)
    pub explosions: Vec<(Dimension, Vec3, f32, bool)>,
    /// 새로 생길 엔티티 (화살 등).
    pub spawns: Vec<Entity>,
    /// 떨어뜨릴 것.
    pub drops: Vec<(Dimension, BlockPos, Item)>,
    /// 이번 틱에 길찾기를 더 할 수 있는 횟수.
    pub path_budget: u32,
}

/// 한 틱에 길찾기(A*)를 하는 몹 수 상한.
const PATHS_PER_TICK: u32 = 6;

#[derive(Default)]
pub struct Entities {
    pub(super) map: Mutex<HashMap<i32, Entity>>,
    pub(super) tick: Mutex<u32>,
    /// 생성기마다 다음에 몹을 낼 틱.
    pub(super) spawners: Mutex<HashMap<(Dimension, BlockPos), u32>>,
    /// 주민을 채운 마을 (차원, 시작 청크).
    pub(super) villages: Mutex<HashSet<(Dimension, i32, i32)>>,
}

fn new_uuid() -> u128 {
    u128::from_le_bytes(crate::crypto::random::bytes::<16>())
}

impl Entities {
    /// 아이템을 떨어뜨립니다. `vel`은 블록/틱.
    pub fn spawn_item(
        &self,
        server: &Server,
        dimension: Dimension,
        pos: Vec3,
        vel: Vec3,
        item: Item,
        pickup_delay: u32,
    ) {
        if item.count <= 0 {
            return;
        }
        self.insert(
            server,
            Entity {
                id: server.allocate_entity_id(),
                uuid: new_uuid(),
                dimension,
                pos,
                vel,
                yaw: (rand_f64() * 360.0) as f32,
                pitch: 0.0,
                on_ground: false,
                collided: false,
                age: 0,
                kind: Kind::Item {
                    item,
                    pickup_delay,
                    claimed: None,
                },
            },
        );
    }

    /// 블록 자리에서 튀어나오는 드롭 (바닐라처럼 조금씩 흩어짐).
    pub fn drop_at_block(
        &self,
        server: &Server,
        dimension: Dimension,
        block: (i32, i32, i32),
        item: Item,
    ) {
        let pos = (
            block.0 as f64 + 0.25 + rand_f64() * 0.5,
            block.1 as f64 + 0.25 + rand_f64() * 0.5,
            block.2 as f64 + 0.25 + rand_f64() * 0.5,
        );
        let vel = (rand_f64() * 0.2 - 0.1, 0.2, rand_f64() * 0.2 - 0.1);
        self.spawn_item(server, dimension, pos, vel, item, 10);
    }

    pub fn insert(&self, server: &Server, e: Entity) {
        let snapshot = e.snapshot();
        self.map.lock().unwrap().insert(e.id, e);
        server.publish(Event::EntitySpawned(snapshot));
    }

    pub fn snapshots_in(&self, dimension: Dimension) -> Vec<Snapshot> {
        self.map
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.dimension == dimension)
            .map(Entity::snapshot)
            .collect()
    }

    pub fn count(&self) -> usize {
        self.map.lock().unwrap().len()
    }

    /// 세션이 줍기 제안에 답합니다: `taken`개를 인벤토리에 넣었음.
    pub fn finish_pickup(&self, server: &Server, entity_id: i32, collector: i32, taken: i32) {
        let mut map = self.map.lock().unwrap();
        let Some(e) = map.get_mut(&entity_id) else {
            return;
        };
        let dimension = e.dimension;
        let Kind::Item { item, claimed, .. } = &mut e.kind else {
            return;
        };
        *claimed = None;
        if taken <= 0 {
            return;
        }
        if taken >= item.count {
            map.remove(&entity_id);
            drop(map);
            server.publish(Event::ItemCollected {
                dimension,
                item_entity: entity_id,
                collector,
                count: taken,
            });
            server.publish(Event::EntitiesRemoved {
                dimension,
                ids: vec![entity_id],
            });
        } else {
            item.count -= taken;
            let item = *item;
            drop(map);
            server.publish(Event::ItemCollected {
                dimension,
                item_entity: entity_id,
                collector,
                count: taken,
            });
            server.publish(Event::ItemStackChanged {
                dimension,
                id: entity_id,
                item,
            });
        }
    }

    pub fn remove(&self, server: &Server, id: i32) -> Option<Entity> {
        let e = self.map.lock().unwrap().remove(&id)?;
        server.publish(Event::EntitiesRemoved {
            dimension: e.dimension,
            ids: vec![id],
        });
        Some(e)
    }

    pub fn get(&self, id: i32) -> Option<Entity> {
        self.map.lock().unwrap().get(&id).cloned()
    }

    /// 엔티티 하나를 고칩니다 (주민 거래, 양털 깎기).
    pub fn update<R>(&self, id: i32, f: impl FnOnce(&mut Entity) -> R) -> Option<R> {
        self.map.lock().unwrap().get_mut(&id).map(f)
    }

    /// 지금 틱 번호.
    pub fn now(&self) -> u32 {
        *self.tick.lock().unwrap()
    }

    /// 1틱 진행.
    pub fn tick(&self, server: &Server) {
        let now = {
            let mut t = self.tick.lock().unwrap();
            *t += 1;
            *t
        };
        let players = server.players();
        let mut out = TickOut {
            path_budget: PATHS_PER_TICK,
            ..TickOut::default()
        };
        let mut removed: Vec<(Dimension, i32)> = Vec::new();
        {
            let mut map = self.map.lock().unwrap();
            for e in map.values_mut() {
                e.age += 1;
                let before = (e.pos, e.yaw);
                match &mut e.kind {
                    Kind::Item {
                        item,
                        pickup_delay,
                        claimed,
                    } => {
                        if e.age > ITEM_LIFETIME {
                            removed.push((e.dimension, e.id));
                            continue;
                        }
                        *pickup_delay = pickup_delay.saturating_sub(1);
                        if claimed.is_some_and(|(_, at)| now - at > CLAIM_TIMEOUT) {
                            *claimed = None;
                        }
                        let (item, ready) = (*item, *pickup_delay == 0 && claimed.is_none());
                        physics(server, e, 0.04, 0.98);
                        if ready {
                            let nearest = players
                                .iter()
                                .filter(|p| p.dimension == e.dimension && !p.dead)
                                .filter(|p| {
                                    let (dx, dz) = (p.position.0 - e.pos.0, p.position.2 - e.pos.2);
                                    let dy = e.pos.1 - p.position.1;
                                    dx * dx + dz * dz < PICKUP_RANGE * PICKUP_RANGE
                                        && (-0.5..2.3).contains(&dy)
                                })
                                .min_by(|a, b| {
                                    dist2(a.position, e.pos).total_cmp(&dist2(b.position, e.pos))
                                });
                            if let Some(p) = nearest {
                                if let Kind::Item { claimed, .. } = &mut e.kind {
                                    *claimed = Some((p.uuid, now));
                                }
                                out.events.push(Event::PickupOffer {
                                    to: p.uuid,
                                    entity_id: e.id,
                                    item,
                                });
                            }
                        }
                    }
                    Kind::Tnt { fuse } => {
                        *fuse = fuse.saturating_sub(1);
                        if *fuse == 0 {
                            out.explosions.push((
                                e.dimension,
                                (e.pos.0, e.pos.1 + 0.06, e.pos.2),
                                super::mechanics::explosion::TNT_POWER,
                                true,
                            ));
                            removed.push((e.dimension, e.id));
                            continue;
                        }
                        physics(server, e, 0.04, 0.98);
                    }
                    Kind::Projectile(_) => {
                        if projectiles::tick(server, e, &players, &mut out.events) {
                            removed.push((e.dimension, e.id));
                            continue;
                        }
                    }
                    Kind::Mob(_) => {
                        mobs::tick(server, e, &players, now, &mut out);
                        if let Kind::Mob(m) = &e.kind
                            && m.dead_ticks > 20
                        {
                            removed.push((e.dimension, e.id));
                            continue;
                        }
                    }
                }
                if dist2(before.0, e.pos) > 1e-6 || before.1 != e.yaw {
                    out.events.push(Event::EntityMoved {
                        dimension: e.dimension,
                        id: e.id,
                        pos: e.pos,
                        yaw: e.yaw,
                        pitch: e.pitch,
                        on_ground: e.on_ground,
                    });
                }
            }
            for (_, id) in &removed {
                map.remove(id);
            }
        }
        for (dimension, id) in removed {
            out.events.push(Event::EntitiesRemoved {
                dimension,
                ids: vec![id],
            });
        }
        for e in out.events {
            server.publish(e);
        }
        for e in out.spawns {
            self.insert(server, e);
        }
        for (dim, at, item) in out.drops {
            self.drop_at_block(server, dim, at, item);
        }
        for (dim, at, power, drops_all) in out.explosions {
            super::mechanics::explosion::explode(server, dim, at, power, drops_all);
        }
        super::mechanics::tick(server, now as u64, &players);
        super::spawning::tick(server, &players, now);
    }
}

pub fn dist2(a: Vec3, b: Vec3) -> f64 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)
}

/// 몸(반쪽 폭 `hw`, 키 `h`)이 (x, y, z)에 있으면 단단한 블록과 겹치나.
fn overlaps(server: &Server, dim: Dimension, x: f64, y: f64, z: f64, hw: f64, h: f64) -> bool {
    let (y0, y1) = ((y + 0.01).floor() as i32, (y + h - 0.01).floor() as i32);
    let (x0, x1) = ((x - hw).floor() as i32, (x + hw).floor() as i32);
    let (z0, z1) = ((z - hw).floor() as i32, (z + hw).floor() as i32);
    for by in y0..=y1 {
        for bx in x0..=x1 {
            for bz in z0..=z1 {
                if server.world.is_solid(dim, bx, by, bz) {
                    return true;
                }
            }
        }
    }
    false
}

/// 중력과 마찰, 땅에 닿기, 옆 벽에 막히기. 바닥은 몸 가운데 아래 칸만 봅니다.
pub fn physics(server: &Server, e: &mut Entity, gravity: f64, drag: f64) {
    let world = &server.world;
    let cell = |v: f64| v.floor() as i32;
    // 블록 안에 끼었으면 위로 밀어 올리기
    if world.is_solid(e.dimension, cell(e.pos.0), cell(e.pos.1), cell(e.pos.2)) {
        e.pos.1 = e.pos.1.floor() + 1.0;
        e.vel.1 = 0.0;
    }
    e.vel.1 -= gravity;
    // 옆으로: x와 z를 따로 막습니다 (벽을 따라 미끄러지게).
    let (hw, h) = e.size();
    e.collided = false;
    let y = e.pos.1.max(e.pos.1 + e.vel.1);
    if e.vel.0 != 0.0
        && overlaps(server, e.dimension, e.pos.0 + e.vel.0, y, e.pos.2, hw, h)
        && !overlaps(server, e.dimension, e.pos.0, y, e.pos.2, hw, h)
    {
        e.vel.0 = 0.0;
        e.collided = true;
    }
    if e.vel.2 != 0.0
        && overlaps(
            server,
            e.dimension,
            e.pos.0 + e.vel.0,
            y,
            e.pos.2 + e.vel.2,
            hw,
            h,
        )
        && !overlaps(server, e.dimension, e.pos.0 + e.vel.0, y, e.pos.2, hw, h)
    {
        e.vel.2 = 0.0;
        e.collided = true;
    }
    let next = (e.pos.0 + e.vel.0, e.pos.1 + e.vel.1, e.pos.2 + e.vel.2);
    if e.vel.1 > 0.0 && world.is_solid(e.dimension, cell(next.0), cell(next.1 + h), cell(next.2)) {
        // 천장
        e.vel.1 = 0.0;
        e.pos = (next.0, e.pos.1, next.2);
        e.on_ground = false;
    } else if e.vel.1 <= 0.0
        && world.is_solid(e.dimension, cell(next.0), cell(next.1), cell(next.2))
    {
        e.pos = (next.0, next.1.floor() + 1.0, next.2);
        e.vel = (e.vel.0 * 0.6, 0.0, e.vel.2 * 0.6);
        e.on_ground = true;
    } else {
        e.pos = next;
        e.on_ground = false;
    }
    e.vel = (e.vel.0 * drag, e.vel.1 * 0.98, e.vel.2 * drag);
    if e.vel.0.abs() < 1e-3 {
        e.vel.0 = 0.0;
    }
    if e.vel.2.abs() < 1e-3 {
        e.vel.2 = 0.0;
    }
    // 공허 아래로 떨어진 것
    if e.pos.1 < (e.dimension.min_y() - 64) as f64 {
        e.age = u32::MAX / 2;
    }
}

/// 서버가 켜져 있는 동안 틱을 돌립니다.
pub async fn run(server: std::sync::Arc<Server>) {
    let mut interval = tokio::time::interval(super::TICK);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let s = server.clone();
        // 블록 조회가 지형 생성을 부를 수 있으므로 비동기 스레드를 막지 않게.
        if tokio::task::spawn_blocking(move || s.entities.tick(&s))
            .await
            .is_err()
        {
            eprintln!("엔티티 틱이 실패했습니다");
        }
    }
}
