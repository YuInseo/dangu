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
use crate::packets::entity as pe;
use crate::packets::play::{self as p, ChatKind, GameMode, SpawnInfo};
use crate::packets::world::Item;
use crate::protocol::conn::{Conn, PacketWriter};
use crate::protocol::frame::{FrameError, RawPacket};
use crate::protocol::nbt::Tag;
use crate::server::{AlreadyOnline, Event, PlayerState, Server};
use crate::text;
use crate::world::{Dimension, terrain};

mod events;
mod packets;
mod view;

const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(10);
/// 체력·배고픔·환경 피해를 보는 간격.
const SURVIVAL_TICK: Duration = Duration::from_millis(250);
/// HUD(재화·스탯·체력바)를 다시 그리는 간격.
const HUD_TICK: Duration = Duration::from_secs(1);
/// 이 시간 안에 keep-alive 응답이 없으면 끊습니다 (바닐라와 같은 30초).
const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(30);
/// 클라이언트가 더 멀리 보겠다고 해도 이만큼만 보냅니다.
pub(crate) const MAX_VIEW_DISTANCE: i32 = 10;
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
        let mut hud = tokio::time::interval(HUD_TICK);
        while session.transfer.is_none() {
            tokio::select! {
                _ = survival.tick() => session.survival_tick().await?,
                _ = hud.tick() => session.hud_tick().await?,
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

pub(crate) struct Session<'a, W> {
    w: PacketWriter<W>,
    pub(crate) server: &'a Server,
    pub(crate) profile: Profile,
    pub(crate) entity_id: i32,
    pub(crate) view_distance: i32,
    pub(crate) dimension: Dimension,
    pub(crate) gamemode: GameMode,
    pub(crate) position: (f64, f64, f64),
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    /// 인벤토리 메뉴 번호 그대로: 9..36 가방, 36..45 핫바, 45 보조 손.
    pub(crate) inventory: [Option<Item>; 46],
    /// 고른 핫바 칸 (0..9).
    pub(crate) held: usize,
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
    pub(crate) vitals: Vitals,
    /// 열린 창 (없으면 플레이어 인벤토리).
    pub(crate) window: Option<Window>,
    pub(crate) next_window: i32,
    pub(crate) state_id: i32,
    /// 창에서 마우스로 들고 있는 것.
    pub(crate) carried: Option<Item>,
    /// 저장해 둔 위치로 처음 들어갈 때.
    resume_at: Option<(f64, f64, f64)>,
    pub(crate) chat: ChatState,
    /// 사용자 정의 아이템 능력을 다시 쓸 수 있는 때.
    pub(crate) ability_ready: HashMap<String, Instant>,
    /// 접속한 IP.
    pub(crate) ip: String,
    pub(crate) origin: (String, u16),
    pub(crate) pack_loaded: bool,
    /// `/npc`로 고른 NPC.
    pub(crate) selected_npc: Option<u32>,
    pub(crate) skin_parts: u8,
    pub(crate) main_arm: i32,
    /// 크리에이티브·관전에서 날고 있는가.
    pub(crate) flying: bool,
    /// 관전 중 빙의한 엔티티.
    pub(crate) camera: Option<i32>,
    /// F3+F4가 보여 주는 이전 게임 모드.
    pub(crate) previous_gamemode: Option<GameMode>,
    /// 옮겨 갈 인스턴스 (`Start Configuration`을 보낸 뒤).
    pub(crate) transfer: Option<Arc<Server>>,
    /// 차원문 안에 서 있은 틱.
    pub(crate) portal_ticks: u32,
    /// 화면 HUD (재화·스탯·체력바).
    pub(crate) hud: crate::plugins::hud::HudState,
    /// RPG 마나 (능력 아이템이 씀).
    pub(crate) mana: crate::plugins::rpg::Mana,
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
            origin: settings.origin.clone(),
            pack_loaded: settings.pack_loaded,
            selected_npc: None,
            skin_parts: settings.skin_parts,
            main_arm: settings.main_hand.clamp(0, 1),
            flying: false,
            camera: None,
            previous_gamemode: None,
            transfer: None,
            portal_ticks: 0,
            hud: Default::default(),
            mana: Default::default(),
            loaded: HashSet::new(),
            center: (0, 0),
            next_teleport_id: 1,
            pending_keep_alive: None,
            listed: HashSet::new(),
            visible: HashMap::new(),
        }
    }

    /// 다음 인스턴스로 넘길 클라이언트 설정.
    pub(crate) fn settings(&self) -> ClientSettings {
        ClientSettings {
            view_distance: self.view_distance as u8,
            skin_parts: self.skin_parts,
            main_hand: self.main_arm,
            ip: self.ip.clone(),
            origin: self.origin.clone(),
            pack_loaded: self.pack_loaded,
        }
    }

    /// 다른 인스턴스로 보냅니다: 클라이언트를 Configuration으로 되돌립니다.
    pub(crate) async fn start_transfer(&mut self, to: Arc<Server>) -> Result<(), FrameError> {
        self.close_window(false).await?;
        self.send(p::START_CONFIGURATION_ID, &[]).await?;
        self.transfer = Some(to);
        Ok(())
    }

    pub(crate) fn state(&self) -> PlayerState {
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
    pub(crate) fn save(&self) {
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

    pub(crate) fn main_hand(&self) -> Option<Item> {
        self.inventory[36 + self.held]
    }

    pub(crate) fn off_hand(&self) -> Option<Item> {
        self.inventory[45]
    }

    pub(crate) fn spawn_info(&self) -> SpawnInfo {
        SpawnInfo {
            dimension: self.dimension,
            // 클라이언트는 이 값을 바이옴 색 섞기에만 씁니다. 실제 시드를 줄 필요는 없습니다.
            hashed_seed: self.server.seed as i64,
            gamemode: self.gamemode,
            previous: self.previous_gamemode,
        }
    }

    pub(crate) async fn send(&mut self, id: i32, body: &[u8]) -> std::io::Result<()> {
        self.w.write_packet(id, body).await
    }

    /// 권한에 맞는 명령어 목록과 OP 단계를 보냅니다 (접속 때, 권한이 바뀔 때).
    pub(crate) async fn send_commands(&mut self) -> std::io::Result<()> {
        let tree = commands::tree(self.server, self.profile.uuid);
        self.send(p::COMMANDS_ID, &tree.encode()).await?;
        // Entity Event 24~28 = OP 0~4단계: F3+F4 게임 모드 전환 등 클라이언트 기능이 켜집니다.
        let level = self.server.perms.op_level(self.profile.uuid);
        let body = pe::encode_entity_event(self.entity_id, 24 + level as i8);
        self.send(pe::ENTITY_EVENT_ID, &body).await
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

    pub(crate) async fn message(&mut self, content: &Tag) -> std::io::Result<()> {
        self.send(p::SYSTEM_CHAT_ID, &p::encode_system_chat(content))
            .await
    }

    /// 보낸 사람 쪽에 남는 귓속말 기록 (`You whisper to 대상: 내용`).
    pub(crate) async fn whisper_echo(
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

/// 바닐라가 막는 문자(§ 서식 코드, 제어 문자)를 걸러내고 길이를 자릅니다.
pub(crate) fn sanitize(message: &str) -> String {
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
