//! 바닐라 클라이언트처럼 Login → Configuration → Play를 끝까지 가 보고, 차원을 옮겨 봅니다.

use std::sync::Arc;

use mc_server::Server;
use mc_server::packets::status::ServerInfo;
use mc_server::protocol::frame::{RawPacket, encode_packet, read_packet};
use mc_server::protocol::types::{read_string, read_uuid, write_string, write_uuid};
use mc_server::protocol::varint::{read_varint, write_varint};
use mc_server::uuid::offline_uuid;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};

async fn start() -> TcpStream {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let info = ServerInfo {
        name: "t".into(),
        description: "t".into(),
        max_players: 10,
    };
    tokio::spawn(mc_server::serve(listener, Arc::new(Server::new(info, 99))));
    TcpStream::connect(addr).await.unwrap()
}

async fn recv(s: &mut TcpStream) -> RawPacket {
    timeout(Duration::from_secs(30), read_packet(s))
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}

/// `id` 패킷이 올 때까지 읽고, 그동안 본 ID들을 함께 돌려줍니다.
async fn recv_until(s: &mut TcpStream, id: i32) -> (RawPacket, Vec<i32>) {
    let mut seen = Vec::new();
    loop {
        let p = recv(s).await;
        if p.id == id {
            return (p, seen);
        }
        seen.push(p.id);
    }
}

async fn send(s: &mut TcpStream, id: i32, body: &[u8]) {
    s.write_all(&encode_packet(id, body)).await.unwrap();
}

#[tokio::test]
async fn login_configure_play_and_travel() {
    let mut s = start().await;

    // ---- Handshake + Login ----
    let mut hs = Vec::new();
    write_varint(&mut hs, 777);
    write_string(&mut hs, "localhost");
    hs.extend_from_slice(&25565u16.to_be_bytes());
    write_varint(&mut hs, 2);
    send(&mut s, 0x00, &hs).await;

    let mut hello = Vec::new();
    write_string(&mut hello, "Tester");
    write_uuid(&mut hello, 12345);
    send(&mut s, 0x00, &hello).await;

    let finished = recv(&mut s).await;
    assert_eq!(finished.id, 0x02, "Login Finished");
    let mut b = finished.body.as_slice();
    assert_eq!(read_uuid(&mut b).unwrap(), offline_uuid("Tester"));
    assert_eq!(read_string(&mut b, 16).unwrap(), "Tester");
    assert_eq!(read_varint(&mut b).unwrap(), 0);
    read_uuid(&mut b).unwrap(); // 26.2 세션 UUID
    assert!(b.is_empty());
    send(&mut s, 0x03, &[]).await; // Login Acknowledged

    // ---- Configuration ----
    let mut info = Vec::new();
    write_string(&mut info, "ko_kr");
    info.push(4); // 시야 거리 4
    info.extend_from_slice(&[0, 1, 0x7f, 1, 0, 1, 0]);
    send(&mut s, 0x00, &info).await;

    let (_, before) = recv_until(&mut s, 0x0F).await; // Select Known Packs
    assert!(
        before.contains(&0x0D),
        "enabled features before known packs"
    );
    let mut packs = Vec::new();
    write_varint(&mut packs, 1);
    for v in ["minecraft", "core", "26.3"] {
        write_string(&mut packs, v);
    }
    send(&mut s, 0x07, &packs).await;

    let (_, config) = recv_until(&mut s, 0x03).await; // Finish Configuration
    let registries = config.iter().filter(|&&id| id == 0x07).count();
    assert_eq!(registries, mc_server::data::SYNCED_REGISTRIES.len());
    assert!(config.contains(&0x0E), "update tags");
    send(&mut s, 0x03, &[]).await;

    // ---- Play ----
    let login = recv(&mut s).await;
    assert_eq!(login.id, 0x32, "Login (play)");
    let mut b = login.body.as_slice();
    b = &b[4..]; // entity id
    assert_ne!(
        login.body[..4],
        [0, 0, 0, 0],
        "entity id 0 is rejected since 26.2"
    );
    b = &b[1..];
    assert_eq!(read_varint(&mut b).unwrap(), 3, "three worlds");

    let (_, seen) = recv_until(&mut s, 0x0B).await; // Chunk Batch Finished
    assert!(seen.contains(&0x49), "player position");
    assert!(seen.contains(&0x27), "game event 13");
    let chunks = seen.iter().filter(|&&id| id == 0x2E).count();
    assert_eq!(chunks, 9 * 9, "view distance 4 → 9×9 chunks");

    // ---- /nether ----
    let mut cmd = Vec::new();
    write_string(&mut cmd, "nether");
    send(&mut s, 0x07, &cmd).await;
    let (respawn, _) = recv_until(&mut s, 0x54).await;
    let mut b = respawn.body.as_slice();
    read_varint(&mut b).unwrap();
    assert_eq!(read_string(&mut b, 64).unwrap(), "minecraft:the_nether");
    let (_, seen) = recv_until(&mut s, 0x0B).await;
    assert_eq!(seen.iter().filter(|&&id| id == 0x2E).count(), 81);

    // ---- 걸어서 청크 경계 넘기 → 새 줄만 받기 ----
    let mut mv = Vec::new();
    for v in [1000.0f64, 80.0, 0.0] {
        mv.extend_from_slice(&v.to_be_bytes());
    }
    mv.push(1);
    send(&mut s, 0x1E, &mv).await;
    let (_, seen) = recv_until(&mut s, 0x0B).await;
    assert!(seen.contains(&0x60), "set center");
    assert!(seen.contains(&0x26), "forget old chunks");
}
