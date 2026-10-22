//! 여러 인스턴스(= 따로 노는 작은 서버)를 한 프로세스·한 포트에 두는 네트워크.
//!
//! 인스턴스마다 월드·엔티티·상자·접속자·채팅·시간·난이도가 따로 있고, 권한·밴·인증·
//! 사용자 정의 아이템은 함께 씁니다. BungeeCord/Velocity처럼 플레이어를 옮기지만, 따로
//! 프록시를 두지 않고 Configuration 단계로 되돌렸다가(`Start Configuration`) 다른
//! 인스턴스의 Play로 다시 들여보냅니다.
//!
//! 목록은 `<기본 폴더>/instances.json`에 둡니다. 처음 켤 때 없으면 만듭니다.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use crate::json::{self, Json};
use crate::packets::play::GameMode;
use crate::packets::status::ServerInfo;
use crate::server::{Config, Difficulty, Server};

pub const FILE_NAME: &str = "instances.json";

/// 인스턴스 하나의 설정 (`instances.json`의 한 항목).
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceSpec {
    pub name: String,
    /// 기본 폴더 기준 상대 경로 ("."이면 기본 폴더 자체 — 예전 단일 월드).
    pub dir: String,
    pub seed: u64,
    pub gamemode: GameMode,
    pub difficulty: Difficulty,
    pub hardcore: bool,
    pub spawn_mobs: bool,
}

impl InstanceSpec {
    pub fn new(name: &str, seed: u64) -> Self {
        Self {
            name: name.to_owned(),
            dir: name.to_owned(),
            seed,
            gamemode: GameMode::Survival,
            difficulty: Difficulty::Normal,
            hardcore: false,
            spawn_mobs: true,
        }
    }

    fn to_json(&self) -> Json {
        Json::Object(vec![
            ("name".into(), Json::String(self.name.clone())),
            ("dir".into(), Json::String(self.dir.clone())),
            // 64비트 시드는 f64에 다 안 들어가므로 문자열로
            ("seed".into(), Json::String(self.seed.to_string())),
            ("gamemode".into(), Json::String(self.gamemode.name().into())),
            (
                "difficulty".into(),
                Json::String(self.difficulty.name().into()),
            ),
            ("hardcore".into(), Json::Bool(self.hardcore)),
            ("spawn_mobs".into(), Json::Bool(self.spawn_mobs)),
        ])
    }

    fn from_json(v: &Json) -> Result<Self, String> {
        let name = v.get("name").and_then(Json::as_str).ok_or("이름 없음")?;
        if !valid_name(name) {
            return Err(format!("인스턴스 이름이 잘못됨: {name}"));
        }
        let seed = match v.get("seed") {
            Some(Json::String(s)) => s
                .parse()
                .map_err(|_| format!("{name}: 시드가 숫자가 아님"))?,
            Some(Json::Number(n)) => *n as u64,
            _ => return Err(format!("{name}: 시드 없음")),
        };
        let mut spec = InstanceSpec::new(name, seed);
        if let Some(d) = v.get("dir").and_then(Json::as_str) {
            spec.dir = d.to_owned();
        }
        if let Some(m) = v.get("gamemode").and_then(Json::as_str) {
            spec.gamemode =
                GameMode::from_name(m).ok_or(format!("{name}: 모르는 게임 모드 {m}"))?;
        }
        if let Some(d) = v.get("difficulty").and_then(Json::as_str) {
            spec.difficulty =
                Difficulty::from_name(d).ok_or(format!("{name}: 모르는 난이도 {d}"))?;
        }
        if let Some(Json::Bool(b)) = v.get("hardcore") {
            spec.hardcore = *b;
        }
        if let Some(Json::Bool(b)) = v.get("spawn_mobs") {
            spec.spawn_mobs = *b;
        }
        Ok(spec)
    }
}

/// 영문 소문자·숫자·`_`·`-`, 1~32자 (폴더 이름이 되므로).
pub fn valid_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

