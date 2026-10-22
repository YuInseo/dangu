//! 핸드셰이크 이후의 접속 흐름: Login → Configuration → Play.

mod abilities;
mod commands;
mod configuration;
mod hud;
mod ignored;
mod interact;
mod login;
mod merchant;
mod modes;
pub(crate) mod play;
mod signed_chat;
mod survival;
pub(crate) mod windows;

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::auth::Property;
use crate::network::Network;
use crate::protocol::conn::Conn;
use crate::protocol::frame::FrameError;

/// 버전 확인을 마친 접속 하나를 끝까지 처리합니다. 로그인은 기본 인스턴스에서 하고,
/// Play가 다른 인스턴스로 옮기자고 하면 Configuration을 다시 거쳐 그쪽 Play로 들어갑니다.
pub async fn run<S>(
    mut conn: Conn<S>,
    net: &Arc<Network>,
    ip: String,
    origin: (String, u16),
) -> Result<(), FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut server = net.default_instance();
    let Some(profile) = login::run(&mut conn, &server, &ip).await? else {
        return Ok(());
    };
    let mut settings = ClientSettings::new(ip);
    settings.origin = origin;
    loop {
        settings = match configuration::run(&mut conn, settings).await? {
            Some(s) => s,
            None => return Ok(()),
        };
        match play::run(conn, &server, profile.clone(), settings).await? {
            play::Outcome::Quit => return Ok(()),
            play::Outcome::Transfer {
                conn: back,
                to,
                settings: kept,
            } => {
                conn = back;
                settings = kept;
                server = to;
            }
        }
    }
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
    /// 클라이언트가 접속한 주소와 포트 (Transfer 패킷으로 다시 보낼 곳).
    pub origin: (String, u16),
    /// 서버 리소스 팩을 이미 적용함 (인스턴스를 옮겨 다시 설정할 때 또 묻지 않음).
    pub pack_loaded: bool,
}

impl ClientSettings {
    pub fn new(ip: String) -> Self {
        Self {
            view_distance: configuration::DEFAULT_VIEW_DISTANCE,
            skin_parts: 0x7F,
            main_hand: 1,
            ip,
            origin: ("localhost".into(), 25565),
            pack_loaded: false,
        }
    }
}
