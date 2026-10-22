//! 인스턴스(자식 프로세스) 쪽: 마스터에 IPC로 붙어 SharedData를 묻고, 옮기기를 부탁하고,
//! 전역 버스의 소식을 받습니다.
//!
//! 마스터 없이 혼자 켠 서버(`mc-server`)에서도 같은 함수들이 돌아가도록, 연결이 없으면
//! 이 프로세스 안의 [`SharedStore`]를 씁니다 (재화는 그 서버에만 남음).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};

use super::ipc::{Msg, read_msg, write_msg};
use super::shared::{Backend, SharedData, SharedStore};
use crate::network::Network;
use crate::server::Event;
use crate::text;

const HEARTBEAT: Duration = Duration::from_secs(5);
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

type Handler = Box<dyn Fn(&str) + Send + Sync>;

struct Link {
    name: String,
    tx: mpsc::UnboundedSender<Msg>,
    next_req: AtomicU64,
    waiting: Mutex<HashMap<u64, oneshot::Sender<(bool, SharedData)>>>,
    instances: Mutex<Vec<(String, u32)>>,
}

static LINK: OnceLock<Link> = OnceLock::new();
static LOCAL: OnceLock<SharedStore> = OnceLock::new();
static HANDLERS: Mutex<Vec<(String, Handler)>> = Mutex::new(Vec::new());

/// 마스터 뒤에서 도는 자식 프로세스인가 (`DAMBI_IPC`가 있음).
pub fn active() -> bool {
    std::env::var_os("DAMBI_IPC").is_some()
}

/// 이 인스턴스의 이름 (자식일 때).
pub fn instance_name() -> Option<&'static str> {
    LINK.get().map(|l| l.name.as_str())
}

/// 혼자 켠 서버: 재화를 이 폴더에 둡니다.
pub fn init_local(dir: Option<PathBuf>) {
    let _ = LOCAL.set(SharedStore::new(match dir {
        Some(d) => Backend::Files(d),
        None => Backend::Memory,
    }));
}

fn local() -> &'static SharedStore {
    LOCAL.get_or_init(|| SharedStore::new(Backend::Memory))
}

/// 전역 버스의 `channel` 소식을 받을 함수 (공유 플러그인용).
pub fn subscribe(channel: &str, f: impl Fn(&str) + Send + Sync + 'static) {
    HANDLERS
        .lock()
        .unwrap()
        .push((channel.to_owned(), Box::new(f)));
}

/// 전역 버스에 올립니다 (모든 인스턴스가 받음). 혼자 켠 서버면 자기만 받습니다.
pub fn publish(channel: &str, payload: &str) {
    match LINK.get() {
        Some(l) => {
            let _ = l.tx.send(Msg::Publish {
                channel: channel.to_owned(),
                payload: payload.to_owned(),
            });
        }
        None => dispatch(channel, payload),
    }
}

fn dispatch(channel: &str, payload: &str) {
    for (c, f) in HANDLERS.lock().unwrap().iter() {
        if c == channel {
            f(payload);
        }
    }
}

/// 마스터에 묻고 답을 기다립니다.
async fn ask(make: impl FnOnce(u64) -> Msg) -> Option<(bool, SharedData)> {
    let l = LINK.get()?;
    let req = l.next_req.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    l.waiting.lock().unwrap().insert(req, tx);
    l.tx.send(make(req)).ok()?;
    let r = tokio::time::timeout(REPLY_TIMEOUT, rx).await.ok()?.ok();
    l.waiting.lock().unwrap().remove(&req);
    r
}

/// 이 사람의 SharedData.
pub async fn shared(uuid: u128) -> Option<SharedData> {
    if LINK.get().is_none() {
        return Some(local().get(uuid));
    }
    ask(|req| Msg::SharedGet { req, uuid }).await.map(|r| r.1)
}

/// 재화 더하기·빼기 (`money` · `cash` · `afk`). (성공, 결과).
pub async fn shared_add(uuid: u128, field: &str, delta: i64) -> Option<(bool, SharedData)> {
    if LINK.get().is_none() {
        return Some(local().modify(uuid, |d| d.add(field, delta)));
    }
    ask(|req| Msg::SharedAdd {
        req,
        uuid,
        field: field.to_owned(),
        delta,
    })
    .await
}

/// 직업 경험치 (공식: 3.0 × 레벨² + 300.0).
pub async fn job_xp(uuid: u128, job: &str, xp: f64) -> Option<SharedData> {
    if LINK.get().is_none() {
        return Some(
            local()
                .modify(uuid, |d| {
                    d.add_job_xp(job, xp);
                    true
                })
                .1,
        );
    }
    ask(|req| Msg::JobXp {
        req,
        uuid,
        job: job.to_owned(),
        xp,
    })
    .await
    .map(|r| r.1)
}

/// 송금. 혼자 켠 서버에서는 받는 사람의 UUID를 알아야 하므로 `to_uuid`도 받습니다.
pub async fn pay(
    from: u128,
    to: &str,
    to_uuid: Option<u128>,
    amount: i64,
) -> Option<(bool, SharedData)> {
    if LINK.get().is_none() {
        let target = to_uuid?;
        if target == from || amount <= 0 {
            return Some((false, local().get(from)));
        }
        let (ok, data) = local().modify(from, |d| d.add("money", -amount));
        if ok {
            local().modify(target, |d| d.add("money", amount));
        }
        return Some((ok, data));
    }
    ask(|req| Msg::Pay {
        req,
        from,
        to: to.to_owned(),
        amount,
    })
    .await
}

