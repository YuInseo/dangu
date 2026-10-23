//! 퀵메뉴 (기획 "퀵메뉴"): Shift+F 또는 `/메뉴`.
//!
//! ```text
//! ┌ 메인: 서버 이동 ───────────────────────────────┐
//! │  광장 (Shift+클릭: 워프)  마을  야생  던전 ▸  레이드 │
//! ├ 서브 (왼쪽부터) ───────────────────────────────┤
//! │ 내 정보 · 길라잡이 · 우편함 · 후원 · 위키/디스코드/마인리스트 · 설정 · 쓰레기통 │
//! └────────────────────────────────────────────────┘
//! ```
//!
//! - 내 정보: 이름·등급·길드·전투력·재화가 적힌 종이, 생활 스탯(`/직업`), 모험가 스탯(`/모험가`), 도감.
//! - 설정: HUD, 갑옷 숨기기, 채팅 채널 켜고 끄기 (BGM·스코어보드는 기획에서 뺌).
//! - 쓰레기통(`/쓰레기통`): 넣고 닫으면 사라짐.
//! - 이동할 곳·워프·던전·링크·길라잡이는 `world/menu.json` (`MC_MENU`, 웹 관리 화면 JSON 탭).
//!   인스턴스가 비었거나 없는 곳은 "준비 중".

pub mod gui;

use std::sync::OnceLock;

use crate::json::Json;
use crate::plugins::store::{JsonFile, data_path};

/// 메뉴에서 갈 수 있는 곳 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct Dest {
    pub id: String,
    pub name: String,
    /// 아이콘 (바닐라 아이템 이름)
    pub icon: String,
    pub lore: Vec<String>,
    /// 옮겨 갈 인스턴스 (비면 "준비 중")
    pub instance: String,
    /// 같은 인스턴스 안에서 갈 자리 (없으면 스폰)
    pub pos: Option<(f64, f64, f64)>,
    /// 이동 대신 칠 명령어 (`대장간`). `@warps` `@dungeons`면 그 메뉴를 엶.
    pub command: String,
    /// Shift+클릭이면 칠 명령어 (광장 → `@warps`).
    pub shift_command: String,
}

impl Dest {
    fn new(id: &str, name: &str, icon: &str, instance: &str, lore: &[&str]) -> Dest {
        Dest {
            id: id.into(),
            name: name.into(),
            icon: icon.into(),
            lore: lore.iter().map(|s| (*s).to_owned()).collect(),
            instance: instance.into(),
            pos: None,
            command: String::new(),
            shift_command: String::new(),
        }
    }

