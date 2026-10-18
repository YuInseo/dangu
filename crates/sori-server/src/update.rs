//! 자동 업데이트.
//!
//! CI가 빌드마다 릴리스(sori-latest)에 올리는 `latest.json`을 보고, 이 빌드보다 새것이면 뒤에서
//! 미리 받아 둔다(크기와 SHA-256을 맞춰 본다). 사용자가 누르면 앱(호스트)에게 설치를 맡긴다.
//! 윈도우는 설치 파일을 돌리고 앱을 닫으며(설치가 끝나면 다시 켜진다), 안드로이드는 시스템 설치
//! 창을 띄운다(안드로이드는 사용자가 한 번 눌러야 설치된다).
//!
//! 브라우저용 서버(호스트 없음)와 직접 빌드한 것(빌드 번호 없음)은 업데이트하지 않는다.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::App;

/// 이 빌드의 커밋(CI가 넣는다). 직접 빌드하면 없다.
pub const BUILD: Option<&str> = option_env!("SORI_BUILD");
/// 그 커밋의 시각(유닉스 초)
const BUILD_TIME: Option<&str> = option_env!("SORI_BUILD_TIME");
const LATEST_URL: &str = "https://github.com/YuInseo/dangu/releases/download/sori-latest/latest.json";
/// 처음 확인은 켜고 조금 뒤에(시작을 느리게 하지 않게), 그다음은 이만큼마다
const FIRST_CHECK: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(3600);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Latest {
    pub build: String,
    /// 커밋 시각(유닉스 초)
    #[serde(default)]
    pub time: u64,
    #[serde(default)]
    pub notes: String,
    pub android: Option<Asset>,
    pub windows: Option<Asset>,
}

impl Latest {
    pub fn asset(&self, platform: &str) -> Option<&Asset> {
        match platform {
            "android" => self.android.as_ref(),
            "windows" => self.windows.as_ref(),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Download {
    Idle,
    Downloading { done: u64, total: u64 },
    Ready,
    Installing,
    Failed { error: String },
}

struct Inner {
    checked_at: Option<i64>,
    latest: Option<Latest>,
    error: Option<String>,
    download: Download,
    /// 받아 둔 파일과 그 빌드
    file: Option<(PathBuf, String)>,
}

pub struct Updater {
    url: String,
    /// 이 빌드(없으면 업데이트하지 않는다)
    build: Option<String>,
    build_time: u64,
    dir: PathBuf,
    settings: PathBuf,
    state: Mutex<Inner>,
    busy: AtomicBool,
    auto: AtomicBool,
}

#[derive(Serialize, Deserialize)]
struct Settings {
    auto: bool,
}

/// `latest`가 이 빌드보다 새것인가. 같은 커밋이거나 더 옛것이면 아니다(거꾸로 가지 않는다).
pub fn is_newer(latest: &Latest, build: Option<&str>, build_time: u64) -> bool {
    let Some(build) = build else { return false };
    latest.build != build && latest.time > build_time
}

impl Updater {
    pub fn new(data_dir: &Path) -> Updater {
        Updater::with(
            data_dir,
            std::env::var("SORI_UPDATE_URL").unwrap_or_else(|_| LATEST_URL.into()),
            BUILD.map(String::from),
            BUILD_TIME.and_then(|t| t.parse().ok()).unwrap_or(0),
        )
    }

    pub fn with(data_dir: &Path, url: String, build: Option<String>, build_time: u64) -> Updater {
        // 안드로이드: 설치 창이 읽을 수 있게 앱의 cache/update 아래에 둔다(FileProvider가 이곳을 연다).
        let dir = if cfg!(target_os = "android") { data_dir.join("cache").join("update") } else { data_dir.join("update") };
        let settings = data_dir.join("update.json");
        let auto = std::fs::read(&settings).ok().and_then(|b| serde_json::from_slice::<Settings>(&b).ok()).is_none_or(|s| s.auto);
        Updater {
            url,
            build,
            build_time,
            dir,
            settings,
            state: Mutex::new(Inner { checked_at: None, latest: None, error: None, download: Download::Idle, file: None }),
            busy: AtomicBool::new(false),
            auto: AtomicBool::new(auto),
        }
    }

    pub fn auto(&self) -> bool {
        self.auto.load(Ordering::Relaxed)
    }

    pub fn set_auto(&self, on: bool) {
        self.auto.store(on, Ordering::Relaxed);
        if let Ok(b) = serde_json::to_vec(&Settings { auto: on }) {
            let _ = sori_core::config::write_atomic(&self.settings, &b);
        }
    }

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(20)))
            .timeout_recv_body(Some(Duration::from_secs(20 * 60)))
            .user_agent(sori_core::live::user_agent())
            .build()
            .into()
    }

