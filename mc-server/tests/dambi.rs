//! 담비 마스터 노드를 실제 프로세스로: 마스터가 인스턴스 두 개를 자식 프로세스로 띄우고,
//! 클라이언트가 프록시를 거쳐 기본 인스턴스에 들어간 뒤, CLI의 `이동`으로 Transfer 패킷을 받아
//! 다시 접속하면 다른 인스턴스(다른 프로세스)로 이어지는지.

mod common;

use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;

use common::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// 마스터 출력에서 `pat`이 들어간 줄을 기다립니다.
async fn wait_line(rx: &mut mpsc::UnboundedReceiver<String>, pat: &str) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let line = tokio::time::timeout_at(deadline, rx.recv())
            .await
            .unwrap_or_else(|_| panic!("기다린 줄이 안 나옴: {pat}"))
            .expect("마스터가 끝남");
        if line.contains(pat) {
            return line;
        }
    }
}

#[tokio::test]
async fn master_routes_players_between_child_processes() {
    let home = std::env::temp_dir().join(format!("dambi-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let (p1, p2, public) = (free_port(), free_port(), free_port());
    std::fs::write(
        home.join("dambi.json"),
        format!(
            r#"{{"instances": [
                {{"name": "마을", "port": {p1}, "ram": "1G", "autostart": true}},
                {{"name": "rpg", "port": {p2}, "autostart": true}}
            ]}}"#
        ),
    )
    .unwrap();

    let mut master = Command::new(env!("CARGO_BIN_EXE_mc-server"))
        .arg("master")
        .env("DAMBI_HOME", &home)
        .env("MC_BIND", format!("127.0.0.1:{public}"))
        .env("MC_ONLINE_MODE", "false")
        .env("MC_PREGEN", "0")
        .env("MC_SPAWN_MOBS", "false")
        .env("MC_PACK_PORT", "0")
        .env("MC_SEED", SEED.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = master.stdin.take().unwrap();
    let (tx, mut out) = mpsc::unbounded_channel();
    let stdout = master.stdout.take().unwrap();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            println!("{l}");
            if tx.send(l).is_err() {
                break;
            }
        }
    });
    // 두 자식은 어느 쪽이 먼저 준비될지 모름
    let mut ready = std::collections::HashSet::new();
    while ready.len() < 2 {
        let l = wait_line(&mut out, "준비됨").await;
        ready.insert(if l.contains("마을") {
            "마을"
        } else {
            "rpg"
        });
    }

    // 서버 목록 핑은 마스터가 직접
    let addr: SocketAddr = format!("127.0.0.1:{public}").parse().unwrap();
    let mut ping = connect(addr).await;
    send(&mut ping, 0x00, &handshake_body(1)).await;
    send(&mut ping, 0x00, &[]).await;
    let status = recv(&mut ping).await;
    assert!(String::from_utf8_lossy(&status.body).contains("Dambi"));

    // 처음엔 기본 인스턴스(마을)로
    let (mut alice, _) = join(addr, "Alice", 2).await;
    wait_line(&mut out, "Alice (127.0.0.1) → 마을").await;
    // 자식이 Joined를 알릴 틈
    for _ in 0..50 {
        stdin.write_all("players\n".as_bytes()).await.unwrap();
        let l = wait_line(&mut out, "접속").await;
        if l.contains("1명") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    stdin
        .write_all("재화 Alice 지급 1500 캐시\n".as_bytes())
        .await
        .unwrap();
    wait_line(&mut out, "캐시 1500").await;

    // CLI로 옮기기 → 클라이언트가 Transfer(0x84)를 받음
    stdin
        .write_all("이동 Alice rpg\n".as_bytes())
        .await
        .unwrap();
    wait_line(&mut out, "Alice → rpg").await;
    let transfer = recv_where(&mut alice, |p| p.id == 0x84).await;
    let mut body = &transfer.body[..];
    let host = mc_server::protocol::types::read_string(&mut body, 255).unwrap();
    assert_eq!(
        host, "localhost",
        "client is sent back to the address it used"
    );
    drop(alice);

    // 다시 접속하면 마스터가 rpg 프로세스로 잇고, 재화는 그대로
    let (_alice, _) = join(addr, "Alice", 2).await;
    wait_line(&mut out, "Alice (127.0.0.1) → rpg").await;
    stdin.write_all("재화 Alice\n".as_bytes()).await.unwrap();
    wait_line(&mut out, "캐시 1500").await;

    stdin.write_all("인스턴스 목록\n".as_bytes()).await.unwrap();
    let l = wait_line(&mut out, "마을").await;
    assert!(l.contains("켜짐"), "{l}");

    stdin.write_all("종료\n".as_bytes()).await.unwrap();
    let status = tokio::time::timeout(Duration::from_secs(30), master.wait())
        .await
        .expect("마스터가 꺼지지 않음")
        .unwrap();
    assert!(status.success());
    // SharedData가 디스크에
    let files = std::fs::read_dir(home.join("shared")).unwrap().count();
    assert_eq!(files, 1);
    std::fs::remove_dir_all(home).ok();
}
