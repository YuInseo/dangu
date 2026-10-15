//! Internet Archive의 Live Music Archive(etree 컬렉션).
//!
//! 밴드가 녹음과 무료 공유를 허락한 공연 실황 모음이다. 원본 대부분이 FLAC이라
//! 무료이면서 무손실인, 합법적인 음원이다. 키도 가입도 필요 없다.
//!
//! 검색과 공연 정보는 archive.org의 JSON을 화면이 쓰는 모양으로 옮긴다(화면의 예전
//! `archive.ts`와 같은 규칙). 곡은 [`http::HttpSource`]로 받으면서 64비트로 푼다.

pub mod http;

use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::decode::{PcmSource, SymphoniaSource};
use crate::model::AudioFormat;

pub const ARCHIVE: &str = "https://archive.org";
const UNKNOWN_BAND: &str = "알 수 없는 밴드";

/// 검색 결과의 공연 하나.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveShow {
    pub id: String,
    pub title: String,
    pub band: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub venue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloads: Option<u64>,
    /// FLAC 원본이 있는지, 24비트인지. 검색 결과의 format 목록에서 판단한다.
    pub quality: Quality,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Flac24,
    Flac,
    Lossy,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTrack {
    pub file: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_no: Option<i64>,
    pub duration: f64,
    pub format: AudioFormat,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveShowDetail {
    #[serde(flatten)]
    pub show: LiveShow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taper: Option<String>,
    pub tracks: Vec<LiveTrack>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SearchResult {
    pub shows: Vec<LiveShow>,
    pub total: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Popular,
    Recent,
    Newest,
}

impl Sort {
    pub fn parse(s: &str) -> Sort {
        match s {
            "recent" => Sort::Recent,
            "newest" => Sort::Newest,
            _ => Sort::Popular,
        }
    }

    fn field(self) -> &'static str {
        match self {
            Sort::Popular => "downloads desc",
            Sort::Recent => "publicdate desc",
            Sort::Newest => "date desc",
        }
    }
}

/// FLAC 머리의 STREAMINFO. 라이브 아카이브는 "Flac"이라고만 알려 주므로 신호 경로에
/// 정확한 샘플레이트·비트 깊이를 적으려고 파일 앞부분만 받아 읽는다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub sample_rate: u32,
    pub channels: u32,
    pub bit_depth: u32,
    pub total_samples: u64,
}

// ---------------------------------------------------------------------------
// archive.org의 필드는 문자열일 때도, 배열일 때도, 수일 때도 있다.

fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Array(a) => a.first().and_then(scalar),
        Value::Object(_) => None,
    }
}

fn one(v: Option<&Value>) -> Option<String> {
    v.and_then(scalar)
}

fn many(v: Option<&Value>) -> Vec<String> {
    match v {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.iter().filter_map(scalar).collect(),
        Some(v) => scalar(v).into_iter().collect(),
    }
}

fn show_date(v: Option<&Value>) -> Option<String> {
    one(v).filter(|s| !s.is_empty()).map(|s| s.chars().take(10).collect())
}

pub fn quality_from_formats<S: AsRef<str>>(formats: &[S]) -> Quality {
    let f: Vec<String> = formats.iter().map(|x| x.as_ref().to_lowercase()).collect();
    if f.iter().any(|x| x.contains("24bit flac")) {
        Quality::Flac24
    } else if f.iter().any(|x| x == "flac") {
        Quality::Flac
    } else if f.iter().any(|x| x.contains("mp3") || x.contains("ogg")) {
        Quality::Lossy
    } else {
        Quality::Unknown
    }
}

/// Lucene 구문으로 해석될 글자를 지워 구절 검색으로만 쓴다.
fn phrase(q: &str) -> String {
    q.replace(['"', '\\'], " ").trim().to_string()
}