/// 다른 인스턴스로 보내 달라고 합니다. 답(`Transfer`/`Deny`)은 세션 이벤트로 옵니다.
pub fn request_transfer(uuid: u128, name: &str, to: &str) -> bool {
    LINK.get().is_some_and(|l| {
        l.tx.send(Msg::TransferRequest {
            uuid,
            name: name.to_owned(),
            to: to.to_owned(),
        })
        .is_ok()
    })
}

/// 마스터가 마지막으로 알려 준 인스턴스 목록 (이름, 접속자).
pub fn instances() -> Vec<(String, u32)> {
    LINK.get()
        .map(|l| l.instances.lock().unwrap().clone())
        .unwrap_or_default()
}

/// 자식 프로세스로 켤 때: 마스터에 붙어 `Hello`를 보내고, 메시지를 처리하는 태스크들을 띄웁니다.
pub async fn start(net: Arc<Network>, port: u16) -> std::io::Result<()> {
    let path =
        std::env::var_os("DAMBI_IPC").ok_or_else(|| std::io::Error::other("DAMBI_IPC 없음"))?;
    let name = std::env::var("DAMBI_NAME").unwrap_or_else(|_| "instance".into());
    // 마스터 소켓이 막 생기는 중일 수 있어 몇 번 다시
    let mut stream = None;
    for _ in 0..50 {
        match UnixStream::connect(&path).await {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    let stream = stream.ok_or_else(|| std::io::Error::other("마스터 IPC에 붙지 못함"))?;
    let (mut r, mut w) = stream.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    let _ = LINK.set(Link {
        name: name.clone(),
        tx: tx.clone(),
        next_req: AtomicU64::new(1),
        waiting: Mutex::new(HashMap::new()),
        instances: Mutex::new(Vec::new()),
    });
    write_msg(
        &mut w,
        &Msg::Hello {
            instance: name,
            port,
            pid: std::process::id(),
        },
    )
    .await?;
    tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if write_msg(&mut w, &m).await.is_err() {
                break;
            }
        }
    });

    let server = net.default_instance();
    // 접속·퇴장을 마스터에
    let mut events = server.subscribe();
    let up = tx.clone();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(Event::Joined(p)) => {
                    let _ = up.send(Msg::Joined {
                        uuid: p.uuid,
                        name: p.name,
                    });
                }
                Ok(Event::Left { uuid, .. }) => {
                    let _ = up.send(Msg::Left { uuid });
                }
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(_) => break,
            }
        }
    });
    // 살아 있다는 신호
    let beat = tx.clone();
    let s = Arc::clone(&server);
    tokio::spawn(async move {
        let mut t = tokio::time::interval(HEARTBEAT);
        loop {
            t.tick().await;
            let rss = super::process::rss_of(std::process::id()).unwrap_or(0);
            if beat
                .send(Msg::Status {
                    online: s.online(),
                    rss_kib: rss / 1024,
                })
                .is_err()
            {
                break;
            }
        }
    });
    // 마스터가 보낸 것
    tokio::spawn(async move {
        loop {
            let msg = match read_msg(&mut r).await {
                Ok(Some(m)) => m,
                Ok(None) | Err(_) => {
                    eprintln!("마스터와 연결이 끊겼습니다 — 저장하고 끕니다");
                    server.shutdown.notify_one();
                    break;
                }
            };
            handle(&server, msg);
        }
    });
    Ok(())
}

fn handle(server: &Arc<crate::Server>, msg: Msg) {
    let Some(l) = LINK.get() else { return };
    match msg {
        Msg::Shared { req, ok, data, .. } => {
            if let Some(tx) = l.waiting.lock().unwrap().remove(&req) {
                let _ = tx.send((ok, data));
            }
        }
        Msg::SharedChanged { uuid, data } => {
            server.publish(Event::Whisper {
                to: uuid,
                sender: "재화".into(),
                content: format!(
                    "돈 {} · 캐시 {} · 잠수포인트 {}",
                    data.money, data.cash, data.afk
                ),
            });
        }
        Msg::Transfer { uuid, to } => server.publish(Event::TransferOut { uuid, to }),
        Msg::Deny { uuid, reason } => server.publish(Event::Whisper {
            to: uuid,
            sender: "이동".into(),
            content: reason,
        }),
        Msg::Command { line } => {
            let line = line.trim().trim_start_matches('/');
            let mut words = line.split_whitespace();
            let name = words.next().unwrap_or_default();
            let args: Vec<&str> = words.collect();
            let actor = crate::admin::Actor::console();
            match crate::admin::run(server, &actor, name, &args) {
                Some(out) => out.iter().for_each(|l| println!("{l}")),
                None => println!("콘솔에서 쓸 수 없는 명령어입니다: /{name}"),
            }
        }
        Msg::Event { channel, payload } => {
            if channel == crate::plugins::contents::PACK_RELOAD {
                let l = crate::plugins::contents::reload_and_rebuild();
                crate::plugins::contents::summary(&l)
                    .iter()
                    .for_each(|l| println!("{l}"));
                server.publish_all(Event::ResourcePackChanged);
            }
            if channel == crate::plugins::guild::BUS {
                crate::plugins::guild::session_receive(server, &payload);
            }
            if channel == crate::plugins::chat::BUS {
                crate::plugins::chat::receive(server, &payload);
            }
            if channel == "broadcast" {
                server.publish(Event::System(text::colored(
                    &format!("[공지] {payload}"),
                    "gold",
                )));
            }
            dispatch(&channel, &payload);
        }
        Msg::Instances { list } => *l.instances.lock().unwrap() = list,
        Msg::Shutdown => {
            println!("마스터가 끄라고 했습니다 — 저장하고 끕니다");
            server.shutdown.notify_one();
        }
        other => eprintln!("마스터가 보낸 뜻밖의 메시지: {other:?}"),
    }
}
