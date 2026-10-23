//! 장비 한 개 한 개의 상태 (`MC_GEAR`, 기본 `world/gear.json`): 강화 단계, 잠재능력, 남은 횟수,
//! 그리고 사람마다 낀 악세서리. 모든 인스턴스·담비 자식 프로세스가 같은 파일을 씁니다.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use super::config::{EquipConfig, GearDef, ScrollDef};
use super::{MAX_ENHANCE, Stat};
use crate::json::Json;
use crate::plugins::store::{JsonFile, data_path};

/// 장비 일련번호는 이 값부터 (복권 등 다른 일련번호와 섞이지 않게).
pub const SERIAL_BASE: u32 = 1 << 30;

#[derive(Debug, Clone, PartialEq)]
pub struct Gear {
    pub def: String,
    pub enhance: u8,
    /// 잠재능력 줄
    pub potential: Vec<(Stat, f64)>,
    /// 남은 잠재능력 횟수
    pub tries_left: u8,
    /// 마지막으로 바른 주문서 등급 (하급·중급·상급)
    pub scroll: Option<String>,
    /// 장인의 기운 (0..=1): 강화에 실패하면 쌓이고, 다 차면 다음 강화는 꼭 성공
    pub artisan: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Data {
    pub next: u32,
    pub gears: BTreeMap<u32, Gear>,
    /// 사람 → 반지·목걸이·귀걸이 일련번호
    pub worn: BTreeMap<u128, [Option<u32>; 3]>,
}

impl Data {
    /// 새 장비 하나 (일련번호).
    pub fn create(&mut self, def: &GearDef) -> u32 {
        let serial = SERIAL_BASE
            .max(self.next)
            .max(self.gears.keys().next_back().map_or(SERIAL_BASE, |k| k + 1));
        self.next = serial + 1;
        self.gears.insert(
            serial,
            Gear {
                def: def.id.clone(),
                enhance: 0,
                potential: Vec::new(),
                tries_left: def.potential_tries,
                scroll: None,
                artisan: 0.0,
            },
        );
        serial
    }

