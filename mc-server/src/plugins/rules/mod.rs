//! 서버 규칙 (`MC_RULES`, 기본 `world/rules.json`): 금지 아이템.
//!
//! 기획 "아이템 정리 — 금지 아이템: 제작 + 부셨을 때 아이템 드롭 불가". 목록의 아이템은 조합 결과로
//! 나오지 않고, 블록을 부수거나 터뜨려도·몹이 죽어도 떨어지지 않습니다. 이름 끝 `*`로 여러 개
//! (`*_head`). 웹 관리 화면의 "서버" 탭에서 고치면 곧바로.

use std::collections::HashSet;
use std::sync::OnceLock;

use crate::json::Json;
use crate::plugins::store::{JsonFile, data_path};

#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    /// 바닐라 아이템 이름 (`redstone`, `*_head`)
    pub banned_items: Vec<String>,
    /// 위 이름을 푼 아이템 번호
    ids: HashSet<i32>,
}

impl Default for Rules {
    fn default() -> Self {
        Rules::new(DEFAULT_BANNED.iter().map(|s| (*s).to_owned()).collect())
    }
}

/// 기획 목록 (호퍼는 "일부 허용"이라 넣지 않음).
pub const DEFAULT_BANNED: &[&str] = &[
    "redstone",
    "redstone_torch",
    "piston",
    "sticky_piston",
    "tnt",
    "enchanting_table",
    "ender_eye",
    "ender_pearl",
    "elytra",
    "firework_rocket",
    "flint_and_steel",
    "lava_bucket",
    "dispenser",
    "dropper",
    "observer",
    "daylight_detector",
    "jukebox",
    "note_block",
    "tripwire_hook",
    "redstone_lamp",
    "anvil",
    "chipped_anvil",
    "damaged_anvil",
    "item_frame",
    "glow_item_frame",
    "armor_stand",
    "brewing_stand",
    "comparator",
    "repeater",
    "sculk*",
    "calibrated_sculk_sensor",
    "slime_block",
    "*_head",
    "*_skull",
    "fire_charge",
    "painting",
    "map",
    "filled_map",
    "*minecart",
    "ender_chest",
    "beacon",
    "fishing_rod",
];

/// `*` 하나까지 맞추기.
fn matches(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        Some((a, b)) => name.len() >= a.len() + b.len() && name.starts_with(a) && name.ends_with(b),
        None => pattern == name,
    }
}

impl Rules {
    pub fn new(banned_items: Vec<String>) -> Self {
        let ids = (0..crate::data::items::count() as i32)
            .filter(|&id| {
                let n = crate::data::items::name(id);
                banned_items
                    .iter()
                    .any(|p| matches(p.trim_start_matches("minecraft:"), n))
            })
            .collect();
        Rules { banned_items, ids }
    }

    pub fn is_banned(&self, item: i32) -> bool {
        self.ids.contains(&item)
    }

    /// 금지된 아이템 수.
    pub fn banned_count(&self) -> usize {
        self.ids.len()
    }

    /// `*`는 하나만, 이름마다 맞는 아이템이 하나는 있어야 함.
    pub fn validate(&self) -> Result<(), String> {
        for p in &self.banned_items {
            if p.matches('*').count() > 1 {
                return Err(format!("{p}: *는 하나만"));
            }
            let p = p.trim_start_matches("minecraft:");
            if !(0..crate::data::items::count() as i32)
                .any(|id| matches(p, crate::data::items::name(id)))
            {
                return Err(format!("모르는 아이템: {p}"));
            }
        }
        Ok(())
    }
}

pub fn encode(r: &Rules) -> Json {
    Json::Object(vec![
        (
            "banned_items".into(),
            Json::Array(
                r.banned_items
                    .iter()
                    .map(|s| Json::String(s.clone()))
                    .collect(),
            ),
        ),
        // 읽을 때는 쓰지 않음 (웹 화면에 보여 주려고)
        ("banned_count".into(), Json::Number(r.banned_count() as f64)),
    ])
}

pub fn decode(j: &Json) -> Option<Rules> {
    let list = match j.get("banned_items").and_then(Json::as_array) {
        Some(a) => a
            .iter()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect(),
        None => Rules::default().banned_items,
    };
    Some(Rules::new(list))
}

static FILE: OnceLock<JsonFile<Rules>> = OnceLock::new();

fn file() -> &'static JsonFile<Rules> {
    FILE.get_or_init(|| JsonFile::in_memory(encode, decode))
}

pub fn open_global() {
    let _ = FILE.set(JsonFile::open(
        data_path("MC_RULES", "rules.json"),
        encode,
        decode,
    ));
    if let Some(p) = file().path()
        && !p.exists()
    {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, encode(&Rules::default()).pretty());
    }
    install();
}

/// 금지 확인을 아이템 표에 걸어 둠 (여러 번 불러도 됨).
pub fn install() {
    crate::data::items::set_ban_check(|id| file().read(|r| r.is_banned(id)));
}

pub fn get() -> Rules {
    file().read(|r| r.clone())
}

pub fn set(r: Rules) -> Result<(), String> {
    r.validate()?;
    file().write(|old| *old = r);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::items::by_name;

    #[test]
    fn banned_items_resolve_with_wildcards() {
        let r = Rules::default();
        for n in [
            "redstone",
            "tnt",
            "zombie_head",
            "player_head",
            "skeleton_skull",
            "tnt_minecart",
            "minecart",
            "sculk_sensor",
            "ender_pearl",
        ] {
            assert!(r.is_banned(by_name(n).unwrap()), "{n}");
        }
        for n in ["hopper", "stone", "diamond_sword", "chest"] {
            assert!(!r.is_banned(by_name(n).unwrap()), "{n}");
        }
        assert_eq!(decode(&encode(&r)).unwrap(), r);
    }
}
