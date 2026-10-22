//! 연결들이 함께 보는 서버 상태: 접속자 명단, 월드 시간, 그리고 서로에게 알리는 이벤트.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
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
    /// 죽어서 부활을 기다리는 중.
    pub dead: bool,
    /// 접속한 IP (밴용).
    pub ip: String,
    /// Mojang 서명을 확인한 채팅 세션 (서명된 채팅용).
    pub chat_session: Option<std::sync::Arc<crate::chat::ChatSession>>,
    /// 겉옷 레이어 비트와 주 손 (0 왼손, 1 오른손).
    pub skin_parts: u8,
    pub main_arm: i32,
    pub sneaking: bool,
}

impl PlayerState {
    /// 시험용: 그 자리에 서 있는 서바이벌 플레이어.
    pub fn test(dimension: Dimension, position: (f64, f64, f64)) -> Self {
        Self {
            uuid: 1,
            name: "test".into(),
            entity_id: 1_000_000,
            dimension,
            position,
            yaw: 0.0,
            pitch: 0.0,
            gamemode: GameMode::Survival,
            main_hand: None,
            off_hand: None,
            properties: Default::default(),
            dead: false,
            ip: String::new(),
            chat_session: None,
            skin_parts: 0,
            main_arm: 1,
            sneaking: false,
        }
    }
}

