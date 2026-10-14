//! 소리(Sori)의 심장.
//!
//! 음악 폴더를 읽어 앨범·아티스트·장르로 묶고, 파일을 64비트 부동소수점으로 풀어
//! 룬의 MUSE처럼 다듬은 뒤 기기로 내보낸다. 서버(`sori-server`)와 앱(`sori-app`)은
//! 이 크레이트를 감싸는 얇은 껍데기다.

pub mod analysis;
pub mod collate;
pub mod config;
pub mod covers;
pub mod decode;
pub mod dsd;
pub mod dsp;
pub mod formats;
pub mod ids;
pub mod library;
pub mod model;
pub mod player;
pub mod quality;
pub mod scan;
pub mod store;

pub use config::Config;
pub use library::Library;
pub use store::Store;

/// 화면의 "정보" 칸에 찍히는 버전.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
