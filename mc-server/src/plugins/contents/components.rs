//! ItemsAdder 아이템 설정 → 바닐라 데이터 컴포넌트 (네트워크 인코딩).
//!
//! 형식은 minecraft-data `pc/26.1`의 `SlotComponent`와 ViaVersion 26.1→26.3 변환기로 맞췄습니다
//! (26.3에서 바뀐 것은 `consumable`의 효과 종류뿐이고, 여기서는 효과를 넣지 않음).
//! 형식이 틀리면 클라이언트가 접속을 끊으므로, 확실한 것만 만듭니다.

use crate::data::{SYNCED_REGISTRIES, component_id};
use crate::json::Json;
use crate::protocol::nbt::Tag;
use crate::protocol::types::{write_bool, write_f32, write_string};
use crate::protocol::varint::write_varint;

use super::text_format;

/// 아이템 하나에서 만든 것.
#[derive(Debug, Default)]
pub struct Built {
    pub components: Vec<(i32, Vec<u8>)>,
    /// 내구도가 있으면 바닐라 규칙상 한 칸에 하나만.
    pub force_stack_one: bool,
    pub warnings: Vec<String>,
    /// 서버 계산용: 손에 들었을 때 공격력 (attribute_modifiers.mainhand.attackDamage).
    pub attack_damage: Option<f32>,
    /// 서버 계산용: 먹으면 (배고픔, 포화).
    pub food: Option<(i32, f32)>,
}

impl Built {
    fn push(&mut self, name: &str, bytes: Vec<u8>) {
        match component_id(name) {
            Some(id) => {
                self.components.retain(|(k, _)| *k != id);
                self.components.push((id, bytes));
            }
            None => self.warnings.push(format!("26.3에 없는 컴포넌트: {name}")),
        }
    }

    pub fn has(&self, name: &str) -> bool {
        component_id(name).is_some_and(|id| self.components.iter().any(|(k, _)| *k == id))
    }
}

fn nbt(t: &Tag) -> Vec<u8> {
    let mut b = Vec::new();
    t.write_root(&mut b);
    b
}

/// 동기화 레지스트리 안의 번호 (없으면 None).
pub fn registry_index(registry: &str, entry: &str) -> Option<i32> {
    let entry = entry.strip_prefix("minecraft:").unwrap_or(entry);
    SYNCED_REGISTRIES
        .iter()
        .find(|(n, _)| *n == registry)
        .and_then(|(_, list)| list.iter().position(|e| *e == entry))
        .map(|i| i as i32)
}

/// Bukkit 옛 마법 부여 이름 → 바닐라 이름.
fn enchant_key(name: &str) -> String {
    let n = name.trim().to_ascii_lowercase();
    let n = n.strip_prefix("minecraft:").unwrap_or(&n);
    match n {
        "damage_all" => "sharpness",
        "damage_undead" => "smite",
        "damage_arthropods" => "bane_of_arthropods",
        "arrow_fire" => "flame",
        "arrow_damage" => "power",
        "arrow_knockback" => "punch",
        "arrow_infinite" => "infinity",
        "durability" => "unbreaking",
        "dig_speed" => "efficiency",
        "loot_bonus_blocks" => "fortune",
        "loot_bonus_mobs" => "looting",
        "luck" => "luck_of_the_sea",
        "oxygen" => "respiration",
        "water_worker" => "aqua_affinity",
        "protection_environmental" => "protection",
        "protection_fire" => "fire_protection",
        "protection_fall" => "feather_falling",
        "protection_explosions" => "blast_protection",
        "protection_projectile" => "projectile_protection",
        "sweeping_edge" | "sweeping" => "sweeping_edge",
        other => other,
    }
    .to_owned()
}

