//! Play 상태: 월드에 들어가 돌아다니고, 다른 사람과 보고 말하기.
//!
//! 소켓을 읽기·쓰기 반쪽으로 나누고, 읽기는 별도 태스크가 채널로 넘깁니다.
//! `read_packet`은 도중에 취소되면 읽던 바이트를 잃기 때문에 `select!`에 직접 넣을 수
//! 없어서입니다. 메인 루프는 받은 패킷, keep-alive 타이머, 서버 이벤트를 함께 기다립니다.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc};

use super::commands;
use super::{ClientSettings, Profile};
use crate::packets::play::{self as p, ChatKind, GameMode, ListedPlayer, Login, SpawnInfo};
use crate::packets::world::{self as pw, Item};
use crate::protocol::frame::{FrameError, RawPacket, read_packet, write_packet};
use crate::protocol::nbt::Tag;
use crate::server::{AlreadyOnline, Event, PlayerState, Server};
use crate::text;
use crate::world::store::WorldStore;
use crate::world::{Dimension, chunk, terrain};

const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(10);
/// 이 시간 안에 keep-alive 응답이 없으면 끊습니다 (바닐라와 같은 30초).
const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(30);
/// 클라이언트가 더 멀리 보겠다고 해도 이만큼만 보냅니다. 압축이 없어서 청크가 큽니다.
const MAX_VIEW_DISTANCE: i32 = 8;
/// 바닐라와 같은 채팅 길이 제한.
const MAX_CHAT_LENGTH: usize = 256;

pub async fn run<S>(
    stream: S,
    server: &Server,
    profile: Profile,
    settings: ClientSettings,
) -> Result<(), FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut reader, writer) = tokio::io::split(stream);
    let (tx, mut rx) = mpsc::channel::<Result<RawPacket, FrameError>>(256);
    let reader_task = tokio::spawn(async move {
        loop {
            match read_packet(&mut reader).await {
                Ok(Some(packet)) => {
                    if tx.send(Ok(packet)).await.is_err() {
                        return;
                    }
                }
                Ok(None) => return,
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            }
        }
    });

    // 명단에 오르기 전에 구독해야 그 사이의 소식을 놓치지 않습니다.
    let mut events = server.subscribe();
    let mut session = Session::new(writer, server, profile, settings);
    let others = match server.add_player(session.state()) {
        Ok(others) => others,
        Err(AlreadyOnline) => {
            let reason = text::plain("같은 이름으로 이미 접속해 있습니다.");
            session
                .send(p::DISCONNECT_ID, &p::encode_disconnect(&reason))
                .await?;
            reader_task.abort();
            return Ok(());
        }
    };

    let result = async {
        session.join(&others).await?;
        server.publish(Event::System(text::colored(
            &format!("{}님이 들어왔습니다", session.profile.name),
            "yellow",
        )));

        let mut keep_alive = tokio::time::interval(KEEP_ALIVE_INTERVAL);
        keep_alive.tick().await; // 첫 tick은 즉시 오므로 버립니다
        loop {
            tokio::select! {
                packet = rx.recv() => match packet {
                    Some(Ok(packet)) => session.handle(packet).await?,
                    Some(Err(e)) => return Err(e),
                    None => return Ok(()), // 클라이언트가 연결을 닫음
                },
                _ = keep_alive.tick() => session.keep_alive().await?,
                event = events.recv() => match event {
                    Ok(event) => session.on_event(event).await?,
                    // 너무 밀렸으면 몇 개 버리고 계속합니다. 위치는 다음 이동에서 다시 맞춰집니다.
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                },
            }
        }
    }
    .await;

    reader_task.abort();
    server.remove_player(session.profile.uuid);
    server.publish(Event::System(text::colored(
        &format!("{}님이 나갔습니다", session.profile.name),
        "yellow",
    )));
    println!("{} 접속 종료", session.profile.name);
    result
}

pub(super) struct Session<'a, W> {
    w: W,
    pub(super) server: &'a Server,
    pub(super) profile: Profile,
    pub(super) entity_id: i32,
    view_distance: i32,
    pub(super) dimension: Dimension,
    pub(super) gamemode: GameMode,
    pub(super) position: (f64, f64, f64),
    pub(super) yaw: f32,
    pub(super) pitch: f32,
    /// 인벤토리 메뉴 번호 그대로: 9..36 가방, 36..45 핫바, 45 보조 손.
    pub(super) inventory: [Option<Item>; 46],
    /// 고른 핫바 칸 (0..9).
    pub(super) held: usize,
    /// 클라이언트에 보낸(그리고 아직 잊으라고 하지 않은) 청크.
    loaded: HashSet<(i32, i32)>,
    center: (i32, i32),
    next_teleport_id: i32,
    /// 보냈지만 아직 답을 못 받은 keep-alive.
    pending_keep_alive: Option<(i64, Instant)>,
    /// 탭 목록에 올린 사람들.
    listed: HashSet<u128>,
    /// 이 클라이언트에 엔티티로 띄운 다른 플레이어: UUID → 엔티티 ID.
    visible: HashMap<u128, i32>,
}

