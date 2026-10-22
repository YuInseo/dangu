//! 아이템 그림: `graphics.*`(새 방식)과 `resource.*`(옛 방식)을 모델 파일과 아이템 모양 정의
//! (`assets/<ns>/items/<id>.json`의 `model` 나무)로 만듭니다.
//!
//! - `texture`/`textures` → 모델을 만들어 줌 (부모: `parent`, 블록이면 `cube`/`cube_all`, 아니면
//!   바닐라 그 상태의 모델 — 활 당기기·낚싯대 던짐 같은 손 자세가 그대로 남음)
//! - `model`/`models` → 주어진 모델 그대로
//! - 상태(`normal`, `pulling_0..`, `cast`, `blocking`, `throwing`, `arrow`, `rocket`, `in_hand`,
//!   `open_back`, `open_front` …)는 바닐라 아이템 모양 정의의 모델 이름을 바꿔 끼워 맞춥니다
//!   (방패·삼지창은 `using_item` 조건으로)
//! - `icon` → 창(GUI)에서만 다른 그림 (`display_context` 선택)
//! - `color` → `dye` 색 입히기, `hand_animation_on_swap`·`oversized_in_gui` → 정의에 그대로
//! - 옛 방식 `generate: true`는 그림 옆의 `_0`·`_1`·`_2`·`_cast`·`_blocking`·`_firework`·`_filled`
//!   파일을 찾아 상태로 씁니다.

use std::collections::BTreeMap;

use crate::json::{self, Json};

use super::load::resource_ref;

/// 만든 모양: 정의의 `model` 나무와 그 밖의 칸들.
#[derive(Debug, Clone)]
pub struct Look {
    pub tree: Json,
    pub top: Vec<(String, Json)>,
}

fn s(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_owned)
}

fn model_node(model: &str, tint: Option<i32>) -> Json {
    let mut f = vec![
        ("type".into(), Json::String("minecraft:model".into())),
        ("model".into(), Json::String(model.to_owned())),
    ];
    if let Some(c) = tint {
        f.push((
            "tints".into(),
            Json::Array(vec![Json::Object(vec![
                ("type".into(), Json::String("minecraft:dye".into())),
                ("default".into(), Json::Number(c as f64)),
            ])]),
        ));
    }
    Json::Object(f)
}

/// 바닐라 모델 이름 → 상태 이름 (`minecraft:item/bow_pulling_1` → `pulling_1`).
fn state_of(vanilla_ref: &str, material: &str) -> String {
    let path = vanilla_ref
        .strip_prefix("minecraft:")
        .unwrap_or(vanilla_ref)
        .strip_prefix("item/")
        .unwrap_or(vanilla_ref);
    let rest = path.strip_prefix(material).unwrap_or(path);
    let state = rest.trim_start_matches('_');
    match state {
        "" => "normal".into(),
        "firework" => "rocket".into(),
        "bundle_open_back" | "open_back" => "open_back".into(),
        "bundle_open_front" | "open_front" => "open_front".into(),
        other => other.to_owned(),
    }
}

/// 모델 이름들을 바꿔 끼운 나무.
fn substitute(tree: &Json, map: &dyn Fn(&str) -> String, tint: Option<i32>) -> Json {
    match tree {
        Json::Object(f) => {
            let is_model = f.iter().any(|(k, v)| {
                k == "type"
                    && v.as_str()
                        .is_some_and(|t| t.ends_with("model") && !t.ends_with("special"))
            });
            if is_model
                && let Some(m) = f
                    .iter()
                    .find(|(k, _)| k == "model")
                    .and_then(|(_, v)| v.as_str())
            {
                return model_node(&map(m), tint);
            }
            Json::Object(
                f.iter()
                    .map(|(k, v)| (k.clone(), substitute(v, map, tint)))
                    .collect(),
            )
        }
        Json::Array(a) => Json::Array(a.iter().map(|x| substitute(x, map, tint)).collect()),
        other => other.clone(),
    }
}

