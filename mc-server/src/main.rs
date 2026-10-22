use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mc_server::Server;
use mc_server::packets::status::ServerInfo;
use mc_server::protocol::{PROTOCOL_VERSION, VERSION_NAME};
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
    let server = Arc::new(Server::open(info, seed, world_dir.clone())?);
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
