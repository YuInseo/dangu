//! 후원 등급 (기획 "후원자 특혜 — 개인"): Budget · VIP · MVP 와 특혜.
//!
//! | 특혜 | Budget | VIP | MVP | 구현 |
//! |---|---|---|---|---|
//! | /ec /제작대 /수리 | ✔ | ✔ | ✔ | 개인 엔더상자 27칸 · 작업대 창 · 수리(내구도가 닳지 않는 서버라 안내만) |
//! | 마을 내부 플라이 무한 | ✔ | ✔ | ✔ | 우리 길드 땅 청크 안에서 날기 |
//! | 더블점프 | ✔ | ✔ | ✔ | 공중에서 점프 한 번 더 (땅에 닿으면 다시) |
//! | 셋홈 + | 1 | 2 | 3 | `/셋홈` 기본 1개 + 등급 |
//! | tp 쿨타임 | -30% | -50% | 없음 | `/홈` 등 이동 대기 (기본 30초) |
//! | 추천보상·프리미엄 출석보상 | ✔ | ✔ | ✔ | 추천·출석이 생기면 (값만 있음) |
//! | 경매장 수수료 제외 · 판매 +N | | +10 | +20 | 경매장이 생기면 (값만 있음) |
//! | 자동심기 | | 10만 | 20만 | 다 자란 작물을 캐면 그 자리에 다시 심김 (등급을 줄 때 블록 수 지급) |
//! | 셜커 쉬프트+우클릭 | | ✔ | ✔ | 셜커 내용물을 아직 저장하지 않아 준비 중 |
//! | 인벤세이브 | | ✔ | ✔ | 죽어도 가방 유지 |
//! | 잠수보상 | | 2배 | 2배 | 2분 넘게 가만히 있으면 1분마다 잠수 포인트 |
//! | 인벤 자동판매 | | | ✔ | 판매 가격표가 생기면 (값만 있음) |
//! | 경험치 부스트 | | | 2배 | 직업 경험치 |
//! | 갑옷 숨기기 | ✔ | ✔ | ✔ | `/갑옷숨기기`: 남에게 내 갑옷을 안 보임 |
//!
//! 등급 표는 `world/ranks.json` (웹 관리 화면 "서버" 탭), 사람마다의 등급·홈·자동심기·플라이 시간·
//! 엔더상자는 `world/rank_players.json`. 둘 다 모든 인스턴스가 함께 씀.

pub mod session;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::json::Json;
use crate::packets::world::Item;
use crate::plugins::store::{JsonFile, data_path};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Perks {
    /// /ec /제작대 /수리
    pub commands: bool,
    pub guild_fly: bool,
    pub double_jump: bool,
    /// 더 놓을 수 있는 홈 수
    pub homes: u32,
    /// 이동 대기 줄이기 (%; 100이면 없음)
    pub tp_cooldown_cut: f64,
    pub referral_bonus: bool,
    pub premium_attendance: bool,
    pub auction_no_fee: bool,
    pub auction_slots: u32,
    /// 등급을 받을 때 주는 자동심기 블록 수
    pub autoplant: u64,
    pub shulker_open: bool,
    pub keep_inventory: bool,
    /// 잠수 포인트 배율
    pub afk_mult: f64,
    pub auto_sell: bool,
    /// 직업 경험치 배율
    pub xp_mult: f64,
    pub hide_armor: bool,
}

