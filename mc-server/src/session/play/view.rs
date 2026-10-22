//! 이 플레이어가 보는 세상: 처음 들어가기, 청크 맞추기, 순간이동·차원 이동,
//! 다른 플레이어·엔티티를 띄우고 지우기.

use tokio::io::AsyncWrite;

use crate::packets::entity as pe;
use crate::packets::play::{self as p, GameMode, ListedPlayer, Login};
use crate::packets::world::{self as pw};
use crate::protocol::frame::FrameError;
use crate::server::{Event, PlayerState};
use crate::text;
use crate::world::{Dimension, chunk, terrain};

use super::Session;

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    pub(super) async fn join(&mut self, others: &[PlayerState]) -> Result<(), FrameError> {
        let login = Login {
            entity_id: self.entity_id,
            view_distance: self.view_distance,
            spawn: self.spawn_info(),
            online_mode: self.server.config.online_mode,
            hardcore: self.server.config.hardcore,
        };
        self.send(p::LOGIN_ID, &login.encode()).await?;
        self.send_difficulty(self.server.difficulty()).await?;
        self.send_server_data().await?;
        self.send_commands().await?;

        // 탭 목록: 나 자신과 이미 있던 사람들.
        let me = self.state();
        let listed: Vec<ListedPlayer<'_>> = std::iter::once(&me)
            .chain(others)
            .map(|s| ListedPlayer {
                uuid: s.uuid,
                name: &s.name,
                properties: &s.properties,
                gamemode: s.gamemode,
            })
            .collect();
        let body = p::encode_player_info_add(&listed);
        self.send(p::PLAYER_INFO_UPDATE_ID, &body).await?;
        self.listed
            .extend(std::iter::once(me.uuid).chain(others.iter().map(|s| s.uuid)));
        for o in others {
            if let Some(session) = &o.chat_session {
                self.announce_chat_session(o.uuid, session).await?;
            }
        }

        self.send_time().await?;
        let resume = self.resume_at.take();
        self.enter_world(resume).await?;
        self.send_inventory().await?;
        self.send(
            pw::SET_HELD_SLOT_ID,
            &pw::encode_held_slot(self.held as i32),
        )
        .await?;
        self.send_health().await?;
        self.broadcast_appearance().await?;
        self.send_tab_list().await?;
        self.message(&text::colored(
            "순수 Rust 서버에 오신 것을 환영합니다! /help 로 명령어를 볼 수 있습니다.",
            "aqua",
        ))
        .await?;
        Ok(())
    }

    /// 지금 차원의 스폰 지점(또는 `at`)에 세우고 주변 청크와 사람·엔티티를 보냅니다.
    /// 접속 직후, 차원 이동과 부활 후에 씁니다.
    pub(crate) async fn enter_world(
        &mut self,
        at: Option<(f64, f64, f64)>,
    ) -> Result<(), FrameError> {
        let spawn = terrain::spawn_point(self.dimension, self.server.seed);
        // 새 차원에 들어가면 클라이언트도 청크와 엔티티를 모두 버립니다.
        self.loaded.clear();
        self.visible.clear();
        self.visible_entities.clear();
        self.reset_fall();

        self.send(p::PLAYER_ABILITIES_ID, &p::encode_abilities(self.gamemode))
            .await?;
        self.rank.abilities_reset();
        let block = (
            spawn.0.floor() as i32,
            spawn.1.floor() as i32,
            spawn.2.floor() as i32,
        );
        let body = p::encode_spawn_position(self.dimension, block);
        self.send(p::SET_DEFAULT_SPAWN_POSITION_ID, &body).await?;
        self.teleport(at.unwrap_or(spawn)).await?;
        let body = p::encode_game_event(p::GAME_EVENT_LEVEL_CHUNKS_LOAD_START, 0.0);
        self.send(p::GAME_EVENT_ID, &body).await?;
        self.update_chunks(true).await?;

        for other in self.server.players() {
            if other.uuid != self.profile.uuid && other.dimension == self.dimension && !other.dead {
                self.show(&other).await?;
            }
        }
        for e in self.server.entities.snapshots_in(self.dimension) {
            self.show_entity(&e).await?;
        }
        Ok(())
    }

    /// 이 클라이언트에 떠 있는 엔티티인가 (플레이어 포함).
    pub(crate) fn sees_entity(&self, id: i32) -> bool {
        self.visible_entities.contains(&id) || self.visible.values().any(|&e| e == id)
    }

    pub(super) async fn show_entity(
        &mut self,
        e: &crate::game::entities::Snapshot,
    ) -> std::io::Result<()> {
        if !self.visible_entities.insert(e.id) {
            return Ok(());
        }
        let body = pe::encode_add_entity(e.id, e.uuid, e.type_id, e.pos, e.vel, e.yaw, e.pitch);
        self.send(p::ADD_ENTITY_ID, &body).await?;
        if let Some(item) = e.item {
            self.send(
                pe::SET_ENTITY_DATA_ID,
                &pe::encode_item_entity_data(e.id, item),
            )
            .await?;
        }
        if !e.meta.is_empty() {
            self.send(
                pe::SET_ENTITY_DATA_ID,
                &pe::encode_entity_data(e.id, &e.meta),
            )
            .await?;
        }
        Ok(())
    }

    /// 지금 차원 안에서 옮깁니다. 다른 사람들에게도 알립니다.
    pub(crate) async fn teleport(&mut self, pos: (f64, f64, f64)) -> std::io::Result<()> {
        let id = self.next_teleport_id;
        self.next_teleport_id += 1;
        self.position = pos;
        self.reset_fall();
        self.send(
            p::PLAYER_POSITION_ID,
            &p::encode_player_position(id, pos, self.yaw, self.pitch),
        )
        .await?;
        self.moved(true);
        Ok(())
    }

    pub(crate) async fn teleport_and_load(
        &mut self,
        pos: (f64, f64, f64),
    ) -> Result<(), FrameError> {
        self.teleport(pos).await?;
        self.update_chunks(false).await
    }

    pub(crate) async fn change_dimension(
        &mut self,
        dimension: Dimension,
    ) -> Result<(), FrameError> {
        self.change_dimension_to(dimension, None).await
    }

    /// 차원을 바꾸고 `at`(없으면 스폰)에 세웁니다.
    pub(crate) async fn change_dimension_to(
        &mut self,
        dimension: Dimension,
        at: Option<(f64, f64, f64)>,
    ) -> Result<(), FrameError> {
        let from = self.dimension;
        self.dimension = dimension;
        self.server
            .update_player(self.profile.uuid, |s| s.dimension = dimension);
        self.send(p::RESPAWN_ID, &p::encode_respawn(&self.spawn_info()))
            .await?;
        self.enter_world(at).await?;
        self.send_health().await?;
        self.server.publish(Event::ChangedDimension {
            from,
            state: self.state(),
        });
        Ok(())
    }

    pub(crate) async fn set_gamemode(&mut self, mode: GameMode) -> Result<(), FrameError> {
        if mode == self.gamemode {
            return Ok(());
        }
        self.previous_gamemode = Some(self.gamemode);
        if self.gamemode == GameMode::Spectator {
            self.set_camera(None).await?;
        }
        if mode == GameMode::Spectator {
            self.close_window(false).await?;
        }
        self.flying = mode == GameMode::Spectator;
        self.reset_fall();
        self.gamemode = mode;
        let body = p::encode_game_event(p::GAME_EVENT_CHANGE_GAME_MODE, mode.id() as f32);
        self.send(p::GAME_EVENT_ID, &body).await?;
        self.send(p::PLAYER_ABILITIES_ID, &p::encode_abilities(mode))
            .await?;
        self.rank.abilities_reset();
        self.server
            .update_player(self.profile.uuid, |s| s.gamemode = mode);
        self.server.publish(Event::GameModeChanged {
            uuid: self.profile.uuid,
            gamemode: mode,
        });
        Ok(())
    }

    /// 내 위치·시선이 바뀌었음을 명단과 다른 사람들에게 알립니다.
    pub(super) fn moved(&mut self, on_ground: bool) {
        let (position, yaw, pitch) = (self.position, self.yaw, self.pitch);
        self.server.update_player(self.profile.uuid, |s| {
            s.position = position;
            s.yaw = yaw;
            s.pitch = pitch;
        });
        self.server.publish(Event::Moved {
            uuid: self.profile.uuid,
            entity_id: self.entity_id,
            dimension: self.dimension,
            position,
            yaw,
            pitch,
            on_ground,
        });
    }

    /// 플레이어 주변 청크를 맞춥니다: 멀어진 것은 잊게 하고, 새로 들어온 것은 보냅니다.
    pub(crate) async fn update_chunks(&mut self, force: bool) -> Result<(), FrameError> {
        let center = (
            (self.position.0.floor() as i32) >> 4,
            (self.position.2.floor() as i32) >> 4,
        );
        if !force && center == self.center {
            return Ok(());
        }
        self.center = center;
        let body = p::encode_chunk_xz_varint(center.0, center.1);
        self.send(p::SET_CHUNK_CACHE_CENTER_ID, &body).await?;

        let r = self.view_distance;
        let in_range =
            |&(x, z): &(i32, i32)| (x - center.0).abs() <= r && (z - center.1).abs() <= r;

        let stale: Vec<_> = self
            .loaded
            .iter()
            .copied()
            .filter(|c| !in_range(c))
            .collect();
        for (x, z) in stale {
            self.loaded.remove(&(x, z));
            self.send(p::FORGET_LEVEL_CHUNK_ID, &p::encode_forget_chunk(x, z))
                .await?;
        }

        let mut wanted: Vec<(i32, i32)> = (-r..=r)
            .flat_map(|dx| (-r..=r).map(move |dz| (center.0 + dx, center.1 + dz)))
            .filter(|c| !self.loaded.contains(c))
            .collect();
        if wanted.is_empty() {
            return Ok(());
        }
        // 가까운 것부터: 발밑 청크가 먼저 와야 로딩 화면이 빨리 끝납니다.
        wanted.sort_by_key(|&(x, z)| (x - center.0).pow(2) + (z - center.1).pow(2));

        // 읽기·생성·인코딩은 CPU·디스크 일이므로 비동기 런타임 밖에서.
        let dimension = self.dimension;
        let world = std::sync::Arc::clone(&self.server.world);
        let jobs = wanted.clone();
        let bodies = tokio::task::spawn_blocking(move || {
            jobs.iter()
                .map(|&(x, z)| chunk::encode(&world.chunk(dimension, x, z), x, z))
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|e| FrameError::Io(std::io::Error::other(e)))?;

        self.send(p::CHUNK_BATCH_START_ID, &[]).await?;
        for body in &bodies {
            self.send(p::LEVEL_CHUNK_WITH_LIGHT_ID, body).await?;
        }
        let body = p::encode_batch_finished(bodies.len() as i32);
        self.send(p::CHUNK_BATCH_FINISHED_ID, &body).await?;
        self.loaded.extend(wanted);
        Ok(())
    }

    /// 다른 플레이어를 엔티티로 띄웁니다 (이미 떠 있으면 그대로).
    pub(super) async fn show(&mut self, other: &PlayerState) -> std::io::Result<()> {
        if self.visible.contains_key(&other.uuid) || !self.listed.contains(&other.uuid) {
            return Ok(());
        }
        let body = p::encode_add_player_entity(
            other.entity_id,
            other.uuid,
            other.position,
            other.yaw,
            other.pitch,
        );
        self.send(p::ADD_ENTITY_ID, &body).await?;
        let looks = pe::encode_player_data(
            other.entity_id,
            other.sneaking,
            false,
            other.skin_parts,
            other.main_arm,
        );
        self.send(pe::SET_ENTITY_DATA_ID, &looks).await?;
        if other.main_hand.is_some()
            || other.off_hand.is_some()
            || other.armor.iter().any(Option::is_some)
        {
            let body = pw::encode_equipment(
                other.entity_id,
                other.main_hand,
                other.off_hand,
                Some(other.armor),
            );
            self.send(pw::SET_EQUIPMENT_ID, &body).await?;
        }
        self.visible.insert(other.uuid, other.entity_id);
        Ok(())
    }

    pub(super) async fn hide(&mut self, uuid: u128) -> std::io::Result<()> {
        if let Some(entity_id) = self.visible.remove(&uuid) {
            self.send(
                p::REMOVE_ENTITIES_ID,
                &p::encode_remove_entities(&[entity_id]),
            )
            .await?;
        }
        Ok(())
    }
}
