//! 상자 내용과 플레이어 인벤토리가 저장되는지.

mod common;

use std::sync::Arc;

use common::{SEED, join, recv_where, send, send_string, start_server_with, test_info};
use mc_server::Server;
use mc_server::data::items::by_name;
use mc_server::packets::world::pack_position;
use mc_server::protocol::varint::{read_varint, write_varint};
use mc_server::world::{Dimension, terrain};

fn use_on(pos: (i32, i32, i32), sequence: i32) -> Vec<u8> {
    let mut b = Vec::new();
    write_varint(&mut b, 0);
    b.extend_from_slice(&pack_position(pos).to_be_bytes());
    write_varint(&mut b, 1);
    for f in [0.5f32, 1.0, 0.5] {
        b.extend_from_slice(&f.to_be_bytes());
    }
    b.extend_from_slice(&[0, 0]);
    write_varint(&mut b, sequence);
    b
}

/// `Container Set Content`의 (창 번호, 칸별 (아이템, 개수)).
fn content(body: &[u8]) -> (i32, Vec<Option<(i32, i32)>>) {
    let mut b = body;
    let window = read_varint(&mut b).unwrap();
    read_varint(&mut b).unwrap();
    let n = read_varint(&mut b).unwrap();
    let mut slots = Vec::new();
    for _ in 0..n {
        let count = read_varint(&mut b).unwrap();
        if count == 0 {
            slots.push(None);
        } else {
            let id = read_varint(&mut b).unwrap();
            read_varint(&mut b).unwrap();
            read_varint(&mut b).unwrap();
            slots.push(Some((id, count)));
        }
    }
    (window, slots)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chest_and_inventory_persist() {
    let dir = std::env::temp_dir().join(format!("dangu-storage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let server = Arc::new(Server::open(test_info(), SEED, dir.clone()).unwrap());
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut s, _) = join(addr, "Saver", 2).await;

    // 상자를 들고 발밑 블록 위(발 자리)에 놓습니다.
    send_string(&mut s, 0x07, "give chest").await;
    send_string(&mut s, 0x07, "give diamond 5").await;
    recv_where(&mut s, |p| {
        p.id == 0x7C && String::from_utf8_lossy(&p.body).contains("diamond")
    })
    .await;
    let (sx, sy, sz) = terrain::spawn_point(Dimension::Overworld, SEED);
    let under = (sx.floor() as i32, sy as i32 - 1, sz.floor() as i32);
    send(&mut s, 0x42, &use_on(under, 1)).await;
    let chest_pos = (under.0, under.1 + 1, under.2);
    recv_where(&mut s, |p| p.id == 0x08).await; // Block Update

    // 상자를 눌러 열기 → 창 내용 (상자 27칸 + 인벤토리 36칸)
    send(&mut s, 0x42, &use_on(chest_pos, 2)).await;
    let open = recv_where(&mut s, |p| p.id == 0x3C).await;
    let window = read_varint(&mut open.body.as_slice()).unwrap();
    let (w, slots) = content(&recv_where(&mut s, |p| p.id == 0x12).await.body);
    assert_eq!((w, slots.len()), (window, 27 + 36));
    // 다이아몬드는 핫바 두 번째 칸(창 번호 27 + 27 + 1 = 55)
    let diamond = by_name("diamond").unwrap();
    assert_eq!(slots[55], Some((diamond, 5)));

    // 다이아몬드를 상자 첫 칸으로 옮기는 클릭
    let mut c = Vec::new();
    write_varint(&mut c, window);
    write_varint(&mut c, 0);
    c.extend_from_slice(&0i16.to_be_bytes());
    c.push(0);
    write_varint(&mut c, 0);
    write_varint(&mut c, 2);
    c.extend_from_slice(&55i16.to_be_bytes());
    c.push(0);
    c.extend_from_slice(&0i16.to_be_bytes());
    c.push(1);
    for v in [diamond, 5, 0, 0] {
        write_varint(&mut c, v);
    }
    c.push(0); // 손에 든 것 없음
    send(&mut s, 0x12, &c).await;
    let mut close = Vec::new();
    write_varint(&mut close, window);
    send(&mut s, 0x13, &close).await;
    recv_where(&mut s, |p| p.id == 0x12).await; // 닫은 뒤 인벤토리 다시 보냄

    assert_eq!(
        server.containers.get(Dimension::Overworld, chest_pos)[0].map(|i| (i.id, i.count)),
        Some((diamond, 5))
    );
    assert!(server.containers.save().unwrap());
    drop(s);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // 다시 열어도 상자와 인벤토리가 그대로 (상자 아이템은 인벤토리에서 빠짐)
    let reopened = Server::open(test_info(), SEED, dir.clone()).unwrap();
    assert_eq!(
        reopened.containers.get(Dimension::Overworld, chest_pos)[0].map(|i| (i.id, i.count)),
        Some((diamond, 5))
    );
    let saved =
        mc_server::game::playerdata::load(&dir, mc_server::uuid::offline_uuid("Saver")).unwrap();
    assert!(saved.inventory.iter().flatten().all(|i| i.id != diamond));
    std::fs::remove_dir_all(&dir).unwrap();
}
