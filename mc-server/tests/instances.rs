//! 인스턴스 사이 이동: Start Configuration → Configuration 다시 → 다른 월드의 Play.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::network::Network;
use mc_server::protocol::types::write_string;
use mc_server::protocol::varint::write_varint;

const START_CONFIGURATION: i32 = 0x78;
const TAB_LIST: i32 = 0x7D;
const PLAYER_INFO_REMOVE: i32 = 0x46;
const SYSTEM_CHAT: i32 = 0x7C;

async fn two_instances() -> (Arc<Network>, std::net::SocketAddr) {
    let lobby = Arc::new(Server::new(test_info(), SEED).named("lobby"));
    let pvp = Arc::new(Server::new(test_info(), 7).named("pvp").share_from(&lobby));
    let net = Network::in_memory(vec![lobby, pvp]);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(mc_server::serve_network(listener, Arc::clone(&net)));
    (net, addr)
}

/// `/server <이름>` 뒤의 재구성과 새 Play 시작까지. 새 Play의 패킷들을 돌려줍니다.
async fn transfer(s: &mut Client, to: &str) -> Vec<mc_server::protocol::frame::RawPacket> {
    send_string(s, 0x07, &format!("server {to}")).await;
    recv_where(s, |p| p.id == START_CONFIGURATION).await;
    send(s, 0x10, &[]).await; // Configuration Acknowledged
    // 다시 Configuration: known pack 합의 → 레지스트리 → 끝
    recv_until(s, 0x0F).await;
    let mut packs = Vec::new();
    write_varint(&mut packs, 1);
    for v in ["minecraft", "core", "26.3"] {
        write_string(&mut packs, v);
    }
    send(s, 0x07, &packs).await;
    recv_until(s, 0x03).await;
    send(s, 0x03, &[]).await;
    recv_until(s, 0x0B).await.1
}

fn text_of(p: &mc_server::protocol::frame::RawPacket) -> String {
    String::from_utf8_lossy(&p.body).into_owned()
}

#[tokio::test]
async fn players_move_between_instances_with_separate_worlds() {
    let (net, addr) = two_instances().await;
    let (mut alice, _) = join(addr, "Alice", 2).await;
    let (mut bob, _) = join(addr, "Bob", 2).await;
    assert_eq!(net.get("lobby").unwrap().online(), 2);

    let play = transfer(&mut alice, "pvp").await;
    assert!(play.iter().any(|p| p.id == 0x32), "new Login (play)");
    // 탭 목록 꼬리에 인스턴스 이름
    recv_where(&mut alice, |p| {
        p.id == TAB_LIST && text_of(p).contains("pvp")
    })
    .await;
    assert_eq!(net.get("lobby").unwrap().online(), 1);
    assert_eq!(net.get("pvp").unwrap().online(), 1);

    // 로비에 남은 Bob에게서는 사라짐
    recv_where(&mut bob, |p| p.id == PLAYER_INFO_REMOVE).await;
    recv_where(&mut bob, |p| {
        p.id == SYSTEM_CHAT && text_of(p).contains("pvp")
    })
    .await;

    // 인스턴스마다 채팅이 따로
    send_string(&mut alice, 0x09, "pvp에서만").await;
    send_string(&mut bob, 0x09, "로비에서만").await;
    let heard = recv_where(&mut alice, |p| p.id == 0x21 || p.id == 0x42).await;
    assert!(text_of(&heard).contains("pvp에서만"));

    // 다른 인스턴스에 있어도 같은 이름은 또 못 들어옴
    let mut again = connect(addr).await;
    login(&mut again, "Alice").await;
    configure(&mut again, 2).await;
    let p = recv_where(&mut again, |p| p.id == 0x20).await; // Disconnect
    assert!(text_of(&p).contains("이미"));

    // 돌아오기
    transfer(&mut alice, "lobby").await;
    assert_eq!(net.get("lobby").unwrap().online(), 2);
    assert_eq!(net.get("pvp").unwrap().online(), 0);
}

#[tokio::test]
async fn admin_can_create_instances_at_runtime() {
    let (net, addr) = two_instances().await;
    let (mut alice, _) = join(addr, "Alice", 2).await;
    send_string(&mut alice, 0x07, "instance create build 5 creative").await;
    recv_where(&mut alice, |p| {
        p.id == SYSTEM_CHAT && text_of(p).contains("만듦")
    })
    .await;
    let build = net.get("build").expect("created");
    assert_eq!(build.seed, 5);
    assert_eq!(
        build.config.default_gamemode,
        mc_server::packets::play::GameMode::Creative
    );
    let play = transfer(&mut alice, "build").await;
    // 크리에이티브로 들어감 (Game Event 3이 아니라 Login의 게임 모드)
    assert!(play.iter().any(|p| p.id == 0x32));
    assert_eq!(build.online(), 1);
}
