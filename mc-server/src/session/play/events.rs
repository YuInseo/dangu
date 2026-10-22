//! 다른 세션·월드가 알린 소식([`Event`])을 이 클라이언트 화면에 반영합니다.

use tokio::io::AsyncWrite;

use crate::packets::entity as pe;
use crate::packets::play::{self as p, ChatKind, ListedPlayer};
use crate::packets::world::{self as pw};
use crate::protocol::frame::FrameError;
use crate::server::Event;
use crate::text;

use super::Session;

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    /// 다른 세션이 알린 소식을 이 클라이언트에 반영합니다.
    pub(super) async fn on_event(&mut self, event: Event) -> Result<(), FrameError> {
        let me = self.profile.uuid;
        match event {
            Event::Joined(state) if state.uuid != me => {
                self.send_tab_list().await?;
                if self.listed.insert(state.uuid) {
                    let entry = ListedPlayer {
                        uuid: state.uuid,
                        name: &state.name,
                        properties: &state.properties,
                        gamemode: state.gamemode,
                    };
                    let body = p::encode_player_info_add(&[entry]);
                    self.send(p::PLAYER_INFO_UPDATE_ID, &body).await?;
                }
                if state.dimension == self.dimension {
                    self.show(&state).await?;
                }
            }
            Event::PlayerData { uuid, body } if self.visible.contains_key(&uuid) => {
                self.send(pe::SET_ENTITY_DATA_ID, &body).await?;
            }
            Event::Left { uuid, .. } if uuid != me => {
                self.send_tab_list().await?;
                self.hide(uuid).await?;
                if self.listed.remove(&uuid) {
                    self.send(
                        p::PLAYER_INFO_REMOVE_ID,
                        &p::encode_player_info_remove(uuid),
                    )
                    .await?;
                }
            }
            Event::Respawned { state, .. } if state.uuid != me => {
                self.hide(state.uuid).await?;
                if state.dimension == self.dimension {
                    self.show(&state).await?;
                }
            }
            Event::ChangedDimension { state, .. } if state.uuid != me => {
                if state.dimension == self.dimension {
                    self.show(&state).await?;
                } else {
                    self.hide(state.uuid).await?;
                }
            }
            Event::Moved {
                uuid,
                entity_id,
                dimension,
                position,
                yaw,
                pitch,
                on_ground,
            } if uuid != me && dimension == self.dimension => {
                if self.visible.contains_key(&uuid) {
                    let body =
                        p::encode_teleport_entity(entity_id, position, yaw, pitch, on_ground);
                    self.send(p::TELEPORT_ENTITY_ID, &body).await?;
                    self.send(p::ROTATE_HEAD_ID, &p::encode_rotate_head(entity_id, yaw))
                        .await?;
                } else if let Some(state) = self.server.find_by_uuid(uuid) {
                    // 접속 순서가 엇갈려 아직 못 띄운 사람
                    self.show(&state).await?;
                }
            }
            Event::GameModeChanged { uuid, gamemode } => {
                if self.listed.contains(&uuid) {
                    let body = p::encode_player_info_gamemode(uuid, gamemode);
                    self.send(p::PLAYER_INFO_UPDATE_ID, &body).await?;
                }
            }
            Event::Chat {
                kind,
                sender,
                content,
            } => {
                let body = p::encode_disguised_chat(
                    kind,
                    &text::plain(&content),
                    &text::plain(&sender),
                    None,
                );
                self.send(p::DISGUISED_CHAT_ID, &body).await?;
            }
            Event::Whisper {
                to,
                sender,
                content,
            } if to == me => {
                let body = p::encode_disguised_chat(
                    ChatKind::WhisperIncoming,
                    &text::plain(&content),
                    &text::plain(&sender),
                    None,
                );
                self.send(p::DISGUISED_CHAT_ID, &body).await?;
            }
            Event::System(message) => self.message(&message).await?,
            Event::ChannelChat(m) => self.deliver_channel_chat(&m).await?,
            Event::Notice { to, message } if to.contains(&me) => {
                self.message(&text::colored(&message, "gold")).await?
            }
            Event::GuildStorageChanged { guild, page } => {
                if matches!(
                    self.window.as_ref().map(|w| &w.kind),
                    Some(crate::session::windows::WindowKind::GuildStorage { guild: g, page: p })
                        if *g == guild && *p == page
                ) {
                    self.send_window().await?;
                }
            }
            Event::SignedChat {
                sender,
                sender_name,
                index,
                signature,
                content,
                timestamp,
                salt,
                last_seen,
            } => {
                self.deliver_signed_chat(
                    sender,
                    &sender_name,
                    index,
                    &signature,
                    &content,
                    timestamp,
                    salt,
                    &last_seen,
                )
                .await?;
            }
            Event::ChatSessionOpened { uuid, session } if self.listed.contains(&uuid) => {
                self.announce_chat_session(uuid, &session).await?;
            }
            Event::TimeChanged => self.send_time().await?,
            Event::BlockChanged {
                by,
                dimension,
                pos,
                state,
                broken,
            } if dimension == self.dimension => {
                self.send(pw::BLOCK_UPDATE_ID, &pw::encode_block_update(pos, state))
                    .await?;
                // 부순 사람은 자기 클라이언트가 이미 소리와 조각을 냈습니다.
                if let (Some(broken), true) = (broken, by != me) {
                    let body =
                        pw::encode_level_event(pw::LEVEL_EVENT_BLOCK_DESTROYED, pos, broken as i32);
                    self.send(pw::LEVEL_EVENT_ID, &body).await?;
                }
            }
            Event::HandsChanged {
                uuid,
                entity_id,
                main,
                off,
                armor,
            } if self.visible.contains_key(&uuid) => {
                self.send(
                    pw::SET_EQUIPMENT_ID,
                    &pw::encode_equipment(entity_id, main, off, Some(armor)),
                )
                .await?;
            }
            Event::EntitySpawned(e) if e.dimension == self.dimension => {
                self.show_entity(&e).await?
            }
            Event::EntityMoved {
                dimension,
                id,
                pos,
                yaw,
                pitch,
                on_ground,
            } if dimension == self.dimension && self.visible_entities.contains(&id) => {
                let body = p::encode_teleport_entity(id, pos, yaw, pitch, on_ground);
                self.send(p::TELEPORT_ENTITY_ID, &body).await?;
                self.send(p::ROTATE_HEAD_ID, &p::encode_rotate_head(id, yaw))
                    .await?;
            }
            Event::EntitiesRemoved { dimension, ids } if dimension == self.dimension => {
                let ids: Vec<i32> = ids
                    .into_iter()
                    .filter(|id| self.visible_entities.remove(id))
                    .collect();
                if !ids.is_empty() {
                    self.send(p::REMOVE_ENTITIES_ID, &p::encode_remove_entities(&ids))
                        .await?;
                }
            }
            Event::ResourcePackChanged => {
                if let Some((uuid, url, hash, forced)) = crate::resource_pack::offer(&self.origin) {
                    let prompt = text::colored("서버 아이템 그림이 바뀌었습니다", "gold");
                    let body = crate::packets::configuration::encode_add_resource_pack(
                        uuid,
                        &url,
                        &hash,
                        forced,
                        Some(&prompt),
                    );
                    self.send(p::RESOURCE_PACK_PUSH_ID, &body).await?;
                    // 새 그림을 반영하도록 인벤토리도 다시
                    self.send_window().await?;
                }
            }
            Event::TransferOut { uuid, to } if uuid == me => {
                self.message(&text::colored(&format!("{to}(으)로 이동합니다…"), "aqua"))
                    .await?;
                self.save();
                let (host, port) = self.origin.clone();
                self.send(p::TRANSFER_ID, &p::encode_transfer(&host, port))
                    .await?;
            }
            Event::EntityData {
                dimension,
                id,
                meta,
            } if dimension == self.dimension && self.visible_entities.contains(&id) => {
                self.send(pe::SET_ENTITY_DATA_ID, &pe::encode_entity_data(id, &meta))
                    .await?;
            }
            Event::ItemStackChanged {
                dimension,
                id,
                item,
            } if dimension == self.dimension && self.visible_entities.contains(&id) => {
                self.send(
                    pe::SET_ENTITY_DATA_ID,
                    &pe::encode_item_entity_data(id, item),
                )
                .await?;
            }
            Event::ItemCollected {
                dimension,
                item_entity,
                collector,
                count,
            } if dimension == self.dimension => {
                self.send(
                    pe::TAKE_ITEM_ENTITY_ID,
                    &pe::encode_take_item(item_entity, collector, count),
                )
                .await?;
            }
            Event::PickupOffer {
                to,
                entity_id,
                item,
            } if to == me => {
                let taken = if self.vitals.dead {
                    0
                } else {
                    self.give(item).await?
                };
                self.server
                    .entities
                    .finish_pickup(self.server, entity_id, self.entity_id, taken);
            }
            Event::Damage {
                target,
                amount,
                damage_type,
                attacker,
                attacker_name,
            } if target == me => {
                self.damage(amount, damage_type, attacker, &attacker_name)
                    .await?;
            }
            Event::Hurt {
                dimension,
                entity_id,
                damage_type,
                attacker,
            } if dimension == self.dimension => {
                let body = pe::encode_damage_event(entity_id, damage_type, attacker);
                self.send(pe::DAMAGE_EVENT_ID, &body).await?;
            }
            Event::EntityAnimation {
                dimension,
                entity_id,
                event,
            } if dimension == self.dimension => {
                self.send(
                    pe::ENTITY_EVENT_ID,
                    &pe::encode_entity_event(entity_id, event),
                )
                .await?;
            }
            Event::Kick { uuid, reason } if uuid == me => {
                let body = p::encode_disconnect(&text::plain(&reason));
                self.send(p::DISCONNECT_ID, &body).await?;
                return Err(FrameError::Io(std::io::ErrorKind::ConnectionAborted.into()));
            }
            Event::PermissionsChanged { uuid } if uuid.is_none_or(|u| u == me) => {
                self.send_commands().await?
            }
            Event::SetGameMode { uuid, gamemode } if uuid == me => {
                self.set_gamemode(gamemode).await?;
                let msg = format!(
                    "게임 모드가 {}(으)로 바뀌었습니다.",
                    gamemode.display_name()
                );
                self.message(&text::colored(&msg, "green")).await?;
            }
            Event::DifficultyChanged(d) => {
                self.send_difficulty(d).await?;
                self.send_tab_list().await?;
            }
            Event::Title { uuid, title } if uuid == me => self.show_title(&title).await?,
            Event::SavePlayers => self.save(),
            Event::ContainerData {
                dimension,
                pos,
                values,
            } => {
                self.container_data(dimension, pos, values).await?;
            }
            Event::Explosion {
                dimension,
                center,
                power,
            } if dimension == self.dimension => {
                let reach = 2.0 * power as f64;
                let (dx, dy, dz) = (
                    self.position.0 - center.0,
                    self.position.1 + 1.62 - center.1,
                    self.position.2 - center.2,
                );
                let d = (dx * dx + dy * dy + dz * dz).sqrt();
                let knockback =
                    (d < reach && d > 0.01 && self.gamemode.takes_damage()).then(|| {
                        let k = 1.0 - d / reach;
                        (dx / d * k, dy / d * k, dz / d * k)
                    });
                let body = crate::packets::effects::encode_explosion(center, power, knockback);
                self.send(crate::packets::effects::EXPLODE_ID, &body)
                    .await?;
            }
            Event::ContainerChanged {
                dimension,
                pos,
                slot,
                item,
            } => {
                self.chest_changed(dimension, pos, slot, item).await?;
            }
            Event::Swing {
                uuid,
                entity_id,
                off_hand,
            } if self.visible.contains_key(&uuid) => {
                self.send(
                    pw::SWING_ANIMATION_ID,
                    &pw::encode_swing(entity_id, off_hand),
                )
                .await?;
            }
            _ => {}
        }
        Ok(())
    }
}
