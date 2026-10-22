//! 마스터 노드: 접속 라우팅(프록시), 관리자 CLI, 인스턴스 프로세스들과의 IPC, SharedData.
//!
//! ```text
//!            플레이어 ──TCP──▶ 마스터:25565 ─┬─(라우팅)─▶ 127.0.0.1:25601  마을   (자식 프로세스)
//!                                            ├──────────▶ 127.0.0.1:25602  야생
//!                                            └──────────▶ 127.0.0.1:25603  rpg
//!            관리자 ──stdin──▶ CLI                ▲  ▲  ▲
//!                               └─ IPC (유닉스 소켓, 프레임 JSON) ─┘
//! ```
//!
//! 인스턴스를 옮길 때는 바닐라 Transfer 패킷을 씁니다: 자식이 `TransferRequest`를 보내면
//! 마스터가 "이 이름은 다음에 rpg로"라고 적어 두고 `Transfer`로 답하고, 자식이 클라이언트에게
//! Transfer 패킷(마스터 주소)을 보냅니다. 클라이언트가 다시 접속하면 프록시가 적어 둔 곳으로
//! 잇습니다. 그래서 LocalData(좌표·체력·쿨타임)는 떠난 인스턴스에 남고(그쪽 플레이어
//! 파일), 새 인스턴스는 자기 것을 씁니다. SharedData만 마스터가 들고 따라갑니다.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::net::{TcpListener, UnixListener, UnixStream};
use tokio::sync::{Notify, mpsc};

use super::ipc::{Msg, read_msg, write_msg};
use super::process::{InstanceConfig, Running, State, Watch, load_configs, save_configs};
use super::shared::{Backend, SharedStore};
use crate::packets::status::ServerInfo;

/// 프록시를 거쳐 다시 들어오기까지 기다려 주는 시간.
const TRANSFER_TTL: Duration = Duration::from_secs(30);
const WATCH_EVERY: Duration = Duration::from_secs(2);
const FLUSH_EVERY: Duration = Duration::from_secs(5);
/// 꺼지라고 한 뒤 기다리는 시간.
const STOP_GRACE: Duration = Duration::from_secs(15);
/// 첫 내부 포트.
pub const FIRST_PORT: u16 = 25601;

#[derive(Debug, Clone)]
pub struct Online {
    pub name: String,
    pub instance: String,
    /// 플레이 시간을 마지막으로 더한 때.
    counted: Instant,
}

pub struct Master {
    pub base: PathBuf,
    pub ipc_path: PathBuf,
    pub info: ServerInfo,
    pub shared: SharedStore,
    configs: Mutex<Vec<InstanceConfig>>,
    running: Mutex<HashMap<String, Running>>,
    links: Mutex<HashMap<String, mpsc::UnboundedSender<Msg>>>,
    /// 처음 들어오는 사람이 갈 곳.
    default_route: Mutex<String>,
    /// 이름(소문자) → (갈 인스턴스, 적어 둔 때).
    pending: Mutex<HashMap<String, (String, Instant)>>,
    players: Mutex<HashMap<u128, Online>>,
    pub shutdown: Notify,
}

impl Master {
    pub fn new(base: &Path, info: ServerInfo) -> std::io::Result<Arc<Self>> {
        std::fs::create_dir_all(base)?;
        let mut configs = load_configs(&base.join("dambi.json"));
        if configs.is_empty() {
            // 처음 켤 때: 기획서의 세 컨텐츠 월드
            configs = ["마을", "야생", "rpg"]
                .iter()
                .enumerate()
                .map(|(i, n)| InstanceConfig::new(n, FIRST_PORT + i as u16))
                .collect();
            save_configs(&base.join("dambi.json"), &configs)?;
        }
        let default = configs.first().map(|c| c.name.clone()).unwrap_or_default();
        Ok(Arc::new(Self {
            base: base.to_owned(),
            ipc_path: base.join("master.sock"),
            info,
            shared: SharedStore::new(Backend::Files(base.join("shared"))),
            configs: Mutex::new(configs),
            running: Mutex::new(HashMap::new()),
            links: Mutex::new(HashMap::new()),
            default_route: Mutex::new(default),
            pending: Mutex::new(HashMap::new()),
            players: Mutex::new(HashMap::new()),
            shutdown: Notify::new(),
        }))
    }

    // ---- 인스턴스 ----

    pub fn configs(&self) -> Vec<InstanceConfig> {
        self.configs.lock().unwrap().clone()
    }

