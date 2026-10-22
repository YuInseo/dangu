use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mc_server::Server;
use mc_server::packets::status::ServerInfo;
use mc_server::protocol::{PROTOCOL_VERSION, VERSION_NAME};
use mc_server::server::Config;
use tokio::net::TcpListener;

const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let addr = std::env::var("MC_BIND").unwrap_or_else(|_| "0.0.0.0:25565".into());
    let seed = std::env::var("MC_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20260915);
    let world_dir = PathBuf::from(std::env::var("MC_WORLD").unwrap_or_else(|_| "world".into()));

    let info = ServerInfo {
        name: "Dangu MC".into(),
        description: "순수 Rust 커스텀 서버입니다".into(),
        max_players: 100,
    };
    // server.properties의 online-mode, network-compression-threshold에 해당합니다.
    let config = Config {
        online_mode: env_flag("MC_ONLINE_MODE", true),
        compression_threshold: match std::env::var("MC_COMPRESSION")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
        {
            Some(t) if t < 0 => None,
            Some(t) => Some(t as usize),
            None => Some(256),
        },
        offline_skins: env_flag("MC_OFFLINE_SKINS", false),
    };
    let server = Arc::new(Server::open(info, seed, world_dir.clone())?.with_config(config.clone()));
    if config.online_mode {
        server.key(); // 첫 접속이 키 만드는 시간을 기다리지 않도록 미리
    }
    println!(
        "정품 인증 {}, 압축 {}",
        if config.online_mode {
            "켬"
        } else {
            "끔 (오프라인 모드)"
        },
        config
            .compression_threshold
            .map_or("끔".to_owned(), |t| format!("{t}바이트 이상")),
    );
    println!(
        "월드 {} 읽음: 바뀐 블록 {}개",
        world_dir.display(),
        server.world.changed_block_count()
    );

    let listener = TcpListener::bind(&addr).await?;
    println!(
        "{addr}에서 듣는 중 — Minecraft {VERSION_NAME} (프로토콜 {PROTOCOL_VERSION}), 시드 {seed}"
    );

    let saver = Arc::clone(&server);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(AUTOSAVE_INTERVAL);
        loop {
            tick.tick().await;
            save(&saver);
        }
    });

    tokio::select! {
        result = mc_server::serve(listener, Arc::clone(&server)) => result?,
        _ = tokio::signal::ctrl_c() => println!("끄는 중…"),
    }
    save(&server);
    Ok(())
}

fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name).as_deref() {
        Ok("true" | "1" | "yes" | "on") => true,
        Ok("false" | "0" | "no" | "off") => false,
        _ => default,
    }
}

fn save(server: &Server) {
    match server.world.save() {
        Ok(true) => println!(
            "월드 저장: 바뀐 블록 {}개",
            server.world.changed_block_count()
        ),
        Ok(false) => {}
        Err(e) => eprintln!("월드 저장 실패: {e}"),
    }
}