impl<'a, W: AsyncWrite + Unpin> Session<'a, W> {
    fn new(w: W, server: &'a Server, profile: Profile, settings: ClientSettings) -> Self {
        let dimension = Dimension::Overworld;
        Self {
            w,
            server,
            profile,
            entity_id: server.allocate_entity_id(),
            view_distance: (settings.view_distance as i32).clamp(2, MAX_VIEW_DISTANCE),
            dimension,
            gamemode: GameMode::Creative,
            position: terrain::spawn_point(dimension, server.seed),
            yaw: 0.0,
            pitch: 0.0,
            inventory: [None; 46],
            held: 0,
            loaded: HashSet::new(),
            center: (0, 0),
            next_teleport_id: 1,
            pending_keep_alive: None,
            listed: HashSet::new(),
            visible: HashMap::new(),
        }
    }

    fn state(&self) -> PlayerState {
        PlayerState {
            uuid: self.profile.uuid,
            name: self.profile.name.clone(),
            entity_id: self.entity_id,
            dimension: self.dimension,
            position: self.position,
            yaw: self.yaw,
            pitch: self.pitch,
            gamemode: self.gamemode,
            main_hand: self.main_hand(),
            off_hand: self.off_hand(),
        }
    }

    pub(super) fn main_hand(&self) -> Option<Item> {
        self.inventory[36 + self.held]
    }

    pub(super) fn off_hand(&self) -> Option<Item> {
        self.inventory[45]
    }

    fn spawn_info(&self) -> SpawnInfo {
        SpawnInfo {
            dimension: self.dimension,
            // 클라이언트는 이 값을 바이옴 색 섞기에만 씁니다. 실제 시드를 줄 필요는 없습니다.
            hashed_seed: self.server.seed as i64,
            gamemode: self.gamemode,
        }
    }

    pub(super) async fn send(&mut self, id: i32, body: &[u8]) -> std::io::Result<()> {
        write_packet(&mut self.w, id, body).await
    }

    async fn join(&mut self, others: &[PlayerState]) -> Result<(), FrameError> {
        let login = Login {
            entity_id: self.entity_id,
            view_distance: self.view_distance,
            spawn: self.spawn_info(),
        };
        self.send(p::LOGIN_ID, &login.encode()).await?;
        self.send(p::COMMANDS_ID, &commands::tree().encode())
            .await?;

        // 탭 목록: 나 자신과 이미 있던 사람들.
        let me = self.state();
        let listed: Vec<ListedPlayer<'_>> = std::iter::once(&me)
            .chain(others)
            .map(|s| ListedPlayer {
                uuid: s.uuid,
                name: &s.name,
                gamemode: s.gamemode,
            })
            .collect();
        let body = p::encode_player_info_add(&listed);
        self.send(p::PLAYER_INFO_UPDATE_ID, &body).await?;
        self.listed
            .extend(std::iter::once(me.uuid).chain(others.iter().map(|s| s.uuid)));

        self.send_time().await?;
        self.enter_world().await?;
        self.message(&text::colored(
            "순수 Rust 서버에 오신 것을 환영합니다! /help 로 명령어를 볼 수 있습니다.",
            "aqua",
        ))
        .await?;
        Ok(())
    }

    /// 지금 차원의 스폰 지점에 세우고 주변 청크와 사람들을 보냅니다.
    /// 접속 직후와 차원 이동 후에 씁니다.
    async fn enter_world(&mut self) -> Result<(), FrameError> {
        let spawn = terrain::spawn_point(self.dimension, self.server.seed);
        // 새 차원에 들어가면 클라이언트도 청크와 엔티티를 모두 버립니다.
        self.loaded.clear();
        self.visible.clear();

        self.send(p::PLAYER_ABILITIES_ID, &p::encode_abilities(self.gamemode))
            .await?;
        let block = (
            spawn.0.floor() as i32,
            spawn.1.floor() as i32,
            spawn.2.floor() as i32,
        );
        let body = p::encode_spawn_position(self.dimension, block);
        self.send(p::SET_DEFAULT_SPAWN_POSITION_ID, &body).await?;
        self.teleport(spawn).await?;
        let body = p::encode_game_event(p::GAME_EVENT_LEVEL_CHUNKS_LOAD_START, 0.0);
        self.send(p::GAME_EVENT_ID, &body).await?;
        self.update_chunks(true).await?;

        for other in self.server.players() {
            if other.uuid != self.profile.uuid && other.dimension == self.dimension {
                self.show(&other).await?;
            }
        }
        Ok(())
    }