/// 접속자 사이를 오가는 소식. 모든 세션이 받고, 각자 자기와 관련 있는 것만 처리합니다.
#[derive(Debug, Clone)]
pub enum Event {
    /// 접속함 (탭 목록에 추가, 같은 차원이면 엔티티로 보이기).
    Joined(PlayerState),
    /// 플레이어 엔티티 메타데이터가 바뀜 (웅크리기, 겉옷 레이어, 주 손). 본문은 그대로 전달.
    PlayerData {
        uuid: u128,
        body: std::sync::Arc<Vec<u8>>,
    },
    /// 접속을 끊음.
    Left {
        uuid: u128,
        entity_id: i32,
    },
    /// 죽었다가 부활함 (쓰러진 모습을 지우고 새로 띄움).
    Respawned {
        from: Dimension,
        state: PlayerState,
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
    /// 몇 사람에게만 가는 알림 (길드 등).
    Notice {
        to: std::sync::Arc<Vec<u128>>,
        message: String,
    },
    /// 길드 창고 한 페이지가 바뀜 (보고 있던 사람은 다시 받음).
    GuildStorageChanged {
        guild: u32,
        page: usize,
    },
    /// 채널 채팅 ([`crate::plugins::chat`]). 받을지는 받는 세션이 정합니다.
    ChannelChat(std::sync::Arc<crate::plugins::chat::Message>),
    /// 서명을 확인한 채팅. 받는 세션마다 서명째 전합니다.
    SignedChat {
        sender: u128,
        sender_name: String,
        index: i32,
        signature: Box<crate::chat::Signature>,
        content: String,
        timestamp: i64,
        salt: i64,
        last_seen: Vec<crate::chat::Signature>,
    },
    /// 누가 채팅 세션을 열었음 (탭 목록에 키를 붙임).
    ChatSessionOpened {
        uuid: u128,
        session: std::sync::Arc<crate::chat::ChatSession>,
    },
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
    /// 엔티티(아이템, 몹)가 생김.
    EntitySpawned(crate::game::entities::Snapshot),
    EntityMoved {
        dimension: Dimension,
        id: i32,
        pos: (f64, f64, f64),
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    },
    EntitiesRemoved {
        dimension: Dimension,
        ids: Vec<i32>,
    },
    /// 이 사람을 다른 인스턴스 프로세스로 보냄 (Transfer 패킷 → 마스터로 다시 접속).
    TransferOut {
        uuid: u128,
        to: String,
    },
    /// 서버 리소스 팩을 새로 만듦: 접속한 사람에게 다시 보냄.
    ResourcePackChanged,
    /// 몹 모습이 바뀜 (불붙음, 크리퍼 부풂, 털 깎임, 주민 직업·레벨).
    EntityData {
        dimension: Dimension,
        id: i32,
        meta: Vec<crate::packets::entity::Meta>,
    },
    /// 떨어진 아이템 묶음의 개수가 바뀜 (일부만 주웠을 때).
    ItemStackChanged {
        dimension: Dimension,
        id: i32,
        item: Item,
    },
    /// 누가 아이템을 주움 (빨려 들어가는 모습).
    ItemCollected {
        dimension: Dimension,
        item_entity: i32,
        collector: i32,
        count: i32,
    },
    /// 이 사람에게 아이템을 주울지 묻습니다. 답은 `Entities::finish_pickup`으로.
    PickupOffer {
        to: u128,
        entity_id: i32,
        item: Item,
    },
    /// 플레이어에게 피해. 맞은 사람의 세션이 처리합니다.
    Damage {
        target: u128,
        amount: f32,
        damage_type: &'static str,
        attacker: Option<(i32, (f64, f64, f64))>,
        attacker_name: String,
    },
    /// 맞은 모습 (빨갛게 깜빡임). 플레이어든 몹이든.
    Hurt {
        dimension: Dimension,
        entity_id: i32,
        damage_type: i32,
        attacker: Option<i32>,
    },
    /// 엔티티 이벤트 (쓰러짐 등).
    EntityAnimation {
        dimension: Dimension,
        entity_id: i32,
        event: i8,
    },
    /// 소리.
    Sound {
        dimension: Dimension,
        pos: (f64, f64, f64),
        sound: i32,
        source: i32,
        volume: f32,
        pitch: f32,
    },
    /// 입자.
    Particles {
        dimension: Dimension,
        pos: (f64, f64, f64),
        particle: i32,
        spread: f32,
        count: i32,
    },
    /// 번개 (잠깐 보이는 엔티티).
    Lightning {
        dimension: Dimension,
        pos: (f64, f64, f64),
        entity_id: i32,
    },
    /// 난이도가 바뀜.
    DifficultyChanged(Difficulty),
    /// 이 사람을 내보냄.
    Kick {
        uuid: u128,
        reason: String,
    },
    /// 이 사람의 권한·OP 단계가 바뀜 (명령어 목록을 다시 보냄).
    PermissionsChanged {
        uuid: Option<u128>,
    },
    /// 접속자 모두 지금 저장.
    SavePlayers,
    /// 화면 가운데 큰 글씨.
    Title {
        uuid: u128,
        title: String,
    },
    /// 다른 사람(또는 콘솔)이 이 사람의 게임 모드를 바꿈.
    SetGameMode {
        uuid: u128,
        gamemode: GameMode,
    },
    /// 상자 한 칸이 바뀜.
    ContainerChanged {
        dimension: Dimension,
        pos: BlockPos,
        slot: usize,
        item: Option<Item>,
    },
    /// 화로의 불·굽기 진행 (창의 화살표·불꽃).
    ContainerData {
        dimension: Dimension,
        pos: BlockPos,
        values: [i16; 4],
    },
    /// 폭발 (화면 흔들림·소리·입자, 가까운 플레이어 밀어냄).
    Explosion {
        dimension: Dimension,
        center: (f64, f64, f64),
        power: f32,
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
    /// 몹이 저절로 생기는가.
    pub spawn_mobs: bool,
    /// 처음 들어온 사람의 게임 모드.
    pub default_gamemode: GameMode,
    /// 모두를 OP 4로 (혼자 쓰는 개발 서버·테스트용). 공개 서버에서는 끄세요.
    pub everyone_op: bool,
    /// 처음 들어오면 OP 4로 올릴 이름들 (UUID를 모를 때를 위해 이름으로).
    pub initial_ops: Vec<String>,
    pub difficulty: Difficulty,
    /// 하드코어: 하트 모양이 바뀌고, 죽으면 관전 모드.
    pub hardcore: bool,
    /// 새 월드를 만들 때 스폰 둘레를 미리 만들 반지름 (청크). 0이면 안 함.
    pub pregen_radius: i32,
    /// 스폰에 복권 NPC를 세움.
    pub lottery_npc: bool,
    /// 채팅 채널(전체·채널·주변·거래·길드·관리자). 끄면 바닐라 서명 채팅.
    pub chat_channels: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Peaceful = 0,
    Easy = 1,
    Normal = 2,
    Hard = 3,
}

impl Difficulty {
    pub const ALL: [Difficulty; 4] = [
        Difficulty::Peaceful,
        Difficulty::Easy,
        Difficulty::Normal,
        Difficulty::Hard,
    ];

    pub fn name(self) -> &'static str {
        ["peaceful", "easy", "normal", "hard"][self as usize]
    }

    pub fn from_name(n: &str) -> Option<Difficulty> {
        Self::ALL.into_iter().find(|d| d.name() == n)
    }

    pub fn from_id(i: u8) -> Difficulty {
        Self::ALL[(i as usize).min(3)]
    }

    pub fn display_name(self) -> &'static str {
        ["평화로움", "쉬움", "보통", "어려움"][self as usize]
    }