    pub fn worn(&self, uuid: u128) -> [Option<u32>; 3] {
        self.worn.get(&uuid).copied().unwrap_or_default()
    }
}

/// 장비 일련번호 범위 (그 위 `0x8000_0000..`은 창 아이콘).
pub fn is_gear_serial(serial: u32) -> bool {
    (SERIAL_BASE..0x8000_0000).contains(&serial)
}

/// 주문서 하나를 바른 결과 줄들. 같은 능력치는 한 번만, 값은 주문서 배율을 곱해 정수로.
pub fn roll_potential(
    cfg: &EquipConfig,
    scroll: &ScrollDef,
    mut rand: impl FnMut() -> u32,
) -> Vec<(Stat, f64)> {
    let lines =
        scroll.min_lines + (rand() % (scroll.max_lines - scroll.min_lines + 1) as u32) as u8;
    let mut pool: Vec<_> = cfg.potentials.iter().filter(|p| p.weight > 0).collect();
    let mut out = Vec::new();
    for _ in 0..lines {
        let total: u32 = pool.iter().map(|p| p.weight).sum();
        if total == 0 {
            break;
        }
        let mut pick = rand() % total;
        let i = pool
            .iter()
            .position(|p| {
                if pick < p.weight {
                    true
                } else {
                    pick -= p.weight;
                    false
                }
            })
            .unwrap();
        let p = pool.remove(i);
        let t = (rand() % 1001) as f64 / 1000.0;
        let v = ((p.min + (p.max - p.min) * t) * scroll.mult)
            .round()
            .max(1.0);
        out.push((p.stat, v));
    }
    out
}

/// 강화 한 번: `roll`(0..1)이 확률보다 작으면, 또는 장인의 기운이 다 찼으면 성공.
pub fn enhance_succeeds(cfg: &EquipConfig, level: u8, artisan: f64, roll: f64) -> bool {
    level < MAX_ENHANCE && (artisan >= 1.0 - 1e-9 || roll < cfg.enhance_chance(level))
}

/// 실패했을 때 쌓이는 장인의 기운 (0..=1).
pub fn artisan_gain(cfg: &EquipConfig, level: u8) -> f64 {
    cfg.enhance_chance(level) * cfg.enhance.artisan
}

/// 조율: 잠재능력 줄의 능력치는 두고 수치만 다시 (그 줄 옵션 범위 × 주문서 배율).
pub fn tune_potential(
    cfg: &EquipConfig,
    lines: &[(Stat, f64)],
    scroll: Option<&str>,
    mut rand: impl FnMut() -> u32,
) -> Vec<(Stat, f64)> {
    let mult = scroll
        .and_then(|name| cfg.scrolls.iter().find(|s| s.name == name))
        .map_or(1.0, |s| s.mult);
    lines
        .iter()
        .map(
            |&(stat, v)| match cfg.potentials.iter().find(|p| p.stat == stat) {
                Some(p) => {
                    let t = (rand() % 1001) as f64 / 1000.0;
                    (
                        stat,
                        ((p.min + (p.max - p.min) * t) * mult).round().max(1.0),
                    )
                }
                None => (stat, v),
            },
        )
        .collect()
}

/// 분해하면 돌려받는 것: 만들 때 재료와 강화에 쓴 강화석의 `salvage`만큼 (적어도 1개씩).
pub fn salvage(cfg: &EquipConfig, def: &GearDef, enhance: u8) -> Vec<(String, i32)> {
    let rate = cfg.forge.salvage;
    let mut out: Vec<(String, i32)> = Vec::new();
    let mut add = |k: &str, n: i32| {
        if n <= 0 {
            return;
        }
        match out.iter_mut().find(|(o, _)| o == k) {
            Some((_, m)) => *m += n,
            None => out.push((k.to_owned(), n)),
        }
    };
    if let Some(r) = cfg.recipes.iter().find(|r| r.result == def.id) {
        for (k, n) in &r.inputs {
            add(k, ((*n as f64 * rate).floor() as i32).max(1));
        }
    }
    let stones: i32 = (0..enhance).map(|l| cfg.enhance_cost(l).1).sum();
    add(&cfg.enhance.stone, (stones as f64 * rate).floor() as i32);
    out
}

// ---- JSON ----

fn encode(d: &Data) -> Json {
    let n = |v: f64| Json::Number(v);
    Json::Object(vec![
        ("next".into(), n(d.next as f64)),
        (
            "gears".into(),
            Json::Object(
                d.gears
                    .iter()
                    .map(|(serial, g)| {
                        let mut f = vec![
                            ("def".to_owned(), Json::String(g.def.clone())),
                            ("enhance".into(), n(g.enhance as f64)),
                            ("tries_left".into(), n(g.tries_left as f64)),
                        ];
                        if !g.potential.is_empty() {
                            f.push((
                                "potential".into(),
                                Json::Array(
                                    g.potential
                                        .iter()
                                        .map(|(k, v)| {
                                            Json::Array(vec![Json::String(k.key().into()), n(*v)])
                                        })
                                        .collect(),
                                ),
                            ));
                        }
                        if let Some(s) = &g.scroll {
                            f.push(("scroll".into(), Json::String(s.clone())));
                        }
                        if g.artisan > 0.0 {
                            f.push(("artisan".into(), n(g.artisan)));
                        }
                        (serial.to_string(), Json::Object(f))
                    })
                    .collect(),
            ),
        ),
        (
            "worn".into(),
            Json::Object(
                d.worn
                    .iter()
                    .filter(|(_, w)| w.iter().any(Option::is_some))
                    .map(|(u, w)| {
                        (
                            format!("{u:032x}"),
                            Json::Array(
                                w.iter()
                                    .map(|s| s.map_or(Json::Null, |s| n(s as f64)))
                                    .collect(),
                            ),
                        )
                    })
                    .collect(),
            ),
        ),
    ])
}

fn decode(j: &Json) -> Option<Data> {
    let mut d = Data {
        next: j.get("next").and_then(Json::as_f64).unwrap_or(0.0) as u32,
        ..Data::default()
    };
    if let Some(Json::Object(f)) = j.get("gears") {
        for (k, g) in f {
            let Ok(serial) = k.parse::<u32>() else {
                continue;
            };
            let potential = g
                .get("potential")
                .and_then(Json::as_array)
                .unwrap_or_default()
                .iter()
                .filter_map(|l| {
                    let a = l.as_array()?;
                    Some((Stat::parse(a.first()?.as_str()?)?, a.get(1)?.as_f64()?))
                })
                .collect();
            d.gears.insert(
                serial,
                Gear {
                    def: g.get("def").and_then(Json::as_str).unwrap_or("").to_owned(),
                    enhance: g.get("enhance").and_then(Json::as_f64).unwrap_or(0.0) as u8,
                    potential,
                    tries_left: g.get("tries_left").and_then(Json::as_f64).unwrap_or(0.0) as u8,
                    scroll: g.get("scroll").and_then(Json::as_str).map(str::to_owned),
                    artisan: g.get("artisan").and_then(Json::as_f64).unwrap_or(0.0),
                },
            );
        }
    }
    if let Some(Json::Object(f)) = j.get("worn") {
        for (k, w) in f {
            let (Ok(uuid), Some(a)) = (u128::from_str_radix(k, 16), w.as_array()) else {
                continue;
            };
            let mut slots = [None; 3];
            for (i, s) in a.iter().take(3).enumerate() {
                slots[i] = s.as_f64().map(|v| v as u32);
            }
            d.worn.insert(uuid, slots);
        }
    }
    Some(d)
}

pub type Gears = JsonFile<Data>;

static GEARS: OnceLock<Gears> = OnceLock::new();

pub fn gears() -> &'static Gears {
    GEARS.get_or_init(|| JsonFile::in_memory(encode, decode))
}

