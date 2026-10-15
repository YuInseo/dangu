//! 다른 기기와 잇기 — 선택 사항이다. 휴대폰 앱은 PC나 서버 없이 혼자 라이브러리를 읽고 튼다.
//! 원할 때만:
//!
//! - **이 기기를 연다**: 같은 와이파이의 다른 기기(휴대폰, 다른 PC)가 이 소리를 열 수 있게
//!   LAN 쪽 포트를 하나 더 연다. 기본은 꺼짐이고, 켜면 6자리 PIN으로 잠근다(비밀번호를
//!   정해 두었으면 그것으로). 끄면 열려 있던 연결도 바로 끊는다.
//! - **찾는다**: 같은 와이파이의 소리를 UDP 방송으로 찾는다.
//! - **기억한다**: 연결했던 소리의 주소.
//!
//! 연결은 화면을 그 소리로 옮기는 것이다. 그 소리의 라이브러리를 이 기기에서 듣거나(브라우저 존),
//! 그 기기의 스피커로 튼다(그 기기의 장치 존을 원격으로).

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use futures_util::StreamExt;
use parking_lot::Mutex;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::App;

/// 찾기에 쓰는 UDP 포트
pub const DISCOVERY_PORT: u16 = 4379;
/// 앱이 LAN 쪽으로 여는 포트(앞에서부터 비어 있는 것)
const LAN_PORTS: std::ops::RangeInclusive<u16> = 4390..=4399;
const PROBE: &[u8] = b"SORI-DISCOVER 1";

/// LAN 쪽 포트로 들어온 요청이라는 표시. 공유 켜고 끄기, PIN 보기는 이 기기에서만 한다.
#[derive(Clone, Copy, Debug)]
pub struct ViaLan;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct LinkFile {
    #[serde(default)]
    lan: bool,
    #[serde(default)]
    pin: String,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    saved: Vec<Saved>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// 이 기기 안에서만(앱의 기본)
    Off,
    /// 같은 와이파이에 열어 두었다(앱에서 켰다)
    On,
    /// 처음부터 LAN에 열린 서버(서버 실행 파일, 0.0.0.0)
    Always,
}

struct Running {
    port: u16,
    stop: CancellationToken,
    udp: Arc<AtomicBool>,
}

pub struct Link {
    file: PathBuf,
    conf: Mutex<LinkFile>,
    /// 이 프로세스의 표식. 찾기에서 나 자신을 뺀다.
    pub id: String,
    /// 주 리스너가 LAN에 열려 있으면 그 포트(서버 실행 파일)
    always: Mutex<Option<u16>>,
    running: Mutex<Option<Running>>,
}

/// 사람이 알아보는 이 기기의 이름: 휴대폰 모델, PC 이름.
pub fn device_name() -> String {
    let cmd = |prog: &str, args: &[&str]| {
        std::process::Command::new(prog)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let name = if cfg!(target_os = "android") {
        cmd("getprop", &["ro.product.model"])
    } else if cfg!(windows) {
        std::env::var("COMPUTERNAME").ok()
    } else if cfg!(target_os = "macos") {
        cmd("scutil", &["--get", "ComputerName"])
    } else {
        std::fs::read_to_string("/proc/sys/kernel/hostname").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    };
    name.unwrap_or_else(|| match std::env::consts::OS {
        "android" => "Android".into(),
        "windows" => "Windows PC".into(),
        os => os.into(),
    })
}

/// 이 기기의 LAN IPv4 주소와 그 망의 방송 주소.
fn lan_addrs() -> Vec<(Ipv4Addr, Option<Ipv4Addr>)> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback() && i.is_oper_up() && !i.is_p2p)
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v) if !v.ip.is_link_local() => Some((v.ip, v.broadcast)),
            _ => None,
        })
        .collect()
}

fn new_pin() -> String {
    format!("{:06}", rand::rng().random_range(0..1_000_000))
}

/// 사람이 넣은 주소를 다듬는다: `192.168.0.5:4390` → `http://192.168.0.5:4390`
pub fn normalize_url(raw: &str) -> Option<String> {
    let s = raw.trim().trim_end_matches('/');
    if s.is_empty() || s.len() > 200 || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let s = if s.starts_with("http://") || s.starts_with("https://") { s.to_string() } else { format!("http://{s}") };
    let rest = s.split_once("://")?.1;
    let host = rest.split('/').next()?;
    (!host.is_empty() && !host.contains('@')).then_some(s)
}

impl Link {
    pub fn open(data_dir: &std::path::Path) -> Link {
        let file = data_dir.join("link.json");
        let conf: LinkFile = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let id: String = (0..12).map(|_| char::from(b"abcdefghijklmnopqrstuvwxyz0123456789"[rand::rng().random_range(0..36)])).collect();
        Link { file, conf: Mutex::new(conf), id, always: Mutex::new(None), running: Mutex::new(None) }
    }

