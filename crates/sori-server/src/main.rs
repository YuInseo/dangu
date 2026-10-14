//! `sori` — 음악 폴더를 읽고 화면과 스트리밍을 내준다.
//!
//!   sori [--music 폴더] [--data 폴더] [--host 0.0.0.0] [--port 4380]
//!
//! 환경변수로도 줄 수 있다: SORI_MUSIC_DIR, SORI_DATA_DIR, SORI_HOST, SORI_PORT, SORI_PASSWORD.

use std::net::{IpAddr, SocketAddr};

use sori_core::{Config, Store};
use sori_server::{App, serve};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=warn,symphonia=error,symphonia_core=error,symphonia_bundle_mp3=error".into()),
        )
        .init();
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("sori [--music 폴더] [--data 폴더] [--host 주소] [--port 번호]");
        return Ok(());
    }
    let mut config = Config::from_env();
    if let Some(m) = arg(&args, "--music") {
        config.music_dir = std::path::absolute(m)?;
    }
    if let Some(d) = arg(&args, "--data") {
        config.data_dir = std::path::absolute(d)?;
    }
    let host: IpAddr = arg(&args, "--host").or_else(|| std::env::var("SORI_HOST").ok()).unwrap_or_else(|| "0.0.0.0".into()).parse()?;
    let port: u16 = arg(&args, "--port").or_else(|| std::env::var("SORI_PORT").ok()).unwrap_or_else(|| "4380".into()).parse()?;
    let password = config.password.clone();
    tracing::info!(music = %config.music_dir.display(), data = %config.data_dir.display(), "라이브러리를 엽니다");
    let store = Store::open(config);
    store.start_scan();
    let mut app = App::new(store, password);
    // --device: 서버가 도는 기기의 오디오 장치로도 틀 수 있게(네이티브 존).
    #[cfg(feature = "device")]
    if args.iter().any(|a| a == "--device") {
        app = app.with_device();
    }
    // --null-device: 소리 없이 실시간으로 흘려보내는 가짜 장치(장치가 없는 곳에서 화면을 시험할 때).
    if args.iter().any(|a| a == "--null-device") {
        app = app.with_output(Box::new(|| Box::new(sori_core::player::output::NullOutput::new(48_000, true))));
    }
    serve(app, SocketAddr::new(host, port), None).await
}