pub fn open_global() {
    let _ = GEARS.set(JsonFile::open(
        data_path("MC_GEAR", "gear.json"),
        encode,
        decode,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng(mut x: u64) -> impl FnMut() -> u32 {
        move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u32
        }
    }

    #[test]
    fn gears_round_trip_and_potentials_roll() {
        let cfg = EquipConfig::default();
        let mut d = Data::default();
        let ring = cfg.gear_def("hoya:mithril_ring").unwrap();
        let s = d.create(ring);
        assert_eq!(s, SERIAL_BASE);
        assert_eq!(d.create(ring), SERIAL_BASE + 1);
        d.gears.get_mut(&s).unwrap().potential = vec![(Stat::Str, 4.0), (Stat::HpPct, 3.0)];
        d.worn.insert(7, [Some(s), None, None]);
        assert_eq!(decode(&encode(&d)).unwrap(), d);
        assert_eq!(d.gears[&s].tries_left, 3);

        for (i, sc) in cfg.scrolls.iter().enumerate() {
            for seed in 1..200u64 {
                let lines = roll_potential(&cfg, sc, rng(seed * 7 + i as u64));
                assert!(
                    (sc.min_lines as usize..=sc.max_lines as usize).contains(&lines.len()),
                    "{}",
                    sc.name
                );
                let mut stats: Vec<Stat> = lines.iter().map(|l| l.0).collect();
                stats.dedup();
                assert_eq!(stats.len(), lines.len(), "같은 줄 두 번");
                for (k, v) in &lines {
                    let o = cfg.potentials.iter().find(|p| p.stat == *k).unwrap();
                    assert!(*v >= 1.0 && *v <= (o.max * sc.mult).round(), "{k:?} {v}");
                }
            }
        }
        assert!(enhance_succeeds(&cfg, 0, 0.0, 0.5));
        assert!(enhance_succeeds(&cfg, 29, 1.0, 0.99));
        assert!(!enhance_succeeds(&cfg, 29, 0.0, 0.5));
        assert!(!enhance_succeeds(&cfg, 30, 1.0, 0.0));
        // 장인의 기운: 확률 × 0.465
        assert!((artisan_gain(&cfg, 0) - 0.95 * 0.465).abs() < 1e-9);
    }

    #[test]
    fn tune_keeps_stats_and_salvage_returns_part() {
        let cfg = EquipConfig::default();
        let lines = vec![(Stat::Str, 3.0), (Stat::CritDamage, 2.0)];
        let mut r = rng(7);
        for _ in 0..50 {
            let out = tune_potential(&cfg, &lines, Some("상급"), &mut r);
            assert_eq!(
                out.iter().map(|l| l.0).collect::<Vec<_>>(),
                [Stat::Str, Stat::CritDamage]
            );
            let mult = cfg.scrolls.iter().find(|s| s.name == "상급").unwrap().mult;
            for (k, v) in &out {
                let o = cfg.potentials.iter().find(|p| p.stat == *k).unwrap();
                assert!(*v >= (o.min * mult).round().max(1.0) && *v <= (o.max * mult).round());
            }
        }
        // 분해: 만든 재료의 반 + 강화석 (+9까지 쓴 것 1+1+1+2+2+2+3+3+3 = 18 → 9)
        let def = cfg
            .gear
            .iter()
            .find(|g| cfg.recipes.iter().any(|r| r.result == g.id))
            .unwrap();
        let back = salvage(&cfg, def, 9);
        let stones = back
            .iter()
            .find(|(k, _)| *k == cfg.enhance.stone)
            .map(|x| x.1);
        let recipe = cfg.recipes.iter().find(|r| r.result == def.id).unwrap();
        let from_recipe = recipe
            .inputs
            .iter()
            .find(|(k, _)| *k == cfg.enhance.stone)
            .map_or(0, |(_, n)| ((*n as f64 * 0.5).floor() as i32).max(1));
        assert_eq!(stones, Some(9 + from_recipe));
        for (k, n) in &recipe.inputs {
            if *k != cfg.enhance.stone {
                let got = back.iter().find(|(o, _)| o == k).unwrap().1;
                assert_eq!(got, ((*n as f64 * 0.5).floor() as i32).max(1));
            }
        }
        assert!(
            salvage(&cfg, def, 0)
                .iter()
                .all(|(k, _)| *k != cfg.enhance.stone || from_recipe > 0)
        );
    }
}
