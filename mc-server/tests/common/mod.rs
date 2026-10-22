//! 통합 테스트용 가짜 클라이언트.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use mc_server::Server;
use mc_server::packets::status::ServerInfo;
use mc_server::protocol::frame::{RawPacket, encode_packet, read_packet};
use mc_server::protocol::types::{write_string, write_uuid};
use mc_server::protocol::varint::write_varint;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};

pub async fn start_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let info = ServerInfo {
        name: "t".into(),
        description: "t".into(),
        max_players: 10,
    };
    tokio::spawn(mc_server::serve(listener, Arc::new(Server::new(info, 99))));
    addr
}

pub async fn recv(s: &mut TcpStream) -> RawPacket {
    timeout(Duration::from_secs(30), read_packet(s))
        .await
        .expect("timed out")
        .unwrap()
        .expect("closed")
}

/// `id` 패킷이 올 때까지 읽고, 그동안 본 패킷들을 함께 돌려줍니다.
pub async fn recv_until(s: &mut TcpStream, id: i32) -> (RawPacket, Vec<RawPacket>) {
    let mut seen = Vec::new();
    loop {
        let p = recv(s).await;
        if p.id == id {
            return (p, seen);
        }
        seen.push(p);
    }
}

/// `pred`를 만족하는 패킷이 올 때까지 읽습니다.
pub async fn recv_where(s: &mut TcpStream, pred: impl Fn(&RawPacket) -> bool) -> RawPacket {
    loop {
        let p = recv(s).await;
        if pred(&p) {
            return p;
        }
    }
}

pub async fn send(s: &mut TcpStream, id: i32, body: &[u8]) {
    s.write_all(&encode_packet(id, body)).await.unwrap();
}

pub fn ids(packets: &[RawPacket]) -> Vec<i32> {
    packets.iter().map(|p| p.id).collect()
}

pub async fn send_string(s: &mut TcpStream, id: i32, text: &str) {
    let mut body = Vec::new();
    write_string(&mut body, text);
    // Chat 패킷이면 뒤에 timestamp·salt·서명 없음·offset·ack·checksum이 붙습니다.
    if id == 0x09 {
        body.extend_from_slice(&[0; 16]);
        body.push(0);
        body.push(0);
        body.extend_from_slice(&[0; 3]);
        body.push(0);
    }
    send(s, id, &body).await;
}

/// Handshake → Login → Configuration을 지나 Play의 첫 청크 묶음까지 받습니다.
/// Play에 들어간 뒤 받은 패킷들(첫 Chunk Batch Finished 직전까지)을 돌려줍니다.
pub async fn join(addr: SocketAddr, name: &str, view_distance: u8) -> (TcpStream, Vec<RawPacket>) {
    let mut s = TcpStream::connect(addr).await.unwrap();

    let mut hs = Vec::new();
    write_varint(&mut hs, 777);
    write_string(&mut hs, "localhost");
    hs.extend_from_slice(&25565u16.to_be_bytes());
    write_varint(&mut hs, 2);
    send(&mut s, 0x00, &hs).await;

    let mut hello = Vec::new();
    write_string(&mut hello, name);
    write_uuid(&mut hello, 12345);
    send(&mut s, 0x00, &hello).await;
    assert_eq!(recv(&mut s).await.id, 0x02, "Login Finished");
    send(&mut s, 0x03, &[]).await;

    let mut info = Vec::new();
    write_string(&mut info, "ko_kr");
    info.push(view_distance);
    info.extend_from_slice(&[0, 1, 0x7f, 1, 0, 1, 0]);
    send(&mut s, 0x00, &info).await;

    recv_until(&mut s, 0x0F).await;
    let mut packs = Vec::new();
    write_varint(&mut packs, 1);
    for v in ["minecraft", "core", "26.3"] {
        write_string(&mut packs, v);
    }
    send(&mut s, 0x07, &packs).await;
    recv_until(&mut s, 0x03).await;
    send(&mut s, 0x03, &[]).await;

    let (_, play) = recv_until(&mut s, 0x0B).await;
    (s, play)
}
