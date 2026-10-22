//! 세션 쪽 서명된 채팅: 채팅 세션 받기, 서명 확인, 서명째 전하기.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::io::AsyncWrite;

use super::play::{Session, sanitize};
use crate::chat::{self, ChatSession, LastSeenValidator};
use crate::crypto::rsa::PublicKey;
use crate::packets::chat::{self as pch, ChatMessage, SignedChat};
use crate::packets::play::{self as p, ChatKind};
use crate::protocol::frame::FrameError;
use crate::server::Event;
use crate::text;

/// 한 클라이언트의 서명된 채팅 상태.
#[derive(Default)]
pub struct ChatState {
    /// Mojang 서명을 확인한 세션과 그 키.
    session: Option<(Arc<ChatSession>, PublicKey)>,
    /// 이 사람이 보낼 다음 메시지의 순번 (세션마다 0부터).
    next_index: i32,
    /// 이 사람에게 보낸 서명된 메시지들.
    last_seen: LastSeenValidator,
    /// 이 사람에게 보낸 플레이어 채팅 수 (`Player Chat`의 global index).
    received: i32,
}

/// 받아 두지 않은 서명된 메시지가 이만큼 쌓이면 끊습니다 (바닐라와 같은 4096).
const MAX_PENDING: usize = 4096;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    pub(super) fn chat_session(&self) -> Option<Arc<ChatSession>> {
        self.chat.session.as_ref().map(|(s, _)| s.clone())
    }

    pub(super) async fn chat_session_update(
        &mut self,
        session: ChatSession,
    ) -> Result<(), FrameError> {
        let keys = self.server.chat_keys.lock().unwrap().clone();
        // 정품 인증을 거친 UUID여야 키 서명이 맞습니다 (오프라인 UUID로는 실패).
        let valid = self.server.config.online_mode
            && session.is_signed_by(self.profile.uuid, &keys, now_ms());
        let key = session.public_key();
        match (valid, key) {
            (true, Some(key)) => {
                let session = Arc::new(session);
                self.chat = ChatState {
                    session: Some((session.clone(), key)),
                    ..ChatState::default()
                };
                self.server.update_player(self.profile.uuid, |s| {
                    s.chat_session = Some(session.clone())
                });
                self.server.publish(Event::ChatSessionOpened {
                    uuid: self.profile.uuid,
                    session,
                });
            }
            _ => {
                // 확인할 수 없는 키: 이 사람의 채팅은 서명 없이 전합니다.
                self.chat.session = None;
            }
        }
        Ok(())
    }

    pub(super) async fn chat_message(&mut self, m: ChatMessage) -> Result<(), FrameError> {
        let clean = sanitize(&m.message);
        if clean.is_empty() {
            return Ok(());
        }
        // 서명된 메시지는 클라이언트 쪽 순번이 늘었으므로, 결과와 상관없이 우리도 늘립니다.
        let index = self.chat.next_index;
        if m.signature.is_some() {
            self.chat.next_index += 1;
        }
        let last_seen = self.chat.last_seen.apply_update(m.offset, m.acknowledged);

        let verified = match (&self.chat.session, m.signature, &last_seen) {
            (Some((session, key)), Some(sig), Ok(seen)) if clean == m.message => {
                let payload = chat::signed_payload(
                    self.profile.uuid,
                    session.session_id,
                    index,
                    m.salt,
                    m.timestamp,
                    &m.message,
                    seen,
                );
                key.verify_sha256(&payload, &sig)
                    .then_some((sig, seen.clone()))
            }
            _ => None,
        };
        match verified {
            Some((signature, last_seen)) => self.server.publish(Event::SignedChat {
                sender: self.profile.uuid,
                sender_name: self.profile.name.clone(),
                index,
                signature: Box::new(signature),
                content: m.message,
                timestamp: m.timestamp,
                salt: m.salt,
                last_seen,
            }),
            None => self.server.publish(Event::Chat {
                kind: ChatKind::Chat,
                sender: self.profile.name.clone(),
                content: clean,
            }),
        }
        Ok(())
    }

    pub(super) fn chat_ack(&mut self, offset: i32) {
        // 틀린 offset은 바닐라라면 끊지만, 여기서는 서명 확인이 실패하는 것으로 충분합니다.
        let _ = self.chat.last_seen.apply_offset(offset);
    }

    /// 다른 사람(또는 나)의 서명된 채팅을 받음.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn deliver_signed_chat(
        &mut self,
        sender: u128,
        sender_name: &str,
        index: i32,
        signature: &chat::Signature,
        content: &str,
        timestamp: i64,
        salt: i64,
        last_seen: &[chat::Signature],
    ) -> Result<(), FrameError> {
        let name = text::plain(sender_name);
        let body = pch::encode_player_chat(&SignedChat {
            global_index: self.chat.received,
            sender,
            index,
            signature,
            content,
            timestamp,
            salt,
            last_seen,
            kind: ChatKind::Chat,
            sender_name: &name,
        });
        self.chat.received += 1;
        self.chat.last_seen.add_pending(*signature);
        self.send(pch::PLAYER_CHAT_ID, &body).await?;
        if self.chat.last_seen.pending_count() > MAX_PENDING {
            let reason = text::plain("확인하지 않은 채팅 메시지가 너무 많습니다.");
            self.send(p::DISCONNECT_ID, &p::encode_disconnect(&reason))
                .await?;
            return Err(FrameError::Io(std::io::ErrorKind::ConnectionAborted.into()));
        }
        Ok(())
    }

    /// 다른 사람의 채팅 세션을 탭 목록에 붙입니다.
    pub(super) async fn announce_chat_session(
        &mut self,
        uuid: u128,
        session: &ChatSession,
    ) -> std::io::Result<()> {
        self.send(
            p::PLAYER_INFO_UPDATE_ID,
            &pch::encode_info_chat_session(uuid, session),
        )
        .await
    }
}
