//! 복권을 실제 패킷으로: `/복권` → 판매소 → 번호 고르기 → 사기(돈이 빠지고 복권 아이템) →
//! 하루 제한 → 추첨 → 스폰의 NPC에게 내서 당첨금 받기.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::dambi::child;
use mc_server::protocol::varint::{read_varint, write_varint};

const SYSTEM_CHAT: i32 = 0x7C;
const OPEN_SCREEN: i32 = 0x3C;
const CONTAINER_SET_CONTENT: i32 = 0x12;
const SET_PLAYER_INVENTORY: i32 = 0x6E;

/// 아무 칸이나 누름 (메뉴는 바뀐 칸을 보지 않음).
fn click(window: i32, slot: i16) -> Vec<u8> {
    let mut c = Vec::new();
    write_varint(&mut c, window);
    write_varint(&mut c, 0);
    c.extend_from_slice(&slot.to_be_bytes());
    c.push(0);
    write_varint(&mut c, 0);
    write_varint(&mut c, 0);
    c.push(0);
    c
}

fn text_of(p: &mc_server::protocol::frame::RawPacket) -> String {
    String::from_utf8_lossy(&p.body).into_owned()
}

async fn chat_containing(s: &mut Client, needle: &str) -> String {
    text_of(&recv_where(s, |p| p.id == SYSTEM_CHAT && text_of(p).contains(needle)).await)
}

/// 창이 열리면 (창 번호, 메뉴 종류).
async fn opened(s: &mut Client) -> (i32, i32) {
    let p = recv_where(s, |p| p.id == OPEN_SCREEN).await;
    let mut b = p.body.as_slice();
    (read_varint(&mut b).unwrap(), read_varint(&mut b).unwrap())
}

async fn press(s: &mut Client, window: i32, slot: i16) {
    send(s, 0x12, &click(window, slot)).await;
    recv_where(s, |p| p.id == CONTAINER_SET_CONTENT).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn buy_draw_and_claim() {
    let server = Arc::new(Server::new(test_info(), SEED));
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut s, _) = join(addr, "Lotto", 2).await;
    let uuid = mc_server::uuid::offline_uuid("Lotto");
    child::shared_add(uuid, "money", 10_000).await.unwrap();

    // 판매소 (3줄) → 복권 사기 → 번호 고르기 (6줄)
    send_string(&mut s, 0x07, "복권").await;
    let (hub, kind) = opened(&mut s).await;
    assert_eq!(kind, 2, "generic_9x3");
    send(&mut s, 0x12, &click(hub, 11)).await;
    let (pick, kind) = opened(&mut s).await;
    assert_eq!(kind, 5, "generic_9x6");

    // 1~5번 (칸 10~14) 고르고 사기 (칸 49)
    for slot in 10..15 {
        press(&mut s, pick, slot).await;
    }
    send(&mut s, 0x12, &click(pick, 49)).await;
    // 복권을 가방에 넣은 뒤 알림
    let ticket = recv_where(&mut s, |p| {
        p.id == SET_PLAYER_INVENTORY && text_of(p).contains("아이템 번호")
    })
    .await;
    chat_containing(&mut s, "복권을 샀습니다").await;
    let lore = text_of(&ticket);
    assert!(lore.contains("01 · 02 · 03 · 04 · 05"), "{lore}");
    assert!(lore.contains("구매"), "purchase date in lore");
    assert_eq!(child::shared(uuid).await.unwrap().money, 9_000);

    // 두 번째: 20~24번 (칸 33 34 37 38 39) — 당첨 번호와 하나도 안 겹침
    for slot in [33, 34, 37, 38, 39] {
        press(&mut s, pick, slot).await;
    }
    send(&mut s, 0x12, &click(pick, 49)).await;
    chat_containing(&mut s, "20 · 21 · 22 · 23 · 24").await;

    // 세 번째는 하루 제한(2장)에 걸리고 돈도 그대로
    press(&mut s, pick, 47).await; // 무작위
    send(&mut s, 0x12, &click(pick, 49)).await;
    chat_containing(&mut s, "오늘은 이미").await;
    assert_eq!(child::shared(uuid).await.unwrap().money, 8_000);
    assert_eq!(server.lottery.sales(), 2);

    // 추첨: 1~5 → 첫 장이 1등 (2장 × 1,000 + 1,000,000)
    let d = server
        .lottery
        .draw(Some([1, 2, 3, 4, 5]), mc_server::game::lottery::now());
    assert_eq!(d.winners, [1, 0, 0, 0]);
    assert_eq!(d.prizes[0], 1_002_000);

    // 스폰의 NPC를 우클릭 → 판매소 → 당첨 복권 내기 (칸 15)
    let id = mc_server::game::lottery::ensure_npc(&server);
    let npc = server.npcs.entity_of(id).unwrap();
    let mut interact = Vec::new();
    write_varint(&mut interact, npc);
    write_varint(&mut interact, 0);
    interact.extend_from_slice(&[0, 0]);
    send(&mut s, 0x1A, &interact).await;
    let (hub, kind) = opened(&mut s).await;
    assert_eq!(kind, 2);
    send(&mut s, 0x12, &click(hub, 15)).await;
    chat_containing(&mut s, "1등").await;
    chat_containing(&mut s, "낙첨").await;
    chat_containing(&mut s, "당첨금").await;
    assert_eq!(child::shared(uuid).await.unwrap().money, 8_000 + 1_002_000);

    // 다시 내면 낼 복권이 없음 (두 장 모두 거둠)
    send(&mut s, 0x12, &click(hub, 15)).await;
    chat_containing(&mut s, "낼 복권이 없습니다").await;
}
