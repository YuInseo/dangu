//! 프레임워크 없이 tokio만으로 짜는 마인크래프트 Java Edition 26.3 서버.

pub mod connection;
pub mod data;
pub mod json;
pub mod packets;
pub mod protocol;
pub mod server;
pub mod session;
pub mod text;
pub mod uuid;
pub mod world;

use std::sync::Arc;

use tokio::net::TcpListener;

pub use server::Server;

/// 연결을 받을 때마다 태스크 하나를 띄웁니다. 한 연결의 오류는 그 연결만 닫습니다.
pub async fn serve(listener: TcpListener, server: Arc<Server>) -> std::io::Result<()> {
    loop {
        let (stream, peer) = listener.accept().await?;
        stream.set_nodelay(true)?;
        let server = Arc::clone(&server);
        tokio::spawn(async move {
            if let Err(e) = connection::handle(stream, &server).await {
                eprintln!("[{peer}] 연결 종료: {e}");
            }
        });
    }
}
