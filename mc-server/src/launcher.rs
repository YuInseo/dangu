//! 프로그램 모드 (런처): 실행 파일을 그냥 켜면 이것이 돕니다.
//!
//! ```text
//! mc-server.exe (런처)  ── 웹 관리 화면 :25580 + 브라우저 자동으로 열기
//!   └─ mc-server.exe serve (실제 서버, 자식 프로세스)
//!        표준 입출력을 런처가 붙잡음 → 대시보드의 실시간 콘솔·명령어
//! ```
//!
//! - 대시보드에서 켜기·끄기·다시 켜기·업데이트, 콘솔 보기와 명령어 치기, 접속자, `server.env` 편집.
//! - 서버가 충돌로 꺼지면 5초 뒤 다시 켬. `stop`(대시보드의 끄기)으로 끄면 런처는 남아 다시 켤 수
//!   있음. 런처 자체를 끝내려면 이 창에 `종료`(또는 창 닫기·Ctrl+C).
//! - 업데이트: 서버가 새 버전을 받아 바꾸고 코드 75로 끝나면, 런처도 새 실행 파일로 다시 켬.
//! - 다른 설정 화면(길드·퀘스트·등급·상점…)은 같은 파일을 다루므로 런처가 그대로 내어 줌.
//! - `MC_LAUNCHER=false`면 예전처럼 서버를 바로 켭니다. `MC_OPEN_BROWSER=false`면 브라우저를 안 엶.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Notify, mpsc};

use crate::json::Json;

/// 대시보드가 기억하는 콘솔 줄 수.
const KEEP_LINES: usize = 3000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Stopped,
    Starting,
    Running,
    Stopping,
    /// 충돌 뒤 다시 켜기를 기다림.
    Crashed,
}

impl State {
    fn key(self) -> &'static str {
        match self {
            State::Stopped => "stopped",
            State::Starting => "starting",
            State::Running => "running",
            State::Stopping => "stopping",
            State::Crashed => "crashed",
        }
    }
}

pub struct Launcher {
    state: Mutex<State>,
    lines: Mutex<VecDeque<(u64, String)>>,
    next_seq: AtomicU64,
    stdin: Mutex<Option<mpsc::UnboundedSender<String>>>,
    /// 켜져 있기를 바람 (끄기를 누르면 false).
    want_running: AtomicBool,
    /// 끈 뒤 곧바로 다시 켜기 (다시 켜기 버튼).
    restart_after_stop: AtomicBool,
    crashes: AtomicU32,
    pid: AtomicU32,
    since: Mutex<Option<SystemTime>>,
    wake: Notify,
}

static LAUNCHER: OnceLock<Arc<Launcher>> = OnceLock::new();

/// 런처 모드로 도는 중이면 그것.
pub fn get() -> Option<&'static Arc<Launcher>> {
    LAUNCHER.get()
}

pub fn enabled() -> bool {
    std::env::var("MC_LAUNCHER").map_or(true, |v| !matches!(v.as_str(), "0" | "false" | "off"))
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}

impl Launcher {
    fn new() -> Self {
        Launcher {
            state: Mutex::new(State::Stopped),
            lines: Mutex::new(VecDeque::new()),
            next_seq: AtomicU64::new(1),
            stdin: Mutex::new(None),
            want_running: AtomicBool::new(true),
            restart_after_stop: AtomicBool::new(false),
            crashes: AtomicU32::new(0),
            pid: AtomicU32::new(0),
            since: Mutex::new(None),
            wake: Notify::new(),
        }
    }

    pub fn state(&self) -> State {
        *self.state.lock().unwrap()
    }

    fn set_state(&self, s: State) {
        *self.state.lock().unwrap() = s;
    }

    /// 콘솔 한 줄 (이 창에도 찍고 대시보드에도).
    pub fn log(&self, line: &str) {
        println!("{line}");
        self.push(line);
    }

    fn push(&self, line: &str) {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        let mut l = self.lines.lock().unwrap();
        l.push_back((seq, line.to_owned()));
        while l.len() > KEEP_LINES {
            l.pop_front();
        }
    }

    /// `after` 다음 줄들과 마지막 번호.
    pub fn lines_after(&self, after: u64) -> (Vec<(u64, String)>, u64) {
        let l = self.lines.lock().unwrap();
        let out: Vec<(u64, String)> = l.iter().filter(|(s, _)| *s > after).cloned().collect();
        let last = l.back().map_or(after, |(s, _)| *s);
        (out, last.max(after))
    }

