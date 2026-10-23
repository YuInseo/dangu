//! 사용자 정의 NPC를 실제 패킷으로: `/npc create` → 마네킹이 뜨고, 메시지·명령어를 달고,
//! 우클릭하면 실행되고, 모습을 바꾸면 다시 뜨고, 다시 켜도 남아 있는지.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::protocol::varint::{read_varint, write_varint};

const SYSTEM_CHAT: i32 = 0x7C;
const ADD_ENTITY: i32 = 0x01;
const SET_ENTITY_DATA: i32 = 0x65;

fn text_of(p: &mc_server::protocol::frame::RawPacket) -> String {
    String::from_utf8_lossy(&p.body).into_owned()
}

type Pred<'a> = &'a dyn Fn(&mc_server::protocol::frame::RawPacket) -> bool;

/// 조건마다 맞는 첫 패킷이 모두 올 때까지 (순서 무관).
async fn until_all(
    s: &mut Client,
    preds: &[Pred<'_>],
) -> Vec<mc_server::protocol::frame::RawPacket> {
    let mut found: Vec<Option<mc_server::protocol::frame::RawPacket>> = vec![None; preds.len()];
    while found.iter().any(Option::is_none) {
        let p = recv(s).await;
        for (i, pred) in preds.iter().enumerate() {
            if found[i].is_none() && pred(&p) {
                found[i] = Some(p.clone());
                break;
            }
        }
    }
    found.into_iter().map(Option::unwrap).collect()
}

async fn chat_containing(s: &mut Client, needle: &str) {
    recv_where(s, |p| p.id == SYSTEM_CHAT && text_of(p).contains(needle)).await;
}

/// Add Entity: (엔티티 번호, 종류).
fn added(p: &mc_server::protocol::frame::RawPacket) -> (i32, i32) {
    let mut b = p.body.as_slice();
    let id = read_varint(&mut b).unwrap();
    b = &b[16..];
    (id, read_varint(&mut b).unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_click_retype_and_persist() {
    let dir = std::env::temp_dir().join(format!("hoya-npc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let server = Arc::new(Server::open(test_info(), SEED, dir.clone()).unwrap());
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut s, _) = join(addr, "Builder", 2).await;
    let mannequin = mc_server::data::entity_id("mannequin").unwrap();
    let villager = mc_server::data::entity_id("villager").unwrap();

    send_string(&mut s, 0x07, "npc create &a상인 철수").await;
    // 띄우기(이벤트)와 답(채팅)은 어느 쪽이 먼저 올지 모름
    let seen = until_all(
        &mut s,
        &[
            &|p| p.id == ADD_ENTITY && added(p).1 == mannequin,
            &|p| p.id == SET_ENTITY_DATA && text_of(p).contains("상인 철수"),
            &|p| p.id == SYSTEM_CHAT && text_of(p).contains("NPC #1"),
        ],
    )
    .await;
    let entity = added(&seen[0]).0;
    assert_eq!(read_varint(&mut seen[1].body.as_slice()).unwrap(), entity);

    send_string(&mut s, 0x07, "엔피시 메시지 추가 어서 오세요 {player}님").await;
    chat_containing(&mut s, "메시지를 더했습니다").await;
    send_string(&mut s, 0x07, "npc cmd add say {player} 손님 방문").await;
    // 편집하면 다시 띄움
    let seen = until_all(
        &mut s,
        &[&|p| p.id == ADD_ENTITY && added(p).1 == mannequin, &|p| {
            p.id == SYSTEM_CHAT && text_of(p).contains("명령어를 더했습니다")
        }],
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let entity = server.npcs.entity_of(1).unwrap();
    assert!(added(&seen[0]).0 <= entity);

    // 우클릭 → 메시지와 명령어
    let mut interact = Vec::new();
    write_varint(&mut interact, entity);
    write_varint(&mut interact, 0);
    interact.extend_from_slice(&[0, 0]);
    send(&mut s, 0x1A, &interact).await;
    chat_containing(&mut s, "어서 오세요 Builder님").await;
    chat_containing(&mut s, "Builder 손님 방문").await;

    // 모습 바꾸기 → 주민으로 다시 뜸
    send_string(&mut s, 0x07, "npc type villager").await;
    recv_where(&mut s, |p| p.id == ADD_ENTITY && added(p).1 == villager).await;

    // 저장되어 다시 켜도 남음
    drop(s);
    let again = Server::open(test_info(), SEED, dir.clone()).unwrap();
    let all = again.npcs.all();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].kind, "villager");
    assert_eq!(all[0].messages, vec!["어서 오세요 {player}님".to_owned()]);
    assert_eq!(all[0].commands[0].line, "say {player} 손님 방문");
    std::fs::remove_dir_all(dir).ok();
}
