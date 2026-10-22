//! 연결들이 함께 보는 서버 상태: 접속자 명단, 월드 시간, 그리고 서로에게 알리는 이벤트.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use tokio::sync::broadcast;

use crate::auth::{MojangSessionService, Property, SessionService};
use crate::crypto::rsa::PrivateKey;
use crate::packets::play::{ChatKind, GameMode};
use crate::packets::status::ServerInfo;
use crate::packets::world::{BlockPos, Item};
use crate::protocol::nbt::Tag;
use crate::world::Dimension;
use crate::world::store::WorldStore;

/// 다른 접속자들이 알아야 하는 한 사람의 현재 모습.
#[derive(Debug, Clone)]
pub struct PlayerState {
    pub uuid: u128,
    pub name: String,
    pub entity_id: i32,
    pub dimension: Dimension,
    pub position: (f64, f64, f64),
    pub yaw: f32,
    pub pitch: f32,
    pub gamemode: GameMode,
    /// 손에 든 것 (다른 사람에게 보이는 것).
    pub main_hand: Option<Item>,
    pub off_hand: Option<Item>,
    /// 프로필 속성 (스킨). 탭 목록에 넣으면 다른 사람에게도 그 스킨으로 보입니다.
    pub properties: std::sync::Arc<Vec<Property>>,
}

/// 접속자 사이를 오가는 소식. 모든 세션이 받고, 각자 자기와 관련 있는 것만 처리합니다.
#[derive(Debug, Clone)]
pub enum Event {
    /// 접속함 (탭 목록에 추가, 같은 차원이면 엔티티로 보이기).
    Joined(PlayerState),
    /// 접속을 끊음.
    Left {
        uuid: u128,
        entity_id: i32,
    },
    /// 다른 차원으로 감. `state`는 도착한 뒤의 모습.
    ChangedDimension {
        from: Dimension,
        state: PlayerState,
    },
    Moved {
        uuid: u128,
        entity_id: i32,
        dimension: Dimension,
        position: (f64, f64, f64),
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    },
    GameModeChanged {
        uuid: u128,
        gamemode: GameMode,
    },
    /// 모두에게 가는 플레이어 채팅 (`<이름> 내용` 또는 `* 이름 내용`).
    Chat {
        kind: ChatKind,
        sender: String,
        content: String,
    },
    /// 한 사람에게만 가는 귓속말.
    Whisper {
        to: u128,
        sender: String,
        content: String,
    },
    /// 모두에게 가는 서버 알림.
    System(Tag),
    /// 누가 시간을 바꿈.
    TimeChanged,
    /// 블록이 바뀜. `broken`은 부서진 블록 (소리·조각을 보여 줄 때).
    BlockChanged {
        by: u128,
        dimension: Dimension,
        pos: BlockPos,
        state: u32,
        broken: Option<u32>,
    },
    /// 손에 든 것이 바뀜.
    HandsChanged {
        uuid: u128,
        entity_id: i32,
        main: Option<Item>,
        off: Option<Item>,
    },
    /// 팔을 휘두름.
    Swing {
        uuid: u128,
        entity_id: i32,
        off_hand: bool,
    },
}

/// `server.properties`에 해당하는 설정.
#[derive(Debug, Clone)]
pub struct Config {
    /// 정품 인증. 켜면 연결을 암호화하고 Mojang 세션 서버로 확인합니다.
    pub online_mode: bool,
    /// 이 크기(바이트) 이상인 패킷을 압축합니다. `None`이면 압축하지 않습니다.
    pub compression_threshold: Option<usize>,
    /// 오프라인 모드에서도 같은 이름의 정품 계정 스킨을 입힙니다 (Mojang API 조회).
    pub offline_skins: bool,
}

impl Default for Config {
    /// 테스트·개발용 기본값: 오프라인, 압축 켬(바닐라와 같은 256).
    fn default() -> Self {
        Self {
            online_mode: false,
            compression_threshold: Some(256),
            offline_skins: false,
        }
    }
}

/// 같은 사람이 이미 접속해 있음.
#[derive(Debug)]
pub struct AlreadyOnline;

pub struct Server {
    pub info: ServerInfo,
    pub config: Config,
    pub sessions: Box<dyn SessionService>,
    /// 정품 인증용 RSA 키. 처음 필요할 때 만듭니다 (1024비트, 바닐라와 같음).
    key: OnceLock<PrivateKey>,
    /// 월드 시드. 세 차원 모두 이 값에서 지형을 만듭니다.
    pub seed: u64,
    /// 플레이어가 바꾼 블록들.
    pub world: WorldStore,
    players: Mutex<HashMap<u128, PlayerState>>,
    next_entity_id: AtomicI32,
    events: broadcast::Sender<Event>,
    clock: Mutex<Clock>,
}

