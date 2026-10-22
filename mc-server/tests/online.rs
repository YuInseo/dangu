//! 정품 모드: RSA 키 교환 → AES-CFB8 암호화 → 세션 서버 확인 → 스킨 속성 전달.

mod common;

use std::sync::{Arc, Mutex};

use common::{LAST_HASH, configure, connect, ids, login, recv_until, start_server_with, test_info};
use mc_server::Server;
use mc_server::auth::{GameProfile, ProfileFuture, Property, SessionService};
use mc_server::protocol::types::{read_string, read_uuid};
use mc_server::protocol::varint::read_varint;
use mc_server::server::Config;

/// 받은 (이름, 서버 해시)를 적어 두고, 아무나 정품으로 통과시키는 가짜 세션 서버.
#[derive(Default, Clone)]
struct FakeSessions {
    seen: Arc<Mutex<Vec<(String, String)>>>,
    reject: bool,
}

fn profile(name: &str) -> GameProfile {
    GameProfile {
        uuid: 0x069a79f444e94726a5befca90e38aaf5 ^ name.len() as u128,
        name: name.to_owned(),
        properties: vec![Property {
            name: "textures".into(),
            value: format!("skin-of-{name}"),
            signature: Some("signed-by-mojang".into()),
        }],
    }
}

impl SessionService for FakeSessions {
    fn has_joined<'a>(&'a self, name: &'a str, server_hash: &'a str) -> ProfileFuture<'a> {
        self.seen
            .lock()
            .unwrap()
            .push((name.to_owned(), server_hash.to_owned()));
        let reject = self.reject;
        Box::pin(async move { Ok((!reject).then(|| profile(name))) })
    }

    fn profile_by_name<'a>(&'a self, name: &'a str) -> ProfileFuture<'a> {
        Box::pin(async move { Ok(Some(profile(name))) })
    }
}

fn online_server(sessions: FakeSessions) -> Arc<Server> {
    let config = Config {
        online_mode: true,
        ..Config::default()
    };
    Arc::new(
        Server::new(test_info(), common::SEED)
            .with_config(config)
            .with_session_service(sessions),
    )
}

#[tokio::test]
async fn encrypted_login_with_skin() {
    let sessions = FakeSessions::default();
    let addr = start_server_with(online_server(sessions.clone())).await;

    let mut alice = connect(addr).await;
    let done = login(&mut alice, "Alice").await;
    assert!(done.encrypted && done.compressed);

    // 서버가 세션 서버에 보낸 해시 = 클라이언트가 계산한 해시
    let client_hash = LAST_HASH.with(|h| h.borrow().clone()).unwrap();
    assert_eq!(
        sessions.seen.lock().unwrap()[0],
        ("Alice".to_owned(), client_hash)
    );

    // Login Finished에 정품 UUID와 스킨 속성이 그대로
    let mut b = done.finished.body.as_slice();
    assert_eq!(read_uuid(&mut b).unwrap(), profile("Alice").uuid);
    assert_eq!(read_string(&mut b, 16).unwrap(), "Alice");
    assert_eq!(read_varint(&mut b).unwrap(), 1);
    assert_eq!(read_string(&mut b, 64).unwrap(), "textures");
    assert_eq!(read_string(&mut b, 64).unwrap(), "skin-of-Alice");

    // 암호화된 채로 Configuration과 Play까지
    configure(&mut alice, 2).await;
    let (_, play) = recv_until(&mut alice, 0x0B).await;
    let info = play.iter().find(|p| p.id == 0x47).expect("player info");
    let body = String::from_utf8_lossy(&info.body);
    assert!(
        body.contains("skin-of-Alice") && body.contains("signed-by-mojang"),
        "skin in tab list"
    );
    assert!(
        ids(&play).contains(&0x2E),
        "chunks arrive encrypted+compressed"
    );
}

#[tokio::test]
async fn failed_authentication_is_disconnected() {
    let addr = start_server_with(online_server(FakeSessions {
        reject: true,
        ..Default::default()
    }))
    .await;
    let mut s = connect(addr).await;
    let result = tokio::spawn(async move { login(&mut s, "Mallory").await.finished.id });
    let err = result.await.expect_err("login panics on disconnect");
    let msg = err.into_panic();
    let text = msg.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(text.contains("login disconnect"), "{text}");
}

#[tokio::test]
async fn offline_skins_borrow_premium_profile() {
    let config = Config {
        offline_skins: true,
        ..Config::default()
    };
    let server = Server::new(test_info(), common::SEED)
        .with_config(config)
        .with_session_service(FakeSessions::default());
    let addr = start_server_with(Arc::new(server)).await;
    let mut s = connect(addr).await;
    let done = login(&mut s, "Bob").await;
    assert!(!done.encrypted);
    let mut b = done.finished.body.as_slice();
    assert_eq!(
        read_uuid(&mut b).unwrap(),
        mc_server::uuid::offline_uuid("Bob"),
        "offline UUID kept"
    );
    read_string(&mut b, 16).unwrap();
    assert_eq!(read_varint(&mut b).unwrap(), 1, "borrowed textures");
}
