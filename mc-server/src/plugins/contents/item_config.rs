//! 설정 파일의 아이템 하나(`items.<id>`)를 [`CustomItem`]으로 읽습니다.
//!
//! 이름·설명은 [`text_format`](글 형식), 그림은 [`graphics`], 그 밖의 성질은 [`components`]가
//! 맡고, 여기서는 그것들을 모아 아이템 하나로 만듭니다.

use std::collections::BTreeMap;

use crate::data::{component, items};
use crate::game::custom_items::CustomItem;
use crate::json::Json;
use crate::protocol::nbt::Tag;
use crate::protocol::varint::write_varint;

use super::load::*;
use super::{components, graphics, text_format};

/// 아이템 하나를 읽은 결과.
pub(super) struct ItemOut {
    pub def: CustomItem,
    /// 블록이면 (아이템 id, 블록 모델, 부수면 떨어뜨림).
    pub block: Option<(String, String, bool)>,
    pub warnings: Vec<String>,
    /// `/ia` 목록에서 숨김 (`show_in_ia_gui: false`).
    pub hidden: bool,
    /// `/ia`에서 보려면 필요한 권한 (`permission`/`permission_suffix`).
    pub permission: Option<String>,
}

fn nbt(t: &Tag) -> Vec<u8> {
    let mut b = Vec::new();
    t.write_root(&mut b);
    b
}

/// 바탕 재료: 새 방식 `material`, 옛 방식 `resource.material`, 옛 갑옷(재료 없이 칸만), 기본 종이.
fn material_of(it: &Json) -> String {
    let given = s(it, "material").or_else(|| it.get("resource").and_then(|r| s(r, "material")));
    if let Some(m) = given {
        let m = m.to_ascii_lowercase();
        return m.strip_prefix("minecraft:").unwrap_or(&m).to_owned();
    }
    let armor_slot = it
        .get("specific_properties")
        .and_then(|p| p.get("armor"))
        .and_then(|a| s(a, "slot"))
        .map(str::to_ascii_lowercase);
    match armor_slot.as_deref() {
        Some("head") => "leather_helmet".into(),
        Some("chest") => "leather_chestplate".into(),
        Some("legs") => "leather_leggings".into(),
        Some("feet") => "leather_boots".into(),
        _ => "paper".into(),
    }
}

