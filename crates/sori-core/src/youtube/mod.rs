//! 유튜브: 공식 경로로만 본다.
//!
//! - 보는 것은 유튜브의 공식 플레이어(IFrame)가 화면에서 한다. 소리 엔진은 영상이나 소리를 받지
//!   않고, 꺼내지도, 저장하지도 않는다(유튜브 약관과 API 정책).
//! - 검색·영상 정보·댓글·채널·재생목록은 YouTube Data API v3(사용자의 API 키)로 묻는다.
//! - 키가 없어도 되는 것: 구독한 채널의 새 영상(채널의 공개 RSS 피드), 영상 제목·채널 이름(oEmbed).
//! - 구독은 이 기기에 둔다(채널 ID와 이름). 구글 테이크아웃의 subscriptions.csv를 가져올 수 있다.
//!
//! 이 파일은 응답을 화면이 쓰는 모양으로 옮기는 일(네트워크 없음)이고, 묻는 쪽은 [`client`]다.

pub mod client;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use client::YouTube;

pub const API: &str = "https://www.googleapis.com/youtube/v3";
pub const WEB: &str = "https://www.youtube.com";

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    pub channel_title: String,
    /// RFC 3339
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
    /// 초
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub views: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comments: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// "live" 지금 방송 중, "upcoming" 예정
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<String>,
    /// 쇼츠
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub short: bool,
    /// 다른 곳(이 앱)에서 틀 수 있는가. 모르면 None.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeddable: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub title: String,
    /// "@handle"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribers: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub videos: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub views: Option<u64>,
    /// 올린 영상 재생목록(UU…)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploads: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    pub channel_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub author: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_channel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_avatar: Option<String>,
    pub text: String,
    pub likes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    /// 고친 적이 있으면
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub edited: bool,
    /// 답글 수(맨 위 댓글만)
    pub replies: u64,
    /// 미리 딸려 온 답글 몇 개
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reply_preview: Vec<Comment>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SearchItem {
    Video(Video),
    Channel(Channel),
    Playlist(Playlist),
}

/// 쪽으로 나뉜 목록. `next`를 다음 쪽 요청에 그대로 넣는다.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

/// 구독한 채널(이 기기에 둔다)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
    #[serde(default)]
    pub added_at: i64,
}

/// 붙여 넣은 유튜브 링크가 가리키는 것
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum Link {
    Video(String),
    Channel(String),
    /// "@handle"(채널 ID는 API로 묻는다)
    Handle(String),
    Playlist(String),
}

// ---------------------------------------------------------------------------
// 링크와 ID