    /// 서버 콘솔에 명령어 (꺼져 있으면 false).
    pub fn command(&self, line: &str) -> bool {
        let line = line.trim().trim_start_matches('/');
        if line.is_empty() {
            return false;
        }
        let ok = self
            .stdin
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|tx| tx.send(format!("{line}\n")).is_ok());
        if ok {
            self.push(&format!("> {line}"));
        }
        ok
    }

    pub fn start(&self) {
        self.want_running.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    pub fn stop(&self) {
        self.want_running.store(false, Ordering::SeqCst);
        self.restart_after_stop.store(false, Ordering::SeqCst);
        self.graceful_stop();
    }

    pub fn restart(&self) {
        self.want_running.store(true, Ordering::SeqCst);
        if matches!(self.state(), State::Running | State::Starting) {
            self.restart_after_stop.store(true, Ordering::SeqCst);
            self.graceful_stop();
        } else {
            self.wake.notify_one();
        }
    }

    /// `stop`을 보내고, 30초 안에 안 꺼지면 (감독이) 강제로.
    fn graceful_stop(&self) {
        if matches!(self.state(), State::Running | State::Starting) {
            self.set_state(State::Stopping);
            self.command("stop");
        }
    }

    pub fn status_json(&self) -> Json {
        let n = |v: f64| Json::Number(v);
        let since = self
            .since
            .lock()
            .unwrap()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(Json::Null, |d| n(d.as_millis() as f64));
        // 서버가 5초마다 적는 접속자 상태
        let live = std::fs::read_to_string(status_path())
            .ok()
            .and_then(|t| crate::json::parse(&t).ok())
            .unwrap_or(Json::Null);
        Json::Object(vec![
            ("state".into(), Json::String(self.state().key().into())),
            ("pid".into(), n(self.pid.load(Ordering::SeqCst) as f64)),
            ("since".into(), since),
            (
                "crashes".into(),
                n(self.crashes.load(Ordering::SeqCst) as f64),
            ),
            (
                "build".into(),
                crate::updater::BUILD.map_or(Json::Null, |b| Json::String(b.to_owned())),
            ),
            ("auto_update".into(), Json::Bool(crate::updater::enabled())),
            ("server".into(), live),
            ("connect".into(), connect_info()),
            ("now".into(), n(now_ms())),
        ])
    }
}

/// 서버(자식)가 적는 상태 파일.
pub fn status_path() -> std::path::PathBuf {
    crate::plugins::store::data_path("MC_STATUS", "status.json")
}

/// 접속 주소: 이 컴퓨터, 같은 공유기(LAN).
fn connect_info() -> Json {
    let bind = std::env::var("MC_BIND").unwrap_or_else(|_| "0.0.0.0:25565".into());
    let port = bind.rsplit(':').next().unwrap_or("25565").to_owned();
    // 패킷을 보내지 않고 바깥으로 나갈 때 쓸 내 주소만 알아냄
    let lan = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .ok()
        .map(|a| a.ip().to_string())
        .filter(|ip| ip != "0.0.0.0");
    let mut f = vec![
        (
            "local".to_owned(),
            Json::String(format!("localhost:{port}")),
        ),
        ("port".into(), Json::String(port.clone())),
    ];
    if let Some(ip) = lan {
        f.push(("lan".into(), Json::String(format!("{ip}:{port}"))));
    }
    Json::Object(f)
}

fn env_path() -> std::path::PathBuf {
    std::env::var_os("MC_ENV_FILE")
        .map(Into::into)
        .unwrap_or_else(|| "server.env".into())
}

/// `server.env` 원문.
pub fn read_settings() -> String {
    std::fs::read_to_string(env_path()).unwrap_or_else(|_| crate::envfile::TEMPLATE.to_owned())
}

pub fn write_settings(text: &str) -> Result<(), String> {
    std::fs::write(env_path(), text).map_err(|e| e.to_string())
}

/// 브라우저로 주소 열기 (실패해도 그만).
pub fn open_browser(url: &str) {
    if std::env::var("MC_OPEN_BROWSER").is_ok_and(|v| matches!(v.as_str(), "0" | "false" | "off"))
        || std::env::var_os("HOYA_RELAUNCHED").is_some()
    {
        return;
    }
    let r = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open")
            .arg(url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    };
    if r.is_err() {
        println!("브라우저를 열지 못했습니다. 직접 여세요: {url}");
    }
}