/// 하루 = 24000틱, 1초 = 20틱. 저장하지 않으므로 서버를 켤 때마다 아침(1000)에서 시작합니다.
struct Clock {
    started: Instant,
    /// `started` 시점의 하루 시각.
    day_time_at_start: i64,
}

pub const TICKS_PER_DAY: i64 = 24000;

impl Server {
    /// 메모리에만 두는 서버 (테스트용). 꺼지면 바꾼 블록이 사라집니다.
    pub fn new(info: ServerInfo, seed: u64) -> Self {
        Self::with_world(
            info,
            seed,
            WorldStore::open(seed, None).expect("memory store"),
        )
    }

    /// `dir`에 월드를 저장하는 서버. 있던 저장을 읽어 들입니다.
    pub fn open(info: ServerInfo, seed: u64, dir: std::path::PathBuf) -> std::io::Result<Self> {
        Ok(Self::with_world(
            info,
            seed,
            WorldStore::open(seed, Some(dir))?,
        ))
    }

    fn with_world(info: ServerInfo, seed: u64, world: WorldStore) -> Self {
        Self {
            info,
            config: Config::default(),
            sessions: Box::new(MojangSessionService),
            key: OnceLock::new(),
            seed,
            world,
            players: Mutex::new(HashMap::new()),
            // 26.2부터 클라이언트가 엔티티 ID 0을 거부합니다.
            next_entity_id: AtomicI32::new(1),
            events: broadcast::channel(1024).0,
            clock: Mutex::new(Clock {
                started: Instant::now(),
                day_time_at_start: 1000,
            }),
        }
    }

    pub fn with_config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    pub fn with_session_service(mut self, sessions: impl SessionService + 'static) -> Self {
        self.sessions = Box::new(sessions);
        self
    }

    pub fn key(&self) -> &PrivateKey {
        self.key.get_or_init(|| PrivateKey::generate(1024))
    }

    pub fn online(&self) -> u32 {
        self.players.lock().unwrap().len() as u32
    }

    pub fn allocate_entity_id(&self) -> i32 {
        self.next_entity_id.fetch_add(1, Ordering::Relaxed)
    }

    /// 이벤트 구독. 명단에 올리기 **전에** 구독해야, 그 사이의 소식을 놓치지 않습니다.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub fn publish(&self, event: Event) {
        // 받는 사람이 없으면 Err인데, 그건 문제가 아닙니다.
        let _ = self.events.send(event);
    }

    /// 명단에 올리고, 이미 있던 사람들을 돌려줍니다. 같은 이름(=같은 오프라인 UUID)이
    /// 이미 접속해 있으면 `Err(AlreadyOnline)`.
    pub fn add_player(&self, state: PlayerState) -> Result<Vec<PlayerState>, AlreadyOnline> {
        let mut players = self.players.lock().unwrap();
        if players.contains_key(&state.uuid) {
            return Err(AlreadyOnline);
        }
        let others = players.values().cloned().collect();
        players.insert(state.uuid, state.clone());
        drop(players);
        self.publish(Event::Joined(state));
        Ok(others)
    }

    pub fn remove_player(&self, uuid: u128) {
        if let Some(state) = self.players.lock().unwrap().remove(&uuid) {
            self.publish(Event::Left {
                uuid,
                entity_id: state.entity_id,
            });
        }
    }

    /// 명단의 내 모습을 고칩니다. 이벤트는 따로 보냅니다.
    pub fn update_player(&self, uuid: u128, f: impl FnOnce(&mut PlayerState)) {
        if let Some(state) = self.players.lock().unwrap().get_mut(&uuid) {
            f(state);
        }
    }

    pub fn players(&self) -> Vec<PlayerState> {
        let mut list: Vec<_> = self.players.lock().unwrap().values().cloned().collect();
        list.sort_by_key(|p| p.name.to_lowercase());
        list
    }

    pub fn find_by_uuid(&self, uuid: u128) -> Option<PlayerState> {
        self.players.lock().unwrap().get(&uuid).cloned()
    }

    pub fn find_player(&self, name: &str) -> Option<PlayerState> {
        self.players
            .lock()
            .unwrap()
            .values()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .cloned()
    }

    /// (월드 나이 틱, 하루 시각 틱).
    pub fn time(&self) -> (i64, i64) {
        let clock = self.clock.lock().unwrap();
        let ticks = (clock.started.elapsed().as_millis() / 50) as i64;
        (ticks, clock.day_time_at_start + ticks)
    }

    /// 하루 시각을 바꿉니다 (0~23999). 날짜는 유지합니다.
    pub fn set_day_time(&self, time_of_day: i64) {
        let (ticks, now) = self.time();
        let day = now.div_euclid(TICKS_PER_DAY);
        let mut target = day * TICKS_PER_DAY + time_of_day;
        if target < now {
            target += TICKS_PER_DAY; // 시간은 앞으로만
        }
        self.clock.lock().unwrap().day_time_at_start = target - ticks;
        self.publish(Event::TimeChanged);
    }
}
