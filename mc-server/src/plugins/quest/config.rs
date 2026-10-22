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
    /// 자동심기 블록 (후원 등급의 그것)
    pub autoplant: u64,
    /// 플라이 시간 (분)
    pub fly_minutes: i64,
}

/// 납품 자원 하나 (일일·주간·길드 납품 퀘스트의 재료).
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub id: String,
    /// 묶음 (광물·물고기·농부·고기·나무)
    pub category: String,
    pub name: String,
    /// 받는 아이템 이름들 (`raw_iron`, `dangu:carp`, `*_planks`)
    pub items: Vec<String>,
    /// 일일 기준 개수
    pub amount: u32,
}

/// 납품 퀘스트: 날마다 자원 몇 개를 뽑고, 모두 내면 보상.
#[derive(Debug, Clone, PartialEq)]
pub struct Supply {
    pub resources: Vec<Resource>,
    /// 하루에 뽑는 자원 수 (이 사이 무작위)
    pub min_pick: usize,
    pub max_pick: usize,
    /// 주간 = 일일 개수 × 이만큼, 길드 일일 = × 이만큼
    pub weekly_mult: u32,
    pub guild_mult: u32,
    pub daily_rewards: Rewards,
    pub weekly_rewards: Rewards,
    /// 길드 납품을 모두 채우면: GP는 길드에, 나머지는 낸 길드원 모두에게
    pub guild_rewards: Rewards,
}

impl Default for Supply {
    fn default() -> Self {
        let r = |category: &str, id: &str, name: &str, items: &[&str], amount: u32| Resource {
            id: id.into(),
            category: category.into(),
            name: name.into(),
            items: items.iter().map(|s| (*s).to_owned()).collect(),
            amount,
        };
        Supply {
            resources: vec![
                r("광물", "cobblestone", "조약돌", &["cobblestone"], 32),
                r("광물", "obsidian", "흑요석", &["obsidian"], 5),
                r("광물", "coal", "석탄", &["coal"], 16),
                r(
                    "광물",
                    "copper",
                    "구리",
                    &["raw_copper", "copper_ingot"],
                    16,
                ),
                r("광물", "iron", "철", &["raw_iron", "iron_ingot"], 8),
                r("광물", "gold", "금", &["raw_gold", "gold_ingot"], 4),
                r("광물", "lapis", "청금석", &["lapis_lazuli"], 16),
                r("광물", "redstone", "레드스톤", &["redstone"], 16),
                r("물고기", "arowana", "아로와나", &["dangu:arowana"], 5),
                r("물고기", "carp", "잉어", &["dangu:carp"], 5),
                r("물고기", "minnow", "피라미", &["dangu:minnow"], 5),
                r("농부", "potato", "감자", &["potato"], 3),
                r("농부", "cabbage", "배추", &["dangu:cabbage"], 3),
                r("농부", "rice", "쌀", &["dangu:rice"], 3),
                r("농부", "asparagus", "아스파라거스", &["dangu:asparagus"], 3),
                r("농부", "paprika", "파프리카", &["dangu:paprika"], 3),
                r("농부", "wild_ginseng", "산삼", &["dangu:wild_ginseng"], 3),
                r("고기", "beef", "소고기", &["beef"], 3),
                r("고기", "chicken", "닭고기", &["chicken"], 3),
                r("고기", "mutton", "양고기", &["mutton"], 3),
                r("고기", "porkchop", "돼지고기", &["porkchop"], 3),
                r("나무", "planks", "목재", &["*_planks"], 32),
                r(
                    "나무",
                    "wood_bundle_1",
                    "목재 묶음 1",
                    &["dangu:wood_bundle_1"],
                    1,
                ),
                r(
                    "나무",
                    "wood_bundle_2",
                    "목재 묶음 2",
                    &["dangu:wood_bundle_2"],
                    2,
                ),
                r("나무", "golden_apple", "황금사과", &["golden_apple"], 1),
            ],
            min_pick: 1,
            max_pick: 3,
            weekly_mult: 3,
            guild_mult: 2,
            daily_rewards: Rewards {
                autoplant: 50,
                fly_minutes: 30,
                ..Rewards::default()
            },
            weekly_rewards: Rewards {
                autoplant: 100,
                fly_minutes: 60,
                ..Rewards::default()
            },
            guild_rewards: Rewards {
                gp: 300,
                autoplant: 50,
                fly_minutes: 30,
                ..Rewards::default()
            },
        }
    }
}

