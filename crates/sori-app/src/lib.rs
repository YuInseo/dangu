//! 소리 앱.
//!
//! 앱 안에서 소리 서버를 띄우고, 창은 그 서버의 화면을 연다. 같은 와이파이의 다른 기기도
//! 같은 화면을 쓸 수 있는 구조이고, 화면 코드는 브라우저 판과 하나다.
//!
//! 주소는 매번 같은 포트(127.0.0.1:4380)를 쓴다. 화면의 설정(DSP, 대기열, 취향)은
//! 주소마다 따로 저장되므로, 포트가 바뀌면 켤 때마다 설정이 사라진 것처럼 보인다.

use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use sori_core::{Config, Store};
use sori_server::{App, Host};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
#[cfg(desktop)]
use tauri_plugin_dialog::DialogExt;

const PORTS: std::ops::RangeInclusive<u16> = 4380..=4389;

/// 비어 있는 포트를 고른다. 이미 소리가 떠 있으면 그 포트는 건너뛴다.
fn pick_port() -> u16 {
    for p in PORTS {
        if TcpListener::bind(("127.0.0.1", p)).is_ok() {
            return p;
        }
    }
    0
}

fn default_music_dir(app: &tauri::App) -> PathBuf {
    #[cfg(target_os = "android")]
    {
        let _ = app;
        PathBuf::from("/storage/emulated/0/Music")
    }
    #[cfg(not(target_os = "android"))]
    {
        app.path().audio_dir().unwrap_or_else(|_| app.path().home_dir().map(|h| h.join("Music")).unwrap_or_else(|_| PathBuf::from(".")))
    }
}

/// 서버가 앱에 부탁하는 일(폴더 고르기 창 같은 것).
struct AppHost {
    handle: tauri::AppHandle,
}

impl Host for AppHost {
    fn pick_folder(&self, start: Option<PathBuf>) -> Option<PathBuf> {
        #[cfg(desktop)]
        {
            let mut d = self.handle.dialog().file().set_title("음악 폴더 고르기");
            if let Some(s) = start {
                d = d.set_directory(s);
            }
            d.blocking_pick_folder().and_then(|f| f.into_path().ok())
        }
        #[cfg(mobile)]
        {
            let _ = (start, &self.handle);
            None
        }
    }

    /// 받아 둔 새 버전을 설치한다.
    fn install_update(&self, file: &Path) -> anyhow::Result<()> {
        #[cfg(target_os = "windows")]
        {
            // 설치 파일을 진행 막대만 보이는 모드(/P)로 돌리고, 끝나면 앱을 다시 켜게 한다(/R).
            // 설치 파일이 앱을 바꿔 끼울 수 있게 앱은 곧 닫는다.
            let args = ["/P", "/UPDATE", "/R"];
            std::process::Command::new(file)
                .args(args)
                .spawn()
                .or_else(|_| std::process::Command::new("cmd").args(["/C", "start", ""]).arg(file).args(args).spawn())?;
            let handle = self.handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(800));
                handle.exit(0);
            });
            Ok(())
        }
        #[cfg(target_os = "android")]
        {
            // 시스템 설치 창을 띄운다(UpdatePlugin.kt). 설치는 사용자가 누른다.
            let plugin = self.handle.try_state::<UpdatePlugin>().ok_or_else(|| anyhow::anyhow!("설치 창을 띄울 곳이 없습니다"))?;
            plugin
                .0
                .run_mobile_plugin::<serde_json::Value>("install", serde_json::json!({ "path": file.to_string_lossy() }))
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(())
        }
        #[cfg(not(any(target_os = "windows", target_os = "android")))]
        {
            let _ = file;
            anyhow::bail!("이 기기에서는 업데이트를 설치할 수 없습니다")
        }
    }

    fn open_url(&self, url: &str) -> anyhow::Result<()> {
        #[cfg(target_os = "windows")]
        {
            // cmd를 거치지 않는다(주소의 &를 cmd가 명령으로 읽는다).
            std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn()?;
            Ok(())
        }
        #[cfg(target_os = "android")]
        {
            let plugin = self.handle.try_state::<UpdatePlugin>().ok_or_else(|| anyhow::anyhow!("열 곳이 없습니다"))?;
            plugin
                .0
                .run_mobile_plugin::<serde_json::Value>("open", serde_json::json!({ "url": url }))
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(())
        }
        #[cfg(not(any(target_os = "windows", target_os = "android")))]
        {
            std::process::Command::new("xdg-open").arg(url).spawn()?;
            Ok(())
        }
    }

    fn name(&self) -> &'static str {
        if cfg!(target_os = "android") {
            "android"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            "desktop"
        }
    }
}