    fn save(&self) {
        let conf = self.conf.lock().clone();
        if let Ok(bytes) = serde_json::to_vec_pretty(&conf)
            && let Err(e) = sori_core::config::write_atomic(&self.file, &bytes)
        {
            tracing::warn!("연결 설정을 저장하지 못했습니다: {e}");
        }
    }

    pub fn name(&self) -> String {
        self.conf.lock().name.clone().unwrap_or_else(device_name)
    }

    pub fn mode(&self) -> Mode {
        if self.always.lock().is_some() {
            Mode::Always
        } else if self.running.lock().is_some() {
            Mode::On
        } else {
            Mode::Off
        }
    }

    /// 다른 기기가 여는 포트
    pub fn port(&self) -> Option<u16> {
        (*self.always.lock()).or_else(|| self.running.lock().as_ref().map(|r| r.port))
    }

    pub fn saved(&self) -> Vec<Saved> {
        self.conf.lock().saved.clone()
    }

    fn reply(&self, port: u16, locked: bool) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "sori": 1, "id": self.id, "name": self.name(), "port": port, "version": sori_core::VERSION, "locked": locked,
        }))
        .unwrap_or_default()
    }
}

/// 찾기 요청에 답하는 스레드. `stop`이 서면 그만둔다.
fn respond(link: Arc<Link>, port: u16, locked: bool, stop: Arc<AtomicBool>) {
    let r = std::thread::Builder::new().name("sori-discovery".into()).spawn(move || {
        let sock = match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("찾기 포트({DISCOVERY_PORT})를 열 수 없습니다: {e}");
                return;
            }
        };
        let _ = sock.set_read_timeout(Some(Duration::from_millis(400)));
        let mut buf = [0u8; 64];
        while !stop.load(Ordering::Relaxed) {
            if let Ok((n, from)) = sock.recv_from(&mut buf)
                && &buf[..n] == PROBE
            {
                let _ = sock.send_to(&link.reply(port, locked), from);
            }
        }
    });
    if let Err(e) = r {
        tracing::warn!("찾기 스레드를 만들 수 없습니다: {e}");
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Found {
    pub id: String,
    pub name: String,
    pub url: String,
    pub version: String,
    pub locked: bool,
}

/// 같은 와이파이의 소리를 찾는다. `wait` 동안 답을 모은다(나 자신은 뺀다).
pub fn discover(own_id: &str, wait: Duration) -> Vec<Found> {
    let Ok(sock) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) else { return Vec::new() };
    let _ = sock.set_broadcast(true);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(60)));
    let mut targets: Vec<Ipv4Addr> = vec![Ipv4Addr::BROADCAST, Ipv4Addr::LOCALHOST];
    targets.extend(lan_addrs().into_iter().filter_map(|(_, b)| b));
    targets.dedup();
    let send = |sock: &UdpSocket| {
        for t in &targets {
            let _ = sock.send_to(PROBE, SocketAddr::new(IpAddr::V4(*t), DISCOVERY_PORT));
        }
    };
    send(&sock);
    let start = Instant::now();
    let mut resent = false;
    let mut found: Vec<Found> = Vec::new();
    let mut buf = [0u8; 1024];
    while start.elapsed() < wait {
        // UDP는 잃어버릴 수 있으니 한 번 더 묻는다.
        if !resent && start.elapsed() > wait / 3 {
            send(&sock);
            resent = true;
        }
        let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&buf[..n]) else { continue };
        let (Some(id), Some(port)) = (v["id"].as_str(), v["port"].as_u64()) else { continue };
        if v["sori"] != 1 || id == own_id {
            continue;
        }
        let url = format!("http://{}:{port}", from.ip());
        let item = Found {
            id: id.to_string(),
            name: v["name"].as_str().unwrap_or("소리").chars().take(80).collect(),
            url,
            version: v["version"].as_str().unwrap_or_default().chars().take(20).collect(),
            locked: v["locked"].as_bool().unwrap_or(false),
        };
        match found.iter_mut().find(|f| f.id == item.id) {
            // 같은 기기가 루프백과 LAN 양쪽에서 답하면 LAN 주소를 쓴다(다른 기기에 알려 줄 수 있는 주소).
            Some(f) if f.url.contains("127.0.0.1") => *f = item,
            Some(_) => {}
            None => found.push(item),
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// 끄면 열려 있던 응답(SSE 포함)도 끝나게 본문을 감싼다.
async fn kill_switch(token: CancellationToken, req: Request, next: Next) -> Response {
    if token.is_cancelled() {
        return (StatusCode::SERVICE_UNAVAILABLE, "공유를 껐습니다").into_response();
    }
    let res = next.run(req).await;
    res.map(|body| Body::from_stream(body.into_data_stream().take_until(token.cancelled_owned())))
}

/// 같은 와이파이에 연다. 이미 열려 있으면 그 포트를 돌려준다.
pub async fn start_lan(app: &App) -> anyhow::Result<u16> {
    let link = app.link.clone();
    if let Some(p) = link.port() {
        return Ok(p);
    }
    let pin = {
        let mut c = link.conf.lock();
        if c.pin.len() != 6 {
            c.pin = new_pin();
        }
        c.pin.clone()
    };
    // 비밀번호를 정해 두었으면 그것으로, 아니면 PIN으로 잠근다. 이 기기 안(127.0.0.1)은 그대로 열려 있다.
    let mut lan_app = app.clone();
    if lan_app.password.is_empty() {
        lan_app.password = Arc::new(pin);
    }
    let token = CancellationToken::new();
    let t = token.clone();
    let router = crate::router(lan_app)
        .layer(Extension(ViaLan))
        .layer(axum::middleware::from_fn(move |req: Request, next: Next| kill_switch(t.clone(), req, next)));
    let preferred = link.conf.lock().port;
    let mut listener = None;
    for p in preferred.into_iter().chain(LAN_PORTS) {
        if let Ok(l) = tokio::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, p)).await {
            listener = Some(l);
            break;
        }
    }
    let listener =
        listener.ok_or_else(|| anyhow::anyhow!("LAN 쪽 포트({}–{})가 모두 쓰이고 있습니다", LAN_PORTS.start(), LAN_PORTS.end()))?;
    let port = listener.local_addr()?.port();
    let stop = token.clone();
    tokio::spawn(async move {
        let serve = axum::serve(listener, router).with_graceful_shutdown(stop.cancelled_owned());
        if let Err(e) = serve.await {
            tracing::warn!("LAN 공유가 멈췄습니다: {e}");
        }
    });
    let udp = Arc::new(AtomicBool::new(false));
    respond(link.clone(), port, true, udp.clone());
    *link.running.lock() = Some(Running { port, stop: token, udp });
    {
        let mut c = link.conf.lock();
        c.lan = true;
        c.port = Some(port);
    }
    link.save();
    tracing::info!("같은 와이파이에 열었습니다: 포트 {port}");
    Ok(port)
}

