//! 팩들을 읽어 [`Loaded`]를 만듭니다.
//!
//! 1. 모든 설정을 읽고, `font_images`(이모지)부터 글자를 정합니다 (이름의 `:이모지:`에 필요).
//! 2. 아이템: `template`/`variant_of`(깊이 합치기), `enabled`, `components_nbt_file`을 거쳐
//!    [`read_item`]으로.
//! 3. 그 밖의 칸: `equipments`, `sounds`, `minecraft_lang_overwrite`, `dictionary`, `categories`.
//! 4. 리소스 팩 파일: 블록 상태, custom_model_data 분기, 폰트, 툴팁 그림.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::data::blocks;
use crate::game::custom_items::{self, CustomItem};
use crate::json::{self, Json};

use super::cache::*;
use super::item_config::read_item;
use super::*;

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

/// `over`를 `base` 위에 깊이 합칩니다 (맵은 키마다, 나머지는 바꿔치기).
pub(super) fn deep_merge(base: &Json, over: &Json) -> Json {
    match (base, over) {
        (Json::Object(b), Json::Object(o)) => {
            let mut out = b.clone();
            for (k, v) in o {
                match out.iter_mut().find(|(bk, _)| bk == k) {
                    Some((_, bv)) => *bv = deep_merge(bv, v),
                    None => out.push((k.clone(), v.clone())),
                }
            }
            Json::Object(out)
        }
        (_, o) => o.clone(),
    }
}

fn without(j: &Json, keys: &[&str]) -> Json {
    match j {
        Json::Object(f) => Json::Object(
            f.iter()
                .filter(|(k, _)| !keys.contains(&k.as_str()))
                .cloned()
                .collect(),
        ),
        o => o.clone(),
    }
}

/// 설정 하나.
struct Doc {
    where_: String,
    ns: String,
    doc: Json,
}

/// 분류의 `items` 한 줄: 정확한 이름, `ns:*`·`*:id`·`ns:prefix_*` 같은 별표, 또는 정규식.
pub(super) fn item_matches(pattern: &str, id: &str) -> bool {
    if pattern == id {
        return true;
    }
    let regexish = pattern.contains('\\')
        || pattern.contains('(')
        || pattern.contains(".*")
        || pattern.contains(".+");
    if regexish {
        return regex_match(&regex_tokens(pattern), &id.chars().collect::<Vec<_>>());
    }
    if pattern.contains('*') {
        let tokens: Vec<Tok> = pattern
            .chars()
            .map(|c| {
                if c == '*' {
                    Tok::Any(true)
                } else {
                    Tok::Lit(c, false)
                }
            })
            .collect();
        return regex_match(&tokens, &id.chars().collect::<Vec<_>>());
    }
    false
}

/// 아주 작은 정규식: 글자, `\x`, `.`, `*`, `+`, `?`. 괄호는 무시 (`(.*)` 같은 쓰임만 봄).
#[derive(Debug, Clone)]
enum Tok {
    /// 아무 글자 (참이면 0개 이상)
    Any(bool),
    /// 글자, 0개 이상인가
    Lit(char, bool),
    AnyOne,
    Plus(Box<Tok>),
    Opt(Box<Tok>),
}

fn regex_tokens(p: &str) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::new();
    let mut chars = p.chars().peekable();
    while let Some(c) = chars.next() {
        let atom = match c {
            '(' | ')' | '^' | '$' => continue,
            '\\' => Tok::Lit(chars.next().unwrap_or('\\'), false),
            '.' => Tok::AnyOne,
            '*' => {
                if let Some(last) = out.pop() {
                    out.push(match last {
                        Tok::AnyOne => Tok::Any(true),
                        Tok::Lit(c, _) => Tok::Lit(c, true),
                        other => other,
                    });
                }
                continue;
            }
            '+' => {
                if let Some(last) = out.pop() {
                    out.push(Tok::Plus(Box::new(last)));
                }
                continue;
            }
            '?' => {
                if let Some(last) = out.pop() {
                    out.push(Tok::Opt(Box::new(last)));
                }
                continue;
            }
            c => Tok::Lit(c, false),
        };
        out.push(atom);
    }
    out
}

fn one(t: &Tok, c: char) -> bool {
    match t {
        Tok::AnyOne | Tok::Any(_) => true,
        Tok::Lit(l, _) => *l == c,
        Tok::Plus(b) | Tok::Opt(b) => one(b, c),
    }
}