/// 납품 퀘스트의 종류 (퀘스트 id 앞부분).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplyKind {
    Daily,
    Weekly,
    Guild,
}

impl SupplyKind {
    pub fn prefix(self) -> &'static str {
        match self {
            SupplyKind::Daily => "daily.",
            SupplyKind::Weekly => "wsupply.",
            SupplyKind::Guild => "gsupply.",
        }
    }

    fn of(id: &str) -> Option<(SupplyKind, &str)> {
        [SupplyKind::Daily, SupplyKind::Weekly, SupplyKind::Guild]
            .into_iter()
            .find_map(|k| id.strip_prefix(k.prefix()).map(|r| (k, r)))
    }
}

impl Supply {
    pub fn mult(&self, kind: SupplyKind) -> u32 {
        match kind {
            SupplyKind::Daily => 1,
            SupplyKind::Weekly => self.weekly_mult.max(1),
            SupplyKind::Guild => self.guild_mult.max(1),
        }
    }

    pub fn rewards(&self, kind: SupplyKind) -> &Rewards {
        match kind {
            SupplyKind::Daily => &self.daily_rewards,
            SupplyKind::Weekly => &self.weekly_rewards,
            SupplyKind::Guild => &self.guild_rewards,
        }
    }

    /// 납품 퀘스트 하나 (`daily.cobblestone` → "조약돌 32개를 내세요").
    pub fn def(&self, id: &str) -> Option<QuestDef> {
        let (kind, rid) = SupplyKind::of(id)?;
        let r = self.resources.iter().find(|r| r.id == rid)?;
        let target = r.amount.max(1) * self.mult(kind);
        Some(QuestDef {
            id: id.to_owned(),
            name: r.name.clone(),
            description: format!("[{}] {} {target}개를 내세요", r.category, r.name),
            objective: Objective {
                kind: "deliver".into(),
                filter: r.items.clone(),
            },
            target,
            rewards: Rewards::default(),
            repeatable: false,
        })
    }
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
    /// 일일·주간·길드 납품.
    pub supply: Supply,
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
            supply: Supply::default(),
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
        let s = &self.supply;
        let mut rids = std::collections::HashSet::new();
        for r in &s.resources {
            if r.id.is_empty() || r.id.contains('.') || !rids.insert(r.id.clone()) {
                return Err(format!("납품 자원 id가 비었거나 겹칩니다: {}", r.id));
            }
            if r.amount == 0 || r.items.is_empty() {
                return Err(format!("{}: 개수는 1 이상, 아이템은 하나 이상", r.id));
            }
        }
        if s.min_pick == 0 || s.min_pick > s.max_pick {
            return Err("납품: 뽑는 수는 1 ≤ 최소 ≤ 최대".into());
        }
        Ok(())
    }

    /// 설정에 있는 퀘스트, 또는 납품 퀘스트.
    pub fn find(&self, id: &str) -> Option<QuestDef> {
        self.normal
            .iter()
            .chain(&self.weekly)
            .chain(&self.guild_weekly)
            .find(|d| d.id == id)
            .cloned()
            .or_else(|| self.supply.def(id))
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
        ("rewards".into(), rewards_json(&d.rewards)),
    ])
}

fn rewards_json(r: &Rewards) -> Json {
    Json::Object(vec![
        ("money".into(), n(r.money as f64)),
        ("cash".into(), n(r.cash as f64)),
        ("gp".into(), n(r.gp as f64)),
        ("autoplant".into(), n(r.autoplant as f64)),
        ("fly_minutes".into(), n(r.fly_minutes as f64)),
        (
            "items".into(),
            Json::Array(
                r.items
                    .iter()
                    .map(|(i, c)| {
                        Json::Object(vec![("item".into(), s(i)), ("count".into(), n(*c as f64))])
                    })
                    .collect(),
            ),
        ),
        (
            "job_xp".into(),
            Json::Array(
                r.job_xp
                    .iter()
                    .map(|(j, x)| Json::Object(vec![("job".into(), s(j)), ("xp".into(), n(*x))]))
                    .collect(),
            ),
        ),
    ])
}

