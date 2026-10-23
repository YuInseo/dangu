//! ItemsAdder식 콘텐츠를 실제 패킷으로: 예시 팩(examples/contents)을 읽고 `/iaget`, `/ia` 목록,
//! 사용자 정의 블록 놓기·부수기, 채팅 이모지, 리소스 팩에 들어간 파일.

mod common;

use std::path::Path;
use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::packets::world::pack_position;
use mc_server::protocol::varint::{read_varint, write_varint};
use mc_server::world::{Dimension, terrain};

const SYSTEM_CHAT: i32 = 0x7C;
const DISGUISED_CHAT: i32 = 0x21;
const SET_PLAYER_INVENTORY: i32 = 0x6E;
const OPEN_SCREEN: i32 = 0x3C;
const CONTAINER_SET_CONTENT: i32 = 0x12;
const BLOCK_UPDATE: i32 = 0x08;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

fn text_of(p: &mc_server::protocol::frame::RawPacket) -> String {
    String::from_utf8_lossy(&p.body).into_owned()
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn itemsadder_style_contents() {
    let dir = std::env::temp_dir().join(format!("hoya-contents-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(Path::new("examples/contents"), &dir);
    let loaded = mc_server::plugins::contents::reload_from(&dir);
    assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
    // 서버 내장 팩(hoya: 복권)을 빼면 예시 팩의 아이템 3개
    let example: Vec<_> = loaded
        .items
        .iter()
        .filter(|i| i.starts_with("example:"))
        .collect();
    // items.yml 3개 + modern.yml 4개 (틀은 빠짐)
    assert_eq!(example.len(), 7, "{:?}", loaded.items);
    assert!(!loaded.items.iter().any(|i| i.ends_with("gem_template")));
    let modern = loaded
        .categories
        .iter()
        .find(|c| c.name == "새 방식")
        .unwrap();
    assert_eq!(modern.items.len(), 4, "{:?}", modern.items);
    assert!(loaded.items.iter().any(|i| i == "hoya:lottery_ticket"));
    // 리소스 팩에 그림·모델·블록 상태·폰트
    let pack = mc_server::resource_pack::rebuild();
    let zip = String::from_utf8_lossy(&pack.bytes);
    for f in [
        "assets/example/textures/item/ruby.png",
        "assets/example/models/item/ruby.json",
        "assets/example/items/ruby.json",
        "assets/minecraft/blockstates/note_block.json",
        "assets/minecraft/font/default.json",
    ] {
        assert!(zip.contains(f), "{f}");
    }

    let server = Arc::new(Server::new(test_info(), SEED));
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut s, _) = join(addr, "Crafter", 2).await;

    // /iaget → 가방에 루비 블록 (item_model = example:ruby_block)
    send_string(&mut s, 0x07, "iaget example:ruby_block 2").await;
    let got = recv_where(&mut s, |p| {
        p.id == SET_PLAYER_INVENTORY && text_of(p).contains("example:ruby_block")
    })
    .await;
    assert!(text_of(&got).contains("루비 블록"));

    // 발밑 블록 위에 놓기 → 켜진 소리 블록 상태 (사용자 정의 블록)
    let (sx, sy, sz) = terrain::spawn_point(Dimension::Overworld, SEED);
    let under = (sx.floor() as i32, sy as i32 - 1, sz.floor() as i32);
    let mut b = Vec::new();
    write_varint(&mut b, 0);
    b.extend_from_slice(&pack_position(under).to_be_bytes());
    write_varint(&mut b, 1); // 윗면
    for f in [0.5f32, 1.0, 0.5] {
        b.extend_from_slice(&f.to_be_bytes());
    }
    b.extend_from_slice(&[0, 0]);
    write_varint(&mut b, 1);
    // 다른 칸에 서서 놓도록 옆으로
    let mut mv = Vec::new();
    for v in [sx + 1.5, sy, sz] {
        mv.extend_from_slice(&v.to_be_bytes());
    }
    mv.push(1);
    send(&mut s, 0x1E, &mv).await;
    send(&mut s, 0x42, &b).await;
    let placed_at = (under.0, under.1 + 1, under.2);
    let index = mc_server::game::custom_items::index_of("example:ruby_block").unwrap();
    let state = mc_server::plugins::contents::block_state(index).unwrap();
    let up = recv_where(&mut s, |p| p.id == BLOCK_UPDATE).await;
    let mut body = &up.body[8..];
    assert_eq!(read_varint(&mut body).unwrap() as u32, state);
    assert_eq!(
        server
            .world
            .get(Dimension::Overworld, placed_at.0, placed_at.1, placed_at.2),
        state
    );

    // 서바이벌로 부수면 루비 블록 아이템이 떨어짐
    let mut dig = Vec::new();
    write_varint(&mut dig, 3);
    dig.extend_from_slice(&pack_position(placed_at).to_be_bytes());
    dig.push(1);
    write_varint(&mut dig, 2);
    send(&mut s, 0x29, &dig).await;
    for _ in 0..50 {
        if server
            .entities
            .snapshots_in(Dimension::Overworld)
            .iter()
            .any(|e| e.item.is_some_and(|i| i.custom == Some(index)))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        server
            .entities
            .snapshots_in(Dimension::Overworld)
            .iter()
            .any(|e| e.item.is_some_and(|i| i.custom == Some(index)))
    );

    // /ia → 분류 → 아이템 → 눌러서 받기
    send_string(&mut s, 0x07, "ia").await;
    let open = recv_where(&mut s, |p| p.id == OPEN_SCREEN).await;
    let window = read_varint(&mut open.body.as_slice()).unwrap();
    recv_where(&mut s, |p| p.id == CONTAINER_SET_CONTENT).await;
    send(&mut s, 0x12, &click(window, 0)).await; // 보석 분류
    let items = recv_where(&mut s, |p| p.id == CONTAINER_SET_CONTENT).await;
    assert!(text_of(&items).contains("example:ruby_sword"));
    send(&mut s, 0x12, &click(window, 1)).await; // 루비 검
    recv_where(&mut s, |p| {
        p.id == SET_PLAYER_INVENTORY && text_of(p).contains("example:ruby_sword")
    })
    .await;

    // 채팅의 :heart: → 그림 글자
    let heart = loaded.fonts[0].ch;
    send_string(&mut s, 0x09, "사랑해요 :heart:").await;
    let chat = recv_where(&mut s, |p| {
        (p.id == DISGUISED_CHAT || p.id == SYSTEM_CHAT) && text_of(p).contains(heart)
    })
    .await;
    assert!(text_of(&chat).contains("사랑해요"));
    std::fs::remove_dir_all(dir).ok();
}
