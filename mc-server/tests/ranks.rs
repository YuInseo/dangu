//! 후원 등급: 주기 → 더블점프 (날기 허용 → 스페이스 두 번이면 튀어 오름) → 셋홈 개수 →
//! /ec 엔더상자에 넣기 → 플라이 시간.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::data::items::by_name;
use mc_server::packets::world::Item;
use mc_server::plugins::rank;
use mc_server::protocol::varint::{read_varint, write_varint};
use mc_server::uuid::offline_uuid;

const SYSTEM_CHAT: i32 = 0x7C;
const OPEN_SCREEN: i32 = 0x3C;
const ABILITIES: i32 = 0x41;
const MOTION: i32 = 0x67;
const SB_ABILITIES: i32 = 0x28;

fn text_of(p: &mc_server::protocol::frame::RawPacket) -> String {
    String::from_utf8_lossy(&p.body).into_owned()
}

async fn chat(s: &mut Client, needle: &str) -> String {
    text_of(&recv_where(s, |p| p.id == SYSTEM_CHAT && text_of(p).contains(needle)).await)
}

/// 다음 능력 패킷의 비트.
async fn abilities(s: &mut Client) -> u8 {
    recv_where(s, |p| p.id == ABILITIES).await.body[0]
}

fn plain(out: &mut Vec<u8>, item: Option<(i32, i32)>) {
    match item {
        None => out.push(0),
        Some((id, n)) => {
            out.push(1);
            write_varint(out, id);
            write_varint(out, n);
            write_varint(out, 0);
            write_varint(out, 0);
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
        plain(&mut c, item);
    }
    plain(&mut c, carried);
    c
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn supporter_perks() {
    let server = Arc::new(Server::new(test_info(), SEED));
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut s, _) = join(addr, "Donor", 2).await;
    let me = offline_uuid("Donor");

    // 등급이 없으면 /ec 막힘
    send_string(&mut s, 0x07, "ec").await;
    chat(&mut s, "후원 등급 특혜").await;

    send_string(&mut s, 0x07, "등급 설정 Donor mvp 30").await;
    chat(&mut s, "MVP 등급을 주었습니다 (30일)").await;
    assert_eq!(rank::rank_of(me).map(|r| r.id), Some("mvp".into()));

    // 다음 틱: 더블점프 때문에 "날 수 있음"
    while abilities(&mut s).await & 0x04 == 0 {}
    // 스페이스 두 번 → 날기 대신 튀어 오르고 날기 허용이 꺼짐
    send(&mut s, SB_ABILITIES, &[0x02]).await;
    let flags = abilities(&mut s).await;
    assert_eq!(flags & 0x06, 0, "{flags:#x}");
    let m = recv_where(&mut s, |p| p.id == MOTION).await;
    assert!(!m.body.is_empty());

    // 셋홈: 기본 1 + MVP 3
    for i in 0..4 {
        send_string(&mut s, 0x07, &format!("셋홈 h{i}")).await;
        chat(&mut s, "놓았습니다").await;
    }
    send_string(&mut s, 0x07, "셋홈 h4").await;
    chat(&mut s, "4개까지").await;
    send_string(&mut s, 0x07, "홈 h1").await;
    chat(&mut s, "홈 'h1'(으)로 왔습니다").await;
    // MVP는 tp 쿨타임 없음
    send_string(&mut s, 0x07, "홈 h2").await;
    chat(&mut s, "홈 'h2'(으)로 왔습니다").await;

    // /ec: 핫바 0의 다이아 3개를 엔더상자 0번 칸에
    let diamond = by_name("diamond").unwrap();
    send_string(&mut s, 0x07, "give diamond 3").await;
    chat(&mut s, "받았습니다").await;
    send_string(&mut s, 0x07, "ec").await;
    let p = recv_where(&mut s, |p| p.id == OPEN_SCREEN).await;
    let w = read_varint(&mut p.body.as_slice()).unwrap();
    // 27칸 창: 가방 27..54, 핫바 54..63
    send(
        &mut s,
        0x12,
        &click(w, 54, &[(54, None)], Some((diamond, 3))),
    )
    .await;
    send(&mut s, 0x12, &click(w, 0, &[(0, Some((diamond, 3)))], None)).await;
    let mut stored = None;
    for _ in 0..50 {
        stored = rank::members().read(|d| d.member(me).ender[0]);
        if stored.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(stored, Some(Item::new(diamond, 3)));

    // 플라이 시간: 진짜로 날 수 있음
    send_string(&mut s, 0x07, "등급 플라이 Donor 30").await;
    chat(&mut s, "플라이 30분").await;
    send_string(&mut s, 0x07, "플라이").await;
    chat(&mut s, "남음").await;
    // 관리: 시·분·초까지
    send_string(&mut s, 0x07, "플라이 설정 Donor 1h2m3s").await;
    chat(&mut s, "이제 1시간 2분").await;
    send_string(&mut s, 0x07, "플라이 빼기 Donor 1시간").await;
    chat(&mut s, "이제 2분").await;
    send_string(&mut s, 0x07, "플라이 추가 Donor 0:00:30").await;
    // 초가 하나 지나갔을 수도 있으니 앞자리만
    chat(&mut s, "이제 2분 3").await;
    send_string(&mut s, 0x07, "플라이 목록").await;
    chat(&mut s, "Donor — 2분").await;

    // 채팅 머리말에 등급
    send_string(&mut s, 0x07, "등급 목록").await;
    chat(&mut s, "Budget").await;
}