/// Bukkit/ItemsAdder 속성 이름 → (레지스트리 번호). 번호는 ViaVersion mapping-26.3 `attributes`.
fn attribute_id(name: &str) -> Option<i32> {
    let n = name.trim();
    let key = match n {
        "attackDamage" => "attack_damage",
        "attackSpeed" => "attack_speed",
        "maxHealth" => "max_health",
        "movementSpeed" => "movement_speed",
        "armorToughness" => "armor_toughness",
        "attackKnockback" => "attack_knockback",
        "knockbackResistance" => "knockback_resistance",
        other => other,
    };
    let key = key.strip_prefix("minecraft:").unwrap_or(key);
    let key = key.strip_prefix("generic.").unwrap_or(key);
    const ATTRIBUTES: [&str; 40] = [
        "air_drag_modifier",
        "armor",
        "armor_toughness",
        "attack_damage",
        "attack_knockback",
        "attack_speed",
        "below_name_distance",
        "block_break_speed",
        "block_interaction_range",
        "bounciness",
        "burning_time",
        "camera_distance",
        "explosion_knockback_resistance",
        "entity_interaction_range",
        "fall_damage_multiplier",
        "flying_speed",
        "follow_range",
        "friction_modifier",
        "gravity",
        "jump_strength",
        "knockback_resistance",
        "luck",
        "max_absorption",
        "max_health",
        "mining_efficiency",
        "movement_efficiency",
        "movement_speed",
        "name_tag_distance",
        "oxygen_bonus",
        "safe_fall_distance",
        "scale",
        "sneaking_speed",
        "spawn_reinforcements",
        "step_height",
        "submerged_mining_speed",
        "sweeping_damage_ratio",
        "tempt_range",
        "water_movement_efficiency",
        "waypoint_transmit_range",
        "waypoint_receive_range",
    ];
    ATTRIBUTES.iter().position(|a| *a == key).map(|i| i as i32)
}

/// 장비 칸 묶음 (`EquipmentSlotGroup` 번호).
fn slot_group(name: &str) -> Option<i32> {
    Some(match name.to_ascii_lowercase().as_str() {
        "any" => 0,
        "mainhand" | "main_hand" | "hand_main" => 1,
        "offhand" | "off_hand" => 2,
        "hand" => 3,
        "feet" | "boots" => 4,
        "legs" | "leggings" => 5,
        "chest" | "chestplate" => 6,
        "head" | "helmet" => 7,
        "armor" => 8,
        "body" => 9,
        _ => return None,
    })
}

/// `equippable`의 칸 번호.
fn equip_slot(name: &str) -> Option<i32> {
    Some(match name.to_ascii_lowercase().as_str() {
        "mainhand" | "main_hand" => 0,
        "offhand" | "off_hand" => 1,
        "feet" => 2,
        "legs" => 3,
        "chest" => 4,
        "head" => 5,
        "body" => 6,
        "saddle" => 7,
        _ => return None,
    })
}

/// 바탕 재료로 정해지는 갑옷 칸.
pub fn armor_slot_of(material: &str) -> Option<&'static str> {
    [
        ("_helmet", "head"),
        ("_chestplate", "chest"),
        ("_leggings", "legs"),
        ("_boots", "feet"),
    ]
    .into_iter()
    .find(|(s, _)| material.ends_with(s))
    .map(|(_, slot)| slot)
}

/// 소리: 인라인 소리 사건 (0 = 레지스트리 참조 아님, 이름, 거리 없음).
fn sound(out: &mut Vec<u8>, name: &str) {
    write_varint(out, 0);
    let n = if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    };
    write_string(out, &n);
    write_bool(out, false);
}

