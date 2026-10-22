//! 복권: 채널(인스턴스)마다 따로, 매주 토요일에 추첨.
//!
//! - 1~28 가운데 번호 5개를 골라 삽니다. 한 사람이 하루(한국 시각)에 살 수 있는 장 수는 정해져
//!   있습니다 (기본 2장).
//! - 추첨: 번호 5개. 5개 맞으면 1등, 4개 2등, 3개 3등, 2개 4등.
//! - 1등 당첨금 = (이번 회차에 팔린 장 수 × 가격) + 1등 기본 당첨금. 1등이 여럿이면 나눕니다.
//!   2~4등은 등수별 기본 당첨금. 1등이 나오면 서버 전체에 공지.
//! - 복권은 한 장마다 일련번호가 있는 아이템이고, 설명(로어)에 번호·아이템 번호·구매 날짜가
//!   나옵니다. 추첨 뒤에는 당첨 번호와 결과도. 스폰의 복권 NPC에게 내면 당첨금을 받습니다.
//!
//! 저장은 인스턴스 폴더의 `lottery.json` (설정·회차·판매·복권·지난 추첨).

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::game::custom_items::{self, CustomItem, Look};
use crate::json::{self, Json};
use crate::packets::world::Item;

pub const NUMBERS: u8 = 28;
pub const PICK: usize = 5;
/// 한국 시각 (UTC+9, 서머타임 없음).
const KST: i64 = 9 * 3600;
const DAY: i64 = 86_400;
/// 받아 간 복권 기록은 이만큼 지난 회차부터 지웁니다.
const KEEP_ROUNDS: u32 = 12;

pub const TICKET_ITEM: &str = "lottery_ticket";
pub const ICON_ITEM: &str = "gui_icon";

