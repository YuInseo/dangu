//! 팩들을 읽어 [`Loaded`]를 만듭니다: 아이템·블록·이모지·분류, 그리고 리소스 팩에 넣을 파일
//! (모델, 블록 상태, 폰트, custom_model_data 분기).

use std::collections::BTreeMap;
use std::path::Path;

use crate::data::blocks;
use crate::game::custom_items::{self, CustomItem};
use crate::json::{self, Json};
use crate::text::legacy_colors;

use super::cache::*;
use super::item_config::read_item;
use super::*;

pub(super) fn model_ref_json(model: &str) -> Json {
    Json::Object(vec![
        ("type".into(), Json::String("minecraft:model".into())),
        ("model".into(), Json::String(model.to_owned())),
    ])
}

/// `item/ruby.png` → `ns:item/ruby`.
pub(super) fn resource_ref(ns: &str, path: &str) -> String {
    let p = path.trim_end_matches(".png").trim_end_matches(".json");
    if p.contains(':') {
        p.to_owned()
    } else {
        format!("{ns}:{p}")
    }
}

pub(super) fn s<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    j.get(k).and_then(Json::as_str)
}

pub(super) fn strings(j: Option<&Json>) -> Vec<String> {
    match j {
        Some(Json::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        Some(Json::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
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