fn regex_match(toks: &[Tok], s: &[char]) -> bool {
    let Some((t, rest)) = toks.split_first() else {
        return s.is_empty();
    };
    match t {
        Tok::Any(true) | Tok::Lit(_, true) => {
            // 0개부터 될 때까지
            let mut i = 0;
            loop {
                if regex_match(rest, &s[i..]) {
                    return true;
                }
                if i < s.len() && one(t, s[i]) {
                    i += 1;
                } else {
                    return false;
                }
            }
        }
        Tok::Plus(b) => {
            !s.is_empty() && one(b, s[0]) && {
                let star = match b.as_ref() {
                    Tok::AnyOne => Tok::Any(true),
                    Tok::Lit(c, _) => Tok::Lit(*c, true),
                    other => other.clone(),
                };
                let mut next = vec![star];
                next.extend_from_slice(rest);
                regex_match(&next, &s[1..])
            }
        }
        Tok::Opt(b) => {
            regex_match(rest, s) || (!s.is_empty() && one(b, s[0]) && regex_match(rest, &s[1..]))
        }
        _ => !s.is_empty() && one(t, s[0]) && regex_match(rest, &s[1..]),
    }
}

/// `minecraft_lang_overwrite`의 `ALL`이 뜻하는 언어들.
const ALL_LOCALES: &[&str] = &[
    "en_us", "en_gb", "ko_kr", "ja_jp", "zh_cn", "zh_tw", "de_de", "fr_fr", "es_es", "it_it",
    "pt_br", "ru_ru", "pl_pl", "tr_tr", "th_th", "nl_nl", "cs_cz",
];

fn merge_json_file(files: &mut BTreeMap<String, Vec<u8>>, key: &str, add: Vec<(String, Json)>) {
    let mut base = files
        .get(key)
        .and_then(|b| json::parse(&String::from_utf8_lossy(b)).ok())
        .unwrap_or(Json::Object(Vec::new()));
    base = deep_merge(&base, &Json::Object(add));
    files.insert(key.to_owned(), base.compact().into_bytes());
}