    /// latest.json을 받는다.
    pub fn check(&self) -> anyhow::Result<Option<Latest>> {
        let res = (|| -> anyhow::Result<Latest> {
            let mut r = Self::agent().get(&self.url).call()?;
            let text = r.body_mut().with_config().limit(256 * 1024).read_to_string()?;
            Ok(serde_json::from_str(&text)?)
        })();
        let mut s = self.state.lock();
        s.checked_at = Some(sori_core::model::now_ms());
        match res {
            Ok(l) => {
                s.error = None;
                s.latest = Some(l.clone());
                Ok(Some(l))
            }
            Err(e) => {
                s.error = Some(format!("확인하지 못했습니다: {e}"));
                Err(e)
            }
        }
    }

    /// 이 기기에 맞는 새 빌드가 있으면 그 파일 정보
    pub fn available(&self, platform: &str) -> Option<(Latest, Asset)> {
        let s = self.state.lock();
        let l = s.latest.as_ref()?;
        if !is_newer(l, self.build.as_deref(), self.build_time) {
            return None;
        }
        Some((l.clone(), l.asset(platform)?.clone()))
    }

    /// 받아 두고(이미 받았으면 그대로) 파일 위치를 돌려준다. 크기와 SHA-256이 맞아야 한다.
    pub fn download(&self, latest: &Latest, asset: &Asset) -> anyhow::Result<PathBuf> {
        if let Some((p, b)) = &self.state.lock().file
            && *b == latest.build
            && p.is_file()
        {
            return Ok(p.clone());
        }
        let name = asset.url.rsplit('/').next().filter(|n| !n.is_empty() && !n.contains("..")).unwrap_or("update.bin");
        std::fs::create_dir_all(&self.dir)?;
        // 옛 파일은 지운다(휴대폰 공간).
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
        let path = self.dir.join(name);
        let part = self.dir.join(format!("{name}.part"));
        self.state.lock().download = Download::Downloading { done: 0, total: asset.size };
        let res = (|| -> anyhow::Result<()> {
            let mut r = Self::agent().get(&asset.url).call()?;
            let mut body = r.body_mut().with_config().limit(asset.size + 1).reader();
            let mut out = std::fs::File::create(&part)?;
            let mut hash = Sha256::new();
            let mut buf = vec![0u8; 256 * 1024];
            let mut done = 0u64;
            loop {
                let n = body.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n])?;
                hash.update(&buf[..n]);
                done += n as u64;
                self.state.lock().download = Download::Downloading { done, total: asset.size };
            }
            out.sync_all()?;
            if done != asset.size {
                anyhow::bail!("받은 크기가 다릅니다({done} / {})", asset.size);
            }
            let got = hex(&hash.finalize());
            if !got.eq_ignore_ascii_case(&asset.sha256) {
                anyhow::bail!("파일이 손상됐습니다(SHA-256이 다릅니다)");
            }
            std::fs::rename(&part, &path)?;
            Ok(())
        })();
        let mut s = self.state.lock();
        match res {
            Ok(()) => {
                s.download = Download::Ready;
                s.file = Some((path.clone(), latest.build.clone()));
                Ok(path)
            }
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                s.download = Download::Failed { error: e.to_string() };
                Err(e)
            }
        }
    }

    pub fn set_download(&self, d: Download) {
        self.state.lock().download = d;
    }

    pub fn json(&self, platform: Option<&str>) -> Value {
        let s = self.state.lock();
        let newer = s.latest.as_ref().is_some_and(|l| is_newer(l, self.build.as_deref(), self.build_time));
        let fits = platform.zip(s.latest.as_ref()).is_some_and(|(p, l)| l.asset(p).is_some());
        json!({
            "build": self.build,
            "buildTime": self.build_time,
            // 이 기기에서 업데이트할 수 있는가(앱 안이고, CI가 만든 빌드)
            "supported": platform.is_some() && self.build.is_some(),
            "auto": self.auto(),
            "checkedAt": s.checked_at,
            "error": s.error,
            "available": newer && fits,
            "latest": s.latest.as_ref().map(|l| json!({
                "build": l.build,
                "time": l.time,
                "notes": l.notes,
                "size": platform.and_then(|p| l.asset(p)).map(|a| a.size),
            })),
            "download": s.download,
        })
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn platform(app: &App) -> Option<&'static str> {
    app.host.as_ref().map(|h| h.name()).filter(|n| matches!(*n, "android" | "windows"))
}

