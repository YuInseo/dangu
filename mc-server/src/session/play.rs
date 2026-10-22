//! Play 상태: 월드에 들어가 돌아다니고, 다른 사람과 보고 말하기.
//!
//! 소켓을 읽기·쓰기 반쪽으로 나누고, 읽기는 별도 태스크가 채널로 넘깁니다.
//! `read_packet`은 도중에 취소되면 읽던 바이트를 잃기 때문에 `select!`에 직접 넣을 수
//! 없어서입니다. 메인 루프는 받은 패킷, keep-alive 타이머, 서버 이벤트를 함께 기다립니다.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc};

use super::commands;
use super::signed_chat::ChatState;
use super::survival::Vitals;
use super::windows::Window;
use super::{ClientSettings, Profile};
use crate::game::playerdata::{self, PlayerData};
use crate::packets::chat as pch;
use crate::packets::container as pc;
use crate::packets::entity as pe;
use crate::packets::hud;
use crate::packets::play::{self as p, ChatKind, GameMode, ListedPlayer, Login, SpawnInfo};
use crate::packets::world::{self as pw, Item};
use crate::protocol::conn::{Conn, PacketWriter};
use crate::protocol::frame::{FrameError, RawPacket};
use crate::protocol::nbt::Tag;
use crate::server::{AlreadyOnline, Event, PlayerState, Server};
use crate::text;
use crate::world::{Dimension, chunk, terrain};

const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(10);
/// 체력·배고픔·환경 피해를 보는 간격.
const SURVIVAL_TICK: Duration = Duration::from_millis(250);
/// 이 시간 안에 keep-alive 응답이 없으면 끊습니다 (바닐라와 같은 30초).
const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(30);
/// 클라이언트가 더 멀리 보겠다고 해도 이만큼만 보냅니다.
pub(super) const MAX_VIEW_DISTANCE: i32 = 10;
/// 바닐라와 같은 채팅 길이 제한.
const MAX_CHAT_LENGTH: usize = 256;

/// Play가 끝난 까닭. 한 번만 돌려주는 값이라 크기 차이는 상관없습니다.
#[allow(clippy::large_enum_variant)]
pub enum Outcome<S> {
    /// 연결이 끝남.
    Quit,
    /// 다른 인스턴스로 옮김: 클라이언트는 이미 Configuration 단계에 있습니다.
    Transfer {
        conn: Conn<S>,
        to: Arc<Server>,
        settings: ClientSettings,
    },
}

