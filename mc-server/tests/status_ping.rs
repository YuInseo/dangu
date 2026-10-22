//! 실제 TCP 소켓으로 바닐라 클라이언트가 하는 순서를 그대로 흉내 냅니다.

use std::sync::Arc;

use mc_server::packets::status::ServerInfo;
use mc_server::protocol::frame::{encode_packet, read_packet};
use mc_server::protocol::types::{read_string, write_string};
use mc_server::protocol::varint::write_varint;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

async fn start() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let info = Arc::new(ServerInfo {
        name: "Dangu MC".into(),
        description: "순수 Rust 커스텀 서버입니다".into(),
        online_players: 0,
        max_players: 100,
    });
    tokio::spawn(mc_server::serve(listener, info));
    addr
}

fn handshake(protocol: i32, intent: i32) -> Vec<u8> {
    let mut body = Vec::new();
    write_varint(&mut body, protocol);
    write_string(&mut body, "localhost");
    body.extend_from_slice(&25565u16.to_be_bytes());
    write_varint(&mut body, intent);
    encode_packet(0x00, &body)
}

#[tokio::test]
async fn status_then_ping() {
    let mut s = TcpStream::connect(start().await).await.unwrap();

    // 클라이언트는 Handshake와 Status Request를 한 번에 보냅니다.
    let mut out = handshake(777, 1);
    out.extend(encode_packet(0x00, &[]));
    s.write_all(&out).await.unwrap();

    let resp = read_packet(&mut s).await.unwrap().unwrap();
    assert_eq!(resp.id, 0x00);
    let json = read_string(&mut resp.body.as_slice(), 32767).unwrap();
    assert!(json.contains(r#""protocol":777"#), "{json}");
    assert!(json.contains(r#""max":100,"online":0"#), "{json}");
    assert!(json.contains("순수 Rust 커스텀 서버입니다"), "{json}");

    let payload: i64 = 1_729_000_000_000;
    s.write_all(&encode_packet(0x01, &payload.to_be_bytes())).await.unwrap();
    let pong = read_packet(&mut s).await.unwrap().unwrap();
    assert_eq!(pong.id, 0x01);
    assert_eq!(pong.body, payload.to_be_bytes());

    // Pong 뒤에는 서버가 닫습니다.
    assert!(read_packet(&mut s).await.unwrap().is_none());
}

#[tokio::test]
async fn old_client_gets_status_with_our_version() {
    let mut s = TcpStream::connect(start().await).await.unwrap();
    let mut out = handshake(775, 1); // 26.1.2
    out.extend(encode_packet(0x00, &[]));
    s.write_all(&out).await.unwrap();

    let resp = read_packet(&mut s).await.unwrap().unwrap();
    let json = read_string(&mut resp.body.as_slice(), 32767).unwrap();
    assert!(json.contains(r#""name":"26.3","protocol":777"#), "{json}");
}

#[tokio::test]
async fn login_with_wrong_version_is_disconnected() {
    let mut s = TcpStream::connect(start().await).await.unwrap();
    s.write_all(&handshake(775, 2)).await.unwrap();

    let p = read_packet(&mut s).await.unwrap().unwrap();
    assert_eq!(p.id, 0x00);
    let reason = read_string(&mut p.body.as_slice(), 262144).unwrap();
    assert!(reason.contains("클라이언트가 오래되었습니다"), "{reason}");
    assert!(read_packet(&mut s).await.unwrap().is_none());
}
