//! 프레임워크 없이 tokio만으로 짜는 마인크래프트 Java Edition 26.3 서버.

pub mod connection;
pub mod json;
pub mod packets;
pub mod protocol;

use std::sync::Arc;

use tokio::net::TcpListener;

use packets::status::ServerInfo;

/// 연결을 받을 때마다 태스크 하나를 띄웁니다. 한 연결의 오류는 그 연결만 닫습니다.
pub async fn serve(listener: TcpListener, info: Arc<ServerInfo>) -> std::io::Result<()> {
    loop {
        let (stream, peer) = listener.accept().await?;
        let info = Arc::clone(&info);
        tokio::spawn(async move {
            if let Err(e) = connection::handle(stream, &info).await {
                eprintln!("[{peer}] 연결 종료: {e}");
            }
        });
    }
}