struct Running {
    spec: InstanceSpec,
    server: Arc<Server>,
    ticker: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(t) = self.ticker.take() {
            t.abort();
        }
    }
}

pub struct Network {
    instances: RwLock<Vec<Running>>,
    default: RwLock<String>,
    /// 없으면 저장하지 않는 네트워크 (테스트).
    base_dir: Option<PathBuf>,
    info: ServerInfo,
    config: Config,
    /// 인스턴스를 만들거나 지울 때 한 번에 하나씩.
    edit: Mutex<()>,
}

impl Network {
    /// 인스턴스 하나뿐인 네트워크 (테스트, 예전 방식).
    pub fn single(server: Arc<Server>) -> Arc<Self> {
        let spec = InstanceSpec::new(&server.name, server.seed);
        let net = Arc::new(Network {
            instances: RwLock::new(Vec::new()),
            default: RwLock::new(spec.name.clone()),
            base_dir: None,
            info: server.info.clone(),
            config: server.config.clone(),
            edit: Mutex::new(()),
        });
        net.insert(spec, server);
        net
    }

    /// 메모리 인스턴스들 (테스트). 첫 번째가 기본.
    pub fn in_memory(servers: Vec<Arc<Server>>) -> Arc<Self> {
        let first = Arc::clone(&servers[0]);
        let net = Network::single(first);
        for s in servers.into_iter().skip(1) {
            net.insert(InstanceSpec::new(&s.name, s.seed), s);
        }
        net
    }

