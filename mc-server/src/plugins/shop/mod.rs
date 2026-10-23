//! 상점: 재화(돈·캐시·잠수 포인트)마다 사고팔기. `/상점 [id]`, 퀵메뉴 워프.
//!
//! - 상점 하나 = 재화 하나 + 물건 목록. 물건마다 한 묶음(`count`개)의 살 값(`buy`)·팔 값(`sell`),
//!   0이면 그쪽은 안 됨.
//! - 물건: 바닐라 이름(`bread`), 우리 아이템(`hoya:enhance_stone`), 또는 특수 —
//!   `@fly:<초>`(플라이 시간), `@autoplant:<블록>`(자동심기).
//! - 창: 좌클릭 1묶음 사기 · Shift+좌클릭 10묶음 · 우클릭 1묶음 팔기 · Shift+우클릭 가진 것 모두 팔기.
//! - `world/shops.json` (`MC_SHOPS`), 웹 관리 화면 "상점" 탭. 고치면 곧바로.

pub mod gui;

use std::sync::OnceLock;

use crate::json::Json;
use crate::plugins::store::{JsonFile, data_path};

/// 재화.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    Money,
    Cash,
    Afk,
}

impl Currency {
    pub const ALL: [Currency; 3] = [Currency::Money, Currency::Cash, Currency::Afk];

    /// 담비 공유 데이터의 칸 이름.
    pub fn field(self) -> &'static str {
        match self {
            Currency::Money => "money",
            Currency::Cash => "cash",
            Currency::Afk => "afk",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Currency::Money => "돈",
            Currency::Cash => "캐시",
            Currency::Afk => "잠수 포인트",
        }
    }

    pub fn color(self) -> &'static str {
        match self {
            Currency::Money => "#ffd866",
            Currency::Cash => "#6fe3ff",
            Currency::Afk => "#9cf2c8",
        }
    }

    pub fn parse(s: &str) -> Option<Currency> {
        match s {
            "money" | "돈" => Some(Currency::Money),
            "cash" | "캐시" => Some(Currency::Cash),
            "afk" | "잠수" | "잠수포인트" | "잠수 포인트" => Some(Currency::Afk),
            _ => None,
        }
    }

    pub fn of(self, d: &crate::dambi::shared::SharedData) -> i64 {
        match self {
            Currency::Money => d.money,
            Currency::Cash => d.cash,
            Currency::Afk => d.afk,
        }
    }
}

/// 물건 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// `bread`, `hoya:enhance_stone`, `@fly:3600`, `@autoplant:1000`
    pub item: String,
    /// 보일 이름 (비면 아이템 이름)
    pub name: String,
    /// 한 묶음 개수
    pub count: i32,
    /// 한 묶음 살 값 (0 = 안 팜)
    pub buy: i64,
    /// 한 묶음 팔 값 (0 = 안 삼)
    pub sell: i64,
}

/// 특수 물건.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    /// 플라이 (초)
    Fly(i64),
    /// 자동심기 (블록)
    Autoplant(u64),
}

