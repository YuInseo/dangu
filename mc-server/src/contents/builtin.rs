//! 서버에 들어 있는 콘텐츠 팩 `dangu` (ItemsAdder 구조 1과 같은 모양을 메모리에 둠).
//!
//! 복권: 바닐라 종이(`PAPER`) + `model_id`(custom_model_data) 10001 → 직접 만든 3D 모델.
//! 두께가 있는 카드 모양이고, 앞면은 복권 그림, 뒷면은 따로 그린 무늬입니다. 다른 팩처럼
//! `contents/dangu/`에 같은 이름의 파일을 두면 그쪽이 이깁니다.

use crate::resource_pack::{LOTTERY_TICKET, Sprite, png};

/// 복권 뒷면: 크림색 바탕에 빨간 사선, 가운데 금빛 별.
pub const LOTTERY_TICKET_BACK: Sprite = Sprite {
    name: "lottery_ticket_back",
    rows: [
        "................",
        ".OOOOOOOOOOOOOO.",
        "OcRccRccRccRccCO",
        "OccRccRccRccRcCO",
        "ORccRccRccRccRCO",
        "OcRccRcgcRccRcCO",
        ".OccRcgggcRccRO.",
        "ORccRgggggcRcCCO",
        "OcRccRgggRcRccCO",
        ".OccRcgcgcRccRO.",
        "ORccRccRccRccRCO",
        "OcRccRccRccRccCO",
        "OCCCCCCCCCCCCCCO",
        ".OOOOOOOOOOOOOO.",
        "................",
        "................",
    ],
    palette: &[
        ('.', [0, 0, 0, 0]),
        ('O', [0x4a, 0x2c, 0x12, 0xff]),
        ('R', [0xe0, 0x40, 0x40, 0xff]),
        ('c', [0xff, 0xf4, 0xcf, 0xff]),
        ('C', [0xec, 0xd9, 0xa0, 0xff]),
        ('g', [0xff, 0xc8, 0x2a, 0xff]),
    ],
};

/// 복권의 사용자 정의 모델 번호 (custom_model_data).
pub const LOTTERY_MODEL_ID: i32 = 10001;

const LOTTERY_YML: &str = r#"# 서버에 들어 있는 복권 아이템 (ItemsAdder 설정과 같은 모양)
info:
  namespace: dangu
items:
  lottery_ticket:
    name: "&6복권"
    lore:
      - "&7스폰의 복권 NPC에게 내면 당첨금을 받습니다"
    max_stack_size: 1
    resource:
      material: PAPER
      generate: false
      model_path: item/lottery_ticket
      model_id: 10001
  gui_icon:
    name: " "
    resource:
      material: PAPER
"#;

/// 두께 0.5의 카드: 남쪽(+Z) 면이 앞, 북쪽 면이 뒤. 손·화면·액자에 맞춘 자세.
const LOTTERY_MODEL: &str = r##"{
  "credit": "dangu - hand made",
  "texture_size": [16, 16],
  "textures": {
    "front": "dangu:item/lottery_ticket",
    "back": "dangu:item/lottery_ticket_back",
    "particle": "dangu:item/lottery_ticket"
  },
  "elements": [
    {
      "name": "card",
      "from": [0, 2, 7.75],
      "to": [16, 15, 8.25],
      "faces": {
        "south": {"uv": [0, 1, 16, 14], "texture": "#front"},
        "north": {"uv": [16, 1, 0, 14], "texture": "#back"},
        "east": {"uv": [14, 5, 15, 13], "texture": "#front"},
        "west": {"uv": [1, 5, 2, 13], "texture": "#front"},
        "up": {"uv": [1, 1, 15, 2], "texture": "#front"},
        "down": {"uv": [1, 12, 15, 13], "texture": "#front"}
      }
    }
  ],
  "gui_light": "front",
  "display": {
    "gui": {"rotation": [12, -24, 0], "translation": [0, -0.5, 0], "scale": [0.95, 0.95, 0.95]},
    "ground": {"rotation": [0, 0, 0], "translation": [0, 2, 0], "scale": [0.5, 0.5, 0.5]},
    "fixed": {"rotation": [0, 180, 0], "translation": [0, 0, 0], "scale": [1, 1, 1]},
    "head": {"rotation": [0, 180, 0], "translation": [0, 13, 7], "scale": [1, 1, 1]},
    "thirdperson_righthand": {"rotation": [0, -90, 55], "translation": [0, 3.5, 0.5], "scale": [0.55, 0.55, 0.55]},
    "thirdperson_lefthand": {"rotation": [0, 90, -55], "translation": [0, 3.5, 0.5], "scale": [0.55, 0.55, 0.55]},
    "firstperson_righthand": {"rotation": [0, -90, 25], "translation": [1.13, 3.2, 1.13], "scale": [0.68, 0.68, 0.68]},
    "firstperson_lefthand": {"rotation": [0, 90, -25], "translation": [1.13, 3.2, 1.13], "scale": [0.68, 0.68, 0.68]}
  }
}
"##;

/// 들어 있는 팩.
pub fn packs() -> Vec<super::source::Pack> {
    let mut files = vec![(
        "assets/dangu/models/item/lottery_ticket.json".to_owned(),
        LOTTERY_MODEL.as_bytes().to_vec(),
    )];
    for s in [&LOTTERY_TICKET, &LOTTERY_TICKET_BACK] {
        files.push((
            format!("assets/dangu/textures/item/{}.png", s.name),
            png(16, 16, &s.rgba()),
        ));
    }
    vec![super::source::Pack {
        name: "dangu".into(),
        configs: vec![(
            "(서버 내장) dangu/configs/lottery.yml".into(),
            LOTTERY_YML.into(),
        )],
        files,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_model_is_valid_json_and_sprites_are_complete() {
        let m = crate::json::parse(LOTTERY_MODEL).unwrap();
        assert_eq!(m.get("elements").unwrap().as_array().unwrap().len(), 1);
        for row in LOTTERY_TICKET_BACK.rows {
            assert_eq!(row.chars().count(), 16);
            assert!(
                row.chars()
                    .all(|c| LOTTERY_TICKET_BACK.palette.iter().any(|(p, _)| *p == c))
            );
        }
        assert!(LOTTERY_YML.contains(&LOTTERY_MODEL_ID.to_string()));
    }
}
