//! ItemsAdder 방식의 "콘텐츠": 설정 파일과 그림만 넣으면 우리만의 아이템·블록·이모지가 생기고,
//! 리소스 팩이 자동으로 만들어져 내려갑니다.
//!
//! ```text
//! contents/                          (MC_CONTENTS, 기본 ./contents — 모든 인스턴스가 같이 씀)
//!   <팩 이름>/
//!     configs/*.yml                  설정 (하위 폴더도)
//!     resourcepack/assets/<ns>/...   그림·모델 (그대로 팩에 들어감)
//!   .cache/ids.json                  블록 상태·이모지 글자 배정 (바뀌지 않게 기억)
//! ```
//!
//! 설정은 ItemsAdder와 같은 모양을 읽습니다.
//!
//! ```yaml
//! info:
//!   namespace: myitems
//! items:
//!   ruby:
//!     display_name: "&c루비"
//!     lore: ["&7빛나는 보석"]
//!     resource:
//!       material: EMERALD        # 바탕 바닐라 아이템
//!       generate: true           # 모델을 만들어 줌
//!       textures: [item/ruby.png]   # assets/myitems/textures/ 기준
//!       # 또는 generate: false + model_path: item/ruby (직접 만든 모델)
//!     # 새 형식: graphics: {texture: item/ruby} 또는 {model: item/ruby, parent: item/handheld}
//!     enchant_glint: true
//!     max_stack_size: 16
//!     attribute_modifiers: {mainhand: {attackDamage: 7}}
//!   ruby_block:
//!     display_name: "루비 블록"
//!     resource: {material: PAPER, generate: true, textures: [block/ruby_block.png]}
//!     specific_properties:
//!       block:
//!         placed_model: {type: REAL_NOTE}   # 소리 블록 상태를 빌려 진짜 블록으로 놓임
//!         drop_when_mined: true
//! font_images:                     # 채팅 등에서 :heart: → 그림 글자
//!   heart: {path: font/heart.png, scale_ratio: 9, y_position: 8}
//! categories:                      # /ia 목록
//!   gems: {name: "&c보석", icon: myitems:ruby, items: [myitems:ruby, myitems:ruby_block]}
//! ```

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

mod builtin;
pub mod source;

pub use builtin::{LOTTERY_MODEL_ID, LOTTERY_TICKET_BACK};
use std::sync::{Arc, RwLock};

use crate::data::{blocks, items};
use crate::game::custom_items::{self, CustomItem};
use crate::json::{self, Json};
use crate::text::legacy_colors;

/// 이모지 글자를 배정하기 시작하는 곳 (유니코드 사용자 영역).
const FIRST_CHAR: u32 = 0xE000;