/// 색: `RED`, `#FF0000`, `FF0000`, `13083194`, `255,0,0`.
pub fn color(j: &Json) -> Option<i32> {
    if let Some(n) = j.as_f64() {
        return Some(n as i32 & 0xFF_FFFF);
    }
    let s = j.as_str()?.trim();
    if let Some((r, rest)) = s.split_once(',') {
        let mut it = std::iter::once(r)
            .chain(rest.split(','))
            .map(|x| x.trim().parse::<i32>());
        let (r, g, b) = (it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?);
        return Some((r.clamp(0, 255) << 16) | (g.clamp(0, 255) << 8) | b.clamp(0, 255));
    }
    let h = s.trim_start_matches('#');
    if h.len() == 6
        && let Ok(v) = i32::from_str_radix(h, 16)
    {
        return Some(v);
    }
    if let Ok(v) = s.parse::<i32>() {
        return Some(v & 0xFF_FFFF);
    }
    // Bukkit Color 이름
    Some(match s.to_ascii_uppercase().as_str() {
        "WHITE" => 0xFFFFFF,
        "SILVER" => 0xC0C0C0,
        "GRAY" => 0x808080,
        "BLACK" => 0x000000,
        "RED" => 0xFF0000,
        "MAROON" => 0x800000,
        "YELLOW" => 0xFFFF00,
        "OLIVE" => 0x808000,
        "LIME" => 0x00FF00,
        "GREEN" => 0x008000,
        "AQUA" => 0x00FFFF,
        "TEAL" => 0x008080,
        "BLUE" => 0x0000FF,
        "NAVY" => 0x000080,
        "FUCHSIA" => 0xFF00FF,
        "PURPLE" => 0x800080,
        "ORANGE" => 0xFFA500,
        _ => return None,
    })
}

/// `item_flags` → `tooltip_display`에서 숨길 컴포넌트들.
fn hidden_for_flag(flag: &str) -> &'static [&'static str] {
    match flag.to_ascii_uppercase().as_str() {
        "HIDE_ATTRIBUTES" => &["attribute_modifiers"],
        "HIDE_ENCHANTS" => &["enchantments"],
        "HIDE_UNBREAKABLE" => &["unbreakable"],
        "HIDE_DESTROYS" => &["can_break"],
        "HIDE_PLACED_ON" => &["can_place_on"],
        "HIDE_POTION_EFFECTS" => &["potion_contents"],
        "HIDE_DYE" => &["dyed_color"],
        "HIDE_ARMOR_TRIM" => &["trim"],
        "HIDE_STORED_ENCHANTS" => &["stored_enchantments"],
        "HIDE_ADDITIONAL_TOOLTIP" => &[
            "potion_contents",
            "jukebox_playable",
            "fireworks",
            "firework_explosion",
            "written_book_content",
            "bundle_contents",
            "container",
        ],
        _ => &[],
    }
}