/// 앱 안에서만: 켜고 조금 뒤, 그다음은 한 시간마다 확인하고, 자동이면 미리 받아 둔다(화면은 앱이 앞으로 올 때도 묻는다).
pub fn start(app: &App) {
    let Some(p) = platform(app) else { return };
    if app.updater.build.is_none() {
        return;
    }
    let updater = app.updater.clone();
    std::thread::Builder::new()
        .name("sori-update".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK);
            loop {
                if updater.check().is_ok()
                    && updater.auto()
                    && let Some((l, a)) = updater.available(p)
                    && !updater.busy.swap(true, Ordering::AcqRel)
                {
                    match updater.download(&l, &a) {
                        Ok(_) => tracing::info!(build = %l.build, "새 버전을 받아 두었습니다"),
                        Err(e) => tracing::warn!("새 버전을 받지 못했습니다: {e}"),
                    }
                    updater.busy.store(false, Ordering::Release);
                }
                std::thread::sleep(EVERY);
            }
        })
        .ok();
}

async fn status(State(app): State<App>) -> Json<Value> {
    Json(app.updater.json(platform(&app)))
}

/// 지금 확인한다.
async fn check(State(app): State<App>) -> Response {
    let u = app.updater.clone();
    let res = tokio::task::spawn_blocking(move || u.check()).await;
    match res {
        Ok(Ok(_)) => Json(app.updater.json(platform(&app))).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("확인하지 못했습니다: {e}") }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

#[derive(Deserialize)]
struct AutoBody {
    on: bool,
}

async fn set_auto(State(app): State<App>, Json(b): Json<AutoBody>) -> Json<Value> {
    app.updater.set_auto(b.on);
    Json(app.updater.json(platform(&app)))
}

/// 받아(아직이면) 설치한다. 오래 걸리므로 뒤에서 하고 바로 돌아간다. 화면은 상태를 다시 묻는다.
async fn install(State(app): State<App>) -> Response {
    let Some(p) = platform(&app) else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "이 기기에서는 업데이트를 설치할 수 없습니다" }))).into_response();
    };
    let Some((latest, asset)) = app.updater.available(p) else {
        return (StatusCode::CONFLICT, Json(json!({ "error": "새 버전이 없습니다" }))).into_response();
    };
    let u = app.updater.clone();
    if u.busy.swap(true, Ordering::AcqRel) {
        return Json(u.json(Some(p))).into_response();
    }
    let host = app.host.clone();
    std::thread::spawn(move || {
        let res = u.download(&latest, &asset).and_then(|file| {
            u.set_download(Download::Installing);
            match &host {
                Some(h) => h.install_update(&file),
                None => anyhow::bail!("설치를 맡을 앱이 없습니다"),
            }
        });
        match res {
            // 안드로이드: 설치 창이 떴다. 사용자가 끝내면 새 앱이 켜진다. 윈도우는 곧 닫힌다.
            Ok(()) => u.set_download(Download::Ready),
            Err(e) => {
                tracing::warn!("업데이트를 설치하지 못했습니다: {e:#}");
                u.set_download(Download::Failed { error: e.to_string() });
            }
        }
        u.busy.store(false, Ordering::Release);
    });
    Json(app.updater.json(Some(p))).into_response()
}