    /// 몹 공격력 보정 (바닐라: 쉬움 절반+1, 어려움 1.5배).
    pub fn scale_mob_damage(self, amount: f32) -> f32 {
        match self {
            Difficulty::Peaceful => 0.0,
            Difficulty::Easy => (amount / 2.0 + 1.0).min(amount),
            Difficulty::Normal => amount,
            Difficulty::Hard => amount * 1.5,
        }
    }

    /// 굶주림이 체력을 어디까지 깎는가 (쉬움 10, 보통 1, 어려움 0).
    pub fn starvation_floor(self) -> f32 {
        match self {
            Difficulty::Peaceful | Difficulty::Easy => 10.0,
            Difficulty::Normal => 1.0,
            Difficulty::Hard => 0.0,
        }
    }
}

impl Default for Config {
    /// 테스트·개발용 기본값: 오프라인, 압축 켬(바닐라와 같은 256).
    fn default() -> Self {
        Self {
            online_mode: false,
            compression_threshold: Some(256),
            offline_skins: false,
            spawn_mobs: true,
            default_gamemode: GameMode::Survival,
            everyone_op: true,
            initial_ops: Vec::new(),
            difficulty: Difficulty::Normal,
            hardcore: false,
            pregen_radius: 0,
            lottery_npc: false,
            chat_channels: false,
        }
    }
}

/// 같은 사람이 이미 접속해 있음.
#[derive(Debug)]
pub struct AlreadyOnline;

pub struct Server {
    /// 인스턴스 이름 (`/server <이름>`으로 오갑니다).
    pub name: String,
    pub info: ServerInfo,
    pub config: Config,
    // ---- 인스턴스끼리 나누는 것 (Arc) ----
    pub sessions: Arc<dyn SessionService>,
    /// 정품 인증용 RSA 키. 처음 필요할 때 만듭니다 (1024비트, 바닐라와 같음).
    key: Arc<OnceLock<PrivateKey>>,
    /// 플레이어 채팅 키에 서명하는 Mojang 공개 키들 (`/publickeys`). 비어 있으면 서명된
    /// 채팅을 확인할 수 없어 모두 서명 없는 채팅으로 전합니다.
    pub chat_keys: Arc<Mutex<Vec<crate::crypto::rsa::PublicKey>>>,
    /// OP·권한·밴은 네트워크 전체에 하나.
    pub perms: Arc<crate::perms::Permissions>,
    /// `/stop`이 울리면 main이 저장하고 끕니다.
    pub shutdown: Arc<tokio::sync::Notify>,
    /// 이 인스턴스가 속한 네트워크 (없으면 혼자).
    network: OnceLock<Weak<crate::network::Network>>,

