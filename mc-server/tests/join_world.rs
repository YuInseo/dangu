//! 바닐라 클라이언트처럼 Login → Configuration → Play를 끝까지 가 보고, 차원을 옮겨 봅니다.

mod common;

use common::{configure, connect, ids, login, recv, recv_until, send, start_server};
use mc_server::protocol::types::{read_string, read_uuid, write_string};
use mc_server::protocol::varint::read_varint;
use mc_server::uuid::offline_uuid;

#[tokio::test]
async fn login_configure_play_and_travel() {
    let mut s = connect(start_server().await).await;

    // ---- Login ----
    let done = login(&mut s, "Tester").await;
    assert!(done.compressed, "Set Compression before Login Finished");
    assert!(!done.encrypted, "offline mode does not encrypt");
    let mut b = done.finished.body.as_slice();
    assert_eq!(read_uuid(&mut b).unwrap(), offline_uuid("Tester"));
    assert_eq!(read_string(&mut b, 16).unwrap(), "Tester");
    assert_eq!(read_varint(&mut b).unwrap(), 0, "no properties offline");
    read_uuid(&mut b).unwrap(); // 26.2 세션 UUID
    assert!(b.is_empty());

    // ---- Configuration ----
    let config = ids(&configure(&mut s, 4).await);
    assert!(config.contains(&0x0D), "enabled features");
    let registries = config.iter().filter(|&&id| id == 0x07).count();
    assert_eq!(registries, mc_server::data::SYNCED_REGISTRIES.len());
    assert!(config.contains(&0x0E), "update tags");

    // ---- Play ----
    let login = recv(&mut s).await;
    assert_eq!(login.id, 0x32, "Login (play)");
    assert_ne!(
        login.body[..4],
        [0, 0, 0, 0],
        "entity id 0 is rejected since 26.2"
    );
    let mut b = &login.body[5..];
    assert_eq!(read_varint(&mut b).unwrap(), 3, "three worlds");

    let (_, seen) = recv_until(&mut s, 0x0B).await; // Chunk Batch Finished
    let seen = ids(&seen);
    assert!(seen.contains(&0x49), "player position");
    assert!(seen.contains(&0x27), "game event 13");
    assert_eq!(
        seen.iter().filter(|&&id| id == 0x2E).count(),
        9 * 9,
        "view distance 4 → 9×9 chunks"
    );

    // ---- /nether ----
    let mut cmd = Vec::new();
    write_string(&mut cmd, "nether");
    send(&mut s, 0x07, &cmd).await;
    let (respawn, _) = recv_until(&mut s, 0x54).await;
    let mut b = respawn.body.as_slice();
    read_varint(&mut b).unwrap();
    assert_eq!(read_string(&mut b, 64).unwrap(), "minecraft:the_nether");
    let (_, seen) = recv_until(&mut s, 0x0B).await;
    assert_eq!(ids(&seen).iter().filter(|&&id| id == 0x2E).count(), 81);

    // ---- 걸어서 청크 경계 넘기 → 새 줄만 받기 ----
    let mut mv = Vec::new();
    for v in [1000.0f64, 80.0, 0.0] {
        mv.extend_from_slice(&v.to_be_bytes());
    }
    mv.push(1);
    send(&mut s, 0x1E, &mv).await;
    let (_, seen) = recv_until(&mut s, 0x0B).await;
    let seen = ids(&seen);
    assert!(seen.contains(&0x60), "set center");
    assert!(seen.contains(&0x26), "forget old chunks");
}
