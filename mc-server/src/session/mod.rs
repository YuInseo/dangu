//! 핸드셰이크 이후의 접속 흐름: Login → Configuration → Play.

mod configuration;
mod login;
mod play;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::protocol::frame::FrameError;
use crate::server::Server;

/// 버전 확인을 마친 접속 하나를 끝까지 처리합니다.
pub async fn run<S>(mut stream: S, server: &Server) -> Result<(), FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let profile = login::run(&mut stream).await?;
    let Some(settings) = configuration::run(&mut stream).await? else {
        return Ok(());
    };
    play::run(stream, server, profile, settings).await
}

/// 로그인한 사람.
#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub uuid: u128,
}

/// Configuration에서 받은 클라이언트 설정 중 쓰는 것.
#[derive(Debug, Clone, Copy)]
pub struct ClientSettings {
    pub view_distance: u8,
}
