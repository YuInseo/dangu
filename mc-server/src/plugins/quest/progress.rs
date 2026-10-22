//! 퀘스트 진행: 사람마다(일반·주간)와 길드마다(길드 주간). 패킷을 모르는 순수한 규칙.

use std::collections::BTreeMap;

use super::config::{QuestConfig, QuestDef};
use super::{name_matches, week_of};
use crate::json::Json;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Normal,
    Weekly,
    Guild,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Normal, Kind::Weekly, Kind::Guild];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Normal => "일반",
            Kind::Weekly => "주간",
            Kind::Guild => "길드 주간",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Kind::Normal => "normal",
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
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildQuests {
    pub week: i64,
    pub quests: Vec<Slot>,
    /// 이번 주 길드원마다 채운 양 (공헌).
    pub contrib: BTreeMap<u128, u32>,
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

/// 주가 바뀌었으면 주간 퀘스트를 새로 뽑습니다.
fn refresh_player(p: &mut PlayerQuests, cfg: &QuestConfig, now: i64) {
    let week = week_of(now);
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

fn refresh_guild(g: &mut GuildQuests, cfg: &QuestConfig, now: i64) {
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
        refresh_player(p, cfg, now);
        p
    }

    pub fn guild_mut(&mut self, guild: u32, cfg: &QuestConfig, now: i64) -> &mut GuildQuests {
        let g = self.guilds.entry(guild).or_default();
        refresh_guild(g, cfg, now);
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
                });
            }
            p.tracked.retain(|(k, id)| match k {
                Kind::Weekly => p.weekly.iter().any(|s| &s.id == id && !s.done),
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

    /// 내기 퀘스트에 아이템을 냅니다: `have`개 있을 때 받아 간 수와 끝났으면 그 퀘스트.
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
    ) -> Result<(u32, Option<Completed>), String> {
        let def = cfg.find(id).cloned().ok_or("그런 퀘스트가 없습니다.")?;
        if def.objective.kind != "deliver" {
            return Err("아이템을 내는 퀘스트가 아닙니다.".into());
        }
        let slot = match kind {
            Kind::Normal => self
                .player_mut(uuid, cfg, now)
                .accepted
                .iter_mut()
                .find(|s| s.id == id),
            Kind::Weekly => self
                .player_mut(uuid, cfg, now)
                .weekly
                .iter_mut()
                .find(|s| s.id == id),
            Kind::Guild => {
                let gid = guild.ok_or("길드에 들어가 있지 않습니다.")?;
                self.guild_mut(gid, cfg, now)
                    .quests
                    .iter_mut()
                    .find(|s| s.id == id)
            }
        }
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
        if kind == Kind::Guild
            && let Some(gid) = guild
        {
            *self
                .guild_mut(gid, cfg, now)
                .contrib
                .entry(uuid)
                .or_default() += take;
        }
        let completed = done.then(|| {
            if kind == Kind::Normal {
                Self::finish_normal(self.player_mut(uuid, cfg, now), &def);
            }
            Completed {
                kind,
                def: def.clone(),
                guild,
            }
        });
        Ok((take, completed))
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
        refresh_player(&mut p, cfg, now);
        all.extend(
            p.weekly
                .iter()
                .map(|s| (Kind::Weekly, s.id.clone(), s.progress, s.done)),
        );
        if let Some(gid) = guild {
            let mut g = self.guilds.get(&gid).cloned().unwrap_or_default();
            refresh_guild(&mut g, cfg, now);
            all.extend(
                g.quests
                    .iter()
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
            Kind::Weekly => {
                let p = self.player_mut(uuid, cfg, now).clone();
                p.weekly
                    .iter()
                    .filter_map(|s| {
                        Some((
                            cfg.weekly.iter().find(|d| d.id == s.id)?.clone(),
                            s.progress,
                            s.done,
                            true,
                        ))
                    })
                    .collect()
            }
            Kind::Guild => match guild {
                None => Vec::new(),
                Some(gid) => {
                    let g = self.guild_mut(gid, cfg, now).clone();
                    g.quests
                        .iter()
                        .filter_map(|s| {
                            Some((
                                cfg.guild_weekly.iter().find(|d| d.id == s.id)?.clone(),
                                s.progress,
                                s.done,
                                true,
                            ))
                        })
                        .collect()
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
                    (
                        "contrib".into(),
                        Json::Object(
                            q.contrib
                                .iter()
                                .map(|(u, n)| (format!("{u:032x}"), Json::Number(*n as f64)))
                                .collect(),
                        ),
                    ),
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
            let mut contrib = BTreeMap::new();
            if let Some(Json::Object(c)) = v.get("contrib") {
                for (u, n) in c {
                    if let (Ok(u), Some(n)) = (u128::from_str_radix(u, 16), n.as_f64()) {
                        contrib.insert(u, n as u32);
                    }
                }
            }
            p.guilds.insert(
                g,
                GuildQuests {
                    week: v.get("week").and_then(Json::as_f64).unwrap_or(0.0) as i64,
                    quests: slots_from(v.get("quests")),
                    contrib,
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
        assert_eq!((took, done.is_none()), (4, true));
        let (took, done) = p
            .deliver(1, None, Kind::Normal, "deliver_bread", 64, &cfg, MONDAY)
            .unwrap();
        assert_eq!(took, 6);
        assert!(done.is_some());
        // HUD: 따라다니게 한 게 없으면 주간부터
        let hud = p.hud(1, None, &cfg, MONDAY);
        assert_eq!(hud.len(), 3);
        assert!(hud.iter().all(|h| h.kind == Kind::Weekly));
        let id = p.players[&1].weekly[1].id.clone();
        p.toggle_track(1, Kind::Weekly, &id, &cfg, MONDAY).unwrap();
        let hud = p.hud(1, None, &cfg, MONDAY);
        assert_eq!(hud.len(), 1);
        assert!(p.is_tracked(1, Kind::Weekly, &id));
    }
}
