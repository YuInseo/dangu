//! 길드 퀘스트: 일일 3개 · 주간 2개. 길드원 모두가 함께 채우고, 다 채우면 길드가 GP(길드 경험치)를
//! 받고, 마지막 한 칸을 채운 사람의 공헌도에도 같은 만큼 더해집니다.
//!
//! 날은 한국 시각 자정에, 주는 월요일 자정에 바뀝니다. 그날의 퀘스트는 날짜로 정해지므로 모든
//! 길드가 같은 퀘스트를 받습니다.

use crate::json::Json;

/// 퀘스트가 세는 일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 아무 블록이나 캐기.
    Mine,
    /// 광석 캐기.
    Ores,
    /// 다 자란 작물 거두기.
    Harvest,
    /// 통나무 베기.
    Logs,
    /// 몹 잡기.
    Kills,
}

impl Kind {
    pub const ALL: [Kind; 5] = [
        Kind::Mine,
        Kind::Ores,
        Kind::Harvest,
        Kind::Logs,
        Kind::Kills,
    ];

    fn key(self) -> &'static str {
        match self {
            Kind::Mine => "mine",
            Kind::Ores => "ores",
            Kind::Harvest => "harvest",
            Kind::Logs => "logs",
            Kind::Kills => "kills",
        }
    }

    fn from_key(k: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|x| x.key() == k)
    }

    /// (일일 목표, 일일 GP, 이름, 창 아이콘)
    fn daily(self) -> (u32, i64, &'static str, &'static str) {
        match self {
            Kind::Mine => (500, 120, "블록 캐기", "stone_pickaxe"),
            Kind::Ores => (64, 150, "광석 캐기", "iron_ore"),
            Kind::Harvest => (128, 120, "작물 거두기", "wheat"),
            Kind::Logs => (128, 100, "나무 베기", "oak_log"),
            Kind::Kills => (50, 150, "몹 잡기", "iron_sword"),
        }
    }

    pub fn name(self) -> &'static str {
        self.daily().2
    }

    pub fn icon(self) -> &'static str {
        self.daily().3
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Quest {
    pub kind: Kind,
    pub target: u32,
    pub reward: i64,
    pub progress: u32,
    pub done: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Board {
    /// 지금 퀘스트가 속한 날·주 (KST 날 번호, 주 번호).
    pub day: i64,
    pub week: i64,
    pub daily: Vec<Quest>,
    pub weekly: Vec<Quest>,
}

const KST: i64 = 9 * 3600;

pub fn day_of(now: i64) -> i64 {
    (now + KST).div_euclid(86_400)
}

/// 1970-01-01은 목요일: +3 하면 월요일에 바뀝니다.
pub fn week_of(now: i64) -> i64 {
    (day_of(now) + 3).div_euclid(7)
}

/// 날 번호로 고르는 퀘스트 `n`개 (겹치지 않게).
fn pick(seed: i64, n: usize) -> Vec<Kind> {
    let mut pool = Kind::ALL.to_vec();
    let mut x = (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut out = Vec::new();
    while out.len() < n && !pool.is_empty() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push(pool.remove((x % pool.len() as u64) as usize));
    }
    out
}

impl Board {
    /// 날·주가 바뀌었으면 새 퀘스트로.
    pub fn refresh(&mut self, now: i64) {
        let (day, week) = (day_of(now), week_of(now));
        if self.daily.is_empty() || self.day != day {
            self.day = day;
            self.daily = pick(day, 3)
                .into_iter()
                .map(|k| {
                    let (target, reward, ..) = k.daily();
                    Quest {
                        kind: k,
                        target,
                        reward,
                        progress: 0,
                        done: false,
                    }
                })
                .collect();
        }
        if self.weekly.is_empty() || self.week != week {
            self.week = week;
            self.weekly = pick(week * 31 + 7, 2)
                .into_iter()
                .map(|k| {
                    let (target, reward, ..) = k.daily();
                    Quest {
                        kind: k,
                        target: target * 6,
                        reward: reward * 8,
                        progress: 0,
                        done: false,
                    }
                })
                .collect();
        }
    }

    /// 한 일을 셉니다. 이번에 끝난 퀘스트들 (이름, 보상 GP).
    pub fn record(&mut self, kind: Kind, amount: u32, now: i64) -> Vec<(String, i64)> {
        self.refresh(now);
        let mut finished = Vec::new();
        for (label, list) in [("일일", &mut self.daily), ("주간", &mut self.weekly)] {
            for q in list.iter_mut().filter(|q| q.kind == kind && !q.done) {
                q.progress = (q.progress + amount).min(q.target);
                if q.progress >= q.target {
                    q.done = true;
                    finished.push((format!("{label} 퀘스트: {}", q.kind.name()), q.reward));
                }
            }
        }
        finished
    }

    pub fn to_json(&self) -> Json {
        let list = |l: &[Quest]| {
            Json::Array(
                l.iter()
                    .map(|q| {
                        Json::Object(vec![
                            ("kind".into(), Json::String(q.kind.key().into())),
                            ("target".into(), Json::Number(q.target as f64)),
                            ("reward".into(), Json::Number(q.reward as f64)),
                            ("progress".into(), Json::Number(q.progress as f64)),
                            ("done".into(), Json::Bool(q.done)),
                        ])
                    })
                    .collect(),
            )
        };
        Json::Object(vec![
            ("day".into(), Json::Number(self.day as f64)),
            ("week".into(), Json::Number(self.week as f64)),
            ("daily".into(), list(&self.daily)),
            ("weekly".into(), list(&self.weekly)),
        ])
    }

    pub fn from_json(j: &Json) -> Board {
        let list = |k: &str| {
            j.get(k)
                .and_then(Json::as_array)
                .unwrap_or_default()
                .iter()
                .filter_map(|q| {
                    Some(Quest {
                        kind: Kind::from_key(q.get("kind")?.as_str()?)?,
                        target: q.get("target")?.as_f64()? as u32,
                        reward: q.get("reward")?.as_f64()? as i64,
                        progress: q.get("progress")?.as_f64()? as u32,
                        done: q.get("done")?.as_bool()?,
                    })
                })
                .collect()
        };
        Board {
            day: j.get("day").and_then(Json::as_f64).unwrap_or(0.0) as i64,
            week: j.get("week").and_then(Json::as_f64).unwrap_or(0.0) as i64,
            daily: list("daily"),
            weekly: list("weekly"),
        }
    }
}

/// 부순 블록 이름으로 센 일들.
pub fn kinds_for_block(name: &str, mature_crop: bool) -> Vec<Kind> {
    let mut out = vec![Kind::Mine];
    if name.ends_with("_ore") || name == "ancient_debris" {
        out.push(Kind::Ores);
    }
    if name.ends_with("_log") || name.ends_with("_stem") {
        out.push(Kind::Logs);
    }
    if mature_crop {
        out.push(Kind::Harvest);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_weeks_and_rotation() {
        // 2026-10-19 (월) 00:00 KST = 2026-10-18 15:00 UTC
        let monday = 1_792_335_600;
        assert_eq!(week_of(monday), week_of(monday + 6 * 86_400 + 86_399));
        assert_ne!(week_of(monday - 1), week_of(monday));
        assert_ne!(day_of(monday - 1), day_of(monday));
        let mut b = Board::default();
        b.refresh(monday);
        assert_eq!((b.daily.len(), b.weekly.len()), (3, 2));
        let kinds: Vec<_> = b.daily.iter().map(|q| q.kind).collect();
        assert_eq!(pick(day_of(monday), 3), kinds, "같은 날은 같은 퀘스트");
        let k = b.daily[0].kind;
        let target = b.daily[0].target;
        assert!(b.record(k, target - 1, monday).is_empty());
        let done = b.record(k, 5, monday);
        assert!(done.iter().any(|(n, _)| n.starts_with("일일")));
        assert!(
            b.record(k, 5, monday)
                .iter()
                .all(|(n, _)| !n.starts_with("일일"))
        );
        // 다음 날이면 새로
        b.refresh(monday + 86_400);
        assert!(b.daily.iter().all(|q| q.progress == 0));
        assert_eq!(Board::from_json(&b.to_json()), b);
    }

    #[test]
    fn block_kinds() {
        assert_eq!(
            kinds_for_block("iron_ore", false),
            vec![Kind::Mine, Kind::Ores]
        );
        assert_eq!(
            kinds_for_block("oak_log", false),
            vec![Kind::Mine, Kind::Logs]
        );
        assert_eq!(
            kinds_for_block("wheat", true),
            vec![Kind::Mine, Kind::Harvest]
        );
    }
}
