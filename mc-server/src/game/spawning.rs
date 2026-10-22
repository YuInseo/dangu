//! 몹이 생기는 곳.
//!
//! - 지상: 낮에는 풀밭에 순한 몹, 밤에는 적대 몹. 사막은 좀비 대신 허스크, 눈 덮인 곳은
//!   스켈레톤 대신 스트레이 (바닐라처럼 80%).
//! - 동굴: 지표에서 8칸 넘게 아래, 빛 내는 블록이 가까이 없는 곳에 낮밤 없이 적대 몹.
//! - 네더: 좀비 피글린, 요새 근처에서는 위더 스켈레톤·블레이즈. 엔드: 엔더맨.
//! - 생성기(스포너): 구조물에 놓인 생성기가 16칸 안에 사람이 있으면 10~40초마다 1~4마리.
//! - 마을: 처음 다가가면 주민을 채웁니다 (마을 종류에 맞는 옷).
//!
//! 팬텀은 어디서도 생기지 않습니다.

use super::entities::{Kind, Vec3, dist2};
use super::mobs::{self, Mob, MobKind};
use super::villagers::{PROFESSIONS, Villager};
use super::{rand_f64, rand_u32};
use crate::data::block;
use crate::packets::world::BlockPos;
use crate::server::{Difficulty, PlayerState, Server};
use crate::world::blockset::name_of;
use crate::world::structure;
use crate::world::{Dimension, biome, terrain};

const MAX_MOBS: usize = 120;
/// 한 사람 주변 64칸 안의 상한.
const HOSTILE_CAP: usize = 8;
const PASSIVE_CAP: usize = 8;
const SPAWNER_RANGE: f64 = 16.0;
/// 마을에 다가갔다고 볼 거리 (청크).
const VILLAGE_REACH: i32 = 5;

pub fn tick(server: &Server, players: &[PlayerState], now: u32) {
    if !server.config.spawn_mobs {
        return;
    }
    if now.is_multiple_of(20) {
        spawners(server, players, now);
    }
    if now.is_multiple_of(100) {
        villages(server, players);
        natural(server, players);
    }
}

struct Census {
    mobs: Vec<(Dimension, Vec3, MobKind)>,
}

impl Census {
    fn take(server: &Server) -> Self {
        let mobs = server
            .entities
            .map
            .lock()
            .unwrap()
            .values()
            .filter_map(|e| match &e.kind {
                Kind::Mob(m) if !m.dead() => Some((e.dimension, e.pos, m.kind)),
                _ => None,
            })
            .collect();
        Self { mobs }
    }

    fn near(&self, dim: Dimension, at: Vec3, r: f64, f: impl Fn(MobKind) -> bool) -> usize {
        self.mobs
            .iter()
            .filter(|(d, p, k)| *d == dim && f(*k) && dist2(*p, at) < r * r)
            .count()
    }
}

/// 그 칸에 몹이 설 수 있나 (실제 블록으로: 플레이어가 바꿨을 수 있음).
fn fits(server: &Server, dim: Dimension, x: i32, y: i32, z: i32) -> bool {
    server.world.is_solid(dim, x, y - 1, z)
        && !server.world.is_solid(dim, x, y, z)
        && !server.world.is_solid(dim, x, y + 1, z)
        && !matches!(
            name_of(server.world.get(dim, x, y - 1, z)),
            "lava" | "magma_block" | "bedrock"
        )
        && name_of(server.world.get(dim, x, y, z)) != "water"
}

/// 지표 부근에서 실제로 설 자리 (구조물·건물 위 포함).
fn surface(server: &Server, dim: Dimension, x: i32, z: i32) -> Option<i32> {
    let guess = terrain::standing_y(dim, server.seed, x, z)?;
    (guess - 4..guess + 24)
        .rev()
        .find(|&y| fits(server, dim, x, y, z))
}

fn emits_light(n: &str) -> bool {
    n.contains("torch")
        || n.contains("lantern")
        || n.contains("campfire")
        || matches!(
            n,
            "glowstone"
                | "lava"
                | "sea_lantern"
                | "shroomlight"
                | "jack_o_lantern"
                | "fire"
                | "soul_fire"
                | "beacon"
                | "end_rod"
                | "froglight"
                | "redstone_lamp"
        )
        || n.ends_with("froglight")
}

