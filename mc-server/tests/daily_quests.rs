//! 일일 납품: `/퀘스트 일일` 창에서 뽑힌 자원을 모두 내면 자동심기·플라이 보상.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::plugins::{quest, rank};
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

/// 메뉴 창의 칸 하나를 누름 (아무것도 옮기지 않음).
fn press(window: i32, slot: i16) -> Vec<u8> {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn daily_supply_pays_fly_and_autoplant() {
    let server = Arc::new(Server::new(test_info(), SEED));
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut s, _) = join(addr, "Courier", 2).await;
    let me = offline_uuid("Courier");

    send_string(&mut s, 0x07, "퀘스트 일일").await;
    let p = recv_where(&mut s, |p| p.id == OPEN_SCREEN).await;
    let w = read_varint(&mut p.body.as_slice()).unwrap();
    let daily = quest::progress().read(|p| p.players[&me].daily.clone());
    assert!((1..=3).contains(&daily.len()));

    for (i, slot) in daily.iter().enumerate() {
        let def = quest::config::with(|c| c.find(&slot.id)).unwrap();
        let item = def.objective.filter[0].replace("*_", "oak_");
        send_string(&mut s, 0x07, &format!("give {item} {}", def.target)).await;
        chat(&mut s, "받았습니다").await;
        send(&mut s, 0x12, &press(w, 9 + i as i16)).await;
        chat(&mut s, "개를 냈습니다").await;
    }
    chat(&mut s, "오늘의 납품 모두 완료").await;
    let m = rank::members().read(|d| d.member(me));
    assert_eq!(m.autoplant, 50);
    assert!(m.fly_until > rank::now() + 29 * 60);
}
