//! 클라이언트가 보낸 Play 패킷 하나를 처리합니다 (패킷 번호 → 처리).

use tokio::io::AsyncWrite;

use crate::packets::chat as pch;
use crate::packets::container as pc;
use crate::packets::entity as pe;
use crate::packets::hud;
use crate::packets::play::{self as p};
use crate::packets::world::{self as pw};
use crate::protocol::frame::{FrameError, RawPacket};
use crate::server::Event;

use super::Session;

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    pub(super) async fn handle(&mut self, packet: RawPacket) -> Result<(), FrameError> {
        match packet.id {
            p::SB_MOVE_PLAYER_POS_ID | p::SB_MOVE_PLAYER_POS_ROT_ID if !self.vitals.dead => {
                let old = self.position;
                self.position = p::decode_position(&packet.body)?;
                let flags_at = if packet.id == p::SB_MOVE_PLAYER_POS_ID {
                    24
                } else {
                    (self.yaw, self.pitch) = p::decode_rotation_after_position(&packet.body)?;
                    32
                };
                let grounded = on_ground(&packet.body, flags_at);
                self.moved(grounded);
                self.after_move(old, grounded).await?;
                self.update_chunks(false).await?;
            }
            p::SB_MOVE_PLAYER_ROT_ID => {
                (self.yaw, self.pitch) = p::decode_rotation(&packet.body)?;
                self.moved(on_ground(&packet.body, 8));
            }
            p::SB_ACCEPT_TELEPORTATION_ID => {
                self.position = p::decode_accept_teleportation(&packet.body)?.1;
            }
            p::SB_KEEP_ALIVE_ID => {
                let id = p::decode_keep_alive(&packet.body)?;
                if self.pending_keep_alive.is_some_and(|(sent, _)| sent == id) {
                    self.pending_keep_alive = None;
                }
            }
            p::SB_CHAT_COMMAND_ID => {
                let command = p::decode_leading_string(&packet.body)?;
                self.command(command.trim()).await?;
            }
            p::SB_CHAT_ID => self.chat_message(pch::decode_chat(&packet.body)?).await?,
            pch::SB_CHAT_ACK_ID => self.chat_ack(pch::decode_chat_ack(&packet.body)?),
            pch::SB_CHAT_SESSION_UPDATE_ID => {
                self.chat_session_update(pch::decode_session_update(&packet.body)?)
                    .await?
            }
            pch::SB_CHAT_COMMAND_SIGNED_ID => {
                // 인자 서명(/msg 등)은 확인하지 않고 명령어로만 처리합니다.
                let command = p::decode_leading_string(&packet.body)?;
                self.command(command.trim()).await?;
            }
            pw::SB_PLAYER_ACTION_ID => {
                self.player_action(pw::decode_player_action(&packet.body)?)
                    .await?;
            }
            pw::SB_USE_ITEM_ON_ID => {
                self.use_item_on(pw::decode_use_item_on(&packet.body)?)
                    .await?;
            }
            pw::SB_SET_CREATIVE_MODE_SLOT_ID => {
                let (slot, item) = pw::decode_creative_slot(&packet.body)?;
                if slot == -1 {
                    self.creative_drop(item);
                } else {
                    self.set_creative_slot(slot, item);
                }
            }
            pw::SB_SET_CARRIED_ITEM_ID => {
                let slot = pw::decode_carried_item(&packet.body)?;
                if (0..9).contains(&slot) {
                    self.held = slot as usize;
                    self.stop_using_item();
                    self.hands_changed();
                }
            }
            pw::SB_PICK_ITEM_FROM_BLOCK_ID => {
                self.pick_block_any_mode(pw::decode_pick_item_from_block(&packet.body)?)
                    .await?;
            }
            pw::SB_PICK_ITEM_FROM_ENTITY_ID => {
                self.pick_entity(hud::decode_varint_only(&packet.body)?)
                    .await?;
            }
            pe::SB_ATTACK_ID => self.attack(pe::decode_entity_id(&packet.body)?).await?,
            pe::SB_CLIENT_COMMAND_ID => {
                if pe::decode_entity_id(&packet.body)? == pe::CLIENT_COMMAND_RESPAWN {
                    self.respawn().await?;
                }
            }
            pe::SB_PLAYER_COMMAND_ID => {
                let sprinting = match pe::decode_player_command(&packet.body)? {
                    pe::PLAYER_COMMAND_START_SPRINT => true,
                    pe::PLAYER_COMMAND_STOP_SPRINT => false,
                    _ => self.vitals.sprinting,
                };
                if sprinting != self.vitals.sprinting {
                    self.vitals.sprinting = sprinting;
                    self.broadcast_appearance().await?;
                }
            }
            p::SB_PLAYER_INPUT_ID => {
                // 0x20 = 웅크리기
                let sneaking = packet.body.first().is_some_and(|f| f & 0x20 != 0);
                if sneaking != self.vitals.sneaking {
                    self.vitals.sneaking = sneaking;
                    // 관전 중 웅크리면 빙의를 풉니다 (바닐라와 같음).
                    if sneaking && self.camera.is_some() {
                        self.set_camera(None).await?;
                    }
                    self.broadcast_appearance().await?;
                }
            }
            pe::SB_USE_ITEM_ID => {
                let (off_hand, sequence) = pe::decode_use_item(&packet.body)?;
                self.use_item(off_hand, sequence).await?;
            }
            pc::SB_CONTAINER_CLICK_ID => self.click(pc::decode_click(&packet.body)?).await?,
            pc::SB_CONTAINER_CLOSE_ID => self.close_window(true).await?,
            pc::SB_SELECT_TRADE_ID => {
                self.select_trade(pc::decode_select_trade(&packet.body)?)
                    .await?
            }
            pe::SB_INTERACT_ID => {
                let (target, hand, sneaking) = pe::decode_interact(&packet.body)?;
                self.interact_entity(target, hand, sneaking).await?
            }
            pw::SB_PUNCH_ID => self.server.publish(Event::Swing {
                uuid: self.profile.uuid,
                entity_id: self.entity_id,
                off_hand: false,
            }),
            hud::SB_CHANGE_GAME_MODE_ID => {
                self.request_game_mode(hud::decode_change_game_mode(&packet.body)?)
                    .await?
            }
            hud::SB_CHANGE_DIFFICULTY_ID => {
                self.request_difficulty(hud::decode_varint_only(&packet.body)?)
                    .await?
            }
            hud::SB_LOCK_DIFFICULTY_ID => {
                self.request_lock_difficulty(hud::decode_bool_only(&packet.body)?)
                    .await?
            }
            hud::SB_PING_REQUEST_ID => self.pong(hud::decode_i64_only(&packet.body)?).await?,
            hud::SB_COMMAND_SUGGESTION_ID => {
                let (id, line) = hud::decode_command_suggestion(&packet.body)?;
                self.suggest(id, &line).await?;
            }
            hud::SB_CLIENT_INFORMATION_ID => self.client_information(&packet.body).await?,
            hud::SB_PLAYER_ABILITIES_ID => {
                self.player_abilities(packet.body.first().copied().unwrap_or(0))
                    .await?
            }
            hud::SB_SPECTATOR_ACTION_ID => {
                self.spectate(hud::decode_spectator_action(&packet.body)?)
                    .await?
            }
            hud::SB_TELEPORT_TO_ENTITY_ID => {
                self.spectator_teleport(hud::decode_uuid_only(&packet.body)?)
                    .await?
            }
            p::SB_MOVE_PLAYER_STATUS_ONLY_ID => {
                let grounded = on_ground(&packet.body, 0);
                let old = self.position;
                self.after_move(old, grounded).await?;
            }
            // 나머지는 [`super::ignored`]에 이유와 함께 적어 둔 것들입니다.
            id => crate::session::ignored::note(id),
        }
        Ok(())
    }
}

/// 이동 패킷의 마지막 플래그 바이트: 0x01 = 땅에 닿음.
fn on_ground(body: &[u8], offset: usize) -> bool {
    body.get(offset).is_some_and(|f| f & 0x01 != 0)
}
