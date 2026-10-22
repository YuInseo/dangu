//! 핸드셰이크 이후의 접속 흐름: Login → Configuration → Play.

mod abilities;
mod commands;
mod configuration;
mod hud;
mod ignored;
mod interact;
mod login;
mod modes;
mod play;
mod signed_chat;
mod survival;
mod windows;

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::auth::Property;
use crate::protocol::conn::Conn;
use crate::protocol::frame::FrameError;
use crate::server::Server;

/// 버전 확인을 마친 접속 하나를 끝까지 처리합니다.
pub async fn run<S>(mut conn: Conn<S>, server: &Server, ip: String) -> Result<(), FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let Some(profile) = login::run(&mut conn, server, &ip).await? else {
        return Ok(());
    };
    let Some(mut settings) = configuration::run(&mut conn).await? else {
        return Ok(());
    };
    settings.ip = ip;
    play::run(conn, server, profile, settings).await
}

/// 로그인한 사람.
#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub uuid: u128,
    /// 스킨 등 프로필 속성 (정품이면 Mojang이 서명한 것).
    pub properties: Arc<Vec<Property>>,
}

/// Configuration에서 받은 클라이언트 설정 중 쓰는 것.
#[derive(Debug, Clone)]
pub struct ClientSettings {
    pub view_distance: u8,
    /// 겉옷(모자·소매 등) 레이어 비트.
    pub skin_parts: u8,
    /// 0 왼손, 1 오른손.
    pub main_hand: i32,
    pub ip: String,
}