pub fn build_search_url(base: &str, q: &str, sort: Sort, page: u32, rows: u32) -> String {
    let mut parts = vec!["collection:etree".to_string(), "mediatype:etree".to_string()];
    let q = phrase(q);
    if !q.is_empty() {
        parts.push(format!(r#"(creator:("{q}") OR title:("{q}") OR venue:("{q}"))"#));
    }
    let mut params = form_urlencoded::Serializer::new(String::new());
    params.append_pair("q", &parts.join(" AND "));
    for f in ["identifier", "title", "creator", "date", "venue", "coverage", "downloads", "format"] {
        params.append_pair("fl[]", f);
    }
    params.append_pair("sort[]", sort.field());
    params.append_pair("rows", &rows.to_string());
    params.append_pair("page", &page.to_string());
    params.append_pair("output", "json");
    format!("{base}/advancedsearch.php?{}", params.finish())
}

pub fn parse_search(json: &Value) -> SearchResult {
    let r = json.get("response");
    let docs = r.and_then(|r| r.get("docs")).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
    let shows: Vec<LiveShow> = docs
        .iter()
        .filter_map(|d| {
            let id = d.get("identifier").and_then(scalar)?;
            Some(LiveShow {
                title: one(d.get("title")).unwrap_or_else(|| id.clone()),
                band: one(d.get("creator")).unwrap_or_else(|| UNKNOWN_BAND.into()),
                date: show_date(d.get("date")),
                venue: one(d.get("venue")),
                city: one(d.get("coverage")),
                downloads: d.get("downloads").and_then(Value::as_f64).filter(|n| *n >= 0.0).map(|n| n.round() as u64),
                quality: quality_from_formats(&many(d.get("format"))),
                id,
            })
        })
        .collect();
    let total = r.and_then(|r| r.get("numFound")).and_then(Value::as_u64).unwrap_or(shows.len() as u64);
    SearchResult { shows, total }
}

/// "343.12", "05:43", "1:05:43" 모두 초로.
pub fn parse_length(s: Option<&str>) -> f64 {
    let num = |p: &str| p.trim().parse::<f64>().ok().filter(|n| n.is_finite()).unwrap_or(0.0);
    match s {
        None | Some("") => 0.0,
        Some(s) if s.contains(':') => s.split(':').fold(0.0, |acc, p| acc * 60.0 + num(p)),
        Some(s) => num(s),
    }
}

/// JS `parseInt(s, 10)`: 앞의 공백을 건너뛰고 앞자리 정수만.
fn parse_int(s: &str) -> Option<i64> {
    let t = s.trim_start();
    let (sign, digits) = match t.as_bytes().first() {
        Some(b'-') => (-1, &t[1..]),
        Some(b'+') => (1, &t[1..]),
        _ => (1, t),
    };
    let end = digits.bytes().take_while(u8::is_ascii_digit).count();
    digits[..end].parse::<i64>().ok().map(|n| sign * n)
}

/// `encodeURIComponent`와 같은 규칙.
pub fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn download_url(base: &str, id: &str, name: &str) -> String {
    let path: Vec<String> = name.split('/').map(encode_component).collect();
    format!("{base}/download/{}/{}", encode_component(id), path.join("/"))
}

/// 원하는 형식. 앞의 것이 하나라도 있으면 그 형식만 쓴다.
fn preferred_format(rank: usize, file: &Value) -> AudioFormat {
    let f = |codec: &str, container: &str, lossless: bool, mime: &str| AudioFormat {
        codec: codec.into(),
        container: container.into(),
        lossless,
        sample_rate: None,
        bit_depth: None,
        bitrate: None,
        channels: None,
        mime: mime.into(),
    };
    match rank {
        0 => AudioFormat { bit_depth: Some(24), ..f("FLAC", "flac", true, "audio/flac") },
        1 => f("FLAC", "flac", true, "audio/flac"),
        2 => AudioFormat {
            bitrate: one(file.get("bitrate"))
                .and_then(|b| b.trim().parse::<f64>().ok())
                .map(|k| k * 1000.0)
                .filter(|b| b.is_finite() && *b != 0.0)
                .map(|b| b as u32),
            ..f("MP3", "mp3", false, "audio/mpeg")
        },
        _ => f("Vorbis", "ogg", false, r#"audio/ogg; codecs="vorbis""#),
    }
}

fn format_rank(format: &str) -> Option<usize> {
    match format {
        "24bit flac" => Some(0),
        "flac" => Some(1),
        "vbr mp3" | "320kbps mp3" => Some(2),
        "ogg vorbis" => Some(3),
        _ => None,
    }
}

/// `<br\s*/?>`을 줄바꿈으로.
fn replace_br(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let b = tail.as_bytes();
        let mut j = 3;
        let hit = b.len() >= 4 && b[1].eq_ignore_ascii_case(&b'b') && b[2].eq_ignore_ascii_case(&b'r') && {
            while j < b.len() && matches!(b[j], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
                j += 1;
            }
            if j < b.len() && b[j] == b'/' {
                j += 1;
            }
            j < b.len() && b[j] == b'>'
        };
        if hit {
            out.push('\n');
            rest = &tail[j + 1..];
        } else {
            out.push('<');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

/// `</(p|div|li)>`를 줄바꿈으로.
fn replace_block_ends(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    'scan: while let Some(i) = rest.find("</") {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        for name in ["p", "div", "li"] {
            let n = 2 + name.len();
            if tail.len() > n && tail.as_bytes()[n] == b'>' && tail[2..n].eq_ignore_ascii_case(name) {
                out.push('\n');
                rest = &tail[n + 1..];
                continue 'scan;
            }
        }
        out.push_str("</");
        rest = &tail[2..];
    }
    out.push_str(rest);
    out
}

/// `<[^>]+>`를 지운다.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        match tail[1..].find('>') {
            Some(k) if k > 0 => rest = &tail[k + 2..],
            _ => {
                out.push('<');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// 공연 설명은 HTML이 섞인 글이다. 태그는 걷어내고 줄바꿈만 살린다.
pub fn html_to_text(html: Option<&str>) -> Option<String> {
    let html = html.filter(|h| !h.is_empty())?;
    let mut t = strip_tags(&replace_block_ends(&replace_br(html)));
    for (from, to) in [("&nbsp;", " "), ("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'")] {
        t = t.replace(from, to);
    }
    t.retain(|c| c != '\r');
    // 세 줄 이상 빈 줄은 두 줄로
    let mut out = String::with_capacity(t.len());
    let mut run = 0;
    for c in t.chars() {
        if c == '\n' {
            run += 1;
            if run <= 2 {
                out.push(c);
            }
        } else {
            run = 0;
            out.push(c);
        }
    }
    let out = out.trim();
    (!out.is_empty()).then(|| out.to_string())
}

fn truthy_private(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Bool(true))) || matches!(v, Some(Value::String(s)) if s == "true")
}

pub fn parse_show(base: &str, id: &str, json: &Value) -> Option<LiveShowDetail> {
    let m = json.get("metadata").filter(|m| m.is_object())?;
    if json.get("is_dark").is_some_and(|d| d.as_bool().unwrap_or(false) || d.as_str().is_some_and(|s| !s.is_empty())) {
        return None;
    }
    let files: Vec<&Value> = json
        .get("files")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter(|f| !truthy_private(f.get("private")))
                .filter(|f| f.get("format").and_then(Value::as_str).is_some_and(|s| !s.is_empty()))
                .filter(|f| f.get("name").and_then(Value::as_str).is_some())
                .collect()
        })
        .unwrap_or_default();
    let format_of = |f: &Value| f.get("format").and_then(Value::as_str).unwrap_or_default().to_lowercase();

    let mut tracks: Vec<LiveTrack> = Vec::new();
    for rank in 0..4 {
        let chosen: Vec<&&Value> = files.iter().filter(|f| format_rank(&format_of(f)) == Some(rank)).collect();
        if chosen.is_empty() {
            continue;
        }
        tracks = chosen
            .into_iter()
            .map(|f| {
                let name = f.get("name").and_then(Value::as_str).unwrap_or_default();
                let leaf = name.rsplit('/').next().unwrap_or(name);
                let base_name = match leaf.rfind('.') {
                    Some(i) if i + 1 < leaf.len() => &leaf[..i],
                    _ => leaf,
                };
                let title = f.get("title").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).unwrap_or(base_name);
                LiveTrack {
                    file: name.to_string(),
                    title: title.to_string(),
                    track_no: f.get("track").and_then(Value::as_str).and_then(parse_int),
                    duration: parse_length(one(f.get("length")).as_deref()),
                    format: preferred_format(rank, f),
                    url: download_url(base, id, name),
                }
            })
            .collect();
        break;
    }
    tracks.sort_by(track_order);

    let formats: Vec<String> = files.iter().map(|f| f.get("format").and_then(Value::as_str).unwrap_or_default().to_string()).collect();
    Some(LiveShowDetail {
        show: LiveShow {
            id: id.to_string(),
            title: one(m.get("title")).unwrap_or_else(|| id.to_string()),
            band: one(m.get("creator")).unwrap_or_else(|| UNKNOWN_BAND.into()),
            date: show_date(m.get("date")),
            venue: one(m.get("venue")),
            city: one(m.get("coverage")),
            downloads: None,
            quality: quality_from_formats(&formats),
        },
        notes: html_to_text(one(m.get("description")).as_deref()),
        source: one(m.get("source")),
        lineage: one(m.get("lineage")),
        taper: one(m.get("taper")),
        tracks,
    })
}

