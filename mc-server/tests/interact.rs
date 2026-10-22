//! 엔티티 우클릭을 실제 패킷으로: 소 젖 짜기, 양털 깎기, 주민과 거래.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::data::items::by_name;
use mc_server::data::trades::TRADES;
use mc_server::game::entities::Kind;
use mc_server::game::mobs::{self, Mob, MobKind};
use mc_server::game::villagers::{Offer, Villager};
use mc_server::protocol::varint::{read_varint, write_varint};
use mc_server::world::{Dimension, terrain};

const SYSTEM_CHAT: i32 = 0x7C;
const SET_PLAYER_INVENTORY: i32 = 0x6E;
const SET_ENTITY_DATA: i32 = 0x65;
const OPEN_SCREEN: i32 = 0x3C;
const MERCHANT_OFFERS: i32 = 0x35;
const CONTAINER_SET_CONTENT: i32 = 0x12;

fn interact(target: i32) -> Vec<u8> {
    let mut b = Vec::new();
    write_varint(&mut b, target);
    write_varint(&mut b, 0); // 주 손
    b.push(0); // 맞은 자리 (LpVec3 0)
    b.push(0); // 웅크리지 않음
    b
}

async fn give(s: &mut Client, what: &str) {
    send_string(s, 0x07, &format!("give {what}")).await;
    let name = what.split(' ').next().unwrap().to_owned();
    recv_where(s, |p| {
        p.id == SYSTEM_CHAT && String::from_utf8_lossy(&p.body).contains(&name)
    })
    .await;
}

/// Set Player Inventory: (칸, 아이템 ID).
fn inventory_slot(body: &[u8]) -> (i32, Option<i32>) {
    let mut b = body;
    let slot = read_varint(&mut b).unwrap();
    let count = read_varint(&mut b).unwrap();
    (slot, (count > 0).then(|| read_varint(&mut b).unwrap()))
}

/// Container Set Content의 칸들: (아이템 ID, 개수).
fn content(body: &[u8]) -> Vec<Option<(i32, i32)>> {
    let mut b = body;
    read_varint(&mut b).unwrap();
    read_varint(&mut b).unwrap();
    let n = read_varint(&mut b).unwrap();
    let mut slots = Vec::new();
    for _ in 0..n {
        let count = read_varint(&mut b).unwrap();
        if count == 0 {
            slots.push(None);
            continue;
        }
        let id = read_varint(&mut b).unwrap();
        let added = read_varint(&mut b).unwrap();
        let removed = read_varint(&mut b).unwrap();
        assert_eq!((added, removed), (0, 0), "plain items only here");
        slots.push(Some((id, count)));
    }
    slots
}

