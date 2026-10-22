//! 설정 파일의 아이템 하나(`items.<id>`)를 [`CustomItem`]으로 읽습니다.

use std::collections::BTreeMap;

use crate::data::items;
use crate::game::custom_items::CustomItem;
use crate::json::Json;
use crate::text::legacy_colors;

use super::load::*;

/// 아이템 하나. 블록이면 (아이템 id, 블록 모델, 부수면 떨어뜨림)도.
#[allow(clippy::type_complexity)]
pub(super) fn read_item(
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
