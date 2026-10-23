//! 상점: 돈으로 사고팔기, 캐시가 모자라면 거절, 잠수 포인트로 플라이 사기.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::dambi::child;
use mc_server::plugins::rank;
use mc_server::protocol::varint::{read_varint, write_varint};
use mc_server::uuid::offline_uuid;

const SYSTEM_CHAT: i32 = 0x7C;
const OPEN_SCREEN: i32 = 0x3C;

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

/// 메뉴 칸 누르기: button 0 좌 / 1 우, mode 0 보통 / 1 Shift.
fn press(window: i32, slot: i16, button: u8, mode: i32) -> Vec<u8> {
    let mut c = Vec::new();
    write_varint(&mut c, window);
    write_varint(&mut c, 0);
    c.extend_from_slice(&slot.to_be_bytes());
    c.push(button);
    write_varint(&mut c, mode);
    write_varint(&mut c, 0);
    c.push(0);
    c
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn buy_and_sell_with_each_currency() {
    let server = Arc::new(Server::new(test_info(), SEED));
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut s, _) = join(addr, "Trader", 2).await;
    let me = offline_uuid("Trader");
    child::shared_add(me, "money", 10_000).await;

    // 잡화 상점: 첫 칸(10) = 조약돌 64개 640 돈 / 팔 때 64 돈
    send_string(&mut s, 0x07, "상점 general").await;
    let w = opened(&mut s).await;
    send(&mut s, 0x12, &press(w, 10, 0, 0)).await;
    let m = chat(&mut s, "샀습니다").await;
    assert!(m.contains("−640"), "{m}");
    assert_eq!(child::shared(me).await.unwrap().money, 9_360);
    // 우클릭: 한 묶음 팔기
    send(&mut s, 0x12, &press(w, 10, 1, 0)).await;
    chat(&mut s, "팔았습니다").await;
    assert_eq!(child::shared(me).await.unwrap().money, 9_424);
    // 더 팔 게 없음
    send(&mut s, 0x12, &press(w, 10, 1, 1)).await;
    chat(&mut s, "있어야 팔 수 있습니다").await;

    // 캐시가 없으면 못 삼
    send_string(&mut s, 0x07, "상점 cash").await;
    let w = opened(&mut s).await;
    send(&mut s, 0x12, &press(w, 10, 0, 0)).await;
    chat(&mut s, "모자랍니다").await;

    // 잠수 포인트로 플라이 10분
    child::shared_add(me, "afk", 100).await;
    send_string(&mut s, 0x07, "상점 afk").await;
    let w = opened(&mut s).await;
    send(&mut s, 0x12, &press(w, 10, 0, 0)).await;
    chat(&mut s, "샀습니다").await;
    let left = rank::fly_left(me);
    assert!((595..=600).contains(&left), "{left}");
    assert_eq!(child::shared(me).await.unwrap().afk, 70);
}
