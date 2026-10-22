//! 채팅 채널: 기본 채널, 앞 기호로 한 번만, 채널 끄기, 도배 막기.

mod common;

use std::sync::Arc;

use common::*;
use mc_server::Server;
use mc_server::server::Config;

const SYSTEM_CHAT: i32 = 0x7C;

fn text_of(p: &mc_server::protocol::frame::RawPacket) -> String {
    String::from_utf8_lossy(&p.body).into_owned()
}

async fn chat_containing(s: &mut Client, needle: &str) -> String {
    let p = recv_where(s, |p| p.id == SYSTEM_CHAT && text_of(p).contains(needle)).await;
    text_of(&p)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn channels_route_and_mute() {
    let config = Config {
        chat_channels: true,
        ..Config::default()
    };
    let server = Arc::new(Server::new(test_info(), SEED).with_config(config));
    let addr = start_server_with(Arc::clone(&server)).await;
    let (mut alice, _) = join(addr, "Alice", 2).await;
    let (mut bob, _) = join(addr, "Bob", 2).await;

    // 기본 채널 = 이 인스턴스
    send_string(&mut alice, 0x09, "안녕").await;
    let line = chat_containing(&mut bob, "안녕").await;
    assert!(line.contains("[채널]") && line.contains("Alice"), "{line}");

    // 앞 기호로 한 번만 전체
    send_string(&mut alice, 0x09, "!모두 안녕").await;
    let line = chat_containing(&mut bob, "모두 안녕").await;
    assert!(line.contains("[전체]"), "{line}");

    // 전체는 3초 도배 막기
    send_string(&mut alice, 0x09, "!또").await;
    chat_containing(&mut alice, "초 뒤에").await;

    // Bob이 거래를 끄면 거래 말은 안 오고, 그 뒤 주변 말은 옴
    send_string(&mut bob, 0x07, "채널 끄기 거래").await;
    chat_containing(&mut bob, "껐습니다").await;
    send_string(&mut alice, 0x09, "$다이아 팝니다").await;
    send_string(&mut alice, 0x09, "~옆에 있어요").await;
    let (_, before) = recv_until_text(&mut bob, "옆에 있어요").await;
    assert!(before.iter().all(|t| !t.contains("다이아")));

    // 기본 채널 바꾸기
    send_string(&mut alice, 0x07, "채널 주변").await;
    chat_containing(&mut alice, "[주변] 채널로").await;
    send_string(&mut alice, 0x09, "가까이").await;
    let line = chat_containing(&mut bob, "가까이").await;
    assert!(line.contains("[주변]"), "{line}");

    // 길드가 없으면 길드 채널은 안 됨
    send_string(&mut alice, 0x09, "@길드야").await;
    chat_containing(&mut alice, "길드에 들어가 있지 않습니다").await;
}

/// `needle`이 든 채팅이 올 때까지, 그 전에 온 채팅 글들과 함께.
async fn recv_until_text(s: &mut Client, needle: &str) -> (String, Vec<String>) {
    let mut seen = Vec::new();
    loop {
        let p = recv(s).await;
        if p.id != SYSTEM_CHAT {
            continue;
        }
        let t = text_of(&p);
        if t.contains(needle) {
            return (t, seen);
        }
        seen.push(t);
    }
}
