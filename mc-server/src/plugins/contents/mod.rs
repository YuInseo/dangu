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

use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod builtin;
mod cache;
pub(crate) mod catalog;
mod components;
mod graphics;
mod item_config;
mod load;
pub mod source;
pub mod text_format;

pub use load::load;

use std::sync::{Arc, RwLock};

use crate::game::custom_items::{self};

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
    /// 보려면 필요한 권한 (`ia.menu.seecategory.<id>` 등).
    pub permission: Option<String>,
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
    /// 읽었지만 아직 동작하지 않는 키 등 (켤 때 알림).
    pub warnings: Vec<String>,
    /// `/ia` 목록에서 숨긴 아이템.
    pub hidden: Vec<String>,
    /// 아이템 → `/ia`에서 보려면 필요한 권한.
    pub item_permissions: HashMap<String, String>,
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
    // 장비: 겉모습·보스 훅, 광석 블록 상태를 월드 생성에
    crate::plugins::equip::after_contents();
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
pub const PACK_RELOAD: &str = "hoya:pack_reload";

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
    v.extend(l.warnings.iter().map(|w| format!("알림: {w}")));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::blocks;

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
        let dir = std::env::temp_dir().join(format!("hoya-contents-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        sample(&dir);
        let l = load(&dir);
        let mine: Vec<&String> = l.items.iter().filter(|i| !i.starts_with("hoya:")).collect();
        assert_eq!(mine, vec!["myitems:ruby", "myitems:ruby_block"]);
        assert!(
            l.items.contains(&"hoya:lottery_ticket".to_owned()),
            "built-in pack"
        );
        assert_eq!(l.errors.len(), 1, "{:?}", l.errors);
        assert!(l.errors[0].contains("not_a_thing"), "{:?}", l.errors);
        let ruby = custom_items::get(custom_items::index_of("myitems:ruby").unwrap()).unwrap();
        assert_eq!(ruby.model.as_deref(), Some("myitems:ruby"));
        assert_eq!(ruby.damage, Some(7.0));
        // 이름은 꾸밈 없는 글(목록용)과, 색이 든 진짜 이름 컴포넌트
        assert!(ruby.name.starts_with("루비 "), "{}", ruby.name);
        let (_, name) = ruby
            .extra
            .iter()
            .find(|(k, _)| *k == crate::data::component::ITEM_NAME)
            .unwrap();
        assert!(String::from_utf8_lossy(name).contains("red"));
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