pub fn load(base: &Path) -> Loaded {
    let mut l = Loaded::default();
    let cache_path = base.join(".cache").join("ids.json");
    let mut cache = read_cache(&cache_path);
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // ---- 1. 읽기 ----
    let mut docs: Vec<Doc> = Vec::new();
    let mut packs = builtin::packs();
    packs.extend(source::from_disk(base));
    for pack in packs {
        for (path, bytes) in pack.files {
            files.insert(path, bytes);
        }
        for (where_, text) in pack.configs {
            match crate::yaml::parse(&text) {
                Ok(doc) => {
                    let ns = doc
                        .get("info")
                        .and_then(|i| s(i, "namespace"))
                        .unwrap_or(&pack.name)
                        .to_owned();
                    docs.push(Doc { where_, ns, doc });
                }
                Err(e) => l.errors.push(format!("{where_}: {e}")),
            }
        }
    }

    // 옛 사전 (`dictionary:` 키 → 글): 이름이 키와 똑같으면 바꿔 씀
    let mut dictionary: HashMap<String, String> = HashMap::new();
    for d in &docs {
        if let Some(Json::Object(entries)) = d.doc.get("dictionary") {
            for (k, v) in entries {
                if let Some(v) = v.as_str() {
                    dictionary.insert(k.clone(), v.to_owned());
                }
            }
        }
    }
    let dict = |t: &str| dictionary.get(t).cloned().unwrap_or_else(|| t.to_owned());

    // ---- 2. 이모지 글자 ----
    let mut font_defs: Vec<(String, String, i32, i32, Option<char>)> = Vec::new();
    for d in &docs {
        let Some(Json::Object(entries)) = d.doc.get("font_images") else {
            continue;
        };
        for (id, f) in entries {
            let Some(path) = s(f, "path") else {
                l.errors
                    .push(format!("{}: font_images.{id}: path가 없습니다", d.where_));
                continue;
            };
            let height = f.get("scale_ratio").and_then(Json::as_f64).unwrap_or(9.0) as i32;
            let ascent = f.get("y_position").and_then(Json::as_f64).unwrap_or(8.0) as i32;
            let symbol = s(f, "symbol").and_then(|x| x.chars().next());
            font_defs.push((
                format!("{}:{id}", d.ns),
                format!("{}.png", resource_ref(&d.ns, path)),
                height.clamp(1, 256),
                ascent.min(height),
                symbol,
            ));
        }
    }
    let mut used: Vec<u32> = cache.chars.values().copied().collect();
    used.extend(font_defs.iter().filter_map(|f| f.4.map(|c| c as u32)));
    for (id, file, height, ascent, symbol) in font_defs {
        let code = match (symbol, cache.chars.get(&id)) {
            (Some(c), _) => c as u32,
            (None, Some(&c)) => c,
            (None, None) => {
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
    let fonts = l.fonts.clone();
    let emoji = move |t: &str| -> String {
        let mut out = dict(t);
        for f in &fonts {
            let short = f.id.split_once(':').map_or(f.id.as_str(), |(_, n)| n);
            let ch = f.ch.to_string();
            out = out
                .replace(&format!(":{}:", f.id), &ch)
                .replace(&format!(":{short}:"), &ch)
                .replace(&format!("%img_{short}%"), &ch);
        }
        out
    };

    // ---- 3. 아이템 ----
    let mut defs: Vec<CustomItem> = Vec::new();
    let mut block_models: Vec<(String, String, bool)> = Vec::new();
    let mut cmd_models: Vec<(String, i32, Json)> = Vec::new();
    // 틀(template)은 이름공간마다
    let mut raw: HashMap<String, Json> = HashMap::new();
    for d in &docs {
        if let Some(Json::Object(entries)) = d.doc.get("items") {
            for (id, it) in entries {
                raw.insert(format!("{}:{id}", d.ns), it.clone());
            }
        }
    }
    for d in &docs {
        let Some(Json::Object(entries)) = d.doc.get("items") else {
            continue;
        };
        for (id, it) in entries {
            if it.get("template").and_then(Json::as_bool) == Some(true) {
                continue;
            }
            if it.get("enabled").and_then(Json::as_bool) == Some(false) {
                continue;
            }
            let mut it = it.clone();
            // 변형: 틀 위에 깊이 합침 (틀의 틀도)
            let mut seen = 0;
            while let Some(parent) = s(&it, "variant_of").map(str::to_owned) {
                let key = if parent.contains(':') {
                    parent.clone()
                } else {
                    format!("{}:{parent}", d.ns)
                };
                let Some(t) = raw.get(&key) else {
                    l.errors.push(format!(
                        "{}: {}:{id}: 틀 {parent}이 없습니다",
                        d.where_, d.ns
                    ));
                    break;
                };
                it = deep_merge(&without(t, &["template"]), &without(&it, &["variant_of"]));
                seen += 1;
                if seen > 8 {
                    break;
                }
            }
            // components_nbt_file: 설정 파일 옆의 JSON {"components": {...}}
            if let Some(file) = s(&it, "components_nbt_file") {
                let dir = Path::new(&d.where_).parent().unwrap_or(Path::new(""));
                let path = base.join(dir).join(file);
                match std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|t| json::parse(&t).ok())
                    .and_then(|j| j.get("components").cloned())
                {
                    Some(c) => {
                        it = deep_merge(&it, &Json::Object(vec![("components".into(), c)]));
                    }
                    None => l
                        .warnings
                        .push(format!("{}: {file}을 읽지 못했습니다", d.where_)),
                }
            }
            match read_item(&d.ns, id, &it, &mut files, &mut cmd_models, &emoji) {
                Ok(out) => {
                    for w in out.warnings {
                        l.warnings.push(format!("{}:{id}: {w}", d.ns));
                    }
                    if let Some(b) = out.block {
                        block_models.push(b);
                    }
                    if out.hidden {
                        l.hidden.push(out.def.id.clone());
                    }
                    if let Some(p) = out.permission {
                        l.item_permissions.insert(out.def.id.clone(), p);
                    }
                    l.items.push(out.def.id.clone());
                    defs.push(out.def);
                }
                Err(e) => l.errors.push(format!("{}: {}:{id}: {e}", d.where_, d.ns)),
            }
            // 툴팁 그림: textures/<path>_background.png → gui/sprites/tooltip/<path>_background.png
            if let Some((tns, tpath)) = s(&it, "tooltip_style").and_then(|x| x.split_once(':')) {
                for part in ["background", "frame"] {
                    for ext in ["png", "png.mcmeta"] {
                        let from = format!("assets/{tns}/textures/{tpath}_{part}.{ext}");
                        if let Some(b) = files.get(&from).cloned() {
                            files.insert(
                                format!(
                                    "assets/{tns}/textures/gui/sprites/tooltip/{tpath}_{part}.{ext}"
                                ),
                                b,
                            );
                        }
                    }
                }
            }
        }
    }

    // ---- 4. 그 밖의 칸 ----
    for d in &docs {
        let ns = &d.ns;
        // 장비 (갑옷 겉모습)
        if let Some(Json::Object(entries)) = d.doc.get("equipments") {
            for (id, e) in entries {
                let mut layers = Vec::new();
                for (key, layer) in [("layer_1", "humanoid"), ("layer_2", "humanoid_leggings")] {
                    let Some(tex) = s(e, key) else { continue };
                    let r = resource_ref(ns, tex);
                    let (tns, tpath) = r.split_once(':').unwrap();
                    match files
                        .get(&format!("assets/{tns}/textures/{tpath}.png"))
                        .cloned()
                    {
                        Some(b) => {
                            files.insert(
                                format!("assets/{ns}/textures/entity/equipment/{layer}/{id}.png"),
                                b,
                            );
                            layers.push((
                                layer.to_owned(),
                                Json::Array(vec![Json::Object(vec![(
                                    "texture".into(),
                                    Json::String(format!("{ns}:{id}")),
                                )])]),
                            ));
                        }
                        None => l
                            .warnings
                            .push(format!("equipments.{id}: 그림 {r}이 없습니다")),
                    }
                }
                files.insert(
                    format!("assets/{ns}/equipment/{id}.json"),
                    Json::Object(vec![("layers".into(), Json::Object(layers))])
                        .compact()
                        .into_bytes(),
                );
            }
        }
        // 소리 (4.0.12+): sounds.<id>.path + settings
        if let Some(Json::Object(entries)) = d.doc.get("sounds") {
            let mut add = Vec::new();
            for (id, sd) in entries {
                let path = s(sd, "path").unwrap_or(id);
                let mut one = vec![("name".to_owned(), Json::String(resource_ref(ns, path)))];
                let settings = sd.get("settings");
                let mut subtitle = None;
                if let Some(Json::Object(st)) = settings {
                    for (k, v) in st {
                        if k == "subtitle" {
                            subtitle = Some(v.clone());
                        } else {
                            one.push((k.clone(), v.clone()));
                        }
                    }
                }
                let mut entry = vec![("sounds".to_owned(), Json::Array(vec![Json::Object(one)]))];
                if let Some(sub) = subtitle {
                    entry.push(("subtitle".into(), sub));
                }
                add.push((id.clone(), Json::Object(entry)));
            }
            merge_json_file(&mut files, &format!("assets/{ns}/sounds.json"), add);
        }
        // 클라이언트 번역 덮어쓰기
        if let Some(Json::Object(entries)) = d.doc.get("minecraft_lang_overwrite") {
            for (_, o) in entries {
                let Some(Json::Object(e)) = o.get("entries") else {
                    continue;
                };
                let langs = strings(o.get("languages"));
                let locales: Vec<String> = if langs.iter().any(|x| x.eq_ignore_ascii_case("all")) {
                    ALL_LOCALES.iter().map(|x| (*x).to_owned()).collect()
                } else {
                    langs.iter().map(|x| x.to_ascii_lowercase()).collect()
                };
                for loc in locales {
                    merge_json_file(
                        &mut files,
                        &format!("assets/minecraft/lang/{loc}.json"),
                        e.clone(),
                    );
                }
            }
        }
    }

    // ---- 5. 분류 (/ia) ----
    let mut merged: Vec<Category> = Vec::new();
    for d in &docs {
        let Some(Json::Object(entries)) = d.doc.get("categories") else {
            continue;
        };
        for (id, c) in entries {
            if c.get("enabled") == Some(&Json::Bool(false)) {
                continue;
            }
            let patterns = strings(c.get("items"));
            let mut items: Vec<String> = Vec::new();
            for p in &patterns {
                let p = if p.contains(':') || p.contains('*') {
                    p.clone()
                } else {
                    format!("{}:{p}", d.ns)
                };
                for item in &l.items {
                    if item_matches(&p, item) && !items.contains(item) && !l.hidden.contains(item) {
                        items.push(item.clone());
                    }
                }
                // 바닐라 아이템 이름도 받음
                if !p.contains('*')
                    && !l.items.contains(&p)
                    && crate::data::items::by_name(p.trim_start_matches("minecraft:")).is_some()
                {
                    items.push(p.clone());
                }
            }
            let name = text_format::plain(&emoji(s(c, "name").unwrap_or(id)));
            // 이름이 같은 분류는 합침
            if let Some(m) = merged.iter_mut().find(|m| m.name == name) {
                for i in items {
                    if !m.items.contains(&i) {
                        m.items.push(i);
                    }
                }
                continue;
            }
            merged.push(Category {
                id: format!("{}:{id}", d.ns),
                name,
                icon: s(c, "icon")
                    .map(str::to_owned)
                    .or_else(|| items.first().cloned())
                    .unwrap_or_else(|| "paper".into()),
                items,
                permission: s(c, "permission").map(str::to_owned),
            });
        }
    }
    l.categories = merged;

    // 이모지 글자는 이름·설명 문자열에도 (명령어 목록 등)
    custom_items::install(defs.clone());

    // ---- 6. 블록 상태 ----
    let free = free_note_states();
    let mut taken: Vec<u32> = cache.blocks.values().copied().collect();
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

    // ---- 7. model_id: 바닐라 아이템마다 custom_model_data로 고르는 모양 정의 ----
    let mut by_material: BTreeMap<String, Vec<(i32, Json)>> = BTreeMap::new();
    for (material, cmd, tree) in cmd_models {
        by_material.entry(material).or_default().push((cmd, tree));
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
            .unwrap_or_else(|| {
                Json::Object(vec![
                    ("type".into(), Json::String("minecraft:model".into())),
                    (
                        "model".into(),
                        Json::String(format!("minecraft:item/{material}")),
                    ),
                ])
            });
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
                        .into_iter()
                        .map(|(c, tree)| {
                            Json::Object(vec![
                                ("threshold".into(), Json::Number(c as f64)),
                                ("model".into(), tree),
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

    // ---- 8. 이모지 폰트 (팩에 이미 default.json이 있으면 이어 붙임) ----
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
    // 목록이 없으면 이름공간마다 하나씩 (서버 내장 dangu는 빼고)
    if l.categories.is_empty() {
        let mut by_ns: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for id in l.items.iter().filter(|i| !l.hidden.contains(i)) {
            let ns = id.split(':').next().unwrap_or("").to_owned();
            if ns != "dangu" {
                by_ns.entry(ns).or_default().push(id.clone());
            }
        }
        for (ns, items) in by_ns {
            l.categories.push(Category {
                id: ns.clone(),
                name: ns,
                icon: items[0].clone(),
                items,
                permission: None,
            });
        }
    }
    write_cache(&cache_path, &cache);
    l.files = files.into_iter().collect();
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_merge_and_regex() {
        assert!(item_matches("example:*", "example:ruby"));
        assert!(!item_matches("example:*", "other:ruby"));
        assert!(item_matches("*:ruby", "other:ruby"));
        assert!(item_matches(
            "myitems:hidden_item_*",
            "myitems:hidden_item_3"
        ));
        assert!(item_matches(
            r"iasurvival\:(.*)_helmet",
            "iasurvival:ruby_helmet"
        ));
        assert!(!item_matches(
            r"iasurvival\:(.*)_helmet",
            "iasurvival:ruby_boots"
        ));
        assert!(item_matches("a:b.+c", "a:bxxc"));
        let a = crate::yaml::parse("graphics:\n  texture: a\n  color: RED\nlore: [x]\n").unwrap();
        let b = crate::yaml::parse("graphics:\n  color: BLUE\nlore: [y]\n").unwrap();
        let m = deep_merge(&a, &b);
        assert_eq!(
            m.get("graphics")
                .unwrap()
                .get("texture")
                .and_then(Json::as_str),
            Some("a")
        );
        assert_eq!(
            m.get("graphics")
                .unwrap()
                .get("color")
                .and_then(Json::as_str),
            Some("BLUE")
        );
        assert_eq!(strings(m.get("lore")), vec!["y".to_owned()]);
    }
}
