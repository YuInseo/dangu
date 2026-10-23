//! 채팅 채널: 전체 · 채널 · 주변 · 거래 · 길드 · 관리자.
//!
//! - 그냥 치면 지금 고른 채널로 갑니다 (처음엔 `채널` = 지금 인스턴스).
//! - 앞에 기호를 붙이면 그 한 번만 다른 채널로: `!전체`, `$거래`, `@길드`, `#관리자`, `~주변`.
//! - `/채널 <이름>`으로 기본 채널을 바꾸고, `/채널 끄기 <이름>`으로 그 채널을 안 받습니다.
//!
//! | 채널 | 누가 받나 | 넘나드는 범위 |
//! |---|---|---|
//! | 전체   | 모두 | 모든 인스턴스(담비 자식 프로세스 포함) |
//! | 채널   | 같은 인스턴스의 모두 | 이 인스턴스 |
//! | 주변   | 같은 차원 64칸 안 | 이 인스턴스 |
//! | 거래   | 모두 (끈 사람 빼고) | 모든 인스턴스 |
//! | 길드   | 같은 길드원 | 모든 인스턴스 |
//! | 관리자 | `dangu.chat.staff` 권한(기본 OP 2) | 모든 인스턴스 |
//!
//! 채널 채팅은 서명 없는 시스템 메시지로 갑니다 (앞에 `[전체]` 같은 머리를 붙여야 해서).
//! `MC_CHAT_CHANNELS=0`이면 채널 없이 바닐라 서명 채팅 그대로.

pub mod log;
pub mod session;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::json::{self, Json};
use crate::protocol::nbt::Tag;
use crate::server::{Event, Server};
use crate::text;
use crate::world::Dimension;

/// 담비 전역 버스에서 채널 채팅이 오가는 이름.
pub const BUS: &str = "dangu:chat";
/// `주변` 채널이 닿는 거리 (블록).
pub const NEAR_RANGE: f64 = 64.0;
/// 관리자 채널 권한.
pub const STAFF_NODE: &str = "dangu.chat.staff";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Global,
    Local,
    Near,
    Trade,
    Guild,
    Staff,
}

impl Channel {
    pub const ALL: [Channel; 6] = [
        Channel::Global,
        Channel::Local,
        Channel::Near,
        Channel::Trade,
        Channel::Guild,
        Channel::Staff,
    ];

    /// 화면에 보이는 이름.
    pub fn name(self) -> &'static str {
        match self {
            Channel::Global => "전체",
            Channel::Local => "채널",
            Channel::Near => "주변",
            Channel::Trade => "거래",
            Channel::Guild => "길드",
            Channel::Staff => "관리자",
        }
    }

    /// 저장·버스용 영어 이름.
    pub fn key(self) -> &'static str {
        match self {
            Channel::Global => "global",
            Channel::Local => "local",
            Channel::Near => "near",
            Channel::Trade => "trade",
            Channel::Guild => "guild",
            Channel::Staff => "staff",
        }
    }

    /// 한 번만 이 채널로 보낼 때 앞에 붙이는 기호.
    pub fn prefix(self) -> char {
        match self {
            Channel::Global => '!',
            Channel::Local => '%',
            Channel::Near => '~',
            Channel::Trade => '$',
            Channel::Guild => '@',
            Channel::Staff => '#',
        }
    }

    /// 머리말 색과 글 색.
    pub fn colors(self) -> (&'static str, &'static str) {
        match self {
            Channel::Global => ("#ffb347", "#fff1dc"),
            Channel::Local => ("#7ec8ff", "white"),
            Channel::Near => ("#b0b0b0", "#e8e8e8"),
            Channel::Trade => ("#7ddc7d", "#dcffdc"),
            Channel::Guild => ("#5ee6c8", "#d6fff5"),
            Channel::Staff => ("#ff6b6b", "#ffd6d6"),
        }
    }

    /// 같은 사람이 이 채널에 다시 말할 수 있기까지 (도배 막기).
    pub fn slow_mode(self) -> Duration {
        match self {
            Channel::Global => Duration::from_secs(3),
            Channel::Trade => Duration::from_secs(10),
            _ => Duration::ZERO,
        }
    }

    /// 다른 인스턴스로도 가는가.
    pub fn network_wide(self) -> bool {
        !matches!(self, Channel::Local | Channel::Near)
    }

    /// `전체`, `global`, `g`, `!` 무엇이든.
    pub fn parse(s: &str) -> Option<Channel> {
        let s = s.trim().to_lowercase();
        Channel::ALL.into_iter().find(|c| {
            s == c.name()
                || s == c.key()
                || s == c.prefix().to_string()
                || s == c.key()[..1]
                || (*c == Channel::Global && s == "all")
                || (*c == Channel::Local && (s == "ch" || s == "instance"))
                || (*c == Channel::Staff && (s == "admin" || s == "op"))
        })
    }

    /// 앞 기호를 떼어 냅니다: `!안녕` → (전체, `안녕`). 기호뿐이거나 없으면 None.
    pub fn split_prefix(message: &str) -> Option<(Channel, &str)> {
        let first = message.chars().next()?;
        let c = Channel::ALL.into_iter().find(|c| c.prefix() == first)?;
        let rest = message[first.len_utf8()..].trim_start();
        (!rest.is_empty()).then_some((c, rest))
    }
}