fn rewards_from(r: Option<&Json>) -> Rewards {
    let num = |j: &Json, k: &str| j.get(k).and_then(Json::as_f64).unwrap_or(0.0);
    let list = |k: &str| {
        r.and_then(|r| r.get(k))
            .and_then(Json::as_array)
            .unwrap_or_default()
            .to_vec()
    };
    Rewards {
        money: r.map_or(0.0, |r| num(r, "money")) as i64,
        cash: r.map_or(0.0, |r| num(r, "cash")) as i64,
        gp: r.map_or(0.0, |r| num(r, "gp")) as i64,
        autoplant: r.map_or(0.0, |r| num(r, "autoplant")) as u64,
        fly_minutes: r.map_or(0.0, |r| num(r, "fly_minutes")) as i64,
        items: list("items")
            .iter()
            .filter_map(|i| Some((i.get("item")?.as_str()?.to_owned(), num(i, "count") as i32)))
            .collect(),
        job_xp: list("job_xp")
            .iter()
            .filter_map(|i| Some((i.get("job")?.as_str()?.to_owned(), num(i, "xp"))))
            .collect(),
    }
}

fn strings(j: Option<&Json>) -> Vec<String> {
    j.and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|f| f.as_str().map(str::to_owned))
        .collect()
}

fn def_from(j: &Json) -> Option<QuestDef> {
    let st = |j: &Json, k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
    let o = j.get("objective")?;
    Some(QuestDef {
        id: st(j, "id"),
        name: st(j, "name"),
        description: st(j, "description"),
        objective: Objective {
            kind: st(o, "kind"),
            filter: strings(o.get("filter")),
        },
        target: j.get("target").and_then(Json::as_f64).unwrap_or(0.0) as u32,
        repeatable: j.get("repeatable").and_then(Json::as_bool).unwrap_or(false),
        rewards: rewards_from(j.get("rewards")),
    })
}

fn supply_json(s: &Supply) -> Json {
    Json::Object(vec![
        (
            "resources".into(),
            Json::Array(
                s.resources
                    .iter()
                    .map(|r| {
                        Json::Object(vec![
                            ("id".into(), Json::String(r.id.clone())),
                            ("category".into(), Json::String(r.category.clone())),
                            ("name".into(), Json::String(r.name.clone())),
                            (
                                "items".into(),
                                Json::Array(
                                    r.items.iter().map(|i| Json::String(i.clone())).collect(),
                                ),
                            ),
                            ("amount".into(), n(r.amount as f64)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("min_pick".into(), n(s.min_pick as f64)),
        ("max_pick".into(), n(s.max_pick as f64)),
        ("weekly_mult".into(), n(s.weekly_mult as f64)),
        ("guild_mult".into(), n(s.guild_mult as f64)),
        ("daily_rewards".into(), rewards_json(&s.daily_rewards)),
        ("weekly_rewards".into(), rewards_json(&s.weekly_rewards)),
        ("guild_rewards".into(), rewards_json(&s.guild_rewards)),
    ])
}

fn supply_from(j: Option<&Json>) -> Supply {
    let d = Supply::default();
    let Some(j) = j else { return d };
    let num = |k: &str, def: f64| j.get(k).and_then(Json::as_f64).unwrap_or(def);
    let st = |j: &Json, k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
    Supply {
        resources: match j.get("resources").and_then(Json::as_array) {
            Some(a) => a
                .iter()
                .map(|r| Resource {
                    id: st(r, "id"),
                    category: st(r, "category"),
                    name: st(r, "name"),
                    items: strings(r.get("items")),
                    amount: r.get("amount").and_then(Json::as_f64).unwrap_or(1.0) as u32,
                })
                .collect(),
            None => d.resources.clone(),
        },
        min_pick: num("min_pick", d.min_pick as f64) as usize,
        max_pick: num("max_pick", d.max_pick as f64) as usize,
        weekly_mult: num("weekly_mult", d.weekly_mult as f64) as u32,
        guild_mult: num("guild_mult", d.guild_mult as f64) as u32,
        daily_rewards: j
            .get("daily_rewards")
            .map_or(d.daily_rewards.clone(), |r| rewards_from(Some(r))),
        weekly_rewards: j
            .get("weekly_rewards")
            .map_or(d.weekly_rewards.clone(), |r| rewards_from(Some(r))),
        guild_rewards: j
            .get("guild_rewards")
            .map_or(d.guild_rewards.clone(), |r| rewards_from(Some(r))),
    }
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
        ("supply".into(), supply_json(&c.supply)),
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
        supply: supply_from(j.get("supply")),
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