    pub fn config(&self, name: &str) -> Option<InstanceConfig> {
        self.configs
            .lock()
            .unwrap()
            .iter()
            .find(|c| c.name == name)
            .cloned()
    }

    /// 설정을 더하거나 바꾸고 저장합니다.
    pub fn upsert_config(&self, c: InstanceConfig) -> std::io::Result<()> {
        let mut all = self.configs.lock().unwrap();
        match all.iter_mut().find(|x| x.name == c.name) {
            Some(x) => *x = c,
            None => all.push(c),
        }
        save_configs(&self.base.join("dambi.json"), &all)
    }

    pub fn free_port(&self) -> u16 {
        let used: Vec<u16> = self
            .configs
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.port)
            .collect();
        (FIRST_PORT..).find(|p| !used.contains(p)).unwrap()
    }

    pub fn start(&self, name: &str) -> Result<u32, String> {
        let c = self
            .config(name)
            .ok_or_else(|| format!("{name}: 설정이 없습니다"))?;
        let mut running = self.running.lock().unwrap();
        if running.contains_key(name) {
            return Err(format!("{name}은(는) 이미 켜져 있습니다"));
        }
        let r = Running::spawn(&c, &self.base, &self.ipc_path).map_err(|e| e.to_string())?;
        let pid = r.pid;
        running.insert(name.to_owned(), r);
        Ok(pid)
    }

    /// 저장하고 끄라고 합니다 (안 꺼지면 감시가 죽임).
    pub fn stop(&self, name: &str) -> Result<(), String> {
        let mut running = self.running.lock().unwrap();
        let r = running
            .get_mut(name)
            .ok_or_else(|| format!("{name}은(는) 꺼져 있습니다"))?;
        r.begin_stop(STOP_GRACE);
        if !self.send(name, Msg::Shutdown) {
            r.kill();
        }
        Ok(())
    }

    pub fn running_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.running.lock().unwrap().keys().cloned().collect();
        v.sort();
        v
    }

    /// (이름, pid, 포트, 상태, 접속자, RSS, 할당량, cgroup 여부, 켠 시간)
    #[allow(clippy::type_complexity)]
    pub fn status(
        &self,
    ) -> Vec<(
        String,
        u32,
        u16,
        State,
        u32,
        u64,
        Option<u64>,
        bool,
        Duration,
    )> {
        let players = self.players.lock().unwrap();
        let online = |name: &str| players.values().filter(|o| o.instance == name).count() as u32;
        let running = self.running.lock().unwrap();
        let mut v: Vec<_> = running
            .values()
            .map(|r| {
                (
                    r.config.name.clone(),
                    r.pid,
                    r.config.port,
                    r.state,
                    online(&r.config.name).max(r.online),
                    r.rss.max(super::process::rss_of(r.pid).unwrap_or(0)),
                    r.config.ram,
                    r.cgroup.is_some(),
                    r.started.elapsed(),
                )
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    pub fn set_default(&self, name: &str) -> Result<(), String> {
        if self.config(name).is_none() {
            return Err(format!("{name}: 설정이 없습니다"));
        }
        *self.default_route.lock().unwrap() = name.to_owned();
        Ok(())
    }

    pub fn default_name(&self) -> String {
        self.default_route.lock().unwrap().clone()
    }

    /// 접속 받을 준비가 된 인스턴스의 포트.
    fn ready_port(&self, name: &str) -> Option<u16> {
        self.running
            .lock()
            .unwrap()
            .get(name)
            .filter(|r| r.state == State::Ready)
            .map(|r| r.config.port)
    }

    /// 프록시: 이 이름을 어디로 보낼까. (인스턴스, 내부 포트)
    pub fn route(&self, player: &str) -> Result<(String, u16), String> {
        let pending = {
            let mut p = self.pending.lock().unwrap();
            p.retain(|_, (_, at)| at.elapsed() < TRANSFER_TTL);
            p.remove(&player.to_lowercase()).map(|(to, _)| to)
        };
        let target = pending.unwrap_or_else(|| self.default_name());
        match self.ready_port(&target) {
            Some(port) => Ok((target, port)),
            None => Err(format!(
                "'{target}' 인스턴스가 아직 준비되지 않았습니다. 잠시 뒤 다시 접속해 주세요."
            )),
        }
    }

    pub fn online_total(&self) -> u32 {
        self.players.lock().unwrap().len() as u32
    }

    pub fn players(&self) -> Vec<(u128, Online)> {
        let mut v: Vec<_> = self
            .players
            .lock()
            .unwrap()
            .iter()
            .map(|(u, o)| (*u, o.clone()))
            .collect();
        v.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        v
    }

    pub fn find_player(&self, name: &str) -> Option<(u128, Online)> {
        self.players
            .lock()
            .unwrap()
            .iter()
            .find(|(_, o)| o.name.eq_ignore_ascii_case(name))
            .map(|(u, o)| (*u, o.clone()))
    }

    // ---- IPC ----

    /// 인스턴스 하나에 보냅니다. 연결이 없으면 false.
    pub fn send(&self, instance: &str, msg: Msg) -> bool {
        self.links
            .lock()
            .unwrap()
            .get(instance)
            .is_some_and(|tx| tx.send(msg).is_ok())
    }

    /// 모든 인스턴스에 (전역 버스).
    pub fn broadcast(&self, msg: Msg) {
        for tx in self.links.lock().unwrap().values() {
            let _ = tx.send(msg.clone());
        }
    }

    /// 플레이어를 옮기도록 적어 두고, 지금 있는 인스턴스에 내보내라고 합니다.
    pub fn transfer(&self, uuid: u128, name: &str, to: &str) -> Result<(), String> {
        if self.ready_port(to).is_none() {
            return Err(format!("'{to}' 인스턴스가 켜져 있지 않습니다"));
        }
        let from = self
            .players
            .lock()
            .unwrap()
            .get(&uuid)
            .map(|o| o.instance.clone());
        let Some(from) = from else {
            return Err(format!("{name}님은 접속해 있지 않습니다"));
        };
        if from == to {
            return Err(format!("{name}님은 이미 {to}에 있습니다"));
        }
        self.pending
            .lock()
            .unwrap()
            .insert(name.to_lowercase(), (to.to_owned(), Instant::now()));
        self.send(
            &from,
            Msg::Transfer {
                uuid,
                to: to.to_owned(),
            },
        );
        Ok(())
    }

    /// 플레이 시간을 SharedData에 더합니다.
    fn count_playtime(&self, uuid: u128) {
        let secs = {
            let mut players = self.players.lock().unwrap();
            let Some(o) = players.get_mut(&uuid) else {
                return;
            };
            let s = o.counted.elapsed().as_secs();
            o.counted += Duration::from_secs(s);
            s
        };
        if secs > 0 {
            self.shared.modify(uuid, |d| {
                d.playtime_secs += secs;
                true
            });
        }
    }

    /// 인스턴스가 보낸 메시지 하나. 답할 것이 있으면 돌려줍니다.
    fn handle(&self, instance: &str, msg: Msg) -> Option<Msg> {
        match msg {
            Msg::Hello { port, pid, .. } => {
                if let Some(r) = self.running.lock().unwrap().get_mut(instance) {
                    r.state = State::Ready;
                }
                println!("[마스터] {instance} 준비됨 (pid {pid}, 127.0.0.1:{port})");
                None
            }
            Msg::Status { online, rss_kib } => {
                if let Some(r) = self.running.lock().unwrap().get_mut(instance) {
                    r.online = online;
                    r.rss = r.rss.max(rss_kib * 1024);
                }
                None
            }
            Msg::Joined { uuid, name } => {
                self.players.lock().unwrap().insert(
                    uuid,
                    Online {
                        name,
                        instance: instance.to_owned(),
                        counted: Instant::now(),
                    },
                );
                None
            }
            Msg::Left { uuid } => {
                self.count_playtime(uuid);
                let mut players = self.players.lock().unwrap();
                // 옮기는 중이면 새 인스턴스의 Joined가 먼저 왔을 수 있음
                if players.get(&uuid).is_some_and(|o| o.instance == instance) {
                    players.remove(&uuid);
                }
                None
            }
            Msg::SharedGet { req, uuid } => Some(Msg::Shared {
                req,
                uuid,
                ok: true,
                data: self.shared.get(uuid),
            }),
            Msg::SharedAdd {
                req,
                uuid,
                field,
                delta,
            } => {
                let (ok, data) = self.shared.modify(uuid, |d| d.add(&field, delta));
                Some(Msg::Shared {
                    req,
                    uuid,
                    ok,
                    data,
                })
            }
            Msg::JobXp { req, uuid, job, xp } => {
                let (ok, data) = self.shared.modify(uuid, |d| {
                    d.add_job_xp(&job, xp);
                    true
                });
                Some(Msg::Shared {
                    req,
                    uuid,
                    ok,
                    data,
                })
            }
            Msg::Pay {
                req,
                from,
                to,
                amount,
            } => {
                let target = self.find_player(&to).map(|(u, o)| (u, o.instance));
                let ok = match target {
                    Some((t, _)) if t != from && amount > 0 => {
                        let (paid, _) = self.shared.modify(from, |d| d.add("money", -amount));
                        if paid {
                            self.shared.modify(t, |d| d.add("money", amount));
                        }
                        paid
                    }
                    _ => false,
                };
                if ok && let Some((t, at)) = target {
                    self.send(
                        &at,
                        Msg::SharedChanged {
                            uuid: t,
                            data: self.shared.get(t),
                        },
                    );
                }
                Some(Msg::Shared {
                    req,
                    uuid: from,
                    ok,
                    data: self.shared.get(from),
                })
            }
            Msg::TransferRequest { uuid, name, to } => match self.transfer(uuid, &name, &to) {
                Ok(()) => None,
                Err(reason) => Some(Msg::Deny { uuid, reason }),
            },
            Msg::Publish { channel, payload } => {
                if channel == crate::contents::PACK_RELOAD {
                    // 마스터가 내려 주는 팩부터 새로
                    let l = crate::contents::reload_and_rebuild();
                    crate::contents::summary(&l)
                        .iter()
                        .for_each(|l| println!("[마스터] {l}"));
                }
                self.broadcast(Msg::Event { channel, payload });
                None
            }
            other => {
                eprintln!("[마스터] {instance}가 보낸 뜻밖의 메시지: {other:?}");
                None
            }
        }
    }

    /// 인스턴스가 끊김: 그쪽 사람들을 정리합니다.
    fn disconnected(&self, instance: &str) {
        self.links.lock().unwrap().remove(instance);
        let gone: Vec<u128> = self
            .players
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, o)| o.instance == instance)
            .map(|(u, _)| *u)
            .collect();
        for u in gone {
            self.count_playtime(u);
            self.players.lock().unwrap().remove(&u);
        }
    }
}

