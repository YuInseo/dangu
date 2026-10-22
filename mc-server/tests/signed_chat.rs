//! 서명된 채팅: Mojang이 서명한 채팅 키 → 서명한 채팅 → 서버 확인 → 서명째 전달.
//! 받는 쪽에서 같은 계산으로 서명을 확인해, 실제 클라이언트가 받아들일지 검사합니다.

mod common;

use std::sync::Arc;

use common::{
    configure, connect, login, recv_until, recv_where, send, start_server_with, test_info,
};
use mc_server::Server;
use mc_server::auth::{GameProfile, ProfileFuture, SessionService};
use mc_server::chat::{self, Signature};
use mc_server::crypto::rsa::{PrivateKey, PublicKey};
use mc_server::protocol::types::{
    read_i64, read_string, read_uuid, write_byte_array, write_i64, write_string, write_uuid,
};
use mc_server::protocol::varint::{read_varint, write_varint};
use mc_server::server::Config;

const EXPIRES: i64 = 4_000_000_000_000;

struct Premium;

fn uuid_of(name: &str) -> u128 {
    0xABCD_0000_0000_0000_0000_0000_0000_0000 | name.len() as u128
}

impl SessionService for Premium {
    fn has_joined<'a>(&'a self, name: &'a str, _: &'a str) -> ProfileFuture<'a> {
        let p = GameProfile {
            uuid: uuid_of(name),
            name: name.to_owned(),
            properties: vec![],
        };
        Box::pin(async move { Ok(Some(p)) })
    }
    fn profile_by_name<'a>(&'a self, _: &'a str) -> ProfileFuture<'a> {
        Box::pin(async { Ok(None) })
    }
}

/// 정품 클라이언트: 접속 → 채팅 키(Mojang 서명) 보내기.
async fn premium_client(
    addr: std::net::SocketAddr,
    name: &str,
    mojang: &PrivateKey,
) -> (common::Client, PrivateKey, u128) {
    let mut s = connect(addr).await;
    login(&mut s, name).await;
    configure(&mut s, 2).await;
    recv_until(&mut s, 0x0B).await;

    let key = PrivateKey::generate(2048);
    let der = key.public.to_der();
    let mut payload = uuid_of(name).to_be_bytes().to_vec();
    payload.extend_from_slice(&EXPIRES.to_be_bytes());
    payload.extend_from_slice(&der);
    let session_id = 0x5E55_1000 + name.len() as u128;
    let mut b = Vec::new();
    write_uuid(&mut b, session_id);
    write_i64(&mut b, EXPIRES);
    write_byte_array(&mut b, &der);
    write_byte_array(&mut b, &mojang.sign_sha1(&payload));
    send(&mut s, 0x0A, &b).await;
    (s, key, session_id)
}

#[allow(clippy::too_many_arguments)]
fn chat_packet(text: &str, ts: i64, salt: i64, sig: &[u8], offset: i32, ack: u32) -> Vec<u8> {
    let mut b = Vec::new();
    write_string(&mut b, text);
    write_i64(&mut b, ts);
    write_i64(&mut b, salt);
    b.push(1);
    b.extend_from_slice(sig);
    write_varint(&mut b, offset);
    b.extend_from_slice(&ack.to_le_bytes()[..3]);
    b.push(0);
    b
}

/// 받은 `Player Chat`을 풀어 실제 클라이언트처럼 서명을 확인합니다.
fn verify_player_chat(
    body: &[u8],
    sender_key: &PublicKey,
    session_id: u128,
) -> (String, Signature, i32) {
    let mut b = body;
    read_varint(&mut b).unwrap(); // global index
    let sender = read_uuid(&mut b).unwrap();
    let index = read_varint(&mut b).unwrap();
    assert_eq!(b[0], 1, "signed");
    let sig: Signature = b[1..257].try_into().unwrap();
    b = &b[257..];
    let content = read_string(&mut b, 256).unwrap();
    let ts = read_i64(&mut b).unwrap();
    let salt = read_i64(&mut b).unwrap();
    let n = read_varint(&mut b).unwrap();
    let mut seen = Vec::new();
    for _ in 0..n {
        assert_eq!(read_varint(&mut b).unwrap(), 0, "full signatures");
        seen.push(<Signature>::try_from(&b[..256]).unwrap());
        b = &b[256..];
    }
    let payload = chat::signed_payload(sender, session_id, index, salt, ts, &content, &seen);
    assert!(
        sender_key.verify_sha256(&payload, &sig),
        "receiver can verify the signature"
    );
    (content, sig, index)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn signed_chat_is_relayed_with_valid_signatures() {
    let mojang = PrivateKey::generate(1024);
    let config = Config {
        online_mode: true,
        spawn_mobs: false,
        ..Config::default()
    };
    let server = Server::new(test_info(), common::SEED)
        .with_config(config)
        .with_session_service(Premium);
    server.chat_keys.lock().unwrap().push(mojang.public.clone());
    let addr = start_server_with(Arc::new(server)).await;

    let (mut alice, alice_key, alice_session) = premium_client(addr, "Alice", &mojang).await;
    let (mut bob, _, _) = premium_client(addr, "Bob", &mojang).await;
    // Bob의 탭 목록에 Alice의 채팅 세션이 붙어 있어야 Bob이 확인할 수 있습니다.
    recv_where(&mut bob, |p| p.id == 0x47 && p.body[0] == 0x02).await;

    // ---- 첫 메시지: 읽은 것 없음 ----
    let alice_uuid = uuid_of("Alice");
    let ts = 1_760_000_000_000i64;
    let payload = chat::signed_payload(alice_uuid, alice_session, 0, 42, ts, "첫 인사", &[]);
    let sig1 = alice_key.sign_sha256(&payload);
    send(
        &mut alice,
        0x09,
        &chat_packet("첫 인사", ts, 42, &sig1, 0, 0),
    )
    .await;

    let got = recv_where(&mut bob, |p| p.id == 0x42).await;
    let (content, s1, index) = verify_player_chat(&got.body, &alice_key.public, alice_session);
    assert_eq!((content.as_str(), index), ("첫 인사", 0));
    // 보낸 사람에게도 되돌아옵니다 (바닐라와 같이) — 이제 Alice의 "읽은 목록" 후보
    recv_where(&mut alice, |p| p.id == 0x42).await;

    // ---- 두 번째: 첫 메시지를 읽었다고 표시 (창을 1 밀고 19번 비트) ----
    let payload = chat::signed_payload(alice_uuid, alice_session, 1, 7, ts + 1000, "둘째", &[s1]);
    let sig2 = alice_key.sign_sha256(&payload);
    send(
        &mut alice,
        0x09,
        &chat_packet("둘째", ts + 1000, 7, &sig2, 1, 1 << 19),
    )
    .await;
    let got = recv_where(&mut bob, |p| p.id == 0x42).await;
    let (content, _, index) = verify_player_chat(&got.body, &alice_key.public, alice_session);
    assert_eq!((content.as_str(), index), ("둘째", 1));

    // ---- 서명이 틀리면 서명 없는 채팅(Disguised Chat)으로 ----
    let mut bad = alice_key.sign_sha256(b"something else");
    bad[0] ^= 1;
    send(
        &mut alice,
        0x09,
        &chat_packet("가짜", ts + 2000, 1, &bad, 0, 1 << 19),
    )
    .await;
    let got = recv_where(&mut bob, |p| p.id == 0x21 || p.id == 0x42).await;
    assert_eq!(got.id, 0x21, "unverifiable chat falls back to unsigned");
}