    /// 지금 차원 안에서 옮깁니다. 다른 사람들에게도 알립니다.
    pub(super) async fn teleport(&mut self, pos: (f64, f64, f64)) -> std::io::Result<()> {
        let id = self.next_teleport_id;
        self.next_teleport_id += 1;
        self.position = pos;
        self.send(
            p::PLAYER_POSITION_ID,
            &p::encode_player_position(id, pos, self.yaw, self.pitch),
        )
        .await?;
        self.moved(true);
        Ok(())
    }

    pub(super) async fn teleport_and_load(
        &mut self,
        pos: (f64, f64, f64),
    ) -> Result<(), FrameError> {
        self.teleport(pos).await?;
        self.update_chunks(false).await
    }

    pub(super) async fn change_dimension(
        &mut self,
        dimension: Dimension,
    ) -> Result<(), FrameError> {
        let from = self.dimension;
        self.dimension = dimension;
        self.server
            .update_player(self.profile.uuid, |s| s.dimension = dimension);
        self.send(p::RESPAWN_ID, &p::encode_respawn(&self.spawn_info()))
            .await?;
        self.enter_world().await?;
        self.server.publish(Event::ChangedDimension {
            from,
            state: self.state(),
        });
        Ok(())
    }

    pub(super) async fn set_gamemode(&mut self, mode: GameMode) -> std::io::Result<()> {
        self.gamemode = mode;
        let body = p::encode_game_event(p::GAME_EVENT_CHANGE_GAME_MODE, mode.id() as f32);
        self.send(p::GAME_EVENT_ID, &body).await?;
        self.send(p::PLAYER_ABILITIES_ID, &p::encode_abilities(mode))
            .await?;
        self.server
            .update_player(self.profile.uuid, |s| s.gamemode = mode);
        self.server.publish(Event::GameModeChanged {
            uuid: self.profile.uuid,
            gamemode: mode,
        });
        Ok(())
    }

