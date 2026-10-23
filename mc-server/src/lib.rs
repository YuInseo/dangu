//! 프레임워크 없이 tokio만으로 짜는 마인크래프트 Java Edition 26.3 서버.
//!
//! 층 (아래층은 위층을 모름):
//! - [`plugins`] 이 서버만의 기능 (콘텐츠·복권·NPC …), 기능 하나 = 폴더 하나
//! - [`session`] 접속 하나의 흐름과 플레이어 행동 / [`game`] 틱으로 돌아가는 세계 / [`world`] 지형과 청크
//! - [`server`]·[`network`] 인스턴스의 공유 상태 / [`dambi`] 여러 프로세스를 묶는 마스터 노드
//! - [`packets`]·[`protocol`]·[`crypto`]·[`data`] 프로토콜과 바닐라 데이터
//!
//! 자세한 지도는 README의 "구조"를 보세요.

pub mod admin;
pub mod auth;
pub mod chat;
pub mod connection;
pub mod crypto;
pub mod dambi;
pub mod data;
pub mod envfile;
pub mod game;
pub mod json;
pub mod network;
pub mod packets;
pub mod perms;
pub mod plugins;
pub mod protocol;
pub mod resource_pack;
pub mod server;
pub mod session;
pub mod text;
pub mod uuid;
pub mod world;
pub mod yaml;

use std::sync::Arc;

use tokio::net::TcpListener;

pub use server::Server;

/// 인스턴스 하나짜리 서버를 엽니다 (이미 네트워크에 속해 있으면 그 네트워크로).
pub async fn serve(listener: TcpListener, server: Arc<Server>) -> std::io::Result<()> {
    let net = server
        .network()
        .unwrap_or_else(|| network::Network::single(server));
    serve_network(listener, net).await
}

/// 연결을 받을 때마다 태스크 하나를 띄웁니다. 한 연결의 오류는 그 연결만 닫습니다.
/// 처음에는 기본 인스턴스로 들어가고, `/server`로 다른 인스턴스로 옮깁니다.
pub async fn serve_network(
    listener: TcpListener,
    net: Arc<network::Network>,
) -> std::io::Result<()> {
    loop {
        let (stream, peer) = listener.accept().await?;
        stream.set_nodelay(true)?;
        let net = Arc::clone(&net);
        tokio::spawn(async move {
            // 리소스 팩 주소가 게임 포트를 가리킬 때: 같은 포트로 온 HTTP 요청에 팩을 줌
            if resource_pack::enabled() && resource_pack::looks_like_http(&stream).await {
                resource_pack::answer(stream).await;
                return;
            }
            if let Err(e) = connection::handle(stream, &net, peer.ip().to_string()).await {
                eprintln!("[{peer}] 연결 종료: {e}");
            }
        });
    }
}
