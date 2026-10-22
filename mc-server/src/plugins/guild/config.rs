//! 길드 설정 (`MC_GUILD_CONFIG`, 기본 `world/guild_config.json`): 레벨별 효과, 업그레이드, 청크
//! 작물 단계, 창고를 막을 세션 등. 웹 관리 페이지([`crate::plugins::webadmin`])에서 고치면 모든
//! 인스턴스가 0.5초 안에 다시 읽습니다. 파일이 없으면 아래 기본값.

use std::sync::OnceLock;

use crate::json::Json;
use crate::plugins::store::{JsonFile, data_path};

/// 레벨 한 줄: 이 레벨이 되는 누적 경험치와 그때의 효과.
#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    pub level: u32,
    /// 이 레벨에 닿는 누적 길드 경험치.
    pub exp: i64,
    pub max_members: usize,
    pub max_lands: usize,
    pub storage_pages: usize,
    /// 신호기 효과를 몇 개까지 열 수 있나.
    pub beacon_slots: usize,
    /// 길드원의 생활 직업 경험치 보너스 (%).
    pub job_xp_bonus: f64,
    /// 광석 덤 확률 보너스 (%p).
    pub rare_bonus: f64,
}

/// 돈으로 사는 업그레이드 (레벨 효과 위에 더함).
#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeRule {
    /// `land`, `members`, `storage`
    pub kind: String,
    pub max_level: u8,
    /// 한 단계마다 더하는 양.
    pub per_level: usize,
    /// 단계 n → n+1 값 = base_cost × (n+1).
    pub base_cost: i64,
    /// 단계 n → n+1에 필요한 길드 레벨 = n+1 + required_level_offset.
    pub required_level_offset: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub levels: Vec<Level>,
    pub upgrades: Vec<UpgradeRule>,
    /// 청크 작물 단계별 성장 배율 (0단계 = 1.0).
    pub crop_levels: Vec<f64>,
    /// 길드 창고를 열 수 없는 세션(인스턴스) 이름.
    pub storage_blocked_instances: Vec<String>,
    pub create_cost: i64,
    pub land_price: i64,
    /// 땅을 팔 때 돌려받는 비율 (0~1).
    pub land_refund: f64,
    pub event_cost_gp: i64,
    pub event_minutes: i64,
    pub max_officers: usize,
}

impl Default for Config {
    fn default() -> Self {
        let levels = (1..=10)
            .map(|n: u32| {
                let n64 = n as i64;
                Level {
                    level: n,
                    // 1→2 1000, 2→3 2000, … (누적)
                    exp: 500 * (n64 - 1) * n64,
                    max_members: 10 + 2 * (n as usize - 1),
                    max_lands: 4 + 2 * (n as usize - 1),
                    storage_pages: 1 + (n as usize - 1) / 3,
                    beacon_slots: (1 + (n as usize - 1) / 2).min(9),
                    job_xp_bonus: (n as f64 - 1.0) * 2.0,
                    rare_bonus: (n as f64 - 1.0) * 0.5,
                }
            })
            .collect();
        Self {
            levels,
            upgrades: vec![
                UpgradeRule {
                    kind: "land".into(),
                    max_level: 10,
                    per_level: 4,
                    base_cost: 30_000,
                    required_level_offset: 0,
                },
                UpgradeRule {
                    kind: "members".into(),
                    max_level: 8,
                    per_level: 5,
                    base_cost: 50_000,
                    required_level_offset: 0,
                },
                UpgradeRule {
                    kind: "storage".into(),
                    max_level: 5,
                    per_level: 1,
                    base_cost: 40_000,
                    required_level_offset: 0,
                },
            ],
            crop_levels: vec![1.0, 1.25, 1.5, 1.75, 2.0, 2.5],
            storage_blocked_instances: Vec::new(),
            create_cost: 10_000,
            land_price: 10_000,
            land_refund: 0.5,
            event_cost_gp: 500,
            event_minutes: 60,
            max_officers: 2,
        }
    }
}

impl Config {
    /// 경험치로 레벨 줄 (표에서 닿은 가장 높은 줄).
    pub fn level_row(&self, exp: i64) -> &Level {
        self.levels
            .iter()
            .filter(|l| l.exp <= exp)
            .max_by_key(|l| l.level)
            .or_else(|| self.levels.first())
            .expect("levels가 비었습니다")
    }

