//! SharedData: 인스턴스를 옮겨도 따라다니는 사람별 데이터. 마스터만 고칩니다.
//!
//! 3중 재화(돈·캐시·잠수 포인트), 누적 플레이 시간, 생활 직업 레벨·경험치. 마스터 메모리에
//! 올려 두고(캐시), 바뀐 것만 몇 초마다 `<기준>/shared/<uuid>.json`에 비동기로 씁니다.
//! 파일 대신 DB를 쓰려면 [`Backend`]만 바꾸면 됩니다.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::json::{self, Json};

/// 직업 레벨 L에서 L+1로 가는 데 드는 경험치: 3.0 × L² + 300.0.
pub fn xp_to_next(level: u32) -> f64 {
    3.0 * (level as f64).powf(2.0) + 300.0
}

/// 전직 초기화 때 돌려주는 비율.
pub const RESET_REFUND: f64 = 0.8;

#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub name: String,
    pub level: u32,
    /// 이번 레벨 안에서 쌓은 경험치.
    pub xp: f64,
    /// 이 직업에 지금까지 넣은 경험치 전부 (초기화 환급 계산용).
    pub total_xp: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SharedData {
    pub money: i64,
    pub cash: i64,
    /// 잠수 포인트.
    pub afk: i64,
    pub playtime_secs: u64,
    pub jobs: Vec<Job>,
    /// 전직 초기화로 돌려받아 다음 직업에 넣을 경험치.
    pub refund_xp: f64,
}

impl SharedData {
    pub fn field_mut(&mut self, field: &str) -> Option<&mut i64> {
        match field {
            "money" | "돈" => Some(&mut self.money),
            "cash" | "캐시" => Some(&mut self.cash),
            "afk" | "잠수" | "잠수포인트" => Some(&mut self.afk),
            _ => None,
        }
    }

    /// 재화를 더하거나 뺍니다. 음수가 되면 바꾸지 않고 false.
    pub fn add(&mut self, field: &str, delta: i64) -> bool {
        match self.field_mut(field) {
            Some(v) if v.checked_add(delta).is_some_and(|n| n >= 0) => {
                *v += delta;
                true
            }
            _ => false,
        }
    }

    pub fn job(&self, name: &str) -> Option<&Job> {
        self.jobs.iter().find(|j| j.name == name)
    }

    /// 직업 경험치를 넣고 오른 레벨 수를 돌려줍니다. 처음 갖는 직업이면 환급 경험치도 함께.
    pub fn add_job_xp(&mut self, name: &str, xp: f64) -> u32 {
        if self.job(name).is_none() {
            let bonus = std::mem::take(&mut self.refund_xp);
            self.jobs.push(Job {
                name: name.to_owned(),
                level: 1,
                xp: 0.0,
                total_xp: 0.0,
            });
            if bonus > 0.0 {
                self.add_job_xp(name, bonus);
            }
        }
        let j = self.jobs.iter_mut().find(|j| j.name == name).unwrap();
        j.xp += xp.max(0.0);
        j.total_xp += xp.max(0.0);
        let mut ups = 0;
        while j.xp >= xp_to_next(j.level) {
            j.xp -= xp_to_next(j.level);
            j.level += 1;
            ups += 1;
        }
        ups
    }

    /// 전직 초기화: 직업을 지우고 넣은 경험치의 80%를 다음 직업용으로 남깁니다.
    pub fn reset_job(&mut self, name: &str) -> Option<f64> {
        let i = self.jobs.iter().position(|j| j.name == name)?;
        let j = self.jobs.remove(i);
        let refund = j.total_xp * RESET_REFUND;
        self.refund_xp += refund;
        Some(refund)
    }