    // ---- 인스턴스마다 따로 ----
    /// 월드 시드. 세 차원 모두 이 값에서 지형을 만듭니다.
    pub seed: u64,
    /// 플레이어가 바꾼 블록들.
    pub world: Arc<WorldStore>,
    /// 떨어진 아이템과 몹.
    pub entities: crate::game::entities::Entities,
    /// 예약 틱 (물·레드스톤·모래 …).
    pub ticks: crate::game::mechanics::Ticks,
    pub furnaces: crate::game::mechanics::furnace::Furnaces,
    pub redstone: crate::game::mechanics::redstone::State,
    /// 이 채널의 복권.
    pub lottery: crate::plugins::lottery::Lottery,
    /// 이 채널의 NPC.
    pub npcs: crate::plugins::npc::Npcs,
    difficulty: std::sync::atomic::AtomicU8,
    /// 난이도 잠금 (설정 화면의 자물쇠). 잠그면 다시 풀 수 없습니다.
    difficulty_locked: std::sync::atomic::AtomicBool,
    pub containers: Arc<crate::game::containers::Containers>,
    /// 플레이어 저장 위치 (없으면 저장하지 않음).
    pub data_dir: Option<std::path::PathBuf>,
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
        if crate::game::custom_items::all().is_empty() {
            crate::game::custom_items::install(crate::game::custom_items::defaults());
        }
        Self::with_world(
            info,
            seed,
            WorldStore::open(seed, None).expect("memory store"),
            None,
        )
    }

    /// `dir`에 월드를 저장하는 서버. 있던 저장을 읽어 들입니다.
    pub fn open(info: ServerInfo, seed: u64, dir: std::path::PathBuf) -> std::io::Result<Self> {
        crate::game::custom_items::load(Some(&dir)).map_err(std::io::Error::other)?;
        let mut s = Self::open_instance(info, seed, dir.clone())?;
        s.perms = Arc::new(
            crate::perms::Permissions::open(Some(dir), s.config.everyone_op)
                .map_err(std::io::Error::other)?,
        );
        Ok(s)
    }

    /// 월드·상자·플레이어만 `dir`에서 읽는 인스턴스. 권한 등은 [`Self::share_from`]으로.
    pub fn open_instance(
        info: ServerInfo,
        seed: u64,
        dir: std::path::PathBuf,
    ) -> std::io::Result<Self> {
        let world = WorldStore::open(seed, Some(dir.clone()))?;
        let containers = Arc::new(crate::game::containers::Containers::open(Some(
            dir.clone(),
        ))?);
        world.attach_containers(Arc::clone(&containers));
        let mut s = Self::with_world(info, seed, world, Some(dir.clone()));
        s.containers = containers;
        Ok(s)
    }

    /// 다른 인스턴스와 권한·인증·키·끄기 신호를 함께 씁니다.
    pub fn share_from(mut self, other: &Server) -> Self {
        self.sessions = Arc::clone(&other.sessions);
        self.key = Arc::clone(&other.key);
        self.chat_keys = Arc::clone(&other.chat_keys);
        self.perms = Arc::clone(&other.perms);
        self.shutdown = Arc::clone(&other.shutdown);
        self
    }

    pub fn named(mut self, name: &str) -> Self {
        self.name = name.to_owned();
        self
    }

    pub(crate) fn attach(&self, network: &Arc<crate::network::Network>) {
        let _ = self.network.set(Arc::downgrade(network));
    }

    pub fn network(&self) -> Option<Arc<crate::network::Network>> {
        self.network.get().and_then(Weak::upgrade)
    }

    /// 같은 네트워크의 모든 인스턴스 (네트워크가 없으면 자기 자신만 — 이때는 빈 목록 대신
    /// 호출한 쪽이 `self`를 씁니다).
    pub fn instances(&self) -> Vec<Arc<Server>> {
        self.network().map(|n| n.all()).unwrap_or_default()
    }

    /// 모든 인스턴스에 알립니다 (혼자면 자기에게만).
    pub fn publish_all(&self, event: Event) {
        let all = self.instances();
        if all.is_empty() {
            self.publish(event);
        } else {
            for s in all {
                s.publish(event.clone());
            }
        }
    }

    /// 어느 인스턴스에서든 이름으로 찾기.
    pub fn find_anywhere(&self, name: &str) -> Option<(Arc<Server>, PlayerState)> {
        self.instances()
            .into_iter()
            .find_map(|s| s.find_player(name).map(|p| (s, p)))
    }

    pub fn find_player_anywhere(&self, name: &str) -> Option<PlayerState> {
        self.find_player(name)
            .or_else(|| self.find_anywhere(name).map(|(_, p)| p))
    }

    /// 네트워크 전체 접속자 (혼자면 이 인스턴스).
    pub fn players_anywhere(&self) -> Vec<PlayerState> {
        let all = self.instances();
        if all.is_empty() {
            return self.players();
        }
        all.iter().flat_map(|s| s.players()).collect()
    }

    /// 네트워크 전체 접속자 수.
    pub fn online_total(&self) -> u32 {
        match self.network() {
            Some(n) => n.all().iter().map(|s| s.online()).sum(),
            None => self.online(),
        }
    }

    fn with_world(
        info: ServerInfo,
        seed: u64,
        world: WorldStore,
        data_dir: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            name: "main".into(),
            entities: Default::default(),
            ticks: Default::default(),
            furnaces: Default::default(),
            redstone: Default::default(),
            lottery: crate::plugins::lottery::Lottery::open(
                data_dir.as_ref().map(|d| d.join("lottery.json")),
            ),
            npcs: crate::plugins::npc::Npcs::open(data_dir.as_ref().map(|d| d.join("npcs.json"))),
            perms: Arc::new(
                crate::perms::Permissions::open(None, true).expect("memory permissions"),
            ),
            network: OnceLock::new(),
            difficulty: std::sync::atomic::AtomicU8::new(Difficulty::Normal as u8),
            difficulty_locked: std::sync::atomic::AtomicBool::new(false),
            shutdown: Arc::new(tokio::sync::Notify::new()),
            containers: Arc::new(
                crate::game::containers::Containers::open(None).expect("memory containers"),
            ),
            data_dir,
            info,
            config: Config::default(),
            sessions: Arc::new(MojangSessionService),
            key: Arc::new(OnceLock::new()),
            chat_keys: Arc::new(Mutex::new(Vec::new())),
            seed,
            world: Arc::new(world),
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
        self.perms.set_everyone_op(config.everyone_op);
        self.difficulty
            .store(config.difficulty as u8, Ordering::Relaxed);
        self.config = config;
        self
    }

    pub fn with_session_service(mut self, sessions: impl SessionService + 'static) -> Self {
        self.sessions = Arc::new(sessions);
        self
    }

    /// 월드·상자를 지금 저장합니다 (플레이어는 각 세션이 따로 저장).
    pub fn save_all(&self) {
        if let Err(e) = self.containers.save() {
            eprintln!("상자 저장 실패: {e}");
        }
        match self.world.save() {
            Ok(true) => println!(
                "[{}] 월드 저장 (켠 뒤 바뀐 블록 {}개, 올린 청크 {}개)",
                self.name,
                self.world.changed_block_count(),
                self.world.loaded_chunks()
            ),
            Ok(false) => {}
            Err(e) => eprintln!("월드 저장 실패: {e}"),
        }
        // level.dat의 시간·난이도·모드도 맞춰 둡니다 (바닐라 도구가 볼 수 있게).
        if let Some(dir) = &self.data_dir
            && let Some(level) = crate::world::level::read(dir)
        {
            let (time, day_time) = self.time();
            let state = crate::world::level::LevelState {
                gamemode: self.config.default_gamemode.id(),
                difficulty: self.difficulty() as u8,
                hardcore: self.config.hardcore,
                time,
                day_time,
            };
            if let Err(e) = crate::world::level::write_level_dat(dir, &level, &state) {
                eprintln!("level.dat 저장 실패: {e}");
            }
        }
        self.publish(Event::SavePlayers);
    }

    /// 블록을 바꾸고 모두에게 알린 뒤, 반응하는 이웃(물·레드스톤·모래 …)에 틱을 예약합니다.
    pub fn set_block(&self, dim: Dimension, pos: BlockPos, state: u32) {
        self.set_block_quiet(dim, pos, state);
        crate::game::mechanics::neighbor_changed(self, dim, pos);
    }

    /// 부서진 블록 (다른 사람에게 부서지는 소리·조각이 보임).
    pub fn set_block_broken(&self, dim: Dimension, pos: BlockPos, state: u32, broken: u32) {
        self.world.set(dim, pos.0, pos.1, pos.2, state);
        self.publish(Event::BlockChanged {
            by: 0,
            dimension: dim,
            pos,
            state,
            broken: Some(broken),
        });
        crate::game::mechanics::neighbor_changed(self, dim, pos);
    }

    /// 바꾸고 알리기만 (이웃 반응 없이 — 레드스톤 가루끼리처럼 이미 함께 계산한 것).
    pub fn set_block_quiet(&self, dim: Dimension, pos: BlockPos, state: u32) {
        if self.world.get(dim, pos.0, pos.1, pos.2) == state {
            return;
        }
        self.world.set(dim, pos.0, pos.1, pos.2, state);
        self.publish(Event::BlockChanged {
            by: 0,
            dimension: dim,
            pos,
            state,
            broken: None,
        });
    }

    /// 그 자리에서 소리 (블록 분류).
    pub fn play_sound(&self, dim: Dimension, pos: BlockPos, name: &str) {
        if let Some(sound) = crate::data::sound_id(name) {
            self.publish(Event::Sound {
                dimension: dim,
                pos: (pos.0 as f64 + 0.5, pos.1 as f64 + 0.5, pos.2 as f64 + 0.5),
                sound,
                source: 4,
                volume: 1.0,
                pitch: 1.0,
            });
        }
    }

    /// 구조물 상자를 처음 열 때 바닐라 전리품 표로 채웁니다 (시드와 위치로 정해짐).
    pub fn fill_structure_loot(&self, dim: Dimension, pos: BlockPos) {
        if self.containers.contains(dim, pos) {
            return;
        }
        let Some(table) = self.world.loot_at(dim, pos.0, pos.1, pos.2) else {
            return;
        };
        let roll_seed = crate::world::noise_hash(self.seed ^ pos.1 as u64, pos.0, pos.2);
        let mut slots = vec![None; crate::game::containers::CHEST_SLOTS];
        for (slot, name, count, _enchanted) in crate::world::structure::roll_loot(table, roll_seed)
        {
            if let Some(id) = crate::data::items::by_name(name) {
                let max = crate::data::items::max_stack(id);
                slots[slot] = Some(Item::new(id, count.min(max)));
            }
        }
        self.containers.fill(dim, pos, slots);
    }

    pub fn difficulty(&self) -> Difficulty {
        Difficulty::from_id(self.difficulty.load(Ordering::Relaxed))
    }

    /// 난이도를 바꿉니다. 잠겨 있으면 false.
    pub fn set_difficulty(&self, d: Difficulty) -> bool {
        if self.difficulty_locked() {
            return false;
        }
        self.difficulty.store(d as u8, Ordering::Relaxed);
        self.publish(Event::DifficultyChanged(d));
        true
    }

    pub fn difficulty_locked(&self) -> bool {
        self.difficulty_locked.load(Ordering::Relaxed)
    }

    pub fn lock_difficulty(&self) {
        self.difficulty_locked.store(true, Ordering::Relaxed);
        self.publish(Event::DifficultyChanged(self.difficulty()));
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
    /// 다른 태스크에서 나중에 이벤트를 보낼 때.
    pub fn events_handle(&self) -> broadcast::Sender<Event> {
        self.events.clone()
    }

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
        // 다른 인스턴스에 있어도 중복 (옮기는 중이면 이미 그쪽에서 빠졌습니다).
        if self
            .instances()
            .iter()
            .any(|s| !std::ptr::eq(s.as_ref(), self) && s.find_by_uuid(state.uuid).is_some())
        {
            return Err(AlreadyOnline);
        }
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
