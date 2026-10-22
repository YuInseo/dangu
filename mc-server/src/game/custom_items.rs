//! 우리만의 아이템. 리소스 팩 없이 바닐라 클라이언트에 보이는 방법은 "아이템 컴포넌트"입니다.
//!
//! - 이름(`item_name`, 색 포함), 설명(`lore`), 반짝임(`enchantment_glint_override`), 등급 색
//!   (`rarity`), 겹치는 수(`max_stack_size`), 그리고 **모양(`item_model`)** — 바닐라의 어떤
//!   아이템·블록 모델이든 빌려 씁니다 (막대기 모양을 다이아몬드 검으로 등).
//! - 새 *그림*(텍스처)은 리소스 팩 없이는 만들 수 없습니다. 모양은 바닐라 것을 조합합니다.
//! - 능력(우클릭 번개 등)과 공격력은 서버가 처리하므로 클라이언트에 따로 줄 것이 없습니다.
//!
//! 어느 사용자 정의 아이템인지는 `custom_data {"dangu:item": "<id>"}`로 표시해 둡니다.
//! 정의는 `world/custom_items.json`에 있고, 없으면 예시를 써 둡니다.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, RwLock};

use crate::data::{component, items};
use crate::json::{self, Json};
use crate::protocol::nbt::Tag;
use crate::protocol::types::{write_bool, write_f32, write_string};
use crate::protocol::varint::write_varint;

/// `custom_data` 안의 표시 키.
pub const TAG_KEY: &str = "dangu:item";
/// `custom_data` 안의 일련번호 키.
pub const SERIAL_KEY: &str = "dangu:serial";
const FILE_NAME: &str = "custom_items.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ability {
    /// 바라보는 곳에 번개 (주변에 피해).
    Lightning,
    /// 바라보는 쪽으로 순간 이동 (최대 10칸).
    Blink,
    /// 바라보는 쪽으로 높이 뛰기 (착지 피해 없음).
    Leap,
    /// 체력 8 회복.
    Heal,
}