/// 채널로 보낸 말 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub channel: Channel,
    pub sender: u128,
    pub sender_name: String,
    /// 보낸 사람이 있던 인스턴스.
    pub instance: String,
    pub dimension: Dimension,
    pub position: (f64, f64, f64),
    /// 길드 채널이면 길드 이름.
    pub guild: Option<String>,
    pub content: String,
    /// 보낸 사람 소개 (머리말과 이름 위 설명).
    pub card: Card,
}

/// 채팅 머리말 `[직업].(Lv.레벨)`과, 이름에 마우스를 올리면 보이는 줄들.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Card {
    /// 대표 직업 (가장 높은 레벨).
    pub job: Option<String>,
    pub level: u32,
    /// 설명 줄들 (`이름표\t값` 이면 이름표를 회색으로).
    pub details: Vec<String>,
    /// 후원 등급 (이름, 색): 머리말 앞에 `[VIP]`.
    pub badge: Option<(String, String)>,
}

/// 받는 사람 쪽에서 본 자기 자신.
pub struct Receiver<'a> {
    pub uuid: u128,
    pub instance: &'a str,
    pub dimension: Dimension,
    pub position: (f64, f64, f64),
    pub staff: bool,
    pub guild: Option<&'a str>,
    pub muted: &'a HashSet<Channel>,
}

/// 이 사람이 이 말을 받나.
pub fn reaches(m: &Message, r: &Receiver) -> bool {
    if m.sender == r.uuid {
        return true;
    }
    if r.muted.contains(&m.channel) {
        return false;
    }
    match m.channel {
        Channel::Global | Channel::Trade => true,
        Channel::Local => m.instance == r.instance,
        Channel::Near => {
            m.instance == r.instance && m.dimension == r.dimension && {
                let (dx, dy, dz) = (
                    m.position.0 - r.position.0,
                    m.position.1 - r.position.1,
                    m.position.2 - r.position.2,
                );
                dx * dx + dy * dy + dz * dz <= NEAR_RANGE * NEAR_RANGE
            }
        }
        Channel::Guild => m.guild.is_some() && m.guild.as_deref() == r.guild,
        Channel::Staff => r.staff,
    }
}

/// `[전체] [광부].(Lv.12) 이름 : 내용`. 이름에 마우스를 올리면 그 사람 소개, 누르면 귓속말 준비.
pub fn render(m: &Message) -> Tag {
    let (head, body) = m.channel.colors();
    let mut tag = format!("[{}]", m.channel.name());
    if let (Channel::Guild, Some(g)) = (m.channel, &m.guild) {
        tag = format!("[{g}]");
    }
    let mut parts = vec![text::colored(&format!("{tag} "), head)];
    if let Some((name, color)) = &m.card.badge {
        parts.push(text::colored(&format!("[{name}] "), color));
    }
    let job = m.card.job.as_deref().unwrap_or("무직");
    parts.push(text::colored(&format!("[{job}]"), "#c9b3ff"));
    parts.push(text::colored(".", "dark_gray"));
    parts.push(text::colored(
        &format!("(Lv.{}) ", m.card.level.max(1)),
        "#ffd866",
    ));
    let mut hover = vec![text::colored(&m.sender_name, "gold")];
    for d in &m.card.details {
        hover.push(text::plain("\n"));
        match d.split_once('\t') {
            Some((k, v)) => {
                hover.push(text::colored(&format!("{k}  "), "gray"));
                hover.push(text::colored(v, "white"));
            }
            None => hover.push(text::colored(d, "white")),
        }
    }
    hover.push(text::colored("\n누르면 귓속말", "dark_gray"));
    let name = text::with_hover(
        text::colored(&m.sender_name, "white"),
        Some(text::join(hover)),
        Some(&format!("/msg {} ", m.sender_name)),
    );
    parts.push(name);
    parts.push(text::colored(" : ", "gray"));
    parts.push(text::colored(&m.content, body));
    text::join(parts)
}