pub fn stop_lan(app: &App) {
    let link = &app.link;
    if let Some(r) = link.running.lock().take() {
        r.stop.cancel();
        r.udp.store(true, Ordering::Relaxed);
    }
    link.conf.lock().lan = false;
    link.save();
}

/// 서버가 뜰 때: 주 리스너가 LAN에 열려 있으면 찾기에만 답하고, 아니면 저장된 설정대로.
pub async fn init(app: &App, main: SocketAddr) {
    let link = app.link.clone();
    if main.ip().is_unspecified() {
        *link.always.lock() = Some(main.port());
        respond(link.clone(), main.port(), !app.password.is_empty(), Arc::new(AtomicBool::new(false)));
    } else if link.conf.lock().lan
        && let Err(e) = start_lan(app).await
    {
        tracing::warn!("같은 와이파이에 열지 못했습니다: {e}");
    }
}

// ---------------------------------------------------------------------------
// API

pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/hello", get(hello))
        .route("/api/link", get(info))
        .route("/api/link/lan", post(set_lan))
        .route("/api/link/pin", post(renew_pin))
        .route("/api/link/name", post(rename))
        .route("/api/link/discover", get(find))
        .route("/api/link/saved", post(save).delete(forget))
}

fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, Json(json!({ "error": "이 설정은 그 기기에서만 바꿀 수 있습니다" }))).into_response()
}

/// 다른 기기의 화면이 "이게 소리인가"를 묻는다. 로그인 없이, 어느 출처에서나.
async fn hello(State(app): State<App>, via: Option<Extension<ViaLan>>) -> Response {
    // LAN 쪽이 PIN으로 잠겨 있으면 로그인 화면이 "PIN"이라고 말한다.
    let pin = via.is_some() && !app.password.is_empty() && *app.password == app.link.conf.lock().pin;
    let mut res = Json(json!({
        "sori": 1,
        "id": app.link.id,
        "name": app.link.name(),
        "version": sori_core::VERSION,
        "locked": !app.password.is_empty(),
        "pin": pin,
    }))
    .into_response();
    res.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    res
}