/// 재구성 요청 뒤 클라이언트의 확인을 기다리는 시간.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn run<S>(
    conn: Conn<S>,
    server: &Server,
    profile: Profile,
    settings: ClientSettings,
) -> Result<Outcome<S>, FrameError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut reader, writer) = conn.into_split();
    let (tx, mut rx) = mpsc::channel::<Result<RawPacket, FrameError>>(256);
    // `Configuration Acknowledged`를 넘기고 나면 읽기를 멈추고 읽기 반쪽을 돌려줍니다.
    // 그 뒤의 바이트는 Configuration 패킷이므로 Play 쪽에서 읽으면 안 됩니다.
    let reader_task = tokio::spawn(async move {
        loop {
            match reader.read_packet().await {
                Ok(Some(packet)) => {
                    let ack = packet.id == p::SB_CONFIGURATION_ACKNOWLEDGED_ID;
                    if tx.send(Ok(packet)).await.is_err() {
                        return None;
                    }
                    if ack {
                        return Some(reader);
                    }
                }
                Ok(None) => return None,
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    return None;
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
            return Ok(Outcome::Quit);
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
        let mut survival = tokio::time::interval(SURVIVAL_TICK);
        while session.transfer.is_none() {
            tokio::select! {
                _ = survival.tick() => session.survival_tick().await?,
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
        // 옮기는 중: Start Configuration을 보냈으니 Play 패킷은 더 보내지 않고 확인만 기다립니다.
        tokio::time::timeout(TRANSFER_TIMEOUT, async {
            loop {
                match rx.recv().await {
                    Some(Ok(p)) if p.id == p::SB_CONFIGURATION_ACKNOWLEDGED_ID => return Ok(()),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e),
                    None => return Err(FrameError::Io(std::io::ErrorKind::UnexpectedEof.into())),
                }
            }
        })
        .await
        .map_err(|_| FrameError::Io(std::io::ErrorKind::TimedOut.into()))?
    }
    .await;

    let transfer = session.transfer.take();
    session.save();
    server.remove_player(session.profile.uuid);
    let left = match &transfer {
        Some(to) if result.is_ok() => {
            format!("{}님이 {}(으)로 갔습니다", session.profile.name, to.name)
        }
        _ => format!("{}님이 나갔습니다", session.profile.name),
    };
    server.publish(Event::System(text::colored(&left, "yellow")));

    match (transfer, result) {
        (Some(to), Ok(())) => {
            let reader = reader_task
                .await
                .ok()
                .flatten()
                .ok_or(FrameError::Io(std::io::ErrorKind::UnexpectedEof.into()))?;
            println!("{}: {} → {}", session.profile.name, server.name, to.name);
            let settings = session.settings();
            let conn = Conn {
                r: reader,
                w: session.w,
            };
            Ok(Outcome::Transfer { conn, to, settings })
        }
        (_, result) => {
            reader_task.abort();
            println!("{} 접속 종료", session.profile.name);
            result.map(|()| Outcome::Quit)
        }
    }
}

pub(super) struct Session<'a, W> {
    w: PacketWriter<W>,
    pub(super) server: &'a Server,
    pub(super) profile: Profile,
    pub(super) entity_id: i32,
    pub(super) view_distance: i32,
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
    /// 띄운 아이템·몹.
    visible_entities: HashSet<i32>,
    pub(super) vitals: Vitals,
    /// 열린 창 (없으면 플레이어 인벤토리).
    pub(super) window: Option<Window>,
    pub(super) next_window: i32,
    pub(super) state_id: i32,
    /// 창에서 마우스로 들고 있는 것.
    pub(super) carried: Option<Item>,
    /// 저장해 둔 위치로 처음 들어갈 때.
    resume_at: Option<(f64, f64, f64)>,
    pub(super) chat: ChatState,
    /// 사용자 정의 아이템 능력을 다시 쓸 수 있는 때.
    pub(super) ability_ready: HashMap<String, Instant>,
    /// 접속한 IP.
    pub(super) ip: String,
    pub(super) skin_parts: u8,
    pub(super) main_arm: i32,
    /// 크리에이티브·관전에서 날고 있는가.
    pub(super) flying: bool,
    /// 관전 중 빙의한 엔티티.
    pub(super) camera: Option<i32>,
    /// F3+F4가 보여 주는 이전 게임 모드.
    pub(super) previous_gamemode: Option<GameMode>,
    /// 옮겨 갈 인스턴스 (`Start Configuration`을 보낸 뒤).
    pub(super) transfer: Option<Arc<Server>>,
}

impl<'a, W: AsyncWrite + Unpin> Session<'a, W> {
    fn new(
        w: PacketWriter<W>,
        server: &'a Server,
        profile: Profile,
        settings: ClientSettings,
    ) -> Self {
        let saved = server
            .data_dir
            .as_ref()
            .and_then(|d| playerdata::load(d, profile.uuid));
        let dimension = saved.as_ref().map_or(Dimension::Overworld, |d| d.dimension);
        Self {
            w,
            server,
            profile,
            entity_id: server.allocate_entity_id(),
            view_distance: (settings.view_distance as i32).clamp(2, MAX_VIEW_DISTANCE),
            dimension,
            gamemode: saved
                .as_ref()
                .map_or(server.config.default_gamemode, |d| d.gamemode),
            position: saved.as_ref().map_or_else(
                || terrain::spawn_point(dimension, server.seed),
                |d| d.position,
            ),
            yaw: saved.as_ref().map_or(0.0, |d| d.yaw),
            pitch: saved.as_ref().map_or(0.0, |d| d.pitch),
            inventory: saved.as_ref().map_or([None; 46], |d| d.inventory),
            held: saved.as_ref().map_or(0, |d| d.held),
            visible_entities: HashSet::new(),
            vitals: saved.as_ref().map_or_else(Vitals::default, |d| {
                Vitals::with(d.health, d.food, d.saturation)
            }),
            window: None,
            next_window: 0,
            state_id: 0,
            carried: None,
            resume_at: saved.map(|d| d.position),
            chat: ChatState::default(),
            ability_ready: HashMap::new(),
            ip: settings.ip.clone(),
            skin_parts: settings.skin_parts,
            main_arm: settings.main_hand.clamp(0, 1),
            flying: false,
            camera: None,
            previous_gamemode: None,
            transfer: None,
            loaded: HashSet::new(),
            center: (0, 0),
            next_teleport_id: 1,
            pending_keep_alive: None,
            listed: HashSet::new(),
            visible: HashMap::new(),
        }
    }

    /// 다음 인스턴스로 넘길 클라이언트 설정.
    pub(super) fn settings(&self) -> ClientSettings {
        ClientSettings {
            view_distance: self.view_distance as u8,
            skin_parts: self.skin_parts,
            main_hand: self.main_arm,
            ip: self.ip.clone(),
        }
    }

    /// 다른 인스턴스로 보냅니다: 클라이언트를 Configuration으로 되돌립니다.
    pub(super) async fn start_transfer(&mut self, to: Arc<Server>) -> Result<(), FrameError> {
        self.close_window(false).await?;
        self.send(p::START_CONFIGURATION_ID, &[]).await?;
        self.transfer = Some(to);
        Ok(())
    }

    pub(super) fn state(&self) -> PlayerState {
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
            properties: self.profile.properties.clone(),
            dead: self.vitals.dead,
            chat_session: self.chat_session(),
            ip: self.ip.clone(),
            skin_parts: self.skin_parts,
            main_arm: self.main_arm,
            sneaking: self.vitals.sneaking,
        }
    }

    /// 다시 접속했을 때 이어지도록 저장합니다.
    pub(super) fn save(&self) {
        let Some(dir) = &self.server.data_dir else {
            return;
        };
        let mut inventory = self.inventory;
        inventory[0] = None; // 조합 결과는 저장하지 않음
        let data = PlayerData {
            dimension: self.dimension,
            position: self.position,
            yaw: self.yaw,
            pitch: self.pitch,
            gamemode: self.gamemode,
            health: if self.vitals.dead {
                super::survival::MAX_HEALTH
            } else {
                self.vitals.health
            },
            food: if self.vitals.dead {
                20
            } else {
                self.vitals.food
            },
            saturation: self.vitals.saturation,
            held: self.held,
            inventory,
        };
        if let Err(e) = playerdata::save(dir, self.profile.uuid, &data) {
            eprintln!("{}의 데이터를 저장하지 못했습니다: {e}", self.profile.name);
        }
    }

    pub(super) fn main_hand(&self) -> Option<Item> {
        self.inventory[36 + self.held]
    }

    pub(super) fn off_hand(&self) -> Option<Item> {
        self.inventory[45]
    }

    pub(super) fn spawn_info(&self) -> SpawnInfo {
        SpawnInfo {
            dimension: self.dimension,
            // 클라이언트는 이 값을 바이옴 색 섞기에만 씁니다. 실제 시드를 줄 필요는 없습니다.
            hashed_seed: self.server.seed as i64,
            gamemode: self.gamemode,
            previous: self.previous_gamemode,
        }
    }

    pub(super) async fn send(&mut self, id: i32, body: &[u8]) -> std::io::Result<()> {
        self.w.write_packet(id, body).await
    }

    async fn join(&mut self, others: &[PlayerState]) -> Result<(), FrameError> {
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
    pub(super) async fn enter_world(
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
    pub(super) fn sees_entity(&self, id: i32) -> bool {
        self.visible_entities.contains(&id) || self.visible.values().any(|&e| e == id)
    }

    async fn show_entity(&mut self, e: &crate::game::entities::Snapshot) -> std::io::Result<()> {
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
        Ok(())
    }

    /// 지금 차원 안에서 옮깁니다. 다른 사람들에게도 알립니다.
    pub(super) async fn teleport(&mut self, pos: (f64, f64, f64)) -> std::io::Result<()> {
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
        self.enter_world(None).await?;
        self.send_health().await?;
        self.server.publish(Event::ChangedDimension {
            from,
            state: self.state(),
        });
        Ok(())
    }

    pub(super) async fn set_gamemode(&mut self, mode: GameMode) -> Result<(), FrameError> {
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
    pub(super) async fn update_chunks(&mut self, force: bool) -> Result<(), FrameError> {
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

    async fn handle(&mut self, packet: RawPacket) -> Result<(), FrameError> {
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
            id => super::ignored::note(id),
        }
        Ok(())
    }

    /// 다른 세션이 알린 소식을 이 클라이언트에 반영합니다.
    async fn on_event(&mut self, event: Event) -> Result<(), FrameError> {
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
            } if self.visible.contains_key(&uuid) => {
                self.send(
                    pw::SET_EQUIPMENT_ID,
                    &pw::encode_hands(entity_id, main, off),
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

    /// 권한에 맞는 명령어 목록과 OP 단계를 보냅니다 (접속 때, 권한이 바뀔 때).
    pub(super) async fn send_commands(&mut self) -> std::io::Result<()> {
        let tree = commands::tree(self.server, self.profile.uuid);
        self.send(p::COMMANDS_ID, &tree.encode()).await?;
        // Entity Event 24~28 = OP 0~4단계: F3+F4 게임 모드 전환 등 클라이언트 기능이 켜집니다.
        let level = self.server.perms.op_level(self.profile.uuid);
        let body = pe::encode_entity_event(self.entity_id, 24 + level as i8);
        self.send(pe::ENTITY_EVENT_ID, &body).await
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
        let looks = pe::encode_player_data(
            other.entity_id,
            other.sneaking,
            false,
            other.skin_parts,
            other.main_arm,
        );
        self.send(pe::SET_ENTITY_DATA_ID, &looks).await?;
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
        self.save();
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