    fn command(mut self, c: &str) -> Dest {
        self.command = c.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuConfig {
    /// 메인 (서버 이동)
    pub main: Vec<Dest>,
    pub dungeons: Vec<Dest>,
    /// 광장 상세 워프
    pub warps: Vec<Dest>,
    pub wiki: String,
    pub discord: String,
    pub minelist: String,
    /// 길라잡이: 채팅에 보일 줄들
    pub guide: Vec<String>,
}

impl Default for MenuConfig {
    fn default() -> Self {
        let mut square = Dest::new(
            "square",
            "광장",
            "beacon",
            "square",
            &["서버의 중심. 상점·대장장이·경매장이 모여 있음"],
        );
        square.shift_command = "@warps".into();
        MenuConfig {
            main: vec![
                square,
                Dest::new(
                    "town",
                    "마을",
                    "bell",
                    "town",
                    &["길드 땅을 사고 마을을 꾸미는 곳"],
                ),
                Dest::new(
                    "wild",
                    "야생",
                    "grass_block",
                    "wild",
                    &["광물·나무를 모으는 곳 (주기적으로 초기화)"],
                ),
                Dest::new(
                    "dungeon",
                    "던전",
                    "iron_sword",
                    "",
                    &["모험가 길드 · 일반 던전 · 성장 던전"],
                )
                .command("@dungeons"),
                Dest::new(
                    "raid",
                    "레이드",
                    "wither_skeleton_skull",
                    "",
                    &["보스 레이드"],
                ),
            ],
            dungeons: vec![
                Dest::new(
                    "adventurer_guild",
                    "모험가 길드",
                    "lectern",
                    "",
                    &["의뢰를 받고 던전으로"],
                ),
                Dest::new(
                    "dungeon_normal",
                    "일반 던전",
                    "mossy_cobblestone",
                    "",
                    &["보스와 중급 주문서"],
                ),
                Dest::new(
                    "dungeon_growth",
                    "성장 던전",
                    "experience_bottle",
                    "",
                    &["레벨을 올리는 던전"],
                ),
            ],
            warps: vec![
                Dest::new("shop", "상점", "emerald", "", &[]),
                Dest::new("afk", "잠수", "clock", "", &["가만히 있으면 잠수 포인트"]),
                Dest::new("fishing", "낚시터", "cod", "", &[]),
                Dest::new(
                    "smith",
                    "대장장이 (강화)",
                    "anvil",
                    "",
                    &["대장간 · 강화를 바로 엶"],
                )
                .command("대장간"),
                Dest::new("cosmetic", "치장", "leather_chestplate", "", &[]),
                Dest::new("cash_shop", "캐시상점", "diamond", "", &[]),
                Dest::new("trade_island", "무역섬", "oak_boat", "", &[]),
                Dest::new("auction", "경매장", "gold_ingot", "", &[]),
                Dest::new("afk_mine", "잠광", "iron_pickaxe", "", &["잠수 광산"]),
            ],
            wiki: String::new(),
            discord: String::new(),
            minelist: String::new(),
            guide: vec![
                "1. /퀘스트 — 일일 납품부터 해 보세요 (보상: 자동심기·플라이)".into(),
                "2. /길드 — 길드를 만들거나 들어가 마을 땅을 사세요".into(),
                "3. /대장간 — 광물을 주괴로 녹여 장비를 만들고 /강화".into(),
                "4. /장비 — 악세서리를 끼고 능력치를 확인".into(),
                "5. Shift+F — 언제든 이 메뉴".into(),
            ],
        }
    }
}

// ---- JSON ----

fn s(v: &str) -> Json {
    Json::String(v.to_owned())
}

fn dest_json(d: &Dest) -> Json {
    let mut f = vec![
        ("id".to_owned(), s(&d.id)),
        ("name".into(), s(&d.name)),
        ("icon".into(), s(&d.icon)),
        (
            "lore".into(),
            Json::Array(d.lore.iter().map(|l| s(l)).collect()),
        ),
        ("instance".into(), s(&d.instance)),
    ];
    if let Some((x, y, z)) = d.pos {
        f.push((
            "pos".into(),
            Json::Array(vec![Json::Number(x), Json::Number(y), Json::Number(z)]),
        ));
    }
    if !d.command.is_empty() {
        f.push(("command".into(), s(&d.command)));
    }
    if !d.shift_command.is_empty() {
        f.push(("shift_command".into(), s(&d.shift_command)));
    }
    Json::Object(f)
}

fn dest_from(j: &Json) -> Option<Dest> {
    let st = |k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
    let pos = j.get("pos").and_then(Json::as_array).and_then(|p| {
        let f = |i: usize| p.get(i).and_then(Json::as_f64);
        Some((f(0)?, f(1)?, f(2)?))
    });
    Some(Dest {
        id: j.get("id")?.as_str()?.to_owned(),
        name: st("name"),
        icon: st("icon"),
        lore: j
            .get("lore")
            .and_then(Json::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(|l| l.as_str().map(str::to_owned))
            .collect(),
        instance: st("instance"),
        pos,
        command: st("command"),
        shift_command: st("shift_command"),
    })
}

pub fn encode(c: &MenuConfig) -> Json {
    let list = |l: &[Dest]| Json::Array(l.iter().map(dest_json).collect());
    Json::Object(vec![
        ("main".into(), list(&c.main)),
        ("dungeons".into(), list(&c.dungeons)),
        ("warps".into(), list(&c.warps)),
        ("wiki".into(), s(&c.wiki)),
        ("discord".into(), s(&c.discord)),
        ("minelist".into(), s(&c.minelist)),
        (
            "guide".into(),
            Json::Array(c.guide.iter().map(|l| s(l)).collect()),
        ),
    ])
}

pub fn decode(j: &Json) -> Option<MenuConfig> {
    let d = MenuConfig::default();
    let list = |k: &str, def: &[Dest]| match j.get(k).and_then(Json::as_array) {
        Some(a) => a.iter().filter_map(dest_from).collect(),
        None => def.to_vec(),
    };
    let st = |k: &str| j.get(k).and_then(Json::as_str).unwrap_or("").to_owned();
    Some(MenuConfig {
        main: list("main", &d.main),
        dungeons: list("dungeons", &d.dungeons),
        warps: list("warps", &d.warps),
        wiki: st("wiki"),
        discord: st("discord"),
        minelist: st("minelist"),
        guide: match j.get("guide").and_then(Json::as_array) {
            Some(a) => a
                .iter()
                .filter_map(|l| l.as_str().map(str::to_owned))
                .collect(),
            None => d.guide,
        },
    })
}

impl MenuConfig {
    /// 메인 5칸, 던전 7칸, 워프 21칸까지 (창에 들어가는 만큼).
    pub fn validate(&self) -> Result<(), String> {
        if self.main.len() > 7 || self.dungeons.len() > 7 || self.warps.len() > 21 {
            return Err("메인·던전은 7개, 워프는 21개까지".into());
        }
        for d in self.main.iter().chain(&self.dungeons).chain(&self.warps) {
            if d.id.is_empty() || d.name.is_empty() {
                return Err("id와 이름은 비울 수 없습니다".into());
            }
        }
        Ok(())
    }
}

static FILE: OnceLock<JsonFile<MenuConfig>> = OnceLock::new();

fn file() -> &'static JsonFile<MenuConfig> {
    FILE.get_or_init(|| JsonFile::in_memory(encode, decode))
}

pub fn with<R>(f: impl FnOnce(&MenuConfig) -> R) -> R {
    file().read(f)
}

pub fn get() -> MenuConfig {
    with(|c| c.clone())
}

pub fn set(c: MenuConfig) -> Result<(), String> {
    c.validate()?;
    file().write(|old| *old = c);
    Ok(())
}

pub fn open_global() {
    let _ = FILE.set(JsonFile::open(
        data_path("MC_MENU", "menu.json"),
        encode,
        decode,
    ));
    if let Some(p) = file().path()
        && !p.exists()
    {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, encode(&MenuConfig::default()).pretty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_plan_and_round_trip() {
        let c = MenuConfig::default();
        c.validate().unwrap();
        assert_eq!(decode(&encode(&c)).unwrap(), c);
        let names: Vec<_> = c.main.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["광장", "마을", "야생", "던전", "레이드"]);
        assert_eq!(c.main[0].shift_command, "@warps");
        assert_eq!(c.dungeons.len(), 3);
        assert_eq!(c.warps.len(), 9);
    }
}