/// IPC 소켓: 인스턴스마다 연결 하나. 첫 메시지는 `Hello`여야 합니다.
pub async fn serve_ipc(master: Arc<Master>) -> std::io::Result<()> {
    let _ = std::fs::remove_file(&master.ipc_path);
    let listener = UnixListener::bind(&master.ipc_path)?;
    loop {
        let (stream, _) = listener.accept().await?;
        let m = Arc::clone(&master);
        tokio::spawn(async move {
            if let Err(e) = ipc_link(&m, stream).await {
                eprintln!("[마스터] IPC 연결 오류: {e}");
            }
        });
    }
}

async fn ipc_link(master: &Arc<Master>, stream: UnixStream) -> std::io::Result<()> {
    let (mut r, mut w) = stream.into_split();
    let Some(first) = read_msg(&mut r).await? else {
        return Ok(());
    };
    let Msg::Hello { instance, .. } = &first else {
        return Err(std::io::Error::other("첫 메시지가 Hello가 아님"));
    };
    let instance = instance.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    master
        .links
        .lock()
        .unwrap()
        .insert(instance.clone(), tx.clone());
    master.handle(&instance, first);
    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if write_msg(&mut w, &m).await.is_err() {
                break;
            }
        }
    });
    let result = loop {
        match read_msg(&mut r).await {
            Ok(Some(msg)) => {
                if let Some(reply) = master.handle(&instance, msg) {
                    let _ = tx.send(reply);
                }
            }
            Ok(None) => break Ok(()),
            Err(e) => break Err(e),
        }
    };
    writer.abort();
    master.disconnected(&instance);
    println!("[마스터] {instance} IPC 끊김");
    result
}

