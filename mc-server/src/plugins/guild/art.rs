//! 작물 성장 촉진제 아이템: 직접 찍은 도트와 ItemsAdder 모양 설정 (내장 콘텐츠 팩 `hoya`).

use crate::plugins::contents::source::Pack;
use crate::resource_pack::{Sprite, png};

/// 초록 물약병에 반짝임과 새싹.
pub const CROP_BOOSTER: Sprite = Sprite {
    name: "crop_booster",
    rows: [
        "......ss........",
        ".....sGGs.......",
        "......ss..*.....",
        "......cc.*.*....",
        ".....cWWc.*.....",
        "....cWwwWc......",
        "...cGgggggc.....",
        "..cGgLggggGc....",
        "..cgLgggggGc.*..",
        "..cggggggDGc*.*.",
        "..cGggggDDGc.*..",
        "...cGgggDGc.....",
        "....cGGGGc......",
        ".....cccc.......",
        "................",
        "................",
    ],
    palette: &[
        ('.', [0, 0, 0, 0]),
        ('c', [0x2b, 0x3a, 0x2e, 0xff]),
        ('W', [0xd8, 0xe8, 0xe0, 0xff]),
        ('w', [0xb0, 0xc8, 0xbc, 0xff]),
        ('g', [0x4c, 0xc9, 0x4a, 0xff]),
        ('G', [0x2e, 0x8f, 0x33, 0xff]),
        ('D', [0x1f, 0x66, 0x24, 0xff]),
        ('L', [0xb6, 0xf5, 0xa8, 0xff]),
        ('s', [0x7b, 0xd1, 0x4f, 0xff]),
        ('*', [0xff, 0xf2, 0x8a, 0xff]),
    ],
};

const YML: &str = r#"# 서버에 들어 있는 길드 아이템 (ItemsAdder 설정과 같은 모양)
info:
  namespace: hoya
items:
  crop_booster:
    name: "<#7BD14F>작물 성장 촉진제"
    lore:
      - "<gray>우클릭하면 길드 청크 지도가 열리고,"
      - "<gray>고른 우리 길드 청크의 작물 성장 단계가 하나 오릅니다."
      - "<dark_gray>그 청크의 작물만 빨리 자랍니다"
    max_stack_size: 16
    material: bone_meal
    graphics:
      texture: item/crop_booster
"#;

pub fn pack() -> Pack {
    Pack {
        name: "hoya".into(),
        configs: vec![("(서버 내장) hoya/configs/guild.yml".into(), YML.into())],
        files: vec![(
            "assets/hoya/textures/item/crop_booster.png".into(),
            png(16, 16, &CROP_BOOSTER.rgba()),
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_is_complete() {
        for (y, row) in CROP_BOOSTER.rows.iter().enumerate() {
            assert_eq!(row.chars().count(), 16, "row {y}");
            assert!(
                row.chars()
                    .all(|c| CROP_BOOSTER.palette.iter().any(|(p, _)| *p == c)),
                "row {y}"
            );
        }
    }
}