impl Perks {
    /// 사람이 읽는 특혜 목록.
    pub fn lines(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.commands {
            v.push("/ec /제작대 /수리".into());
        }
        if self.guild_fly {
            v.push("마을(길드 땅) 안 플라이 무한".into());
        }
        if self.double_jump {
            v.push("더블점프".into());
        }
        if self.homes > 0 {
            v.push(format!("셋홈 +{}개", self.homes));
        }
        if self.tp_cooldown_cut >= 100.0 {
            v.push("tp 쿨타임 없음".into());
        } else if self.tp_cooldown_cut > 0.0 {
            v.push(format!("tp 쿨타임 -{:.0}%", self.tp_cooldown_cut));
        }
        if self.referral_bonus {
            v.push("추천보상 증가".into());
        }
        if self.premium_attendance {
            v.push("프리미엄 출석보상".into());
        }
        if self.auction_no_fee {
            v.push("경매장 수수료 없음".into());
        }
        if self.auction_slots > 0 {
            v.push(format!("경매장 판매 물건 +{}", self.auction_slots));
        }
        if self.autoplant > 0 {
            v.push(format!(
                "자동심기 +{}블록",
                crate::plugins::hud::number(self.autoplant as i64)
            ));
        }
        if self.shulker_open {
            v.push("셜커 쉬프트+우클릭으로 열기".into());
        }
        if self.keep_inventory {
            v.push("인벤세이브".into());
        }
        if self.afk_mult > 1.0 {
            v.push(format!("잠수보상 {:.0}배", self.afk_mult));
        }
        if self.auto_sell {
            v.push("인벤 아이템 자동판매".into());
        }
        if self.xp_mult > 1.0 {
            v.push(format!("경험치 부스트 {:.0}배", self.xp_mult));
        }
        if self.hide_armor {
            v.push("갑옷 숨기기".into());
        }
        v
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankDef {
    pub id: String,
    pub name: String,
    pub color: String,
    /// 원
    pub price: i64,
    pub perks: Perks,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankConfig {
    pub ranks: Vec<RankDef>,
    pub base_homes: u32,
    /// `/홈` 등 이동 대기 (초)
    pub tp_cooldown_secs: u64,
    /// 이만큼 가만히 있으면 잠수 (초)
    pub afk_after_secs: u64,
    /// 잠수 1분마다 포인트
    pub afk_points_per_minute: i64,
    /// 잠수 포인트가 쌓이는 인스턴스 (비면 모두)
    pub afk_instances: Vec<String>,
    /// `/등급` 창의 후원 안내 (주소 등)
    pub shop_url: String,
}

impl Default for RankConfig {
    fn default() -> Self {
        let budget = Perks {
            commands: true,
            guild_fly: true,
            double_jump: true,
            homes: 1,
            tp_cooldown_cut: 30.0,
            referral_bonus: true,
            premium_attendance: true,
            afk_mult: 1.0,
            xp_mult: 1.0,
            hide_armor: true,
            ..Perks::default()
        };
        let vip = Perks {
            homes: 2,
            tp_cooldown_cut: 50.0,
            auction_no_fee: true,
            auction_slots: 10,
            autoplant: 100_000,
            shulker_open: true,
            keep_inventory: true,
            afk_mult: 2.0,
            ..budget.clone()
        };
        let mvp = Perks {
            homes: 3,
            tp_cooldown_cut: 100.0,
            auction_slots: 20,
            autoplant: 200_000,
            auto_sell: true,
            xp_mult: 2.0,
            ..vip.clone()
        };
        RankConfig {
            ranks: vec![
                RankDef {
                    id: "budget".into(),
                    name: "Budget".into(),
                    color: "#7be07b".into(),
                    price: 9_900,
                    perks: budget,
                },
                RankDef {
                    id: "vip".into(),
                    name: "VIP".into(),
                    color: "#ffd866".into(),
                    price: 19_900,
                    perks: vip,
                },
                RankDef {
                    id: "mvp".into(),
                    name: "MVP".into(),
                    color: "#6fe3ff".into(),
                    price: 39_900,
                    perks: mvp,
                },
            ],
            base_homes: 1,
            tp_cooldown_secs: 30,
            afk_after_secs: 120,
            afk_points_per_minute: 1,
            afk_instances: Vec::new(),
            shop_url: String::new(),
        }
    }
}

impl RankConfig {
    /// id는 비지 않고 겹치지 않게, 배율·값은 0 이상.
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        for r in &self.ranks {
            if r.id.is_empty() || !ids.insert(r.id.as_str()) {
                return Err(format!("등급 id가 비었거나 겹칩니다: '{}'", r.id));
            }
            let p = &r.perks;
            if p.afk_mult < 0.0 || p.xp_mult < 0.0 || r.price < 0 || p.tp_cooldown_cut < 0.0 {
                return Err(format!("{}: 값은 0 이상이어야 합니다", r.name));
            }
        }
        Ok(())
    }

    pub fn rank(&self, id: &str) -> Option<&RankDef> {
        self.ranks
            .iter()
            .find(|r| r.id == id || r.name.eq_ignore_ascii_case(id))
    }
}

/// 집 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct Home {
    pub name: String,
    pub instance: String,
    pub dimension: String,
    pub pos: (f64, f64, f64),
    pub look: (f32, f32),
}

/// 한 사람의 등급과 특혜 상태.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Member {
    /// 마지막으로 본 이름 (웹 관리 화면·이름으로 찾기)
    pub name: String,
    pub rank: Option<String>,
    /// 끝나는 때 (유닉스 초, 0이면 영구)
    pub until: i64,
    pub homes: Vec<Home>,
    /// 남은 자동심기 블록
    pub autoplant: u64,
    /// 플라이가 끝나는 때 (유닉스 초)
    pub fly_until: i64,
    pub hide_armor: bool,
    /// 개인 엔더상자 27칸
    pub ender: Vec<Option<Item>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Members {
    pub players: BTreeMap<u128, Member>,
}

pub const ENDER_SLOTS: usize = 27;

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl Members {
    pub fn member(&self, uuid: u128) -> Member {
        self.players.get(&uuid).cloned().unwrap_or_default()
    }
    pub fn member_mut(&mut self, uuid: u128) -> &mut Member {
        let m = self.players.entry(uuid).or_default();
        if m.ender.len() != ENDER_SLOTS {
            m.ender.resize(ENDER_SLOTS, None);
        }
        m
    }
}

/// 지금 유효한 등급 (끝났으면 None).
pub fn active_rank(m: &Member, cfg: &RankConfig, now: i64) -> Option<RankDef> {
    let id = m.rank.as_deref()?;
    if m.until != 0 && m.until <= now {
        return None;
    }
    cfg.rank(id).cloned()
}

/// 이 사람의 등급 (없으면 None).
pub fn rank_of(uuid: u128) -> Option<RankDef> {
    let m = members().read(|d| d.member(uuid));
    config(|c| active_rank(&m, c, now()))
}

/// 등급이 없는 사람의 특혜: 아무것도 없고 배율 1.
pub fn no_perks() -> Perks {
    Perks {
        afk_mult: 1.0,
        xp_mult: 1.0,
        ..Perks::default()
    }
}

/// 이 사람의 특혜.
pub fn perks_of(uuid: u128) -> Perks {
    rank_of(uuid).map_or_else(no_perks, |r| r.perks)
}

/// 직업 경험치 (등급의 경험치 부스트를 곱해서).
pub async fn job_xp(uuid: u128, job: &str, xp: f64) {
    crate::dambi::child::job_xp(uuid, job, xp * perks_of(uuid).xp_mult).await;
}

/// 등급을 줌 (`days` 0이면 영구). 자동심기 블록도 그만큼 더해 줌.
pub fn grant(uuid: u128, rank: Option<&RankDef>, days: i64) {
    let t = now();
    members().write(|d| {
        let m = d.member_mut(uuid);
        match rank {
            Some(r) => {
                m.rank = Some(r.id.clone());
                m.until = if days > 0 { t + days * 86_400 } else { 0 };
                m.autoplant += r.perks.autoplant;
            }
            None => {
                m.rank = None;
                m.until = 0;
            }
        }
    });
}

/// 자동심기 블록 더하기 (퀘스트 보상 등).
pub fn add_autoplant(uuid: u128, n: u64) {
    members().write(|d| d.member_mut(uuid).autoplant += n);
}

/// 플라이 시간 더하기 (분; 남은 시간에 이어서).
pub fn add_fly_minutes(uuid: u128, minutes: i64) {
    add_fly_secs(uuid, minutes * 60);
}

/// 남은 플라이 (초, 없으면 0).
pub fn fly_left(uuid: u128) -> i64 {
    (members().read(|d| d.member(uuid).fly_until) - now()).max(0)
}

/// 플라이를 더하거나(음수면 빼기) 바꾼 뒤 남은 초. 0 아래로는 안 내려감.
pub fn add_fly_secs(uuid: u128, delta: i64) -> i64 {
    let t = now();
    members().write(|d| {
        let m = d.member_mut(uuid);
        let left = (m.fly_until - t).max(0).saturating_add(delta).max(0);
        m.fly_until = if left > 0 { t + left } else { 0 };
        left
    })
}

/// 플라이를 꼭 이만큼으로 (초).
pub fn set_fly_secs(uuid: u128, secs: i64) -> i64 {
    let t = now();
    let secs = secs.max(0);
    members().write(|d| d.member_mut(uuid).fly_until = if secs > 0 { t + secs } else { 0 });
    secs
}

/// 이름으로 찾기 (한 번이라도 접속한 사람, 대소문자 무시).
pub fn uuid_by_name(name: &str) -> Option<(u128, String)> {
    members().read(|d| {
        d.players
            .iter()
            .find(|(_, m)| m.name.eq_ignore_ascii_case(name))
            .map(|(u, m)| (*u, m.name.clone()))
    })
}

/// 시간 글 → 초. `1h30m20s` `1시간 30분 20초` `2d` `90m` `1:30:20`(시:분:초) `45:00`(분:초)
/// `3600`(단위 없으면 초).
pub fn parse_duration(text: &str) -> Option<i64> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if t.is_empty() {
        return None;
    }
    if t.contains(':') {
        let parts: Vec<i64> = t
            .split(':')
            .map(|p| p.parse::<i64>().ok().filter(|v| *v >= 0))
            .collect::<Option<_>>()?;
        return match parts[..] {
            [m, s] => Some(m * 60 + s),
            [h, m, s] => Some(h * 3600 + m * 60 + s),
            [d, h, m, s] => Some(d * 86_400 + h * 3600 + m * 60 + s),
            _ => None,
        };
    }
    if let Ok(v) = t.parse::<i64>() {
        return (v >= 0).then_some(v);
    }
    let mut total: i64 = 0;
    let mut num = String::new();
    let mut any = false;
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        // 단위: 영어 한 글자, 한글 (일·시간·분·초)
        let unit = match c {
            'd' | 'D' | '일' => 86_400,
            'h' | 'H' => 3600,
            '시' => {
                if chars.peek() == Some(&'간') {
                    chars.next();
                }
                3600
            }
            'm' | 'M' | '분' => 60,
            's' | 'S' | '초' => 1,
            _ => return None,
        };
        let v: i64 = num.parse().ok()?;
        num.clear();
        total = total.checked_add(v.checked_mul(unit)?)?;
        any = true;
    }
    if !num.is_empty() {
        return None;
    }
    any.then_some(total)
}

