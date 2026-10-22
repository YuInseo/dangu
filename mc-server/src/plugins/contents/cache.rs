//! `contents/.cache/ids.json`: 한 번 배정한 블록 상태·이모지 글자를 기억해 다시 읽어도 같은 번호가
//! 나오게 합니다 (번호가 바뀌면 이미 놓인 블록이 다른 블록으로 보임).

use std::collections::BTreeMap;
use std::path::Path;

use crate::data::blocks;
use crate::json::{self, Json};

/// 바뀌지 않게 기억해 둔 배정 (블록 상태, 이모지 글자).
#[derive(Default)]
pub(super) struct Cache {
    pub(super) blocks: BTreeMap<String, u32>,
    pub(super) chars: BTreeMap<String, u32>,
}

pub(super) fn read_cache(path: &Path) -> Cache {
    let mut c = Cache::default();
    let Some(j) = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| json::parse(&t).ok())
    else {
        return c;
    };
    for (key, map) in [("blocks", &mut c.blocks), ("chars", &mut c.chars)] {
        if let Some(Json::Object(fields)) = j.get(key) {
            for (k, v) in fields {
                if let Some(n) = v.as_f64() {
                    map.insert(k.clone(), n as u32);
                }
            }
        }
    }
    c
}

pub(super) fn write_cache(path: &Path, c: &Cache) {
    let obj = |m: &BTreeMap<String, u32>| {
        Json::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), Json::Number(*v as f64)))
                .collect(),
        )
    };
    let j = Json::Object(vec![
        ("blocks".into(), obj(&c.blocks)),
        ("chars".into(), obj(&c.chars)),
    ]);
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(path, j.pretty());
}

/// 소리 블록 상태 가운데 바닐라가 스스로 만들지 않는 것 (켜진 상태들) — 사용자 정의 블록용.
pub(super) fn free_note_states() -> Vec<u32> {
    let Some(b) = blocks::by_name("note_block") else {
        return Vec::new();
    };
    let first = blocks::default_state(b);
    let mut out = Vec::new();
    let mut s = first.saturating_sub(2000);
    // 소리 블록의 모든 상태를 찾습니다 (같은 블록인 동안)
    while s < first + 2000 {
        if blocks::block_of(s) == b && blocks::property(s, "powered") == Some("true") {
            out.push(s);
        }
        s += 1;
    }
    out
}