pub fn is_valid_id(id: &str) -> bool {
    (1..=120).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// 공연 안의 파일 이름. 위로 올라가거나 절대 경로인 것은 받지 않는다.
pub fn is_valid_file(file: &str) -> bool {
    !file.is_empty() && file.len() <= 300 && !file.contains("..") && !file.starts_with('/') && !file.contains('\\')
}

pub fn parse_flac_stream_info(buf: &[u8]) -> Option<StreamInfo> {
    let mut off = 0usize;
    // 앞에 ID3v2 태그가 붙은 FLAC도 있다.
    if buf.len() >= 10 && &buf[..3] == b"ID3" {
        let size = ((buf[6] as usize & 0x7f) << 21)
            | ((buf[7] as usize & 0x7f) << 14)
            | ((buf[8] as usize & 0x7f) << 7)
            | (buf[9] as usize & 0x7f);
        off = 10 + size + if buf[5] & 0x10 != 0 { 10 } else { 0 };
    }
    if buf.len() < off + 4 + 4 + 18 || &buf[off..off + 4] != b"fLaC" {
        return None;
    }
    off += 4;
    // 첫 블록은 반드시 STREAMINFO
    if buf[off] & 0x7f != 0 {
        return None;
    }
    off += 4;
    let b = &buf[off + 10..off + 18];
    let sample_rate = ((b[0] as u32) << 12) | ((b[1] as u32) << 4) | ((b[2] as u32) >> 4);
    let channels = ((b[2] as u32 >> 1) & 0x7) + 1;
    let bit_depth = (((b[2] as u32 & 1) << 4) | (b[3] as u32 >> 4)) + 1;
    let total_samples = ((b[3] as u64 & 0x0f) << 32) | ((b[4] as u64) << 24) | ((b[5] as u64) << 16) | ((b[6] as u64) << 8) | b[7] as u64;
    (sample_rate != 0).then_some(StreamInfo { sample_rate, channels, bit_depth, total_samples })
}

// ---------------------------------------------------------------------------
// archive.org에 묻는 쪽

type CacheEntry = (Instant, Arc<dyn Any + Send + Sync>);

pub struct Archive {
    base: String,
    json: ureq::Agent,
    media: ureq::Agent,
    cache: Mutex<HashMap<String, CacheEntry>>,
}

pub fn user_agent() -> String {
    format!("Sori/{} (self-hosted music player)", crate::VERSION)
}

impl Archive {
    /// `SORI_ARCHIVE_URL`이 있으면 그 주소로(시험용 가짜 서버).
    pub fn from_env() -> Archive {
        Archive::new(std::env::var("SORI_ARCHIVE_URL").ok().filter(|s| !s.is_empty()).as_deref().unwrap_or(ARCHIVE))
    }

    pub fn new(base: &str) -> Archive {
        let json = ureq::Agent::config_builder()
            .user_agent(user_agent())
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(12)))
            .build()
            .new_agent();
        Archive { base: base.trim_end_matches('/').to_string(), json, media: http::agent(), cache: Mutex::new(HashMap::new()) }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn download_url(&self, id: &str, file: &str) -> String {
        download_url(&self.base, id, file)
    }

    fn cached<T: Send + Sync + 'static>(
        &self,
        key: String,
        ttl: Duration,
        load: impl FnOnce() -> anyhow::Result<T>,
    ) -> anyhow::Result<Arc<T>> {
        if let Some((at, v)) = self.cache.lock().get(&key)
            && at.elapsed() < ttl
            && let Ok(v) = v.clone().downcast::<T>()
        {
            return Ok(v);
        }
        let v = Arc::new(load()?);
        let mut c = self.cache.lock();
        c.insert(key, (Instant::now(), v.clone()));
        if c.len() > 500
            && let Some(old) = c.iter().min_by_key(|(_, (at, _))| *at).map(|(k, _)| k.clone())
        {
            c.remove(&old);
        }
        Ok(v)
    }

    fn get_json(&self, url: &str) -> anyhow::Result<Value> {
        let mut res = self.json.get(url).call().map_err(|e| anyhow!("archive.org에 닿지 못했습니다: {e}"))?;
        if !res.status().is_success() {
            bail!("archive.org {}", res.status().as_u16());
        }
        let bytes = res.body_mut().with_config().limit(32 << 20).read_to_vec()?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn search(&self, q: &str, sort: Sort, page: u32, rows: u32) -> anyhow::Result<Arc<SearchResult>> {
        let url = build_search_url(&self.base, q, sort, page.max(1), rows.clamp(1, 100));
        self.cached(format!("s:{url}"), Duration::from_secs(10 * 60), || Ok(parse_search(&self.get_json(&url)?)))
    }

    pub fn show(&self, id: &str) -> anyhow::Result<Arc<Option<LiveShowDetail>>> {
        if !is_valid_id(id) {
            return Ok(Arc::new(None));
        }
        self.cached(format!("m:{id}"), Duration::from_secs(30 * 60), || {
            let json = self.get_json(&format!("{}/metadata/{}", self.base, encode_component(id)))?;
            Ok(parse_show(&self.base, id, &json))
        })
    }

    /// 파일 앞 64KB만 받아 STREAMINFO를 읽는다.
    pub fn probe(&self, id: &str, file: &str) -> anyhow::Result<Option<StreamInfo>> {
        if !is_valid_id(id) || !is_valid_file(file) || !file.to_ascii_lowercase().ends_with(".flac") {
            return Ok(None);
        }
        let url = self.download_url(id, file);
        let v = self.cached(format!("p:{id}/{file}"), Duration::from_secs(24 * 60 * 60), || {
            let head = http::fetch_head(&self.media, &url, 65536)?;
            Ok(parse_flac_stream_info(&head))
        })?;
        Ok(*v)
    }

    /// 곡을 받으면서 푼다. 받는 일은 뒤에서 하고, 디코더는 받은 만큼 읽는다.
    pub fn open(&self, id: &str, file: &str) -> anyhow::Result<Box<dyn PcmSource>> {
        if !is_valid_id(id) || !is_valid_file(file) {
            bail!("잘못된 공연 파일입니다");
        }
        let src = http::HttpSource::open(&self.media, &self.download_url(id, file))?;
        let gate = src.gate();
        let ext = file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        Ok(Box::new(SymphoniaSource::from_media(Box::new(src), &ext)?.with_gate(move || gate.ready())))
    }
}