pub fn routes() -> Router<App> {
    Router::new()
        .route("/api/update", get(status))
        .route("/api/update/check", post(check))
        .route("/api/update/auto", post(set_auto))
        .route("/api/update/install", post(install))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latest(build: &str, time: u64) -> Latest {
        Latest {
            build: build.into(),
            time,
            notes: String::new(),
            android: Some(Asset { url: "https://x/Sori-android.apk".into(), size: 3, sha256: String::new() }),
            windows: None,
        }
    }

    #[test]
    fn only_newer_builds_are_offered() {
        assert!(is_newer(&latest("b", 200), Some("a"), 100));
        assert!(!is_newer(&latest("a", 200), Some("a"), 100), "같은 커밋");
        assert!(!is_newer(&latest("b", 50), Some("a"), 100), "더 옛것으로 가지 않는다");
        assert!(!is_newer(&latest("b", 200), None, 0), "직접 빌드한 것은 업데이트하지 않는다");
        assert!(latest("b", 1).asset("android").is_some());
        assert!(latest("b", 1).asset("windows").is_none());
        assert!(latest("b", 1).asset("linux").is_none());
    }

    /// 작은 HTTP 서버: 경로마다 정해진 바이트를 돌려준다.
    fn serve(files: Vec<(&'static str, Vec<u8>)>) -> String {
        use std::io::{BufRead, BufReader};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                }
                let mut s = s;
                match files.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => {
                        let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                        let _ = s.write_all(body);
                    }
                    None => {
                        let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn checks_downloads_and_verifies() {
        let apk = b"PK\x03\x04 not really an apk".to_vec();
        let good = hex(&Sha256::digest(&apk));
        let files = serve(vec![("/Sori-android.apk", apk.clone())]);
        let latest = |sha: &str| {
            serde_json::to_vec(&json!({
                "build": "new", "time": 200, "notes": "고침",
                "android": { "url": format!("{files}/Sori-android.apk"), "size": apk.len(), "sha256": sha },
            }))
            .unwrap()
        };

        let dir = tempfile::tempdir().unwrap();
        let srv = serve(vec![("/latest.json", latest(&good))]);
        let u = Updater::with(dir.path(), format!("{srv}/latest.json"), Some("old".into()), 100);
        assert_eq!(u.check().unwrap().unwrap().build, "new");
        let (l, a) = u.available("android").expect("새 빌드가 있다");
        assert!(u.available("windows").is_none(), "윈도우 파일은 없다");
        let file = u.download(&l, &a).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), apk);
        let st = u.json(Some("android"));
        assert_eq!((st["download"]["state"].as_str(), st["available"].as_bool()), (Some("ready"), Some(true)));

        // SHA-256이 다르면 받지 않고, 받다 만 것은 지운다.
        let dir = tempfile::tempdir().unwrap();
        let srv = serve(vec![("/latest.json", latest(&"00".repeat(32)))]);
        let u = Updater::with(dir.path(), format!("{srv}/latest.json"), Some("old".into()), 100);
        u.check().unwrap();
        let (l, a) = u.available("android").unwrap();
        let err = u.download(&l, &a).unwrap_err().to_string();
        assert!(err.contains("SHA-256"), "{err}");
        assert_eq!(u.json(Some("android"))["download"]["state"], "failed");
        assert_eq!(std::fs::read_dir(dir.path().join("update")).unwrap().count(), 0);
    }

    #[test]
    fn auto_setting_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let u = Updater::with(dir.path(), "http://127.0.0.1:9/latest.json".into(), Some("a".into()), 1);
        assert!(u.auto(), "처음에는 자동");
        u.set_auto(false);
        assert!(!Updater::with(dir.path(), String::new(), None, 0).auto());
    }
}
