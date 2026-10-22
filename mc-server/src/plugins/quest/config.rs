//! 퀘스트 내용 (`MC_QUESTS`, 기본 `world/quests.json`). 없으면 기본 퀘스트를 써 둡니다.

use std::sync::OnceLock;

use crate::json::Json;
use crate::plugins::store::{JsonFile, data_path};

/// 무엇을 세나.
#[derive(Debug, Clone, PartialEq)]
pub struct Objective {
    /// `mine` `ores` `logs` `harvest` `kill` `deliver`
    pub kind: String,
    /// 이름 거르기 (비면 아무거나).
    pub filter: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rewards {
    pub money: i64,
    pub cash: i64,
    /// (아이템 이름 또는 `ns:id`, 개수)
    pub items: Vec<(String, i32)>,
    /// (직업, 경험치)
    pub job_xp: Vec<(String, f64)>,
    /// 길드 주간: 길드에 GP
    pub gp: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuestDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub objective: Objective,
    pub target: u32,
    pub rewards: Rewards,
    /// 일반 퀘스트를 끝낸 뒤 다시 받을 수 있나.
    pub repeatable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuestConfig {
    pub normal: Vec<QuestDef>,
    /// 주간 퀘스트 후보 (주마다 `weekly_count`개를 뽑음).
    pub weekly: Vec<QuestDef>,
    pub weekly_count: usize,
    pub guild_weekly: Vec<QuestDef>,
    pub guild_weekly_count: usize,
    /// HUD에 따라다니게 할 퀘스트 수.
    pub max_tracked: usize,
}

fn q(
    id: &str,
    name: &str,
    desc: &str,
    kind: &str,
    filter: &[&str],
    target: u32,
    rewards: Rewards,
) -> QuestDef {
    QuestDef {
        id: id.into(),
        name: name.into(),
        description: desc.into(),
        objective: Objective {
            kind: kind.into(),
            filter: filter.iter().map(|s| (*s).to_owned()).collect(),
        },
        target,
        rewards,
        repeatable: false,
    }
}

fn money(m: i64) -> Rewards {
    Rewards {
        money: m,
        ..Rewards::default()
    }
}

impl Default for QuestConfig {
    fn default() -> Self {
        let mut wood = q(
            "first_wood",
            "첫 나무",
            "통나무 16개를 베세요",
            "logs",
            &[],
            16,
            money(500),
        );
        wood.rewards.items = vec![("bread".into(), 8)];
        let mut stone = q(
            "stone_age",
            "석기 시대",
            "돌 64개를 캐세요",
            "mine",
            &["stone", "cobblestone", "deepslate"],
            64,
            money(1_000),
        );
        stone.rewards.job_xp = vec![("광부".into(), 50.0)];
        let mut wheat = q(
            "harvest_wheat",
            "밀 농사",
            "다 자란 밀 32개를 거두세요",
            "harvest",
            &["wheat"],
            32,
            money(800),
        );
        wheat.repeatable = true;
        wheat.rewards.job_xp = vec![("농부".into(), 40.0)];
        let deliver = q(
            "deliver_bread",
            "빵 배달",
            "빵 10개를 내세요",
            "deliver",
            &["bread"],
            10,
            money(1_500),
        );
        let hunter = q(
            "zombie_hunter",
            "좀비 사냥",
            "좀비 20마리를 잡으세요",
            "kill",
            &["zombie", "husk"],
            20,
            money(2_000),
        );
        let mut weekly_ore = q(
            "weekly_ores",
            "광맥 탐사",
            "광석 200개를 캐세요",
            "ores",
            &[],
            200,
            money(20_000),
        );
        weekly_ore.rewards.job_xp = vec![("광부".into(), 300.0)];
        let mut weekly_crops = q(
            "weekly_harvest",
            "풍년",
            "다 자란 작물 500개를 거두세요",
            "harvest",
            &[],
            500,
            money(15_000),
        );
        weekly_crops.rewards.items = vec![("dangu:crop_booster".into(), 1)];
        let weekly_kills = q(
            "weekly_kills",
            "마을 지킴이",
            "몹 150마리를 잡으세요",
            "kill",
            &[],
            150,
            money(18_000),
        );
        let weekly_logs = q(
            "weekly_logs",
            "목재 공급",
            "통나무 400개를 베세요",
            "logs",
            &[],
            400,
            money(12_000),
        );
        let g = |id: &str, name: &str, desc: &str, kind: &str, target: u32, gp: i64| {
            let mut d = q(
                id,
                name,
                desc,
                kind,
                &[],
                target,
                Rewards {
                    gp,
                    items: vec![("dangu:crop_booster".into(), 1)],
                    ..Rewards::default()
                },
            );
            d.repeatable = false;
            d
        };
        Self {
            normal: vec![wood, stone, wheat, deliver, hunter],
            weekly: vec![weekly_ore, weekly_crops, weekly_kills, weekly_logs],
            weekly_count: 3,
            guild_weekly: vec![
                g(
                    "guild_mine",
                    "길드 채굴 작전",
                    "길드원 모두 블록 3000개를 캐세요",
                    "mine",
                    3_000,
                    800,
                ),
                g(
                    "guild_ores",
                    "길드 광맥",
                    "길드원 모두 광석 600개를 캐세요",
                    "ores",
                    600,
                    900,
                ),
                g(
                    "guild_harvest",
                    "길드 수확제",
                    "길드원 모두 작물 1500개를 거두세요",
                    "harvest",
                    1_500,
                    800,
                ),
                g(
                    "guild_hunt",
                    "길드 토벌",
                    "길드원 모두 몹 500마리를 잡으세요",
                    "kill",
                    500,
                    900,
                ),
            ],
            guild_weekly_count: 3,
            max_tracked: 3,
        }
    }
}

impl QuestConfig {
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        for d in self
            .normal
            .iter()
            .chain(&self.weekly)
            .chain(&self.guild_weekly)
        {
            if d.id.is_empty() || !ids.insert(d.id.clone()) {
                return Err(format!("퀘스트 id가 비었거나 겹칩니다: {}", d.id));
            }
            if d.target == 0 {
                return Err(format!("{}: target은 1 이상", d.id));
            }
            if !["mine", "ores", "logs", "harvest", "kill", "deliver"]
                .contains(&d.objective.kind.as_str())
            {
                return Err(format!("{}: 모르는 목표 {}", d.id, d.objective.kind));
            }
        }
        Ok(())
    }

    pub fn find(&self, id: &str) -> Option<&QuestDef> {
        self.normal
            .iter()
            .chain(&self.weekly)
            .chain(&self.guild_weekly)
            .find(|d| d.id == id)
    }
}

// ---- JSON ----

fn n(v: f64) -> Json {
    Json::Number(v)
}
fn s(v: &str) -> Json {
    Json::String(v.to_owned())
}

fn def_json(d: &QuestDef) -> Json {
    let r = &d.rewards;
    Json::Object(vec![
        ("id".into(), s(&d.id)),
        ("name".into(), s(&d.name)),
        ("description".into(), s(&d.description)),
        (
            "objective".into(),
            Json::Object(vec![
                ("kind".into(), s(&d.objective.kind)),
                (
                    "filter".into(),
                    Json::Array(d.objective.filter.iter().map(|f| s(f)).collect()),
                ),
            ]),
        ),
        ("target".into(), n(d.target as f64)),
        ("repeatable".into(), Json::Bool(d.repeatable)),
        (
            "rewards".into(),
            Json::Object(vec![
                ("money".into(), n(r.money as f64)),
                ("cash".into(), n(r.cash as f64)),
                ("gp".into(), n(r.gp as f64)),
                (
                    "items".into(),
                    Json::Array(
                        r.items
                            .iter()
                            .map(|(i, c)| {
                                Json::Object(vec![
                                    ("item".into(), s(i)),
                                    ("count".into(), n(*c as f64)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    "job_xp".into(),
                    Json::Array(
                        r.job_xp
                            .iter()
                            .map(|(j, x)| {
                                Json::Object(vec![("job".into(), s(j)), ("xp".into(), n(*x))])
                            })
                            .collect(),
                    ),
                ),
            ]),
        ),
    ])
}

fn def_from(j: &Json) -> Option<QuestDef> {
    let st = |j: &Json, k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
    let num = |j: &Json, k: &str| j.get(k).and_then(Json::as_f64).unwrap_or(0.0);
    let o = j.get("objective")?;
    let r = j.get("rewards");
    let list = |k: &str| {
        r.and_then(|r| r.get(k))
            .and_then(Json::as_array)
            .unwrap_or_default()
            .to_vec()
    };
    Some(QuestDef {
        id: st(j, "id"),
        name: st(j, "name"),
        description: st(j, "description"),
        objective: Objective {
            kind: st(o, "kind"),
            filter: o
                .get("filter")
                .and_then(Json::as_array)
                .unwrap_or_default()
                .iter()
                .filter_map(|f| f.as_str().map(str::to_owned))
                .collect(),
        },
        target: num(j, "target") as u32,
        repeatable: j.get("repeatable").and_then(Json::as_bool).unwrap_or(false),
        rewards: Rewards {
            money: r.map_or(0.0, |r| num(r, "money")) as i64,
            cash: r.map_or(0.0, |r| num(r, "cash")) as i64,
            gp: r.map_or(0.0, |r| num(r, "gp")) as i64,
            items: list("items")
                .iter()
                .filter_map(|i| Some((i.get("item")?.as_str()?.to_owned(), num(i, "count") as i32)))
                .collect(),
            job_xp: list("job_xp")
                .iter()
                .filter_map(|i| Some((i.get("job")?.as_str()?.to_owned(), num(i, "xp"))))
                .collect(),
        },
    })
}

pub fn encode(c: &QuestConfig) -> Json {
    let list = |l: &[QuestDef]| Json::Array(l.iter().map(def_json).collect());
    Json::Object(vec![
        ("normal".into(), list(&c.normal)),
        ("weekly".into(), list(&c.weekly)),
        ("weekly_count".into(), n(c.weekly_count as f64)),
        ("guild_weekly".into(), list(&c.guild_weekly)),
        ("guild_weekly_count".into(), n(c.guild_weekly_count as f64)),
        ("max_tracked".into(), n(c.max_tracked as f64)),
    ])
}

pub fn decode(j: &Json) -> Option<QuestConfig> {
    let d = QuestConfig::default();
    let list = |k: &str, def: &[QuestDef]| match j.get(k).and_then(Json::as_array) {
        Some(a) => a.iter().filter_map(def_from).collect(),
        None => def.to_vec(),
    };
    let num = |k: &str, def: usize| j.get(k).and_then(Json::as_f64).map_or(def, |v| v as usize);
    Some(QuestConfig {
        normal: list("normal", &d.normal),
        weekly: list("weekly", &d.weekly),
        weekly_count: num("weekly_count", d.weekly_count),
        guild_weekly: list("guild_weekly", &d.guild_weekly),
        guild_weekly_count: num("guild_weekly_count", d.guild_weekly_count),
        max_tracked: num("max_tracked", d.max_tracked).clamp(1, 3),
    })
}

static FILE: OnceLock<JsonFile<QuestConfig>> = OnceLock::new();

fn file() -> &'static JsonFile<QuestConfig> {
    FILE.get_or_init(|| JsonFile::in_memory(encode, decode))
}

pub fn open_global() {
    let _ = FILE.set(JsonFile::open(
        data_path("MC_QUESTS", "quests.json"),
        encode,
        decode,
    ));
    if let Some(p) = file().path()
        && !p.exists()
    {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, encode(&QuestConfig::default()).pretty());
    }
}

pub fn with<R>(f: impl FnOnce(&QuestConfig) -> R) -> R {
    file().read(f)
}

pub fn get() -> QuestConfig {
    file().read(|c| c.clone())
}

pub fn set(c: QuestConfig) -> Result<(), String> {
    c.validate()?;
    file().write(|old| *old = c);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_valid_and_round_trip() {
        let c = QuestConfig::default();
        c.validate().unwrap();
        assert_eq!(decode(&encode(&c)).unwrap(), c);
        assert!(c.find("weekly_ores").is_some());
    }
}
