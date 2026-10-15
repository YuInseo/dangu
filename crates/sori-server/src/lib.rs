//! 소리 서버. 앱(Tauri)도 이걸 그대로 띄워서 화면을 연다.

pub mod api;
pub mod auth;
pub mod dsp;
pub mod embed;
pub mod link;
pub mod live;
pub mod media;
pub mod player;
pub mod wav;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use sori_core::Store;
use tower_http::compression::CompressionLayer;

/// 서버를 품은 쪽(앱)이 해 줄 수 있는 일. 브라우저용 서버에는 없다.
pub trait Host: Send + Sync {
    /// 폴더 고르기 창을 띄운다. 고르지 않으면 None.
    fn pick_folder(&self, start: Option<PathBuf>) -> Option<PathBuf>;
    /// "windows", "android" …
    fn name(&self) -> &'static str;
}

#[derive(Clone)]
pub struct App {
    pub store: Arc<Store>,
    /// 비어 있으면 잠그지 않는다.
    pub password: Arc<String>,
    pub host: Option<Arc<dyn Host>>,
    /// 이 기기의 오디오 장치로 트는 존. 없으면 브라우저 존만 있다.
    pub player: Option<Arc<sori_core::player::Player>>,
    pub irs: Arc<sori_core::dsp::ir::IrStore>,
    pub clips: Arc<sori_core::dsp::chain::Clips>,
    /// 라이브 아카이브(archive.org)
    pub live: Arc<sori_core::live::Archive>,
    /// 다른 기기와 잇기(선택): LAN 공유, 찾기, 기억한 주소
    pub link: Arc<link::Link>,
}

impl App {
    pub fn new(store: Arc<Store>, password: String) -> App {
        let irs = Arc::new(sori_core::dsp::ir::IrStore::new(&store.config.data_dir));
        let link = Arc::new(link::Link::open(&store.config.data_dir));
        App {
            store,
            password: Arc::new(password),
            host: None,
            player: None,
            irs,
            clips: Arc::new(Default::default()),
            live: Arc::new(sori_core::live::Archive::from_env()),
            link,
        }
    }

    /// 이 기기의 기본 오디오 장치로 트는 존을 붙인다.
    #[cfg(feature = "device")]
    pub fn with_device(self) -> App {
        use sori_core::player::device::DeviceOutput;
        self.with_output(Box::new(|| Box::new(DeviceOutput::new(None))))
    }

    /// 출력을 골라 네이티브 존을 붙인다(시험할 때는 소리 없는 가짜 출력).
    pub fn with_output(mut self, factory: sori_core::player::OutputFactory) -> App {
        use sori_core::player::{Player, resolver};
        let player = Player::with_dsp(factory, resolver(self.store.clone(), self.live.clone()), self.irs.clone(), self.clips.clone());
        self.player = Some(Arc::new(player));
        self
    }
}

pub fn router(app: App) -> Router {
    Router::new()
        .merge(api::routes())
        .merge(media::routes())
        .merge(player::routes())
        .merge(dsp::routes())
        .merge(live::routes())
        .merge(link::routes())
        .merge(auth::routes())
        .fallback(embed::serve)
        // 오디오는 이미 압축돼 있거나(FLAC, MP3) 받는 쪽이 길이를 알아야 하므로(Range) 압축하지 않는다.
        .layer(CompressionLayer::new().compress_when(media::compressible()))
        .layer(axum::middleware::from_fn_with_state(app.clone(), auth::gate))
        .with_state(app)
}

/// 주소에 묶고 끝날 때까지 돈다. 포트를 0으로 주면 남는 포트를 고르고, 실제 주소를 `ready`로 알린다.
pub async fn serve(app: App, addr: SocketAddr, ready: Option<tokio::sync::oneshot::Sender<SocketAddr>>) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;
    tracing::info!("소리가 http://{local} 에서 기다립니다");
    // 같은 와이파이에 열기(선택): 서버 실행 파일은 처음부터 열려 있고, 앱은 설정대로.
    link::init(&app, local).await;
    if let Some(tx) = ready {
        let _ = tx.send(local);
    }
    axum::serve(listener, router(app)).with_graceful_shutdown(shutdown()).await?;
    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}