/// 상태별 모델로 모양 나무를 만듭니다.
fn tree_for(material: &str, states: &BTreeMap<String, String>, tint: Option<i32>) -> Json {
    let normal = states
        .get("normal")
        .or_else(|| states.values().next())
        .cloned()
        .unwrap_or_else(|| format!("minecraft:item/{material}"));
    let pick = |st: &str| states.get(st).cloned().unwrap_or_else(|| normal.clone());
    let using_item = |other: &str| {
        Json::Object(vec![
            ("type".into(), Json::String("minecraft:condition".into())),
            (
                "property".into(),
                Json::String("minecraft:using_item".into()),
            ),
            ("on_false".into(), model_node(&normal, tint)),
            ("on_true".into(), model_node(&pick(other), tint)),
        ])
    };
    if states.len() <= 1 {
        return model_node(&normal, tint);
    }
    match material {
        "shield" => return using_item("blocking"),
        "trident" => return using_item("throwing"),
        _ => {}
    }
    match crate::data::vanilla_item_definition(material).and_then(|v| v.get("model").cloned()) {
        Some(v) => substitute(&v, &|m| pick(&state_of(m, material)), tint),
        None => model_node(&normal, tint),
    }
}

/// 생성 모델의 부모가 하나의 그림 변수만 가질 때 그 이름.
fn single_variable(parent: &str, files: &BTreeMap<String, Vec<u8>>) -> String {
    let p = parent.strip_prefix("minecraft:").unwrap_or(parent);
    match p {
        "block/cross" | "block/tinted_cross" => return "cross".into(),
        "block/cube_all" | "block/leaves" => return "all".into(),
        "block/crop" => return "crop".into(),
        "block/flower_pot_cross" => return "plant".into(),
        _ => {}
    }
    // 팩에 든 부모 모델이면 그 안의 변수 이름
    if let Some((ns, path)) = parent.split_once(':')
        && let Some(b) = files.get(&format!("assets/{ns}/models/{path}.json"))
        && let Ok(j) = json::parse(&String::from_utf8_lossy(b))
        && let Some(Json::Object(t)) = j.get("textures")
        && let [(k, _)] = t.as_slice()
    {
        return k.clone();
    }
    "layer0".into()
}

fn handheld(material: &str) -> bool {
    [
        "_sword", "_axe", "_pickaxe", "_shovel", "_hoe", "stick", "_rod", "_spear", "mace",
    ]
    .iter()
    .any(|t| material.ends_with(t))
}

fn write_model(files: &mut BTreeMap<String, Vec<u8>>, ns: &str, name: &str, body: Json) -> String {
    files.insert(
        format!("assets/{ns}/models/{name}.json"),
        body.compact().into_bytes(),
    );
    format!("{ns}:{name}")
}

fn generated(parent: &str, var: &str, tex: &str) -> Json {
    Json::Object(vec![
        ("parent".into(), Json::String(parent.to_owned())),
        (
            "textures".into(),
            Json::Object(vec![(var.to_owned(), Json::String(tex.to_owned()))]),
        ),
    ])
}

const FACES: [&str; 6] = ["north", "south", "east", "west", "up", "down"];

