//! 바닐라가 아닌 클라이언트(Lunar·Feather·모드 클라이언트)가 보내는 것들에 끊기지 않는지.
//!
//! - 레거시(0xFE) 서버 목록 핑에 답함
//! - 핸드셰이크 주소에 Forge `\0FML3\0`가 붙어 있어도 됨
//! - Configuration에 쿠키 응답·플러그인 채널·대화 상자 버튼·행동 강령 동의 같은 패킷이 끼어도 됨
//! - Play에서 모양이 이상한 패킷 하나는 버리고 이어 감

mod common;

use common::*;
use mc_server::protocol::types::write_string;
use mc_server::protocol::varint::write_varint;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn legacy_ping_gets_an_answer() {
    let addr = start_server().await;
    let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
    c.write_all(&[0xFE, 0x01, 0xFA]).await.unwrap();
    let mut out = Vec::new();
    c.read_to_end(&mut out).await.unwrap();
    assert_eq!(out[0], 0xFF);
    let text: String =
        char::decode_utf16(out[3..].chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])))
            .map(|c| c.unwrap())
            .collect();
    assert!(text.starts_with("§1\u{0}777\u{0}"), "{text:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn odd_clients_still_get_in() {
    let addr = start_server().await;
    let mut s = connect(addr).await;
    // Forge처럼 주소 뒤에 표시를 붙인 핸드셰이크
    let mut hs = Vec::new();
    write_varint(&mut hs, 777);
    write_string(&mut hs, "localhost\0FML3\0");
    hs.extend_from_slice(&25565u16.to_be_bytes());
    write_varint(&mut hs, 2);
    send(&mut s, 0x00, &hs).await;
    login_after_handshake(&mut s, "Lunar").await;

    // Configuration: 바닐라에 없는 순서·패킷을 섞어 보냄
    let mut brand = Vec::new();
    write_string(&mut brand, "minecraft:brand");
    write_string(&mut brand, "lunarclient:v2.18.3-2432");
    send(&mut s, 0x02, &brand).await;
    let mut reg = Vec::new();
    write_string(&mut reg, "minecraft:register");
    reg.extend_from_slice(b"lunar:apollo\0lunar:pm");
    send(&mut s, 0x02, &reg).await;
    let mut cookie = Vec::new();
    write_string(&mut cookie, "lunar:session");
    cookie.push(0); // 내용 없음
    send(&mut s, 0x01, &cookie).await;
    send(&mut s, 0x09, &[]).await; // 행동 강령 동의
    send(&mut s, 0x05, &7i32.to_be_bytes()).await; // 묻지 않은 pong
    let mut info = Vec::new();
    write_string(&mut info, "ko_kr");
    info.push(12);
    info.extend_from_slice(&[0, 1, 0x7f, 1, 0, 1, 0]);
    send(&mut s, 0x00, &info).await;
    recv_until(&mut s, 0x0F).await;
    let mut packs = Vec::new();
    write_varint(&mut packs, 1);
    for v in ["minecraft", "core", "26.3"] {
        write_string(&mut packs, v);
    }
    send(&mut s, 0x07, &packs).await;
    let mut click = Vec::new();
    write_string(&mut click, "lunar:noop");
    click.push(0);
    send(&mut s, 0x08, &click).await; // 대화 상자 버튼
    recv_until(&mut s, 0x03).await;
    send(&mut s, 0x03, &[]).await;
    recv_until(&mut s, 0x0B).await; // Play의 첫 청크 묶음

    // Play: 모양이 틀린 keep-alive (8바이트여야 함) → 그 패킷만 버림
    send(&mut s, 0x1C, &[1]).await;
    // 아직 연결되어 있음: 명령어에 답이 옴
    let mut cmd = Vec::new();
    write_string(&mut cmd, "help");
    send(&mut s, 0x07, &cmd).await;
    let reply = recv_where(&mut s, |p| p.id == 0x7C).await;
    assert_eq!(reply.id, 0x7C);
}