/// `items.<id>` 설정에서 컴포넌트들을 만듭니다. `material`은 바탕 아이템 이름, `full`은 `ns:id`.
pub fn build(it: &Json, material: &str, full: &str) -> Built {
    let mut b = Built::default();

    // 마법 부여: ["SHARPNESS:3", "ns:key:2"] 또는 {SHARPNESS: 3}
    let mut ench: Vec<(i32, i32)> = Vec::new();
    let mut add_ench = |b: &mut Built, name: &str, level: i32| match registry_index(
        "enchantment",
        &enchant_key(name),
    ) {
        Some(id) => ench.push((id, level.max(1))),
        None => b.warnings.push(format!("모르는 마법 부여: {name}")),
    };
    match it.get("enchants") {
        Some(Json::Array(list)) => {
            for e in list {
                let Some(s) = e.as_str() else { continue };
                let (name, level) = s.rsplit_once(':').unwrap_or((s, "1"));
                add_ench(&mut b, name, level.trim().parse().unwrap_or(1));
            }
        }
        Some(Json::Object(map)) => {
            for (name, level) in map {
                add_ench(&mut b, name, level.as_f64().unwrap_or(1.0) as i32);
            }
        }
        _ => {}
    }
    if !ench.is_empty() {
        let mut out = Vec::new();
        write_varint(&mut out, ench.len() as i32);
        for (id, level) in ench {
            write_varint(&mut out, id);
            write_varint(&mut out, level);
        }
        b.push("enchantments", out);
    }

    // 속성: attribute_modifiers.<칸>.<속성>, equipment.slot_attribute_modifiers.<속성>
    let mut mods: Vec<(i32, String, f64, i32)> = Vec::new();
    if let Some(Json::Object(slots)) = it.get("attribute_modifiers") {
        for (slot, attrs) in slots {
            let Some(group) = slot_group(slot) else {
                b.warnings
                    .push(format!("모르는 칸: attribute_modifiers.{slot}"));
                continue;
            };
            if let Json::Object(attrs) = attrs {
                for (attr, v) in attrs {
                    let Some(amount) = v.as_f64() else { continue };
                    if group == 1 && attr == "attackDamage" {
                        b.attack_damage = Some(amount as f32);
                    }
                    match attribute_id(attr) {
                        Some(id) => mods.push((id, format!("{full}_{slot}_{attr}"), amount, group)),
                        None => b.warnings.push(format!("모르는 속성: {attr}")),
                    }
                }
            }
        }
    }
    if let Some(Json::Object(attrs)) = it
        .get("equipment")
        .and_then(|e| e.get("slot_attribute_modifiers"))
    {
        let slot = it
            .get("equipment")
            .and_then(|e| e.get("slot"))
            .and_then(Json::as_str)
            .map(str::to_ascii_lowercase)
            .or_else(|| armor_slot_of(material).map(str::to_owned))
            .unwrap_or_else(|| "head".into());
        let group = slot_group(&slot).unwrap_or(0);
        for (attr, v) in attrs {
            if let (Some(id), Some(amount)) = (attribute_id(attr), v.as_f64()) {
                mods.push((id, format!("{full}_equipment_{attr}"), amount, group));
            }
        }
    }
    if !mods.is_empty() {
        let mut out = Vec::new();
        write_varint(&mut out, mods.len() as i32);
        for (attr, id, amount, group) in mods {
            write_varint(&mut out, attr);
            let key = id.to_ascii_lowercase().replace(
                |c: char| !(c.is_ascii_alphanumeric() || "_-./:".contains(c)),
                "_",
            );
            write_string(&mut out, &key);
            out.extend_from_slice(&amount.to_be_bytes());
            write_varint(&mut out, 0); // add_value
            write_varint(&mut out, group);
            write_varint(&mut out, 0); // 기본 표시
        }
        b.push("attribute_modifiers", out);
    }

    // 내구도
    if let Some(d) = it.get("durability") {
        let num = |k: &str| d.get(k).and_then(Json::as_f64);
        if d.get("unbreakable").and_then(Json::as_bool) == Some(true) {
            b.push("unbreakable", Vec::new());
        }
        if let Some(max) = num("max_durability").or_else(|| num("max_custom_durability")) {
            let max = (max as i32).max(1);
            let mut out = Vec::new();
            write_varint(&mut out, max);
            b.push("max_damage", out);
            b.force_stack_one = true;
            if let Some(now) = num("durability").or_else(|| num("custom_durability")) {
                let mut out = Vec::new();
                write_varint(&mut out, (max - now as i32).clamp(0, max - 1));
                b.push("damage", out);
            }
        }
        if d.get("usages").is_some() {
            b.warnings
                .push("durability.usages는 아직 세지 않습니다".into());
        }
    }

    // 툴팁에서 숨김
    let mut hidden: Vec<i32> = Vec::new();
    for f in it
        .get("item_flags")
        .and_then(Json::as_array)
        .unwrap_or_default()
    {
        let Some(name) = f.as_str() else { continue };
        let list = hidden_for_flag(name);
        if list.is_empty() {
            b.warnings.push(format!("모르는 item_flags: {name}"));
        }
        hidden.extend(list.iter().filter_map(|c| component_id(c)));
    }
    if !hidden.is_empty() {
        hidden.sort_unstable();
        hidden.dedup();
        let mut out = Vec::new();
        write_bool(&mut out, false);
        write_varint(&mut out, hidden.len() as i32);
        for h in hidden {
            write_varint(&mut out, h);
        }
        b.push("tooltip_display", out);
    }

    if let Some(style) = it.get("tooltip_style").and_then(Json::as_str) {
        let mut out = Vec::new();
        write_string(&mut out, style);
        b.push("tooltip_style", out);
    }

    // 먹는 것
    if let Some(c) = it.get("consumable") {
        let nutrition = c.get("nutrition").and_then(Json::as_f64).unwrap_or(0.0) as i32;
        let saturation = c.get("saturation").and_then(Json::as_f64).unwrap_or(0.0) as f32;
        let mut out = Vec::new();
        write_varint(&mut out, nutrition.max(0));
        write_f32(&mut out, saturation);
        write_bool(
            &mut out,
            c.get("can_always_eat")
                .and_then(Json::as_bool)
                .unwrap_or(false),
        );
        b.push("food", out);
        b.food = Some((nutrition, saturation));
        let drink = c.get("animation").and_then(Json::as_str) == Some("drink");
        let mut out = Vec::new();
        write_f32(
            &mut out,
            c.get("consume_seconds")
                .and_then(Json::as_f64)
                .unwrap_or(1.6) as f32,
        );
        write_varint(&mut out, if drink { 2 } else { 1 });
        sound(
            &mut out,
            if drink {
                "entity.generic.drink"
            } else {
                "entity.generic.eat"
            },
        );
        write_bool(&mut out, true);
        write_varint(&mut out, 0);
        b.push("consumable", out);
    }

    // 색 (graphics.color, specific_properties.*.color)
    let tint = it
        .get("graphics")
        .and_then(|g| g.get("color"))
        .or_else(|| {
            it.get("specific_properties").and_then(|p| {
                p.get("leather_horse_armor")
                    .or_else(|| p.get("potion"))
                    .or_else(|| p.get("armor"))
                    .and_then(|x| x.get("color"))
            })
        })
        .and_then(color);
    if let Some(c) = tint {
        let mut out = Vec::new();
        out.extend_from_slice(&c.to_be_bytes());
        b.push("dyed_color", out);
    }

    // 장비 (equipment.id → equippable.asset_id)
    if let Some(e) = it.get("equipment") {
        let slot = e
            .get("slot")
            .and_then(Json::as_str)
            .map(str::to_ascii_lowercase)
            .or_else(|| armor_slot_of(material).map(str::to_owned))
            .unwrap_or_else(|| "head".into());
        let flag = |k: &str, d: bool| e.get(k).and_then(Json::as_bool).unwrap_or(d);
        let mut out = Vec::new();
        write_varint(&mut out, equip_slot(&slot).unwrap_or(5));
        sound(
            &mut out,
            e.get("sound")
                .and_then(Json::as_str)
                .unwrap_or("item.armor.equip_generic"),
        );
        match e.get("id").and_then(Json::as_str) {
            Some(id) => {
                write_bool(&mut out, true);
                write_string(&mut out, id);
            }
            None => write_bool(&mut out, false),
        }
        write_bool(&mut out, false); // 카메라 덮개 없음
        write_bool(&mut out, false); // 모든 엔티티
        write_bool(&mut out, flag("dispensable", true));
        write_bool(&mut out, flag("swappable", true));
        write_bool(&mut out, flag("damage_on_hurt", true));
        write_bool(&mut out, false); // 우클릭으로 입기 없음
        write_bool(&mut out, false); // 가위로 벗기기 없음
        sound(&mut out, "item.shears.snip");
        b.push("equippable", out);
        if flag("glider", false) {
            b.push("glider", Vec::new());
        }
    }

    // 직접 넣는 컴포넌트 (components_nbt_file의 JSON, 또는 nbt의 {components:{...}})
    if let Some(Json::Object(list)) = it.get("components") {
        for (name, v) in list {
            raw_component(&mut b, name, v);
        }
    }
    b
}

