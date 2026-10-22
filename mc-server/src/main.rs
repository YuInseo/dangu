use std::sync::Arc;

use mc_server::Server;
use mc_server::packets::status::ServerInfo;
use mc_server::protocol::{PROTOCOL_VERSION, VERSION_NAME};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let addr = std::env::var("MC_BIND").unwrap_or_else(|_| "0.0.0.0:25565".into());
    let seed = std::env::var("MC_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20260915);
    let listener = TcpListener::bind(&addr).await?;
    println!(
        "{addr}에서 듣는 중 — Minecraft {VERSION_NAME} (프로토콜 {PROTOCOL_VERSION}), 시드 {seed}"
    );

    let info = ServerInfo {
        name: "Dangu MC".into(),
        description: "순수 Rust 커스텀 서버입니다".into(),
        max_players: 100,
    };
    mc_server::serve(listener, Arc::new(Server::new(info, seed))).await
}
