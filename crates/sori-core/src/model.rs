//! 라이브러리의 모양. 화면이 받는 JSON과 필드 이름이 같다(camelCase).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QualityTier {
    Hires,
    Lossless,
    Lossy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioFormat {
    /// 사람이 읽는 코덱 이름: FLAC, ALAC, WAV, AIFF, MP3, AAC, Vorbis, Opus, DSD …
    pub codec: String,
    /// 파일 확장자(소문자).
    pub container: String,
    pub lossless: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<u32>,
    /// 초당 비트 수.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<u32>,
    /// 브라우저 `canPlayType`에 넘길 MIME.
    pub mime: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverRef {
    /// 표지가 바뀌면 같이 바뀌는 값. URL에 붙여 캐시를 깬다.
    pub v: String,
    /// 표지의 대표 색.
    pub color: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoudnessValue {
    /// LUFS
    pub lufs: f64,
    /// dBTP(분석) 또는 dBFS(태그)
    pub peak: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoudnessSource {
    Analysis,
    Replaygain,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Loudness {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<LoudnessValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<LoudnessValue>,
    pub source: LoudnessSource,
}

/// ReplayGain 태그. 분석 전까지 볼륨 레벨링이 이걸 쓴다.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReplayGain {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tp: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ag: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ap: Option<f64>,
}

impl ReplayGain {
    pub fn is_empty(&self) -> bool {
        self.tg.is_none() && self.tp.is_none() && self.ag.is_none() && self.ap.is_none()
    }
}

/// 디스크에 저장되는 곡. 다시 스캔할 때 바뀌지 않은 파일을 건너뛰는 근거(mtime, size)가 같이 있다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTrack {
    pub id: String,
    /// 음악 폴더 기준 상대 경로. 항상 `/`로 구분한다.
    pub path: String,
    pub mtime: i64,
    pub size: u64,
    pub title: String,
    pub artist: String,
    pub artists: Vec<String>,
    pub album: String,
    pub album_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album_artist_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_no: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disc_no: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    pub genres: Vec<String>,
    /// 초
    pub duration: f64,
    pub format: AudioFormat,
    pub added_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rg: Option<ReplayGain>,
}

/// 화면으로 가는 곡. 파일 경로처럼 서버만 알아야 하는 것은 빠진다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artist_id: String,
    pub artists: Vec<String>,
    pub album: String,
    pub album_id: String,
    pub album_artist: String,
    pub album_artist_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_no: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disc_no: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    pub genres: Vec<String>,
    pub duration: f64,
    pub format: AudioFormat,
    pub added_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<CoverRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loudness: Option<Loudness>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artist_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    pub genres: Vec<String>,
    pub track_ids: Vec<String>,
    pub disc_count: u32,
    pub duration: f64,
    pub added_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<CoverRef>,
    /// 앨범에서 가장 좋은 곡의 포맷. 카드와 머리말의 배지가 이걸 쓴다.
    pub format: AudioFormat,
}

/// 그리드 카드 하나에 필요한 만큼만.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumCard {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artist_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    pub added_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<CoverRef>,
    pub format: AudioFormat,
}

impl From<&Album> for AlbumCard {
    fn from(a: &Album) -> Self {
        AlbumCard {
            id: a.id.clone(),
            title: a.title.clone(),
            artist: a.artist.clone(),
            artist_id: a.artist_id.clone(),
            year: a.year,
            added_at: a.added_at,
            cover: a.cover.clone(),
            format: a.format.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: String,
    pub name: String,
    /// 이 사람이 앨범 아티스트인 앨범.
    pub album_ids: Vec<String>,
    /// 남의 앨범에 참여한 곡.
    pub appears_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_album_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<CoverRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenreCover {
    pub album_id: String,
    pub cover: CoverRef,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Genre {
    pub id: String,
    pub name: String,
    pub album_ids: Vec<String>,
    pub covers: Vec<GenreCover>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySummary {
    pub albums: usize,
    pub artists: usize,
    pub tracks: usize,
    pub duration: f64,
    pub bytes: u64,
    pub hires: usize,
    pub lossless: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scanned_at: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanState {
    Idle,
    Scanning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanPhase {
    Listing,
    Reading,
    Covers,
    Saving,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub state: ScanState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<ScanPhase>,
    pub processed: usize,
    pub total: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<usize>,
}

impl Default for ScanStatus {
    fn default() -> Self {
        ScanStatus {
            state: ScanState::Idle,
            phase: None,
            processed: 0,
            total: 0,
            started_at: None,
            finished_at: None,
            error: None,
            added: None,
            removed: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnalysisSpeed {
    Off,
    Throttled,
    Fast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnalysisState {
    Idle,
    Running,
    Off,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalysisStatus {
    pub speed: AnalysisSpeed,
    pub state: AnalysisState,
    pub done: usize,
    pub total: usize,
}

/// 지금 시각(유닉스 밀리초).
pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