/// 감시: 2초마다 자식들의 생사와 램, 5초마다 SharedData 쓰기와 인스턴스 목록 알림.
pub async fn housekeeping(master: Arc<Master>) {
    let mut watch = tokio::time::interval(WATCH_EVERY);
    let mut last_flush = Instant::now();
    loop {
        watch.tick().await;
        let mut restart = Vec::new();
        let mut over = Vec::new();
        {
            let mut running = master.running.lock().unwrap();
            let mut exited = Vec::new();
            for (name, r) in running.iter_mut() {
                match r.watch() {
                    Watch::Fine => {}
                    Watch::Warn => println!(
                        "[마스터] {name}: 램 {}% 사용 (할당 {})",
                        r.rss * 100 / r.config.ram.unwrap_or(u64::MAX).max(1),
                        super::process::format_bytes(r.config.ram.unwrap_or(0))
                    ),
                    Watch::OverLimit => over.push(name.clone()),
                    Watch::Killed => println!("[마스터] {name}: 제때 꺼지지 않아 강제 종료"),
                    Watch::Exited(code) => {
                        let crashed = r.state != State::Stopping;
                        println!(
                            "[마스터] {name} 종료 (코드 {}){}",
                            code.map_or("신호".into(), |c| c.to_string()),
                            if crashed { " — 비정상 종료" } else { "" }
                        );
                        // 켜자마자 죽는 것은 다시 켜지 않음 (무한 재시작 방지)
                        if crashed && r.started.elapsed() > Duration::from_secs(30) {
                            restart.push(name.clone());
                        }
                        exited.push(name.clone());
                    }
                }
            }
            for n in exited {
                running.remove(&n);
            }
        }
        for name in over {
            println!("[마스터] {name}: 램 할당량을 넘어 저장 후 재시작합니다");
            let _ = master.stop(&name);
        }
        for name in restart {
            match master.start(&name) {
                Ok(pid) => println!("[마스터] {name} 다시 켬 (pid {pid})"),
                Err(e) => println!("[마스터] {name} 다시 켜지 못함: {e}"),
            }
        }
        if last_flush.elapsed() >= FLUSH_EVERY {
            last_flush = Instant::now();
            let uuids: Vec<u128> = master.players.lock().unwrap().keys().copied().collect();
            for u in uuids {
                master.count_playtime(u);
            }
            let list: Vec<(String, u32)> = master
                .status()
                .into_iter()
                .filter(|s| s.3 == State::Ready)
                .map(|s| (s.0, s.4))
                .collect();
            master.broadcast(Msg::Instances { list });
            let m = Arc::clone(&master);
            match tokio::task::spawn_blocking(move || m.shared.flush()).await {
                Ok(Err(e)) => eprintln!("[마스터] SharedData 저장 실패: {e}"),
                Err(e) => eprintln!("[마스터] SharedData 저장 실패: {e}"),
                Ok(Ok(_)) => {}
            }
        }
    }
}

