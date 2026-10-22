//! OP·권한·밴·화이트리스트, 게임 모드 전환 권한, 자동 완성, 핑, 사용자 정의 아이템.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::admin::{self, Actor};
use mc_server::protocol::types::{read_string, write_string};
use mc_server::protocol::varint::{read_varint, write_varint};
use mc_server::server::Config;

fn strict_server() -> Arc<Server> {
    Arc::new(Server::new(test_info(), SEED).with_config(Config {
        everyone_op: false,
        ..Config::default()
    }))
}

fn console(server: &Server, line: &str) -> Vec<String> {
    let mut w = line.split_whitespace();
    let name = w.next().unwrap();
    let args: Vec<&str> = w.collect();
    admin::run(server, &Actor::console(), name, &args).expect("admin command")
}

const ENTITY_EVENT: i32 = 0x22;
const GAME_EVENT: i32 = 0x27;

/// 로그인 단계에서 끊기면 그 사유(JSON)를, 아니면 None.
async fn login_refusal(addr: std::net::SocketAddr, name: &str) -> Option<String> {
    let mut s = connect(addr).await;
    send(&mut s, 0x00, &handshake_body(2)).await;
    let mut hello = Vec::new();
    write_string(&mut hello, name);
    hello.extend_from_slice(&[0; 16]);
    send(&mut s, 0x00, &hello).await;
    loop {
        let p = recv(&mut s).await;
        match p.id {
            0x00 => return Some(read_string(&mut p.body.as_slice(), 32767).unwrap()),
            0x03 => {
                let t = read_varint(&mut p.body.as_slice()).unwrap();
                s.set_compression(Some(t as usize));
            }
            0x02 => return None,
            id => panic!("unexpected {id:#x}"),
        }
    }
}

#[tokio::test]
async fn bans_and_whitelist_refuse_login() {
    let server = strict_server();
    let addr = start_server_with(Arc::clone(&server)).await;

    console(&server, "ban Mallory 도배");
    let reason = login_refusal(addr, "Mallory").await.expect("refused");
    assert!(reason.contains("도배"), "{reason}");
    console(&server, "pardon Mallory");
    assert_eq!(login_refusal(addr, "Mallory").await, None);

    console(&server, "whitelist on");
    console(&server, "whitelist add Alice");
    assert_eq!(login_refusal(addr, "Alice").await, None);
    let reason = login_refusal(addr, "Eve").await.expect("not whitelisted");
    assert!(reason.contains("화이트리스트"), "{reason}");
}

#[tokio::test]
async fn op_level_gates_game_mode_switch() {
    let server = strict_server();
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut alice, play) = join(addr, "Alice", 2).await;
    // 처음에는 OP 0단계 (Entity Event 24)
    let level = play
        .iter()
        .find(|p| p.id == ENTITY_EVENT)
        .map(|p| p.body[4])
        .expect("op level event");
    assert_eq!(level, 24);

    // F3+F4로 크리에이티브를 골라도 거절 → 시스템 메시지
    let mut body = Vec::new();
    write_varint(&mut body, 1);
    send(&mut alice, 0x05, &body).await;
    recv_where(&mut alice, |p| {
        p.id == 0x7C && String::from_utf8_lossy(&p.body).contains("권한")
    })
    .await;

    console(&server, "op Alice");
    recv_where(&mut alice, |p| p.id == ENTITY_EVENT && p.body[4] == 28).await;
    send(&mut alice, 0x05, &body).await;
    let ev = recv_where(&mut alice, |p| p.id == GAME_EVENT && p.body[0] == 3).await;
    assert_eq!(f32::from_be_bytes(ev.body[1..5].try_into().unwrap()), 1.0);
}

#[tokio::test]
async fn pardon_suggestions_come_from_ban_list() {
    let server = strict_server();
    let addr = start_server_with(Arc::clone(&server)).await;
    console(&server, "op Alice");
    console(&server, "ban Bobby");
    let (mut alice, _) = join(addr, "Alice", 2).await;

    let mut body = Vec::new();
    write_varint(&mut body, 7);
    write_string(&mut body, "/pardon Bo");
    send(&mut alice, 0x0F, &body).await;
    let p = recv_where(&mut alice, |p| p.id == 0x0F).await;
    let mut b = p.body.as_slice();
    assert_eq!(read_varint(&mut b).unwrap(), 7);
    assert_eq!(read_varint(&mut b).unwrap(), 8); // "/pardon " 다음
    assert_eq!(read_varint(&mut b).unwrap(), 2);
    assert_eq!(read_varint(&mut b).unwrap(), 1);
    assert_eq!(read_string(&mut b, 100).unwrap(), "Bobby");
}

#[tokio::test]
async fn ping_is_answered_with_pong() {
    let addr = start_server().await;
    let (mut alice, _) = join(addr, "Alice", 2).await;
    send(&mut alice, 0x26, &1234567i64.to_be_bytes()).await;
    let p = recv_where(&mut alice, |p| p.id == 0x3F).await;
    assert_eq!(p.body, 1234567i64.to_be_bytes());
}

#[tokio::test]
async fn custom_item_carries_components() {
    let addr = start_server().await;
    let (mut alice, _) = join(addr, "Alice", 2).await;
    send_string(&mut alice, 0x07, "item thunder_staff").await;
    let p = recv_where(&mut alice, |p| p.id == 0x14 || p.id == 0x6E).await;
    let has = |needle: &[u8]| p.body.windows(needle.len()).any(|w| w == needle);
    assert!(has(b"dangu:item"), "custom_data");
    assert!(has(b"thunder_staff"));
}