/// 초 → `1일 2시간 3분 4초` (0인 칸은 뺌, 0이면 `0초`).
pub fn duration_text(secs: i64) -> String {
    let secs = secs.max(0);
    let (d, h, m, s) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60, secs % 60);
    let mut parts = Vec::new();
    for (v, u) in [(d, "일"), (h, "시간"), (m, "분"), (s, "초")] {
        if v > 0 {
            parts.push(format!("{v}{u}"));
        }
    }
    if parts.is_empty() {
        "0초".into()
    } else {
        parts.join(" ")
    }
}

// ---- JSON ----

fn n(v: f64) -> Json {
    Json::Number(v)
}
fn b(v: bool) -> Json {
    Json::Bool(v)
}

fn perks_json(p: &Perks) -> Json {
    Json::Object(
        [
            ("commands", b(p.commands)),
            ("guild_fly", b(p.guild_fly)),
            ("double_jump", b(p.double_jump)),
            ("homes", n(p.homes as f64)),
            ("tp_cooldown_cut", n(p.tp_cooldown_cut)),
            ("referral_bonus", b(p.referral_bonus)),
            ("premium_attendance", b(p.premium_attendance)),
            ("auction_no_fee", b(p.auction_no_fee)),
            ("auction_slots", n(p.auction_slots as f64)),
            ("autoplant", n(p.autoplant as f64)),
            ("shulker_open", b(p.shulker_open)),
            ("keep_inventory", b(p.keep_inventory)),
            ("afk_mult", n(p.afk_mult)),
            ("auto_sell", b(p.auto_sell)),
            ("xp_mult", n(p.xp_mult)),
            ("hide_armor", b(p.hide_armor)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect(),
    )
}

fn perks_from(j: &Json) -> Perks {
    let bo = |k: &str| j.get(k).and_then(Json::as_bool).unwrap_or(false);
    let nu = |k: &str, d: f64| j.get(k).and_then(Json::as_f64).unwrap_or(d);
    Perks {
        commands: bo("commands"),
        guild_fly: bo("guild_fly"),
        double_jump: bo("double_jump"),
        homes: nu("homes", 0.0) as u32,
        tp_cooldown_cut: nu("tp_cooldown_cut", 0.0),
        referral_bonus: bo("referral_bonus"),
        premium_attendance: bo("premium_attendance"),
        auction_no_fee: bo("auction_no_fee"),
        auction_slots: nu("auction_slots", 0.0) as u32,
        autoplant: nu("autoplant", 0.0) as u64,
        shulker_open: bo("shulker_open"),
        keep_inventory: bo("keep_inventory"),
        afk_mult: nu("afk_mult", 1.0),
        auto_sell: bo("auto_sell"),
        xp_mult: nu("xp_mult", 1.0),
        hide_armor: bo("hide_armor"),
    }
}

pub fn encode_config(c: &RankConfig) -> Json {
    Json::Object(vec![
        (
            "ranks".into(),
            Json::Array(
                c.ranks
                    .iter()
                    .map(|r| {
                        Json::Object(vec![
                            ("id".into(), Json::String(r.id.clone())),
                            ("name".into(), Json::String(r.name.clone())),
                            ("color".into(), Json::String(r.color.clone())),
                            ("price".into(), n(r.price as f64)),
                            ("perks".into(), perks_json(&r.perks)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("base_homes".into(), n(c.base_homes as f64)),
        ("tp_cooldown_secs".into(), n(c.tp_cooldown_secs as f64)),
        ("afk_after_secs".into(), n(c.afk_after_secs as f64)),
        (
            "afk_points_per_minute".into(),
            n(c.afk_points_per_minute as f64),
        ),
        (
            "afk_instances".into(),
            Json::Array(c.afk_instances.iter().cloned().map(Json::String).collect()),
        ),
        ("shop_url".into(), Json::String(c.shop_url.clone())),
    ])
}

pub fn decode_config(j: &Json) -> Option<RankConfig> {
    let d = RankConfig::default();
    let nu = |k: &str, def: f64| j.get(k).and_then(Json::as_f64).unwrap_or(def);
    let s = |j: &Json, k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
    Some(RankConfig {
        ranks: match j.get("ranks").and_then(Json::as_array) {
            Some(a) => a
                .iter()
                .map(|r| RankDef {
                    id: s(r, "id"),
                    name: s(r, "name"),
                    color: s(r, "color"),
                    price: r.get("price").and_then(Json::as_f64).unwrap_or(0.0) as i64,
                    perks: r.get("perks").map(perks_from).unwrap_or_default(),
                })
                .collect(),
            None => d.ranks.clone(),
        },
        base_homes: nu("base_homes", d.base_homes as f64) as u32,
        tp_cooldown_secs: nu("tp_cooldown_secs", d.tp_cooldown_secs as f64) as u64,
        afk_after_secs: nu("afk_after_secs", d.afk_after_secs as f64) as u64,
        afk_points_per_minute: nu("afk_points_per_minute", d.afk_points_per_minute as f64) as i64,
        afk_instances: j
            .get("afk_instances")
            .and_then(Json::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(Json::as_str)
            .map(str::to_owned)
            .collect(),
        shop_url: s(j, "shop_url"),
    })
}

fn encode_members(d: &Members) -> Json {
    use crate::plugins::guild::store::item_json;
    Json::Object(
        d.players
            .iter()
            .map(|(u, m)| {
                let mut f = vec![];
                if let Some(r) = &m.rank {
                    f.push(("rank".to_owned(), Json::String(r.clone())));
                    f.push(("until".into(), n(m.until as f64)));
                }
                if !m.homes.is_empty() {
                    f.push((
                        "homes".into(),
                        Json::Array(
                            m.homes
                                .iter()
                                .map(|h| {
                                    Json::Object(vec![
                                        ("name".into(), Json::String(h.name.clone())),
                                        ("instance".into(), Json::String(h.instance.clone())),
                                        ("dimension".into(), Json::String(h.dimension.clone())),
                                        (
                                            "pos".into(),
                                            Json::Array(vec![n(h.pos.0), n(h.pos.1), n(h.pos.2)]),
                                        ),
                                        (
                                            "look".into(),
                                            Json::Array(vec![
                                                n(h.look.0 as f64),
                                                n(h.look.1 as f64),
                                            ]),
                                        ),
                                    ])
                                })
                                .collect(),
                        ),
                    ));
                }
                if !m.name.is_empty() {
                    f.insert(0, ("name".into(), Json::String(m.name.clone())));
                }
                if m.autoplant > 0 {
                    f.push(("autoplant".into(), n(m.autoplant as f64)));
                }
                if m.fly_until > 0 {
                    f.push(("fly_until".into(), n(m.fly_until as f64)));
                }
                if m.hide_armor {
                    f.push(("hide_armor".into(), Json::Bool(true)));
                }
                if m.ender.iter().any(Option::is_some) {
                    f.push((
                        "ender".into(),
                        Json::Array(
                            m.ender
                                .iter()
                                .map(|i| i.as_ref().map_or(Json::Null, item_json))
                                .collect(),
                        ),
                    ));
                }
                (format!("{u:032x}"), Json::Object(f))
            })
            .collect(),
    )
}

fn decode_members(j: &Json) -> Option<Members> {
    use crate::plugins::guild::store::item_from;
    let Json::Object(f) = j else {
        return Some(Members::default());
    };
    let mut d = Members::default();
    for (k, m) in f {
        let Ok(uuid) = u128::from_str_radix(k, 16) else {
            continue;
        };
        let nu = |k: &str| m.get(k).and_then(Json::as_f64).unwrap_or(0.0);
        let mut ender: Vec<Option<Item>> = m
            .get("ender")
            .and_then(Json::as_array)
            .unwrap_or_default()
            .iter()
            .map(item_from)
            .collect();
        ender.resize(ENDER_SLOTS, None);
        d.players.insert(
            uuid,
            Member {
                rank: m.get("rank").and_then(Json::as_str).map(str::to_owned),
                until: nu("until") as i64,
                homes: m
                    .get("homes")
                    .and_then(Json::as_array)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|h| {
                        let p = h.get("pos")?.as_array()?;
                        let l = h.get("look").and_then(Json::as_array).unwrap_or_default();
                        let f =
                            |a: &[Json], i: usize| a.get(i).and_then(Json::as_f64).unwrap_or(0.0);
                        Some(Home {
                            name: h.get("name")?.as_str()?.to_owned(),
                            instance: h.get("instance")?.as_str()?.to_owned(),
                            dimension: h.get("dimension")?.as_str()?.to_owned(),
                            pos: (f(p, 0), f(p, 1), f(p, 2)),
                            look: (f(l, 0) as f32, f(l, 1) as f32),
                        })
                    })
                    .collect(),
                name: m
                    .get("name")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_owned(),
                autoplant: nu("autoplant") as u64,
                fly_until: nu("fly_until") as i64,
                hide_armor: m.get("hide_armor").and_then(Json::as_bool).unwrap_or(false),
                ender,
            },
        );
    }
    Some(d)
}

static CONFIG: OnceLock<JsonFile<RankConfig>> = OnceLock::new();
static MEMBERS: OnceLock<JsonFile<Members>> = OnceLock::new();

fn config_file() -> &'static JsonFile<RankConfig> {
    CONFIG.get_or_init(|| JsonFile::in_memory(encode_config, decode_config))
}

pub fn members() -> &'static JsonFile<Members> {
    MEMBERS.get_or_init(|| JsonFile::in_memory(encode_members, decode_members))
}

pub fn config<R>(f: impl FnOnce(&RankConfig) -> R) -> R {
    config_file().read(f)
}

pub fn get_config() -> RankConfig {
    config(|c| c.clone())
}

pub fn set_config(c: RankConfig) -> Result<(), String> {
    c.validate()?;
    config_file().write(|old| *old = c);
    Ok(())
}

pub fn open_global() {
    let _ = CONFIG.set(JsonFile::open(
        data_path("MC_RANKS", "ranks.json"),
        encode_config,
        decode_config,
    ));
    let _ = MEMBERS.set(JsonFile::open(
        data_path("MC_RANK_PLAYERS", "rank_players.json"),
        encode_members,
        decode_members,
    ));
    if let Some(p) = config_file().path()
        && !p.exists()
    {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, encode_config(&RankConfig::default()).pretty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_match_the_plan_and_round_trip() {
        let c = RankConfig::default();
        assert_eq!(decode_config(&encode_config(&c)).unwrap(), c);
        let mvp = c.rank("MVP").unwrap();
        assert_eq!(
            (mvp.price, mvp.perks.homes, mvp.perks.xp_mult),
            (39_900, 3, 2.0)
        );
        assert!(mvp.perks.lines().iter().any(|l| l == "tp 쿨타임 없음"));
        let budget = c.rank("budget").unwrap();
        assert!(!budget.perks.keep_inventory && budget.perks.hide_armor);
        assert_eq!(c.rank("vip").unwrap().perks.autoplant, 100_000);

        let mut d = Members::default();
        let m = d.member_mut(7);
        m.rank = Some("vip".into());
        m.until = 100;
        m.homes.push(Home {
            name: "집".into(),
            instance: "main".into(),
            dimension: "overworld".into(),
            pos: (1.5, 64.0, -2.5),
            look: (90.0, 0.0),
        });
        m.ender[3] = Some(Item::new(1, 5));
        assert_eq!(decode_members(&encode_members(&d)).unwrap(), d);
        // 끝난 등급은 없는 것
        assert!(active_rank(&d.member(7), &c, 50).is_some());
        assert!(active_rank(&d.member(7), &c, 100).is_none());
    }

    #[test]
    fn durations_parse_and_print() {
        assert_eq!(parse_duration("1h30m20s"), Some(5420));
        assert_eq!(parse_duration("1시간 30분 20초"), Some(5420));
        assert_eq!(parse_duration("2d"), Some(172_800));
        assert_eq!(parse_duration("90m"), Some(5400));
        assert_eq!(parse_duration("1:30:20"), Some(5420));
        assert_eq!(parse_duration("45:00"), Some(2700));
        assert_eq!(parse_duration("3600"), Some(3600));
        assert_eq!(parse_duration("1x"), None);
        assert_eq!(parse_duration("h"), None);
        assert_eq!(parse_duration("10m5"), None);
        assert_eq!(duration_text(5420), "1시간 30분 20초");
        assert_eq!(duration_text(90_061), "1일 1시간 1분 1초");
        assert_eq!(duration_text(0), "0초");
    }
}
