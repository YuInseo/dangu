//! 서바이벌: 낙하 피해, 부수면 떨어지는 아이템과 줍기, 조합, 복제 거부, 좀비, 죽음과 부활.

mod common;

use common::{SEED, join, recv_where, send, send_string, start_server};
use mc_server::data::entity_type;
use mc_server::data::items::by_name;
use mc_server::packets::world::pack_position;
use mc_server::protocol::varint::{read_varint, write_varint};
use mc_server::world::{Dimension, terrain};

const ADD_ENTITY: i32 = 0x01;
const CONTAINER_SET_CONTENT: i32 = 0x12;
const CONTAINER_SET_SLOT: i32 = 0x14;
const DAMAGE_EVENT: i32 = 0x19;
const ENTITY_EVENT: i32 = 0x22;
const PLAYER_COMBAT_KILL: i32 = 0x45;
const RESPAWN: i32 = 0x54;
const SET_HEALTH: i32 = 0x6A;
const SET_PLAYER_INVENTORY: i32 = 0x6E;
const TAKE_ITEM_ENTITY: i32 = 0x7F;

fn health(body: &[u8]) -> f32 {
    f32::from_be_bytes(body[..4].try_into().unwrap())
}

fn move_to(x: f64, y: f64, z: f64, on_ground: bool) -> Vec<u8> {
    let mut b = Vec::new();
    for v in [x, y, z] {
        b.extend_from_slice(&v.to_be_bytes());
    }
    b.push(on_ground as u8);
    b
}

/// Slot 본문: 개수, 아이템, 컴포넌트 0/0. 해시된 슬롯이면 앞에 "있음" 바이트.
fn hashed(item: Option<(i32, i32)>) -> Vec<u8> {
    let mut b = Vec::new();
    match item {
        Some((id, count)) => {
            b.push(1);
            write_varint(&mut b, id);
            write_varint(&mut b, count);
            write_varint(&mut b, 0);
            write_varint(&mut b, 0);
        }
        None => b.push(0),
    }
    b
}

fn click(
    window: i32,
    slot: i16,
    changed: &[(i16, Option<(i32, i32)>)],
    carried: Option<(i32, i32)>,
) -> Vec<u8> {
    let mut b = Vec::new();
    write_varint(&mut b, window);
    write_varint(&mut b, 0);
    b.extend_from_slice(&slot.to_be_bytes());
    b.push(0);
    write_varint(&mut b, 0);
    write_varint(&mut b, changed.len() as i32);
    for (s, item) in changed {
        b.extend_from_slice(&s.to_be_bytes());
        b.extend(hashed(*item));
    }
    b.extend(hashed(carried));
    b
}