impl Entry {
    pub fn special(&self) -> Option<Special> {
        let (kind, v) = self.item.strip_prefix('@')?.split_once(':')?;
        match kind {
            "fly" => Some(Special::Fly(v.parse().ok()?)),
            "autoplant" => Some(Special::Autoplant(v.parse().ok()?)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shop {
    pub id: String,
    pub name: String,
    pub currency: Currency,
    /// 목록에서 보일 아이콘
    pub icon: String,
    pub description: String,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShopConfig {
    pub shops: Vec<Shop>,
}

fn e(item: &str, count: i32, buy: i64, sell: i64) -> Entry {
    Entry {
        item: item.into(),
        name: String::new(),
        count,
        buy,
        sell,
    }
}

fn named(item: &str, name: &str, count: i32, buy: i64, sell: i64) -> Entry {
    Entry {
        name: name.into(),
        ..e(item, count, buy, sell)
    }
}

impl Default for ShopConfig {
    fn default() -> Self {
        ShopConfig {
            shops: vec![
                Shop {
                    id: "general".into(),
                    name: "잡화 상점".into(),
                    currency: Currency::Money,
                    icon: "emerald".into(),
                    description: "생활 자원을 사고팝니다".into(),
                    entries: vec![
                        e("cobblestone", 64, 640, 64),
                        e("oak_log", 16, 480, 80),
                        e("oak_planks", 64, 640, 64),
                        e("coal", 16, 960, 160),
                        e("raw_copper", 16, 1_280, 240),
                        e("raw_iron", 8, 1_600, 320),
                        e("raw_gold", 4, 2_000, 400),
                        e("lapis_lazuli", 16, 1_600, 240),
                        e("obsidian", 4, 2_000, 300),
                        e("wheat_seeds", 16, 160, 0),
                        e("wheat", 16, 480, 64),
                        e("carrot", 16, 480, 64),
                        e("potato", 16, 480, 64),
                        e("beetroot", 16, 480, 64),
                        e("beef", 8, 640, 96),
                        e("chicken", 8, 480, 64),
                        e("mutton", 8, 560, 80),
                        e("porkchop", 8, 640, 96),
                        e("bread", 16, 640, 0),
                        e("cooked_beef", 16, 1_280, 0),
                        e("torch", 64, 320, 0),
                        e("golden_apple", 1, 5_000, 800),
                        e("hoya:enhance_stone", 1, 2_000, 0),
                        e("hoya:scroll_low", 1, 5_000, 0),
                    ],
                },
                Shop {
                    id: "cash".into(),
                    name: "캐시 상점".into(),
                    currency: Currency::Cash,
                    icon: "diamond".into(),
                    description: "플라이·자동심기·강화 재료".into(),
                    entries: vec![
                        named("@fly:3600", "플라이 1시간", 1, 100, 0),
                        named("@fly:36000", "플라이 10시간", 1, 900, 0),
                        named("@autoplant:1000", "자동심기 1,000블록", 1, 50, 0),
                        named("@autoplant:10000", "자동심기 10,000블록", 1, 450, 0),
                        e("hoya:enhance_stone", 10, 100, 0),
                        e("hoya:scroll_low", 3, 150, 0),
                    ],
                },
                Shop {
                    id: "afk".into(),
                    name: "잠수 상점".into(),
                    currency: Currency::Afk,
                    icon: "clock".into(),
                    description: "잠수 포인트로 바꾸기".into(),
                    entries: vec![
                        named("@fly:600", "플라이 10분", 1, 30, 0),
                        named("@autoplant:100", "자동심기 100블록", 1, 20, 0),
                        e("experience_bottle", 8, 10, 0),
                        e("hoya:enhance_stone", 1, 40, 0),
                        e("hoya:scroll_low", 1, 120, 0),
                    ],
                },
            ],
        }
    }
}

impl ShopConfig {
    pub fn shop(&self, id: &str) -> Option<&Shop> {
        self.shops
            .iter()
            .find(|s| s.id == id || s.name == id || s.name.replace(' ', "") == id)
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        for s in &self.shops {
            if s.id.is_empty() || !ids.insert(s.id.as_str()) {
                return Err(format!("상점 id가 비었거나 겹칩니다: '{}'", s.id));
            }
            if s.entries.len() > gui::PER_PAGE * 20 {
                return Err(format!("{}: 물건이 너무 많습니다", s.name));
            }
            for en in &s.entries {
                if en.count < 1 || en.count > 64 * 36 {
                    return Err(format!("{} / {}: 묶음 개수는 1~2304", s.name, en.item));
                }
                if en.buy < 0 || en.sell < 0 {
                    return Err(format!("{} / {}: 값은 0 이상", s.name, en.item));
                }
                if en.buy > 0 && en.sell > en.buy {
                    return Err(format!(
                        "{} / {}: 팔 값이 살 값보다 크면 돈을 찍어 낼 수 있습니다",
                        s.name, en.item
                    ));
                }
                if en.item.starts_with('@') && en.special().is_none() {
                    return Err(format!("{}: 모르는 특수 물건 {}", s.name, en.item));
                }
                if en.item.starts_with('@') && en.sell > 0 {
                    return Err(format!("{}: {}는 팔 수 없습니다", s.name, en.item));
                }
            }
        }
        Ok(())
    }
}

// ---- JSON ----

pub fn encode(c: &ShopConfig) -> Json {
    let n = |v: f64| Json::Number(v);
    let s = |v: &str| Json::String(v.to_owned());
    Json::Object(vec![(
        "shops".into(),
        Json::Array(
            c.shops
                .iter()
                .map(|sh| {
                    Json::Object(vec![
                        ("id".into(), s(&sh.id)),
                        ("name".into(), s(&sh.name)),
                        ("currency".into(), s(sh.currency.field())),
                        ("icon".into(), s(&sh.icon)),
                        ("description".into(), s(&sh.description)),
                        (
                            "entries".into(),
                            Json::Array(
                                sh.entries
                                    .iter()
                                    .map(|en| {
                                        Json::Object(vec![
                                            ("item".into(), s(&en.item)),
                                            ("name".into(), s(&en.name)),
                                            ("count".into(), n(en.count as f64)),
                                            ("buy".into(), n(en.buy as f64)),
                                            ("sell".into(), n(en.sell as f64)),
                                        ])
                                    })
                                    .collect(),
                            ),
                        ),
                    ])
                })
                .collect(),
        ),
    )])
}

pub fn decode(j: &Json) -> Option<ShopConfig> {
    let Some(shops) = j.get("shops").and_then(Json::as_array) else {
        return Some(ShopConfig::default());
    };
    let st = |j: &Json, k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
    let nu = |j: &Json, k: &str, d: f64| j.get(k).and_then(Json::as_f64).unwrap_or(d);
    Some(ShopConfig {
        shops: shops
            .iter()
            .map(|sh| Shop {
                id: st(sh, "id"),
                name: st(sh, "name"),
                currency: Currency::parse(&st(sh, "currency")).unwrap_or(Currency::Money),
                icon: st(sh, "icon"),
                description: st(sh, "description"),
                entries: sh
                    .get("entries")
                    .and_then(Json::as_array)
                    .unwrap_or_default()
                    .iter()
                    .map(|en| Entry {
                        item: st(en, "item"),
                        name: st(en, "name"),
                        count: nu(en, "count", 1.0) as i32,
                        buy: nu(en, "buy", 0.0) as i64,
                        sell: nu(en, "sell", 0.0) as i64,
                    })
                    .collect(),
            })
            .collect(),
    })
}

static FILE: OnceLock<JsonFile<ShopConfig>> = OnceLock::new();

fn file() -> &'static JsonFile<ShopConfig> {
    FILE.get_or_init(|| JsonFile::in_memory(encode, decode))
}

pub fn with<R>(f: impl FnOnce(&ShopConfig) -> R) -> R {
    file().read(f)
}

pub fn get() -> ShopConfig {
    with(|c| c.clone())
}

pub fn set(c: ShopConfig) -> Result<(), String> {
    c.validate()?;
    file().write(|old| *old = c);
    Ok(())
}

pub fn open_global() {
    let _ = FILE.set(JsonFile::open(
        data_path("MC_SHOPS", "shops.json"),
        encode,
        decode,
    ));
    if let Some(p) = file().path()
        && !p.exists()
    {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, encode(&ShopConfig::default()).pretty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe_and_round_trip() {
        let c = ShopConfig::default();
        c.validate().unwrap();
        assert_eq!(decode(&encode(&c)).unwrap(), c);
        // 세 재화 모두 상점이 있음
        for cur in Currency::ALL {
            assert!(c.shops.iter().any(|s| s.currency == cur));
        }
        assert_eq!(
            c.shop("cash").unwrap().entries[0].special(),
            Some(Special::Fly(3600))
        );
        // 팔 값 > 살 값은 막음 (무한 돈)
        let mut bad = c.clone();
        bad.shops[0].entries[0].sell = 10_000;
        assert!(bad.validate().is_err());
    }

    /// 설명 꾸밈 미리보기용: 툴팁 몇 개를 JSON으로 (`tools/lore_preview.py`가 그림으로).
    /// `MC_LORE_PREVIEW_JSON=out.json cargo test lore_showcase -- --ignored`
    #[test]
    #[ignore]
    fn lore_showcase() {
        use crate::protocol::nbt::Tag;
        use crate::text::{colored, lore_line};
        let Ok(path) = std::env::var("MC_LORE_PREVIEW_JSON") else {
            return;
        };
        fn js(t: &Tag) -> Json {
            match t {
                Tag::String(s) => Json::String(s.clone()),
                Tag::Byte(b) => Json::Number(f64::from(*b)),
                Tag::List(l) => Json::Array(l.iter().map(js).collect()),
                Tag::Compound(f) => {
                    Json::Object(f.iter().map(|(k, v)| (k.clone(), js(v))).collect())
                }
                _ => Json::Null,
            }
        }
        let tip = |name: Tag, lore: Vec<String>| {
            Json::Object(vec![
                ("name".into(), js(&name)),
                (
                    "lore".into(),
                    Json::Array(lore.iter().map(|l| js(&lore_line(l))).collect()),
                ),
            ])
        };
        let c = ShopConfig::default();
        let general = c.shop("general").unwrap();
        let cash = c.shop("cash").unwrap();
        let rank = crate::plugins::rank::RankConfig::default();
        let tips = vec![
            tip(
                colored("조약돌", Currency::Money.color()),
                gui::entry_lore(general, &general.entries[0], Some(128)),
            ),
            tip(
                colored("플라이 1시간", Currency::Cash.color()),
                gui::entry_lore(cash, &cash.entries[0], None),
            ),
            tip(
                colored("VIP", &rank.ranks[1].color),
                crate::plugins::rank::session::card_lore(&rank, 1, false),
            ),
            tip(
                colored("광장", "gold"),
                vec![
                    "서버의 중심. 상점·대장장이·경매장이 모여 있음".into(),
                    String::new(),
                    "▶ 클릭: 광장(으)로 이동".into(),
                    "▶ Shift+클릭: 자세한 워프".into(),
                ],
            ),
            tip(
                colored("[일일] 조약돌", "white"),
                vec![
                    "[광물] 조약돌 32개를 내세요".into(),
                    "■■■■■■■□□□□□ 19/32".into(),
                    "좌클릭: 가방의 아이템 내기".into(),
                ],
            ),
            tip(
                colored("Trader", "gold"),
                vec![
                    "등급  VIP".into(),
                    "길드  달빛 · 길드원".into(),
                    "레벨  12 · 전투력 1,284".into(),
                    "대표 직업  광부 Lv.12".into(),
                    "돈 9,424 · 캐시 0 · 잠수 70".into(),
                    "놀이 시간  3시간 12분".into(),
                ],
            ),
        ];
        std::fs::write(path, Json::Array(tips).pretty()).unwrap();
    }
}