impl Ability {
    fn parse(s: &str) -> Option<Ability> {
        Some(match s {
            "lightning" => Ability::Lightning,
            "blink" => Ability::Blink,
            "leap" => Ability::Leap,
            "heal" => Ability::Heal,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Ability::Lightning => "lightning",
            Ability::Blink => "blink",
            Ability::Leap => "leap",
            Ability::Heal => "heal",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CustomItem {
    /// 명령어와 저장에 쓰는 이름 (`thunder_staff`).
    pub id: String,
    /// 바탕이 되는 바닐라 아이템 ID.
    pub base: i32,
    pub name: String,
    /// `gold`, `#ff8800` 등.
    pub color: String,
    pub lore: Vec<String>,
    /// 빌려 쓸 모양 (`minecraft:diamond_sword`). 없으면 바탕 아이템 모양.
    pub model: Option<String>,
    pub glint: bool,
    /// 0 일반, 1 고급, 2 희귀, 3 영웅 (이름 기본 색).
    pub rarity: u8,
    pub max_stack: Option<i32>,
    /// 때릴 때의 피해 (없으면 바탕 아이템 그대로).
    pub damage: Option<f32>,
    pub ability: Option<Ability>,
    /// 능력 재사용 대기 (초).
    pub cooldown: f32,
    /// 능력을 쓸 때 드는 마나 ([`crate::plugins::rpg`]). 0이면 공짜.
    pub mana_cost: f32,
    /// 먹으면 (배고픔, 포화) — ItemsAdder `consumable`.
    pub food: Option<(i32, f32)>,
    /// 사용자 정의 모델 번호 (`custom_model_data`의 첫 실수). 바닐라 모양 정의가 이 번호로
    /// 우리 모델을 고릅니다 (ItemsAdder의 `model_id`).
    pub custom_model_data: Option<f32>,
    /// 그대로 붙일 다른 컴포넌트들 (종류, 인코딩된 값) — 내구도·속성·마법 부여 등.
    pub extra: Vec<(i32, Vec<u8>)>,
}

static REGISTRY: RwLock<Vec<CustomItem>> = RwLock::new(Vec::new());
/// 클라이언트가 보내는 `custom_data` 해시 → (사용자 정의 아이템, 일련번호). 해시 계산(CRC32C
/// 기반의 HashOps)을 따라 하지 않고, 어느 아이템인지 분명할 때 보고 배워 둡니다.
static LEARNED_HASHES: Mutex<Option<HashMap<i32, (u16, u32)>>> = Mutex::new(None);

/// 일련번호마다 다른 겉모습 (복권 한 장 한 장의 이름·설명).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Look {
    pub name: Option<(String, String)>,
    pub lore: Vec<String>,
    pub glint: Option<bool>,
}

static LOOKS: RwLock<Option<HashMap<u32, Look>>> = RwLock::new(None);

/// 이 일련번호의 아이템을 이렇게 보이게 합니다 (다음에 보내는 칸부터).
pub fn set_look(serial: u32, look: Look) {
    LOOKS
        .write()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(serial, look);
}

pub fn remove_look(serial: u32) {
    if let Some(m) = LOOKS.write().unwrap().as_mut() {
        m.remove(&serial);
    }
}

fn look(serial: u32) -> Option<Look> {
    LOOKS.read().unwrap().as_ref()?.get(&serial).cloned()
}

pub fn get(index: u16) -> Option<CustomItem> {
    REGISTRY.read().unwrap().get(index as usize).cloned()
}

pub fn index_of(id: &str) -> Option<u16> {
    // 예전 저장의 이름 (복권이 내장 콘텐츠로 옮기기 전)
    let id = match id {
        "lottery_ticket" => "dangu:lottery_ticket",
        "gui_icon" => "dangu:gui_icon",
        id => id,
    };
    REGISTRY
        .read()
        .unwrap()
        .iter()
        .position(|c| c.id == id)
        .map(|i| i as u16)
}

pub fn all() -> Vec<CustomItem> {
    REGISTRY.read().unwrap().clone()
}

pub fn learn_hash(hash: i32, index: u16, serial: u32) {
    LEARNED_HASHES
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(hash, (index, serial));
}

pub fn by_hash(hash: i32) -> Option<(u16, u32)> {
    LEARNED_HASHES.lock().unwrap().as_ref()?.get(&hash).copied()
}

/// 정의를 바꿉니다. 이미 있던 아이템은 같은 번호를 유지하고 (가진 아이템이 바뀌지 않도록),
/// 새 것은 뒤에 붙습니다.
pub fn install(defs: Vec<CustomItem>) {
    let mut reg = REGISTRY.write().unwrap();
    for d in defs {
        match reg.iter_mut().find(|c| c.id == d.id) {
            Some(existing) => *existing = d,
            None => reg.push(d),
        }
    }
}

fn text(s: &str, color: &str) -> Tag {
    Tag::Compound(vec![
        ("text".into(), Tag::String(s.to_owned())),
        ("color".into(), Tag::String(color.to_owned())),
        ("italic".into(), Tag::Byte(0)),
    ])
}

/// 이 아이템을 보낼 때 붙이는 (컴포넌트 종류, 내용) 목록.
pub fn components(index: u16) -> Vec<(i32, Vec<u8>)> {
    components_with(index, 0)
}

/// 일련번호가 있으면 `custom_data`에 번호를 넣고(클라이언트가 한 장씩 구별하도록), 그 번호의
/// [`Look`]으로 이름·설명·반짝임을 바꿉니다.
pub fn components_with(index: u16, serial: u32) -> Vec<(i32, Vec<u8>)> {
    let Some(mut c) = get(index) else {
        return Vec::new();
    };
    if serial != 0
        && let Some(l) = look(serial)
    {
        if let Some((name, color)) = l.name {
            c.name = name;
            c.color = color;
        }
        c.lore = l.lore;
        if let Some(g) = l.glint {
            c.glint = g;
        }
    }
    let mut out = Vec::new();
    // 설정에서 직접 만든 컴포넌트(이름·설명 등)가 있으면 기본값 대신 그것
    let own: Vec<i32> = c.extra.iter().map(|(k, _)| *k).collect();
    let nbt = |t: Tag| {
        let mut b = Vec::new();
        t.write_root(&mut b);
        b
    };
    let mut data = vec![(TAG_KEY.into(), Tag::String(c.id.clone()))];
    if serial != 0 {
        data.push((SERIAL_KEY.into(), Tag::Int(serial as i32)));
    }
    out.push((component::CUSTOM_DATA, nbt(Tag::Compound(data))));
    if !own.contains(&component::ITEM_NAME) || serial != 0 {
        out.push((component::ITEM_NAME, nbt(text(&c.name, &c.color))));
    }
    if !c.lore.is_empty() && (!own.contains(&component::LORE) || serial != 0) {
        let mut b = Vec::new();
        write_varint(&mut b, c.lore.len() as i32);
        for line in &c.lore {
            text(line, "gray").write_root(&mut b);
        }
        out.push((component::LORE, b));
    }
    if let Some(model) = &c.model {
        let mut b = Vec::new();
        write_string(&mut b, model);
        out.push((component::ITEM_MODEL, b));
    }
    if c.glint && !own.contains(&component::ENCHANTMENT_GLINT_OVERRIDE) {
        let mut b = Vec::new();
        write_bool(&mut b, true);
        out.push((component::ENCHANTMENT_GLINT_OVERRIDE, b));
    }
    if let (Some(cmd), Some(kind)) = (
        c.custom_model_data,
        crate::data::component_id("custom_model_data"),
    ) {
        // floats [cmd], flags [], strings [], colors []
        let mut b = Vec::new();
        write_varint(&mut b, 1);
        write_f32(&mut b, cmd);
        b.extend_from_slice(&[0, 0, 0]);
        out.push((kind, b));
    }
    let looked = serial != 0;
    out.extend(
        c.extra
            .iter()
            .filter(|(k, _)| !(looked && (*k == component::ITEM_NAME || *k == component::LORE)))
            .cloned(),
    );
    if !own.contains(&component::RARITY) {
        let mut b = Vec::new();
        write_varint(&mut b, c.rarity as i32);
        out.push((component::RARITY, b));
    }
    if let Some(n) = c
        .max_stack
        .filter(|_| !own.contains(&component::MAX_STACK_SIZE))
    {
        let mut b = Vec::new();
        write_varint(&mut b, n.clamp(1, 99));
        out.push((component::MAX_STACK_SIZE, b));
    }
    out
}

#[allow(dead_code)]
fn _food_component(nutrition: i32, saturation: f32) -> Vec<u8> {
    let mut b = Vec::new();
    write_varint(&mut b, nutrition);
    write_f32(&mut b, saturation);
    write_bool(&mut b, false);
    b
}

/// 먹을 수 있으면 (배고픔, 포화): 사용자 정의 아이템의 `consumable`, 아니면 바닐라.
pub fn food_of(item: &crate::packets::world::Item) -> Option<(i32, f32)> {
    match item.custom.and_then(get) {
        Some(c) if c.food.is_some() => c.food,
        _ => items::food(item.id),
    }
}

pub fn max_stack(item: &crate::packets::world::Item) -> i32 {
    item.custom
        .and_then(get)
        .and_then(|c| c.max_stack)
        .unwrap_or_else(|| items::max_stack(item.id))
}

/// 기본 예시 아이템들.
pub fn defaults() -> Vec<CustomItem> {
    let item = |name: &str| items::by_name(name).unwrap_or(1);
    vec![
        CustomItem {
            id: "thunder_staff".into(),
            base: item("blaze_rod"),
            name: "번개 지팡이".into(),
            color: "gold".into(),
            lore: vec!["우클릭: 바라보는 곳에 번개".into(), "재사용 2초".into()],
            model: None,
            glint: true,
            rarity: 3,
            max_stack: Some(1),
            damage: Some(4.0),
            ability: Some(Ability::Lightning),
            cooldown: 2.0,
            mana_cost: 20.0,
            food: None,
            custom_model_data: None,
            extra: Vec::new(),
        },
        CustomItem {
            id: "blink_wand".into(),
            base: item("stick"),
            name: "순간 이동 막대".into(),
            color: "light_purple".into(),
            lore: vec!["우클릭: 앞으로 10칸 순간 이동".into()],
            model: Some("minecraft:end_rod".into()),
            glint: true,
            rarity: 2,
            max_stack: Some(1),
            damage: None,
            ability: Some(Ability::Blink),
            cooldown: 1.0,
            mana_cost: 10.0,
            food: None,
            custom_model_data: None,
            extra: Vec::new(),
        },
        CustomItem {
            id: "leap_feather".into(),
            base: item("feather"),
            name: "도약의 깃털".into(),
            color: "aqua".into(),
            lore: vec!["우클릭: 높이 뛰기, 떨어져도 안 다침".into()],
            model: None,
            glint: false,
            rarity: 1,
            max_stack: Some(1),
            damage: None,
            ability: Some(Ability::Leap),
            cooldown: 1.5,
            mana_cost: 15.0,
            food: None,
            custom_model_data: None,
            extra: Vec::new(),
        },
        CustomItem {
            id: "healing_gem".into(),
            base: item("emerald"),
            name: "치유의 보석".into(),
            color: "green".into(),
            lore: vec!["우클릭: 체력 4칸 회복".into(), "재사용 10초".into()],
            model: Some("minecraft:heart_of_the_sea".into()),
            glint: true,
            rarity: 2,
            max_stack: Some(1),
            damage: None,
            ability: Some(Ability::Heal),
            cooldown: 10.0,
            mana_cost: 30.0,
            food: None,
            custom_model_data: None,
            extra: Vec::new(),
        },
        CustomItem {
            id: "hero_sword".into(),
            base: item("iron_sword"),
            name: "용사의 검".into(),
            color: "red".into(),
            lore: vec!["공격력 12".into()],
            model: Some("minecraft:netherite_sword".into()),
            glint: true,
            rarity: 3,
            max_stack: None,
            damage: Some(12.0),
            ability: None,
            cooldown: 0.0,
            mana_cost: 0.0,
            food: None,
            custom_model_data: None,
            extra: Vec::new(),
        },
    ]
}

// ---- 저장 파일 ----

/// 저장 파일의 아이템 꼬리: u8 길이 + 사용자 정의 id (0 = 바닐라). 번호가 아니라 이름을
/// 저장하므로 정의 순서가 바뀌어도 그대로입니다. 일련번호가 있으면 `id#번호`.
pub fn write_custom_tag(out: &mut Vec<u8>, custom: Option<u16>, serial: u32) {
    match custom.and_then(get) {
        Some(c) => {
            let text = if serial != 0 {
                format!("{}#{serial}", c.id)
            } else {
                c.id.clone()
            };
            let id = &text.as_bytes()[..text.len().min(255)];
            out.push(id.len() as u8);
            out.extend_from_slice(id);
        }
        None => out.push(0),
    }
}

/// 읽을 수 없으면 None, 바닐라거나 지금은 없는 정의면 Some((None, 0)).
pub fn read_custom_tag<'a>(
    take: &mut impl FnMut(usize) -> Option<&'a [u8]>,
) -> Option<(Option<u16>, u32)> {
    let len = take(1)?[0] as usize;
    if len == 0 {
        return Some((None, 0));
    }
    let text = String::from_utf8_lossy(take(len)?).into_owned();
    let (id, serial) = match text.split_once('#') {
        Some((id, n)) => (id.to_owned(), n.parse().unwrap_or(0)),
        None => (text, 0),
    };
    Some((index_of(&id), serial))
}

// ---- JSON 읽기·쓰기 ----

fn parse_def(v: &Json) -> Result<CustomItem, String> {
    let s = |k: &str| v.get(k).and_then(Json::as_str);
    let num = |k: &str| match v.get(k) {
        Some(Json::Number(n)) => Some(*n),
        _ => None,
    };
    let id = s("id").ok_or("id가 없습니다")?.to_owned();
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(format!("id '{id}'는 소문자·숫자·_만 쓸 수 있습니다"));
    }
    let base_name = s("base").ok_or_else(|| format!("{id}: base가 없습니다"))?;
    let base = items::by_name(base_name)
        .filter(|&b| b > 0)
        .ok_or_else(|| format!("{id}: 모르는 아이템 {base_name}"))?;
    let rarity = match s("rarity").unwrap_or("common") {
        "common" => 0,
        "uncommon" => 1,
        "rare" => 2,
        "epic" => 3,
        r => return Err(format!("{id}: 모르는 등급 {r}")),
    };
    let ability = match s("ability") {
        Some(a) => Some(Ability::parse(a).ok_or_else(|| format!("{id}: 모르는 능력 {a}"))?),
        None => None,
    };
    Ok(CustomItem {
        name: s("name").unwrap_or(&id).to_owned(),
        color: s("color").unwrap_or("white").to_owned(),
        lore: v
            .get("lore")
            .and_then(Json::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(|l| l.as_str().map(str::to_owned))
            .collect(),
        model: s("model").map(str::to_owned),
        glint: matches!(v.get("glint"), Some(Json::Bool(true))),
        rarity,
        max_stack: num("max_stack").map(|n| n as i32),
        damage: num("damage").map(|n| n as f32),
        ability,
        cooldown: num("cooldown").unwrap_or(0.0) as f32,
        mana_cost: num("mana_cost").or_else(|| num("mana")).unwrap_or(0.0) as f32,
        food: None,
        custom_model_data: num("custom_model_data").map(|n| n as f32),
        extra: Vec::new(),
        id,
        base,
    })
}

pub fn parse(text: &str) -> Result<Vec<CustomItem>, String> {
    let v = json::parse(text)?;
    v.get("items")
        .and_then(Json::as_array)
        .ok_or("\"items\" 배열이 없습니다")?
        .iter()
        .map(parse_def)
        .collect()
}

pub fn to_json(defs: &[CustomItem]) -> String {
    let rarity = ["common", "uncommon", "rare", "epic"];
    let items: Vec<Json> = defs
        .iter()
        .map(|d| {
            let mut f = vec![
                ("id".to_owned(), Json::String(d.id.clone())),
                (
                    "base".to_owned(),
                    Json::String(items::name(d.base).to_owned()),
                ),
                ("name".to_owned(), Json::String(d.name.clone())),
                ("color".to_owned(), Json::String(d.color.clone())),
                (
                    "lore".to_owned(),
                    Json::Array(d.lore.iter().cloned().map(Json::String).collect()),
                ),
                ("glint".to_owned(), Json::Bool(d.glint)),
                (
                    "rarity".to_owned(),
                    Json::String(rarity[d.rarity as usize % 4].to_owned()),
                ),
            ];
            if let Some(m) = &d.model {
                f.push(("model".into(), Json::String(m.clone())));
            }
            if let Some(n) = d.max_stack {
                f.push(("max_stack".into(), Json::Number(n as f64)));
            }
            if let Some(dmg) = d.damage {
                f.push(("damage".into(), Json::Number(dmg as f64)));
            }
            if let Some(a) = d.ability {
                f.push(("ability".into(), Json::String(a.name().into())));
                f.push(("cooldown".into(), Json::Number(d.cooldown as f64)));
                if d.mana_cost > 0.0 {
                    f.push(("mana_cost".into(), Json::Number(d.mana_cost as f64)));
                }
            }
            Json::Object(f)
        })
        .collect();
    Json::Object(vec![("items".into(), Json::Array(items))]).pretty()
}

/// 월드 폴더에서 읽습니다 (없으면 예시를 써 둠).
pub fn load(dir: Option<&Path>) -> Result<usize, String> {
    let defs = match dir {
        Some(dir) => {
            let path = dir.join(FILE_NAME);
            match std::fs::read_to_string(&path) {
                Ok(text) => parse(&text).map_err(|e| format!("{}: {e}", path.display()))?,
                Err(_) => {
                    let defs = defaults();
                    std::fs::create_dir_all(dir).ok();
                    std::fs::write(&path, to_json(&defs)).map_err(|e| e.to_string())?;
                    defs
                }
            }
        }
        None => defaults(),
    };
    let n = defs.len();
    install(defs);
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip() {
        let defs = defaults();
        assert_eq!(parse(&to_json(&defs)).unwrap(), defs);
        assert!(parse(r#"{"items":[{"id":"Bad Id","base":"stick"}]}"#).is_err());
        assert!(parse(r#"{"items":[{"id":"x","base":"no_such_item"}]}"#).is_err());
    }

    #[test]
    fn components_mark_the_item() {
        install(defaults());
        let i = index_of("thunder_staff").unwrap();
        let c = components(i);
        let (kind, data) = &c[0];
        assert_eq!(*kind, component::CUSTOM_DATA);
        assert_eq!(
            crate::protocol::nbt::find_string(data, TAG_KEY).as_deref(),
            Some("thunder_staff")
        );
        assert!(c.iter().any(|(k, _)| *k == component::ITEM_NAME));
    }
}