// ---- 사람마다의 설정 ----

/// 고른 채널, 끈 채널, 도배 막기 시각. 프로세스 안에서만 기억합니다 (재접속해도 유지, 껐다
/// 켜거나 담비에서 다른 프로세스로 옮기면 처음으로).
#[derive(Debug, Clone, Default)]
pub struct Prefs {
    pub channel: Option<Channel>,
    pub muted: HashSet<Channel>,
    last_sent: HashMap<Channel, Instant>,
}

impl Prefs {
    pub fn current(&self) -> Channel {
        self.channel.unwrap_or(Channel::Local)
    }
}

static PREFS: Mutex<Option<HashMap<u128, Prefs>>> = Mutex::new(None);

pub fn prefs(uuid: u128) -> Prefs {
    PREFS
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .get(&uuid)
        .cloned()
        .unwrap_or_default()
}

pub fn update_prefs(uuid: u128, f: impl FnOnce(&mut Prefs)) {
    let mut g = PREFS.lock().unwrap();
    f(g.get_or_insert_with(HashMap::new).entry(uuid).or_default());
}

/// 도배 막기: 보낼 수 있으면 시각을 적고 None, 아니면 남은 시간.
pub fn throttle(uuid: u128, channel: Channel, now: Instant) -> Option<Duration> {
    let wait = channel.slow_mode();
    if wait.is_zero() {
        return None;
    }
    let mut left = None;
    update_prefs(uuid, |p| match p.last_sent.get(&channel) {
        Some(&t) if now.duration_since(t) < wait => left = Some(wait - now.duration_since(t)),
        _ => {
            p.last_sent.insert(channel, now);
        }
    });
    left
}

// ---- 보내기 ----

/// 말을 알맞은 곳에 내보냅니다. 이 인스턴스만 → 이 서버, 넘나드는 채널 → 담비 버스 또는 같은
/// 프로세스의 모든 인스턴스.
pub fn send(server: &Server, m: Message) {
    if !m.channel.network_wide() {
        server.publish(Event::ChannelChat(Arc::new(m)));
    } else if crate::dambi::child::active() {
        crate::dambi::child::publish(BUS, &to_json(&m).compact());
    } else {
        server.publish_all(Event::ChannelChat(Arc::new(m)));
    }
}

/// 담비 버스로 받은 말 (마스터가 모든 자식에게 돌림).
pub fn receive(server: &Server, payload: &str) {
    if let Some(m) = json::parse(payload).ok().as_ref().and_then(from_json) {
        server.publish(Event::ChannelChat(Arc::new(m)));
    }
}

fn to_json(m: &Message) -> Json {
    let mut f = vec![
        ("channel".into(), Json::String(m.channel.key().into())),
        ("sender".into(), Json::String(format!("{:032x}", m.sender))),
        ("name".into(), Json::String(m.sender_name.clone())),
        ("instance".into(), Json::String(m.instance.clone())),
        ("content".into(), Json::String(m.content.clone())),
    ];
    if let Some(g) = &m.guild {
        f.push(("guild".into(), Json::String(g.clone())));
    }
    if let Some((name, color)) = &m.card.badge {
        f.push(("badge".into(), Json::String(name.clone())));
        f.push(("badge_color".into(), Json::String(color.clone())));
    }
    if let Some(j) = &m.card.job {
        f.push(("job".into(), Json::String(j.clone())));
    }
    f.push(("level".into(), Json::Number(f64::from(m.card.level))));
    f.push((
        "details".into(),
        Json::Array(
            m.card
                .details
                .iter()
                .map(|d| Json::String(d.clone()))
                .collect(),
        ),
    ));
    Json::Object(f)
}