/// 근처 빛 내는 블록이 없나 (빛 계산 대신).
fn dark(server: &Server, dim: Dimension, x: i32, y: i32, z: i32) -> bool {
    for dx in -5..=5 {
        for dy in -3..=3 {
            for dz in -5..=5 {
                if emits_light(name_of(server.world.get(dim, x + dx, y + dy, z + dz))) {
                    return false;
                }
            }
        }
    }
    true
}

fn pick<T: Copy>(weighted: &[(T, u32)]) -> T {
    let total: u32 = weighted.iter().map(|w| w.1).sum();
    let mut r = rand_u32() % total.max(1);
    for &(t, w) in weighted {
        if r < w {
            return t;
        }
        r -= w;
    }
    weighted[0].0
}

/// 밤 지상·동굴의 적대 몹 (바이옴에 따라 바뀜).
fn overworld_hostile(biome_name: &str, snowy: bool) -> MobKind {
    let k = pick(&[
        (MobKind::Zombie, 95),
        (MobKind::Skeleton, 100),
        (MobKind::Spider, 100),
        (MobKind::Creeper, 100),
        (MobKind::Enderman, 10),
    ]);
    let swap = !rand_u32().is_multiple_of(5);
    match k {
        MobKind::Zombie if swap && biome_name == "desert" => MobKind::Husk,
        MobKind::Skeleton if swap && (snowy || biome_name.contains("frozen")) => MobKind::Stray,
        k => k,
    }
}

fn near_structure(
    server: &Server,
    dim: Dimension,
    x: i32,
    z: i32,
    name: &str,
    chunks: i32,
) -> bool {
    structure::starts_near(server.seed, dim, x >> 4, z >> 4)
        .iter()
        .any(|s| {
            s.structure == name
                && (s.chunk.0 - (x >> 4)).abs() <= chunks
                && (s.chunk.1 - (z >> 4)).abs() <= chunks
        })
}

fn natural(server: &Server, players: &[PlayerState]) {
    let census = Census::take(server);
    if census.mobs.len() >= MAX_MOBS {
        return;
    }
    let peaceful = server.difficulty() == Difficulty::Peaceful;
    let night = mobs::is_night(server);
    for p in players.iter().filter(|p| !p.dead) {
        let dim = p.dimension;
        let hostile_near = census.near(dim, p.position, 64.0, MobKind::hostile);
        let passive_near = census.near(dim, p.position, 64.0, |k| MobKind::PASSIVE.contains(&k));
        let angle = rand_f64() * std::f64::consts::TAU;
        let r = 24.0 + rand_f64() * 16.0;
        let (x, z) = (
            (p.position.0 + angle.cos() * r).floor() as i32,
            (p.position.2 + angle.sin() * r).floor() as i32,
        );
        match dim {
            Dimension::Overworld => {
                let bio = structure::biome_at(dim, server.seed, x, z);
                let surface_y = biome::surface_height(server.seed, x, z);
                // 동굴: 사람 높이 근처, 지표보다 충분히 아래
                if !peaceful && hostile_near < HOSTILE_CAP && p.position.1 < (surface_y - 8) as f64
                {
                    let y0 = p.position.1 as i32 + (rand_u32() % 17) as i32 - 8;
                    if let Some(y) = (y0 - 8..=y0 + 8).find(|&y| fits(server, dim, x, y, z))
                        && y < surface_y - 8
                        && dark(server, dim, x, y, z)
                    {
                        let k = overworld_hostile("", false);
                        mobs::spawn_natural(
                            server,
                            k,
                            dim,
                            (x as f64 + 0.5, y as f64, z as f64 + 0.5),
                        );
                        continue;
                    }
                }
                let Some(y) = surface(server, dim, x, z) else {
                    continue;
                };
                if night && !peaceful && hostile_near < HOSTILE_CAP {
                    if dark(server, dim, x, y, z) {
                        let def = bio.def();
                        let k = overworld_hostile(def.name, def.snowy);
                        mobs::spawn_natural(
                            server,
                            k,
                            dim,
                            (x as f64 + 0.5, y as f64, z as f64 + 0.5),
                        );
                    }
                } else if passive_near < PASSIVE_CAP
                    && server.world.get(dim, x, y - 1, z) == block::GRASS_BLOCK
                {
                    let k = MobKind::PASSIVE[rand_u32() as usize % MobKind::PASSIVE.len()];
                    // 바닐라처럼 무리 지어 (2~4마리)
                    for i in 0..2 + rand_u32() % 3 {
                        let (gx, gz) = (x + (i as i32 % 2) * 2, z + (i as i32 / 2) * 2);
                        if let Some(gy) = surface(server, dim, gx, gz) {
                            mobs::spawn_natural(
                                server,
                                k,
                                dim,
                                (gx as f64 + 0.5, gy as f64, gz as f64 + 0.5),
                            );
                        }
                    }
                }
            }
            Dimension::Nether if !peaceful && hostile_near < HOSTILE_CAP => {
                let y0 = (p.position.1 as i32 + (rand_u32() % 33) as i32 - 16).clamp(5, 120);
                let Some(y) = (y0 - 10..=y0 + 10).find(|&y| fits(server, dim, x, y, z)) else {
                    continue;
                };
                let k = if near_structure(server, dim, x, z, "fortress", 6) {
                    pick(&[
                        (MobKind::WitherSkeleton, 8),
                        (MobKind::Blaze, 10),
                        (MobKind::ZombifiedPiglin, 5),
                    ])
                } else {
                    pick(&[(MobKind::ZombifiedPiglin, 10), (MobKind::Enderman, 1)])
                };
                mobs::spawn_natural(server, k, dim, (x as f64 + 0.5, y as f64, z as f64 + 0.5));
            }
            Dimension::End if !peaceful && hostile_near < HOSTILE_CAP => {
                if let Some(y) = surface(server, dim, x, z) {
                    mobs::spawn_natural(
                        server,
                        MobKind::Enderman,
                        dim,
                        (x as f64 + 0.5, y as f64, z as f64 + 0.5),
                    );
                }
            }
            _ => {}
        }
    }
}

