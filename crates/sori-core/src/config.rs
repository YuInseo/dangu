//! 실행할 때 정해지는 것들. 앱(Tauri)은 직접 채워 넣고, 서버는 환경변수에서 읽는다.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    /// 음악이 있는 폴더. 읽기만 하고, 라이브 아카이브 공연을 저장할 때만 그 아래 `Live Archive`에 쓴다.
    /// 설정 화면에서 바꾸면 `settings.json`의 값이 이긴다.
    pub music_dir: PathBuf,
    /// 색인, 표지 캐시, 분석 결과, 설정을 두는 곳.
    pub data_dir: PathBuf,
    /// 비어 있으면 로그인 없이 열린다. 집 밖으로 내놓을 거면 꼭 넣는다.
    pub password: String,
}

impl Config {
    pub fn new(music_dir: impl Into<PathBuf>, data_dir: impl Into<PathBuf>) -> Self {
        Config { music_dir: music_dir.into(), data_dir: data_dir.into(), password: String::new() }
    }

    /// `SORI_MUSIC_DIR`(또는 `MUSIC_DIR`), `SORI_DATA_DIR`(또는 `DATA_DIR`), `SORI_PASSWORD`.
    pub fn from_env() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let pick = |a: &str, b: &str, fallback: &str| -> PathBuf {
            let v = std::env::var(a).or_else(|_| std::env::var(b)).unwrap_or_default();
            let p = if v.is_empty() { PathBuf::from(fallback) } else { PathBuf::from(v) };
            if p.is_absolute() { p } else { cwd.join(p) }
        };
        Config {
            music_dir: pick("SORI_MUSIC_DIR", "MUSIC_DIR", "music"),
            data_dir: pick("SORI_DATA_DIR", "DATA_DIR", ".sori"),
            password: std::env::var("SORI_PASSWORD").unwrap_or_default(),
        }
    }

    pub fn data_file(&self, name: &str) -> PathBuf {
        self.data_dir.join(name)
    }
}

/// 쓰다 꺼져도 반쪽짜리 파일이 남지 않게 임시 파일에 쓰고 바꿔 끼운다.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
