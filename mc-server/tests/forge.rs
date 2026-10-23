//! 장비 손질 창 (`/강화`): 강화(장인의 기운) → 전승 → 연마 → 조율 → 분해, 그리고 창 제목(그림 + 글).
//!
//! `FORGE_PREVIEW=<폴더>`를 주면 탭마다 창 제목을 JSON으로 남깁니다
//! (`python3 tools/forge_preview.py <폴더>`가 그림으로 그림).

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::dambi::child;
use mc_server::plugins::equip::gears;
use mc_server::protocol::nbt::Tag;
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

/// 열린 창: (번호, 제목).
async fn opened(s: &mut Client) -> (i32, Tag) {
    let p = recv_where(s, |p| p.id == OPEN_SCREEN).await;
    let mut r = p.body.as_slice();
    let id = read_varint(&mut r).unwrap();
    let _kind = read_varint(&mut r).unwrap();
    (id, Tag::read_root(r).expect("제목 NBT"))
}

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

fn json(t: &Tag) -> String {
    let esc = |s: &str| {
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                    let mut buf = [0u16; 2];
                    for u in c.encode_utf16(&mut buf) {
                        o.push_str(&format!("\\u{u:04x}"));
                    }
                }
                c => o.push(c),
            }
        }
        o.push('"');
        o
    };
    match t {
        Tag::String(s) => esc(s),
        Tag::Byte(v) => v.to_string(),
        Tag::Int(v) => v.to_string(),
        Tag::List(l) => format!("[{}]", l.iter().map(json).collect::<Vec<_>>().join(",")),
        Tag::Compound(f) => format!(
            "{{{}}}",
            f.iter()
                .map(|(k, v)| format!("{}:{}", esc(k), json(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        other => format!("{other:?}"),
    }
}

/// 제목의 글자 전부 (글자 색 표시 포함, 확인용).
fn flat(t: &Tag) -> String {
    let mut out = String::new();
    if let Some(s) = t.get("text").and_then(Tag::as_str) {
        out.push_str(s);
    }
    for e in t.get("extra").map_or(&[][..], Tag::as_list) {
        out.push_str(&flat(e));
    }
    out
}

fn dump(name: &str, title: &Tag) {
    if let Ok(dir) = std::env::var("FORGE_PREVIEW") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(format!("{dir}/{name}.json"), json(title)).unwrap();
    }
}

fn serial_of(def: &str, nth: usize) -> u32 {
    gears().read(|d| {
        d.gears
            .iter()
            .filter(|(_, g)| g.def == def)
            .map(|(k, _)| *k)
            .nth(nth)
            .unwrap()
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn forge_tabs() {
    let server = Arc::new(Server::new(test_info(), SEED));
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut s, _) = join(addr, "Forger", 2).await;
    let me = offline_uuid("Forger");
    child::shared_add(me, "money", 10_000_000).await;

    // 핫바: 투구 둘(81, 82), 강화석(83), 반지(84), 주문서(85), 마력석(86)
    for (cmd, n) in [
        ("mithril_helmet", 2),
        ("enhance_stone", 64),
        ("mithril_ring", 1),
        ("scroll_low", 3),
        ("magic_stone", 10),
    ] {
        send_string(&mut s, 0x07, &format!("장비 지급 {cmd} {n}")).await;
        chat(&mut s, "받았습니다").await;
    }
    let helm_a = serial_of("hoya:mithril_helmet", 0);
    let helm_b = serial_of("hoya:mithril_helmet", 1);

    // 빈 강화 탭
    send_string(&mut s, 0x07, "강화").await;
    let (w, title) = opened(&mut s).await;
    let text = flat(&title);
    assert!(
        text.contains("장비 손질") && text.contains("고른 장비 없음"),
        "{text}"
    );
    dump("empty", &title);

    // 투구 A를 고르고 강화 (+0→+1 95%: 성공할 때까지)
    send(&mut s, 0x12, &click(w, 81)).await;
    let (mut w, title) = opened(&mut s).await;
    assert!(flat(&title).contains("미스릴 투구"));
    dump("enhance", &title);
    for _ in 0..8 {
        send(&mut s, 0x12, &click(w, 48)).await;
        let m = text_of(
            &recv_where(&mut s, |p| {
                p.id == SYSTEM_CHAT
                    && (text_of(p).contains("강화 성공") || text_of(p).contains("강화 실패"))
            })
            .await,
        );
        w = opened(&mut s).await.0;
        if m.contains("강화 성공") {
            break;
        }
    }
    assert!(gears().read(|d| d.gears[&helm_a].enhance) >= 1);

    // 장인의 기운이 다 차면 +29→+30(1%)도 꼭 성공
    gears().write(|d| {
        let g = d.gears.get_mut(&helm_a).unwrap();
        g.enhance = 29;
        g.artisan = 1.0;
    });
    send(&mut s, 0x12, &click(w, 50)).await;
    chat(&mut s, "장인의 기운으로 강화 성공").await;
    let (w, title) = opened(&mut s).await;
    assert!(flat(&title).contains("MAX"));
    gears().read(|d| {
        assert_eq!(d.gears[&helm_a].enhance, 30);
        assert_eq!(d.gears[&helm_a].artisan, 0.0);
    });

    // 실패하면 기운이 쌓임 (확률 1%, 쌓이는 비율 10배 → 실패마다 10%)
    let default = mc_server::plugins::equip::config::get();
    let mut cfg = default.clone();
    cfg.enhance.chances = vec![0.01; 30];
    cfg.enhance.artisan = 10.0;
    mc_server::plugins::equip::config::set(cfg).unwrap();
    send(&mut s, 0x12, &click(w, 82)).await;
    let (mut w, _) = opened(&mut s).await;
    // 성공(1%)하면 단계가 오르고 기운은 0이 되니, 실패한 그 한 번만 봄
    let mut seen = None;
    for _ in 0..10 {
        let before = gears().read(|d| d.gears[&helm_b].artisan);
        send(&mut s, 0x12, &click(w, 49)).await;
        let m = text_of(
            &recv_where(&mut s, |p| {
                p.id == SYSTEM_CHAT
                    && (text_of(p).contains("강화 성공") || text_of(p).contains("강화 실패"))
            })
            .await,
        );
        w = opened(&mut s).await.0;
        if m.contains("강화 실패") {
            let after = gears().read(|d| d.gears[&helm_b].artisan);
            assert!(
                (after - (before + 0.1).min(1.0)).abs() < 1e-9,
                "{before} → {after}"
            );
            seen = Some(after);
            break;
        }
    }
    let a = seen.expect("1% 강화가 열 번 다 성공할 리 없음");
    // 창을 다시 그림 (막대와 퍼센트)
    send(&mut s, 0x12, &click(w, 82)).await;
    let (w, title) = opened(&mut s).await;
    let want = format!("{:.0}%", a * 100.0);
    assert!(flat(&title).contains(&want), "{want} {}", flat(&title));
    dump("enhance_artisan", &title);
    mc_server::plugins::equip::config::set(default).unwrap();
    gears().write(|d| d.gears.get_mut(&helm_b).unwrap().enhance = 0);

    // 전승: A(+30) → B
    send(&mut s, 0x12, &click(w, 6)).await;
    let (w, _) = opened(&mut s).await;
    send(&mut s, 0x12, &click(w, 81)).await;
    let (w, _) = opened(&mut s).await;
    send(&mut s, 0x12, &click(w, 82)).await;
    let (w, title) = opened(&mut s).await;
    assert!(flat(&title).contains("+30"), "{}", flat(&title));
    dump("inherit", &title);
    send(&mut s, 0x12, &click(w, 49)).await;
    chat(&mut s, "전승 완료").await;
    let (w, _) = opened(&mut s).await;
    gears().read(|d| {
        assert_eq!(d.gears[&helm_a].enhance, 0);
        assert_eq!(d.gears[&helm_b].enhance, 30);
    });

    // 연마: 반지 + 주문서
    send(&mut s, 0x12, &click(w, 2)).await;
    let (w, _) = opened(&mut s).await;
    send(&mut s, 0x12, &click(w, 84)).await;
    let (w, _) = opened(&mut s).await;
    send(&mut s, 0x12, &click(w, 85)).await;
    let (w, title) = opened(&mut s).await;
    assert!(flat(&title).contains("하급 주문서"), "{}", flat(&title));
    send(&mut s, 0x12, &click(w, 48)).await;
    chat(&mut s, "[하급 연마]").await;
    let (w, title) = opened(&mut s).await;
    dump("polish", &title);
    let ring = serial_of("hoya:mithril_ring", 0);
    let before = gears().read(|d| d.gears[&ring].potential.clone());
    assert!(!before.is_empty());

    // 조율: 반지는 그대로 골라져 있음
    send(&mut s, 0x12, &click(w, 4)).await;
    let (w, title) = opened(&mut s).await;
    dump("tune", &title);
    send(&mut s, 0x12, &click(w, 48)).await;
    chat(&mut s, "[조율]").await;
    let (w, _) = opened(&mut s).await;
    let after = gears().read(|d| d.gears[&ring].potential.clone());
    assert_eq!(
        before.iter().map(|l| l.0).collect::<Vec<_>>(),
        after.iter().map(|l| l.0).collect::<Vec<_>>()
    );

    // 분해: 두 번 눌러야 부서짐
    send(&mut s, 0x12, &click(w, 7)).await;
    let (w, _) = opened(&mut s).await;
    send(&mut s, 0x12, &click(w, 48)).await;
    let (w, title) = opened(&mut s).await;
    assert!(flat(&title).contains("정말 분해?"));
    dump("salvage", &title);
    assert!(gears().read(|d| d.gears.contains_key(&ring)));
    send(&mut s, 0x12, &click(w, 48)).await;
    chat(&mut s, "분해했습니다").await;
    assert!(!gears().read(|d| d.gears.contains_key(&ring)));
}