/// 구조물 생성기: 사람이 16칸 안에 있으면 돌아갑니다.
fn spawners(server: &Server, players: &[PlayerState], now: u32) {
    let peaceful = server.difficulty() == Difficulty::Peaceful;
    let mut found: Vec<(Dimension, BlockPos, MobKind)> = Vec::new();
    for p in players.iter().filter(|p| !p.dead) {
        let (pcx, pcz) = (
            p.position.0.floor() as i32 >> 4,
            p.position.2.floor() as i32 >> 4,
        );
        for cx in pcx - 1..=pcx + 1 {
            for cz in pcz - 1..=pcz + 1 {
                let Some(col) = server.world.chunk_if_loaded(p.dimension, cx, cz) else {
                    continue;
                };
                for &(pos, name) in &col.spawners {
                    let centre = (pos.0 as f64 + 0.5, pos.1 as f64 + 0.5, pos.2 as f64 + 0.5);
                    if dist2(centre, p.position) > SPAWNER_RANGE * SPAWNER_RANGE {
                        continue;
                    }
                    if let Some(k) = MobKind::by_name(name)
                        && !found.iter().any(|f| f.0 == p.dimension && f.1 == pos)
                    {
                        found.push((p.dimension, pos, k));
                    }
                }
            }
        }
    }
    if found.is_empty() || peaceful {
        return;
    }
    let census = Census::take(server);
    for (dim, pos, kind) in found {
        let due = {
            let mut timers = server.entities.spawners.lock().unwrap();
            let t = timers
                .entry((dim, pos))
                .or_insert(now + 20 + rand_u32() % 180);
            if now < *t {
                false
            } else {
                *t = now + 200 + rand_u32() % 600;
                true
            }
        };
        if !due || name_of(server.world.get(dim, pos.0, pos.1, pos.2)) != "spawner" {
            continue;
        }
        let centre = (pos.0 as f64 + 0.5, pos.1 as f64, pos.2 as f64 + 0.5);
        if census.near(dim, centre, 9.0, |k| k == kind) >= 6 {
            continue;
        }
        for _ in 0..1 + rand_u32() % 4 {
            let (x, z) = (
                pos.0 + (rand_u32() % 9) as i32 - 4,
                pos.2 + (rand_u32() % 9) as i32 - 4,
            );
            if let Some(y) = (pos.1 - 1..=pos.1 + 1).find(|&y| fits(server, dim, x, y, z)) {
                mobs::spawn(
                    server,
                    kind,
                    dim,
                    (x as f64 + 0.5, y as f64, z as f64 + 0.5),
                );
                server.play_sound(dim, pos, "block.fire.extinguish");
            }
        }
    }
}

/// 마을 종류 → 주민 옷 (바이옴 종류).
fn village_variant(structure: &str) -> &'static str {
    match structure {
        "village_desert" => "desert",
        "village_savanna" => "savanna",
        "village_snowy" => "snow",
        "village_taiga" => "taiga",
        _ => "plains",
    }
}

