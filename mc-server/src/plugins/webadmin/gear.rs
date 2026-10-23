//! 웹 관리 화면 "장비" 탭의 장비 목록: 장비 한 개 한 개(일련번호)의 강화 단계·장인의 기운·잠재능력을
//! 보고 바꿉니다.
//!
//! - `GET /api/gear?q=<이름·일련번호>&limit=<n>` — 일련번호 큰 것(새 것)부터
//! - `POST /api/gear` `{ "serial": n, "enhance"?: 0..=30, "artisan"?: 0..=1, "tries_left"?: n,
//!   "clear_potential"?: true }`
//!
//! 바뀐 것은 그 장비를 가진 사람의 가방이 다시 그려질 때 이름·설명에 보입니다.

use crate::json::Json;
use crate::plugins::equip::{MAX_ENHANCE, config, gears};

fn n(v: f64) -> Json {
    Json::Number(v)
}

/// 낀 장신구의 주인 이름 (일련번호 → 이름).
fn wearers() -> std::collections::HashMap<u32, String> {
    let worn = gears().read(|d| d.worn.clone());
    let names = crate::plugins::rank::members().read(|d| {
        d.players
            .iter()
            .map(|(u, m)| (*u, m.name.clone()))
            .collect::<std::collections::HashMap<_, _>>()
    });
    let mut out = std::collections::HashMap::new();
    for (uuid, slots) in worn {
        let name = names
            .get(&uuid)
            .filter(|n| !n.is_empty())
            .cloned()
            .unwrap_or_else(|| format!("{uuid:032x}"));
        for s in slots.into_iter().flatten() {
            out.insert(s, name.clone());
        }
    }
    out
}

pub fn list(query: Option<&str>, limit: usize) -> Json {
    let cfg = config::get();
    let worn = wearers();
    let q = query.unwrap_or("").trim().to_lowercase();
    let all = gears().read(|d| d.gears.clone());
    let total = all.len();
    let rows: Vec<Json> = all
        .iter()
        .rev()
        .filter_map(|(serial, g)| {
            let def = cfg.gear_def(&g.def);
            let name = def.map_or(g.def.clone(), |d| d.name.clone());
            let wearer = worn.get(serial);
            if !q.is_empty()
                && !name.to_lowercase().contains(&q)
                && !g.def.to_lowercase().contains(&q)
                && serial.to_string() != q
                && !wearer.is_some_and(|w| w.to_lowercase().contains(&q))
            {
                return None;
            }
            Some(Json::Object(vec![
                ("serial".into(), n(*serial as f64)),
                ("def".into(), Json::String(g.def.clone())),
                ("name".into(), Json::String(name)),
                (
                    "part".into(),
                    Json::String(def.map_or("?", |d| d.part.name()).into()),
                ),
                (
                    "grade".into(),
                    Json::String(def.map_or(String::new(), |d| d.grade.clone())),
                ),
                (
                    "accessory".into(),
                    Json::Bool(def.is_some_and(|d| d.part.is_accessory())),
                ),
                ("enhance".into(), n(g.enhance as f64)),
                ("artisan".into(), n(g.artisan)),
                ("tries_left".into(), n(g.tries_left as f64)),
                (
                    "max_tries".into(),
                    n(def.map_or(0.0, |d| d.potential_tries as f64)),
                ),
                (
                    "potential".into(),
                    Json::String(crate::plugins::equip::look::potential_text(&g.potential)),
                ),
                (
                    "scroll".into(),
                    g.scroll.clone().map_or(Json::Null, Json::String),
                ),
                (
                    "wearer".into(),
                    wearer.cloned().map_or(Json::Null, Json::String),
                ),
            ]))
        })
        .take(limit)
        .collect();
    Json::Object(vec![
        ("total".into(), n(total as f64)),
        ("gear".into(), Json::Array(rows)),
    ])
}

/// 한 장비를 고침. 성공하면 알림 글.
pub fn act(j: &Json) -> Result<String, String> {
    let serial = j
        .get("serial")
        .and_then(Json::as_f64)
        .ok_or("serial이 없습니다")? as u32;
    let enhance = j.get("enhance").and_then(Json::as_f64);
    let artisan = j.get("artisan").and_then(Json::as_f64);
    let tries = j.get("tries_left").and_then(Json::as_f64);
    let clear = j
        .get("clear_potential")
        .and_then(Json::as_bool)
        .unwrap_or(false);
    if let Some(e) = enhance
        && !(0.0..=MAX_ENHANCE as f64).contains(&e)
    {
        return Err(format!("강화 단계는 0~{MAX_ENHANCE}입니다"));
    }
    if let Some(a) = artisan
        && !(0.0..=1.0).contains(&a)
    {
        return Err("장인의 기운은 0~1(0~100%)입니다".into());
    }
    if let Some(t) = tries
        && !(0.0..=255.0).contains(&t)
    {
        return Err("남은 횟수가 이상합니다".into());
    }
    gears().write(|d| {
        let g = d
            .gears
            .get_mut(&serial)
            .ok_or_else(|| format!("{serial}번 장비가 없습니다"))?;
        if let Some(e) = enhance {
            g.enhance = e.round() as u8;
        }
        if let Some(a) = artisan {
            g.artisan = a;
        }
        if let Some(t) = tries {
            g.tries_left = t.round() as u8;
        }
        if clear {
            g.potential.clear();
            g.scroll = None;
        }
        Ok(format!(
            "{serial}번: +{} · 장인의 기운 {:.1}% · 남은 횟수 {}",
            g.enhance,
            g.artisan * 100.0,
            g.tries_left
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_and_edit_gear() {
        let def = config::get().gear[0].clone();
        let serial = gears().write(|d| d.create(&def));
        let j = list(Some(&serial.to_string()), 10);
        let rows = j.get("gear").unwrap().as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("name").unwrap().as_str(),
            Some(def.name.as_str())
        );

        let set = crate::json::parse(&format!(
            r#"{{"serial":{serial},"enhance":12,"artisan":0.5}}"#
        ))
        .unwrap();
        assert!(act(&set).unwrap().contains("+12"));
        gears().read(|d| {
            assert_eq!(d.gears[&serial].enhance, 12);
            assert_eq!(d.gears[&serial].artisan, 0.5);
        });
        let bad = crate::json::parse(&format!(r#"{{"serial":{serial},"enhance":31}}"#)).unwrap();
        assert!(act(&bad).is_err());
        let missing = crate::json::parse(r#"{"serial":1}"#).unwrap();
        assert!(act(&missing).is_err());
    }
}