fn from_json(j: &Json) -> Option<Message> {
    let s = |k: &str| j.get(k).and_then(Json::as_str);
    Some(Message {
        channel: Channel::parse(s("channel")?)?,
        sender: u128::from_str_radix(s("sender")?, 16).ok()?,
        sender_name: s("name")?.to_owned(),
        instance: s("instance")?.to_owned(),
        // 넘나드는 채널은 위치를 쓰지 않습니다
        dimension: Dimension::Overworld,
        position: (0.0, 0.0, 0.0),
        guild: s("guild").map(str::to_owned),
        content: s("content")?.to_owned(),
        card: Card {
            badge: s("badge")
                .map(|b| (b.to_owned(), s("badge_color").unwrap_or("gold").to_owned())),
            job: s("job").map(str::to_owned),
            level: j.get("level").and_then(Json::as_f64).unwrap_or(1.0) as u32,
            details: j
                .get("details")
                .and_then(Json::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Json::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(channel: Channel) -> Message {
        Message {
            channel,
            sender: 1,
            sender_name: "Alex".into(),
            instance: "lobby".into(),
            dimension: Dimension::Overworld,
            position: (0.0, 64.0, 0.0),
            guild: Some("달빛".into()),
            content: "안녕".into(),
            card: Card {
                job: Some("광부".into()),
                level: 12,
                details: vec!["길드\t달빛 · 길드원".into()],
                badge: Some(("VIP".into(), "#ffd866".into())),
            },
        }
    }

    #[test]
    fn prefixes_and_names_parse() {
        assert_eq!(
            Channel::split_prefix("!안녕"),
            Some((Channel::Global, "안녕"))
        );
        assert_eq!(
            Channel::split_prefix("$ 다이아 팝니다"),
            Some((Channel::Trade, "다이아 팝니다"))
        );
        assert_eq!(Channel::split_prefix("!"), None);
        assert_eq!(Channel::split_prefix("안녕"), None);
        assert_eq!(Channel::parse("길드"), Some(Channel::Guild));
        assert_eq!(Channel::parse("global"), Some(Channel::Global));
        assert_eq!(Channel::parse("t"), Some(Channel::Trade));
        assert_eq!(Channel::parse("뭐"), None);
        // 기호는 서로 다름
        let mut seen = HashSet::new();
        assert!(Channel::ALL.iter().all(|c| seen.insert(c.prefix())));
    }

    #[test]
    fn who_receives_what() {
        let none = HashSet::new();
        let me = |instance, pos: (f64, f64, f64), staff, guild| Receiver {
            uuid: 2,
            instance,
            dimension: Dimension::Overworld,
            position: pos,
            staff,
            guild,
            muted: &none,
        };
        let here = me("lobby", (10.0, 64.0, 0.0), false, None);
        let far = me("lobby", (500.0, 64.0, 0.0), false, None);
        let other = me("survival", (0.0, 64.0, 0.0), false, Some("달빛"));
        assert!(reaches(&msg(Channel::Global), &other));
        assert!(reaches(&msg(Channel::Local), &far));
        assert!(!reaches(&msg(Channel::Local), &other));
        assert!(reaches(&msg(Channel::Near), &here));
        assert!(!reaches(&msg(Channel::Near), &far));
        assert!(reaches(&msg(Channel::Guild), &other));
        assert!(!reaches(&msg(Channel::Guild), &here));
        assert!(!reaches(&msg(Channel::Staff), &here));
        assert!(reaches(
            &msg(Channel::Staff),
            &me("x", (0.0, 0.0, 0.0), true, None)
        ));
        let muted: HashSet<_> = [Channel::Trade].into();
        let r = Receiver {
            muted: &muted,
            ..me("lobby", (0.0, 0.0, 0.0), false, None)
        };
        assert!(!reaches(&msg(Channel::Trade), &r));
    }

    #[test]
    fn bus_round_trip_and_render() {
        let m = msg(Channel::Guild);
        let back = from_json(&json::parse(&to_json(&m).compact()).unwrap()).unwrap();
        assert_eq!(back.content, m.content);
        assert_eq!(back.guild, m.guild);
        assert_eq!(back.channel, Channel::Guild);
        assert_eq!(back.card, m.card);
        let shown = format!("{:?}", render(&m));
        assert!(shown.contains("[달빛]") && shown.contains("Alex") && shown.contains("안녕"));
        let flat = text::plain_text(&render(&msg(Channel::Global)));
        assert_eq!(flat, "[전체] [VIP] [광부].(Lv.12) Alex : 안녕");
        assert!(shown.contains("길드  ") && shown.contains("show_text"));
    }

    #[test]
    fn slow_mode_blocks_repeats() {
        let t = Instant::now();
        assert_eq!(throttle(77, Channel::Global, t), None);
        assert!(throttle(77, Channel::Global, t + Duration::from_secs(1)).is_some());
        assert_eq!(
            throttle(77, Channel::Global, t + Duration::from_secs(4)),
            None
        );
        assert_eq!(throttle(77, Channel::Local, t), None);
        assert_eq!(throttle(77, Channel::Local, t), None);
    }
}