pub fn is_video_id(s: &str) -> bool {
    s.len() == 11 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn is_channel_id(s: &str) -> bool {
    s.len() == 24 && s.starts_with("UC") && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn is_playlist_id(s: &str) -> bool {
    (2..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn is_handle(s: &str) -> bool {
    let h = s.strip_prefix('@').unwrap_or(s);
    (3..=30).contains(&h.chars().count()) && h.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// 유튜브 링크(watch, youtu.be, shorts, embed, live, channel, @handle, playlist)를 읽는다.
pub fn parse_link(text: &str) -> Option<Link> {
    let t = text.trim();
    if is_video_id(t) {
        return Some(Link::Video(t.to_string()));
    }
    if is_channel_id(t) {
        return Some(Link::Channel(t.to_string()));
    }
    if t.starts_with('@') && is_handle(t) {
        return Some(Link::Handle(t.to_string()));
    }
    let rest = t.strip_prefix("https://").or_else(|| t.strip_prefix("http://")).unwrap_or(t);
    let (host, tail) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").or_else(|| host.strip_prefix("m.")).or_else(|| host.strip_prefix("music.")).unwrap_or(&host);
    let (path, query) = tail.split_once('?').unwrap_or((tail, ""));
    let query = query.split('#').next().unwrap_or("");
    let param = |name: &str| form_urlencoded::parse(query.as_bytes()).find(|(k, _)| k == name).map(|(_, v)| v.into_owned());
    let parts: Vec<&str> = path.split('#').next().unwrap_or("").split('/').filter(|p| !p.is_empty()).collect();
    match host {
        "youtu.be" => parts.first().filter(|id| is_video_id(id)).map(|id| Link::Video(id.to_string())),
        "youtube.com" | "youtube-nocookie.com" => match parts.as_slice() {
            ["watch", ..] => match (param("v"), param("list")) {
                (Some(v), _) if is_video_id(&v) => Some(Link::Video(v)),
                (_, Some(l)) if is_playlist_id(&l) => Some(Link::Playlist(l)),
                _ => None,
            },
            ["playlist", ..] => param("list").filter(|l| is_playlist_id(l)).map(Link::Playlist),
            ["shorts" | "embed" | "live" | "v" | "e", id, ..] if is_video_id(id) => Some(Link::Video(id.to_string())),
            ["channel", id, ..] if is_channel_id(id) => Some(Link::Channel(id.to_string())),
            [h, ..] if h.starts_with('@') => {
                let h = percent_decode(h);
                is_handle(&h).then_some(Link::Handle(h))
            }
            _ => None,
        },
        _ => None,
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------------------------------------------------------------------------
// 값 읽기

/// ISO 8601 길이("PT1H2M3S", "P1DT2H") → 초
pub fn parse_duration(s: &str) -> Option<f64> {
    let s = s.strip_prefix('P')?;
    let (days, time) = match s.split_once('T') {
        Some((d, t)) => (d, t),
        None => (s, ""),
    };
    let mut total = 0.0;
    let mut num = String::new();
    for (part, units) in [(days, [('W', 604_800.0), ('D', 86_400.0), ('_', 0.0)]), (time, [('H', 3600.0), ('M', 60.0), ('S', 1.0)])] {
        num.clear();
        for c in part.chars() {
            if c.is_ascii_digit() || c == '.' {
                num.push(c);
            } else {
                let mult = units.iter().find(|(u, _)| *u == c)?.1;
                total += num.parse::<f64>().ok()? * mult;
                num.clear();
            }
        }
        if !num.is_empty() {
            return None;
        }
    }
    Some(total)
}

/// 유튜브는 수를 문자열로 준다("1234").
fn num(v: Option<&Value>) -> Option<u64> {
    match v? {
        Value::String(s) => s.trim().parse().ok(),
        Value::Number(n) => n.as_u64(),
        _ => None,
    }
}

fn text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// 썸네일 중 `prefer` 순서로 처음 있는 것
fn thumb(v: Option<&Value>, prefer: &[&str]) -> Option<String> {
    let t = v?;
    prefer.iter().find_map(|k| t.get(*k).and_then(|x| x.get("url")).and_then(Value::as_str)).map(https)
}

/// 구글의 그림 주소는 https로(앱 화면이 섞인 콘텐츠를 막지 않게). 다른 곳은 그대로.
fn https(u: &str) -> String {
    if let Some(rest) = u.strip_prefix("//") {
        return format!("https://{rest}");
    }
    match u.strip_prefix("http://") {
        Some(rest) => {
            let host = rest.split('/').next().unwrap_or("");
            if ["ytimg.com", "ggpht.com", "googleusercontent.com", "youtube.com"]
                .iter()
                .any(|h| host == *h || host.ends_with(&format!(".{h}")))
            {
                format!("https://{rest}")
            } else {
                u.to_string()
            }
        }
        None => u.to_string(),
    }
}

const VIDEO_THUMBS: &[&str] = &["maxres", "standard", "high", "medium", "default"];
const SMALL_THUMBS: &[&str] = &["high", "medium", "default"];
const AVATARS: &[&str] = &["medium", "high", "default"];

/// search.list는 제목에 HTML 엔티티를 남긴다(&#39; &quot; &amp;).
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.find(';').filter(|&e| e <= 10);
        let decoded = end.and_then(|e| {
            let name = &tail[1..e];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => name
                    .strip_prefix("#x")
                    .or_else(|| name.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            };
            c.map(|c| (c, e))
        });
        match decoded {
            Some((c, e)) => {
                out.push(c);
                rest = &tail[e + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// Data API 응답 → 모양

/// videos.list(snippet, contentDetails, statistics, status)의 한 항목
pub fn parse_video(item: &Value) -> Option<Video> {
    let id = item.get("id").and_then(Value::as_str).filter(|i| is_video_id(i))?.to_string();
    let sn = item.get("snippet");
    let get = |k: &str| sn.and_then(|s| s.get(k));
    let cd = item.get("contentDetails");
    let st = item.get("statistics");
    let live = text(get("liveBroadcastContent")).filter(|l| l != "none");
    let duration = cd.and_then(|c| c.get("duration")).and_then(Value::as_str).and_then(parse_duration);
    Some(Video {
        title: text(get("title")).unwrap_or_else(|| id.clone()),
        channel_id: text(get("channelId")),
        channel_title: text(get("channelTitle")).unwrap_or_default(),
        published: text(get("publishedAt")),
        thumb: thumb(get("thumbnails"), VIDEO_THUMBS),
        // 방송 중인 것은 길이가 0으로 온다
        duration: duration.filter(|d| *d > 0.0),
        views: num(st.and_then(|s| s.get("viewCount"))),
        likes: num(st.and_then(|s| s.get("likeCount"))),
        comments: num(st.and_then(|s| s.get("commentCount"))),
        description: text(get("description")),
        live,
        short: false,
        embeddable: item.get("status").and_then(|s| s.get("embeddable")).and_then(Value::as_bool),
        id,
    })
}

pub fn parse_channel(item: &Value) -> Option<Channel> {
    let id = item.get("id").and_then(Value::as_str).filter(|i| is_channel_id(i))?.to_string();
    let sn = item.get("snippet");
    let get = |k: &str| sn.and_then(|s| s.get(k));
    let st = item.get("statistics");
    let hidden = st.and_then(|s| s.get("hiddenSubscriberCount")).and_then(Value::as_bool).unwrap_or(false);
    Some(Channel {
        title: text(get("title")).unwrap_or_else(|| id.clone()),
        handle: text(get("customUrl")).map(|h| if h.starts_with('@') { h } else { format!("@{h}") }),
        description: text(get("description")),
        thumb: thumb(get("thumbnails"), AVATARS),
        banner: item
            .get("brandingSettings")
            .and_then(|b| b.get("image"))
            .and_then(|i| i.get("bannerExternalUrl"))
            .and_then(Value::as_str)
            // 원본은 2560×1440이다. 화면은 가운데 띠만 보이므로 너비만 줄여 받는다.
            .map(|u| format!("{}=w1600", https(u))),
        subscribers: if hidden { None } else { num(st.and_then(|s| s.get("subscriberCount"))) },
        videos: num(st.and_then(|s| s.get("videoCount"))),
        views: num(st.and_then(|s| s.get("viewCount"))),
        uploads: item
            .get("contentDetails")
            .and_then(|c| c.get("relatedPlaylists"))
            .and_then(|r| r.get("uploads"))
            .and_then(Value::as_str)
            .map(str::to_string),
        id,
    })
}

pub fn parse_playlist(item: &Value) -> Option<Playlist> {
    let id = item.get("id").and_then(Value::as_str).filter(|i| is_playlist_id(i))?.to_string();
    let sn = item.get("snippet");
    let get = |k: &str| sn.and_then(|s| s.get(k));
    Some(Playlist {
        title: text(get("title")).unwrap_or_else(|| id.clone()),
        channel_id: text(get("channelId")),
        channel_title: text(get("channelTitle")).unwrap_or_default(),
        thumb: thumb(get("thumbnails"), SMALL_THUMBS),
        count: num(item.get("contentDetails").and_then(|c| c.get("itemCount"))),
        description: text(get("description")),
        id,
    })
}

/// search.list의 한 항목. 제목 같은 글자는 HTML 엔티티를 푼다. 자세한 것(길이, 조회수)은 따로 묻는다.
pub fn parse_search_item(item: &Value) -> Option<SearchItem> {
    let id = item.get("id")?;
    let sn = item.get("snippet");
    let get = |k: &str| sn.and_then(|s| s.get(k));
    let t = |k: &str| text(get(k)).map(|s| unescape(&s));
    match id.get("kind").and_then(Value::as_str)? {
        "youtube#video" => {
            let vid = id.get("videoId").and_then(Value::as_str).filter(|i| is_video_id(i))?.to_string();
            Some(SearchItem::Video(Video {
                title: t("title").unwrap_or_else(|| vid.clone()),
                channel_id: text(get("channelId")),
                channel_title: t("channelTitle").unwrap_or_default(),
                published: text(get("publishedAt")),
                thumb: thumb(get("thumbnails"), SMALL_THUMBS),
                description: t("description"),
                live: text(get("liveBroadcastContent")).filter(|l| l != "none"),
                id: vid,
                ..Default::default()
            }))
        }
        "youtube#channel" => {
            let cid = id.get("channelId").and_then(Value::as_str).filter(|i| is_channel_id(i))?.to_string();
            Some(SearchItem::Channel(Channel {
                title: t("title").unwrap_or_else(|| cid.clone()),
                description: t("description"),
                thumb: thumb(get("thumbnails"), AVATARS),
                id: cid,
                ..Default::default()
            }))
        }
        "youtube#playlist" => {
            let pid = id.get("playlistId").and_then(Value::as_str).filter(|i| is_playlist_id(i))?.to_string();
            Some(SearchItem::Playlist(Playlist {
                title: t("title").unwrap_or_else(|| pid.clone()),
                channel_id: text(get("channelId")),
                channel_title: t("channelTitle").unwrap_or_default(),
                thumb: thumb(get("thumbnails"), SMALL_THUMBS),
                description: t("description"),
                id: pid,
                ..Default::default()
            }))
        }
        _ => None,
    }
}

/// playlistItems.list의 한 항목(영상). 지워졌거나 비공개가 된 것은 뺀다.
pub fn parse_playlist_item(item: &Value) -> Option<Video> {
    let sn = item.get("snippet")?;
    let vid = sn
        .get("resourceId")
        .and_then(|r| r.get("videoId"))
        .or_else(|| item.get("contentDetails").and_then(|c| c.get("videoId")))
        .and_then(Value::as_str)
        .filter(|i| is_video_id(i))?
        .to_string();
    let title = text(sn.get("title"))?;
    if title == "Private video" || title == "Deleted video" {
        return None;
    }
    Some(Video {
        title,
        channel_id: text(sn.get("videoOwnerChannelId")).or_else(|| text(sn.get("channelId"))),
        channel_title: text(sn.get("videoOwnerChannelTitle")).or_else(|| text(sn.get("channelTitle"))).unwrap_or_default(),
        published: item.get("contentDetails").and_then(|c| text(c.get("videoPublishedAt"))).or_else(|| text(sn.get("publishedAt"))),
        thumb: thumb(sn.get("thumbnails"), SMALL_THUMBS),
        id: vid,
        ..Default::default()
    })
}

fn parse_comment_snippet(id: &str, sn: &Value) -> Comment {
    let published = text(sn.get("publishedAt"));
    let updated = text(sn.get("updatedAt"));
    Comment {
        id: id.to_string(),
        author: text(sn.get("authorDisplayName")).unwrap_or_default(),
        author_channel_id: sn.get("authorChannelId").and_then(|a| a.get("value")).and_then(Value::as_str).map(str::to_string),
        author_avatar: text(sn.get("authorProfileImageUrl")).map(|u| https(&u)),
        // textFormat=plainText로 물으므로 textDisplay가 그대로 읽을 글이다.
        text: text(sn.get("textDisplay")).or_else(|| text(sn.get("textOriginal"))).map(|t| unescape(&t)).unwrap_or_default(),
        likes: num(sn.get("likeCount")).unwrap_or(0),
        edited: published.is_some() && updated.is_some() && published != updated,
        published,
        replies: 0,
        reply_preview: Vec::new(),
    }
}

/// commentThreads.list의 한 항목
pub fn parse_thread(item: &Value) -> Option<Comment> {
    let sn = item.get("snippet")?;
    let top = sn.get("topLevelComment")?;
    let id = top.get("id").and_then(Value::as_str)?;
    let mut c = parse_comment_snippet(id, top.get("snippet")?);
    c.replies = num(sn.get("totalReplyCount")).unwrap_or(0);
    c.reply_preview = item
        .get("replies")
        .and_then(|r| r.get("comments"))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(parse_reply).collect())
        .unwrap_or_default();
    Some(c)
}

/// comments.list의 한 항목(답글)
pub fn parse_reply(item: &Value) -> Option<Comment> {
    let id = item.get("id").and_then(Value::as_str)?;
    Some(parse_comment_snippet(id, item.get("snippet")?))
}

pub fn page<T>(json: &Value, parse: impl Fn(&Value) -> Option<T>) -> Page<T> {
    Page {
        items: json.get("items").and_then(Value::as_array).map(|a| a.iter().filter_map(&parse).collect()).unwrap_or_default(),
        next: text(json.get("nextPageToken")),
        total: num(json.get("pageInfo").and_then(|p| p.get("totalResults"))),
    }
}

/// API 오류 → 사람이 읽는 말
pub fn api_error(status: u16, json: &Value) -> String {
    let e = json.get("error");
    let reason = e.and_then(|e| e.get("errors")).and_then(|a| a.get(0)).and_then(|x| x.get("reason")).and_then(Value::as_str).unwrap_or("");
    let message = e.and_then(|e| e.get("message")).and_then(Value::as_str).unwrap_or("");
    match reason {
        "quotaExceeded" | "dailyLimitExceeded" => {
            "오늘 쓸 수 있는 유튜브 API 할당량을 다 썼습니다. 한국 시간 오후 4–5시쯤 다시 채워집니다".into()
        }
        "keyInvalid" => "유튜브 API 키가 맞지 않습니다".into(),
        "accessNotConfigured" | "SERVICE_DISABLED" => "Google Cloud에서 이 프로젝트의 YouTube Data API v3를 켜 주세요".into(),
        "commentsDisabled" => "이 영상은 댓글을 막아 두었습니다".into(),
        "videoNotFound" | "channelNotFound" | "playlistNotFound" | "commentThreadNotFound" => "찾을 수 없습니다".into(),
        "forbidden" if message.contains("comment") => "이 영상의 댓글은 볼 수 없습니다".into(),
        _ if message.contains("API key not valid") => "유튜브 API 키가 맞지 않습니다".into(),
        _ if message.contains("blocked") || message.contains("referer") || message.contains("restricted") => {
            "API 키에 걸어 둔 제한(앱·웹사이트·IP) 때문에 막혔습니다. Google Cloud에서 키의 제한을 '없음'으로 두세요".into()
        }
        _ if !message.is_empty() => format!("유튜브가 거절했습니다({status}): {message}"),
        _ => format!("유튜브가 거절했습니다({status})"),
    }
}

// ---------------------------------------------------------------------------
// oEmbed(키 없이 영상 제목·채널 이름)

pub fn parse_oembed(id: &str, json: &Value) -> Option<Video> {
    let title = text(json.get("title"))?;
    Some(Video {
        id: id.to_string(),
        title,
        channel_title: text(json.get("author_name")).unwrap_or_default(),
        thumb: Some(format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")),
        ..Default::default()
    })
}

/// oEmbed의 author_url(https://www.youtube.com/@handle 또는 /channel/UC…)에서 채널을 읽는다.
pub fn oembed_author(json: &Value) -> Option<Link> {
    let url = json.get("author_url").and_then(Value::as_str)?;
    match parse_link(url)? {
        l @ (Link::Channel(_) | Link::Handle(_)) => Some(l),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 채널 RSS(키 없이 새 영상 15개)

/// 채널의 공개 피드(Atom). 기계가 만든 고른 XML이라 필요한 태그만 읽는다.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feed {
    pub channel_id: String,
    pub title: String,
    pub videos: Vec<Video>,
}

fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}");
    let mut from = 0;
    loop {
        let i = from + xml[from..].find(&open)?;
        let after = &xml[i + open.len()..];
        // <title> 과 <titles> 를 가르기
        match after.chars().next() {
            Some('>') | Some(' ') | Some('/') | Some('\n') | Some('\t') | Some('\r') => {}
            _ => {
                from = i + open.len();
                continue;
            }
        }
        let gt = after.find('>')?;
        if after[..gt].ends_with('/') {
            return Some("");
        }
        let body = &after[gt + 1..];
        let close = format!("</{name}>");
        let end = body.find(&close)?;
        return Some(&body[..end]);
    }
}

fn attr<'a>(xml: &'a str, name: &str, key: &str) -> Option<&'a str> {
    let open = format!("<{name} ");
    let i = xml.find(&open)?;
    let rest = &xml[i + open.len()..];
    let el = &rest[..rest.find('>')?];
    let k = format!("{key}=\"");
    let j = el.find(&k)?;
    let v = &el[j + k.len()..];
    Some(&v[..v.find('"')?])
}

fn cdata_or_text(s: &str) -> String {
    let s = s.trim();
    match s.strip_prefix("<![CDATA[").and_then(|x| x.strip_suffix("]]>")) {
        Some(c) => c.to_string(),
        None => unescape(s),
    }
}

pub fn parse_feed(channel_id: &str, xml: &str) -> Option<Feed> {
    if !xml.contains("<feed") {
        return None;
    }
    let head_end = xml.find("<entry").unwrap_or(xml.len());
    let head = &xml[..head_end];
    let title = tag(head, "title").map(cdata_or_text).unwrap_or_default();
    let mut videos = Vec::new();
    let mut rest = &xml[head_end..];
    while let Some(i) = rest.find("<entry>") {
        let after = &rest[i + 7..];
        let Some(end) = after.find("</entry>") else { break };
        let e = &after[..end];
        rest = &after[end + 8..];
        let Some(id) = tag(e, "yt:videoId").map(str::trim).filter(|v| is_video_id(v)) else { continue };
        let link = attr(e, "link", "href").unwrap_or("");
        let views = attr(e, "media:statistics", "views").and_then(|v| v.parse().ok());
        let likes = attr(e, "media:starRating", "count").and_then(|v| v.parse().ok());
        videos.push(Video {
            id: id.to_string(),
            title: tag(e, "title").map(cdata_or_text).unwrap_or_else(|| id.to_string()),
            channel_id: tag(e, "yt:channelId")
                .map(str::trim)
                .filter(|c| is_channel_id(c))
                .map(str::to_string)
                .or(Some(channel_id.to_string())),
            channel_title: tag(e, "name").map(cdata_or_text).unwrap_or_else(|| title.clone()),
            published: tag(e, "published").map(|p| p.trim().to_string()),
            thumb: attr(e, "media:thumbnail", "url").map(https),
            views,
            likes,
            description: tag(e, "media:description").map(cdata_or_text).filter(|d| !d.is_empty()),
            short: link.contains("/shorts/"),
            ..Default::default()
        });
    }
    Some(Feed { channel_id: channel_id.to_string(), title, videos })
}

// ---------------------------------------------------------------------------
// 구글 테이크아웃 subscriptions.csv

/// "Channel Id,Channel Url,Channel Title"(나라마다 머리말 말이 다르다). 채널 ID가 있는 줄만 읽는다.
pub fn parse_takeout_csv(csv: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in csv.lines() {
        let cols = csv_fields(line);
        let Some(id) = cols.iter().map(|c| c.trim()).find(|c| is_channel_id(c)) else { continue };
        let title = cols.get(2).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| id.to_string());
        if !out.iter().any(|(i, _)| i == id) {
            out.push((id.to_string(), title));
        }
    }
    out
}

fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

#[cfg(test)]
mod tests;
