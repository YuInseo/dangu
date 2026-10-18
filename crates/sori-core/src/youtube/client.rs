//! 유튜브에 묻는 쪽. Data API(사용자의 키), 채널 RSS와 oEmbed(키 없이), 그리고 이 기기에 둔
//! 구독(youtube.json).

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::*;

const FILE: &str = "youtube.json";
/// 구독 피드를 합친 것(앱을 다시 켜면 곧바로 보인다)
const FEED_FILE: &str = "youtube-feed.json";
/// 구독 피드가 이만큼 묵으면 뒤에서 새로 읽는다.
const FEED_STALE: Duration = Duration::from_secs(15 * 60);
/// 구독 피드에 남겨 두는 영상 수
const FEED_KEEP: usize = 1500;

/// 키가 있어야 하는 일을 키 없이 불렀다.
#[derive(Debug)]
pub struct NeedKey;

impl fmt::Display for NeedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("유튜브 API 키가 필요합니다(설정 → 연결 → 유튜브)")
    }
}

impl std::error::Error for NeedKey {}

/// 유튜브가 거절했다(할당량, 잘못된 키, 댓글 막힘 …).
#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

/// 잘못된 요청(ID 모양이 틀렸다). 유튜브에 묻지도 않는다.
fn bad(message: &str) -> anyhow::Error {
    ApiError { status: 400, message: message.to_string() }.into()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
    #[serde(default)]
    subscriptions: Vec<Subscription>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoDetail {
    pub video: Video,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<Channel>,
    /// 키 없이 읽었다(조회수·설명·댓글 없음)
    pub limited: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelDetail {
    pub channel: Channel,
    pub videos: Page<Video>,
    pub limited: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistDetail {
    pub playlist: Playlist,
    pub videos: Page<Video>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionFeed {
    pub items: Vec<Video>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// 피드를 읽지 못한 채널
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<String>,
    /// 묵은 목록을 주고 뒤에서 새로 읽는 중(조금 뒤 다시 물으면 새 영상이 있을 수 있다)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub refreshing: bool,
}

/// 구독한 채널들의 새 영상을 합친 것
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Merged {
    /// 마지막으로 모두 새로 읽은 때(유닉스 밀리초)
    at: i64,
    /// 이 목록이 아는 채널
    channels: Vec<String>,
    items: Vec<Video>,
    failed: Vec<String>,
}

type CacheEntry = (Instant, Arc<dyn Any + Send + Sync>);

/// 쉬던 연결이 저쪽에서 끊겼다(새 연결이면 된다)
fn dropped(e: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(e.kind(), UnexpectedEof | ConnectionReset | ConnectionAborted | BrokenPipe)
}

pub struct YouTube {
    api: String,
    web: String,
    agent: ureq::Agent,
    file: PathBuf,
    /// 환경 변수로 준 키(설정에 넣은 것이 없을 때 쓴다)
    env_key: Option<String>,
    saved: Mutex<Saved>,
    cache: Mutex<HashMap<String, CacheEntry>>,
    feed_file: PathBuf,
    /// 구독 피드. 처음 물을 때 디스크에서 읽는다.
    merged: Mutex<Option<Merged>>,
    refreshing: AtomicBool,
}

impl YouTube {
    /// `SORI_YOUTUBE_API`, `SORI_YOUTUBE_WEB`가 있으면 그 주소로(시험용 가짜 서버). `SORI_YOUTUBE_KEY`는 키.
    pub fn open(data_dir: &Path) -> YouTube {
        let env = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let mut yt = YouTube::with_bases(
            data_dir,
            env("SORI_YOUTUBE_API").as_deref().unwrap_or(API),
            env("SORI_YOUTUBE_WEB").as_deref().unwrap_or(WEB),
        );
        yt.env_key = env("SORI_YOUTUBE_KEY");
        yt
    }

    pub fn with_bases(data_dir: &Path, api: &str, web: &str) -> YouTube {
        let file = data_dir.join(FILE);
        let saved = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let agent = ureq::Agent::config_builder()
            .user_agent(crate::live::user_agent())
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(15)))
            .build()
            .new_agent();
        YouTube {
            api: api.trim_end_matches('/').to_string(),
            web: web.trim_end_matches('/').to_string(),
            agent,
            file,
            env_key: None,
            saved: Mutex::new(saved),
            cache: Mutex::new(HashMap::new()),
            feed_file: data_dir.join(FEED_FILE),
            merged: Mutex::new(None),
            refreshing: AtomicBool::new(false),
        }
    }

    fn persist(&self, s: &Saved) {
        if let Ok(bytes) = serde_json::to_vec_pretty(s)
            && let Err(e) = crate::config::write_atomic(&self.file, &bytes)
        {
            tracing::warn!("유튜브 설정을 적지 못했습니다: {e}");
        }
    }

    // -----------------------------------------------------------------------
    // 키

    fn key(&self) -> Option<String> {
        self.saved.lock().api_key.clone().or_else(|| self.env_key.clone())
    }

    pub fn has_key(&self) -> bool {
        self.key().is_some()
    }

    /// 화면에 보일 끝 네 글자
    pub fn key_hint(&self) -> Option<String> {
        self.key().map(|k| {
            let tail: String = k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
            format!("…{tail}")
        })
    }

    /// 넣기 전에 한 번 물어 본다(1 할당량). 맞으면 저장한다.
    pub fn set_key(&self, key: &str) -> anyhow::Result<()> {
        let key = key.trim();
        if key.len() < 20 || key.len() > 100 || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            anyhow::bail!("API 키 모양이 아닙니다(AIza로 시작하는 39글자)");
        }
        self.call(key, "videos", &[("part", "id"), ("chart", "mostPopular"), ("maxResults", "1")])?;
        let mut s = self.saved.lock();
        s.api_key = Some(key.to_string());
        let snapshot = s.clone();
        self.persist(&snapshot);
        drop(s);
        self.cache.lock().clear();
        Ok(())
    }

    pub fn clear_key(&self) {
        let mut s = self.saved.lock();
        s.api_key = None;
        let snapshot = s.clone();
        self.persist(&snapshot);
    }

    // -----------------------------------------------------------------------
    // 묻기

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
        if c.len() > 600
            && let Some(old) = c.iter().min_by_key(|(_, (at, _))| *at).map(|(k, _)| k.clone())
        {
            c.remove(&old);
        }
        Ok(v)
    }

    fn url(base: &str, path: &str, params: &[(&str, &str)]) -> String {
        let mut q = form_urlencoded::Serializer::new(String::new());
        for (k, v) in params {
            q.append_pair(k, v);
        }
        format!("{base}/{path}?{}", q.finish())
    }

    /// GET. 쉬던 연결이 저쪽에서 끊겨 있었으면(휴대폰이 망을 바꿨다 등) 곧바로 실패하므로, 그때만 새 연결로 다시.
    fn send(&self, url: &str) -> anyhow::Result<ureq::http::Response<ureq::Body>> {
        let mut tries = 0;
        loop {
            match self.agent.get(url).call() {
                Err(ureq::Error::Io(e)) if tries < 2 && dropped(&e) => tries += 1,
                r => return r.map_err(|e| anyhow!("유튜브에 닿지 못했습니다: {e}")),
            }
        }
    }

    fn call(&self, key: &str, path: &str, params: &[(&str, &str)]) -> anyhow::Result<Value> {
        let mut all: Vec<(&str, &str)> = params.to_vec();
        all.push(("key", key));
        let url = Self::url(&self.api, path, &all);
        let mut res = self.send(&url)?;
        let status = res.status().as_u16();
        let body = res.body_mut().with_config().limit(16 << 20).read_to_vec()?;
        let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        if !(200..300).contains(&status) {
            return Err(ApiError { status, message: api_error(status, &json) }.into());
        }
        Ok(json)
    }

    fn get(&self, path: &str, params: &[(&str, &str)]) -> anyhow::Result<Value> {
        let key = self.key().ok_or(NeedKey)?;
        self.call(&key, path, params)
    }

    fn get_web(&self, path: &str, params: &[(&str, &str)]) -> anyhow::Result<(u16, String)> {
        let url = Self::url(&self.web, path, params);
        let mut res = self.send(&url)?;
        let status = res.status().as_u16();
        let body = res.body_mut().with_config().limit(8 << 20).read_to_string()?;
        Ok((status, body))
    }

    /// 영상 자세히(최대 50개씩). 순서는 `ids` 그대로.
    pub fn videos(&self, ids: &[String]) -> anyhow::Result<Vec<Video>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(50) {
            let joined = chunk.join(",");
            let json =
                self.get("videos", &[("part", "snippet,contentDetails,statistics,status"), ("id", &joined), ("maxResults", "50")])?;
            let got: HashMap<String, Video> = page(&json, parse_video).items.into_iter().map(|v| (v.id.clone(), v)).collect();
            out.extend(chunk.iter().filter_map(|id| got.get(id).cloned()));
        }
        Ok(out)
    }

    fn channels(&self, ids: &[String]) -> anyhow::Result<Vec<Channel>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(50) {
            let joined = chunk.join(",");
            let json = self.get(
                "channels",
                &[("part", "snippet,statistics,brandingSettings,contentDetails"), ("id", &joined), ("maxResults", "50")],
            )?;
            out.extend(page(&json, parse_channel).items);
        }
        Ok(out)
    }

    /// 목록의 영상에 길이·조회수를 채운다(1 할당량).
    fn enrich(&self, mut videos: Vec<Video>) -> anyhow::Result<Vec<Video>> {
        let ids: Vec<String> = videos.iter().map(|v| v.id.clone()).collect();
        if ids.is_empty() {
            return Ok(videos);
        }
        let full: HashMap<String, Video> = self.videos(&ids)?.into_iter().map(|v| (v.id.clone(), v)).collect();
        for v in &mut videos {
            if let Some(f) = full.get(&v.id) {
                v.duration = f.duration;
                v.views = f.views;
                v.likes = f.likes;
                v.live = f.live.clone();
                v.embeddable = f.embeddable;
                if v.thumb.is_none() {
                    v.thumb = f.thumb.clone();
                }
            }
        }
        Ok(videos)
    }

    /// 지역의 인기 음악(카테고리 10). 차트를 모두(최대 200, 4 할당량) 받아 새 것부터 둔다.
    /// `page_token`은 앞 쪽의 `next`.
    pub fn popular(&self, region: &str, page_token: Option<&str>) -> anyhow::Result<Arc<Page<Video>>> {
        let region =
            if region.len() == 2 && region.bytes().all(|b| b.is_ascii_alphabetic()) { region.to_ascii_uppercase() } else { "KR".into() };
        let chart = self.cached(format!("pop:{region}"), Duration::from_secs(30 * 60), || {
            let mut items = Vec::new();
            let mut token = String::new();
            for _ in 0..4 {
                let mut p = vec![
                    ("part", "snippet,contentDetails,statistics,status"),
                    ("chart", "mostPopular"),
                    ("videoCategoryId", "10"),
                    ("regionCode", region.as_str()),
                    ("maxResults", "50"),
                ];
                if !token.is_empty() {
                    p.push(("pageToken", token.as_str()));
                }
                let got = match self.get("videos", &p) {
                    Ok(j) => page(&j, parse_video),
                    // 뒤쪽을 못 받았으면 받은 데까지
                    Err(e) if !items.is_empty() => {
                        tracing::info!("인기 음악 뒤쪽을 받지 못했습니다: {e:#}");
                        break;
                    }
                    Err(e) => return Err(e),
                };
                items.extend(got.items);
                match got.next {
                    Some(t) if !t.is_empty() => token = t,
                    _ => break,
                }
            }
            newest_first(&mut items);
            Ok(items)
        })?;
        let start = page_token.and_then(|p| p.parse::<usize>().ok()).unwrap_or(0).min(chart.len());
        let end = (start + 24).min(chart.len());
        Ok(Arc::new(Page {
            items: chart[start..end].to_vec(),
            next: (end < chart.len()).then(|| end.to_string()),
            total: Some(chart.len() as u64),
        }))
    }

    /// 검색(100 할당량). `kind`는 "video", "channel", "playlist" 또는 빈 것(모두). `order`는 "date"(새 것부터, 기본)
    /// 또는 "relevance"(관련순).
    pub fn search(
        &self,
        q: &str,
        kind: &str,
        page_token: Option<&str>,
        region: &str,
        order: &str,
    ) -> anyhow::Result<Arc<Page<SearchItem>>> {
        let order = if order == "relevance" { "relevance" } else { "date" };
        let q = q.trim().to_string();
        let kind = match kind {
            "video" | "channel" | "playlist" => kind.to_string(),
            _ => "video,channel,playlist".to_string(),
        };
        let token = page_token.unwrap_or("").to_string();
        let region = if region.len() == 2 { region.to_ascii_uppercase() } else { "KR".into() };
        self.cached(format!("s:{kind}:{region}:{order}:{token}:{q}"), Duration::from_secs(30 * 60), || {
            let mut p = vec![
                ("part", "snippet"),
                ("q", q.as_str()),
                ("type", kind.as_str()),
                ("order", order),
                ("maxResults", "20"),
                ("regionCode", region.as_str()),
            ];
            if !token.is_empty() {
                p.push(("pageToken", token.as_str()));
            }
            let mut res = page(&self.get("search", &p)?, parse_search_item);
            // 영상은 길이·조회수를, 채널은 구독자 수를 붙인다.
            let vids: Vec<Video> =
                res.items.iter().filter_map(|i| if let SearchItem::Video(v) = i { Some(v.clone()) } else { None }).collect();
            let vids: HashMap<String, Video> = self.enrich(vids)?.into_iter().map(|v| (v.id.clone(), v)).collect();
            let cids: Vec<String> =
                res.items.iter().filter_map(|i| if let SearchItem::Channel(c) = i { Some(c.id.clone()) } else { None }).collect();
            let chans: HashMap<String, Channel> =
                if cids.is_empty() { HashMap::new() } else { self.channels(&cids)?.into_iter().map(|c| (c.id.clone(), c)).collect() };
            for item in &mut res.items {
                match item {
                    SearchItem::Video(v) => {
                        if let Some(f) = vids.get(&v.id) {
                            *v = f.clone();
                        }
                    }
                    SearchItem::Channel(c) => {
                        if let Some(f) = chans.get(&c.id) {
                            *c = f.clone();
                        }
                    }
                    SearchItem::Playlist(_) => {}
                }
            }
            Ok(res)
        })
    }

    /// 영상 하나. 키가 없으면 oEmbed로 제목·채널 이름만.
    pub fn video(&self, id: &str) -> anyhow::Result<Arc<VideoDetail>> {
        if !is_video_id(id) {
            return Err(bad("영상 주소가 이상합니다"));
        }
        let id = id.to_string();
        let keyed = self.has_key();
        self.cached(format!("v:{keyed}:{id}"), Duration::from_secs(10 * 60), || {
            if keyed {
                let video = self
                    .videos(std::slice::from_ref(&id))?
                    .into_iter()
                    .next()
                    .ok_or_else(|| ApiError {
                        status: 404, message: "영상을 찾을 수 없습니다(지워졌거나 비공개)".into()
                    })?;
                let channel = match &video.channel_id {
                    Some(c) => self.channels(std::slice::from_ref(c))?.into_iter().next(),
                    None => None,
                };
                return Ok(VideoDetail { video, channel, limited: false });
            }
            let watch = format!("https://www.youtube.com/watch?v={id}");
            let (status, body) = self.get_web("oembed", &[("url", &watch), ("format", "json")])?;
            let json: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            match status {
                200 => {}
                401 | 403 => {
                    // 올린 사람이 유튜브 밖에서 틀지 못하게 막았다. 제목도 주지 않으니 막혔다는 것만 알린다.
                    let video = Video { id: id.clone(), embeddable: Some(false), ..Default::default() };
                    return Ok(VideoDetail { video, channel: None, limited: true });
                }
                404 | 400 => {
                    return Err(ApiError { status: 404, message: "영상을 찾을 수 없습니다(지워졌거나 비공개)".into() }.into());
                }
                s => return Err(ApiError { status: s, message: format!("유튜브가 거절했습니다({s})") }.into()),
            }
            let video = parse_oembed(&id, &json).ok_or_else(|| anyhow!("영상 정보를 읽지 못했습니다"))?;
            let channel = match oembed_author(&json) {
                Some(Link::Channel(cid)) => Some(Channel { id: cid, title: video.channel_title.clone(), ..Default::default() }),
                _ => None,
            };
            Ok(VideoDetail { video, channel, limited: true })
        })
    }

    /// 댓글(맨 위 댓글과 답글 몇 개). `order`는 "relevance"(인기) 또는 "time"(최신).
    pub fn comments(&self, video_id: &str, order: &str, page_token: Option<&str>) -> anyhow::Result<Arc<Page<Comment>>> {
        if !is_video_id(video_id) {
            return Err(bad("영상 주소가 이상합니다"));
        }
        let order = if order == "time" { "time" } else { "relevance" };
        let token = page_token.unwrap_or("").to_string();
        let vid = video_id.to_string();
        self.cached(format!("c:{vid}:{order}:{token}"), Duration::from_secs(5 * 60), || {
            let mut p = vec![
                ("part", "snippet,replies"),
                ("videoId", vid.as_str()),
                ("order", order),
                ("maxResults", "20"),
                ("textFormat", "plainText"),
            ];
            if !token.is_empty() {
                p.push(("pageToken", token.as_str()));
            }
            Ok(page(&self.get("commentThreads", &p)?, parse_thread))
        })
    }

    pub fn replies(&self, comment_id: &str, page_token: Option<&str>) -> anyhow::Result<Arc<Page<Comment>>> {
        if comment_id.is_empty() || comment_id.len() > 100 || !comment_id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err(bad("댓글 주소가 이상합니다"));
        }
        let token = page_token.unwrap_or("").to_string();
        let cid = comment_id.to_string();
        self.cached(format!("r:{cid}:{token}"), Duration::from_secs(5 * 60), || {
            let mut p = vec![("part", "snippet"), ("parentId", cid.as_str()), ("maxResults", "20"), ("textFormat", "plainText")];
            if !token.is_empty() {
                p.push(("pageToken", token.as_str()));
            }
            Ok(page(&self.get("comments", &p)?, parse_reply))
        })
    }

    /// 채널 첫 화면: 정보와 최근 영상. 키가 없으면 RSS로 이름과 최근 영상 15개.
    pub fn channel(&self, id: &str) -> anyhow::Result<Arc<ChannelDetail>> {
        if !is_channel_id(id) {
            return Err(bad("채널 주소가 이상합니다"));
        }
        let id = id.to_string();
        let keyed = self.has_key();
        self.cached(format!("ch:{keyed}:{id}"), Duration::from_secs(30 * 60), || {
            if keyed {
                let channel = self
                    .channels(std::slice::from_ref(&id))?
                    .into_iter()
                    .next()
                    .ok_or_else(|| ApiError { status: 404, message: "채널을 찾을 수 없습니다".into() })?;
                let videos = match &channel.uploads {
                    Some(u) => self.channel_videos(u, None)?.as_ref().clone(),
                    None => Page { items: Vec::new(), next: None, total: None },
                };
                return Ok(ChannelDetail { channel, videos, limited: false });
            }
            let feed = self.feed(&id)?;
            let sub_thumb = self.saved.lock().subscriptions.iter().find(|s| s.id == id).and_then(|s| s.thumb.clone());
            Ok(ChannelDetail {
                channel: Channel { id: id.clone(), title: feed.title.clone(), thumb: sub_thumb, ..Default::default() },
                videos: Page { items: feed.videos.clone(), next: None, total: None },
                limited: true,
            })
        })
    }

    /// 채널이 올린 영상(올린 영상 재생목록 UU…)
    pub fn channel_videos(&self, uploads: &str, page_token: Option<&str>) -> anyhow::Result<Arc<Page<Video>>> {
        self.playlist_items(uploads, page_token)
    }

    fn playlist_items(&self, playlist: &str, page_token: Option<&str>) -> anyhow::Result<Arc<Page<Video>>> {
        if !is_playlist_id(playlist) {
            return Err(bad("재생목록 주소가 이상합니다"));
        }
        let token = page_token.unwrap_or("").to_string();
        let pl = playlist.to_string();
        self.cached(format!("pi:{pl}:{token}"), Duration::from_secs(15 * 60), || {
            let mut p = vec![("part", "snippet,contentDetails"), ("playlistId", pl.as_str()), ("maxResults", "30")];
            if !token.is_empty() {
                p.push(("pageToken", token.as_str()));
            }
            let mut res = page(&self.get("playlistItems", &p)?, parse_playlist_item);
            res.items = self.enrich(std::mem::take(&mut res.items))?;
            Ok(res)
        })
    }

    pub fn channel_playlists(&self, channel_id: &str, page_token: Option<&str>) -> anyhow::Result<Arc<Page<Playlist>>> {
        if !is_channel_id(channel_id) {
            return Err(bad("채널 주소가 이상합니다"));
        }
        let token = page_token.unwrap_or("").to_string();
        let cid = channel_id.to_string();
        self.cached(format!("cp:{cid}:{token}"), Duration::from_secs(30 * 60), || {
            let mut p = vec![("part", "snippet,contentDetails"), ("channelId", cid.as_str()), ("maxResults", "30")];
            if !token.is_empty() {
                p.push(("pageToken", token.as_str()));
            }
            Ok(page(&self.get("playlists", &p)?, parse_playlist))
        })
    }

    pub fn playlist(&self, id: &str, page_token: Option<&str>) -> anyhow::Result<Arc<PlaylistDetail>> {
        if !is_playlist_id(id) {
            return Err(bad("재생목록 주소가 이상합니다"));
        }
        let token = page_token.unwrap_or("").to_string();
        let pid = id.to_string();
        self.cached(format!("pl:{pid}:{token}"), Duration::from_secs(15 * 60), || {
            let info = self.get("playlists", &[("part", "snippet,contentDetails"), ("id", pid.as_str())])?;
            let playlist = page(&info, parse_playlist)
                .items
                .into_iter()
                .next()
                .ok_or_else(|| ApiError { status: 404, message: "재생목록을 찾을 수 없습니다".into() })?;
            let videos = self.playlist_items(&pid, (!token.is_empty()).then_some(token.as_str()))?.as_ref().clone();
            Ok(PlaylistDetail { playlist, videos })
        })
    }

    /// @handle → 채널(1 할당량)
    pub fn resolve(&self, link: Link) -> anyhow::Result<Link> {
        match link {
            Link::Handle(h) => {
                let json = self.get("channels", &[("part", "id"), ("forHandle", h.as_str())])?;
                let id = json
                    .get("items")
                    .and_then(|a| a.get(0))
                    .and_then(|i| i.get("id"))
                    .and_then(Value::as_str)
                    .filter(|i| is_channel_id(i))
                    .ok_or_else(|| ApiError { status: 404, message: format!("{h} 채널을 찾을 수 없습니다") })?;
                Ok(Link::Channel(id.to_string()))
            }
            other => Ok(other),
        }
    }

    // -----------------------------------------------------------------------
    // 키 없이: 채널 RSS

    /// 채널의 최근 영상 15개(공개 피드)
    pub fn feed(&self, channel_id: &str) -> anyhow::Result<Arc<Feed>> {
        if !is_channel_id(channel_id) {
            return Err(bad("채널 주소가 이상합니다"));
        }
        self.cached(format!("f:{channel_id}"), Duration::from_secs(15 * 60), || self.fetch_feed(channel_id))
    }

    fn fetch_feed(&self, channel_id: &str) -> anyhow::Result<Feed> {
        let (status, body) = self.get_web("feeds/videos.xml", &[("channel_id", channel_id)])?;
        if status == 404 {
            return Err(ApiError { status: 404, message: "채널을 찾을 수 없습니다".into() }.into());
        }
        if status != 200 {
            return Err(ApiError { status, message: format!("채널 피드를 읽지 못했습니다({status})") }.into());
        }
        parse_feed(channel_id, &body).ok_or_else(|| anyhow!("채널 피드를 읽지 못했습니다"))
    }

    // -----------------------------------------------------------------------
    // 구독

    pub fn subscriptions(&self) -> Vec<Subscription> {
        self.saved.lock().subscriptions.clone()
    }

    pub fn is_subscribed(&self, id: &str) -> bool {
        self.saved.lock().subscriptions.iter().any(|s| s.id == id)
    }

    /// 새로 구독했으면 true. 이미 있으면 이름·그림만 새로 적는다.
    pub fn subscribe(&self, sub: Subscription) -> bool {
        if !is_channel_id(&sub.id) {
            return false;
        }
        let mut s = self.saved.lock();
        let fresh = match s.subscriptions.iter_mut().find(|x| x.id == sub.id) {
            Some(x) => {
                if !sub.title.is_empty() {
                    x.title = sub.title;
                }
                if sub.thumb.is_some() {
                    x.thumb = sub.thumb;
                }
                false
            }
            None => {
                s.subscriptions.insert(0, Subscription { added_at: crate::model::now_ms(), ..sub });
                true
            }
        };
        let snapshot = s.clone();
        self.persist(&snapshot);
        fresh
    }

    pub fn unsubscribe(&self, id: &str) -> bool {
        let mut s = self.saved.lock();
        let before = s.subscriptions.len();
        s.subscriptions.retain(|x| x.id != id);
        let changed = s.subscriptions.len() != before;
        if changed {
            let snapshot = s.clone();
            self.persist(&snapshot);
        }
        changed
    }

    /// 테이크아웃 CSV에서 읽은 것을 더한다. 새로 더한 수.
    pub fn import(&self, list: Vec<(String, String)>) -> usize {
        let mut s = self.saved.lock();
        let now = crate::model::now_ms();
        let mut added = 0;
        for (id, title) in list {
            if is_channel_id(&id) && !s.subscriptions.iter().any(|x| x.id == id) {
                s.subscriptions.push(Subscription { id, title, thumb: None, added_at: now });
                added += 1;
            }
        }
        if added > 0 {
            let snapshot = s.clone();
            self.persist(&snapshot);
        }
        added
    }

    /// 구독한 채널들의 새 영상, 새 것부터(채널마다 RSS, 키 없이). `cursor`는 앞 쪽의 `next`.
    ///
    /// 마지막으로 합친 목록을 곧바로 준다(앱을 다시 켜도 디스크에서). 목록이 묵었으면 뒤에서 모두 새로
    /// 읽는다(`refreshing`). 새로 구독한 채널만은 그 자리에서 읽어 더하고, 끊은 채널은 뺀다.
    pub fn subscription_feed(self: &Arc<Self>, cursor: Option<&str>, per: usize) -> SubscriptionFeed {
        let subs: Vec<String> = self.subscriptions().into_iter().map(|s| s.id).collect();
        let (items, failed, stale) = {
            let mut guard = self.merged.lock();
            let m = guard.get_or_insert_with(|| self.load_merged());
            let missing: Vec<String> = subs.iter().filter(|c| !m.channels.contains(c)).cloned().collect();
            if !missing.is_empty() {
                let (feeds, failed) = self.read_feeds(&missing);
                if m.channels.is_empty() {
                    m.at = crate::model::now_ms();
                }
                for f in feeds {
                    m.items.extend(f.videos);
                }
                m.failed.retain(|c| !missing.contains(c));
                m.failed.extend(failed);
                m.channels.extend(missing);
                newest_first(&mut m.items);
                m.items.truncate(FEED_KEEP);
                self.save_merged(m);
            }
            let subscribed: HashSet<&str> = subs.iter().map(String::as_str).collect();
            let items: Vec<Video> =
                m.items.iter().filter(|v| v.channel_id.as_deref().is_some_and(|c| subscribed.contains(c))).cloned().collect();
            let failed: Vec<String> = m.failed.iter().filter(|c| subscribed.contains(c.as_str())).cloned().collect();
            let stale = crate::model::now_ms().saturating_sub(m.at) > FEED_STALE.as_millis() as i64;
            (items, failed, stale)
        };
        let refreshing = if stale && !subs.is_empty() { self.refresh_in_background() } else { self.refreshing.load(Ordering::Acquire) };
        let per = per.clamp(1, 100);
        let start = cursor.map_or(0, |c| after_cursor(&items, c));
        let end = (start + per).min(items.len());
        let next = (end < items.len()).then(|| feed_cursor(&items[end - 1]));
        SubscriptionFeed { items: items[start..end].to_vec(), next, failed, refreshing }
    }

    /// 구독 피드를 뒤에서 새로 읽기 시작한다(이미 읽는 중이면 그대로). 읽는 중이면 true.
    fn refresh_in_background(self: &Arc<Self>) -> bool {
        if self.refreshing.swap(true, Ordering::AcqRel) {
            return true;
        }
        let me = self.clone();
        let spawned = std::thread::Builder::new().name("sori-yt-feed".into()).spawn(move || {
            /// 끝나면(실패해도) 다음 새로 읽기를 막지 않게
            struct Done<'a>(&'a AtomicBool);
            impl Drop for Done<'_> {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Release);
                }
            }
            let _done = Done(&me.refreshing);
            me.refresh_feed();
        });
        if spawned.is_err() {
            self.refreshing.store(false, Ordering::Release);
        }
        spawned.is_ok()
    }

    /// 구독한 채널을 모두 새로 읽어 합친다. 이번에 못 읽은 채널은 앞의 영상을 그대로 둔다.
    fn refresh_feed(&self) {
        let subs: Vec<String> = self.subscriptions().into_iter().map(|s| s.id).collect();
        let (feeds, failed) = self.read_feeds(&subs);
        let mut guard = self.merged.lock();
        let old = guard.take().unwrap_or_default();
        let kept: HashSet<&str> = failed.iter().map(String::as_str).collect();
        let mut items: Vec<Video> = old.items.into_iter().filter(|v| v.channel_id.as_deref().is_some_and(|c| kept.contains(c))).collect();
        for f in feeds {
            items.extend(f.videos);
        }
        newest_first(&mut items);
        items.truncate(FEED_KEEP);
        let m = Merged { at: crate::model::now_ms(), channels: subs, items, failed };
        self.save_merged(&m);
        *guard = Some(m);
    }

    /// 채널 피드들을 여덟 개씩 함께 읽는다(캐시 없이). 읽은 피드와 못 읽은 채널.
    fn read_feeds(&self, ids: &[String]) -> (Vec<Feed>, Vec<String>) {
        let results: Mutex<Vec<(String, anyhow::Result<Feed>)>> = Mutex::new(Vec::new());
        let next = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8.min(ids.len()) {
                scope.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(id) = ids.get(i) else { break };
                        let r = self.fetch_feed(id);
                        results.lock().push((id.clone(), r));
                    }
                });
            }
        });
        let mut feeds = Vec::new();
        let mut failed = Vec::new();
        for (id, r) in results.into_inner() {
            match r {
                Ok(f) => feeds.push(f),
                Err(e) => {
                    tracing::info!(channel = id, "구독 피드를 읽지 못했습니다: {e:#}");
                    failed.push(id);
                }
            }
        }
        (feeds, failed)
    }

    fn load_merged(&self) -> Merged {
        std::fs::read(&self.feed_file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn save_merged(&self, m: &Merged) {
        if let Ok(bytes) = serde_json::to_vec(m)
            && let Err(e) = crate::config::write_atomic(&self.feed_file, &bytes)
        {
            tracing::warn!("구독 피드를 적지 못했습니다: {e}");
        }
    }
}