fn hashed(out: &mut Vec<u8>, item: Option<(i32, i32)>) {
    match item {
        None => out.push(0),
        Some((id, n)) => {
            out.push(1);
            for v in [id, n, 0, 0] {
                write_varint(out, v);
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn click(
    window: i32,
    slot: i16,
    changed: &[(i16, Option<(i32, i32)>)],
    carried: Option<(i32, i32)>,
) -> Vec<u8> {
    let mut c = Vec::new();
    write_varint(&mut c, window);
    write_varint(&mut c, 0);
    c.extend_from_slice(&slot.to_be_bytes());
    c.push(0);
    write_varint(&mut c, 0);
    write_varint(&mut c, changed.len() as i32);
    for &(s, item) in changed {
        c.extend_from_slice(&s.to_be_bytes());
        hashed(&mut c, item);
    }
    hashed(&mut c, carried);
    c
}

fn villager(server: &Server, id: i32) -> Villager {
    match server.entities.get(id).unwrap().kind {
        Kind::Mob(m) => m.villager.unwrap(),
        _ => unreachable!(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn milk_shear_and_trade() {
    let server = Arc::new(Server::new(test_info(), SEED));
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut s, _) = join(addr, "Trader", 2).await;
    let d = Dimension::Overworld;
    let (sx, sy, sz) = terrain::spawn_point(d, SEED);
    let near = (sx + 1.5, sy, sz);

    // 소: 양동이로 젖 짜기 → 손의 양동이가 우유 양동이로
    give(&mut s, "bucket").await;
    let cow = mobs::spawn(&server, MobKind::Cow, d, near);
    send(&mut s, 0x1A, &interact(cow)).await;
    let milk = by_name("milk_bucket").unwrap();
    let p = recv_where(&mut s, |p| {
        p.id == SET_PLAYER_INVENTORY && inventory_slot(&p.body).1 == Some(milk)
    })
    .await;
    assert_eq!(inventory_slot(&p.body).0, 0, "held hotbar slot");
    server.entities.remove(&server, cow);

    // 양: 가위로 털 깎기 → 털이 떨어지고 깎인 모습 (엔티티 데이터 18번 = 0x10)
    give(&mut s, "shears").await;
    let mut select = Vec::new();
    select.extend_from_slice(&1i16.to_be_bytes());
    send(&mut s, 0x36, &select).await; // 핫바 두 번째 칸 (가위)
    let sheep = mobs::spawn(&server, MobKind::Sheep, d, near);
    send(&mut s, 0x1A, &interact(sheep)).await;
    recv_where(&mut s, |p| {
        p.id == SET_ENTITY_DATA && read_varint(&mut p.body.as_slice()).unwrap() == sheep
    })
    .await;
    assert!(matches!(
        server.entities.get(sheep).unwrap().kind,
        Kind::Mob(ref m) if m.sheared
    ));
    let wool = by_name("white_wool").unwrap();
    assert!(
        server
            .entities
            .snapshots_in(d)
            .iter()
            .any(|e| e.item.is_some_and(|i| i.id == wool)),
        "wool dropped"
    );
    // 두 번은 깎이지 않음
    let before = server.entities.count();
    send(&mut s, 0x1A, &interact(sheep)).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(server.entities.count() <= before);
    server.entities.remove(&server, sheep);

    // 주민: 밀 20개 → 에메랄드 1개 (농부 1레벨 바닐라 거래)
    let trade = TRADES
        .iter()
        .find(|(p, l, _)| *p == "farmer" && *l == 1)
        .and_then(|(_, _, t)| t.iter().find(|t| t.wants.0 == "wheat"))
        .unwrap();
    let mut v = Villager::new("farmer", "plains");
    v.offers = vec![Offer { trade, uses: 0 }];
    let mut m = Mob::new(MobKind::Villager);
    m.villager = Some(v);
    let villager_id = mobs::spawn_mob(&server, m, d, near);
    give(&mut s, &format!("wheat {}", trade.wants.1)).await;
    send(&mut s, 0x1A, &interact(villager_id)).await;
    let open = recv_where(&mut s, |p| p.id == OPEN_SCREEN).await;
    let mut b = open.body.as_slice();
    let window = read_varint(&mut b).unwrap();
    assert_eq!(read_varint(&mut b).unwrap(), 19, "merchant menu");
    let offers = recv_where(&mut s, |p| p.id == MERCHANT_OFFERS).await;
    let mut b = offers.body.as_slice();
    assert_eq!(read_varint(&mut b).unwrap(), window);
    assert_eq!(read_varint(&mut b).unwrap(), 1, "one offer");
    let wheat = by_name("wheat").unwrap();
    assert_eq!(read_varint(&mut b).unwrap(), wheat);

    // 창을 연 직후의 내용: 칸 0·1·2는 비어 있음
    let opened = content(
        &recv_where(&mut s, |p| p.id == CONTAINER_SET_CONTENT)
            .await
            .body,
    );
    assert_eq!(&opened[..3], &[None, None, None]);

    // 목록에서 고르면 가방의 밀이 칸 0으로, 결과 칸 2에 에메랄드
    let mut sel = Vec::new();
    write_varint(&mut sel, 0);
    send(&mut s, 0x34, &sel).await;
    let slots = content(
        &recv_where(&mut s, |p| p.id == CONTAINER_SET_CONTENT)
            .await
            .body,
    );
    let emerald = by_name("emerald").unwrap();
    let n = trade.wants.1 as i32;
    assert_eq!(slots[0], Some((wheat, n)));
    assert_eq!(slots[2], Some((emerald, trade.gives.1 as i32)));
    assert_eq!(slots.len(), 3 + 36);

    // 결과를 꺼냄: 칸 0이 비고 손에 에메랄드
    send(
        &mut s,
        0x12,
        &click(window, 2, &[(0, None), (2, None)], Some((emerald, 1))),
    )
    .await;
    let again = recv_where(&mut s, |p| p.id == MERCHANT_OFFERS).await;
    assert_eq!(read_varint(&mut again.body.as_slice()).unwrap(), window);
    let v = villager(&server, villager_id);
    assert_eq!(v.offers[0].uses, 1);
    assert_eq!(v.xp, trade.xp as u32);

    // 밀 없이 결과를 꺼내려 하면 거부되고 창을 다시 보냄
    send(
        &mut s,
        0x12,
        &click(window, 2, &[(2, None)], Some((emerald, 2))),
    )
    .await;
    let slots = content(
        &recv_where(&mut s, |p| p.id == CONTAINER_SET_CONTENT)
            .await
            .body,
    );
    assert_eq!(slots[2], None);
    assert_eq!(villager(&server, villager_id).offers[0].uses, 1);
}
