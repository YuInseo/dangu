//! 장비 내용과 수치 (`MC_EQUIPMENT`, 기본 `world/equipment.json`). 없으면 기본값을 써 둡니다.
//! 웹 관리 화면의 "장비" 탭에서 고치면 모든 인스턴스가 곧바로 다시 읽습니다 (수치·확률·조합법·드랍).
//! 장비·재료를 새로 더하는 것은 아이템 그림이 필요해 서버를 다시 켤 때 반영됩니다.

use std::sync::OnceLock;

use super::{Part, Stat, Stats};
use crate::json::Json;
use crate::plugins::store::{JsonFile, data_path};

/// 재료 (원석·주괴·마석·강화석·주문서).
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// `dangu:mithril_ingot`
    pub id: String,
    pub name: String,
    /// 이름 색 (`#9fd8ff`)
    pub color: String,
    /// 아이템 등급 이름 (일반·고급·희귀·상급 …)
    pub grade: String,
    pub lore: Vec<String>,
    /// 그림 모양: `raw` `ingot` `gem` `magic_stone` `enhance_stone` `scroll`
    pub art: String,
    /// 색 이름 ([`super::art::palette`])
    pub palette: String,
}

/// 장비 한 종류.
#[derive(Debug, Clone, PartialEq)]
pub struct GearDef {
    /// `dangu:mithril_helmet`
    pub id: String,
    pub name: String,
    pub part: Part,
    /// 세트 (없으면 None)
    pub set: Option<String>,
    /// 1..=9 (높을수록 좋음; 그림 모양도 바뀜)
    pub tier: u8,
    /// 등급 이름 (고급·희귀·영웅·전설·신화)
    pub grade: String,
    pub stats: Stats,
    /// 악세서리: 잠재능력을 바를 수 있는 횟수
    pub potential_tries: u8,
    pub palette: String,
    /// 설명 한 줄
    pub lore: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetDef {
    pub id: String,
    pub name: String,
    pub color: String,
    /// (필요 개수, 효과)
    pub bonuses: Vec<(u8, Stats)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    pub result: String,
    pub count: i32,
    /// (아이템 — 우리 것은 `dangu:…`, 바닐라는 이름, 개수)
    pub inputs: Vec<(String, i32)>,
    pub money: i64,
    /// 필요한 대장장이 레벨
    pub smith_level: u32,
    /// 대장장이 경험치
    pub xp: f64,
    /// 창의 탭: 제련 · 장비 · 악세서리 · 재료
    pub category: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OreDef {
    /// 광석 블록 아이템 (`dangu:mithril_ore`)
    pub block: String,
    pub name: String,
    /// 캐면 나오는 것
    pub drop: String,
    pub nether: bool,
    pub min_y: i32,
    pub max_y: i32,
    /// 청크마다 광맥 수
    pub tries: u32,
    /// 광맥 크기
    pub size: u32,
    /// 광부 경험치
    pub xp: f64,
    /// 캐려면 필요한 곡괭이 (`iron` `diamond` …)
    pub pickaxe: String,
    pub palette: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Drop {
    pub item: String,
    /// 0..=1
    pub chance: f64,
    pub min: i32,
    pub max: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BossDef {
    pub id: String,
    pub name: String,
    /// 바탕 몹 (`zombie`, `wither_skeleton` …)
    pub mob: String,
    /// 바닐라 단위 체력
    pub health: f32,
    /// 공격 배율
    pub damage: f32,
    /// 이 몹이 자연히 생길 때 보스로 바뀔 확률 (0이면 소환으로만)
    pub spawn_chance: f64,
    /// 레이드 보스 (상급 주문서)
    pub raid: bool,
    pub drops: Vec<Drop>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnhanceRule {
    /// `chances[n]` = +n → +n+1 성공 확률 (0..=1)
    pub chances: Vec<f64>,
    /// 단계마다 기본 능력치 +
    pub per_level: f64,
    /// +10·+20·+30에서 더
    pub milestone: f64,
    /// 돈: `money × (n+1)²`
    pub money: i64,
    /// 강화석: `1 + n/3`개
    pub stone: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrollDef {
    pub item: String,
    pub name: String,
    pub min_lines: u8,
    pub max_lines: u8,
    /// 잠재능력 값 배율
    pub mult: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PotentialOption {
    pub stat: Stat,
    pub weight: u32,
    /// 하급 주문서 기준 값 범위
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShopEntry {
    pub item: String,
    pub price: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EquipConfig {
    pub materials: Vec<Material>,
    pub gear: Vec<GearDef>,
    pub sets: Vec<SetDef>,
    pub recipes: Vec<Recipe>,
    pub ores: Vec<OreDef>,
    pub bosses: Vec<BossDef>,
    pub enhance: EnhanceRule,
    pub scrolls: Vec<ScrollDef>,
    pub potentials: Vec<PotentialOption>,
    pub shop: Vec<ShopEntry>,
    /// 대장장이 직업 이름
    pub smith_job: String,
}

impl EquipConfig {
    pub fn gear_def(&self, id: &str) -> Option<&GearDef> {
        self.gear.iter().find(|g| g.id == id)
    }
    pub fn set(&self, id: &str) -> Option<&SetDef> {
        self.sets.iter().find(|s| s.id == id)
    }
    pub fn material(&self, id: &str) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == id)
    }
    pub fn scroll(&self, item: &str) -> Option<&ScrollDef> {
        self.scrolls.iter().find(|s| s.item == item)
    }
    pub fn boss(&self, id: &str) -> Option<&BossDef> {
        self.bosses.iter().find(|b| b.id == id)
    }
    pub fn ore_by_block(&self, block: &str) -> Option<&OreDef> {
        self.ores.iter().find(|o| o.block == block)
    }
    /// 우리 아이템 이름 (재료든 장비든).
    pub fn item_name(&self, id: &str) -> Option<String> {
        self.material(id)
            .map(|m| m.name.clone())
            .or_else(|| self.gear_def(id).map(|g| g.name.clone()))
            .or_else(|| {
                self.ores
                    .iter()
                    .find(|o| o.block == id)
                    .map(|o| o.name.clone())
            })
    }
    /// +n → +n+1 성공 확률.
    pub fn enhance_chance(&self, level: u8) -> f64 {
        self.enhance
            .chances
            .get(level as usize)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }
    /// +n → +n+1 비용 (돈, 강화석).
    pub fn enhance_cost(&self, level: u8) -> (i64, i32) {
        let n = level as i64 + 1;
        (self.enhance.money * n * n, 1 + level as i32 / 3)
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        for id in self
            .materials
            .iter()
            .map(|m| &m.id)
            .chain(self.gear.iter().map(|g| &g.id))
            .chain(self.ores.iter().map(|o| &o.block))
        {
            if !id.starts_with("dangu:") || !ids.insert(id.clone()) {
                return Err(format!(
                    "아이템 id는 dangu:로 시작하고 겹치지 않아야 합니다: {id}"
                ));
            }
        }
        for g in &self.gear {
            if let Some(s) = &g.set
                && self.set(s).is_none()
            {
                return Err(format!("{}: 없는 세트 {s}", g.id));
            }
        }
        if self.enhance.chances.len() < super::MAX_ENHANCE as usize {
            return Err(format!(
                "enhance.chances는 {}개(+0→+1 … +29→+30)여야 합니다",
                super::MAX_ENHANCE
            ));
        }
        if self.potentials.iter().all(|p| p.weight == 0) {
            return Err("잠재능력 옵션이 하나는 있어야 합니다".into());
        }
        for s in &self.scrolls {
            if s.min_lines == 0 || s.min_lines > s.max_lines {
                return Err(format!("{}: 줄 수가 이상합니다", s.item));
            }
        }
        for b in &self.bosses {
            if crate::game::mobs::MobKind::by_name(&b.mob).is_none() {
                return Err(format!("{}: 모르는 몹 {}", b.id, b.mob));
            }
        }
        Ok(())
    }
}

// ---- 기본 내용 ----

/// 광물 6종 + 마석 세트: (키, 이름, 색, 단계, 지옥, 원석 이름, 주괴 이름)
const MATERIALS: [(&str, &str, &str, u8, bool, &str, &str); 6] = [
    (
        "mithril",
        "미스릴",
        "#9fd8ff",
        1,
        false,
        "미스릴 원석",
        "미스릴 주괴",
    ),
    (
        "cobalt",
        "코발트",
        "#4f7bff",
        2,
        true,
        "코발트 원석",
        "코발트 주괴",
    ),
    (
        "orichalcum",
        "오리하르콘",
        "#ff9a3c",
        3,
        false,
        "오리하르콘 원석",
        "오리하르콘 주괴",
    ),
    (
        "soul_crystal",
        "소울 크리스탈",
        "#5ff0e6",
        4,
        true,
        "소울 크리스탈 조각",
        "정제된 소울 크리스탈",
    ),
    (
        "seraphium",
        "세라피움",
        "#fff1a8",
        5,
        false,
        "세라피움 원석",
        "세라피움 주괴",
    ),
    (
        "evilium",
        "이블리움",
        "#e0294f",
        6,
        true,
        "이블리움 원석",
        "이블리움 주괴",
    ),
];

pub fn grade_of(tier: u8) -> (&'static str, &'static str) {
    match tier {
        0..=1 => ("고급", "#7be07b"),
        2..=3 => ("희귀", "#5aa0ff"),
        4..=5 => ("영웅", "#c77dff"),
        6..=7 => ("전설", "#ffb347"),
        _ => ("신화", "#ff5a5a"),
    }
}

/// 등급 이름 → 색.
pub fn grade_color(grade: &str) -> &'static str {
    match grade {
        "일반" => "#d2d2dc",
        "고급" => "#7be07b",
        "희귀" => "#5aa0ff",
        "영웅" | "상급" => "#c77dff",
        "전설" => "#ffb347",
        "신화" => "#ff5a5a",
        _ => "#ffffff",
    }
}

fn st(list: &[(Stat, f64)]) -> Stats {
    Stats::of(list)
}

fn gear_piece(key: &str, set_name: &str, part: Part, tier: u8, palette: &str) -> GearDef {
    use Stat::*;
    let t = tier as f64;
    let (noun, stats) = match part {
        Part::Helmet => ("투구", st(&[(Defense, 3.0 * t + 2.0), (Mp, 15.0 * t)])),
        Part::Chest => ("흉갑", st(&[(Defense, 6.0 * t + 4.0), (Hp, 30.0 * t)])),
        Part::Legs => ("각반", st(&[(Defense, 5.0 * t + 3.0), (Hp, 20.0 * t)])),
        Part::Boots => ("장화", st(&[(Defense, 2.0 * t + 2.0), (Mp, 10.0 * t)])),
        Part::Sword => ("검", st(&[(Attack, 30.0 + 12.0 * t), (Hp, 10.0 * t)])),
        Part::Staff => ("지팡이", st(&[(Magic, 30.0 + 12.0 * t), (Mp, 20.0 * t)])),
        Part::Ring => ("반지", st(&[(CritChance, 1.0 + 0.5 * t)])),
        Part::Necklace => ("목걸이", st(&[(CritDamage, 4.0 * t)])),
        Part::Earring => (
            "귀걸이",
            st(&[(CritChance, 0.5 * t), (CritDamage, 2.0 * t)]),
        ),
    };
    let accessory = part.is_accessory();
    GearDef {
        id: format!("dangu:{key}_{}", part.key()),
        name: format!("{set_name} {noun}"),
        part,
        set: (!accessory).then(|| key.to_owned()),
        tier,
        grade: grade_of(tier).0.to_owned(),
        stats,
        potential_tries: if accessory { 3 } else { 0 },
        palette: palette.to_owned(),
        lore: if accessory {
            format!("{set_name}로 만든 장신구. 잠재능력을 바를 수 있습니다")
        } else {
            format!("{set_name}로 벼려 낸 장비")
        },
    }
}

/// 보스 악세서리 세 개.
fn boss_accessories(key: &str, owner: &str, power: f64, tries: u8, grade: &str) -> Vec<GearDef> {
    use Stat::*;
    [
        (Part::Ring, "반지", st(&[(CritChance, 1.2 * power)])),
        (Part::Necklace, "목걸이", st(&[(CritDamage, 5.0 * power)])),
        (
            Part::Earring,
            "귀걸이",
            st(&[(CritChance, 0.6 * power), (CritDamage, 2.5 * power)]),
        ),
    ]
    .into_iter()
    .map(|(part, noun, stats)| GearDef {
        id: format!("dangu:{key}_{}", part.key()),
        name: format!("{owner}의 {noun}"),
        part,
        set: None,
        tier: power.round() as u8,
        grade: grade.to_owned(),
        stats,
        potential_tries: tries,
        palette: key.to_owned(),
        lore: format!("{owner}에게서 얻은 장신구. 만든 것보다 강합니다"),
    })
    .collect()
}

fn set_def(id: &str, name: &str, color: &str, bonuses: Vec<(u8, Vec<(Stat, f64)>)>) -> SetDef {
    SetDef {
        id: id.into(),
        name: name.into(),
        color: color.into(),
        bonuses: bonuses.into_iter().map(|(n, s)| (n, st(&s))).collect(),
    }
}

fn mat(
    id: &str,
    name: &str,
    color: &str,
    grade: &str,
    art: &str,
    palette: &str,
    lore: &[&str],
) -> Material {
    Material {
        id: format!("dangu:{id}"),
        name: name.into(),
        color: color.into(),
        grade: grade.into(),
        lore: lore.iter().map(|s| (*s).to_owned()).collect(),
        art: art.into(),
        palette: palette.into(),
    }
}

fn recipe(
    result: &str,
    count: i32,
    inputs: &[(&str, i32)],
    money: i64,
    smith_level: u32,
    xp: f64,
    category: &str,
) -> Recipe {
    Recipe {
        result: result.into(),
        count,
        inputs: inputs.iter().map(|(i, n)| ((*i).to_owned(), *n)).collect(),
        money,
        smith_level,
        xp,
        category: category.into(),
    }
}

fn drop(item: &str, chance: f64, min: i32, max: i32) -> Drop {
    Drop {
        item: item.into(),
        chance,
        min,
        max,
    }
}

impl Default for EquipConfig {
    fn default() -> Self {
        use Stat::*;
        let mut materials = Vec::new();
        let mut gear = Vec::new();
        let mut recipes = Vec::new();
        let mut ores = Vec::new();
        for (key, name, color, tier, nether, raw, ingot) in MATERIALS {
            let (grade, _) = grade_of(tier);
            let crystal = key == "soul_crystal";
            materials.push(mat(
                &format!("{key}_raw"),
                raw,
                color,
                grade,
                "raw",
                key,
                &["대장간에서 제련하면 주괴가 됩니다"],
            ));
            materials.push(mat(
                &format!("{key}_ingot"),
                ingot,
                color,
                grade,
                if crystal { "gem" } else { "ingot" },
                key,
                &["장비 제작 재료"],
            ));
            let t = tier as u32;
            ores.push(OreDef {
                block: format!("dangu:{key}_ore"),
                name: format!("{name} 광석"),
                drop: format!("dangu:{key}_raw"),
                nether,
                min_y: match (nether, tier) {
                    (false, 1) => -16,
                    (false, 3) => -48,
                    (false, _) => -64,
                    (true, 6) => 8,
                    (true, _) => 10,
                },
                max_y: match (nether, tier) {
                    (false, 1) => 64,
                    (false, 3) => 16,
                    (false, _) => -24,
                    (true, 2) => 110,
                    (true, 4) => 64,
                    (true, _) => 30,
                },
                tries: match tier {
                    1 | 2 => 6,
                    3 | 4 => 4,
                    _ => 2,
                },
                size: match tier {
                    1 | 2 => 5,
                    3 | 4 => 4,
                    _ => 3,
                },
                xp: 6.0 * tier as f64,
                pickaxe: if tier <= 2 { "iron" } else { "diamond" }.into(),
                palette: key.into(),
            });
            let ingot_id = format!("dangu:{key}_ingot");
            recipes.push(recipe(
                &ingot_id,
                1,
                &[(&format!("dangu:{key}_raw"), 2)],
                50 * t as i64,
                0,
                2.0 * tier as f64,
                "제련",
            ));
            let level = 5 * (t - 1);
            let money = 400 * (t * t) as i64;
            for (part, n, extra) in [
                (Part::Helmet, 5, None),
                (Part::Chest, 8, None),
                (Part::Legs, 7, None),
                (Part::Boots, 4, None),
                (Part::Sword, 6, Some(("stick", 2))),
                (Part::Staff, 4, Some(("amethyst_shard", 2))),
                (Part::Ring, 3, Some(("gold_ingot", 1))),
                (Part::Necklace, 4, Some(("gold_ingot", 2))),
                (Part::Earring, 2, Some(("gold_nugget", 4))),
            ] {
                let g = gear_piece(key, name, part, tier, key);
                let mut inputs = vec![(ingot_id.as_str(), n)];
                if let Some(e) = extra {
                    inputs.push(e);
                }
                recipes.push(recipe(
                    &g.id,
                    1,
                    &inputs,
                    money * if part.is_accessory() { 2 } else { 1 },
                    level,
                    10.0 * tier as f64,
                    if part.is_accessory() {
                        "악세서리"
                    } else {
                        "장비"
                    },
                ));
                gear.push(g);
            }
        }
        // 최상위: 마석으로 만드는 아르카나 세트 (악세서리는 보스 것만)
        for (part, n) in [
            (Part::Helmet, 3),
            (Part::Chest, 5),
            (Part::Legs, 4),
            (Part::Boots, 2),
            (Part::Sword, 4),
            (Part::Staff, 4),
        ] {
            let g = gear_piece("arcana", "아르카나", part, 7, "arcana");
            recipes.push(recipe(
                &g.id,
                1,
                &[
                    ("dangu:magic_stone", n),
                    ("dangu:seraphium_ingot", 2),
                    ("dangu:evilium_ingot", 2),
                ],
                50_000,
                35,
                120.0,
                "장비",
            ));
            gear.push(g);
        }
        gear.extend(boss_accessories("goblin_king", "고블린 왕", 4.0, 5, "영웅"));
        gear.extend(boss_accessories(
            "fallen_knight",
            "타락한 기사",
            6.0,
            5,
            "전설",
        ));
        gear.extend(boss_accessories("abyss_lord", "심연 군주", 9.0, 7, "신화"));

        materials.push(mat(
            "magic_stone",
            "마석",
            "#c77dff",
            "상급",
            "magic_stone",
            "arcana",
            &[
                "보스에게서 얻는 상급 재료",
                "대장장이가 최상위 장비(아르카나)를 만듭니다",
            ],
        ));
        materials.push(mat(
            "enhance_stone",
            "강화석",
            "#ffd866",
            "고급",
            "enhance_stone",
            "enhance",
            &["/강화 에서 장비를 강화할 때 씁니다"],
        ));
        for (key, name, grade, lines, how) in [
            (
                "scroll_low",
                "하급 잠재능력 부여 주문서",
                "고급",
                "1~2줄",
                "대장간 상점에서 삽니다",
            ),
            (
                "scroll_mid",
                "중급 잠재능력 부여 주문서",
                "희귀",
                "2~3줄",
                "던전·보스에서 얻습니다",
            ),
            (
                "scroll_high",
                "상급 잠재능력 부여 주문서",
                "전설",
                "3~4줄",
                "레이드에서만 (50%로 1개)",
            ),
        ] {
            materials.push(mat(
                key,
                name,
                grade_color(grade),
                grade,
                "scroll",
                key,
                &[
                    &format!("잠재능력 {lines}을 새로 붙입니다"),
                    "가방에서 집어 반지·목걸이·귀걸이 위에 놓으세요",
                    how,
                ],
            ));
        }
        recipes.push(recipe(
            "dangu:enhance_stone",
            4,
            &[("dangu:mithril_ingot", 1), ("dangu:cobalt_ingot", 1)],
            500,
            0,
            4.0,
            "재료",
        ));

        let sets = vec![
            set_def(
                "mithril",
                "미스릴",
                "#9fd8ff",
                vec![
                    (2, vec![(DefensePct, 5.0)]),
                    (3, vec![(HpPct, 5.0)]),
                    (5, vec![(AttackPct, 5.0), (MagicPct, 5.0)]),
                ],
            ),
            set_def(
                "cobalt",
                "코발트",
                "#4f7bff",
                vec![
                    (2, vec![(HpPct, 6.0)]),
                    (3, vec![(DefensePct, 8.0)]),
                    (5, vec![(CritDamage, 10.0), (AttackPct, 5.0)]),
                ],
            ),
            set_def(
                "orichalcum",
                "오리하르콘",
                "#ff9a3c",
                vec![
                    (2, vec![(AttackPct, 6.0)]),
                    (3, vec![(CritChance, 3.0)]),
                    (5, vec![(AttackPct, 12.0), (Str, 10.0)]),
                ],
            ),
            set_def(
                "soul_crystal",
                "소울 크리스탈",
                "#5ff0e6",
                vec![
                    (2, vec![(MpPct, 8.0)]),
                    (3, vec![(MagicPct, 8.0)]),
                    (5, vec![(MagicPct, 15.0), (Int, 10.0)]),
                ],
            ),
            set_def(
                "seraphium",
                "세라피움",
                "#fff1a8",
                vec![
                    (2, vec![(HpPct, 10.0)]),
                    (3, vec![(DefensePct, 12.0)]),
                    (5, vec![(AttackPct, 10.0), (MagicPct, 10.0), (HpPct, 10.0)]),
                ],
            ),
            set_def(
                "evilium",
                "이블리움",
                "#e0294f",
                vec![
                    (2, vec![(AttackPct, 10.0)]),
                    (3, vec![(CritDamage, 15.0)]),
                    (5, vec![(AttackPct, 15.0), (CritChance, 5.0), (Dex, 10.0)]),
                ],
            ),
            set_def(
                "arcana",
                "아르카나",
                "#c77dff",
                vec![
                    (2, vec![(AttackPct, 10.0), (MagicPct, 10.0)]),
                    (3, vec![(HpPct, 15.0), (MpPct, 15.0)]),
                    (
                        5,
                        vec![(AttackPct, 20.0), (MagicPct, 20.0), (CritDamage, 20.0)],
                    ),
                ],
            ),
        ];

        let bosses = vec![
            BossDef {
                id: "goblin_king".into(),
                name: "고블린 왕".into(),
                mob: "zombie".into(),
                health: 300.0,
                damage: 2.0,
                spawn_chance: 0.003,
                raid: false,
                drops: vec![
                    drop("dangu:magic_stone", 0.6, 1, 2),
                    drop("dangu:enhance_stone", 0.9, 2, 4),
                    drop("dangu:orichalcum_raw", 0.5, 2, 5),
                    drop("dangu:goblin_king_ring", 0.08, 1, 1),
                    drop("dangu:goblin_king_necklace", 0.08, 1, 1),
                    drop("dangu:goblin_king_earring", 0.08, 1, 1),
                    drop("dangu:scroll_mid", 0.10, 1, 1),
                ],
            },
            BossDef {
                id: "fallen_knight".into(),
                name: "타락한 기사".into(),
                mob: "wither_skeleton".into(),
                health: 500.0,
                damage: 2.5,
                spawn_chance: 0.004,
                raid: false,
                drops: vec![
                    drop("dangu:magic_stone", 0.75, 2, 3),
                    drop("dangu:enhance_stone", 1.0, 3, 6),
                    drop("dangu:evilium_raw", 0.4, 1, 3),
                    drop("dangu:fallen_knight_ring", 0.08, 1, 1),
                    drop("dangu:fallen_knight_necklace", 0.08, 1, 1),
                    drop("dangu:fallen_knight_earring", 0.08, 1, 1),
                    drop("dangu:scroll_mid", 0.15, 1, 1),
                ],
            },
            BossDef {
                id: "abyss_lord".into(),
                name: "심연 군주".into(),
                mob: "wither_skeleton".into(),
                health: 2500.0,
                damage: 4.0,
                spawn_chance: 0.0,
                raid: true,
                drops: vec![
                    drop("dangu:scroll_high", 0.5, 1, 1),
                    drop("dangu:magic_stone", 1.0, 4, 8),
                    drop("dangu:enhance_stone", 1.0, 8, 12),
                    drop("dangu:abyss_lord_ring", 0.15, 1, 1),
                    drop("dangu:abyss_lord_necklace", 0.15, 1, 1),
                    drop("dangu:abyss_lord_earring", 0.15, 1, 1),
                ],
            },
        ];

        let po = |stat, weight, min, max| PotentialOption {
            stat,
            weight,
            min,
            max,
        };
        EquipConfig {
            materials,
            gear,
            sets,
            recipes,
            ores,
            bosses,
            enhance: EnhanceRule {
                chances: [
                    95, 90, 85, 80, 75, 70, 65, 60, 55, 50, 45, 40, 35, 30, 27, 24, 21, 18, 16, 14,
                    12, 10, 8, 7, 6, 5, 4, 3, 2, 1,
                ]
                .iter()
                .map(|p| *p as f64 / 100.0)
                .collect(),
                per_level: 0.05,
                milestone: 0.10,
                money: 1_000,
                stone: "dangu:enhance_stone".into(),
            },
            scrolls: vec![
                ScrollDef {
                    item: "dangu:scroll_low".into(),
                    name: "하급".into(),
                    min_lines: 1,
                    max_lines: 2,
                    mult: 1.0,
                },
                ScrollDef {
                    item: "dangu:scroll_mid".into(),
                    name: "중급".into(),
                    min_lines: 2,
                    max_lines: 3,
                    mult: 1.5,
                },
                ScrollDef {
                    item: "dangu:scroll_high".into(),
                    name: "상급".into(),
                    min_lines: 3,
                    max_lines: 4,
                    mult: 2.2,
                },
            ],
            potentials: vec![
                po(Str, 10, 2.0, 6.0),
                po(Dex, 10, 2.0, 6.0),
                po(Int, 10, 2.0, 6.0),
                po(Hp, 8, 30.0, 100.0),
                po(HpPct, 6, 2.0, 5.0),
                po(Mp, 8, 20.0, 60.0),
                po(MpPct, 6, 2.0, 5.0),
                po(Attack, 8, 4.0, 12.0),
                po(AttackPct, 5, 2.0, 5.0),
                po(Defense, 8, 4.0, 12.0),
                po(DefensePct, 6, 2.0, 5.0),
                po(CritChance, 5, 1.0, 3.0),
                po(CritDamage, 5, 3.0, 8.0),
            ],
            shop: vec![
                ShopEntry {
                    item: "dangu:scroll_low".into(),
                    price: 20_000,
                },
                ShopEntry {
                    item: "dangu:enhance_stone".into(),
                    price: 3_000,
                },
            ],
            smith_job: "대장장이".into(),
        }
    }
}

// ---- JSON ----

fn n(v: f64) -> Json {
    Json::Number(v)
}
fn s(v: &str) -> Json {
    Json::String(v.to_owned())
}
fn obj(f: Vec<(&str, Json)>) -> Json {
    Json::Object(f.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
fn arr<T>(v: &[T], f: impl Fn(&T) -> Json) -> Json {
    Json::Array(v.iter().map(f).collect())
}
fn stats_json(st: &Stats) -> Json {
    Json::Object(
        st.entries()
            .into_iter()
            .map(|(k, v)| (k.key().to_owned(), n(v)))
            .collect(),
    )
}
fn stats_from(j: Option<&Json>) -> Stats {
    let mut out = Stats::default();
    if let Some(Json::Object(f)) = j {
        for (k, v) in f {
            if let (Some(stat), Some(v)) = (Stat::parse(k), v.as_f64()) {
                out[stat] += v;
            }
        }
    }
    out
}
fn pairs_json(v: &[(String, i32)]) -> Json {
    Json::Object(v.iter().map(|(k, c)| (k.clone(), n(*c as f64))).collect())
}
fn pairs_from(j: Option<&Json>) -> Vec<(String, i32)> {
    match j {
        Some(Json::Object(f)) => f
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_f64()? as i32)))
            .collect(),
        _ => Vec::new(),
    }
}

pub fn encode(c: &EquipConfig) -> Json {
    obj(vec![
        ("smith_job", s(&c.smith_job)),
        (
            "enhance",
            obj(vec![
                ("chances", arr(&c.enhance.chances, |p| n(*p))),
                ("per_level", n(c.enhance.per_level)),
                ("milestone", n(c.enhance.milestone)),
                ("money", n(c.enhance.money as f64)),
                ("stone", s(&c.enhance.stone)),
            ]),
        ),
        (
            "scrolls",
            arr(&c.scrolls, |x| {
                obj(vec![
                    ("item", s(&x.item)),
                    ("name", s(&x.name)),
                    ("min_lines", n(x.min_lines as f64)),
                    ("max_lines", n(x.max_lines as f64)),
                    ("mult", n(x.mult)),
                ])
            }),
        ),
        (
            "potentials",
            arr(&c.potentials, |p| {
                obj(vec![
                    ("stat", s(p.stat.key())),
                    ("weight", n(p.weight as f64)),
                    ("min", n(p.min)),
                    ("max", n(p.max)),
                ])
            }),
        ),
        (
            "shop",
            arr(&c.shop, |e| {
                obj(vec![("item", s(&e.item)), ("price", n(e.price as f64))])
            }),
        ),
        (
            "sets",
            arr(&c.sets, |x| {
                obj(vec![
                    ("id", s(&x.id)),
                    ("name", s(&x.name)),
                    ("color", s(&x.color)),
                    (
                        "bonuses",
                        arr(&x.bonuses, |(need, b)| {
                            obj(vec![("pieces", n(*need as f64)), ("stats", stats_json(b))])
                        }),
                    ),
                ])
            }),
        ),
        (
            "gear",
            arr(&c.gear, |g| {
                let mut f = vec![
                    ("id", s(&g.id)),
                    ("name", s(&g.name)),
                    ("part", s(g.part.key())),
                    ("tier", n(g.tier as f64)),
                    ("grade", s(&g.grade)),
                    ("stats", stats_json(&g.stats)),
                    ("palette", s(&g.palette)),
                    ("lore", s(&g.lore)),
                ];
                if let Some(set) = &g.set {
                    f.push(("set", s(set)));
                }
                if g.potential_tries > 0 {
                    f.push(("potential_tries", n(g.potential_tries as f64)));
                }
                obj(f)
            }),
        ),
        (
            "materials",
            arr(&c.materials, |m| {
                obj(vec![
                    ("id", s(&m.id)),
                    ("name", s(&m.name)),
                    ("color", s(&m.color)),
                    ("grade", s(&m.grade)),
                    ("lore", arr(&m.lore, |l| s(l))),
                    ("art", s(&m.art)),
                    ("palette", s(&m.palette)),
                ])
            }),
        ),
        (
            "recipes",
            arr(&c.recipes, |r| {
                obj(vec![
                    ("result", s(&r.result)),
                    ("count", n(r.count as f64)),
                    ("inputs", pairs_json(&r.inputs)),
                    ("money", n(r.money as f64)),
                    ("smith_level", n(r.smith_level as f64)),
                    ("xp", n(r.xp)),
                    ("category", s(&r.category)),
                ])
            }),
        ),
        (
            "ores",
            arr(&c.ores, |o| {
                obj(vec![
                    ("block", s(&o.block)),
                    ("name", s(&o.name)),
                    ("drop", s(&o.drop)),
                    ("nether", Json::Bool(o.nether)),
                    ("min_y", n(o.min_y as f64)),
                    ("max_y", n(o.max_y as f64)),
                    ("tries", n(o.tries as f64)),
                    ("size", n(o.size as f64)),
                    ("xp", n(o.xp)),
                    ("pickaxe", s(&o.pickaxe)),
                    ("palette", s(&o.palette)),
                ])
            }),
        ),
        (
            "bosses",
            arr(&c.bosses, |b| {
                obj(vec![
                    ("id", s(&b.id)),
                    ("name", s(&b.name)),
                    ("mob", s(&b.mob)),
                    ("health", n(b.health as f64)),
                    ("damage", n(b.damage as f64)),
                    ("spawn_chance", n(b.spawn_chance)),
                    ("raid", Json::Bool(b.raid)),
                    (
                        "drops",
                        arr(&b.drops, |d| {
                            obj(vec![
                                ("item", s(&d.item)),
                                ("chance", n(d.chance)),
                                ("min", n(d.min as f64)),
                                ("max", n(d.max as f64)),
                            ])
                        }),
                    ),
                ])
            }),
        ),
    ])
}

/// 빠진 칸은 기본값으로.
pub fn decode(j: &Json) -> Option<EquipConfig> {
    let d = EquipConfig::default();
    let st_ = |j: &Json, k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
    let num = |j: &Json, k: &str, def: f64| j.get(k).and_then(Json::as_f64).unwrap_or(def);
    let list = |k: &str| j.get(k).and_then(Json::as_array);
    let enhance = match j.get("enhance") {
        Some(e) => EnhanceRule {
            chances: e
                .get("chances")
                .and_then(Json::as_array)
                .map(|a| a.iter().filter_map(Json::as_f64).collect())
                .unwrap_or(d.enhance.chances.clone()),
            per_level: num(e, "per_level", d.enhance.per_level),
            milestone: num(e, "milestone", d.enhance.milestone),
            money: num(e, "money", d.enhance.money as f64) as i64,
            stone: e
                .get("stone")
                .and_then(Json::as_str)
                .map_or(d.enhance.stone.clone(), str::to_owned),
        },
        None => d.enhance.clone(),
    };
    Some(EquipConfig {
        smith_job: j
            .get("smith_job")
            .and_then(Json::as_str)
            .map_or(d.smith_job.clone(), str::to_owned),
        enhance,
        scrolls: list("scrolls").map_or(d.scrolls.clone(), |a| {
            a.iter()
                .map(|x| ScrollDef {
                    item: st_(x, "item"),
                    name: st_(x, "name"),
                    min_lines: num(x, "min_lines", 1.0) as u8,
                    max_lines: num(x, "max_lines", 1.0) as u8,
                    mult: num(x, "mult", 1.0),
                })
                .collect()
        }),
        potentials: list("potentials").map_or(d.potentials.clone(), |a| {
            a.iter()
                .filter_map(|p| {
                    Some(PotentialOption {
                        stat: Stat::parse(p.get("stat")?.as_str()?)?,
                        weight: num(p, "weight", 1.0) as u32,
                        min: num(p, "min", 0.0),
                        max: num(p, "max", 0.0),
                    })
                })
                .collect()
        }),
        shop: list("shop").map_or(d.shop.clone(), |a| {
            a.iter()
                .map(|e| ShopEntry {
                    item: st_(e, "item"),
                    price: num(e, "price", 0.0) as i64,
                })
                .collect()
        }),
        sets: list("sets").map_or(d.sets.clone(), |a| {
            a.iter()
                .map(|x| SetDef {
                    id: st_(x, "id"),
                    name: st_(x, "name"),
                    color: st_(x, "color"),
                    bonuses: x
                        .get("bonuses")
                        .and_then(Json::as_array)
                        .unwrap_or_default()
                        .iter()
                        .map(|b| (num(b, "pieces", 2.0) as u8, stats_from(b.get("stats"))))
                        .collect(),
                })
                .collect()
        }),
        gear: list("gear").map_or(d.gear.clone(), |a| {
            a.iter()
                .filter_map(|g| {
                    Some(GearDef {
                        id: st_(g, "id"),
                        name: st_(g, "name"),
                        part: Part::parse(g.get("part")?.as_str()?)?,
                        set: g.get("set").and_then(Json::as_str).map(str::to_owned),
                        tier: num(g, "tier", 1.0) as u8,
                        grade: st_(g, "grade"),
                        stats: stats_from(g.get("stats")),
                        potential_tries: num(g, "potential_tries", 0.0) as u8,
                        palette: st_(g, "palette"),
                        lore: st_(g, "lore"),
                    })
                })
                .collect()
        }),
        materials: list("materials").map_or(d.materials.clone(), |a| {
            a.iter()
                .map(|m| Material {
                    id: st_(m, "id"),
                    name: st_(m, "name"),
                    color: st_(m, "color"),
                    grade: st_(m, "grade"),
                    lore: m
                        .get("lore")
                        .and_then(Json::as_array)
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|l| l.as_str().map(str::to_owned))
                        .collect(),
                    art: st_(m, "art"),
                    palette: st_(m, "palette"),
                })
                .collect()
        }),
        recipes: list("recipes").map_or(d.recipes.clone(), |a| {
            a.iter()
                .map(|r| Recipe {
                    result: st_(r, "result"),
                    count: num(r, "count", 1.0) as i32,
                    inputs: pairs_from(r.get("inputs")),
                    money: num(r, "money", 0.0) as i64,
                    smith_level: num(r, "smith_level", 0.0) as u32,
                    xp: num(r, "xp", 0.0),
                    category: st_(r, "category"),
                })
                .collect()
        }),
        ores: list("ores").map_or(d.ores.clone(), |a| {
            a.iter()
                .map(|o| OreDef {
                    block: st_(o, "block"),
                    name: st_(o, "name"),
                    drop: st_(o, "drop"),
                    nether: o.get("nether").and_then(Json::as_bool).unwrap_or(false),
                    min_y: num(o, "min_y", 0.0) as i32,
                    max_y: num(o, "max_y", 64.0) as i32,
                    tries: num(o, "tries", 1.0) as u32,
                    size: num(o, "size", 3.0) as u32,
                    xp: num(o, "xp", 0.0),
                    pickaxe: st_(o, "pickaxe"),
                    palette: st_(o, "palette"),
                })
                .collect()
        }),
        bosses: list("bosses").map_or(d.bosses.clone(), |a| {
            a.iter()
                .map(|b| BossDef {
                    id: st_(b, "id"),
                    name: st_(b, "name"),
                    mob: st_(b, "mob"),
                    health: num(b, "health", 100.0) as f32,
                    damage: num(b, "damage", 1.0) as f32,
                    spawn_chance: num(b, "spawn_chance", 0.0),
                    raid: b.get("raid").and_then(Json::as_bool).unwrap_or(false),
                    drops: b
                        .get("drops")
                        .and_then(Json::as_array)
                        .unwrap_or_default()
                        .iter()
                        .map(|x| Drop {
                            item: st_(x, "item"),
                            chance: num(x, "chance", 0.0),
                            min: num(x, "min", 1.0) as i32,
                            max: num(x, "max", 1.0) as i32,
                        })
                        .collect(),
                })
                .collect()
        }),
    })
}

static FILE: OnceLock<JsonFile<EquipConfig>> = OnceLock::new();

fn file() -> &'static JsonFile<EquipConfig> {
    FILE.get_or_init(|| JsonFile::in_memory(encode, decode))
}

/// 서버를 켤 때 한 번 (콘텐츠를 읽기 전에 — 아이템 목록이 이 설정에서 나옴).
pub fn open_global() {
    let _ = FILE.set(JsonFile::open(
        data_path("MC_EQUIPMENT", "equipment.json"),
        encode,
        decode,
    ));
    if let Some(p) = file().path()
        && !p.exists()
    {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, encode(&EquipConfig::default()).pretty());
    }
}

pub fn with<R>(f: impl FnOnce(&EquipConfig) -> R) -> R {
    file().read(f)
}

pub fn get() -> EquipConfig {
    file().read(|c| c.clone())
}

pub fn set(c: EquipConfig) -> Result<(), String> {
    c.validate()?;
    file().write(|old| *old = c);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_round_trip() {
        let c = EquipConfig::default();
        c.validate().unwrap();
        assert_eq!(decode(&encode(&c)).unwrap(), c);
        // 7세트 × 6부위 + 제작 악세 18 + 보스 악세 9
        assert_eq!(c.gear.len(), 42 + 18 + 9);
        assert_eq!(c.ores.len(), 6);
        assert_eq!(c.enhance_chance(0), 0.95);
        assert_eq!(c.enhance_chance(29), 0.01);
        assert_eq!(c.enhance_chance(30), 0.0);
        assert_eq!(c.enhance_cost(9), (100_000, 4));
        // 모든 조합법의 재료와 결과가 있는 아이템
        let known = |id: &str| !id.starts_with("dangu:") || c.item_name(id).is_some();
        for r in &c.recipes {
            assert!(known(&r.result), "{}", r.result);
            for (i, _) in &r.inputs {
                assert!(known(i), "{i}");
                if !i.starts_with("dangu:") {
                    assert!(crate::data::items::by_name(i).is_some(), "{i}");
                }
            }
        }
        for b in &c.bosses {
            for d in &b.drops {
                assert!(known(&d.item), "{}", d.item);
            }
        }
        // 보스 악세가 같은 자리 제작 악세보다 좋음 (고블린 왕 > 오리하르콘)
        let cc = |id: &str| c.gear_def(id).unwrap().stats[Stat::CritChance];
        assert!(cc("dangu:goblin_king_ring") > cc("dangu:orichalcum_ring"));
        assert!(cc("dangu:abyss_lord_ring") > cc("dangu:evilium_ring"));
    }
}
