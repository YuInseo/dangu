//! 살아 있는 월드: 물이 흐르고, 레버를 당기면 램프가 켜지는 것을 실제 패킷으로.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::data::blocks;
use mc_server::packets::world::{pack_position, unpack_position};
use mc_server::protocol::varint::{read_varint, write_varint};
use mc_server::world::Dimension;
use mc_server::world::blockset::{b, bp, name_of};

const BLOCK_UPDATE: i32 = 0x08;

fn block_update(body: &[u8]) -> ((i32, i32, i32), u32) {
    let pos = unpack_position(i64::from_be_bytes(body[..8].try_into().unwrap()));
    let mut rest = &body[8..];
    (pos, read_varint(&mut rest).unwrap() as u32)
}

#[tokio::test]
async fn water_flows_and_lever_lights_lamp() {
    let server = Arc::new(Server::new(test_info(), SEED));
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut alice, _) = join(addr, "Alice", 2).await;
    let d = Dimension::Overworld;
    let spawn = mc_server::world::terrain::spawn_point(d, SEED);
    let (x0, y, z0) = (spawn.0 as i32 + 2, spawn.1 as i32 + 6, spawn.2 as i32 + 2);

    // 공중 받침 위에 물 원천 → 옆으로 흘러 나감
    for dx in -3..=3 {
        for dz in -3..=3 {
            server.world.set(d, x0 + dx, y - 1, z0 + dz, b("stone"));
            server
                .world
                .set(d, x0 + dx, y, z0 + dz, mc_server::data::block::AIR);
        }
    }
    server.set_block(d, (x0, y, z0), b("water"));
    recv_where(&mut alice, |p| {
        p.id == BLOCK_UPDATE && {
            let (pos, s) = block_update(&p.body);
            pos == (x0 + 1, y, z0)
                && name_of(s) == "water"
                && blocks::property(s, "level") == Some("1")
        }
    })
    .await;

    // 레버 옆 램프: 플레이어가 레버를 누르면 켜짐
    let (lx, lz) = (x0 + 5, z0);
    server.world.set(d, lx, y - 1, lz, b("stone"));
    server.world.set(d, lx + 1, y - 1, lz, b("stone"));
    server.set_block(
        d,
        (lx, y, lz),
        bp("lever", &[("face", "floor"), ("powered", "false")]),
    );
    server.set_block(d, (lx + 1, y, lz), bp("redstone_lamp", &[("lit", "false")]));
    let mut use_on = Vec::new();
    write_varint(&mut use_on, 0);
    use_on.extend_from_slice(&pack_position((lx, y, lz)).to_be_bytes());
    write_varint(&mut use_on, 1);
    for f in [0.5f32, 0.1, 0.5] {
        use_on.extend_from_slice(&f.to_be_bytes());
    }
    use_on.extend_from_slice(&[0, 0]);
    write_varint(&mut use_on, 3);
    // 손이 닿도록 가까이 감
    let mut mv = Vec::new();
    for v in [lx as f64 + 0.5, y as f64, lz as f64 - 1.5] {
        mv.extend_from_slice(&v.to_be_bytes());
    }
    mv.push(1);
    send(&mut alice, 0x1E, &mv).await;
    send(&mut alice, 0x42, &use_on).await;
    recv_where(&mut alice, |p| {
        p.id == BLOCK_UPDATE && {
            let (pos, s) = block_update(&p.body);
            pos == (lx + 1, y, lz)
                && name_of(s) == "redstone_lamp"
                && blocks::property(s, "lit") == Some("true")
        }
    })
    .await;
}