    pub fn to_json(&self) -> Json {
        let num = |v: f64| Json::Number(v);
        Json::Object(vec![
            ("money".into(), num(self.money as f64)),
            ("cash".into(), num(self.cash as f64)),
            ("afk".into(), num(self.afk as f64)),
            ("playtime_secs".into(), num(self.playtime_secs as f64)),
            ("refund_xp".into(), num(self.refund_xp)),
            (
                "jobs".into(),
                Json::Array(
                    self.jobs
                        .iter()
                        .map(|j| {
                            Json::Object(vec![
                                ("name".into(), Json::String(j.name.clone())),
                                ("level".into(), num(j.level as f64)),
                                ("xp".into(), num(j.xp)),
                                ("total_xp".into(), num(j.total_xp)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    pub fn from_json(j: &Json) -> Result<SharedData, String> {
        let f = |j: &Json, k: &str| j.get(k).and_then(Json::as_f64).unwrap_or(0.0);
        let jobs = j
            .get("jobs")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .map(|e| Job {
                name: e
                    .get("name")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_owned(),
                level: f(e, "level").max(1.0) as u32,
                xp: f(e, "xp"),
                total_xp: f(e, "total_xp"),
            })
            .collect();
        Ok(SharedData {
            money: f(j, "money") as i64,
            cash: f(j, "cash") as i64,
            afk: f(j, "afk") as i64,
            playtime_secs: f(j, "playtime_secs") as u64,
            refund_xp: f(j, "refund_xp"),
            jobs,
        })
    }
}

/// 어디에 오래 남기나.
pub enum Backend {
    /// 메모리에만 (시험용).
    Memory,
    /// 사람마다 JSON 파일 하나.
    Files(PathBuf),
}

struct Entry {
    data: SharedData,
    dirty: bool,
}

/// 마스터의 SharedData 캐시.
pub struct SharedStore {
    backend: Backend,
    cache: Mutex<HashMap<u128, Entry>>,
}

impl SharedStore {
    pub fn new(backend: Backend) -> Self {
        Self {
            backend,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn path(&self, uuid: u128) -> Option<PathBuf> {
        match &self.backend {
            Backend::Files(dir) => Some(dir.join(format!("{uuid:032x}.json"))),
            Backend::Memory => None,
        }
    }

    fn load(&self, uuid: u128) -> SharedData {
        self.path(uuid)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| json::parse(&t).ok())
            .and_then(|j| SharedData::from_json(&j).ok())
            .unwrap_or_default()
    }

    pub fn get(&self, uuid: u128) -> SharedData {
        if let Some(e) = self.cache.lock().unwrap().get(&uuid) {
            return e.data.clone();
        }
        let data = self.load(uuid);
        self.cache
            .lock()
            .unwrap()
            .entry(uuid)
            .or_insert(Entry { data, dirty: false })
            .data
            .clone()
    }

    /// 고칩니다. `f`가 false를 돌려주면 바뀌지 않은 것으로 봅니다. (성공, 결과 값).
    pub fn modify(
        &self,
        uuid: u128,
        f: impl FnOnce(&mut SharedData) -> bool,
    ) -> (bool, SharedData) {
        self.get(uuid);
        let mut cache = self.cache.lock().unwrap();
        let e = cache.get_mut(&uuid).unwrap();
        let mut copy = e.data.clone();
        let ok = f(&mut copy);
        if ok {
            e.data = copy;
            e.dirty = true;
        }
        (ok, e.data.clone())
    }

    /// 바뀐 것을 씁니다 (블로킹 — 마스터는 `spawn_blocking`에서 부름). 쓴 수.
    pub fn flush(&self) -> std::io::Result<usize> {
        let dirty: Vec<(u128, SharedData)> = {
            let mut cache = self.cache.lock().unwrap();
            cache
                .iter_mut()
                .filter(|(_, e)| e.dirty)
                .map(|(u, e)| {
                    e.dirty = false;
                    (*u, e.data.clone())
                })
                .collect()
        };
        for (uuid, data) in &dirty {
            if let Some(p) = self.path(*uuid) {
                if let Some(dir) = p.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let tmp = p.with_extension("json.tmp");
                std::fs::write(&tmp, data.to_json().pretty())?;
                std::fs::rename(tmp, p)?;
            }
        }
        Ok(dirty.len())
    }

    pub fn cached(&self) -> usize {
        self.cache.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_levels_follow_the_curve_and_reset_refunds_80_percent() {
        assert_eq!(xp_to_next(1), 303.0);
        assert_eq!(xp_to_next(10), 600.0);
        let mut d = SharedData::default();
        // 1→2: 303, 2→3: 312 → 615로 3레벨
        assert_eq!(d.add_job_xp("광부", 615.0), 2);
        assert_eq!(d.job("광부").unwrap().level, 3);
        assert_eq!(d.reset_job("광부"), Some(492.0));
        assert!(d.job("광부").is_none());
        // 다음 직업은 환급 경험치로 시작
        d.add_job_xp("농부", 0.0);
        assert_eq!(d.job("농부").unwrap().level, 2);
        assert_eq!(d.refund_xp, 0.0);
    }

    #[test]
    fn currency_never_goes_negative_and_files_round_trip() {
        let dir = std::env::temp_dir().join(format!("dambi-shared-{}", std::process::id()));
        let store = SharedStore::new(Backend::Files(dir.clone()));
        let u = 77;
        assert!(store.modify(u, |d| d.add("money", 100)).0);
        assert!(!store.modify(u, |d| d.add("money", -101)).0);
        assert!(store.modify(u, |d| d.add("캐시", 5)).0);
        assert_eq!(store.flush().unwrap(), 1);
        let again = SharedStore::new(Backend::Files(dir.clone()));
        let d = again.get(u);
        assert_eq!((d.money, d.cash), (100, 5));
        std::fs::remove_dir_all(dir).ok();
    }
}