/// 런처를 돌립니다 (돌아오지 않음 — 끝날 때 프로세스를 끝냄).
pub async fn run() -> std::io::Result<()> {
    let l = Arc::new(Launcher::new());
    let _ = LAUNCHER.set(Arc::clone(&l));

    // 설정 화면들이 같은 파일을 보도록 (서버와 같은 설정 파일들)
    crate::plugins::guild::config::open_global();
    crate::plugins::quest::open_global();
    crate::plugins::rules::open_global();
    crate::plugins::rank::open_global();
    crate::plugins::menu::open_global();
    crate::plugins::shop::open_global();
    crate::plugins::equip::config::open_global();

    // 대시보드 (웹 관리 화면)
    match crate::plugins::webadmin::settings() {
        Some((addr, token)) => match bind_retry(&addr).await {
            Ok(listener) => {
                let shown = addr.replace("0.0.0.0", "127.0.0.1");
                let url = format!("http://{shown}/#token={token}");
                l.log(&format!("대시보드: {url}"));
                tokio::spawn(crate::plugins::webadmin::serve(listener, token));
                open_browser(&url);
            }
            Err(e) => l.log(&format!("대시보드 {addr}을 열지 못했습니다: {e}")),
        },
        None => l.log("MC_ADMIN_PORT=0 — 대시보드 없이 돕니다."),
    }
    l.log("이 창: 서버 명령어를 그대로 칠 수 있습니다 · 프로그램 끝내기: 종료");

    // 이 창의 키보드 입력 → 서버 콘솔 (`종료`는 런처가)
    let lk = Arc::clone(&l);
    tokio::spawn(async move {
        let mut input = BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = input.next_line().await {
            match line.trim() {
                "종료" | "quit" | "exit" => {
                    lk.log("프로그램을 끝냅니다…");
                    lk.stop();
                    // 서버가 꺼지면 감독이 끝냄
                    lk.want_running.store(false, Ordering::SeqCst);
                    QUIT.store(true, Ordering::SeqCst);
                    lk.wake.notify_one();
                }
                "켜기" | "start" if lk.state() == State::Stopped => lk.start(),
                other => {
                    if !lk.command(other) && !other.is_empty() {
                        lk.log("서버가 꺼져 있습니다. (켜기 / 종료)");
                    }
                }
            }
        }
    });

    // Ctrl+C (유닉스는 SIGTERM도): 서버를 저장하고 끄게 두고 끝냄
    let lc = Arc::clone(&l);
    tokio::spawn(async move {
        #[cfg(unix)]
        let term = async {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut s) => {
                    s.recv().await;
                }
                Err(_) => std::future::pending::<()>().await,
            }
        };
        #[cfg(not(unix))]
        let term = std::future::pending::<()>();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term => {}
        }
        QUIT.store(true, Ordering::SeqCst);
        lc.want_running.store(false, Ordering::SeqCst);
        lc.wake.notify_one();
    });

    supervise(l).await
}

static QUIT: AtomicBool = AtomicBool::new(false);