fn villages(server: &Server, players: &[PlayerState]) {
    for p in players
        .iter()
        .filter(|p| p.dimension == Dimension::Overworld && !p.dead)
    {
        let (pcx, pcz) = (
            p.position.0.floor() as i32 >> 4,
            p.position.2.floor() as i32 >> 4,
        );
        for start in structure::starts_near(server.seed, Dimension::Overworld, pcx, pcz) {
            if !start.structure.starts_with("village")
                || (start.chunk.0 - pcx).abs() > VILLAGE_REACH
                || (start.chunk.1 - pcz).abs() > VILLAGE_REACH
            {
                continue;
            }
            if !server.entities.villages.lock().unwrap().insert((
                Dimension::Overworld,
                start.chunk.0,
                start.chunk.1,
            )) {
                continue;
            }
            populate(server, start.chunk, village_variant(start.structure));
        }
    }
}

/// 마을 하나에 주민 4~6명 (한 명은 무직).
pub fn populate(server: &Server, chunk: (i32, i32), variant: &str) -> usize {
    let dim = Dimension::Overworld;
    let home = (chunk.0 * 16 + 8, 0, chunk.1 * 16 + 8);
    let n = 4 + rand_u32() as usize % 3;
    let mut spawned = 0;
    // 물가·지붕 위처럼 설 자리가 없는 곳은 다시 골라 봅니다.
    for _ in 0..n * 6 {
        if spawned >= n {
            break;
        }
        let i = spawned;
        let (x, z) = (
            home.0 + (rand_u32() % 25) as i32 - 12,
            home.2 + (rand_u32() % 25) as i32 - 12,
        );
        let Some(y) = surface(server, dim, x, z) else {
            continue;
        };
        let mut m = Mob::new(MobKind::Villager);
        m.villager = Some(if i == 0 {
            Villager::new("none", variant)
        } else {
            Villager::new(
                PROFESSIONS[rand_u32() as usize % PROFESSIONS.len()],
                variant,
            )
        });
        m.home = Some((home.0, y, home.2));
        mobs::spawn_mob(server, m, dim, (x as f64 + 0.5, y as f64, z as f64 + 0.5));
        spawned += 1;
    }
    spawned
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::status::ServerInfo;
    use crate::world::blockset::b;

    #[test]
    fn spawner_fills_its_room() {
        let s = Server::new(
            ServerInfo {
                name: "t".into(),
                description: "t".into(),
                max_players: 1,
            },
            3,
        );
        let d = Dimension::Overworld;
        let y = 200;
        for x in -5..=5 {
            for z in -5..=5 {
                s.world.set(d, x, y - 1, z, block::STONE);
                for dy in 0..4 {
                    s.world.set(d, x, y + dy, z, block::AIR);
                }
            }
        }
        s.world.set(d, 0, y, 0, b("spawner"));
        // 청크에 생성기를 적어 둠 (구조물이 지을 때처럼)
        s.world
            .edit_chunk(d, 0, 0, |col| col.spawners.push(((0, y, 0), "skeleton")));
        let player = PlayerState::test(d, (3.0, y as f64, 3.0));
        for t in 0..=400 {
            spawners(&s, std::slice::from_ref(&player), t);
        }
        let census = Census::take(&s);
        assert!(census.near(d, (0.5, y as f64, 0.5), 9.0, |k| k == MobKind::Skeleton) > 0);
    }

    #[test]
    fn villages_get_villagers_with_matching_clothes() {
        let s = Server::new(
            ServerInfo {
                name: "t".into(),
                description: "t".into(),
                max_players: 1,
            },
            3,
        );
        let (cx, cz, name) =
            structure::locate(3, Dimension::Overworld, "village", (0, 0), 100).expect("a village");
        let home = (cx, cz);
        let surface_ok = (-12..=12)
            .filter(|d| surface(&s, Dimension::Overworld, home.0 + d, home.1).is_some())
            .count();
        let spawned = populate(&s, (cx >> 4, cz >> 4), village_variant(name));
        assert!(
            spawned >= 3,
            "{spawned} villagers, {surface_ok}/25 standable at {home:?} ({name})"
        );
        let villagers: Vec<_> = s
            .entities
            .map
            .lock()
            .unwrap()
            .values()
            .filter_map(|e| match &e.kind {
                Kind::Mob(m) => m.villager.clone(),
                _ => None,
            })
            .collect();
        assert_eq!(villagers.len(), spawned);
        assert!(villagers.iter().filter(|v| v.trades()).count() >= spawned - 1);
    }
}
