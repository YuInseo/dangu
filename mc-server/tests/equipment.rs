//! 장비: 지급 → 주문서를 악세 위에 놓아 잠재능력 → /장비 창에서 끼기 → /강화 → /대장간 제련 →
//! 보스 소환.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::dambi::child;
use mc_server::data::items::by_name;
use mc_server::plugins::equip::{self, gears};
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

/// 해시된 칸: 우리 아이템처럼 `custom_data` 해시를 하나 붙임 (값은 아무거나 — 서버가 옮겨진
/// 것 가운데서 찾음).
fn hashed(out: &mut Vec<u8>, item: Option<(i32, i32)>) {
    match item {
        None => out.push(0),
        Some((id, n)) => {
            out.push(1);
            write_varint(out, id);
            write_varint(out, n);
            write_varint(out, 1);
            write_varint(out, mc_server::data::component::CUSTOM_DATA);
            out.extend_from_slice(&(0x5EED_0000u32 as i32 + id).to_be_bytes());
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
        hashed(&mut c, item);
    }
    hashed(&mut c, carried);
    c
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gear_life() {
    let server = Arc::new(Server::new(test_info(), SEED));
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut s, _) = join(addr, "Smith", 2).await;
    let me = offline_uuid("Smith");
    child::shared_add(me, "money", 1_000_000).await;
    let paper = by_name("paper").unwrap();

    // 반지와 하급 주문서 (핫바 0, 1)
    send_string(&mut s, 0x07, "장비 지급 mithril_ring").await;
    chat(&mut s, "받았습니다").await;
    send_string(&mut s, 0x07, "장비 지급 scroll_low").await;
    chat(&mut s, "받았습니다").await;
    let ring = gears().read(|d| {
        d.gears
            .iter()
            .find(|(_, g)| g.def == "dangu:mithril_ring")
            .map(|(k, _)| *k)
    });
    let ring = ring.expect("반지에 일련번호");

    // 가방에서 주문서를 집어 (핫바 1 = 37) 반지 위에 (36) 놓음
    send(&mut s, 0x12, &click(0, 37, &[(37, None)], Some((paper, 1)))).await;
    send(
        &mut s,
        0x12,
        &click(0, 36, &[(36, Some((paper, 1)))], Some((paper, 1))),
    )
    .await;
    let msg = chat(&mut s, "[하급]").await;
    assert!(msg.contains("남은 횟수 2"), "{msg}");
    gears().read(|d| {
        let g = &d.gears[&ring];
        assert!((1..=2).contains(&g.potential.len()));
        assert_eq!(g.tries_left, 2);
        assert_eq!(g.scroll.as_deref(), Some("하급"));
    });

    // /장비 창에서 가방의 반지(핫바 0 = 창 칸 81)를 누르면 낌
    send_string(&mut s, 0x07, "장비").await;
    let w = opened(&mut s).await;
    send(&mut s, 0x12, &click(w, 81, &[], None)).await;
    chat(&mut s, "꼈습니다").await;
    assert_eq!(gears().read(|d| d.worn(me))[0], Some(ring));

    // 강화: 투구 + 강화석 → 성공할 때까지 (+0→+1은 95%)
    send_string(&mut s, 0x07, "장비 지급 mithril_helmet").await;
    chat(&mut s, "받았습니다").await;
    send_string(&mut s, 0x07, "장비 지급 enhance_stone 10").await;
    chat(&mut s, "받았습니다").await;
    let helm = gears()
        .read(|d| {
            d.gears
                .iter()
                .find(|(_, g)| g.def == "dangu:mithril_helmet")
                .map(|(k, _)| *k)
        })
        .unwrap();
    send_string(&mut s, 0x07, "강화").await;
    let w = opened(&mut s).await;
    // 반지를 꼈으니 투구는 핫바 0 (창 칸 81)
    send(&mut s, 0x12, &click(w, 81, &[], None)).await;
    let mut done = false;
    for _ in 0..6 {
        send(&mut s, 0x12, &click(w, 31, &[], None)).await;
        let m = text_of(
            &recv_where(&mut s, |p| {
                p.id == SYSTEM_CHAT
                    && (text_of(p).contains("강화 성공") || text_of(p).contains("강화 실패"))
            })
            .await,
        );
        if m.contains("강화 성공") {
            done = true;
            break;
        }
    }
    assert!(done);
    assert_eq!(gears().read(|d| d.gears[&helm].enhance), 1);

    // 대장간 제련: 원석 2 → 주괴 1
    send_string(&mut s, 0x07, "장비 지급 mithril_raw 4").await;
    chat(&mut s, "받았습니다").await;
    send_string(&mut s, 0x07, "대장간 제련").await;
    let w = opened(&mut s).await;
    send(&mut s, 0x12, &click(w, 9, &[], None)).await;
    chat(&mut s, "미스릴 주괴을(를) 만들었습니다").await;

    // 보스 소환
    send_string(&mut s, 0x07, "보스 소환 goblin_king").await;
    chat(&mut s, "나타났습니다").await;

    // 꼈던 반지의 잠재능력이 능력치에 들어감
    let def = equip::config::with(|c| c.gear_def("dangu:mithril_ring").cloned()).unwrap();
    assert!(def.stats[equip::Stat::CritChance] > 0.0);
}