/// `{"minecraft:max_damage": 440, ...}`의 한 칸. 아는 것만 인코딩하고 나머지는 알립니다.
pub fn raw_component(b: &mut Built, name: &str, v: &Json) {
    let key = name.strip_prefix("minecraft:").unwrap_or(name);
    let int = |b: &mut Built, k: &str| {
        if let Some(n) = v.as_f64() {
            let mut out = Vec::new();
            write_varint(&mut out, n as i32);
            b.push(k, out);
        }
    };
    match key {
        "max_damage" => {
            int(b, key);
            b.force_stack_one = true;
        }
        "damage" | "max_stack_size" | "repair_cost" => int(b, key),
        "unbreakable" | "glider" => b.push(key, Vec::new()),
        "custom_name" | "item_name" => b.push(key, nbt(&text_format::json_to_tag(v))),
        "lore" => {
            let lines = v.as_array().unwrap_or_default();
            let mut out = Vec::new();
            write_varint(&mut out, lines.len() as i32);
            for l in lines {
                out.extend(nbt(&text_format::json_to_tag(l)));
            }
            b.push(key, out);
        }
        "item_model" | "tooltip_style" => {
            if let Some(s) = v.as_str() {
                let mut out = Vec::new();
                write_string(&mut out, s);
                b.push(key, out);
            }
        }
        "enchantment_glint_override" => {
            if let Some(x) = v.as_bool() {
                b.push(key, vec![x as u8]);
            }
        }
        "dyed_color" => {
            if let Some(c) = color(v).or_else(|| v.get("rgb").and_then(color)) {
                b.push(key, c.to_be_bytes().to_vec());
            }
        }
        "rarity" => {
            let r = match v.as_str().unwrap_or("").to_ascii_lowercase().as_str() {
                "uncommon" => 1,
                "rare" => 2,
                "epic" => 3,
                _ => 0,
            };
            b.push(key, vec![r]);
        }
        "attribute_modifiers" => {
            let list = v
                .as_array()
                .or_else(|| v.get("modifiers").and_then(Json::as_array))
                .unwrap_or_default();
            let mut out = Vec::new();
            let mut n = 0;
            let mut body = Vec::new();
            for m in list {
                let (Some(attr), Some(amount)) = (
                    m.get("type").and_then(Json::as_str).and_then(attribute_id),
                    m.get("amount").and_then(Json::as_f64),
                ) else {
                    continue;
                };
                write_varint(&mut body, attr);
                write_string(
                    &mut body,
                    m.get("id")
                        .and_then(Json::as_str)
                        .unwrap_or("dangu:modifier"),
                );
                body.extend_from_slice(&amount.to_be_bytes());
                let op = match m
                    .get("operation")
                    .and_then(Json::as_str)
                    .unwrap_or("add_value")
                {
                    "add_multiplied_base" => 1,
                    "add_multiplied_total" => 2,
                    _ => 0,
                };
                write_varint(&mut body, op);
                write_varint(
                    &mut body,
                    m.get("slot")
                        .and_then(Json::as_str)
                        .and_then(slot_group)
                        .unwrap_or(0),
                );
                write_varint(&mut body, 0);
                n += 1;
            }
            write_varint(&mut out, n);
            out.extend(body);
            b.push(key, out);
        }
        "enchantments" => {
            let map = v.get("levels").unwrap_or(v);
            if let Json::Object(m) = map {
                let mut out = Vec::new();
                let list: Vec<(i32, i32)> = m
                    .iter()
                    .filter_map(|(k, l)| {
                        Some((registry_index("enchantment", k)?, l.as_f64()? as i32))
                    })
                    .collect();
                write_varint(&mut out, list.len() as i32);
                for (id, l) in list {
                    write_varint(&mut out, id);
                    write_varint(&mut out, l);
                }
                b.push(key, out);
            }
        }
        "food" => {
            let mut out = Vec::new();
            write_varint(
                &mut out,
                v.get("nutrition").and_then(Json::as_f64).unwrap_or(0.0) as i32,
            );
            write_f32(
                &mut out,
                v.get("saturation").and_then(Json::as_f64).unwrap_or(0.0) as f32,
            );
            write_bool(
                &mut out,
                v.get("can_always_eat")
                    .and_then(Json::as_bool)
                    .unwrap_or(false),
            );
            b.push(key, out);
        }
        "custom_data" => b.warnings.push("custom_data는 서버가 쓰므로 건너뜀".into()),
        other => b
            .warnings
            .push(format!("아직 넣지 못하는 컴포넌트: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(s: &str) -> Json {
        crate::yaml::parse(s).unwrap()
    }

    #[test]
    fn builds_known_components() {
        let it = yaml(
            "enchants:\n  - DAMAGE_ALL:3\n  - minecraft:unbreaking:2\n  - nope:1\nattribute_modifiers:\n  mainhand:\n    attackDamage: 9\n    attackSpeed: -2.4\ndurability:\n  max_durability: 200\n  durability: 150\n  unbreakable: true\nitem_flags:\n  - HIDE_ENCHANTS\n  - HIDE_ATTRIBUTES\ntooltip_style: test:tooltip/rainbow\nconsumable:\n  nutrition: 7\n  saturation: 4\ngraphics:\n  color: 255,0,0\nequipment:\n  id: tut:my_armor\n",
        );
        let b = build(&it, "iron_chestplate", "tut:x");
        for c in [
            "enchantments",
            "attribute_modifiers",
            "max_damage",
            "damage",
            "unbreakable",
            "tooltip_display",
            "tooltip_style",
            "food",
            "consumable",
            "dyed_color",
            "equippable",
        ] {
            assert!(b.has(c), "{c}");
        }
        assert!(b.force_stack_one);
        assert_eq!(b.attack_damage, Some(9.0));
        assert_eq!(b.food, Some((7, 4.0)));
        assert!(b.warnings.iter().any(|w| w.contains("nope")));
        // 마법 부여: 2개, 날카로움 3
        let (_, e) = b
            .components
            .iter()
            .find(|(k, _)| Some(*k) == component_id("enchantments"))
            .unwrap();
        assert_eq!(e[0], 2);
        assert!(
            e[1..]
                .windows(2)
                .any(|w| w == [registry_index("enchantment", "sharpness").unwrap() as u8, 3])
        );
        // 피해 = 최대 - 지금
        let (_, d) = b
            .components
            .iter()
            .find(|(k, _)| Some(*k) == component_id("damage"))
            .unwrap();
        assert_eq!(d, &vec![50]);
        // 색
        let (_, c) = b
            .components
            .iter()
            .find(|(k, _)| Some(*k) == component_id("dyed_color"))
            .unwrap();
        assert_eq!(c, &0xFF0000i32.to_be_bytes().to_vec());
    }

    #[test]
    fn colors_and_names() {
        assert_eq!(color(&Json::String("RED".into())), Some(0xFF0000));
        assert_eq!(color(&Json::String("#326da8".into())), Some(0x326DA8));
        assert_eq!(color(&Json::Number(13083194.0)), Some(13083194));
        assert_eq!(enchant_key("ARROW_FIRE"), "flame");
        assert_eq!(attribute_id("movementSpeed"), Some(26));
        assert_eq!(attribute_id("generic.max_health"), Some(23));
        assert_eq!(armor_slot_of("diamond_leggings"), Some("legs"));
    }

    #[test]
    fn raw_components_from_json_file() {
        let j = crate::json::parse(r#"{"minecraft:max_damage":440,"minecraft:custom_name":{"text":"T","italic":false},"minecraft:attribute_modifiers":[{"id":"t","type":"minecraft:armor","amount":6,"operation":"add_value","slot":"chest"}],"minecraft:repairable":{"items":"minecraft:turtle_scute"}}"#).unwrap();
        let mut b = Built::default();
        if let Json::Object(f) = &j {
            for (k, v) in f {
                raw_component(&mut b, k, v);
            }
        }
        assert!(b.has("max_damage") && b.has("custom_name") && b.has("attribute_modifiers"));
        assert!(b.warnings.iter().any(|w| w.contains("repairable")));
    }
}