    /// 내 위치·시선이 바뀌었음을 명단과 다른 사람들에게 알립니다.
    fn moved(&mut self, on_ground: bool) {
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
    async fn update_chunks(&mut self, force: bool) -> Result<(), FrameError> {
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

        // 생성은 CPU 일이므로 비동기 런타임 밖에서.
        // 바뀐 블록은 여기서 복사해 넘깁니다 (잠금을 다른 스레드로 가져가지 않도록).
        let (dimension, seed) = (self.dimension, self.server.seed);
        let jobs: Vec<_> = wanted
            .iter()
            .map(|&(x, z)| (x, z, self.server.world.changes_in(dimension, x, z)))
            .collect();
        let bodies = tokio::task::spawn_blocking(move || {
            jobs.iter()
                .map(|(x, z, changes)| {
                    chunk::encode(
                        &WorldStore::column(seed, dimension, *x, *z, changes),
                        *x,
                        *z,
                    )
                })
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

    async fn handle(&mut self, packet: RawPacket) -> Result<(), FrameError> {
        match packet.id {
            p::SB_MOVE_PLAYER_POS_ID => {
                self.position = p::decode_position(&packet.body)?;
                self.moved(on_ground(&packet.body, 24));
                self.update_chunks(false).await?;
            }
            p::SB_MOVE_PLAYER_POS_ROT_ID => {
                self.position = p::decode_position(&packet.body)?;
                (self.yaw, self.pitch) = p::decode_rotation_after_position(&packet.body)?;
                self.moved(on_ground(&packet.body, 32));
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
            p::SB_CHAT_ID => {
                let message = sanitize(&p::decode_leading_string(&packet.body)?);
                if !message.is_empty() {
                    self.server.publish(Event::Chat {
                        kind: ChatKind::Chat,
                        sender: self.profile.name.clone(),
                        content: message,
                    });
                }
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
                self.set_creative_slot(slot, item);
            }
            pw::SB_SET_CARRIED_ITEM_ID => {
                let slot = pw::decode_carried_item(&packet.body)?;
                if (0..9).contains(&slot) {
                    self.held = slot as usize;
                    self.hands_changed();
                }
            }
            pw::SB_PICK_ITEM_FROM_BLOCK_ID => {
                self.pick_block(pw::decode_pick_item_from_block(&packet.body)?)
                    .await?;
            }
            pw::SB_PUNCH_ID => self.server.publish(Event::Swing {
                uuid: self.profile.uuid,
                entity_id: self.entity_id,
                off_hand: false,
            }),
            // 그 밖의 패킷(클라이언트 틱, 입력, 설정 등)은 아직 쓰지 않습니다.
            _ => {}
        }
        Ok(())
    }

    /// 다른 세션이 알린 소식을 이 클라이언트에 반영합니다.
    async fn on_event(&mut self, event: Event) -> Result<(), FrameError> {
        let me = self.profile.uuid;
        match event {
            Event::Joined(state) if state.uuid != me => {
                if self.listed.insert(state.uuid) {
                    let entry = ListedPlayer {
                        uuid: state.uuid,
                        name: &state.name,
                        gamemode: state.gamemode,
                    };
                    let body = p::encode_player_info_add(&[entry]);
                    self.send(p::PLAYER_INFO_UPDATE_ID, &body).await?;
                }
                if state.dimension == self.dimension {
                    self.show(&state).await?;
                }
            }
            Event::Left { uuid, .. } if uuid != me => {
                self.hide(uuid).await?;
                if self.listed.remove(&uuid) {
                    self.send(
                        p::PLAYER_INFO_REMOVE_ID,
                        &p::encode_player_info_remove(uuid),
                    )
                    .await?;
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
            } if self.visible.contains_key(&uuid) => {
                self.send(
                    pw::SET_EQUIPMENT_ID,
                    &pw::encode_hands(entity_id, main, off),
                )
                .await?;
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

    /// 다른 플레이어를 엔티티로 띄웁니다 (이미 떠 있으면 그대로).
    async fn show(&mut self, other: &PlayerState) -> std::io::Result<()> {
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
        if other.main_hand.is_some() || other.off_hand.is_some() {
            let body = pw::encode_hands(other.entity_id, other.main_hand, other.off_hand);
            self.send(pw::SET_EQUIPMENT_ID, &body).await?;
        }
        self.visible.insert(other.uuid, other.entity_id);
        Ok(())
    }

    async fn hide(&mut self, uuid: u128) -> std::io::Result<()> {
        if let Some(entity_id) = self.visible.remove(&uuid) {
            self.send(
                p::REMOVE_ENTITIES_ID,
                &p::encode_remove_entities(&[entity_id]),
            )
            .await?;
        }
        Ok(())
    }

    async fn send_time(&mut self) -> std::io::Result<()> {
        let (game_time, day_time) = self.server.time();
        self.send(p::SET_TIME_ID, &p::encode_set_time(game_time, day_time))
            .await
    }

    async fn keep_alive(&mut self) -> Result<(), FrameError> {
        if let Some((_, sent_at)) = self.pending_keep_alive {
            if sent_at.elapsed() > KEEP_ALIVE_TIMEOUT {
                let reason = text::plain("시간 초과 (keep-alive 응답 없음)");
                self.send(p::DISCONNECT_ID, &p::encode_disconnect(&reason))
                    .await?;
                self.w.shutdown().await?;
                return Err(FrameError::Io(std::io::ErrorKind::TimedOut.into()));
            }
            return Ok(());
        }
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        self.pending_keep_alive = Some((id, Instant::now()));
        self.send(p::KEEP_ALIVE_ID, &p::encode_keep_alive(id))
            .await?;
        // 시계가 어긋나지 않도록 가끔 다시 맞춥니다.
        Ok(self.send_time().await?)
    }

    pub(super) async fn message(&mut self, content: &Tag) -> std::io::Result<()> {
        self.send(p::SYSTEM_CHAT_ID, &p::encode_system_chat(content))
            .await
    }

    /// 보낸 사람 쪽에 남는 귓속말 기록 (`You whisper to 대상: 내용`).
    pub(super) async fn whisper_echo(
        &mut self,
        target: &str,
        content: &str,
    ) -> std::io::Result<()> {
        let body = p::encode_disguised_chat(
            ChatKind::WhisperOutgoing,
            &text::plain(content),
            &text::plain(&self.profile.name),
            Some(&text::plain(target)),
        );
        self.send(p::DISGUISED_CHAT_ID, &body).await
    }
}

/// 이동 패킷의 마지막 플래그 바이트: 0x01 = 땅에 닿음.
fn on_ground(body: &[u8], offset: usize) -> bool {
    body.get(offset).is_some_and(|f| f & 0x01 != 0)
}

/// 바닐라가 막는 문자(§ 서식 코드, 제어 문자)를 걸러내고 길이를 자릅니다.
pub(super) fn sanitize(message: &str) -> String {
    message
        .chars()
        .filter(|&c| c != '§' && !c.is_control())
        .take(MAX_CHAT_LENGTH)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_formatting_and_controls() {
        assert_eq!(sanitize("  §c빨강\u{7}  "), "c빨강");
        assert_eq!(sanitize(&"가".repeat(300)).chars().count(), 256);
    }
}