fn info_json(app: &App, local: bool) -> serde_json::Value {
    let link = &app.link;
    let port = link.port();
    let urls: Vec<String> = port.map(|p| lan_addrs().into_iter().map(|(ip, _)| format!("http://{ip}:{p}")).collect()).unwrap_or_default();
    let mode = link.mode();
    json!({
        "id": link.id,
        "name": link.name(),
        "mode": mode,
        "port": port,
        "urls": urls,
        // PIN은 이 기기에서만 보인다(비밀번호를 정해 두었으면 PIN 대신 그것).
        "pin": (local && mode == Mode::On && app.password.is_empty()).then(|| link.conf.lock().pin.clone()),
        "password": !app.password.is_empty(),
        "local": local,
        "saved": if local { link.saved() } else { Vec::new() },
    })
}

async fn info(State(app): State<App>, via: Option<Extension<ViaLan>>) -> Json<serde_json::Value> {
    Json(info_json(&app, via.is_none()))
}

#[derive(Deserialize)]
struct LanBody {
    enabled: bool,
}

async fn set_lan(State(app): State<App>, via: Option<Extension<ViaLan>>, Json(b): Json<LanBody>) -> Response {
    if via.is_some() {
        return forbidden();
    }
    if app.link.mode() == Mode::Always {
        return (StatusCode::CONFLICT, Json(json!({ "error": "이 서버는 처음부터 네트워크에 열려 있습니다" }))).into_response();
    }
    if b.enabled {
        if let Err(e) = start_lan(&app).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response();
        }
    } else {
        stop_lan(&app);
    }
    Json(info_json(&app, true)).into_response()
}

/// PIN을 새로 만든다. 열려 있으면 새 PIN으로 다시 연다(다른 기기는 다시 들어와야 한다).
async fn renew_pin(State(app): State<App>, via: Option<Extension<ViaLan>>) -> Response {
    if via.is_some() {
        return forbidden();
    }
    app.link.conf.lock().pin = new_pin();
    app.link.save();
    if app.link.mode() == Mode::On {
        stop_lan(&app);
        if let Err(e) = start_lan(&app).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response();
        }
    }
    Json(info_json(&app, true)).into_response()
}

#[derive(Deserialize)]
struct NameBody {
    name: String,
}

/// 다른 기기에서 찾을 때 보이는 이름(예: 거실 PC). 비우면 기기 이름으로 돌아간다.
async fn rename(State(app): State<App>, via: Option<Extension<ViaLan>>, Json(b): Json<NameBody>) -> Response {
    if via.is_some() {
        return forbidden();
    }
    let name: String = b.name.trim().chars().filter(|c| !c.is_control()).take(40).collect();
    app.link.conf.lock().name = (!name.is_empty()).then_some(name);
    app.link.save();
    Json(info_json(&app, true)).into_response()
}

async fn find(State(app): State<App>) -> Json<Vec<Found>> {
    let id = app.link.id.clone();
    Json(tokio::task::spawn_blocking(move || discover(&id, Duration::from_millis(1200))).await.unwrap_or_default())
}

#[derive(Deserialize)]
struct SaveBody {
    name: Option<String>,
    url: String,
}

async fn save(State(app): State<App>, via: Option<Extension<ViaLan>>, Json(b): Json<SaveBody>) -> Response {
    if via.is_some() {
        return forbidden();
    }
    let Some(url) = normalize_url(&b.url) else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "주소가 이상합니다" }))).into_response();
    };
    let name = b.name.map(|n| n.trim().chars().take(80).collect::<String>()).filter(|n| !n.is_empty()).unwrap_or_else(|| url.clone());
    {
        let mut c = app.link.conf.lock();
        c.saved.retain(|s| s.url != url);
        c.saved.insert(0, Saved { name, url });
        c.saved.truncate(20);
    }
    app.link.save();
    Json(info_json(&app, true)).into_response()
}

#[derive(Deserialize)]
struct ForgetQuery {
    url: String,
}

async fn forget(State(app): State<App>, via: Option<Extension<ViaLan>>, Query(q): Query<ForgetQuery>) -> Response {
    if via.is_some() {
        return forbidden();
    }
    app.link.conf.lock().saved.retain(|s| s.url != q.url);
    app.link.save();
    Json(info_json(&app, true)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_people_type() {
        assert_eq!(normalize_url("192.168.0.5:4390").as_deref(), Some("http://192.168.0.5:4390"));
        assert_eq!(normalize_url(" https://music.example.com/ ").as_deref(), Some("https://music.example.com"));
        assert_eq!(normalize_url("http://a b"), None);
        assert_eq!(normalize_url("http://user@evil"), None);
        assert_eq!(normalize_url(""), None);
    }

    #[test]
    fn pins_are_six_digits() {
        for _ in 0..100 {
            let p = new_pin();
            assert_eq!(p.len(), 6);
            assert!(p.bytes().all(|b| b.is_ascii_digit()));
        }
    }
}