    /// `base`의 `instances.json`을 읽어 인스턴스를 엽니다. 없으면 `initial`로 만들고 저장.
    /// `base`에 예전 단일 월드(`blocks.dat`)가 있으면 그것이 첫 인스턴스(`dir: "."`)가 됩니다.
    pub fn open(
        base: &Path,
        info: ServerInfo,
        config: Config,
        initial: Vec<InstanceSpec>,
    ) -> std::io::Result<Arc<Self>> {
        std::fs::create_dir_all(base)?;
        let path = base.join(FILE_NAME);
        let (default, specs) = match std::fs::read_to_string(&path) {
            Ok(text) => parse_file(&text)
                .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))?,
            Err(_) => {
                let mut specs = initial;
                if base.join("blocks.dat").exists() || base.join("players").exists() {
                    // 예전 단일 월드를 그대로 첫 인스턴스로
                    specs[0].dir = ".".into();
                }
                let default = specs[0].name.clone();
                write_file(&path, &default, &specs)?;
                println!("{} 만듦: 인스턴스 {}개", path.display(), specs.len());
                (default, specs)
            }
        };
        if !specs.iter().any(|s| s.name == default) {
            return Err(std::io::Error::other(format!(
                "기본 인스턴스 {default}가 목록에 없음"
            )));
        }

        crate::game::custom_items::load(Some(base)).map_err(std::io::Error::other)?;
        let perms = Arc::new(
            crate::perms::Permissions::open(Some(base.to_path_buf()), config.everyone_op)
                .map_err(std::io::Error::other)?,
        );
        let net = Arc::new(Network {
            instances: RwLock::new(Vec::new()),
            default: RwLock::new(default),
            base_dir: Some(base.to_path_buf()),
            info,
            config,
            edit: Mutex::new(()),
        });
        let mut first: Option<Arc<Server>> = None;
        for spec in specs {
            let server = net.build(&spec, first.as_deref(), Some(Arc::clone(&perms)))?;
            first.get_or_insert_with(|| Arc::clone(&server));
            net.insert(spec, server);
        }
        Ok(net)
    }

    fn build(
        &self,
        spec: &InstanceSpec,
        share: Option<&Server>,
        perms: Option<Arc<crate::perms::Permissions>>,
    ) -> std::io::Result<Arc<Server>> {
        let config = Config {
            default_gamemode: spec.gamemode,
            difficulty: spec.difficulty,
            hardcore: spec.hardcore,
            spawn_mobs: spec.spawn_mobs && self.config.spawn_mobs,
            ..self.config.clone()
        };
        let mut created = false;
        let server = match &self.base_dir {
            Some(base) => {
                let dir = base.join(&spec.dir);
                created = crate::world::level::ensure(&dir, &spec.name, spec.seed)?;
                Server::open_instance(self.info.clone(), spec.seed, dir)?
            }
            None => Server::new(self.info.clone(), spec.seed),
        }
        .named(&spec.name);
        // 새 월드: 스폰 둘레를 미리 만들어 둡니다 (첫 접속이 기다리지 않도록).
        if created && self.config.pregen_radius > 0 {
            let spawn =
                crate::world::terrain::spawn_point(crate::world::Dimension::Overworld, spec.seed);
            let center = ((spawn.0.floor() as i32) >> 4, (spawn.2.floor() as i32) >> 4);
            let r = self.config.pregen_radius;
            println!(
                "{}: 스폰 둘레 {}×{} 청크를 미리 만듭니다",
                spec.name,
                2 * r + 1,
                2 * r + 1
            );
            let t = std::time::Instant::now();
            server
                .world
                .pregenerate(crate::world::Dimension::Overworld, center, r)?;
            println!("{}: {:.1}초 걸림", spec.name, t.elapsed().as_secs_f64());
        }
        let mut server = match share {
            Some(other) => server.share_from(other),
            None => server,
        };
        if share.is_none()
            && let Some(p) = perms
        {
            server.perms = p;
        }
        Ok(Arc::new(server.with_config(config)))
    }

    fn insert(self: &Arc<Self>, spec: InstanceSpec, server: Arc<Server>) {
        server.attach(self);
        // 런타임 밖(동기 테스트)에서도 만들 수 있게, 틱은 런타임이 있을 때만
        let ticker = tokio::runtime::Handle::try_current()
            .ok()
            .map(|h| h.spawn(crate::game::entities::run(Arc::clone(&server))));
        self.instances.write().unwrap().push(Running {
            spec,
            server,
            ticker,
        });
    }

    pub fn all(&self) -> Vec<Arc<Server>> {
        self.instances
            .read()
            .unwrap()
            .iter()
            .map(|r| Arc::clone(&r.server))
            .collect()
    }

    pub fn specs(&self) -> Vec<InstanceSpec> {
        self.instances
            .read()
            .unwrap()
            .iter()
            .map(|r| r.spec.clone())
            .collect()
    }

    pub fn get(&self, name: &str) -> Option<Arc<Server>> {
        self.instances
            .read()
            .unwrap()
            .iter()
            .find(|r| r.spec.name == name)
            .map(|r| Arc::clone(&r.server))
    }

    pub fn default_name(&self) -> String {
        self.default.read().unwrap().clone()
    }

    pub fn default_instance(&self) -> Arc<Server> {
        self.get(&self.default_name())
            .expect("default instance exists")
    }

    /// 새 인스턴스를 만들고 목록에 저장합니다.
    pub fn create(self: &Arc<Self>, spec: InstanceSpec) -> Result<Arc<Server>, String> {
        let _guard = self.edit.lock().unwrap();
        if !valid_name(&spec.name) {
            return Err("이름은 영문 소문자·숫자·_·- 1~32자".into());
        }
        if self.get(&spec.name).is_some() {
            return Err(format!("{}은(는) 이미 있습니다.", spec.name));
        }
        let first = self.default_instance();
        let server = self
            .build(&spec, Some(&first), None)
            .map_err(|e| e.to_string())?;
        self.insert(spec, Arc::clone(&server));
        self.save().map_err(|e| e.to_string())?;
        Ok(server)
    }

    /// 인스턴스를 내립니다 (폴더는 남김). 기본 인스턴스와 사람이 있는 인스턴스는 안 됨.
    pub fn remove(&self, name: &str) -> Result<(), String> {
        let _guard = self.edit.lock().unwrap();
        if name == self.default_name() {
            return Err("기본 인스턴스는 내릴 수 없습니다.".into());
        }
        let server = self
            .get(name)
            .ok_or(format!("{name} 인스턴스가 없습니다."))?;
        if server.online() > 0 {
            return Err(format!("{name}에 아직 {}명이 있습니다.", server.online()));
        }
        server.save_all();
        self.instances
            .write()
            .unwrap()
            .retain(|r| r.spec.name != name);
        self.save().map_err(|e| e.to_string())
    }

    pub fn set_default(&self, name: &str) -> Result<(), String> {
        if self.get(name).is_none() {
            return Err(format!("{name} 인스턴스가 없습니다."));
        }
        *self.default.write().unwrap() = name.to_owned();
        self.save().map_err(|e| e.to_string())
    }

    pub fn save_all(&self) {
        for s in self.all() {
            s.save_all();
        }
    }

    fn save(&self) -> std::io::Result<()> {
        match &self.base_dir {
            Some(base) => write_file(&base.join(FILE_NAME), &self.default_name(), &self.specs()),
            None => Ok(()),
        }
    }
}

