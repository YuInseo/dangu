//! 블록 놓기·부수기가 다른 사람에게 보이고, 저장되는지.

mod common;

use std::sync::Arc;

use common::{SEED, join, recv_where, send, start_server_with, test_info};
use mc_server::Server;
use mc_server::data::{block, blocks};
use mc_server::packets::world::{pack_position, unpack_position};
use mc_server::protocol::varint::{read_varint, write_varint};
use mc_server::world::{Dimension, terrain};

const BLOCK_CHANGED_ACK: i32 = 0x04;
const BLOCK_UPDATE: i32 = 0x08;
const LEVEL_EVENT: i32 = 0x2F;
const SET_EQUIPMENT: i32 = 0x68;
const SWING_ANIMATION: i32 = 0x7B;

fn block_update(body: &[u8]) -> ((i32, i32, i32), i32) {
    let pos = unpack_position(i64::from_be_bytes(body[..8].try_into().unwrap()));
    let mut b = &body[8..];
    (pos, read_varint(&mut b).unwrap())
}

#[tokio::test]
async fn place_and_break_are_shared_and_saved() {
    let dir = std::env::temp_dir().join(format!("dangu-building-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let server = Arc::new(Server::open(test_info(), SEED, dir.clone()).unwrap());
    let addr = start_server_with(Arc::clone(&server)).await;

    let (mut alice, _) = join(addr, "Alice", 2).await;
    let (mut bob, _) = join(addr, "Bob", 2).await;
    common::send_string(&mut alice, 0x07, "gamemode creative").await;

    // Alice가 크리에이티브 인벤토리에서 핫바 첫 칸(메뉴 36번)에 돌(아이템 1)을 넣습니다.
    let mut slot = 36i16.to_be_bytes().to_vec();
    for v in [64, 1, 0, 0] {
        write_varint(&mut slot, v);
    }
    send(&mut alice, 0x39, &slot).await;
    let eq = recv_where(&mut bob, |p| p.id == SET_EQUIPMENT).await;
    // 주 손(0x80 = 뒤에 더 있음) 돌 64개, 보조 손(1)과 갑옷 넷(2..5)은 비어 있음
    assert_eq!(
        eq.body[eq.body.len() - 15..],
        [0x80, 64, 1, 0, 0, 0x81, 0, 0x82, 0, 0x83, 0, 0x84, 0, 5, 0]
    );

    // 발밑 블록의 윗면을 누릅니다 → 발 자리에 돌.
    let (sx, sy, sz) = terrain::spawn_point(Dimension::Overworld, SEED);
    let under = (sx.floor() as i32, sy as i32 - 1, sz.floor() as i32);
    let mut use_on = Vec::new();
    write_varint(&mut use_on, 0); // 주 손
    use_on.extend_from_slice(&pack_position(under).to_be_bytes());
    write_varint(&mut use_on, 1); // 윗면
    for f in [0.5f32, 1.0, 0.5] {
        use_on.extend_from_slice(&f.to_be_bytes());
    }
    use_on.extend_from_slice(&[0, 0]);
    write_varint(&mut use_on, 7); // 순번
    send(&mut alice, 0x42, &use_on).await;

    let ack = recv_where(&mut alice, |p| p.id == BLOCK_CHANGED_ACK).await;
    assert_eq!(ack.body, [7]);
    let placed = (under.0, under.1 + 1, under.2);
    let update = recv_where(&mut bob, |p| p.id == BLOCK_UPDATE).await;
    assert_eq!(block_update(&update.body), (placed, block::STONE as i32));
    recv_where(&mut bob, |p| p.id == SWING_ANIMATION).await;
    assert_eq!(
        server
            .world
            .get(Dimension::Overworld, placed.0, placed.1, placed.2),
        block::STONE
    );

    // 크리에이티브에서 부수기 시작 = 바로 부서짐. Bob은 소리·조각(2001)도 받습니다.
    let mut dig = Vec::new();
    write_varint(&mut dig, 0);
    dig.extend_from_slice(&pack_position(placed).to_be_bytes());
    dig.push(1);
    write_varint(&mut dig, 8);
    send(&mut alice, 0x29, &dig).await;
    let update = recv_where(&mut bob, |p| p.id == BLOCK_UPDATE).await;
    assert!(blocks::is_air(block_update(&update.body).1 as u32));
    let event = recv_where(&mut bob, |p| p.id == LEVEL_EVENT).await;
    assert_eq!(
        i32::from_be_bytes(event.body[..4].try_into().unwrap()),
        2001
    );

    // 휘두르기
    send(&mut alice, 0x2E, &[]).await;
    recv_where(&mut bob, |p| p.id == SWING_ANIMATION).await;

    // 저장했다가 다시 열면 남아 있습니다.
    assert!(server.world.save().unwrap());
    let reopened = Server::open(test_info(), SEED, dir.clone()).unwrap();
    assert!(blocks::is_air(reopened.world.get(
        Dimension::Overworld,
        placed.0,
        placed.1,
        placed.2
    )));
    // 바닐라 Anvil 영역 파일로 저장됨
    let region = dir.join(format!("region/r.{}.{}.mca", placed.0 >> 9, placed.2 >> 9));
    assert!(region.exists(), "{}", region.display());
    std::fs::remove_dir_all(&dir).unwrap();
}