/// `mc-server master`: 마스터 노드를 켭니다.
pub async fn run() -> std::io::Result<()> {
    let base = PathBuf::from(std::env::var("DAMBI_HOME").unwrap_or_else(|_| "dambi".into()));
    let bind = std::env::var("MC_BIND").unwrap_or_else(|_| "0.0.0.0:25565".into());
    let info = ServerInfo {
        name: "Dambi".into(),
        description: "담비 서버 — 순수 Rust".into(),
        max_players: 500,
    };
    let master = Master::new(&base, info)?;
    tokio::spawn({
        let m = Arc::clone(&master);
        async move {
            if let Err(e) = serve_ipc(m).await {
                eprintln!("[마스터] IPC 소켓을 열지 못했습니다: {e}");
            }
        }
    });
    tokio::spawn(housekeeping(Arc::clone(&master)));
    for c in master.configs().into_iter().filter(|c| c.autostart) {
        match master.start(&c.name) {
            Ok(pid) => println!("[마스터] {} 켬 (pid {pid}, 포트 {})", c.name, c.port),
            Err(e) => println!("[마스터] {} 켜지 못함: {e}", c.name),
        }
    }
    let listener = TcpListener::bind(&bind).await?;
    println!(
        "[마스터] {bind}에서 듣는 중 — 기본 인스턴스 '{}', 기준 폴더 {} (도움말: help)",
        master.default_name(),
        base.display()
    );
    tokio::spawn(super::cli::run(Arc::clone(&master)));
    tokio::select! {
        r = super::proxy::serve(listener, Arc::clone(&master)) => r?,
        _ = tokio::signal::ctrl_c() => println!("[마스터] 끄는 중…"),
        _ = master.shutdown.notified() => println!("[마스터] 끄는 중…"),
    }
    shutdown_all(&master).await;
    Ok(())
}

/// 모든 인스턴스를 저장·종료시키고 SharedData를 씁니다.
pub async fn shutdown_all(master: &Arc<Master>) {
    for name in master.running_names() {
        let _ = master.stop(&name);
    }
    let deadline = Instant::now() + STOP_GRACE;
    while !master.running_names().is_empty() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
        master
            .running
            .lock()
            .unwrap()
            .retain(|_, r| !matches!(r.watch(), Watch::Exited(_)));
    }
    for name in master.running_names() {
        if let Some(r) = master.running.lock().unwrap().get_mut(&name) {
            r.kill();
        }
    }
    let uuids: Vec<u128> = master.players.lock().unwrap().keys().copied().collect();
    for u in uuids {
        master.count_playtime(u);
    }
    if let Err(e) = master.shared.flush() {
        eprintln!("[마스터] SharedData 저장 실패: {e}");
    }
}
