//! 소리 서버. 앱(Tauri)도 이걸 그대로 띄워서 화면을 연다.

pub mod api;
pub mod auth;
pub mod embed;
pub mod media;
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
}

pub fn router(app: App) -> Router {
    Router::new()
        .merge(api::routes())
        .merge(media::routes())
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
    if let Some(tx) = ready {
        let _ = tx.send(local);
    }
    axum::serve(listener, router(app)).with_graceful_shutdown(shutdown()).await?;
    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}
