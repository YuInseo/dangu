//! 길드: 만들기 → 가입 신청 → 창에서 수락 → 땅 사기 → 남의 땅 보호 → 창고 넣기·기록.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::dambi::child;
use mc_server::data::items::by_name;
use mc_server::packets::world::pack_position;
use mc_server::plugins::guild::{self, Role};
use mc_server::protocol::varint::{read_varint, write_varint};
use mc_server::uuid::offline_uuid;
use mc_server::world::{Dimension, terrain};

const SYSTEM_CHAT: i32 = 0x7C;
const OPEN_SCREEN: i32 = 0x3C;
const ACTION_BAR: i32 = 0x59;

fn text_of(p: &mc_server::protocol::frame::RawPacket) -> String {
    String::from_utf8_lossy(&p.body).into_owned()
}

async fn chat(s: &mut Client, needle: &str) -> String {
    text_of(&recv_where(s, |p| p.id == SYSTEM_CHAT && text_of(p).contains(needle)).await)
}

async fn opened(s: &mut Client) -> i32 {
    let p = recv_where(s, |p| p.id == OPEN_SCREEN).await;
    read_varint(&mut p.body.as_slice()).unwrap()
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
    button: i8,
    changed: &[(i16, Option<(i32, i32)>)],
    carried: Option<(i32, i32)>,
) -> Vec<u8> {
    let mut c = Vec::new();
    write_varint(&mut c, window);
    write_varint(&mut c, 0);
    c.extend_from_slice(&slot.to_be_bytes());
    c.push(button as u8);
    write_varint(&mut c, 0);
    write_varint(&mut c, changed.len() as i32);
    for &(s, item) in changed {
        c.extend_from_slice(&s.to_be_bytes());
        hashed(&mut c, item);
    }
    hashed(&mut c, carried);
    c
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn guild_lifecycle() {
    let server = Arc::new(Server::new(test_info(), SEED));
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut alice, _) = join(addr, "Alice", 2).await;
    let (mut bob, _) = join(addr, "Bob", 2).await;
    let (mut carol, _) = join(addr, "Carol", 2).await;
    let (a, b) = (offline_uuid("Alice"), offline_uuid("Bob"));
    child::shared_add(a, "money", 100_000).await;

    // 만들기 (10,000원)
    send_string(&mut alice, 0x07, "길드 만들기 달빛").await;
    chat(&mut alice, "길드를 만들었습니다").await;
    assert_eq!(child::shared(a).await.unwrap().money, 90_000);

    // Bob이 신청하면 Alice에게 알림, Alice가 창에서 수락
    send_string(&mut bob, 0x07, "길드 가입 달빛").await;
    chat(&mut bob, "가입을 신청했습니다").await;
    chat(&mut alice, "Bob님이 가입을 신청했습니다").await;
    send_string(&mut alice, 0x07, "길드").await;
    let w = opened(&mut alice).await;
    send(&mut alice, 0x12, &click(w, 30, 0, &[], None)).await; // 가입 신청
    let w = opened(&mut alice).await;
    send(&mut alice, 0x12, &click(w, 0, 0, &[], None)).await; // 좌클릭 = 수락
    chat(&mut bob, "가입이 수락되었습니다").await;
    guild::guilds().read(|d| {
        let g = d.by_name("달빛").unwrap();
        assert_eq!(g.role_of(b), Some(Role::Member));
    });

    // 땅 사기 (스폰 청크) → Carol은 거기서 못 부숨
    send_string(&mut alice, 0x07, "길드 땅 사기").await;
    chat(&mut alice, "를 샀습니다").await;
    let d = Dimension::Overworld;
    let (sx, sy, sz) = terrain::spawn_point(d, SEED);
    let under = (sx.floor() as i32, sy as i32 - 1, sz.floor() as i32);
    let before = server.world.get(d, under.0, under.1, under.2);
    // 크리에이티브 OP는 보호를 넘으므로 서바이벌로
    send_string(&mut carol, 0x07, "gamemode survival").await;
    chat(&mut carol, "서바이벌").await;
    let mut dig = Vec::new();
    write_varint(&mut dig, 3); // 다 캠 (서바이벌, 26.3 번호)
    dig.extend_from_slice(&pack_position(under).to_be_bytes());
    dig.push(1);
    write_varint(&mut dig, 9);
    send(&mut carol, 0x29, &dig).await;
    let bar = recv_where(&mut carol, |p| p.id == ACTION_BAR).await;
    assert!(
        text_of(&bar).contains("달빛 길드의 땅"),
        "{}",
        text_of(&bar)
    );
    assert_eq!(server.world.get(d, under.0, under.1, under.2), before);

    // 창고: 다이아 5개를 1페이지 첫 칸에 → 저장과 기록
    send_string(&mut alice, 0x07, "give diamond 5").await;
    chat(&mut alice, "받았습니다").await;
    send_string(&mut alice, 0x07, "길드 창고").await;
    let w = opened(&mut alice).await;
    let diamond = by_name("diamond").unwrap();
    // 창고 창(54칸) 뒤 가방 27칸, 핫바 첫 칸 = 81
    send(
        &mut alice,
        0x12,
        &click(w, 81, 0, &[(81, None)], Some((diamond, 5))),
    )
    .await;
    send(
        &mut alice,
        0x12,
        &click(w, 0, 0, &[(0, Some((diamond, 5)))], None),
    )
    .await;
    let mut stored = None;
    for _ in 0..50 {
        stored = guild::guilds().read(|d| d.by_name("달빛").unwrap().storage[0][0]);
        if stored.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(stored.map(|i| (i.id, i.count)), Some((diamond, 5)));
    guild::guilds().read(|d| {
        let log = &d.by_name("달빛").unwrap().log;
        assert_eq!(
            log.last().map(|e| (e.who.as_str(), e.delta)),
            Some(("Alice", 5))
        );
    });

    // 길드 채널 채팅은 길드원에게만 (채널은 기본으로 꺼져 있으니 직접 켜진 서버가 아니면 생략)
    // 탈퇴는 확인 창에서
    send_string(&mut bob, 0x07, "길드 탈퇴").await;
    let w = opened(&mut bob).await;
    send(&mut bob, 0x12, &click(w, 11, 0, &[], None)).await;
    chat(&mut bob, "길드에서 나왔습니다").await;
    chat(&mut alice, "Bob님이 길드를 떠났습니다").await;
}