fn parse_file(text: &str) -> Result<(String, Vec<InstanceSpec>), String> {
    let v = json::parse(text)?;
    let list = match v.get("instances") {
        Some(Json::Array(a)) => a
            .iter()
            .map(InstanceSpec::from_json)
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err("instances 배열이 없음".into()),
    };
    if list.is_empty() {
        return Err("인스턴스가 하나도 없음".into());
    }
    let default = v
        .get("default")
        .and_then(Json::as_str)
        .map_or_else(|| list[0].name.clone(), str::to_owned);
    Ok((default, list))
}

fn write_file(path: &Path, default: &str, specs: &[InstanceSpec]) -> std::io::Result<()> {
    let v = Json::Object(vec![
        ("default".into(), Json::String(default.to_owned())),
        (
            "instances".into(),
            Json::Array(specs.iter().map(InstanceSpec::to_json).collect()),
        ),
    ]);
    let dir = path.parent().unwrap_or(Path::new("."));
    crate::game::playerdata::write_atomically(dir, FILE_NAME, v.pretty().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_round_trip() {
        let mut s = InstanceSpec::new("creative-1", u64::MAX - 3);
        s.gamemode = GameMode::Creative;
        s.difficulty = Difficulty::Peaceful;
        s.hardcore = true;
        let text = Json::Object(vec![
            ("default".into(), Json::String("creative-1".into())),
            ("instances".into(), Json::Array(vec![s.to_json()])),
        ])
        .pretty();
        let (default, list) = parse_file(&text).unwrap();
        assert_eq!(default, "creative-1");
        assert_eq!(list, vec![s]);
    }

    #[test]
    fn first_start_creates_instances_and_worlds() {
        let base = std::env::temp_dir().join(format!("dangu-net-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let info = ServerInfo {
            name: "t".into(),
            description: "t".into(),
            max_players: 1,
        };
        let mut pvp = InstanceSpec::new("pvp", 7);
        pvp.hardcore = true;
        let net = Network::open(
            &base,
            info.clone(),
            Config::default(),
            vec![InstanceSpec::new("lobby", 3), pvp],
        )
        .unwrap();
        assert_eq!(net.default_name(), "lobby");
        assert!(base.join(FILE_NAME).exists());
        assert_eq!(
            crate::world::level::read(&base.join("pvp")).unwrap().seed,
            7
        );
        assert!(net.get("pvp").unwrap().config.hardcore);
        // 권한은 하나를 함께 씀
        assert!(Arc::ptr_eq(
            &net.get("pvp").unwrap().perms,
            &net.get("lobby").unwrap().perms
        ));
        drop(net);
        // 다시 열면 파일을 따르고 초기 목록은 무시
        let net = Network::open(
            &base,
            info,
            Config::default(),
            vec![InstanceSpec::new("other", 1)],
        )
        .unwrap();
        assert_eq!(net.specs().len(), 2);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn names_are_folder_safe() {
        assert!(valid_name("lobby"));
        assert!(valid_name("pvp_2"));
        assert!(!valid_name("../x"));
        assert!(!valid_name("Lobby"));
        assert!(!valid_name(""));
    }
}