#[derive(Debug, Clone, PartialEq)]
pub struct FontImage {
    /// `ns:name`
    pub id: String,
    pub ch: char,
    /// 팩 안의 그림 (`ns:font/heart.png`).
    pub file: String,
    pub height: i32,
    pub ascent: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Category {
    pub id: String,
    pub name: String,
    /// 아이콘 아이템 (`ns:id` 또는 바닐라 이름).
    pub icon: String,
    pub items: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Loaded {
    /// 올린 아이템 (`ns:id`), 읽은 순서.
    pub items: Vec<String>,
    pub fonts: Vec<FontImage>,
    pub categories: Vec<Category>,
    /// 리소스 팩에 넣을 파일 (팩 안 경로, 내용).
    pub files: Vec<(String, Vec<u8>)>,
    pub errors: Vec<String>,
    /// 사용자 정의 아이템 번호 → 놓으면 생기는 블록 상태, 그 반대.
    block_of_item: HashMap<u16, u32>,
    item_of_block: HashMap<u32, (u16, bool)>,
}

static CURRENT: RwLock<Option<Arc<Loaded>>> = RwLock::new(None);

pub fn dir() -> PathBuf {
    PathBuf::from(std::env::var("MC_CONTENTS").unwrap_or_else(|_| "contents".into()))
}

/// 지금 콘텐츠 (처음이면 읽음).
pub fn current() -> Arc<Loaded> {
    if let Some(l) = CURRENT.read().unwrap().as_ref() {
        return Arc::clone(l);
    }
    reload()
}

/// 다시 읽고 아이템을 올립니다.
pub fn reload() -> Arc<Loaded> {
    reload_from(&dir())
}

/// `base` 폴더에서 다시 읽습니다 (시험용).
pub fn reload_from(base: &Path) -> Arc<Loaded> {
    let loaded = Arc::new(load(base));
    *CURRENT.write().unwrap() = Some(Arc::clone(&loaded));
    loaded
}

/// `/iareload`: 다시 읽고, 팩을 새로 만들고, 접속한 사람들에게 보냅니다. 담비 자식이면
/// 마스터에게 부탁해 모든 인스턴스가 함께 (마스터가 팩을 내려 주므로).
pub fn apply(server: &crate::Server) -> Vec<String> {
    if crate::dambi::child::active() {
        crate::dambi::child::publish(PACK_RELOAD, "");
        return vec!["모든 인스턴스가 콘텐츠를 다시 읽고 팩을 새로 만듭니다.".into()];
    }
    let l = reload_and_rebuild();
    server.publish_all(crate::server::Event::ResourcePackChanged);
    summary(&l)
}

/// 전역 버스에서 팩을 다시 만들라는 채널.
pub const PACK_RELOAD: &str = "dangu:pack_reload";

/// 콘텐츠를 다시 읽고 팩을 새로 만듭니다.
pub fn reload_and_rebuild() -> Arc<Loaded> {
    let l = reload();
    crate::resource_pack::rebuild();
    l
}

/// 읽은 결과 요약 (명령어·콘솔에 보일 줄들).
pub fn summary(l: &Loaded) -> Vec<String> {
    let mut v = vec![format!(
        "콘텐츠: 아이템 {}개, 블록 {}개, 이모지 {}개, 목록 {}개, 팩 파일 {}개",
        l.items.len(),
        l.block_of_item.len(),
        l.fonts.len(),
        l.categories.len(),
        l.files.len()
    )];
    v.extend(l.errors.iter().map(|e| format!("오류: {e}")));
    v
}

/// 사용자 정의 아이템이 블록이면 놓을 상태.
pub fn block_state(custom: u16) -> Option<u32> {
    current().block_of_item.get(&custom).copied()
}

/// 이 상태가 사용자 정의 블록이면 (아이템 번호, 부수면 떨어뜨림).
pub fn block_item(state: u32) -> Option<(u16, bool)> {
    let c = CURRENT.read().unwrap();
    c.as_ref()?.item_of_block.get(&state).copied()
}

/// `:heart:`(또는 `:ns:heart:`)를 그림 글자로.
pub fn emojify(text: &str) -> String {
    if !text.contains(':') {
        return text.to_owned();
    }
    let fonts = &current().fonts;
    if fonts.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        // 가장 긴 `:이름:`부터 (ns:name 포함)
        let found = fonts.iter().find_map(|f| {
            let short = f.id.split_once(':').map_or(f.id.as_str(), |(_, n)| n);
            [f.id.as_str(), short]
                .into_iter()
                .find(|name| after.starts_with(name) && after[name.len()..].starts_with(':'))
                .map(|name| (f.ch, name.len()))
        });
        match found {
            Some((ch, len)) => {
                out.push(ch);
                rest = &after[len + 1..];
            }
            None => {
                out.push(':');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 사용자 정의 아이템 하나 (`ns:id`)를 줄 때의 [`crate::packets::world::Item`].
pub fn item(id: &str, count: i32) -> Option<crate::packets::world::Item> {
    let index = custom_items::index_of(id)?;
    let def = custom_items::get(index)?;
    Some(crate::packets::world::Item {
        id: def.base,
        count,
        custom: Some(index),
        serial: 0,
    })
}

// ---- 읽기 ----

/// 바뀌지 않게 기억해 둔 배정 (블록 상태, 이모지 글자).
#[derive(Default)]
struct Cache {
    blocks: BTreeMap<String, u32>,
    chars: BTreeMap<String, u32>,
}

fn read_cache(path: &Path) -> Cache {
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

fn write_cache(path: &Path, c: &Cache) {
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

fn model_ref_json(model: &str) -> Json {
    Json::Object(vec![
        ("type".into(), Json::String("minecraft:model".into())),
        ("model".into(), Json::String(model.to_owned())),
    ])
}

/// `item/ruby.png` → `ns:item/ruby`.
fn resource_ref(ns: &str, path: &str) -> String {
    let p = path.trim_end_matches(".png").trim_end_matches(".json");
    if p.contains(':') {
        p.to_owned()
    } else {
        format!("{ns}:{p}")
    }
}

fn s<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    j.get(k).and_then(Json::as_str)
}

fn strings(j: Option<&Json>) -> Vec<String> {
    match j {
        Some(Json::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        Some(Json::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// 소리 블록 상태 가운데 바닐라가 스스로 만들지 않는 것 (켜진 상태들) — 사용자 정의 블록용.
fn free_note_states() -> Vec<u32> {
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

pub fn load(base: &Path) -> Loaded {
    let mut l = Loaded::default();
    let cache_path = base.join(".cache").join("ids.json");
    let mut cache = read_cache(&cache_path);
    let mut defs: Vec<CustomItem> = Vec::new();
    // (아이템 ns:id, 모델 이름) — 블록 상태 모델
    let mut block_models: Vec<(String, String, bool)> = Vec::new();
    let mut font_defs: Vec<(String, String, i32, i32)> = Vec::new();
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    // (바탕 아이템, custom_model_data, 모델) — `model_id`로 바닐라 모양을 갈아 끼울 것들
    let mut cmd_models: Vec<(String, i32, String)> = Vec::new();

    // 서버 내장 팩 먼저, 디스크의 팩이 같은 파일을 덮어씀
    let mut packs = builtin::packs();
    packs.extend(source::from_disk(base));
    for pack in packs {
        let pack_name = pack.name.clone();
        for (path, bytes) in pack.files {
            files.insert(path, bytes);
        }
        for (where_, text) in pack.configs {
            let doc = match crate::yaml::parse(&text) {
                Ok(d) => d,
                Err(e) => {
                    l.errors.push(format!("{where_}: {e}"));
                    continue;
                }
            };
            let ns = doc
                .get("info")
                .and_then(|i| s(i, "namespace"))
                .unwrap_or(&pack_name)
                .to_owned();
            if let Some(Json::Object(entries)) = doc.get("items") {
                for (id, it) in entries {
                    match read_item(&ns, id, it, &mut files, &mut cmd_models) {
                        Ok((def, block)) => {
                            if let Some(b) = block {
                                block_models.push(b);
                            }
                            l.items.push(def.id.clone());
                            defs.push(def);
                        }
                        Err(e) => l.errors.push(format!("{where_}: {ns}:{id}: {e}")),
                    }
                }
            }
            if let Some(Json::Object(entries)) = doc.get("font_images") {
                for (id, f) in entries {
                    let Some(path) = s(f, "path") else {
                        l.errors
                            .push(format!("{where_}: font_images.{id}: path가 없습니다"));
                        continue;
                    };
                    let height = f.get("scale_ratio").and_then(Json::as_f64).unwrap_or(9.0) as i32;
                    let ascent = f.get("y_position").and_then(Json::as_f64).unwrap_or(8.0) as i32;
                    let file = format!("{}.png", resource_ref(&ns, path));
                    font_defs.push((
                        format!("{ns}:{id}"),
                        file,
                        height.clamp(1, 256),
                        ascent.min(height),
                    ));
                }
            }
            if let Some(Json::Object(entries)) = doc.get("categories") {
                for (id, c) in entries {
                    if c.get("enabled") == Some(&Json::Bool(false)) {
                        continue;
                    }
                    let items = strings(c.get("items"))
                        .into_iter()
                        .map(|i| {
                            if i.contains(':') {
                                i
                            } else {
                                format!("{ns}:{i}")
                            }
                        })
                        .collect::<Vec<_>>();
                    l.categories.push(Category {
                        id: format!("{ns}:{id}"),
                        name: legacy_colors(s(c, "name").unwrap_or(id)),
                        icon: s(c, "icon")
                            .map(str::to_owned)
                            .or_else(|| items.first().cloned())
                            .unwrap_or_else(|| "paper".into()),
                        items,
                    });
                }
            }
        }
    }

    // 이모지 글자 배정 (기억해 둔 것 우선)
    let mut used: Vec<u32> = cache.chars.values().copied().collect();
    for (id, file, height, ascent) in font_defs {
        let code = match cache.chars.get(&id) {
            Some(&c) => c,
            None => {
                let c = (FIRST_CHAR..).find(|c| !used.contains(c)).unwrap();
                used.push(c);
                cache.chars.insert(id.clone(), c);
                c
            }
        };
        if let Some(ch) = char::from_u32(code) {
            l.fonts.push(FontImage {
                id,
                ch,
                file,
                height,
                ascent,
            });
        }
    }
    // 이름·설명의 :이모지:를 글자로 (폰트를 먼저 정한 뒤)
    let emoji = |t: &str, fonts: &[FontImage]| {
        let mut out = t.to_owned();
        for f in fonts {
            let short = f.id.split_once(':').map_or(f.id.as_str(), |(_, n)| n);
            out = out.replace(&format!(":{}:", f.id), &f.ch.to_string());
            out = out.replace(&format!(":{short}:"), &f.ch.to_string());
        }
        out
    };
    for d in &mut defs {
        d.name = emoji(&d.name, &l.fonts);
        for line in &mut d.lore {
            *line = emoji(line, &l.fonts);
        }
    }

    // 블록 상태 배정
    let free = free_note_states();
    let mut taken: Vec<u32> = cache.blocks.values().copied().collect();
    custom_items::install(defs.clone());
    let mut variants: Vec<(u32, String)> = Vec::new();
    for (item_id, model, drop) in &block_models {
        let state = match cache.blocks.get(item_id) {
            Some(&s) => s,
            None => match free.iter().find(|s| !taken.contains(s)) {
                Some(&s) => {
                    taken.push(s);
                    cache.blocks.insert(item_id.clone(), s);
                    s
                }
                None => {
                    l.errors
                        .push(format!("{item_id}: 사용자 정의 블록 자리가 모자랍니다"));
                    continue;
                }
            },
        };
        if let Some(index) = custom_items::index_of(item_id) {
            l.block_of_item.insert(index, state);
            l.item_of_block.insert(state, (index, *drop));
        }
        variants.push((state, model.clone()));
    }
    if !variants.is_empty()
        && let Some(b) = blocks::by_name("note_block")
    {
        // 소리 블록의 모든 상태: 사용자 정의는 그 모델, 나머지는 바닐라 그대로
        let first = blocks::default_state(b);
        let mut entries = Vec::new();
        let mut st = first.saturating_sub(2000);
        while st < first + 2000 {
            if blocks::block_of(st) == b {
                let key = blocks::properties(st)
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let model = variants
                    .iter()
                    .find(|(s, _)| *s == st)
                    .map_or("minecraft:block/note_block".to_owned(), |(_, m)| m.clone());
                entries.push((
                    key,
                    Json::Object(vec![("model".into(), Json::String(model))]),
                ));
            }
            st += 1;
        }
        files.insert(
            "assets/minecraft/blockstates/note_block.json".into(),
            Json::Object(vec![("variants".into(), Json::Object(entries))])
                .compact()
                .into_bytes(),
        );
    }

    // model_id: 바닐라 아이템마다 custom_model_data로 고르는 모양 정의
    let mut by_material: BTreeMap<String, Vec<(i32, String)>> = BTreeMap::new();
    for (material, cmd, model) in cmd_models {
        by_material.entry(material).or_default().push((cmd, model));
    }
    for (material, mut entries) in by_material {
        let key = format!("assets/minecraft/items/{material}.json");
        if files.contains_key(&key) {
            l.errors.push(format!(
                "{key}: 팩에 이미 있어 model_id 모양을 만들지 않았습니다"
            ));
            continue;
        }
        entries.sort_by_key(|(c, _)| *c);
        entries.dedup_by_key(|(c, _)| *c);
        let vanilla = crate::data::vanilla_item_definition(&material);
        let fallback = vanilla
            .as_ref()
            .and_then(|v| v.get("model").cloned())
            .unwrap_or_else(|| model_ref_json(&format!("minecraft:item/{material}")));
        let dispatch = Json::Object(vec![
            (
                "type".into(),
                Json::String("minecraft:range_dispatch".into()),
            ),
            (
                "property".into(),
                Json::String("minecraft:custom_model_data".into()),
            ),
            ("index".into(), Json::Number(0.0)),
            (
                "entries".into(),
                Json::Array(
                    entries
                        .iter()
                        .map(|(c, m)| {
                            Json::Object(vec![
                                ("threshold".into(), Json::Number(*c as f64)),
                                ("model".into(), model_ref_json(m)),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("fallback".into(), fallback),
        ]);
        // 바닐라 정의의 다른 칸(hand_animation_on_swap 등)은 그대로
        let mut top = match vanilla {
            Some(Json::Object(fields)) => {
                fields.into_iter().filter(|(k, _)| k != "model").collect()
            }
            _ => Vec::new(),
        };
        top.insert(0, ("model".into(), dispatch));
        files.insert(key, Json::Object(top).compact().into_bytes());
    }

    // 이모지 폰트 (팩에 이미 default.json이 있으면 이어 붙임)
    if !l.fonts.is_empty() {
        let key = "assets/minecraft/font/default.json".to_owned();
        let mut providers: Vec<Json> = files
            .get(&key)
            .and_then(|b| json::parse(&String::from_utf8_lossy(b)).ok())
            .and_then(|j| {
                j.get("providers")
                    .and_then(Json::as_array)
                    .map(<[Json]>::to_vec)
            })
            .unwrap_or_default();
        for f in &l.fonts {
            providers.push(Json::Object(vec![
                ("type".into(), Json::String("bitmap".into())),
                ("file".into(), Json::String(f.file.clone())),
                ("ascent".into(), Json::Number(f.ascent as f64)),
                ("height".into(), Json::Number(f.height as f64)),
                (
                    "chars".into(),
                    Json::Array(vec![Json::String(f.ch.to_string())]),
                ),
            ]));
        }
        files.insert(
            key,
            Json::Object(vec![("providers".into(), Json::Array(providers))])
                .compact()
                .into_bytes(),
        );
    }
    // 목록이 없으면 이름공간마다 하나씩
    if l.categories.is_empty() && !l.items.is_empty() {
        let mut by_ns: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for id in &l.items {
            let ns = id.split(':').next().unwrap_or("").to_owned();
            by_ns.entry(ns).or_default().push(id.clone());
        }
        for (ns, items) in by_ns {
            l.categories.push(Category {
                id: ns.clone(),
                name: ns,
                icon: items[0].clone(),
                items,
            });
        }
    }
    write_cache(&cache_path, &cache);
    l.files = files.into_iter().collect();
    l
}

/// 아이템 하나. 블록이면 (아이템 id, 블록 모델, 부수면 떨어뜨림)도.
#[allow(clippy::type_complexity)]
fn read_item(
    ns: &str,
    id: &str,
    it: &Json,
    files: &mut BTreeMap<String, Vec<u8>>,
    cmd_models: &mut Vec<(String, i32, String)>,
) -> Result<(CustomItem, Option<(String, String, bool)>), String> {
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' || c == '.')
    {
        return Err("아이디는 소문자·숫자·_만 쓸 수 있습니다".into());
    }
    let full = format!("{ns}:{id}");
    let resource = it.get("resource");
    let graphics = it.get("graphics");
    let material = resource
        .and_then(|r| s(r, "material"))
        .or_else(|| s(it, "material"))
        .unwrap_or("PAPER")
        .to_ascii_lowercase();
    let material = material.strip_prefix("minecraft:").unwrap_or(&material);
    let base = items::by_name(material)
        .filter(|&b| b > 0)
        .ok_or_else(|| format!("모르는 material {material}"))?;
    let block = it.get("specific_properties").and_then(|p| p.get("block"));
    let is_block = block.is_some();
    // 모델: 직접 만든 것(model_path, graphics.model) 또는 그림으로 만들어 줌
    let textures: Vec<String> = {
        let mut t = strings(resource.and_then(|r| r.get("textures")));
        if t.is_empty() {
            t = strings(resource.and_then(|r| r.get("texture")));
        }
        if t.is_empty() {
            t = strings(graphics.and_then(|g| g.get("texture")));
        }
        t
    };
    let given_model = resource
        .and_then(|r| s(r, "model_path"))
        .or_else(|| graphics.and_then(|g| s(g, "model")))
        .map(|m| resource_ref(ns, m));
    let generate = resource
        .and_then(|r| r.get("generate"))
        .and_then(Json::as_bool)
        .unwrap_or(!textures.is_empty());
    let model = if let Some(m) = given_model {
        Some(m)
    } else if generate && !textures.is_empty() {
        let (folder, body) = if is_block {
            let tex = resource_ref(ns, &textures[0]);
            (
                "block",
                format!(r#"{{"parent":"minecraft:block/cube_all","textures":{{"all":"{tex}"}}}}"#),
            )
        } else {
            let parent = graphics
                .and_then(|g| s(g, "parent"))
                .or_else(|| resource.and_then(|r| s(r, "parent")))
                .map(|p| {
                    if p.contains(':') {
                        p.to_owned()
                    } else {
                        format!("minecraft:{p}")
                    }
                })
                .unwrap_or_else(|| {
                    let handheld = [
                        "_sword", "_axe", "_pickaxe", "_shovel", "_hoe", "stick", "rod",
                    ]
                    .iter()
                    .any(|t| material.ends_with(t));
                    format!(
                        "minecraft:item/{}",
                        if handheld { "handheld" } else { "generated" }
                    )
                });
            let layers = textures
                .iter()
                .enumerate()
                .map(|(i, t)| format!(r#""layer{i}":"{}""#, resource_ref(ns, t)))
                .collect::<Vec<_>>()
                .join(",");
            (
                "item",
                format!(r#"{{"parent":"{parent}","textures":{{{layers}}}}}"#),
            )
        };
        files.insert(
            format!("assets/{ns}/models/{folder}/{id}.json"),
            body.into_bytes(),
        );
        Some(format!("{ns}:{folder}/{id}"))
    } else {
        None
    };
    // `model_id`(custom_model_data): 바닐라 아이템 모양 정의가 이 번호로 우리 모델을 고름
    let model_id = resource
        .and_then(|r| r.get("model_id"))
        .and_then(Json::as_f64)
        .map(|n| n as i32);
    if let (Some(cmd), Some(m)) = (model_id, &model) {
        cmd_models.push((material.to_owned(), cmd, m.clone()));
    }
    // 아이템 모양 정의 (assets/<ns>/items/<id>.json) → item_model = ns:id
    let item_model = model.as_ref().filter(|_| model_id.is_none()).map(|m| {
        files.insert(
            format!("assets/{ns}/items/{id}.json"),
            format!(r#"{{"model":{{"type":"minecraft:model","model":"{m}"}}}}"#).into_bytes(),
        );
        full.clone()
    });
    let damage = it
        .get("attribute_modifiers")
        .and_then(|a| a.get("mainhand"))
        .and_then(|m| m.get("attackDamage").or_else(|| m.get("attack_damage")))
        .and_then(Json::as_f64)
        .map(|d| d as f32);
    let rarity = match s(it, "rarity")
        .unwrap_or("common")
        .to_ascii_lowercase()
        .as_str()
    {
        "uncommon" => 1,
        "rare" => 2,
        "epic" => 3,
        _ => 0,
    };
    let def = CustomItem {
        id: full.clone(),
        base,
        name: legacy_colors(
            s(it, "display_name")
                .or_else(|| s(it, "name"))
                .unwrap_or(id),
        ),
        color: "white".into(),
        lore: strings(it.get("lore"))
            .iter()
            .map(|l| legacy_colors(l))
            .collect(),
        model: item_model,
        glint: it
            .get("enchant_glint")
            .or_else(|| it.get("glint"))
            .and_then(Json::as_bool)
            .unwrap_or(false),
        rarity,
        max_stack: it
            .get("max_stack_size")
            .and_then(Json::as_f64)
            .map(|n| n as i32),
        damage,
        ability: None,
        cooldown: 0.0,
        custom_model_data: model_id.map(|n| n as f32),
        extra: Vec::new(),
    };
    let block_info = block.map(|b| {
        let drop = b
            .get("drop_when_mined")
            .and_then(Json::as_bool)
            .unwrap_or(true);
        let model = model
            .clone()
            .unwrap_or_else(|| "minecraft:block/note_block".into());
        (full.clone(), model, drop)
    });
    Ok((def, block_info))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(dir: &Path) {
        let pack = dir.join("gems");
        std::fs::create_dir_all(pack.join("configs")).unwrap();
        std::fs::create_dir_all(pack.join("resourcepack/assets/myitems/textures/item")).unwrap();
        std::fs::create_dir_all(pack.join("resourcepack/assets/myitems/textures/font")).unwrap();
        std::fs::write(
            pack.join("resourcepack/assets/myitems/textures/item/ruby.png"),
            b"png",
        )
        .unwrap();
        std::fs::write(
            pack.join("resourcepack/assets/myitems/textures/font/heart.png"),
            b"png",
        )
        .unwrap();
        std::fs::write(
            pack.join("configs/items.yml"),
            r#"
info:
  namespace: myitems
items:
  ruby:
    display_name: "&c루비 :heart:"
    lore: ["&7빛나는 보석"]
    resource:
      material: EMERALD
      generate: true
      textures: [item/ruby.png]
    attribute_modifiers: {mainhand: {attackDamage: 7}}
  ruby_block:
    display_name: 루비 블록
    resource: {material: PAPER, generate: true, textures: [block/ruby_block.png]}
    specific_properties:
      block:
        placed_model: {type: REAL_NOTE}
  bad:
    resource: {material: NOT_A_THING}
font_images:
  heart: {path: font/heart.png, scale_ratio: 9, y_position: 8}
categories:
  gems: {name: "&c보석", icon: myitems:ruby, items: [ruby, ruby_block]}
"#,
        )
        .unwrap();
    }

    #[test]
    fn loads_items_blocks_fonts_and_builds_pack_files() {
        let dir = std::env::temp_dir().join(format!("dangu-contents-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        sample(&dir);
        let l = load(&dir);
        let mine: Vec<&String> = l
            .items
            .iter()
            .filter(|i| !i.starts_with("dangu:"))
            .collect();
        assert_eq!(mine, vec!["myitems:ruby", "myitems:ruby_block"]);
        assert!(
            l.items.contains(&"dangu:lottery_ticket".to_owned()),
            "built-in pack"
        );
        assert_eq!(l.errors.len(), 1, "{:?}", l.errors);
        assert!(l.errors[0].contains("not_a_thing"), "{:?}", l.errors);
        let ruby = custom_items::get(custom_items::index_of("myitems:ruby").unwrap()).unwrap();
        assert_eq!(ruby.model.as_deref(), Some("myitems:ruby"));
        assert_eq!(ruby.damage, Some(7.0));
        assert!(ruby.name.starts_with("§c루비 "));
        assert!(
            ruby.name.ends_with('\u{E000}'),
            "emoji in name: {}",
            ruby.name
        );
        let names: Vec<&str> = l.files.iter().map(|(n, _)| n.as_str()).collect();
        for want in [
            "assets/myitems/textures/item/ruby.png",
            "assets/myitems/models/item/ruby.json",
            "assets/myitems/items/ruby.json",
            "assets/myitems/models/block/ruby_block.json",
            "assets/minecraft/blockstates/note_block.json",
            "assets/minecraft/font/default.json",
        ] {
            assert!(names.contains(&want), "{want} in {names:?}");
        }
        let model = &l
            .files
            .iter()
            .find(|(n, _)| n == "assets/myitems/models/item/ruby.json")
            .unwrap()
            .1;
        assert!(String::from_utf8_lossy(model).contains(r#""layer0":"myitems:item/ruby""#));
        // 블록: 켜진 소리 블록 상태 하나를 받고, 되돌려 찾을 수 있음
        let block_index = custom_items::index_of("myitems:ruby_block").unwrap();
        let state = l.block_of_item[&block_index];
        assert_eq!(blocks::property(state, "powered"), Some("true"));
        assert_eq!(l.item_of_block[&state], (block_index, true));
        let bs = &l
            .files
            .iter()
            .find(|(n, _)| n.ends_with("note_block.json"))
            .unwrap()
            .1;
        let bs = String::from_utf8_lossy(bs);
        assert!(bs.contains("myitems:block/ruby_block"));
        assert!(
            bs.contains("instrument=harp,note=0,powered=false"),
            "vanilla states kept"
        );
        assert_eq!(
            l.categories[0].items,
            vec!["myitems:ruby", "myitems:ruby_block"]
        );
        // 다시 읽어도 같은 상태·글자 (기억)
        let again = load(&dir);
        assert_eq!(again.block_of_item[&block_index], state);
        assert_eq!(again.fonts[0].ch, '\u{E000}');
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn emoji_replacement() {
        let l = Loaded {
            fonts: vec![FontImage {
                id: "myitems:heart".into(),
                ch: '\u{E001}',
                file: String::new(),
                height: 9,
                ascent: 8,
            }],
            ..Default::default()
        };
        *CURRENT.write().unwrap() = Some(Arc::new(l));
        assert_eq!(
            emojify("사랑 :heart: 해요 :myitems:heart:"),
            "사랑 \u{E001} 해요 \u{E001}"
        );
        assert_eq!(emojify("시간 12:30 :nope:"), "시간 12:30 :nope:");
        *CURRENT.write().unwrap() = None;
    }
}
