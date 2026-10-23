//! 웹 관리 화면 "플레이어" 탭: 등급·플라이·자동심기를 사람마다 보고 바꿉니다.
//!
//! - `GET /api/players` — 한 번이라도 접속한 사람 (이름·등급·끝나는 때·플라이 남은 초·자동심기·홈 수)
//! - `POST /api/players` `{ "target": 이름|uuid, "action": ..., "value": ..., "days": n }`
//!   - `fly_set` `fly_add` `fly_sub`: value = 시간 (`1h30m20s`, `1:30:20`, `3600`)
//!   - `rank`: value = 등급 id (비우면 없앰), days (0 = 영구)
//!   - `autoplant`: value = 블록 수 (그 값으로)

use crate::json::Json;
use crate::plugins::rank::{self, duration_text, parse_duration};

fn n(v: f64) -> Json {
    Json::Number(v)
}

pub fn list() -> Json {
    let t = rank::now();
    let cfg = rank::get_config();
    let players = rank::members().read(|d| {
        d.players
            .iter()
            .map(|(u, m)| {
                let active = rank::active_rank(m, &cfg, t);
                Json::Object(vec![
                    ("uuid".into(), Json::String(format!("{u:032x}"))),
                    ("name".into(), Json::String(m.name.clone())),
                    (
                        "rank".into(),
                        active
                            .as_ref()
                            .map_or(Json::Null, |r| Json::String(r.id.clone())),
                    ),
                    (
                        "rank_name".into(),
                        active
                            .as_ref()
                            .map_or(Json::Null, |r| Json::String(r.name.clone())),
                    ),
                    ("until".into(), n(m.until as f64)),
                    ("fly_left".into(), n((m.fly_until - t).max(0) as f64)),
                    ("autoplant".into(), n(m.autoplant as f64)),
                    ("homes".into(), n(m.homes.len() as f64)),
                ])
            })
            .collect()
    });
    Json::Object(vec![
        ("now".into(), n(t as f64)),
        ("players".into(), Json::Array(players)),
        (
            "ranks".into(),
            Json::Array(
                cfg.ranks
                    .iter()
                    .map(|r| {
                        Json::Object(vec![
                            ("id".into(), Json::String(r.id.clone())),
                            ("name".into(), Json::String(r.name.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn find(target: &str) -> Option<(u128, String)> {
    rank::uuid_by_name(target).or_else(|| {
        let u = u128::from_str_radix(target.trim(), 16).ok()?;
        let name = rank::members().read(|d| d.players.get(&u).map(|m| m.name.clone()))?;
        Some((
            u,
            if name.is_empty() {
                target.to_owned()
            } else {
                name
            },
        ))
    })
}

/// 바꾸고 결과 한 줄.
pub fn act(j: &Json) -> Result<String, String> {
    let s = |k: &str| {
        j.get(k)
            .and_then(Json::as_str)
            .unwrap_or("")
            .trim()
            .to_owned()
    };
    let target = s("target");
    let (uuid, name) = find(&target)
        .ok_or_else(|| format!("{target}: 한 번이라도 접속한 사람만 (이름 또는 uuid)"))?;
    let value = match j.get("value") {
        Some(Json::Number(v)) => format!("{v}"),
        _ => s("value"),
    };
    let secs = || parse_duration(&value).ok_or_else(|| format!("시간을 읽지 못했습니다: {value}"));
    match s("action").as_str() {
        "fly_set" => {
            let left = rank::set_fly_secs(uuid, secs()?);
            Ok(format!("{name}: 플라이 {} 남음", duration_text(left)))
        }
        "fly_add" => {
            let left = rank::add_fly_secs(uuid, secs()?);
            Ok(format!("{name}: 플라이 {} 남음", duration_text(left)))
        }
        "fly_sub" => {
            let left = rank::add_fly_secs(uuid, -secs()?);
            Ok(format!("{name}: 플라이 {} 남음", duration_text(left)))
        }
        "rank" => {
            let days = j.get("days").and_then(Json::as_f64).unwrap_or(0.0) as i64;
            if value.is_empty() {
                rank::grant(uuid, None, 0);
                return Ok(format!("{name}: 등급 없앰"));
            }
            let def = rank::config(|c| c.rank(&value).cloned())
                .ok_or_else(|| format!("모르는 등급: {value}"))?;
            rank::grant(uuid, Some(&def), days.max(0));
            Ok(format!(
                "{name}: {} 등급 ({})",
                def.name,
                if days > 0 {
                    format!("{days}일")
                } else {
                    "영구".into()
                }
            ))
        }
        "autoplant" => {
            let v: u64 = value
                .parse::<f64>()
                .ok()
                .filter(|v| *v >= 0.0)
                .ok_or("0 이상의 숫자")? as u64;
            rank::members().write(|d| d.member_mut(uuid).autoplant = v);
            Ok(format!("{name}: 자동심기 {v}블록"))
        }
        other => Err(format!("모르는 동작: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_fly_actions_are_exact_to_the_second() {
        let u = 0xABCDu128;
        rank::members().write(|d| d.member_mut(u).name = "WebFly".into());
        let a = |action: &str, value: &str| {
            act(&crate::json::parse(&format!(
                r#"{{"target":"webfly","action":"{action}","value":"{value}"}}"#
            ))
            .unwrap())
        };
        assert!(a("fly_set", "1h2m3s").unwrap().contains("1시간 2분 3초"));
        assert!(a("fly_add", "30s").unwrap().contains("1시간 2분 33초"));
        assert!(a("fly_sub", "2m").unwrap().contains("1시간 33초"));
        assert!(a("fly_sub", "9h").unwrap().contains("0초"));
        assert!(a("fly_add", "bad").is_err());
        assert!(a("rank", "vip").unwrap().contains("VIP"));
        assert!(
            act(
                &crate::json::parse(r#"{"target":"nobody","action":"fly_add","value":"1m"}"#)
                    .unwrap()
            )
            .is_err()
        );
        let l = list();
        assert!(format!("{l:?}").contains("WebFly"));
    }
}