/// 같은 공연 안에서의 순서: 트랙 번호가 있으면 그 순서로, 없으면 파일 이름을 사람이 읽는 순서로.
fn track_order(a: &LiveTrack, b: &LiveTrack) -> Ordering {
    let key = |t: &LiveTrack| (t.track_no.is_none(), t.track_no.unwrap_or(0));
    key(a).cmp(&key(b)).then_with(|| crate::collate::compare(&a.file, &b.file)).then_with(|| a.file.cmp(&b.file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn search_fixture() -> Value {
        json!({
          "responseHeader": { "status": 0 },
          "response": {
            "numFound": 2,
            "start": 0,
            "docs": [
              {
                "identifier": "gd1977-05-08.sbd.example.flac16",
                "title": "Grateful Dead Live at Barton Hall on 1977-05-08",
                "creator": "Grateful Dead",
                "date": "1977-05-08T00:00:00Z",
                "venue": "Barton Hall, Cornell University",
                "coverage": "Ithaca, NY",
                "downloads": 1234567,
                "format": ["Flac", "VBR MP3", "Text", "Checksums"]
              },
              {
                "identifier": "kglw2023-06-07.example",
                "title": "King Gizzard & The Lizard Wizard Live at Red Rocks",
                "creator": ["King Gizzard & The Lizard Wizard"],
                "date": "2023-06-07T00:00:00Z",
                "format": ["24bit Flac", "VBR MP3"]
              }
            ]
          }
        })
    }

    fn show_fixture() -> Value {
        json!({
          "metadata": {
            "identifier": "kglw2023-06-07.example",
            "title": "King Gizzard & The Lizard Wizard Live at Red Rocks",
            "creator": "King Gizzard & The Lizard Wizard",
            "date": "2023-06-07",
            "venue": "Red Rocks Amphitheatre",
            "coverage": "Morrison, CO",
            "description": "Set 1:<br />01. Mars For The Rich<br/>02. Hate Dancin&#39;<br><br>Recorded by &amp; transferred by taper",
            "source": "Schoeps MK4 > Sound Devices MixPre-6",
            "lineage": "WAV > FLAC",
            "taper": "Someone"
          },
          "files": [
            { "name": "kglw2023-06-07d1t02.flac", "format": "24bit Flac", "source": "original", "title": "Hate Dancin'", "track": "02", "length": "5:12" },
            { "name": "kglw2023-06-07d1t01.flac", "format": "24bit Flac", "source": "original", "title": "Mars For The Rich", "track": "01", "length": "423.54" },
            { "name": "kglw2023-06-07d1t01.mp3", "format": "VBR MP3", "source": "derivative", "title": "Mars For The Rich", "length": "07:03" },
            { "name": "kglw2023-06-07.ffp", "format": "Flac FingerPrint", "source": "original" },
            { "name": "secret.flac", "format": "24bit Flac", "private": "true" }
          ]
        })
    }

    #[test]
    fn search_results() {
        let r = parse_search(&search_fixture());
        assert_eq!(r.total, 2);
        assert_eq!(r.shows[0].band, "Grateful Dead");
        assert_eq!(r.shows[0].date.as_deref(), Some("1977-05-08"));
        assert_eq!(r.shows[0].quality, Quality::Flac);
        assert_eq!(r.shows[0].downloads, Some(1234567));
        assert_eq!(r.shows[1].band, "King Gizzard & The Lizard Wizard");
        assert_eq!(r.shows[1].quality, Quality::Flac24);
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!(j["shows"][1]["quality"], "flac24");
        assert!(j["shows"][1].get("venue").is_none());
    }

    #[test]
    fn search_url_is_a_phrase_search() {
        let url = build_search_url(ARCHIVE, r#"King "Gizzard""#, Sort::Recent, 2, 48);
        let query = url.split_once('?').unwrap().1;
        let params: Vec<(String, String)> = form_urlencoded::parse(query.as_bytes()).into_owned().collect();
        let get = |k: &str| params.iter().filter(|(a, _)| a == k).map(|(_, v)| v.clone()).collect::<Vec<_>>();
        let q = &get("q")[0];
        assert!(q.contains("collection:etree"));
        assert!(q.contains(r#"creator:("King  Gizzard")"#), "{q}");
        assert_eq!(get("sort[]"), ["publicdate desc"]);
        assert_eq!(get("page"), ["2"]);
        assert!(get("fl[]").contains(&"format".to_string()));
        assert!(url.starts_with("https://archive.org/advancedsearch.php?q="));
    }

    #[test]
    fn show_best_format_in_track_order_without_private_files() {
        let s = parse_show(ARCHIVE, "kglw2023-06-07.example", &show_fixture()).unwrap();
        assert_eq!(s.show.band, "King Gizzard & The Lizard Wizard");
        assert_eq!(s.tracks.len(), 2);
        assert_eq!(s.tracks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Mars For The Rich", "Hate Dancin'"]);
        assert_eq!(s.tracks[0].format.bit_depth, Some(24));
        assert!(s.tracks[0].format.lossless);
        assert_eq!(s.tracks[0].duration.round(), 424.0);
        assert_eq!(s.tracks[1].duration, 312.0);
        assert_eq!(s.tracks[0].url, "https://archive.org/download/kglw2023-06-07.example/kglw2023-06-07d1t01.flac");
        let notes = s.notes.as_deref().unwrap();
        assert!(notes.contains("01. Mars For The Rich\n02. Hate Dancin'"), "{notes:?}");
        assert!(notes.contains("Recorded by & transferred"));
        assert_eq!(s.show.quality, Quality::Flac24);
        let j = serde_json::to_value(&s).unwrap();
        assert_eq!(j["band"], "King Gizzard & The Lizard Wizard");
        assert_eq!(j["tracks"][0]["trackNo"], 1);
        assert_eq!(j["tracks"][0]["format"]["bitDepth"], 24);
    }

    #[test]
    fn mp3_when_no_flac() {
        let s = parse_show(
            ARCHIVE,
            "x",
            &json!({ "metadata": { "title": "x", "creator": "Band" }, "files": [{ "name": "a.mp3", "format": "VBR MP3", "title": "A", "bitrate": "218" }] }),
        )
        .unwrap();
        assert_eq!(s.tracks[0].format.codec, "MP3");
        assert!(!s.tracks[0].format.lossless);
        assert_eq!(s.tracks[0].format.bitrate, Some(218_000));
    }

    #[test]
    fn missing_show() {
        assert_eq!(parse_show(ARCHIVE, "x", &json!({})), None);
        assert_eq!(parse_show(ARCHIVE, "x", &json!({ "metadata": {}, "is_dark": true })), None);
    }

    #[test]
    fn lengths_and_text() {
        assert_eq!(parse_length(Some("1:02:03")), 3723.0);
        assert_eq!(parse_length(Some("05:43")), 343.0);
        assert_eq!(parse_length(Some("343.5")), 343.5);
        assert_eq!(parse_length(None), 0.0);
        assert_eq!(parse_length(Some("abc")), 0.0);
        assert_eq!(html_to_text(Some("<p>a</p><p>b &amp; c</p>")).as_deref(), Some("a\nb & c"));
        assert_eq!(html_to_text(Some("a<BR>b<br/>c<br   />d")).as_deref(), Some("a\nb\nc\nd"));
        assert_eq!(html_to_text(Some("x</DIV>y</li>z</span>w")).as_deref(), Some("x\ny\nzw"));
        // 화면의 정규식(`<[^>]+>`)과 같이, 짝 없는 `<`는 다음 `>`까지 태그로 본다.
        assert_eq!(html_to_text(Some("1 < 2 and <b>bold</b>")).as_deref(), Some("1 bold"));
        assert_eq!(html_to_text(Some("a < b")).as_deref(), Some("a < b"));
        assert_eq!(html_to_text(Some("a\r\n\n\n\n\nb")).as_deref(), Some("a\n\nb"));
        assert_eq!(html_to_text(Some("<p></p>")), None);
        assert_eq!(quality_from_formats(&["Shorten", "Ogg Vorbis"]), Quality::Lossy);
        assert_eq!(parse_int(" 07"), Some(7));
        assert_eq!(parse_int("2/12"), Some(2));
        assert_eq!(parse_int("x"), None);
    }

    #[test]
    fn ids_and_urls() {
        assert!(is_valid_id("gd1977-05-08.sbd.miller.97065.flac16"));
        assert!(!is_valid_id("../etc"));
        assert!(!is_valid_id(""));
        assert!(is_valid_file("disc 1/01 Intro.flac"));
        assert!(!is_valid_file("../x.flac"));
        assert!(!is_valid_file("/x.flac"));
        assert_eq!(download_url(ARCHIVE, "a b", "d1/t 01 (live)#.flac"), "https://archive.org/download/a%20b/d1/t%2001%20(live)%23.flac");
        assert_eq!(encode_component("한"), "%ED%95%9C");
    }

    fn streaminfo(rate: u32, channels: u32, bits: u32, total: u64) -> Vec<u8> {
        let mut si = [0u8; 34];
        si[..4].copy_from_slice(&[0x10, 0x00, 0x10, 0x00]);
        si[10] = (rate >> 12) as u8;
        si[11] = (rate >> 4) as u8;
        si[12] = (((rate & 0xf) << 4) | ((channels - 1) << 1) | ((bits - 1) >> 4)) as u8;
        si[13] = ((((bits - 1) & 0xf) << 4) as u64 | (total >> 32)) as u8;
        si[14..18].copy_from_slice(&(total as u32).to_be_bytes());
        let mut v = b"fLaC".to_vec();
        v.extend_from_slice(&[0x00, 0x00, 0x00, 34]);
        v.extend_from_slice(&si);
        v
    }

    #[test]
    fn flac_streaminfo_with_and_without_id3() {
        let flac = streaminfo(44_100, 2, 16, 441_000);
        let mut id3 = vec![b'I', b'D', b'3', 3, 0, 0, 0, 0, 0, 5, 1, 2, 3, 4, 5];
        id3.extend_from_slice(&flac);
        let want = StreamInfo { sample_rate: 44_100, channels: 2, bit_depth: 16, total_samples: 441_000 };
        assert_eq!(parse_flac_stream_info(&flac), Some(want));
        assert_eq!(parse_flac_stream_info(&id3), Some(want));
        assert_eq!(parse_flac_stream_info(&[1, 2, 3]), None);
        let big = streaminfo(192_000, 6, 24, (1 << 33) + 5);
        assert_eq!(
            parse_flac_stream_info(&big),
            Some(StreamInfo { sample_rate: 192_000, channels: 6, bit_depth: 24, total_samples: (1 << 33) + 5 })
        );
        assert_eq!(
            serde_json::to_value(want).unwrap(),
            json!({ "sampleRate": 44100, "channels": 2, "bitDepth": 16, "totalSamples": 441000 })
        );
    }
}
