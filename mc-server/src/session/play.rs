//! Play 상태: 월드에 들어가 돌아다니기.
//!
//! 소켓을 읽기·쓰기 반쪽으로 나누고, 읽기는 별도 태스크가 채널로 넘깁니다.
//! `read_packet`은 도중에 취소되면 읽던 바이트를 잃기 때문에 `select!`에 직접 넣을 수
//! 없어서입니다. 메인 루프는 받은 패킷, keep-alive 타이머, 전체 채팅을 함께 기다립니다.

use std::collections::HashSet;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc};

use super::{ClientSettings, Profile};
use crate::packets::play::{self as p, Login, SpawnInfo};
use crate::protocol::frame::{FrameError, RawPacket, read_packet, write_packet};
use crate::protocol::nbt::Tag;
use crate::server::Server;
use crate::text;
use crate::world::{Dimension, chunk, terrain};

const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(10);
/// 이 시간 안에 keep-alive 응답이 없으면 끊습니다 (바닐라와 같은 30초).
const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(30);
/// 클라이언트가 더 멀리 보겠다고 해도 이만큼만 보냅니다. 압축이 없어서 청크가 큽니다.
const MAX_VIEW_DISTANCE: i32 = 8;

const COMMANDS: &[&str] = &["overworld", "nether", "end", "spawn", "help"];

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

    let _online = server.join();
    let mut chat = server.subscribe_chat();
    let mut session = Session::new(writer, server, profile, settings);
    session.join().await?;
    server.broadcast(text::colored(
        &format!("{}님이 들어왔습니다", session.profile.name),
        "yellow",
    ));

    let mut keep_alive = tokio::time::interval(KEEP_ALIVE_INTERVAL);
    keep_alive.tick().await; // 첫 tick은 즉시 오므로 버립니다
    let result = loop {
        tokio::select! {
            packet = rx.recv() => match packet {
                Some(Ok(packet)) => {
                    if let Err(e) = session.handle(packet).await {
                        break Err(e);
                    }
                }
                Some(Err(e)) => break Err(e),
                None => break Ok(()), // 클라이언트가 연결을 닫음
            },
            _ = keep_alive.tick() => {
                if let Err(e) = session.keep_alive().await {
                    break Err(e);
                }
            }
            message = chat.recv() => match message {
                Ok(message) => {
                    if let Err(e) = session.send_chat(&message).await {
                        break Err(e.into());
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => {}
            },
        }
    };

    reader_task.abort();
    server.broadcast(text::colored(
        &format!("{}님이 나갔습니다", session.profile.name),
        "yellow",
    ));
    println!("{} 접속 종료", session.profile.name);
    result
}

struct Session<'a, W> {
    w: W,
    server: &'a Server,
    profile: Profile,
    entity_id: i32,
    view_distance: i32,
    dimension: Dimension,
    position: (f64, f64, f64),
    /// 클라이언트에 보낸(그리고 아직 잊으라고 하지 않은) 청크.
    loaded: HashSet<(i32, i32)>,
    center: (i32, i32),
    next_teleport_id: i32,
    /// 보냈지만 아직 답을 못 받은 keep-alive.
    pending_keep_alive: Option<(i64, Instant)>,
}

impl<'a, W: AsyncWrite + Unpin> Session<'a, W> {
    fn new(w: W, server: &'a Server, profile: Profile, settings: ClientSettings) -> Self {
        Self {
            w,
            server,
            profile,
            entity_id: server.allocate_entity_id(),
            view_distance: (settings.view_distance as i32).clamp(2, MAX_VIEW_DISTANCE),
            dimension: Dimension::Overworld,
            position: (0.0, 0.0, 0.0),
            loaded: HashSet::new(),
            center: (0, 0),
            next_teleport_id: 1,
            pending_keep_alive: None,
        }
    }

    fn spawn_info(&self) -> SpawnInfo {
        SpawnInfo {
            dimension: self.dimension,
            // 클라이언트는 이 값을 바이옴 색 섞기에만 씁니다. 실제 시드를 줄 필요는 없습니다.
            hashed_seed: self.server.seed as i64,
            gamemode: p::GAMEMODE_CREATIVE,
        }
    }

    async fn send(&mut self, id: i32, body: &[u8]) -> std::io::Result<()> {
        write_packet(&mut self.w, id, body).await
    }

    async fn join(&mut self) -> Result<(), FrameError> {
        let login = Login {
            entity_id: self.entity_id,
            view_distance: self.view_distance,
            spawn: self.spawn_info(),
        };
        self.send(p::LOGIN_ID, &login.encode()).await?;
        self.send(p::COMMANDS_ID, &p::encode_commands(COMMANDS))
            .await?;
        self.enter_world().await?;
        self.message(&text::colored(
            "순수 Rust 서버에 오신 것을 환영합니다! /overworld /nether /end 로 차원을 오갈 수 있습니다.",
            "aqua",
        ))
        .await?;
        Ok(())
    }

