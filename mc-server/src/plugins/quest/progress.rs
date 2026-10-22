//! 퀘스트 진행: 사람마다(일반·주간)와 길드마다(길드 주간). 패킷을 모르는 순수한 규칙.

use std::collections::BTreeMap;

use super::config::{QuestConfig, QuestDef, Resource, Rewards, SupplyKind};
use super::{day_of, name_matches, week_of};
use crate::json::Json;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Normal,
    /// 일일 납품.
    Daily,
    Weekly,
    Guild,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Normal, Kind::Daily, Kind::Weekly, Kind::Guild];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Normal => "일반",
            Kind::Daily => "일일",
            Kind::Weekly => "주간",
            Kind::Guild => "길드",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Kind::Normal => "normal",
            Kind::Daily => "daily",
            Kind::Weekly => "weekly",
            Kind::Guild => "guild",
        }
    }

    fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.key() == s)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Slot {
    pub id: String,
    pub progress: u32,
    pub done: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerQuests {
    /// 수락한 일반 퀘스트.
    pub accepted: Vec<Slot>,
    /// 끝낸 (다시 받을 수 없는) 일반 퀘스트.
    pub finished: Vec<String>,
    pub week: i64,
    pub weekly: Vec<Slot>,
    /// HUD에 따라다니게 한 퀘스트.
    pub tracked: Vec<(Kind, String)>,
    /// 오늘의 일일 납품과, 모두 내서 보상을 받았나.
    pub day: i64,
    pub daily: Vec<Slot>,
    pub daily_paid: bool,
    /// 이번 주의 주간 납품.
    pub supply_week: i64,
    pub wsupply: Vec<Slot>,
    pub wsupply_paid: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildQuests {
    pub week: i64,
    pub quests: Vec<Slot>,
    /// 이번 주 길드원마다 채운 양 (공헌).
    pub contrib: BTreeMap<u128, u32>,
    /// 오늘의 길드 납품, 낸 길드원들, 보상을 받았나.
    pub day: i64,
    pub supply: Vec<Slot>,
    pub supply_contrib: BTreeMap<u128, u32>,
    pub supply_paid: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Progress {
    pub players: BTreeMap<u128, PlayerQuests>,
    pub guilds: BTreeMap<u32, GuildQuests>,
}

/// 게임에서 일어난 일 하나 (`mine`·`ores`·`logs`·`harvest`·`kill`, 그 이름).
#[derive(Debug, Clone)]
pub struct Happened {
    pub kind: &'static str,
    pub name: String,
    pub amount: u32,
}

/// 이번에 끝난 퀘스트.
#[derive(Debug, Clone, PartialEq)]
pub struct Completed {
    pub kind: Kind,
    pub def: QuestDef,
    pub guild: Option<u32>,
    /// 길드 납품: 보상(GP 말고)을 받을 길드원들.
    pub contributors: Vec<u128>,
}

/// HUD 한 줄.
#[derive(Debug, Clone, PartialEq)]
pub struct HudQuest {
    pub kind: Kind,
    pub name: String,
    pub progress: u32,
    pub target: u32,
    pub done: bool,
}

/// 그 주의 퀘스트 `n`개 (주 번호로 정해서 모두가 같음).
pub fn pick(pool: &[QuestDef], n: usize, seed: i64) -> Vec<String> {
    let mut ids: Vec<&str> = pool.iter().map(|d| d.id.as_str()).collect();
    let mut x = (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut out = Vec::new();
    while out.len() < n && !ids.is_empty() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push(ids.remove((x % ids.len() as u64) as usize).to_owned());
    }
    out
}

/// 이 자원을 지금 받을 수 있나 (아이템이 서버에 있어야 — `dangu:` 물고기·작물은 콘텐츠가 있을 때만).
fn available(r: &Resource) -> bool {
    r.items.iter().any(|i| {
        if i.contains('*') {
            (0..crate::data::items::count() as i32)
                .any(|id| name_matches(i, crate::data::items::name(id)))
        } else {
            super::session::item_by_name(i, 1).is_some()
        }
    })
}

/// 납품 자원을 `min_pick..=max_pick`개 뽑습니다 (`seed`가 같으면 같음).
pub fn pick_supply(cfg: &QuestConfig, kind: SupplyKind, seed: u64) -> Vec<Slot> {
    let s = &cfg.supply;
    let mut pool: Vec<&Resource> = s.resources.iter().filter(|r| available(r)).collect();
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let span = s.max_pick.saturating_sub(s.min_pick) as u64 + 1;
    let n = (s.min_pick + (next() % span) as usize).min(pool.len());
    (0..n)
        .map(|_| {
            let r = pool.remove((next() % pool.len() as u64) as usize);
            Slot {
                id: format!("{}{}", kind.prefix(), r.id),
                ..Slot::default()
            }
        })
        .collect()
}

fn seed(period: i64, who: u128) -> u64 {
    (period as u64).wrapping_mul(1_000_003) ^ (who as u64) ^ ((who >> 64) as u64)
}

/// 납품 퀘스트를 모두 끝냈을 때 받는 것 (보상만 담은 가짜 퀘스트).
fn supply_set(cfg: &QuestConfig, kind: SupplyKind) -> QuestDef {
    let name = match kind {
        SupplyKind::Daily => "오늘의 납품",
        SupplyKind::Weekly => "이번 주 납품",
        SupplyKind::Guild => "길드 납품",
    };
    QuestDef {
        id: format!("{}all", kind.prefix()),
        name: format!("{name} 모두"),
        description: String::new(),
        objective: super::config::Objective {
            kind: "deliver".into(),
            filter: Vec::new(),
        },
        target: 1,
        rewards: cfg.supply.rewards(kind).clone(),
        repeatable: false,
    }
}

/// 날·주가 바뀌었으면 일일·주간 퀘스트를 새로 뽑습니다.
fn refresh_player(p: &mut PlayerQuests, uuid: u128, cfg: &QuestConfig, now: i64) {
    let day = day_of(now);
    if p.day != day {
        p.day = day;
        p.daily = pick_supply(cfg, SupplyKind::Daily, seed(day, uuid));
        p.daily_paid = false;
        p.tracked.retain(|(k, _)| *k != Kind::Daily);
    }
    let week = week_of(now);
    if p.supply_week != week {
        p.supply_week = week;
        p.wsupply = pick_supply(cfg, SupplyKind::Weekly, seed(week, uuid ^ 0x5157));
        p.wsupply_paid = false;
    }
    if p.week != week || p.weekly.is_empty() && !cfg.weekly.is_empty() {
        p.week = week;
        p.weekly = pick(&cfg.weekly, cfg.weekly_count, week)
            .into_iter()
            .map(|id| Slot {
                id,
                ..Slot::default()
            })
            .collect();
        p.tracked.retain(|(k, _)| *k != Kind::Weekly);
    }
    // 설정에서 사라진 퀘스트는 버림
    p.accepted
        .retain(|s| cfg.normal.iter().any(|d| d.id == s.id));
}

fn refresh_guild(g: &mut GuildQuests, gid: u32, cfg: &QuestConfig, now: i64) {
    let day = day_of(now);
    if g.day != day {
        g.day = day;
        g.supply = pick_supply(cfg, SupplyKind::Guild, seed(day, gid as u128 + 0x6175));
        g.supply_contrib.clear();
        g.supply_paid = false;
    }
    let week = week_of(now);
    if g.week != week || g.quests.is_empty() && !cfg.guild_weekly.is_empty() {
        g.week = week;
        g.quests = pick(&cfg.guild_weekly, cfg.guild_weekly_count, week * 31 + 7)
            .into_iter()
            .map(|id| Slot {
                id,
                ..Slot::default()
            })
            .collect();
        g.contrib.clear();
    }
}

fn matches(def: &QuestDef, h: &Happened) -> bool {
    def.objective.kind == h.kind
        && (def.objective.filter.is_empty()
            || def
                .objective
                .filter
                .iter()
                .any(|f| name_matches(f, &h.name)))
}

impl Progress {
    pub fn player_mut(&mut self, uuid: u128, cfg: &QuestConfig, now: i64) -> &mut PlayerQuests {
        let p = self.players.entry(uuid).or_default();
        refresh_player(p, uuid, cfg, now);
        p
    }

    pub fn guild_mut(&mut self, guild: u32, cfg: &QuestConfig, now: i64) -> &mut GuildQuests {
        let g = self.guilds.entry(guild).or_default();
        refresh_guild(g, guild, cfg, now);
        g
    }

    pub fn accept(
        &mut self,
        uuid: u128,
        id: &str,
        cfg: &QuestConfig,
        now: i64,
    ) -> Result<String, String> {
        let def = cfg
            .normal
            .iter()
            .find(|d| d.id == id)
            .ok_or("그런 퀘스트가 없습니다.")?;
        let max = cfg.max_tracked;
        let p = self.player_mut(uuid, cfg, now);
        if p.accepted.iter().any(|s| s.id == id) {
            return Err("이미 진행 중입니다.".into());
        }
        if p.finished.iter().any(|f| f == id) {
            return Err("이미 끝낸 퀘스트입니다.".into());
        }
        p.accepted.push(Slot {
            id: id.to_owned(),
            ..Slot::default()
        });
        if p.tracked.len() < max {
            p.tracked.push((Kind::Normal, id.to_owned()));
        }
        Ok(format!("퀘스트를 받았습니다: {}", def.name))
    }

    pub fn abandon(&mut self, uuid: u128, id: &str) -> Result<String, String> {
        let p = self.players.get_mut(&uuid).ok_or("진행 중이 아닙니다.")?;
        let before = p.accepted.len();
        p.accepted.retain(|s| s.id != id);
        p.tracked.retain(|(k, t)| !(*k == Kind::Normal && t == id));
        if p.accepted.len() == before {
            return Err("진행 중이 아닙니다.".into());
        }
        Ok("퀘스트를 포기했습니다.".into())
    }

    /// HUD 따라다니기 켜고 끄기.
    pub fn toggle_track(
        &mut self,
        uuid: u128,
        kind: Kind,
        id: &str,
        cfg: &QuestConfig,
        now: i64,
    ) -> Result<String, String> {
        let max = cfg.max_tracked;
        let p = self.player_mut(uuid, cfg, now);
        let key = (kind, id.to_owned());
        if let Some(i) = p.tracked.iter().position(|t| *t == key) {
            p.tracked.remove(i);
            return Ok("HUD에서 뺐습니다.".into());
        }
        if p.tracked.len() >= max {
            p.tracked.remove(0);
        }
        p.tracked.push(key);
        Ok("HUD에 띄웁니다.".into())
    }

    /// 일어난 일을 셉니다. 끝난 퀘스트들.
    pub fn record(
        &mut self,
        uuid: u128,
        guild: Option<u32>,
        h: &Happened,
        cfg: &QuestConfig,
        now: i64,
    ) -> Vec<Completed> {
        let mut out = Vec::new();
        {
            let p = self.player_mut(uuid, cfg, now);
            let mut finished_normal = Vec::new();
            for s in p.accepted.iter_mut() {
                let Some(def) = cfg.normal.iter().find(|d| d.id == s.id) else {
                    continue;
                };
                if matches(def, h) && !s.done {
                    s.progress = (s.progress + h.amount).min(def.target);
                    if s.progress >= def.target {
                        s.done = true;
                        finished_normal.push(def.clone());
                    }
                }
            }
            for s in p.weekly.iter_mut() {
                let Some(def) = cfg.weekly.iter().find(|d| d.id == s.id) else {
                    continue;
                };
                if matches(def, h) && !s.done {
                    s.progress = (s.progress + h.amount).min(def.target);
                    if s.progress >= def.target {
                        s.done = true;
                        out.push(Completed {
                            kind: Kind::Weekly,
                            def: def.clone(),
                            guild: None,
                            contributors: Vec::new(),
                        });
                    }
                }
            }
            for def in finished_normal {
                Self::finish_normal(p, &def);
                out.push(Completed {
                    kind: Kind::Normal,
                    def,
                    guild: None,
                    contributors: Vec::new(),
                });
            }
            p.tracked.retain(|(k, id)| match k {
                Kind::Weekly => p
                    .weekly
                    .iter()
                    .chain(&p.wsupply)
                    .any(|s| &s.id == id && !s.done),
                Kind::Daily => p.daily.iter().any(|s| &s.id == id && !s.done),
                Kind::Normal => p.accepted.iter().any(|s| &s.id == id),
                Kind::Guild => true,
            });
        }
        if let Some(gid) = guild {
            let g = self.guild_mut(gid, cfg, now);
            for s in g.quests.iter_mut() {
                let Some(def) = cfg.guild_weekly.iter().find(|d| d.id == s.id) else {
                    continue;
                };
                if matches(def, h) && !s.done {
                    let before = s.progress;
                    s.progress = (s.progress + h.amount).min(def.target);
                    *g.contrib.entry(uuid).or_default() += s.progress - before;
                    if s.progress >= def.target {
                        s.done = true;
                        out.push(Completed {
                            kind: Kind::Guild,
                            def: def.clone(),
                            guild: Some(gid),
                            contributors: Vec::new(),
                        });
                    }
                }
            }
        }
        out
    }

    fn finish_normal(p: &mut PlayerQuests, def: &QuestDef) {
        p.accepted.retain(|s| s.id != def.id);
        if !def.repeatable && !p.finished.contains(&def.id) {
            p.finished.push(def.id.clone());
        }
        p.tracked
            .retain(|(k, id)| !(*k == Kind::Normal && *id == def.id));
    }

    /// 내기 퀘스트에 아이템을 냅니다: `have`개 있을 때 받아 간 수와 이번에 끝난 것들
    /// (납품은 하나하나가 아니라 그날·그 주의 것을 모두 냈을 때).
    #[allow(clippy::too_many_arguments)]
    pub fn deliver(
        &mut self,
        uuid: u128,
        guild: Option<u32>,
        kind: Kind,
        id: &str,
        have: u32,
        cfg: &QuestConfig,
        now: i64,
    ) -> Result<(u32, Vec<Completed>), String> {
        let def = cfg.find(id).ok_or("그런 퀘스트가 없습니다.")?;
        if def.objective.kind != "deliver" {
            return Err("아이템을 내는 퀘스트가 아닙니다.".into());
        }
        let supply = [SupplyKind::Daily, SupplyKind::Weekly, SupplyKind::Guild]
            .into_iter()
            .find(|k| id.starts_with(k.prefix()));
        let gid = guild.filter(|_| kind == Kind::Guild);
        if kind == Kind::Guild && gid.is_none() {
            return Err("길드에 들어가 있지 않습니다.".into());
        }
        let slots: &mut Vec<Slot> = match (kind, gid) {
            (Kind::Normal, _) => &mut self.player_mut(uuid, cfg, now).accepted,
            (Kind::Daily, _) => &mut self.player_mut(uuid, cfg, now).daily,
            (Kind::Weekly, _) if supply.is_some() => &mut self.player_mut(uuid, cfg, now).wsupply,
            (Kind::Weekly, _) => &mut self.player_mut(uuid, cfg, now).weekly,
            (Kind::Guild, Some(g)) if supply.is_some() => &mut self.guild_mut(g, cfg, now).supply,
            (Kind::Guild, Some(g)) => &mut self.guild_mut(g, cfg, now).quests,
            (Kind::Guild, None) => unreachable!(),
        };
        let slot = slots
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or("진행 중인 퀘스트가 아닙니다.")?;
        if slot.done {
            return Err("이미 끝냈습니다.".into());
        }
        let take = have.min(def.target - slot.progress);
        if take == 0 {
            return Err("낼 아이템이 없습니다.".into());
        }
        slot.progress += take;
        let done = slot.progress >= def.target;
        if done {
            slot.done = true;
        }
        let all_done = slots.iter().all(|s| s.done);
        let mut out = Vec::new();
        if let Some(g) = gid {
            let gq = self.guild_mut(g, cfg, now);
            let contrib = if supply.is_some() {
                &mut gq.supply_contrib
            } else {
                &mut gq.contrib
            };
            *contrib.entry(uuid).or_default() += take;
        }
        match supply {
            // 납품: 그날(그 주)의 것을 모두 냈을 때 한 번
            Some(sk) if all_done => {
                let paid = match (sk, gid) {
                    (SupplyKind::Guild, Some(g)) => &mut self.guild_mut(g, cfg, now).supply_paid,
                    (SupplyKind::Weekly, _) => &mut self.player_mut(uuid, cfg, now).wsupply_paid,
                    _ => &mut self.player_mut(uuid, cfg, now).daily_paid,
                };
                if !std::mem::replace(paid, true) {
                    let contributors = match gid {
                        Some(g) => self
                            .guild_mut(g, cfg, now)
                            .supply_contrib
                            .keys()
                            .copied()
                            .collect(),
                        None => Vec::new(),
                    };
                    out.push(Completed {
                        kind,
                        def: supply_set(cfg, sk),
                        guild: gid,
                        contributors,
                    });
                }
            }
            Some(_) => {}
            None if done => {
                if kind == Kind::Normal {
                    Self::finish_normal(self.player_mut(uuid, cfg, now), &def);
                }
                out.push(Completed {
                    kind,
                    def,
                    guild: gid,
                    contributors: Vec::new(),
                });
            }
            None => {}
        }
        Ok((take, out))
    }

    /// 납품을 모두 냈을 때의 보상 (창에 보여 주려고).
    pub fn supply_rewards(cfg: &QuestConfig, kind: Kind) -> Option<&Rewards> {
        match kind {
            Kind::Daily => Some(&cfg.supply.daily_rewards),
            Kind::Weekly => Some(&cfg.supply.weekly_rewards),
            Kind::Guild => Some(&cfg.supply.guild_rewards),
            Kind::Normal => None,
        }
    }

    /// HUD에 띄울 퀘스트 (따라다니게 한 것, 없으면 끝나지 않은 주간·길드·일반 순).
    pub fn hud(
        &self,
        uuid: u128,
        guild: Option<u32>,
        cfg: &QuestConfig,
        now: i64,
    ) -> Vec<HudQuest> {
        // 1초마다 불리므로 파일을 고치지 않고 (주가 바뀐 것은 사본에서만) 봅니다
        let max = cfg.max_tracked;
        let mut all: Vec<(Kind, String, u32, bool)> = Vec::new();
        let mut p = self.players.get(&uuid).cloned().unwrap_or_default();
        refresh_player(&mut p, uuid, cfg, now);
        all.extend(
            p.daily
                .iter()
                .map(|s| (Kind::Daily, s.id.clone(), s.progress, s.done)),
        );
        all.extend(
            p.weekly
                .iter()
                .chain(&p.wsupply)
                .map(|s| (Kind::Weekly, s.id.clone(), s.progress, s.done)),
        );
        if let Some(gid) = guild {
            let mut g = self.guilds.get(&gid).cloned().unwrap_or_default();
            refresh_guild(&mut g, gid, cfg, now);
            all.extend(
                g.quests
                    .iter()
                    .chain(&g.supply)
                    .map(|s| (Kind::Guild, s.id.clone(), s.progress, s.done)),
            );
        }
        all.extend(
            p.accepted
                .iter()
                .map(|s| (Kind::Normal, s.id.clone(), s.progress, s.done)),
        );
        let show = |(k, id, prog, done): &(Kind, String, u32, bool)| {
            cfg.find(id).map(|d| HudQuest {
                kind: *k,
                name: d.name.clone(),
                progress: *prog,
                target: d.target,
                done: *done,
            })
        };
        let tracked: Vec<HudQuest> = p
            .tracked
            .iter()
            .filter_map(|(k, id)| all.iter().find(|a| a.0 == *k && &a.1 == id))
            .filter_map(show)
            .take(max)
            .collect();
        if !tracked.is_empty() {
            return tracked;
        }
        all.iter()
            .filter(|a| !a.3)
            .filter_map(show)
            .take(max)
            .collect()
    }

    /// 창에 보일 목록: (퀘스트, 진행, 끝남, 수락함).
    pub fn list(
        &mut self,
        uuid: u128,
        guild: Option<u32>,
        kind: Kind,
        cfg: &QuestConfig,
        now: i64,
    ) -> Vec<(QuestDef, u32, bool, bool)> {
        match kind {
            Kind::Normal => {
                let p = self.player_mut(uuid, cfg, now).clone();
                cfg.normal
                    .iter()
                    .map(|d| {
                        let s = p.accepted.iter().find(|s| s.id == d.id);
                        let finished = p.finished.contains(&d.id);
                        (
                            d.clone(),
                            s.map_or(if finished { d.target } else { 0 }, |s| s.progress),
                            finished,
                            s.is_some(),
                        )
                    })
                    .collect()
            }
            Kind::Daily => {
                let p = self.player_mut(uuid, cfg, now).clone();
                slot_list(&p.daily, cfg)
            }
            Kind::Weekly => {
                let p = self.player_mut(uuid, cfg, now).clone();
                let mut v = slot_list(&p.weekly, cfg);
                v.extend(slot_list(&p.wsupply, cfg));
                v
            }
            Kind::Guild => match guild {
                None => Vec::new(),
                Some(gid) => {
                    let g = self.guild_mut(gid, cfg, now).clone();
                    let mut v = slot_list(&g.quests, cfg);
                    v.extend(slot_list(&g.supply, cfg));
                    v
                }
            },
        }
    }

    pub fn is_tracked(&self, uuid: u128, kind: Kind, id: &str) -> bool {
        self.players
            .get(&uuid)
            .is_some_and(|p| p.tracked.iter().any(|(k, t)| *k == kind && t == id))
    }
}

fn slot_list(slots: &[Slot], cfg: &QuestConfig) -> Vec<(QuestDef, u32, bool, bool)> {
    slots
        .iter()
        .filter_map(|s| Some((cfg.find(&s.id)?, s.progress, s.done, true)))
        .collect()
}

// ---- JSON ----

fn slots_json(l: &[Slot]) -> Json {
    Json::Array(
        l.iter()
            .map(|s| {
                Json::Object(vec![
                    ("id".into(), Json::String(s.id.clone())),
                    ("progress".into(), Json::Number(s.progress as f64)),
                    ("done".into(), Json::Bool(s.done)),
                ])
            })
            .collect(),
    )
}

fn slots_from(j: Option<&Json>) -> Vec<Slot> {
    j.and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|s| {
            Some(Slot {
                id: s.get("id")?.as_str()?.to_owned(),
                progress: s.get("progress").and_then(Json::as_f64).unwrap_or(0.0) as u32,
                done: s.get("done").and_then(Json::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

fn contrib_json(c: &BTreeMap<u128, u32>) -> Json {
    Json::Object(
        c.iter()
            .map(|(u, n)| (format!("{u:032x}"), Json::Number(*n as f64)))
            .collect(),
    )
}

fn contrib_from(j: Option<&Json>) -> BTreeMap<u128, u32> {
    let mut out = BTreeMap::new();
    if let Some(Json::Object(c)) = j {
        for (u, n) in c {
            if let (Ok(u), Some(n)) = (u128::from_str_radix(u, 16), n.as_f64()) {
                out.insert(u, n as u32);
            }
        }
    }
    out
}

pub fn encode(p: &Progress) -> Json {
    let players = p
        .players
        .iter()
        .map(|(u, q)| {
            (
                format!("{u:032x}"),
                Json::Object(vec![
                    ("accepted".into(), slots_json(&q.accepted)),
                    (
                        "finished".into(),
                        Json::Array(q.finished.iter().map(|f| Json::String(f.clone())).collect()),
                    ),
                    ("week".into(), Json::Number(q.week as f64)),
                    ("weekly".into(), slots_json(&q.weekly)),
                    ("day".into(), Json::Number(q.day as f64)),
                    ("daily".into(), slots_json(&q.daily)),
                    ("daily_paid".into(), Json::Bool(q.daily_paid)),
                    ("supply_week".into(), Json::Number(q.supply_week as f64)),
                    ("wsupply".into(), slots_json(&q.wsupply)),
                    ("wsupply_paid".into(), Json::Bool(q.wsupply_paid)),
                    (
                        "tracked".into(),
                        Json::Array(
                            q.tracked
                                .iter()
                                .map(|(k, id)| Json::String(format!("{}:{id}", k.key())))
                                .collect(),
                        ),
                    ),
                ]),
            )
        })
        .collect();
    let guilds = p
        .guilds
        .iter()
        .map(|(g, q)| {
            (
                g.to_string(),
                Json::Object(vec![
                    ("week".into(), Json::Number(q.week as f64)),
                    ("quests".into(), slots_json(&q.quests)),
                    ("day".into(), Json::Number(q.day as f64)),
                    ("supply".into(), slots_json(&q.supply)),
                    ("supply_paid".into(), Json::Bool(q.supply_paid)),
                    ("supply_contrib".into(), contrib_json(&q.supply_contrib)),
                    ("contrib".into(), contrib_json(&q.contrib)),
                ]),
            )
        })
        .collect();
    Json::Object(vec![
        ("players".into(), Json::Object(players)),
        ("guilds".into(), Json::Object(guilds)),
    ])
}

pub fn decode(j: &Json) -> Option<Progress> {
    let num = |v: &Json, k: &str| v.get(k).and_then(Json::as_f64).unwrap_or(0.0) as i64;
    let flag = |v: &Json, k: &str| v.get(k).and_then(Json::as_bool).unwrap_or(false);
    let mut p = Progress::default();
    if let Some(Json::Object(players)) = j.get("players") {
        for (k, v) in players {
            let Ok(u) = u128::from_str_radix(k, 16) else {
                continue;
            };
            p.players.insert(
                u,
                PlayerQuests {
                    accepted: slots_from(v.get("accepted")),
                    finished: v
                        .get("finished")
                        .and_then(Json::as_array)
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|f| f.as_str().map(str::to_owned))
                        .collect(),
                    week: v.get("week").and_then(Json::as_f64).unwrap_or(0.0) as i64,
                    weekly: slots_from(v.get("weekly")),
                    day: num(v, "day"),
                    daily: slots_from(v.get("daily")),
                    daily_paid: flag(v, "daily_paid"),
                    supply_week: num(v, "supply_week"),
                    wsupply: slots_from(v.get("wsupply")),
                    wsupply_paid: flag(v, "wsupply_paid"),
                    tracked: v
                        .get("tracked")
                        .and_then(Json::as_array)
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|t| {
                            let (k, id) = t.as_str()?.split_once(':')?;
                            Some((Kind::parse(k)?, id.to_owned()))
                        })
                        .collect(),
                },
            );
        }
    }
    if let Some(Json::Object(guilds)) = j.get("guilds") {
        for (k, v) in guilds {
            let Ok(g) = k.parse::<u32>() else { continue };
            p.guilds.insert(
                g,
                GuildQuests {
                    week: num(v, "week"),
                    quests: slots_from(v.get("quests")),
                    contrib: contrib_from(v.get("contrib")),
                    day: num(v, "day"),
                    supply: slots_from(v.get("supply")),
                    supply_contrib: contrib_from(v.get("supply_contrib")),
                    supply_paid: flag(v, "supply_paid"),
                },
            );
        }
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONDAY: i64 = 1_792_335_600;

    fn ev(kind: &'static str, name: &str, amount: u32) -> Happened {
        Happened {
            kind,
            name: name.into(),
            amount,
        }
    }

    #[test]
    fn normal_weekly_and_guild_flow() {
        let cfg = QuestConfig::default();
        let mut p = Progress::default();
        // 일반: 수락해야 셈
        assert!(
            p.record(1, None, &ev("logs", "oak_log", 16), &cfg, MONDAY)
                .iter()
                .all(|c| c.kind != Kind::Normal)
        );
        p.accept(1, "first_wood", &cfg, MONDAY).unwrap();
        assert!(p.accept(1, "first_wood", &cfg, MONDAY).is_err());
        let done = p.record(1, None, &ev("logs", "oak_log", 16), &cfg, MONDAY);
        assert!(
            done.iter()
                .any(|c| c.kind == Kind::Normal && c.def.id == "first_wood")
        );
        assert!(
            p.accept(1, "first_wood", &cfg, MONDAY).is_err(),
            "다시 못 받음"
        );
        // 필터: 돌만
        p.accept(1, "stone_age", &cfg, MONDAY).unwrap();
        p.record(1, None, &ev("mine", "dirt", 64), &cfg, MONDAY);
        assert_eq!(p.players[&1].accepted[0].progress, 0);
        p.record(1, None, &ev("mine", "stone", 10), &cfg, MONDAY);
        assert_eq!(p.players[&1].accepted[0].progress, 10);
        // 주간: 저절로 3개, 같은 주는 모두 같음
        let weekly: Vec<_> = p.players[&1].weekly.iter().map(|s| s.id.clone()).collect();
        assert_eq!(weekly.len(), 3);
        p.player_mut(2, &cfg, MONDAY);
        assert_eq!(
            p.players[&2]
                .weekly
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>(),
            weekly
        );
        // 다음 주면 새로
        p.record(1, None, &ev("kill", "zombie", 1), &cfg, MONDAY + 7 * 86_400);
        assert_eq!(p.players[&1].week, week_of(MONDAY + 7 * 86_400));
        // 길드 주간: 둘이 함께, 공헌 따로
        let gq = p.guild_mut(9, &cfg, MONDAY).quests[0].id.clone();
        let def = cfg.find(&gq).unwrap().clone();
        let h = ev(
            Box::leak(def.objective.kind.clone().into_boxed_str()),
            "iron_ore",
            def.target - 1,
        );
        p.record(1, Some(9), &h, &cfg, MONDAY);
        let done = p.record(2, Some(9), &ev(h.kind, "iron_ore", 5), &cfg, MONDAY);
        assert!(
            done.iter()
                .any(|c| c.kind == Kind::Guild && c.guild == Some(9))
        );
        assert_eq!(p.guilds[&9].contrib[&2], 1);
        // 저장
        assert_eq!(decode(&encode(&p)).unwrap(), p);
    }

    #[test]
    fn deliver_and_hud() {
        let cfg = QuestConfig::default();
        let mut p = Progress::default();
        p.accept(1, "deliver_bread", &cfg, MONDAY).unwrap();
        let (took, done) = p
            .deliver(1, None, Kind::Normal, "deliver_bread", 4, &cfg, MONDAY)
            .unwrap();
        assert_eq!((took, done.is_empty()), (4, true));
        let (took, done) = p
            .deliver(1, None, Kind::Normal, "deliver_bread", 64, &cfg, MONDAY)
            .unwrap();
        assert_eq!(took, 6);
        assert_eq!(done.len(), 1);
        // HUD: 따라다니게 한 게 없으면 일일 납품·주간부터
        let hud = p.hud(1, None, &cfg, MONDAY);
        assert_eq!(hud.len(), 3);
        assert_eq!(hud[0].kind, Kind::Daily);
        assert!(hud.iter().all(|h| h.kind != Kind::Normal));
        let id = p.players[&1].weekly[1].id.clone();
        p.toggle_track(1, Kind::Weekly, &id, &cfg, MONDAY).unwrap();
        let hud = p.hud(1, None, &cfg, MONDAY);
        assert_eq!(hud.len(), 1);
        assert!(p.is_tracked(1, Kind::Weekly, &id));
    }

    #[test]
    fn supply_daily_weekly_and_guild() {
        let cfg = QuestConfig::default();
        let mut p = Progress::default();
        let me = 7u128;
        let daily = p.player_mut(me, &cfg, MONDAY).daily.clone();
        assert!((1..=3).contains(&daily.len()), "{daily:?}");
        // 물고기·특수 작물은 그 아이템이 없으니 뽑히지 않음
        assert!(
            daily
                .iter()
                .all(|s| !s.id.contains("carp") && !s.id.contains("cabbage"))
        );
        // 하나씩 내다 마지막에 "모두" 보상 한 번
        let mut got = Vec::new();
        for s in &daily {
            let def = cfg.find(&s.id).unwrap();
            let (took, done) = p
                .deliver(me, None, Kind::Daily, &s.id, 999, &cfg, MONDAY)
                .unwrap();
            assert_eq!(took, def.target);
            got.extend(done);
        }
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].def.rewards.autoplant, 50);
        assert_eq!(got[0].def.rewards.fly_minutes, 30);
        assert!(
            p.deliver(me, None, Kind::Daily, &daily[0].id, 9, &cfg, MONDAY)
                .is_err()
        );
        // 다음 날 새로
        p.player_mut(me, &cfg, MONDAY + 86_400);
        assert!(!p.players[&me].daily_paid);
        // 주간 납품은 3배
        let w = p.players[&me].wsupply[0].clone();
        let rid = w.id.strip_prefix("wsupply.").unwrap();
        let base = cfg
            .supply
            .resources
            .iter()
            .find(|r| r.id == rid)
            .unwrap()
            .amount;
        assert_eq!(cfg.find(&w.id).unwrap().target, base * 3);
        // 길드 납품: 2배, 둘이 나눠 내면 둘 다 보상
        let g = p.guild_mut(4, &cfg, MONDAY).supply.clone();
        let mut done = Vec::new();
        for (i, s) in g.iter().enumerate() {
            let target = cfg.find(&s.id).unwrap().target;
            let first = p
                .deliver(1, Some(4), Kind::Guild, &s.id, target / 2, &cfg, MONDAY)
                .unwrap();
            done.extend(first.1);
            let who = if i == 0 { 2 } else { 1 };
            done.extend(
                p.deliver(who, Some(4), Kind::Guild, &s.id, target, &cfg, MONDAY)
                    .unwrap()
                    .1,
            );
        }
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].guild, Some(4));
        let mut c = done[0].contributors.clone();
        c.sort();
        assert!(c.contains(&1));
        assert_eq!(decode(&encode(&p)).unwrap(), p);
    }
}