/// 옛 방식 그림 옆에 있는 상태 파일들 (`_0`, `_cast` …).
fn legacy_states(
    material: &str,
    base_tex: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Vec<(&'static str, String)> {
    let suffixes: &[(&str, &str)] = match material {
        "bow" => &[
            ("pulling_0", "_0"),
            ("pulling_1", "_1"),
            ("pulling_2", "_2"),
        ],
        "crossbow" => &[
            ("pulling_0", "_0"),
            ("pulling_1", "_1"),
            ("pulling_2", "_2"),
            ("arrow", "_charged"),
            ("rocket", "_firework"),
        ],
        "fishing_rod" => &[("cast", "_cast")],
        "shield" => &[("blocking", "_blocking")],
        "bundle" => &[("filled", "_filled")],
        _ => &[],
    };
    let Some((ns, path)) = base_tex.split_once(':') else {
        return Vec::new();
    };
    suffixes
        .iter()
        .filter(|(_, suf)| files.contains_key(&format!("assets/{ns}/textures/{path}{suf}.png")))
        .map(|(st, suf)| (*st, format!("{base_tex}{suf}")))
        .collect()
}

/// 아이템 하나의 그림. 그릴 것이 없으면 None (바탕 아이템 그대로).
pub fn build(
    ns: &str,
    id: &str,
    it: &Json,
    material: &str,
    is_block: bool,
    tint: Option<i32>,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Option<Look> {
    let graphics = it.get("graphics");
    let resource = it.get("resource");
    let r = |p: &str| resource_ref(ns, p);
    let mut states: BTreeMap<String, String> = BTreeMap::new();
    let mut icon: Option<String> = None;
    let mut top = Vec::new();

    if let Some(g) = graphics {
        let parent = s(g, "parent").map(|p| {
            if p.contains(':') {
                p
            } else if p.starts_with("block/") || p.starts_with("item/") {
                format!("minecraft:{p}")
            } else {
                r(&p)
            }
        });
        // 부모가 팩 안의 모델이면 그 이름공간으로
        let parent = parent.map(|p| {
            let local = r(p.strip_prefix("minecraft:").unwrap_or(&p));
            if files.contains_key(&format!(
                "assets/{}/models/{}.json",
                ns,
                local.split_once(':').unwrap().1
            )) {
                local
            } else {
                p
            }
        });
        if let Some(m) = s(g, "model") {
            states.insert("normal".into(), r(&m));
        }
        if let Some(Json::Object(models)) = g.get("models") {
            for (st, m) in models {
                if let Some(m) = m.as_str() {
                    states.insert(st.clone(), r(m));
                }
            }
        }
        let texture = s(g, "texture");
        let textures = g.get("textures");
        if is_block
            && states.is_empty()
            && (texture.is_some() || textures.is_some())
            && parent.is_none()
        {
            // 블록: 여섯 면 또는 한 그림
            let body = match (textures, &texture) {
                (Some(Json::Object(t)), _)
                    if t.iter().any(|(k, _)| FACES.contains(&k.as_str())) =>
                {
                    let mut tex: Vec<(String, Json)> = t
                        .iter()
                        .filter_map(|(k, v)| Some((k.clone(), Json::String(r(v.as_str()?)))))
                        .collect();
                    if let Some((_, first)) = tex.first().cloned() {
                        tex.push(("particle".into(), first));
                    }
                    Json::Object(vec![
                        ("parent".into(), Json::String("minecraft:block/cube".into())),
                        ("textures".into(), Json::Object(tex)),
                    ])
                }
                (_, Some(t)) => generated("minecraft:block/cube_all", "all", &r(t)),
                _ => generated("minecraft:block/cube_all", "all", "minecraft:block/stone"),
            };
            states.insert(
                "normal".into(),
                write_model(files, ns, &format!("block/{id}"), body),
            );
        } else if let Some(p) = &parent
            && states.is_empty()
        {
            // 부모 모델의 변수를 갈아 끼운 사본
            let mut tex: Vec<(String, Json)> = match textures {
                Some(Json::Object(t)) => t
                    .iter()
                    .filter_map(|(k, v)| Some((k.clone(), Json::String(r(v.as_str()?)))))
                    .collect(),
                _ => Vec::new(),
            };
            if let Some(t) = &texture {
                tex.push((single_variable(p, files), Json::String(r(t))));
            }
            let body = Json::Object(vec![
                ("parent".into(), Json::String(p.clone())),
                ("textures".into(), Json::Object(tex)),
            ]);
            let folder = if is_block { "block" } else { "item" };
            states.insert(
                "normal".into(),
                write_model(files, ns, &format!("{folder}/{id}"), body),
            );
        } else if states.is_empty() {
            let mut tex: BTreeMap<String, String> = BTreeMap::new();
            if let Some(t) = &texture {
                tex.insert("normal".into(), r(t));
            }
            if let Some(Json::Object(t)) = textures {
                for (st, v) in t {
                    if let Some(v) = v.as_str() {
                        tex.insert(st.clone(), r(v));
                    }
                }
            }
            for (st, t) in tex {
                let vanilla_parent = if st == "normal" {
                    // 활·쇠뇌·낚싯대는 바닐라 모델의 손 자세를 그대로
                    if matches!(material, "bow" | "crossbow" | "fishing_rod") {
                        format!("minecraft:item/{material}")
                    } else if handheld(material) {
                        "minecraft:item/handheld".to_owned()
                    } else {
                        "minecraft:item/generated".to_owned()
                    }
                } else {
                    vanilla_state_model(material, &st)
                        .unwrap_or_else(|| "minecraft:item/generated".into())
                };
                let name = if st == "normal" {
                    format!("item/{id}")
                } else {
                    format!("item/{id}_{st}")
                };
                let m = write_model(files, ns, &name, generated(&vanilla_parent, "layer0", &t));
                states.insert(st, m);
            }
        }
        if let Some(i) = s(g, "icon") {
            icon = Some(r(&i));
        }
        for k in ["hand_animation_on_swap", "oversized_in_gui"] {
            if let Some(v) = g.get(k) {
                top.push((k.to_owned(), v.clone()));
            }
        }
    } else if let Some(res) = resource {
        // 옛 방식
        if let Some(m) = s(res, "model_path") {
            states.insert("normal".into(), r(&m));
        } else {
            let mut t: Vec<String> = match res.get("textures") {
                Some(Json::Array(a)) => a
                    .iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect(),
                Some(Json::String(x)) => vec![x.clone()],
                _ => Vec::new(),
            };
            if t.is_empty()
                && let Some(x) = s(res, "texture")
            {
                t.push(x);
            }
            let generate = res
                .get("generate")
                .and_then(Json::as_bool)
                .unwrap_or(!t.is_empty());
            if generate && !t.is_empty() {
                let body = if is_block {
                    if t.len() >= 6 {
                        let tex: Vec<(String, Json)> = FACES
                            .iter()
                            .zip(&t)
                            .map(|(f, x)| (f.to_string(), Json::String(r(x))))
                            .chain(std::iter::once((
                                "particle".to_owned(),
                                Json::String(r(&t[0])),
                            )))
                            .collect();
                        Json::Object(vec![
                            ("parent".into(), Json::String("minecraft:block/cube".into())),
                            ("textures".into(), Json::Object(tex)),
                        ])
                    } else {
                        generated("minecraft:block/cube_all", "all", &r(&t[0]))
                    }
                } else {
                    let parent = s(res, "parent")
                        .map(|p| {
                            if p.contains(':') {
                                p
                            } else {
                                format!("minecraft:{p}")
                            }
                        })
                        .unwrap_or_else(|| {
                            if handheld(material) {
                                "minecraft:item/handheld".into()
                            } else {
                                "minecraft:item/generated".into()
                            }
                        });
                    let layers = t
                        .iter()
                        .enumerate()
                        .map(|(i, x)| (format!("layer{i}"), Json::String(r(x))))
                        .collect();
                    Json::Object(vec![
                        ("parent".into(), Json::String(parent)),
                        ("textures".into(), Json::Object(layers)),
                    ])
                };
                let folder = if is_block { "block" } else { "item" };
                states.insert(
                    "normal".into(),
                    write_model(files, ns, &format!("{folder}/{id}"), body),
                );
                if !is_block {
                    for (st, tex) in legacy_states(material, &r(&t[0]), files) {
                        let parent = vanilla_state_model(material, st)
                            .unwrap_or_else(|| "minecraft:item/generated".into());
                        let m = write_model(
                            files,
                            ns,
                            &format!("item/{id}_{st}"),
                            generated(&parent, "layer0", &tex),
                        );
                        states.insert(st.into(), m);
                    }
                }
            }
        }
        if let Some(i) = s(res, "icon") {
            icon = Some(r(&i));
        }
    }

    if states.is_empty() {
        return None;
    }
    let mut tree = tree_for(material, &states, tint);
    if let Some(icon) = icon {
        // 창에서만 다른 그림 (창 모델은 그림 하나로 만든 평면 모델)
        let icon_model = write_model(
            files,
            ns,
            &format!("item/{id}_icon"),
            generated("minecraft:item/generated", "layer0", &icon),
        );
        let when: Vec<Json> = if material.ends_with("_spear") {
            ["gui", "ground", "fixed"]
                .iter()
                .map(|w| Json::String((*w).into()))
                .collect()
        } else {
            vec![Json::String("gui".into())]
        };
        tree = Json::Object(vec![
            ("type".into(), Json::String("minecraft:select".into())),
            (
                "property".into(),
                Json::String("minecraft:display_context".into()),
            ),
            (
                "cases".into(),
                Json::Array(vec![Json::Object(vec![
                    ("when".into(), Json::Array(when)),
                    ("model".into(), model_node(&icon_model, tint)),
                ])]),
            ),
            ("fallback".into(), tree),
        ]);
    }
    Some(Look { tree, top })
}

/// 바닐라 아이템 모양 정의 안에서 이 상태가 쓰는 모델 (생성 모델의 부모로).
fn vanilla_state_model(material: &str, state: &str) -> Option<String> {
    let def = crate::data::vanilla_item_definition(material)?;
    let mut found = None;
    fn walk(j: &Json, material: &str, state: &str, found: &mut Option<String>) {
        match j {
            Json::Object(f) => {
                for (k, v) in f {
                    if k == "model"
                        && let Some(m) = v.as_str()
                        && state_of(m, material) == state
                    {
                        *found = Some(m.to_owned());
                    }
                    walk(v, material, state, found);
                }
            }
            Json::Array(a) => a.iter().for_each(|x| walk(x, material, state, found)),
            _ => {}
        }
    }
    walk(&def, material, state, &mut found);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(s: &str) -> Json {
        crate::yaml::parse(s).unwrap()
    }

    fn models_in(j: &Json) -> Vec<String> {
        let mut out = Vec::new();
        fn walk(j: &Json, out: &mut Vec<String>) {
            match j {
                Json::Object(f) => {
                    for (k, v) in f {
                        if k == "model"
                            && let Some(m) = v.as_str()
                        {
                            out.push(m.to_owned());
                        }
                        walk(v, out);
                    }
                }
                Json::Array(a) => a.iter().for_each(|x| walk(x, out)),
                _ => {}
            }
        }
        walk(j, &mut out);
        out
    }

    #[test]
    fn modern_bow_states_follow_vanilla_dispatch() {
        let mut files = BTreeMap::new();
        let it = yaml(
            "graphics:\n  textures:\n    normal: item/modern_bow\n    pulling_0: item/modern_bow_0\n    pulling_1: item/modern_bow_1\n    pulling_2: item/modern_bow_2\n",
        );
        let look = build("test", "modern_bow", &it, "bow", false, None, &mut files).unwrap();
        let used = models_in(&look.tree);
        for m in [
            "test:item/modern_bow",
            "test:item/modern_bow_pulling_0",
            "test:item/modern_bow_pulling_2",
        ] {
            assert!(used.iter().any(|u| u == m), "{m} in {used:?}");
        }
        assert!(look.tree.compact().contains("minecraft:use_duration"));
        let m0 =
            String::from_utf8_lossy(&files["assets/test/models/item/modern_bow_pulling_0.json"])
                .into_owned();
        assert!(
            m0.contains("minecraft:item/bow_pulling_0") && m0.contains("test:item/modern_bow_0"),
            "{m0}"
        );
    }

    #[test]
    fn texture_icon_color_block_and_parent() {
        let mut files = BTreeMap::new();
        let it = yaml(
            "graphics:\n  texture: minecraft:item/quartz\n  icon: item/x_icon\n  hand_animation_on_swap: false\n",
        );
        let look = build("t", "x", &it, "paper", false, Some(0xFF0000), &mut files).unwrap();
        let j = look.tree.compact();
        assert!(
            j.contains("minecraft:display_context") && j.contains("t:item/x_icon"),
            "{j}"
        );
        assert!(j.contains("minecraft:dye"));
        assert_eq!(look.top[0].0, "hand_animation_on_swap");
        assert!(
            String::from_utf8_lossy(&files["assets/t/models/item/x.json"])
                .contains("minecraft:item/quartz")
        );

        let it = yaml(
            "graphics:\n  textures:\n    north: block/n\n    south: block/s\n    east: block/e\n    west: block/w\n    up: block/u\n    down: block/d\n",
        );
        build("t", "b", &it, "paper", true, None, &mut files).unwrap();
        let b = String::from_utf8_lossy(&files["assets/t/models/block/b.json"]).into_owned();
        assert!(
            b.contains("minecraft:block/cube") && b.contains("t:block/u"),
            "{b}"
        );

        let it = yaml("graphics:\n  parent: block/cross\n  texture: block/flower\n");
        build("t", "f", &it, "paper", false, None, &mut files).unwrap();
        let f = String::from_utf8_lossy(&files["assets/t/models/item/f.json"]).into_owned();
        assert!(f.contains(r#""cross":"t:block/flower""#), "{f}");
    }

    #[test]
    fn shield_trident_and_legacy_suffixes() {
        let mut files = BTreeMap::new();
        let it = yaml("graphics:\n  models:\n    normal: item/s\n    blocking: item/s2\n");
        let look = build("t", "s", &it, "shield", false, None, &mut files).unwrap();
        assert!(look.tree.compact().contains("minecraft:using_item"));
        files.insert("assets/t/textures/item/rod_cast.png".into(), vec![]);
        let it = yaml(
            "resource:\n  material: FISHING_ROD\n  generate: true\n  textures:\n    - item/rod.png\n",
        );
        let look = build("t", "rod", &it, "fishing_rod", false, None, &mut files).unwrap();
        assert!(
            models_in(&look.tree).iter().any(|m| m == "t:item/rod_cast"),
            "{}",
            look.tree.compact()
        );
    }
}