/// `Set Player Inventory`(칸, Slot)의 아이템 ID와 개수.
fn inventory_update(body: &[u8]) -> (i32, Option<(i32, i32)>) {
    let mut b = body;
    let slot = read_varint(&mut b).unwrap();
    let count = read_varint(&mut b).unwrap();
    if count == 0 {
        return (slot, None);
    }
    (slot, Some((read_varint(&mut b).unwrap(), count)))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn survival_loop() {
    let addr = start_server().await;
    let (mut s, play) = join(addr, "Steve", 2).await;
    let me = i32::from_be_bytes(
        play.iter().find(|p| p.id == 0x32).unwrap().body[..4]
            .try_into()
            .unwrap(),
    );
    let (sx, sy, sz) = terrain::spawn_point(Dimension::Overworld, SEED);

    // ---- 10칸 떨어지면 7 피해 ----
    send(&mut s, 0x1E, &move_to(sx, sy + 10.0, sz, false)).await;
    send(&mut s, 0x1E, &move_to(sx, sy, sz, true)).await;
    // (접속 직후의 체력 20 알림을 건너뜀)
    let h = recv_where(&mut s, |p| p.id == SET_HEALTH && health(&p.body) < 20.0).await;
    assert_eq!(health(&h.body), 13.0);

    // ---- 맨손으로 발밑 잔디 블록을 다 캠 → 흙이 떨어짐 ----
    let under = (sx.floor() as i32, sy as i32 - 1, sz.floor() as i32);
    let mut dig = Vec::new();
    write_varint(&mut dig, 3); // 다 캤음 (26.3 번호)
    dig.extend_from_slice(&pack_position(under).to_be_bytes());
    dig.push(1);
    write_varint(&mut dig, 1);
    send(&mut s, 0x29, &dig).await;
    let drop = recv_where(&mut s, |p| {
        if p.id != ADD_ENTITY {
            return false;
        }
        let mut b = p.body.as_slice();
        read_varint(&mut b).unwrap();
        b = &b[16..];
        read_varint(&mut b).unwrap() == entity_type::ITEM
    })
    .await;
    let drop_id = read_varint(&mut drop.body.as_slice()).unwrap();

    // 구멍으로 내려가면 주움
    send(&mut s, 0x1E, &move_to(sx, sy - 1.0, sz, true)).await;
    // 인벤토리 갱신과 빨려 드는 모습은 어느 쪽이 먼저 올지 모릅니다.
    let (mut took, mut stored) = (false, false);
    while !(took && stored) {
        let p = recv_where(&mut s, |p| {
            p.id == TAKE_ITEM_ENTITY || p.id == SET_PLAYER_INVENTORY
        })
        .await;
        if p.id == TAKE_ITEM_ENTITY {
            let mut b = p.body.as_slice();
            assert_eq!(read_varint(&mut b).unwrap(), drop_id);
            assert_eq!(read_varint(&mut b).unwrap(), me);
            took = true;
        } else {
            // 발밑 블록의 바닐라 전리품 (잔디면 흙, 모래면 모래 …)
            let (ccx, ccz) = (under.0 >> 4, under.2 >> 4);
            let col = mc_server::world::terrain::generate(Dimension::Overworld, SEED, ccx, ccz);
            let state = col.get((under.0 & 15) as usize, under.1, (under.2 & 15) as usize);
            let expected = mc_server::data::items::block_drop(state, 0, 0).unwrap();
            assert_eq!(inventory_update(&p.body), (0, Some((expected.id, 1))));
            stored = true;
        }
    }

    // ---- 조합: 원목을 2×2 칸에 → 판자 4, 결과 꺼내기 ----
    send_string(&mut s, 0x07, "give oak_log 1").await;
    let inv = recv_where(&mut s, |p| p.id == SET_PLAYER_INVENTORY).await;
    let log = by_name("oak_log").unwrap();
    assert_eq!(inventory_update(&inv.body), (1, Some((log, 1))));

    send(
        &mut s,
        0x12,
        &click(0, 37, &[(37, None), (1, Some((log, 1)))], None),
    )
    .await;
    let result = recv_where(&mut s, |p| p.id == CONTAINER_SET_SLOT).await;
    let planks = by_name("oak_planks").unwrap();
    let mut b = &result.body[..];
    read_varint(&mut b).unwrap();
    read_varint(&mut b).unwrap();
    assert_eq!(&b[..2], &0i16.to_be_bytes());
    let mut b = &b[2..];
    assert_eq!(
        (read_varint(&mut b).unwrap(), read_varint(&mut b).unwrap()),
        (4, planks),
        "4 planks offered"
    );

    send(&mut s, 0x12, &click(0, 0, &[(1, None)], Some((planks, 4)))).await;
    let result = recv_where(&mut s, |p| p.id == CONTAINER_SET_SLOT).await;
    assert_eq!(
        *result.body.last().unwrap(),
        0,
        "result slot empty after taking"
    );

    // ---- 없던 다이아몬드를 만들어 내는 클릭은 거부하고 창 전체를 다시 보냄 ----
    let diamond = by_name("diamond").unwrap();
    send(
        &mut s,
        0x12,
        &click(0, 20, &[(20, Some((diamond, 64)))], Some((planks, 4))),
    )
    .await;
    recv_where(&mut s, |p| p.id == CONTAINER_SET_CONTENT).await;

    // ---- 좀비: 불러내면 와서 때림 ----
    send(&mut s, 0x1E, &move_to(sx, sy - 1.0, sz, true)).await;
    send_string(&mut s, 0x07, "summon zombie").await;
    let zombie = recv_where(&mut s, |p| {
        if p.id != ADD_ENTITY {
            return false;
        }
        let mut b = p.body.as_slice();
        read_varint(&mut b).unwrap();
        b = &b[16..];
        read_varint(&mut b).unwrap() == entity_type::ZOMBIE
    })
    .await;
    let zombie_id = read_varint(&mut zombie.body.as_slice()).unwrap();
    let hurt = recv_where(&mut s, |p| {
        p.id == DAMAGE_EVENT && read_varint(&mut p.body.as_slice()).unwrap() == me
    })
    .await;
    assert!(!hurt.body.is_empty());
    let h = recv_where(&mut s, |p| p.id == SET_HEALTH).await;
    assert!(health(&h.body) < 13.0, "zombie hit us");

    // 다이아몬드 검(7)으로 세 번 → 좀비(20)는 쓰러짐
    send_string(&mut s, 0x07, "give diamond_sword").await;
    recv_where(&mut s, |p| p.id == SET_PLAYER_INVENTORY).await;
    let mut held = Vec::new();
    held.extend_from_slice(&1i16.to_be_bytes()); // 칸 1에 들어갔으면 고르기
    send(&mut s, 0x36, &held).await;
    let mut died = false;
    for _ in 0..8 {
        let mut a = Vec::new();
        write_varint(&mut a, zombie_id);
        send(&mut s, 0x01, &a).await;
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        if died {
            break;
        }
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(100);
        while let Ok(Ok(Some(p))) = tokio::time::timeout_at(deadline, s.read_packet()).await {
            if p.id == ENTITY_EVENT && p.body[..4] == zombie_id.to_be_bytes() && p.body[4] == 3 {
                died = true;
            }
        }
    }
    assert!(died, "zombie died");

    // ---- /kill → 사망 화면 → 부활 ----
    send_string(&mut s, 0x07, "kill").await;
    let kill = recv_where(&mut s, |p| p.id == PLAYER_COMBAT_KILL).await;
    assert_eq!(read_varint(&mut kill.body.as_slice()).unwrap(), me);
    send(&mut s, 0x0C, &[0]).await;
    recv_where(&mut s, |p| p.id == RESPAWN).await;
    let h = recv_where(&mut s, |p| p.id == SET_HEALTH).await;
    assert_eq!(health(&h.body), 20.0);
}
