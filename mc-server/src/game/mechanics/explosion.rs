//! 폭발과 TNT.
//!
//! TNT는 불을 붙이거나(부싯돌) 레드스톤 신호를 받으면 불붙은 TNT 엔티티가 되어 4초 뒤
//! 터집니다. 폭발은 둥글게(가장자리는 들쭉날쭉) 블록을 부수고 — 흑요석·기반암 등은 버팀 —
//! 근처 플레이어와 몹에게 거리에 따라 피해를 주고, 근처 TNT를 잇달아 터뜨립니다.

use crate::data::{block, items};
use crate::game::entities::{Entity, Kind, Vec3};
use crate::packets::world::BlockPos;
use crate::server::{Event, Server};
use crate::world::Dimension;
use crate::world::blockset::name_of;

/// 불붙은 TNT가 터지기까지 (4초).
pub const TNT_FUSE: u32 = 80;
pub const TNT_POWER: f32 = 4.0;

/// 폭발에 버티는 블록.
fn resists(name: &str) -> bool {
    matches!(
        name,
        "bedrock"
            | "obsidian"
            | "crying_obsidian"
            | "end_portal_frame"
            | "end_portal"
            | "end_gateway"
            | "reinforced_deepslate"
            | "barrier"
            | "command_block"
            | "respawn_anchor"
            | "ancient_debris"
            | "netherite_block"
            | "enchanting_table"
            | "ender_chest"
            | "water"
            | "lava"
            | "trial_spawner"
            | "vault"
            | "structure_block"
            | "jigsaw"
            | "light"
    ) || name.ends_with("anvil")
}

/// TNT 블록에 불을 붙입니다.
pub fn prime(server: &Server, dim: Dimension, pos: BlockPos) {
    if name_of(server.world.get(dim, pos.0, pos.1, pos.2)) != "tnt" {
        return;
    }
    server.set_block(dim, pos, block::AIR);
    spawn_primed(
        server,
        dim,
        (pos.0 as f64 + 0.5, pos.1 as f64, pos.2 as f64 + 0.5),
        TNT_FUSE,
    );
    server.play_sound(dim, pos, "entity.tnt.primed");
}

pub fn spawn_primed(server: &Server, dim: Dimension, at: Vec3, fuse: u32) {
    let a = crate::game::rand_f64() * std::f64::consts::TAU;
    server.entities.insert(
        server,
        Entity {
            id: server.allocate_entity_id(),
            uuid: u128::from_le_bytes(crate::crypto::random::bytes::<16>()),
            dimension: dim,
            pos: at,
            vel: (-a.sin() * 0.02, 0.2, -a.cos() * 0.02),
            yaw: 0.0,
            pitch: 0.0,
            on_ground: false,
            age: 0,
            kind: Kind::Tnt { fuse },
        },
    );
}

/// 엔티티 틱에서는 할 일이 없습니다 (TNT 엔티티가 스스로 셈).
pub fn tick(_server: &Server, _now: u64) {}

/// 터뜨립니다. `drops_all`이면 부서진 블록이 모두 떨어지고(TNT), 아니면 1/세기 확률(크리퍼).
pub fn explode(server: &Server, dim: Dimension, center: Vec3, power: f32, drops_all: bool) {
    let r = (power * 1.3) as f64;
    let ri = r.ceil() as i32;
    let c = (
        center.0.floor() as i32,
        center.1.floor() as i32,
        center.2.floor() as i32,
    );
    let mut broken = 0;
    let mut chain = Vec::new();
    for dx in -ri..=ri {
        for dy in -ri..=ri {
            for dz in -ri..=ri {
                let d = ((dx * dx + dy * dy + dz * dz) as f64).sqrt();
                // 가장자리는 들쭉날쭉하게
                if d > r * (0.7 + crate::game::rand_f64() * 0.3) {
                    continue;
                }
                let p = (c.0 + dx, c.1 + dy, c.2 + dz);
                if p.1 < dim.min_y() {
                    continue;
                }
                let s = server.world.get(dim, p.0, p.1, p.2);
                let n = name_of(s);
                if s == block::AIR || resists(n) {
                    continue;
                }
                if n == "tnt" {
                    chain.push(p);
                    continue;
                }
                server.set_block_broken(dim, p, block::AIR, s);
                broken += 1;
                if drops_all || crate::game::rand_f64() < 1.0 / power as f64 {
                    let roll = crate::game::rand_u32();
                    if let Some(drop) = items::block_drop(s, roll % 1000, roll >> 10) {
                        server.entities.drop_at_block(server, dim, p, drop);
                    }
                }
                if matches!(
                    n,
                    "chest" | "barrel" | "trapped_chest" | "furnace" | "smoker" | "blast_furnace"
                ) {
                    for item in server.containers.take_all(dim, p) {
                        server.entities.drop_at_block(server, dim, p, item);
                    }
                }
            }
        }
    }
    // 이어서 터지는 TNT (짧은 심지)
    for p in chain {
        server.set_block(dim, p, block::AIR);
        let fuse = 10 + crate::game::rand_u32() % 20;
        spawn_primed(
            server,
            dim,
            (p.0 as f64 + 0.5, p.1 as f64, p.2 as f64 + 0.5),
            fuse,
        );
    }

    // 피해: 플레이어와 몹 (거리 2×세기 안, 가까울수록 셈)
    let reach = 2.0 * power as f64;
    for p in server.players() {
        if p.dimension != dim || p.dead {
            continue;
        }
        let d = crate::game::entities::dist2(p.position, center).sqrt();
        if d >= reach {
            continue;
        }
        let impact = 1.0 - d / reach;
        let amount = ((impact * impact + impact) / 2.0 * 7.0 * reach + 1.0) as f32;
        server.publish(Event::Damage {
            target: p.uuid,
            amount,
            damage_type: "explosion",
            attacker: None,
            attacker_name: String::new(),
        });
    }
    for e in server.entities.snapshots_in(dim) {
        let d = crate::game::entities::dist2(e.pos, center).sqrt();
        if d < reach && e.item.is_none() {
            let impact = 1.0 - d / reach;
            let amount = ((impact * impact + impact) / 2.0 * 7.0 * reach + 1.0) as f32;
            crate::game::mobs::damage(server, e.id, amount, Some(center), None);
        }
    }
    server.publish(Event::Explosion {
        dimension: dim,
        center,
        power,
    });
    let _ = broken;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::status::ServerInfo;

    #[test]
    fn explosion_breaks_stone_but_not_obsidian() {
        let s = Server::new(
            ServerInfo {
                name: "t".into(),
                description: "t".into(),
                max_players: 1,
            },
            3,
        );
        let d = Dimension::Overworld;
        for x in -3..=3 {
            for y in 197..=203 {
                for z in -3..=3 {
                    s.world.set(d, x, y, z, block::STONE);
                }
            }
        }
        s.world
            .set(d, 1, 200, 0, crate::world::blockset::b("obsidian"));
        explode(&s, d, (0.5, 200.5, 0.5), TNT_POWER, true);
        assert_eq!(s.world.get(d, 0, 200, 0), block::AIR);
        assert_eq!(name_of(s.world.get(d, 1, 200, 0)), "obsidian");
    }
}