    /// 다음 레벨 줄.
    pub fn next_level(&self, exp: i64) -> Option<&Level> {
        let now = self.level_row(exp).level;
        self.levels
            .iter()
            .filter(|l| l.level > now)
            .min_by_key(|l| l.level)
    }

    pub fn upgrade(&self, kind: &str) -> Option<&UpgradeRule> {
        self.upgrades.iter().find(|u| u.kind == kind)
    }

    pub fn crop_multiplier(&self, level: u8) -> f64 {
        self.crop_levels
            .get(level as usize)
            .or(self.crop_levels.last())
            .copied()
            .unwrap_or(1.0)
    }

    pub fn max_crop_level(&self) -> u8 {
        self.crop_levels.len().saturating_sub(1) as u8
    }

    /// 고친 설정이 쓸 만한가 (웹에서 저장할 때).
    pub fn validate(&self) -> Result<(), String> {
        if self.levels.is_empty() {
            return Err("levels가 하나는 있어야 합니다".into());
        }
        let mut seen = std::collections::HashSet::new();
        for l in &self.levels {
            if !seen.insert(l.level) {
                return Err(format!("레벨 {}이 두 번 있습니다", l.level));
            }
            if l.max_members == 0 || l.storage_pages == 0 || l.storage_pages > 54 {
                return Err(format!("레벨 {}: 인원·창고 값이 이상합니다", l.level));
            }
        }
        if !self.levels.iter().any(|l| l.exp <= 0) {
            return Err("경험치 0에 닿는 레벨(시작 레벨)이 있어야 합니다".into());
        }
        if self.crop_levels.is_empty() || self.crop_levels.iter().any(|m| *m <= 0.0 || *m > 20.0) {
            return Err("crop_levels는 0보다 크고 20 이하인 배율 목록이어야 합니다".into());
        }
        if !(0.0..=1.0).contains(&self.land_refund) {
            return Err("land_refund는 0~1".into());
        }
        Ok(())
    }
}

// ---- JSON ----

fn n(v: f64) -> Json {
    Json::Number(v)
}

