//! 화면과 클라이언트 상태: 난이도, 제목, 탭 목록, 서버 정보, 핑, 자동 완성, 겉모습.

use std::sync::Arc;

use tokio::io::AsyncWrite;

use super::play::Session;
use crate::admin::{self, Actor};
use crate::packets::entity as pe;
use crate::packets::hud;
use crate::protocol::frame::FrameError;
use crate::protocol::nbt::Tag;
use crate::server::{Difficulty, Event};
use crate::text;

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    pub(crate) async fn send_difficulty(&mut self, d: Difficulty) -> std::io::Result<()> {
        let body = hud::encode_difficulty(d as u8, self.server.difficulty_locked());
        self.send(hud::CHANGE_DIFFICULTY_ID, &body).await
    }

    /// 화면 가운데 큰 글씨. `제목|부제목`으로 부제목을 함께 띄웁니다.
    pub(crate) async fn show_title(&mut self, title: &str) -> std::io::Result<()> {
        let (main, sub) = title.split_once('|').unwrap_or((title, ""));
        self.send(
            hud::SET_TITLES_ANIMATION_ID,
            &hud::encode_titles_animation(10, 70, 20),
        )
        .await?;
        if !sub.trim().is_empty() {
            let body = hud::encode_text(&text::colored(sub.trim(), "gray"));
            self.send(hud::SET_SUBTITLE_TEXT_ID, &body).await?;
        }
        let body = hud::encode_text(&text::colored(main.trim(), "gold"));
        self.send(hud::SET_TITLE_TEXT_ID, &body).await
    }

    pub(crate) async fn action_bar(&mut self, message: &Tag) -> std::io::Result<()> {
        self.send(hud::SET_ACTION_BAR_TEXT_ID, &hud::encode_text(message))
            .await
    }

    /// 탭 목록 위아래 글: 서버 이름과 접속자 수.
    pub(crate) async fn send_tab_list(&mut self) -> std::io::Result<()> {
        let header = text::colored(&self.server.info.name, "aqua");
        let footer = text::colored(
            &format!(
                "{} · 접속 {}명 · 난이도 {}",
                self.server.name,
                self.server.online(),
                self.server.difficulty().display_name()
            ),
            "gray",
        );
        self.send(hud::TAB_LIST_ID, &hud::encode_tab_list(&header, &footer))
            .await
    }

    pub(crate) async fn send_server_data(&mut self) -> std::io::Result<()> {
        let motd = text::plain(&self.server.info.description);
        self.send(hud::SERVER_DATA_ID, &hud::encode_server_data(&motd))
            .await
    }

    pub(crate) async fn pong(&mut self, payload: i64) -> std::io::Result<()> {
        self.send(hud::PONG_RESPONSE_ID, &hud::encode_pong(payload))
            .await
    }

    /// `ask_server` 인자의 자동 완성.
    pub(crate) async fn suggest(&mut self, transaction: i32, line: &str) -> std::io::Result<()> {
        let (slash, line) = match line.strip_prefix('/') {
            Some(rest) => (1, rest),
            None => (0, line),
        };
        let actor = Actor {
            name: self.profile.name.clone(),
            uuid: Some(self.profile.uuid),
        };
        let (start, matches) = admin::suggest(self.server, &actor, line);
        // 위치·길이는 UTF-16이 아니라 글자 수 (클라이언트 문자열 인덱스)
        let start_chars = line[..start].chars().count() + slash;
        let length = line[start..].chars().count();
        let body =
            hud::encode_suggestions(transaction, start_chars as i32, length as i32, &matches);
        self.send(hud::COMMAND_SUGGESTIONS_ID, &body).await
    }

    /// Play 중에 바꾼 설정 (시야 거리, 겉옷 레이어, 주 손).
    pub(crate) async fn client_information(&mut self, body: &[u8]) -> Result<(), FrameError> {
        let (view, skin, arm) = hud::decode_client_information(body)?;
        self.skin_parts = skin;
        self.main_arm = arm.clamp(0, 1);
        self.broadcast_appearance().await?;
        let view = (view as i32).clamp(2, super::play::MAX_VIEW_DISTANCE);
        if view != self.view_distance {
            self.view_distance = view;
            self.update_chunks(true).await?;
        }
        Ok(())
    }

    pub(crate) fn appearance_body(&self) -> Vec<u8> {
        pe::encode_player_data(
            self.entity_id,
            self.vitals.sneaking,
            self.vitals.sprinting,
            self.skin_parts,
            self.main_arm,
        )
    }

    /// 겉모습을 나와 다른 사람들에게 알립니다 (클라이언트는 자기 겉옷 레이어도 서버가
    /// 보낸 값으로 그립니다).
    pub(crate) async fn broadcast_appearance(&mut self) -> std::io::Result<()> {
        let body = self.appearance_body();
        let (sneaking, skin, arm) = (self.vitals.sneaking, self.skin_parts, self.main_arm);
        self.server.update_player(self.profile.uuid, |s| {
            s.sneaking = sneaking;
            s.skin_parts = skin;
            s.main_arm = arm;
        });
        self.send(pe::SET_ENTITY_DATA_ID, &body).await?;
        self.server.publish(Event::PlayerData {
            uuid: self.profile.uuid,
            body: Arc::new(body),
        });
        Ok(())
    }
}