/// 업데이트로 다시 켤 때는 옛 런처가 포트를 막 놓는 중일 수 있어 몇 번 다시.
async fn bind_retry(addr: &str) -> std::io::Result<tokio::net::TcpListener> {
    let mut last = None;
    for _ in 0..20 {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => return Ok(l),
            Err(e) => last = Some(e),
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(last.unwrap())
}

/// 서버 프로세스를 띄우고 지켜봄.
async fn supervise(l: Arc<Launcher>) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    loop {
        if QUIT.load(Ordering::SeqCst) {
            std::process::exit(0);
        }
        if !l.want_running.load(Ordering::SeqCst) {
            l.set_state(State::Stopped);
            tokio::select! {
                _ = l.wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
            continue;
        }
        l.set_state(State::Starting);
        l.log("── 서버를 켭니다 ──");
        // server.env를 켤 때마다 다시 읽어 넘김: 대시보드에서 고친 설정이 "다시 켜기"로 적용
        // (진짜 환경 변수로 준 것은 그대로 이김)
        let settings: Vec<(String, String)> = crate::envfile::parse(&read_settings())
            .into_iter()
            .filter(|(k, v)| !v.is_empty() && !crate::envfile::given_by_environment(k))
            .collect();
        let mut child = match Command::new(&exe)
            .arg("serve")
            .envs(settings)
            .env("HOYA_LAUNCHER", "launcher")
            .env("MC_ADMIN_PORT", "0")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                l.log(&format!("서버를 켜지 못했습니다: {e}"));
                l.want_running.store(false, Ordering::SeqCst);
                continue;
            }
        };
        l.pid.store(child.id().unwrap_or(0), Ordering::SeqCst);
        *l.since.lock().unwrap() = Some(SystemTime::now());

        // 출력 → 콘솔 줄들. "듣는 중"이 나오면 켜짐
        for out in [
            child
                .stdout
                .take()
                .map(|o| Box::new(o) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
            child
                .stderr
                .take()
                .map(|o| Box::new(o) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
        ]
        .into_iter()
        .flatten()
        {
            let lo = Arc::clone(&l);
            tokio::spawn(async move {
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.contains("에서 듣는 중") && lo.state() == State::Starting {
                        lo.set_state(State::Running);
                    }
                    lo.log(&line);
                }
            });
        }
        // 명령어 → 표준 입력
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        *l.stdin.lock().unwrap() = Some(tx);
        if let Some(mut stdin) = child.stdin.take() {
            tokio::spawn(async move {
                while let Some(line) = rx.recv().await {
                    if stdin.write_all(line.as_bytes()).await.is_err() {
                        break;
                    }
                    let _ = stdin.flush().await;
                }
            });
        }

        // 끝나기를 기다림 (끄기를 눌렀는데 30초 넘게 안 꺼지면 강제로)
        let status = loop {
            tokio::select! {
                s = child.wait() => break s,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    if QUIT.load(Ordering::SeqCst) && l.state() != State::Stopping {
                        l.graceful_stop();
                    }
                }
            }
            if l.state() == State::Stopping {
                match tokio::time::timeout(Duration::from_secs(30), child.wait()).await {
                    Ok(s) => break s,
                    Err(_) => {
                        l.log("30초 안에 꺼지지 않아 강제로 끕니다.");
                        let _ = child.kill().await;
                        break child.wait().await;
                    }
                }
            }
        };
        *l.stdin.lock().unwrap() = None;
        l.pid.store(0, Ordering::SeqCst);
        *l.since.lock().unwrap() = None;
        let _ = std::fs::remove_file(status_path());
        let code = status.ok().and_then(|s| s.code());
        match code {
            Some(0) => {
                l.log("── 서버를 껐습니다 ──");
                if l.restart_after_stop.swap(false, Ordering::SeqCst) {
                    continue;
                }
                if !QUIT.load(Ordering::SeqCst) {
                    l.want_running.store(false, Ordering::SeqCst);
                }
            }
            Some(crate::updater::RESTART_EXIT_CODE) => {
                l.log("── 업데이트한 새 버전으로 프로그램을 다시 켭니다 ──");
                // 서버가 이미 실행 파일을 갈아 끼웠음 (아직이면 여기서)
                let _ = crate::updater::apply_staged();
                crate::updater::relaunch();
            }
            other => {
                let n = l.crashes.fetch_add(1, Ordering::SeqCst) + 1;
                l.set_state(State::Crashed);
                l.log(&format!(
                    "── 서버가 비정상 종료했습니다 (코드 {}, {n}번째). 5초 뒤 다시 켭니다 ──",
                    other.map_or("없음".to_owned(), |c| c.to_string())
                ));
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

/// 서버(자식)에서: 5초마다 접속자 상태를 적음 (런처 대시보드가 읽음).
pub async fn status_writer(net: Arc<crate::network::Network>) {
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    let started = now_ms();
    loop {
        tick.tick().await;
        let instances: Vec<Json> = net
            .all()
            .iter()
            .map(|s| {
                Json::Object(vec![
                    ("name".into(), Json::String(s.name.clone())),
                    ("online".into(), Json::Number(f64::from(s.online()))),
                    (
                        "players".into(),
                        Json::Array(
                            s.players()
                                .into_iter()
                                .map(|p| Json::String(p.name))
                                .collect(),
                        ),
                    ),
                ])
            })
            .collect();
        let j = Json::Object(vec![
            ("updated".into(), Json::Number(now_ms())),
            ("started".into(), Json::Number(started)),
            ("instances".into(), Json::Array(instances)),
        ]);
        let p = status_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, j.compact()).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}