/// 복권 일련번호 (프로세스 안에서 겹치지 않게 — 채널이 여럿이어도).
static NEXT_SERIAL: AtomicU32 = AtomicU32::new(1);

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub price: i64,
    pub daily_limit: u32,
    /// 1~4등 기본 당첨금.
    pub base_prizes: [i64; 4],
    /// 추첨 요일 (0 일요일 … 6 토요일)과 시각 (한국 시각, 시).
    pub draw_weekday: u32,
    pub draw_hour: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            price: 1_000,
            daily_limit: 2,
            base_prizes: [1_000_000, 100_000, 10_000, 2_000],
            draw_weekday: 6,
            draw_hour: 21,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ticket {
    pub serial: u32,
    pub owner: u128,
    pub owner_name: String,
    pub numbers: [u8; PICK],
    pub round: u32,
    /// 산 때 (유닉스 초).
    pub bought: i64,
    pub claimed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Draw {
    pub round: u32,
    pub numbers: [u8; PICK],
    pub at: i64,
    /// 이번 회차 판매 장 수.
    pub sales: u32,
    /// 등수별 한 장당 당첨금 (1등은 나눈 뒤).
    pub prizes: [i64; 4],
    /// 등수별 당첨 장 수.
    pub winners: [u32; 4],
}

impl Draw {
    /// 맞힌 수 → 등수 (1~4).
    pub fn rank(&self, numbers: &[u8; PICK]) -> Option<u8> {
        let hits = numbers.iter().filter(|n| self.numbers.contains(n)).count();
        match hits {
            5 => Some(1),
            4 => Some(2),
            3 => Some(3),
            2 => Some(4),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct State {
    config: Config,
    round: u32,
    /// 이번 회차가 시작된 때.
    round_start: i64,
    sales: u32,
    tickets: Vec<Ticket>,
    draws: Vec<Draw>,
}

/// 낸 복권 한 장의 결과.
#[derive(Debug, Clone, PartialEq)]
pub enum Claim {
    /// 이 채널의 복권이 아님.
    Unknown,
    NotDrawn {
        draw_at: i64,
    },
    AlreadyClaimed,
    /// 등수(없으면 낙첨)와 당첨금.
    Done {
        round: u32,
        rank: Option<u8>,
        amount: i64,
    },
}

pub struct Lottery {
    state: Mutex<State>,
    path: Option<PathBuf>,
    /// 스폰의 NPC 엔티티 (켤 때마다 새로 세움).
    pub npc: Mutex<Option<i32>>,
}

// ---- 시각 ----

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// 1970-01-01부터의 날 수 → (년, 월, 일). (Howard Hinnant의 civil_from_days)
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// 0 일요일 … 6 토요일. 1970-01-01은 목요일.
fn weekday(days: i64) -> u32 {
    (days + 4).rem_euclid(7) as u32
}

const WEEKDAYS: [&str; 7] = ["일", "월", "화", "수", "목", "금", "토"];

/// 한국 시각의 날 (하루 구매 제한용).
pub fn kst_day(t: i64) -> i64 {
    (t + KST).div_euclid(DAY)
}

/// `2026-10-22 (목) 20:31`
pub fn format_time(t: i64) -> String {
    let local = t + KST;
    let days = local.div_euclid(DAY);
    let (y, m, d) = civil(days);
    let secs = local.rem_euclid(DAY);
    format!(
        "{y}-{m:02}-{d:02} ({}) {:02}:{:02}",
        WEEKDAYS[weekday(days) as usize],
        secs / 3600,
        secs % 3600 / 60
    )
}

/// `10월 24일 (토) 21:00`
pub fn format_short(t: i64) -> String {
    let local = t + KST;
    let days = local.div_euclid(DAY);
    let (_, m, d) = civil(days);
    let secs = local.rem_euclid(DAY);
    format!(
        "{m}월 {d}일 ({}) {:02}:{:02}",
        WEEKDAYS[weekday(days) as usize],
        secs / 3600,
        secs % 3600 / 60
    )
}

/// `t` 뒤의 첫 추첨 시각.
pub fn next_draw_after(t: i64, weekday_: u32, hour: u32) -> i64 {
    let today = kst_day(t);
    for k in 0..8 {
        let d = today + k;
        if weekday(d) == weekday_ {
            let at = d * DAY + hour as i64 * 3600 - KST;
            if at > t {
                return at;
            }
        }
    }
    t + 7 * DAY
}

pub fn won(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{}{out}원", if n < 0 { "-" } else { "" })
}

pub fn numbers_text(n: &[u8]) -> String {
    n.iter()
        .map(|x| format!("{x:02}"))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// 공정한 추첨: 1~28에서 겹치지 않게 5개 (운영체제 난수).
pub fn random_numbers(keep: &[u8]) -> [u8; PICK] {
    let mut picked: Vec<u8> = keep.iter().copied().take(PICK).collect();
    while picked.len() < PICK {
        let r = u32::from_le_bytes(crate::crypto::random::bytes::<4>());
        let n = (r % NUMBERS as u32) as u8 + 1;
        if !picked.contains(&n) {
            picked.push(n);
        }
    }
    picked.sort_unstable();
    picked.try_into().unwrap()
}

// ---- 아이템 ----

/// 복권 아이템과 GUI 아이콘 정의를 올립니다 (한 번).
pub fn register_items() {
    if custom_items::index_of(TICKET_ITEM).is_some() {
        return;
    }
    let paper = crate::data::items::by_name("paper").unwrap_or(1);
    let base = CustomItem {
        id: TICKET_ITEM.into(),
        base: paper,
        name: "복권".into(),
        color: "gold".into(),
        lore: vec!["스폰의 복권 NPC에게 내면 당첨금을 받습니다".into()],
        // 서버 리소스 팩을 쓰면 직접 그린 복권 그림, 아니면 종이 모양
        model: crate::resource_pack::enabled().then(|| "dangu:lottery_ticket".to_owned()),
        glint: false,
        rarity: 1,
        max_stack: Some(1),
        damage: None,
        ability: None,
        cooldown: 0.0,
    };
    let icon = CustomItem {
        id: ICON_ITEM.into(),
        name: " ".into(),
        color: "white".into(),
        lore: Vec::new(),
        model: None,
        rarity: 0,
        ..base.clone()
    };
    custom_items::install(vec![base, icon]);
}

pub fn ticket_index() -> Option<u16> {
    custom_items::index_of(TICKET_ITEM)
}

impl Lottery {
    pub fn open(path: Option<PathBuf>) -> Self {
        register_items();
        let loaded = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| json::parse(&t).ok())
            .and_then(|j| State::from_json(&j));
        let state = loaded.unwrap_or_else(|| State {
            config: Config::default(),
            round: 1,
            round_start: now(),
            sales: 0,
            tickets: Vec::new(),
            draws: Vec::new(),
        });
        let max = state.tickets.iter().map(|t| t.serial).max().unwrap_or(0);
        NEXT_SERIAL.fetch_max(max + 1, Ordering::Relaxed);
        let l = Self {
            state: Mutex::new(state),
            path,
            npc: Mutex::new(None),
        };
        let st = l.state.lock().unwrap().clone();
        for t in &st.tickets {
            custom_items::set_look(t.serial, look(&st, t));
        }
        l
    }

    fn save(&self, st: &State) {
        if let Some(p) = &self.path {
            let tmp = p.with_extension("json.tmp");
            if std::fs::write(&tmp, st.to_json().pretty()).is_ok() {
                let _ = std::fs::rename(tmp, p);
            }
        }
    }

    pub fn config(&self) -> Config {
        self.state.lock().unwrap().config.clone()
    }

    pub fn set_config(&self, c: Config) {
        let mut st = self.state.lock().unwrap();
        st.config = c;
        self.save(&st);
    }

    pub fn round(&self) -> u32 {
        self.state.lock().unwrap().round
    }

    pub fn sales(&self) -> u32 {
        self.state.lock().unwrap().sales
    }

    pub fn next_draw(&self) -> i64 {
        let st = self.state.lock().unwrap();
        next_draw_after(st.round_start, st.config.draw_weekday, st.config.draw_hour)
    }

    /// 지금 1등 당첨금 (이번 회차 판매 × 가격 + 1등 기본).
    pub fn jackpot(&self) -> i64 {
        let st = self.state.lock().unwrap();
        st.sales as i64 * st.config.price + st.config.base_prizes[0]
    }

    pub fn last_draw(&self) -> Option<Draw> {
        self.state.lock().unwrap().draws.last().cloned()
    }

    /// 오늘(한국 시각) 이 사람이 산 장 수.
    pub fn bought_today(&self, owner: u128, at: i64) -> u32 {
        let st = self.state.lock().unwrap();
        st.tickets
            .iter()
            .filter(|t| t.owner == owner && kst_day(t.bought) == kst_day(at))
            .count() as u32
    }

    /// 살 수 있나 (하루 제한).
    pub fn can_buy(&self, owner: u128, at: i64) -> Result<(), String> {
        let limit = self.config().daily_limit;
        if self.bought_today(owner, at) >= limit {
            return Err(format!(
                "오늘은 이미 {limit}장을 샀습니다. 내일 다시 사 주세요."
            ));
        }
        Ok(())
    }

    /// 복권을 만듭니다 (돈은 부른 쪽이 먼저 받음). 줄 아이템.
    pub fn buy(
        &self,
        owner: u128,
        name: &str,
        numbers: [u8; PICK],
        at: i64,
    ) -> Result<Item, String> {
        let mut sorted = numbers;
        sorted.sort_unstable();
        if sorted.windows(2).any(|w| w[0] == w[1]) || sorted.iter().any(|&n| n == 0 || n > NUMBERS)
        {
            return Err("번호는 1~28 가운데 서로 다른 5개여야 합니다.".into());
        }
        self.can_buy(owner, at)?;
        let mut st = self.state.lock().unwrap();
        let serial = NEXT_SERIAL.fetch_add(1, Ordering::Relaxed);
        let t = Ticket {
            serial,
            owner,
            owner_name: name.to_owned(),
            numbers: sorted,
            round: st.round,
            bought: at,
            claimed: false,
        };
        st.sales += 1;
        custom_items::set_look(serial, look(&st, &t));
        st.tickets.push(t);
        self.save(&st);
        let index = ticket_index().ok_or("복권 아이템이 없습니다")?;
        Ok(Item {
            id: custom_items::get(index).map_or(1, |c| c.base),
            count: 1,
            custom: Some(index),
            serial,
        })
    }

    pub fn ticket(&self, serial: u32) -> Option<Ticket> {
        self.state
            .lock()
            .unwrap()
            .tickets
            .iter()
            .find(|t| t.serial == serial)
            .cloned()
    }

    /// 복권 한 장을 냅니다. 당첨금은 부른 쪽이 줍니다.
    pub fn claim(&self, serial: u32) -> Claim {
        let mut st = self.state.lock().unwrap();
        let next = next_draw_after(st.round_start, st.config.draw_weekday, st.config.draw_hour);
        let Some(i) = st.tickets.iter().position(|t| t.serial == serial) else {
            return Claim::Unknown;
        };
        let t = st.tickets[i].clone();
        if t.claimed {
            return Claim::AlreadyClaimed;
        }
        let Some(draw) = st.draws.iter().find(|d| d.round == t.round).cloned() else {
            return Claim::NotDrawn { draw_at: next };
        };
        let rank = draw.rank(&t.numbers);
        let amount = rank.map_or(0, |r| draw.prizes[r as usize - 1]);
        st.tickets[i].claimed = true;
        self.save(&st);
        custom_items::remove_look(serial);
        Claim::Done {
            round: t.round,
            rank,
            amount,
        }
    }

    /// 추첨할 때가 되었나.
    pub fn due(&self, at: i64) -> bool {
        at >= self.next_draw()
    }

    /// 추첨합니다 (`numbers`가 있으면 그 번호로 — 시험용).
    pub fn draw(&self, numbers: Option<[u8; PICK]>, at: i64) -> Draw {
        let mut st = self.state.lock().unwrap();
        let numbers = numbers.unwrap_or_else(|| random_numbers(&[]));
        let mut d = Draw {
            round: st.round,
            numbers,
            at,
            sales: st.sales,
            prizes: [0; 4],
            winners: [0; 4],
        };
        for t in st.tickets.iter().filter(|t| t.round == st.round) {
            if let Some(r) = d.rank(&t.numbers) {
                d.winners[r as usize - 1] += 1;
            }
        }
        let c = &st.config;
        let jackpot = st.sales as i64 * c.price + c.base_prizes[0];
        d.prizes = [
            jackpot / d.winners[0].max(1) as i64,
            c.base_prizes[1],
            c.base_prizes[2],
            c.base_prizes[3],
        ];
        st.draws.push(d.clone());
        st.round += 1;
        st.round_start = at;
        st.sales = 0;
        // 오래되고 받아 간 기록은 정리
        let keep_from = st.round.saturating_sub(KEEP_ROUNDS);
        st.tickets.retain(|t| !(t.claimed && t.round < keep_from));
        st.draws.retain(|d| d.round >= keep_from);
        let snapshot = st.clone();
        for t in snapshot.tickets.iter().filter(|t| t.round == d.round) {
            custom_items::set_look(t.serial, look(&snapshot, t));
        }
        self.save(&st);
        d
    }
}

/// 복권 한 장의 이름과 설명.
fn look(st: &State, t: &Ticket) -> Look {
    let mut lore = vec![
        format!("번호  {}", numbers_text(&t.numbers)),
        format!("아이템 번호  #{:06}", t.serial),
        format!("구매  {}", format_time(t.bought)),
    ];
    match st.draws.iter().find(|d| d.round == t.round) {
        Some(d) => {
            lore.push(format!("당첨 번호  {}", numbers_text(&d.numbers)));
            lore.push(match d.rank(&t.numbers) {
                Some(r) => format!("결과  {r}등 · {}", won(d.prizes[r as usize - 1])),
                None => "결과  낙첨".into(),
            });
        }
        None => lore.push(format!(
            "추첨  {}",
            format_short(next_draw_after(
                st.round_start,
                st.config.draw_weekday,
                st.config.draw_hour
            ))
        )),
    }
    lore.push("스폰의 복권 NPC에게 내면 당첨금을 받습니다".into());
    let drawn = st.draws.iter().any(|d| d.round == t.round);
    Look {
        name: Some((format!("복권 · 제{}회", t.round), "gold".into())),
        lore,
        glint: Some(
            drawn
                && st
                    .draws
                    .iter()
                    .any(|d| d.round == t.round && d.rank(&t.numbers).is_some()),
        ),
    }
}

// ---- 저장 ----

impl State {
    fn to_json(&self) -> Json {
        let n = |v: f64| Json::Number(v);
        let nums = |a: &[u8]| Json::Array(a.iter().map(|&x| n(x as f64)).collect());
        let c = &self.config;
        Json::Object(vec![
            (
                "config".into(),
                Json::Object(vec![
                    ("price".into(), n(c.price as f64)),
                    ("daily_limit".into(), n(c.daily_limit as f64)),
                    (
                        "base_prizes".into(),
                        Json::Array(c.base_prizes.iter().map(|&p| n(p as f64)).collect()),
                    ),
                    ("draw_weekday".into(), n(c.draw_weekday as f64)),
                    ("draw_hour".into(), n(c.draw_hour as f64)),
                ]),
            ),
            ("round".into(), n(self.round as f64)),
            ("round_start".into(), n(self.round_start as f64)),
            ("sales".into(), n(self.sales as f64)),
            (
                "tickets".into(),
                Json::Array(
                    self.tickets
                        .iter()
                        .map(|t| {
                            Json::Object(vec![
                                ("serial".into(), n(t.serial as f64)),
                                ("owner".into(), Json::String(format!("{:032x}", t.owner))),
                                ("owner_name".into(), Json::String(t.owner_name.clone())),
                                ("numbers".into(), nums(&t.numbers)),
                                ("round".into(), n(t.round as f64)),
                                ("bought".into(), n(t.bought as f64)),
                                ("claimed".into(), Json::Bool(t.claimed)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "draws".into(),
                Json::Array(
                    self.draws
                        .iter()
                        .map(|d| {
                            Json::Object(vec![
                                ("round".into(), n(d.round as f64)),
                                ("numbers".into(), nums(&d.numbers)),
                                ("at".into(), n(d.at as f64)),
                                ("sales".into(), n(d.sales as f64)),
                                (
                                    "prizes".into(),
                                    Json::Array(d.prizes.iter().map(|&p| n(p as f64)).collect()),
                                ),
                                (
                                    "winners".into(),
                                    Json::Array(d.winners.iter().map(|&w| n(w as f64)).collect()),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    fn from_json(j: &Json) -> Option<State> {
        let f = |j: &Json, k: &str| j.get(k).and_then(Json::as_f64);
        let arr = |j: &Json, k: &str| -> Vec<f64> {
            j.get(k)
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(Json::as_f64)
                .collect()
        };
        let five = |v: Vec<f64>| -> Option<[u8; PICK]> {
            let v: Vec<u8> = v.into_iter().map(|x| x as u8).collect();
            v.try_into().ok()
        };
        let four_i = |v: Vec<f64>| -> Option<[i64; 4]> {
            let v: Vec<i64> = v.into_iter().map(|x| x as i64).collect();
            v.try_into().ok()
        };
        let d = Config::default();
        let c = j.get("config");
        let config = Config {
            price: c.and_then(|c| f(c, "price")).map_or(d.price, |v| v as i64),
            daily_limit: c
                .and_then(|c| f(c, "daily_limit"))
                .map_or(d.daily_limit, |v| v as u32),
            base_prizes: c
                .and_then(|c| four_i(arr(c, "base_prizes")))
                .unwrap_or(d.base_prizes),
            draw_weekday: c
                .and_then(|c| f(c, "draw_weekday"))
                .map_or(d.draw_weekday, |v| (v as u32).min(6)),
            draw_hour: c
                .and_then(|c| f(c, "draw_hour"))
                .map_or(d.draw_hour, |v| (v as u32).min(23)),
        };
        let tickets = j
            .get("tickets")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|t| {
                Some(Ticket {
                    serial: f(t, "serial")? as u32,
                    owner: u128::from_str_radix(t.get("owner")?.as_str()?, 16).ok()?,
                    owner_name: t.get("owner_name")?.as_str()?.to_owned(),
                    numbers: five(arr(t, "numbers"))?,
                    round: f(t, "round")? as u32,
                    bought: f(t, "bought")? as i64,
                    claimed: t.get("claimed").and_then(Json::as_bool).unwrap_or(false),
                })
            })
            .collect();
        let draws = j
            .get("draws")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|d| {
                let w: Vec<u32> = arr(d, "winners").into_iter().map(|x| x as u32).collect();
                Some(Draw {
                    round: f(d, "round")? as u32,
                    numbers: five(arr(d, "numbers"))?,
                    at: f(d, "at")? as i64,
                    sales: f(d, "sales").unwrap_or(0.0) as u32,
                    prizes: four_i(arr(d, "prizes"))?,
                    winners: w.try_into().ok()?,
                })
            })
            .collect();
        Some(State {
            config,
            round: f(j, "round")?.max(1.0) as u32,
            round_start: f(j, "round_start")? as i64,
            sales: f(j, "sales").unwrap_or(0.0) as u32,
            tickets,
            draws,
        })
    }
}

// ---- 틱: 추첨과 NPC ----

/// 5초마다 (엔티티 틱에서): 추첨할 때면 추첨하고 알리며, 스폰의 NPC를 세워 둡니다.
pub fn tick(server: &crate::Server) {
    let at = now();
    if server.lottery.due(at) {
        let d = server.lottery.draw(None, at);
        announce(server, &d);
    }
    if server.config.lottery_npc {
        ensure_npc(server);
    }
}

/// 추첨 결과를 이 채널에, 1등은 서버 전체에.
pub fn announce(server: &crate::Server, d: &Draw) {
    use crate::server::Event;
    use crate::text;
    let lines = [
        format!(
            "[복권] 제{}회 추첨! 당첨 번호: {}",
            d.round,
            numbers_text(&d.numbers)
        ),
        format!(
            "[복권] 1등 {}명 ({}) · 2등 {}명 · 3등 {}명 · 4등 {}명 — 스폰의 복권 NPC에게 내세요",
            d.winners[0],
            won(d.prizes[0]),
            d.winners[1],
            d.winners[2],
            d.winners[3]
        ),
    ];
    for l in lines {
        server.publish(Event::System(text::colored(&l, "gold")));
    }
    if d.winners[0] > 0 {
        let msg = format!(
            "{} 채널 제{}회 복권 1등 {}명! 당첨금 {}",
            server.name,
            d.round,
            d.winners[0],
            won(d.prizes[0])
        );
        if crate::dambi::child::active() {
            crate::dambi::child::publish("broadcast", &msg);
        } else {
            server.publish_all(Event::System(text::colored(
                &format!("[공지] {msg}"),
                "light_purple",
            )));
        }
    }
}

/// 스폰의 복권 NPC가 없으면 세웁니다.
pub fn ensure_npc(server: &crate::Server) {
    use crate::game::entities::Kind;
    use crate::game::mobs::{self, Mob, MobKind};
    use crate::world::{Dimension, terrain};
    let current = *server.lottery.npc.lock().unwrap();
    if current.is_some_and(|id| {
        server
            .entities
            .get(id)
            .is_some_and(|e| matches!(&e.kind, Kind::Mob(m) if !m.dead()))
    }) {
        return;
    }
    let d = Dimension::Overworld;
    let (sx, sy, sz) = terrain::spawn_point(d, server.seed);
    let (x, z) = (sx.floor() as i32 + 3, sz.floor() as i32);
    let base = sy as i32;
    let y = (base - 4..base + 12)
        .rev()
        .find(|&y| {
            server.world.is_solid(d, x, y - 1, z)
                && !server.world.is_solid(d, x, y, z)
                && !server.world.is_solid(d, x, y + 1, z)
        })
        .unwrap_or(base);
    let mut m = Mob::new(MobKind::Villager);
    m.villager = Some(crate::game::villagers::Villager::new("librarian", "plains"));
    if let Some(v) = &mut m.villager {
        v.level = 5;
        v.offers.clear();
    }
    m.npc = Some("lottery");
    m.custom_name = Some("복권 판매소".into());
    let id = mobs::spawn_mob(server, m, d, (x as f64 + 0.5, y as f64, z as f64 + 0.5));
    // 스폰 쪽을 봄
    server.entities.update(id, |e| e.yaw = 90.0);
    *server.lottery.npc.lock().unwrap() = Some(id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_and_draw_schedule() {
        // 2026-10-22 20:31 KST = 11:31 UTC
        let t = 1_792_668_660;
        assert_eq!(format_time(t), "2026-10-22 (목) 20:31");
        let next = next_draw_after(t, 6, 21);
        assert_eq!(format_time(next), "2026-10-24 (토) 21:00");
        // 토요일 21:00을 지나면 다음 주
        assert_eq!(
            format_time(next_draw_after(next, 6, 21)),
            "2026-10-31 (토) 21:00"
        );
        assert_eq!(won(1_012_000), "1,012,000원");
        assert_eq!(won(500), "500원");
    }

    #[test]
    fn ranks_jackpot_split_and_daily_limit() {
        let dir = std::env::temp_dir().join(format!("dangu-lottery-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lottery.json");
        let _ = std::fs::remove_file(&path);
        let l = Lottery::open(Some(path.clone()));
        let t0 = 1_792_668_660;
        let a = l.buy(1, "A", [1, 2, 3, 4, 5], t0).unwrap();
        let b = l.buy(2, "B", [5, 4, 3, 2, 1], t0).unwrap();
        let c = l.buy(1, "A", [1, 2, 3, 20, 21], t0).unwrap();
        assert!(
            l.buy(1, "A", [6, 7, 8, 9, 10], t0).is_err(),
            "daily limit 2"
        );
        assert!(l.buy(3, "C", [1, 1, 2, 3, 4], t0).is_err(), "duplicates");
        assert!(
            l.buy(1, "A", [6, 7, 8, 9, 10], t0 + DAY).is_ok(),
            "next day"
        );
        assert_eq!(l.sales(), 4);
        assert!(matches!(l.claim(a.serial), Claim::NotDrawn { .. }));
        let d = l.draw(Some([1, 2, 3, 4, 5]), t0 + 2 * DAY);
        assert_eq!(d.winners, [2, 0, 1, 0]);
        // 1등 = (4장 × 1,000원 + 1,000,000원) / 2명
        assert_eq!(d.prizes[0], 502_000);
        assert_eq!(
            l.claim(a.serial),
            Claim::Done {
                round: 1,
                rank: Some(1),
                amount: 502_000
            }
        );
        assert_eq!(l.claim(a.serial), Claim::AlreadyClaimed);
        assert!(matches!(
            l.claim(c.serial),
            Claim::Done {
                rank: Some(3),
                amount: 10_000,
                ..
            }
        ));
        assert_eq!(l.round(), 2);
        assert_eq!(l.sales(), 0);
        // 다시 열어도 그대로
        let again = Lottery::open(Some(path));
        assert!(matches!(
            again.claim(b.serial),
            Claim::Done { rank: Some(1), .. }
        ));
        assert_eq!(again.round(), 2);
        std::fs::remove_dir_all(dir).ok();
    }
}