pub fn encode(c: &Config) -> Json {
    let obj = |f: Vec<(&str, Json)>| {
        Json::Object(f.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    };
    obj(vec![
        (
            "levels",
            Json::Array(
                c.levels
                    .iter()
                    .map(|l| {
                        obj(vec![
                            ("level", n(l.level as f64)),
                            ("exp", n(l.exp as f64)),
                            ("max_members", n(l.max_members as f64)),
                            ("max_lands", n(l.max_lands as f64)),
                            ("storage_pages", n(l.storage_pages as f64)),
                            ("beacon_slots", n(l.beacon_slots as f64)),
                            ("job_xp_bonus", n(l.job_xp_bonus)),
                            ("rare_bonus", n(l.rare_bonus)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "upgrades",
            Json::Array(
                c.upgrades
                    .iter()
                    .map(|u| {
                        obj(vec![
                            ("kind", Json::String(u.kind.clone())),
                            ("max_level", n(u.max_level as f64)),
                            ("per_level", n(u.per_level as f64)),
                            ("base_cost", n(u.base_cost as f64)),
                            ("required_level_offset", n(u.required_level_offset as f64)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "crop_levels",
            Json::Array(c.crop_levels.iter().map(|&m| n(m)).collect()),
        ),
        (
            "storage_blocked_instances",
            Json::Array(
                c.storage_blocked_instances
                    .iter()
                    .map(|s| Json::String(s.clone()))
                    .collect(),
            ),
        ),
        ("create_cost", n(c.create_cost as f64)),
        ("land_price", n(c.land_price as f64)),
        ("land_refund", n(c.land_refund)),
        ("event_cost_gp", n(c.event_cost_gp as f64)),
        ("event_minutes", n(c.event_minutes as f64)),
        ("max_officers", n(c.max_officers as f64)),
    ])
}

/// 빠진 칸은 기본값으로.
pub fn decode(j: &Json) -> Option<Config> {
    let d = Config::default();
    let num = |j: &Json, k: &str, def: f64| j.get(k).and_then(Json::as_f64).unwrap_or(def);
    let levels = match j.get("levels").and_then(Json::as_array) {
        Some(list) => list
            .iter()
            .map(|l| Level {
                level: num(l, "level", 1.0) as u32,
                exp: num(l, "exp", 0.0) as i64,
                max_members: num(l, "max_members", 10.0) as usize,
                max_lands: num(l, "max_lands", 4.0) as usize,
                storage_pages: num(l, "storage_pages", 1.0) as usize,
                beacon_slots: num(l, "beacon_slots", 1.0) as usize,
                job_xp_bonus: num(l, "job_xp_bonus", 0.0),
                rare_bonus: num(l, "rare_bonus", 0.0),
            })
            .collect(),
        None => d.levels.clone(),
    };
    let upgrades = match j.get("upgrades").and_then(Json::as_array) {
        Some(list) => list
            .iter()
            .filter_map(|u| {
                Some(UpgradeRule {
                    kind: u.get("kind")?.as_str()?.to_owned(),
                    max_level: num(u, "max_level", 0.0) as u8,
                    per_level: num(u, "per_level", 0.0) as usize,
                    base_cost: num(u, "base_cost", 0.0) as i64,
                    required_level_offset: num(u, "required_level_offset", 0.0) as u32,
                })
            })
            .collect(),
        None => d.upgrades.clone(),
    };
    Some(Config {
        levels,
        upgrades,
        crop_levels: j
            .get("crop_levels")
            .and_then(Json::as_array)
            .map(|a| a.iter().filter_map(Json::as_f64).collect())
            .unwrap_or(d.crop_levels),
        storage_blocked_instances: j
            .get("storage_blocked_instances")
            .and_then(Json::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        create_cost: num(j, "create_cost", d.create_cost as f64) as i64,
        land_price: num(j, "land_price", d.land_price as f64) as i64,
        land_refund: num(j, "land_refund", d.land_refund),
        event_cost_gp: num(j, "event_cost_gp", d.event_cost_gp as f64) as i64,
        event_minutes: num(j, "event_minutes", d.event_minutes as f64) as i64,
        max_officers: num(j, "max_officers", d.max_officers as f64) as usize,
    })
}

static FILE: OnceLock<JsonFile<Config>> = OnceLock::new();

fn file() -> &'static JsonFile<Config> {
    FILE.get_or_init(|| JsonFile::in_memory(encode, decode))
}

/// 서버를 켤 때 한 번: 파일에서 읽는 설정으로.
pub fn open_global() {
    let _ = FILE.set(JsonFile::open(
        data_path("MC_GUILD_CONFIG", "guild_config.json"),
        encode,
        decode,
    ));
    // 처음이면 기본값을 파일로 써 둠 (웹·손으로 고치기 쉽게)
    if let Some(p) = file().path()
        && !p.exists()
    {
        let text = encode(&Config::default()).pretty();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, text);
    }
}

/// 지금 설정 (복사본).
pub fn get() -> Config {
    file().read(|c| c.clone())
}

pub fn with<R>(f: impl FnOnce(&Config) -> R) -> R {
    file().read(f)
}

/// 새 설정으로 바꿈 (확인을 통과해야).
pub fn set(c: Config) -> Result<(), String> {
    c.validate()?;
    file().write(|old| *old = c);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_and_levels() {
        let c = Config::default();
        c.validate().unwrap();
        assert_eq!(decode(&encode(&c)).unwrap(), c);
        assert_eq!(c.level_row(0).level, 1);
        assert_eq!(c.level_row(999).level, 1);
        assert_eq!(c.level_row(1000).level, 2);
        assert_eq!(c.level_row(3000).level, 3);
        assert_eq!(c.next_level(1500).unwrap().exp, 3000);
        assert_eq!(c.crop_multiplier(0), 1.0);
        assert_eq!(c.crop_multiplier(99), 2.5);
        let mut bad = c.clone();
        bad.levels.clear();
        assert!(bad.validate().is_err());
        // 빠진 칸은 기본값
        let partial = crate::json::parse(r#"{"land_price": 5}"#).unwrap();
        let p = decode(&partial).unwrap();
        assert_eq!(p.land_price, 5);
        assert_eq!(p.levels, c.levels);
    }
}