/// 아이템 하나. `emoji`는 글 안의 `:이름:`을 그림 글자로 바꿉니다.
pub(super) fn read_item(
    ns: &str,
    id: &str,
    it: &Json,
    files: &mut BTreeMap<String, Vec<u8>>,
    cmd_models: &mut Vec<(String, i32, Json)>,
    emoji: &dyn Fn(&str) -> String,
) -> Result<ItemOut, String> {
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' || c == '.')
    {
        return Err("아이디는 소문자·숫자·_만 쓸 수 있습니다".into());
    }
    let full = format!("{ns}:{id}");
    let material = material_of(it);
    let base = items::by_name(&material)
        .filter(|&b| b > 0)
        .ok_or_else(|| format!("모르는 material {material}"))?;
    // 블록: 새 방식 behaviours.block, 옛 방식 specific_properties.block
    let block = it
        .get("behaviours")
        .and_then(|b| b.get("block"))
        .or_else(|| it.get("specific_properties").and_then(|p| p.get("block")));
    let is_block = block.is_some();

    let mut built = components::build(it, &material, &full);
    let tint = it
        .get("graphics")
        .and_then(|g| g.get("color"))
        .and_then(components::color);

    // 그림
    let look = graphics::build(ns, id, it, &material, is_block, tint, files);
    let model_id = it
        .get("resource")
        .and_then(|r| r.get("model_id"))
        .and_then(Json::as_f64)
        .map(|n| n as i32);
    let mut item_model = s(it, "item_model").map(str::to_owned);
    let mut block_model = None;
    if let Some(look) = &look {
        block_model = first_model(&look.tree);
        match model_id {
            // 옛 방식 custom_model_data: 바닐라 재료의 모양 정의가 이 번호로 고름
            Some(cmd) if item_model.is_none() => {
                cmd_models.push((material.clone(), cmd, look.tree.clone()));
            }
            _ if item_model.is_none() => {
                let mut top = vec![("model".to_owned(), look.tree.clone())];
                top.extend(look.top.iter().cloned());
                files.insert(
                    format!("assets/{ns}/items/{id}.json"),
                    Json::Object(top).compact().into_bytes(),
                );
                item_model = Some(full.clone());
            }
            _ => {}
        }
    }

    // 이름·설명 (`name`이 새 이름, `display_name`이 옛 이름)
    let raw_name = s(it, "name")
        .or_else(|| s(it, "display_name"))
        .map(emoji)
        .unwrap_or_else(|| id.to_owned());
    if !built.has("item_name") && !built.has("custom_name") {
        let tag = text_format::parse(&raw_name, None);
        built.components.push((component::ITEM_NAME, nbt(&tag)));
    }
    let lore: Vec<String> = strings(it.get("lore")).iter().map(|l| emoji(l)).collect();
    if !lore.is_empty() && !built.has("lore") {
        let mut b = Vec::new();
        write_varint(&mut b, lore.len() as i32);
        for l in &lore {
            b.extend(nbt(&text_format::parse(l, Some("gray"))));
        }
        built.components.push((component::LORE, b));
    }

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
    let max_stack = if built.force_stack_one {
        Some(1)
    } else {
        it.get("max_stack_size")
            .and_then(Json::as_f64)
            .map(|n| n as i32)
    };
    let def = CustomItem {
        id: full.clone(),
        base,
        name: text_format::plain(&raw_name),
        color: "white".into(),
        lore: lore.iter().map(|l| text_format::plain(l)).collect(),
        model: item_model,
        glint: it
            .get("glint")
            .or_else(|| it.get("enchant_glint"))
            .and_then(Json::as_bool)
            .unwrap_or(false),
        rarity,
        max_stack,
        damage: built.attack_damage,
        ability: None,
        cooldown: 0.0,
        mana_cost: 0.0,
        food: built.food,
        custom_model_data: model_id.map(|n| n as f32),
        extra: built.components,
    };
    let block_info = block.map(|b| {
        let drop = b
            .get("drop_when_mined")
            .and_then(Json::as_bool)
            .or_else(|| b.get("cancel_drop").and_then(Json::as_bool).map(|c| !c))
            .unwrap_or(true);
        let model = block_model
            .clone()
            .unwrap_or_else(|| "minecraft:block/note_block".into());
        (full.clone(), model, drop)
    });
    let mut warnings = built.warnings;
    for (key, why) in UNSUPPORTED {
        if it.get(key).is_some() {
            warnings.push(format!("{key}: {why}"));
        }
    }
    Ok(ItemOut {
        def,
        block: block_info,
        warnings,
        hidden: it.get("show_in_ia_gui").and_then(Json::as_bool) == Some(false),
        permission: s(it, "permission_suffix")
            .or_else(|| s(it, "permission"))
            .map(|p| format!("ia.user.ia.seeitem.{p}")),
    })
}

/// 읽기는 하지만 아직 동작하지 않는 키 (경고로 알림).
const UNSUPPORTED: &[(&str, &str)] = &[
    ("events", "이벤트·동작(actions)은 아직 실행하지 않습니다"),
    ("events_settings", "이벤트 재사용 대기는 아직"),
    ("elytra", "사용자 정의 겉날개는 아직"),
    ("jukebox_disc", "음반은 아직"),
    ("drop", "떨어진 아이템 빛남·이름은 아직"),
    ("blocked_enchants", "마법 부여 막기는 아직"),
    (
        "nbt",
        "옛 NBT 문자열은 읽지 않습니다 (components_nbt_file을 쓰세요)",
    ),
    ("script", "스크립트는 지원하지 않습니다"),
];

/// 모양 나무에서 처음 나오는 모델 이름 (창 그림 선택이면 기본 쪽부터).
fn first_model(j: &Json) -> Option<String> {
    match j {
        Json::Object(f) => {
            if let Some(m) = f
                .iter()
                .find(|(k, _)| k == "model")
                .and_then(|(_, v)| v.as_str())
            {
                return Some(m.to_owned());
            }
            f.iter()
                .find(|(k, _)| k == "fallback")
                .and_then(|(_, v)| first_model(v))
                .or_else(|| f.iter().find_map(|(_, v)| first_model(v)))
        }
        Json::Array(a) => a.iter().find_map(first_model),
        _ => None,
    }
}