/// 안드로이드: 받아 둔 APK의 설치 창을 띄우는 플러그인(UpdatePlugin.kt)
#[cfg(target_os = "android")]
struct UpdatePlugin(tauri::plugin::PluginHandle<tauri::Wry>);

#[cfg(target_os = "android")]
fn update_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("sori-update")
        .setup(|app, api| {
            let handle = api.register_android_plugin("app.sori.player", "UpdatePlugin")?;
            app.manage(UpdatePlugin(handle));
            Ok(())
        })
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 기록은 표준 출력과, 설정(정보) 화면이 보여 주는 최근 기록에 남는다(휴대폰은 표준 출력이 없다).
    use tracing_subscriber::prelude::*;
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,symphonia=error,symphonia_core=error,symphonia_bundle_mp3=error".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(sori_server::logs::layer())
        .try_init();

    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    #[cfg(target_os = "android")]
    let builder = builder.plugin(update_plugin());
    builder
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let music_dir = default_music_dir(app);
            tracing::info!(music = %music_dir.display(), data = %data_dir.display(), "소리를 엽니다");
            let store = Store::open(Config::new(music_dir, data_dir.clone()));
            store.start_scan();
            keep_trying(store.clone());

            let host: Arc<dyn Host> = Arc::new(AppHost { handle: app.handle().clone() });
            let port = pick_port();
            let (tx, rx) = std::sync::mpsc::channel::<SocketAddr>();
            let server_store = store.clone();
            std::thread::Builder::new().name("sori-server".into()).spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().expect("tokio");
                rt.block_on(async move {
                    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
                    tokio::spawn(async move {
                        if let Ok(a) = ready_rx.await {
                            let _ = tx.send(a);
                        }
                    });
                    let mut app = App::new(server_store, String::new()).with_device();
                    app.host = Some(host);
                    if let Err(e) = sori_server::serve(app, SocketAddr::from(([127, 0, 0, 1], port)), Some(ready_tx)).await {
                        tracing::error!("서버가 멈췄습니다: {e}");
                    }
                });
            })?;
            let addr = rx.recv_timeout(Duration::from_secs(15)).map_err(|_| anyhow::anyhow!("서버가 뜨지 않습니다"))?;
            // 안드로이드의 재생 서비스(잠금 화면·알림)가 엔진을 찾는 곳
            let _ = std::fs::write(data_dir.join("server.port"), addr.port().to_string());
            let url = format!("http://127.0.0.1:{}/", addr.port()).parse()?;

            let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url));
            #[cfg(desktop)]
            let builder = builder
                .title("Sori")
                .inner_size(1320.0, 860.0)
                .min_inner_size(380.0, 560.0)
                .background_color(tauri::window::Color(14, 14, 16, 255))
                .theme(Some(tauri::Theme::Dark));
            let _win = builder.build()?;
            // 화면이 창보다 작으면(노트북, 작은 모니터) 작업 영역에 맞춰 줄이고 가운데에 둔다.
            #[cfg(desktop)]
            if let Ok(Some(m)) = _win.current_monitor() {
                let scale = m.scale_factor();
                let area = m.work_area();
                let (w, h) = (area.size.width as f64 / scale, area.size.height as f64 / scale);
                if w < 1320.0 || h < 860.0 {
                    let _ = _win.set_size(tauri::LogicalSize::new((w * 0.94).min(1320.0), (h * 0.94).min(860.0)));
                    let _ = _win.center();
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("소리 앱을 시작하지 못했습니다");
}

/// 안드로이드에서는 처음 켤 때 음악 접근 권한을 묻는다. 허락하기 전에 한 스캔은 빈손으로
/// 끝나므로, 라이브러리가 비어 있는 동안은 몇 초마다 다시 본다.
fn keep_trying(store: Arc<Store>) {
    std::thread::spawn(move || {
        for _ in 0..60 {
            std::thread::sleep(Duration::from_secs(4));
            if store.library().summary.tracks > 0 {
                return;
            }
            if store.scan_status().state != sori_core::model::ScanState::Scanning {
                store.start_scan();
            }
        }
    });
}
