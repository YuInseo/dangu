//! 통합 테스트용 가짜 클라이언트. 서버와 같은 코덱(압축·암호화)을 씁니다.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use mc_server::Server;
use mc_server::crypto::random;
use mc_server::crypto::rsa::PublicKey;
use mc_server::packets::status::ServerInfo;
use mc_server::protocol::conn::Conn;
use mc_server::protocol::frame::RawPacket;
use mc_server::protocol::types::{
    read_bool, read_string, write_byte_array, write_string, write_uuid,
};
use mc_server::protocol::varint::{read_varint, write_varint};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};

pub type Client = Conn<TcpStream>;

pub const SEED: u64 = 99;

pub fn test_info() -> ServerInfo {
    ServerInfo {
        name: "t".into(),
        description: "t".into(),
        max_players: 10,
    }
}

pub async fn start_server() -> SocketAddr {
    start_server_with(Arc::new(Server::new(test_info(), SEED))).await
}

pub async fn start_server_with(server: Arc<Server>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(mc_server::serve(listener, server));
    addr
}

pub async fn connect(addr: SocketAddr) -> Client {
    Conn::new(TcpStream::connect(addr).await.unwrap())
}

pub async fn recv(s: &mut Client) -> RawPacket {
    timeout(Duration::from_secs(30), s.read_packet())
        .await
        .expect("timed out")
        .unwrap()
        .expect("closed")
}

/// `id` 패킷이 올 때까지 읽고, 그동안 본 패킷들을 함께 돌려줍니다.
pub async fn recv_until(s: &mut Client, id: i32) -> (RawPacket, Vec<RawPacket>) {
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
pub async fn recv_where(s: &mut Client, pred: impl Fn(&RawPacket) -> bool) -> RawPacket {
    loop {
        let p = recv(s).await;
        if pred(&p) {
            return p;
        }
    }
}

pub async fn send(s: &mut Client, id: i32, body: &[u8]) {
    s.write_packet(id, body).await.unwrap();
}

pub fn ids(packets: &[RawPacket]) -> Vec<i32> {
    packets.iter().map(|p| p.id).collect()
}

pub async fn send_string(s: &mut Client, id: i32, text: &str) {
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

pub fn handshake_body(intent: i32) -> Vec<u8> {
    let mut hs = Vec::new();
    write_varint(&mut hs, 777);
    write_string(&mut hs, "localhost");
    hs.extend_from_slice(&25565u16.to_be_bytes());
    write_varint(&mut hs, intent);
    hs
}

/// 정품 클라이언트가 Mojang에 알리는 것과 같은 서버 해시. 테스트의 가짜 세션 서버가 비교합니다.
pub fn server_hash(secret: &[u8], public_der: &[u8]) -> String {
    let mut h = mc_server::crypto::sha1::Sha1::default();
    h.update(secret);
    h.update(public_der);
    mc_server::crypto::sha1::minecraft_hex_digest(&h.finish())
}

/// Login이 끝난 결과.
pub struct LoggedIn {
    pub finished: RawPacket,
    pub compressed: bool,
    pub encrypted: bool,
}

/// Handshake → Hello → (암호화) → (압축) → Login Finished → Acknowledged.
pub async fn login(s: &mut Client, name: &str) -> LoggedIn {
    send(s, 0x00, &handshake_body(2)).await;
    let mut hello = Vec::new();
    write_string(&mut hello, name);
    write_uuid(&mut hello, 12345);
    send(s, 0x00, &hello).await;

    let (mut compressed, mut encrypted) = (false, false);
    loop {
        let p = recv(s).await;
        match p.id {
            0x01 => {
                // Encryption Request: 서버 ID, 공개 키, 토큰, 인증 여부
                let mut b = p.body.as_slice();
                assert_eq!(read_string(&mut b, 20).unwrap(), "");
                let take = |b: &mut &[u8]| {
                    let n = read_varint(b).unwrap() as usize;
                    let (h, r) = b.split_at(n);
                    *b = r;
                    h.to_vec()
                };
                let der = take(&mut b);
                let token = take(&mut b);
                assert!(read_bool(&mut b).unwrap());
                let key = PublicKey::from_der(&der).expect("valid DER public key");
                let secret = random::bytes::<16>();
                let mut resp = Vec::new();
                write_byte_array(&mut resp, &key.encrypt(&secret));
                write_byte_array(&mut resp, &key.encrypt(&token));
                send(s, 0x01, &resp).await;
                s.enable_encryption(&secret);
                encrypted = true;
                LAST_HASH.with(|h| *h.borrow_mut() = Some(server_hash(&secret, &der)));
            }
            0x03 => {
                let threshold = read_varint(&mut p.body.as_slice()).unwrap();
                s.set_compression(Some(threshold as usize));
                compressed = true;
            }
            0x02 => {
                send(s, 0x03, &[]).await;
                return LoggedIn {
                    finished: p,
                    compressed,
                    encrypted,
                };
            }
            0x00 => panic!("login disconnect: {:?}", String::from_utf8_lossy(&p.body)),
            id => panic!("unexpected login packet {id:#x}"),
        }
    }
}

thread_local! {
    /// 마지막 정품 로그인에서 클라이언트가 계산한 서버 해시.
    pub static LAST_HASH: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Configuration을 끝까지 (known pack 합의 포함).
pub async fn configure(s: &mut Client, view_distance: u8) -> Vec<RawPacket> {
    let mut info = Vec::new();
    write_string(&mut info, "ko_kr");
    info.push(view_distance);
    info.extend_from_slice(&[0, 1, 0x7f, 1, 0, 1, 0]);
    send(s, 0x00, &info).await;

    let (_, mut seen) = recv_until(s, 0x0F).await;
    let mut packs = Vec::new();
    write_varint(&mut packs, 1);
    for v in ["minecraft", "core", "26.3"] {
        write_string(&mut packs, v);
    }
    send(s, 0x07, &packs).await;
    let (_, rest) = recv_until(s, 0x03).await;
    seen.extend(rest);
    send(s, 0x03, &[]).await;
    seen
}

/// Handshake부터 Play의 첫 청크 묶음까지. Play에 들어간 뒤 받은 패킷들(첫 Chunk Batch
/// Finished 직전까지)을 돌려줍니다.
pub async fn join(addr: SocketAddr, name: &str, view_distance: u8) -> (Client, Vec<RawPacket>) {
    let mut s = connect(addr).await;
    login(&mut s, name).await;
    configure(&mut s, view_distance).await;
    let (_, play) = recv_until(&mut s, 0x0B).await;
    (s, play)
}