    /// 지금 차원의 스폰 지점에 세우고 주변 청크를 보냅니다. 접속 직후와 차원 이동 후에 씁니다.
    async fn enter_world(&mut self) -> Result<(), FrameError> {
        let spawn = terrain::spawn_point(self.dimension, self.server.seed);
        self.position = spawn;
        self.loaded.clear(); // 새 차원에 들어가면 클라이언트도 청크를 모두 버립니다

        self.send(p::PLAYER_ABILITIES_ID, &p::encode_creative_abilities())
            .await?;
        let block = (
            spawn.0.floor() as i32,
            spawn.1.floor() as i32,
            spawn.2.floor() as i32,
        );
        self.send(
            p::SET_DEFAULT_SPAWN_POSITION_ID,
            &p::encode_spawn_position(self.dimension, block),
        )
        .await?;
        self.teleport(spawn).await?;
        self.send(
            p::GAME_EVENT_ID,
            &p::encode_game_event(p::GAME_EVENT_LEVEL_CHUNKS_LOAD_START, 0.0),
        )
        .await?;
        self.update_chunks(true).await
    }

    async fn teleport(&mut self, pos: (f64, f64, f64)) -> std::io::Result<()> {
        let id = self.next_teleport_id;
        self.next_teleport_id += 1;
        self.position = pos;
        self.send(
            p::PLAYER_POSITION_ID,
            &p::encode_player_position(id, pos, 0.0, 0.0),
        )
        .await
    }

    async fn change_dimension(&mut self, dimension: Dimension) -> Result<(), FrameError> {
        self.dimension = dimension;
        self.send(p::RESPAWN_ID, &p::encode_respawn(&self.spawn_info()))
            .await?;
        self.enter_world().await
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
        self.send(
            p::SET_CHUNK_CACHE_CENTER_ID,
            &p::encode_chunk_xz_varint(center.0, center.1),
        )
        .await?;

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
        let (dimension, seed) = (self.dimension, self.server.seed);
        let coords = wanted.clone();
        let bodies = tokio::task::spawn_blocking(move || {
            coords
                .iter()
                .map(|&(x, z)| chunk::encode(&terrain::generate(dimension, seed, x, z), x, z))
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|e| FrameError::Io(std::io::Error::other(e)))?;

        self.send(p::CHUNK_BATCH_START_ID, &[]).await?;
        for body in &bodies {
            self.send(p::LEVEL_CHUNK_WITH_LIGHT_ID, body).await?;
        }
        self.send(
            p::CHUNK_BATCH_FINISHED_ID,
            &p::encode_batch_finished(bodies.len() as i32),
        )
        .await?;
        self.loaded.extend(wanted);
        Ok(())
    }

    async fn handle(&mut self, packet: RawPacket) -> Result<(), FrameError> {
        match packet.id {
            p::SB_MOVE_PLAYER_POS_ID => {
                self.position = p::decode_position(&packet.body)?;
                self.update_chunks(false).await?;
            }
            p::SB_MOVE_PLAYER_POS_ROT_ID => {
                self.position = p::decode_position(&packet.body)?;
                self.update_chunks(false).await?;
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
                let message = p::decode_leading_string(&packet.body)?;
                self.server.broadcast(Tag::Compound(vec![
                    (
                        "text".into(),
                        Tag::String(format!("<{}> ", self.profile.name)),
                    ),
                    ("extra".into(), Tag::List(vec![text::plain(&message)])),
                ]));
            }
            // 그 밖의 패킷(시선만 바뀐 이동, 클라이언트 틱, 입력, 설정 등)은 아직 쓰지 않습니다.
            _ => {}
        }
        Ok(())
    }

    async fn command(&mut self, command: &str) -> Result<(), FrameError> {
        if let Some(dimension) = Dimension::from_command(command) {
            if dimension == self.dimension {
                let msg = format!("이미 {}에 있습니다.", dimension.display_name());
                return Ok(self.message(&text::colored(&msg, "gray")).await?);
            }
            self.change_dimension(dimension).await?;
            let msg = format!("{}(으)로 이동했습니다.", dimension.display_name());
            return Ok(self.message(&text::colored(&msg, "green")).await?);
        }
        match command {
            "spawn" => {
                let spawn = terrain::spawn_point(self.dimension, self.server.seed);
                self.teleport(spawn).await?;
                self.update_chunks(false).await
            }
            "help" => Ok(self
                .message(&text::plain(
                    "/overworld, /nether, /end — 차원 이동 · /spawn — 스폰으로",
                ))
                .await?),
            other => Ok(self
                .message(&text::colored(&format!("모르는 명령어: /{other}"), "red"))
                .await?),
        }
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
        Ok(self
            .send(p::KEEP_ALIVE_ID, &p::encode_keep_alive(id))
            .await?)
    }

    async fn message(&mut self, content: &Tag) -> std::io::Result<()> {
        self.send(p::SYSTEM_CHAT_ID, &p::encode_system_chat(content))
            .await
    }

    async fn send_chat(&mut self, content: &Tag) -> std::io::Result<()> {
        self.message(content).await
    }
}
