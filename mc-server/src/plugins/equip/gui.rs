//! 창: `/장비` (악세서리 칸·능력치·세트) · `/강화` · `/대장간` (제련·장비·악세서리·재료·상점).
//!
//! 모두 메뉴 창이라 아이템을 직접 끌어 옮기지 않고, 칸을 누르면 그 일을 합니다.
//! 아래 가방(창 칸 54..90)을 누르면: 장비 창은 그 악세서리를 끼고, 강화 창은 그 장비를 고릅니다.
//!
//! ```text
//! 장비 창                         강화 창                     대장간
//! ┌─────────────────────┐        ┌──────────────────┐        ┌───────────────────────┐
//! │ 모자   무기   반지  능력치 │        │      [장비]       │        │ 제련 장비 악세 재료 상점 │
//! │ 상의          목걸이 세트   │        │ [정보] [강화] [다음]│        │ (조합법 36칸)           │
//! │ 하의          귀걸이 잠재   │        │                  │        │ ◀  대장장이 Lv  ▶      │
//! │ 신발                      │        │       ◀ 장비      │        └───────────────────────┘
//! │      [강화]  [대장간]      │        └──────────────────┘
//! └─────────────────────┘
//! ```

use tokio::io::AsyncWrite;

use super::config::{self, Recipe, grade_color};
use super::session::{gear_of, item_of};
use super::store::gears;
use super::{MAX_ENHANCE, Part, enhance_multiplier};
use crate::data::menu;
use crate::game::custom_items::{self, Look};
use crate::packets::container as pc;
use crate::packets::world::Item;
use crate::plugins::hud::number;
use crate::protocol::frame::FrameError;
use crate::protocol::nbt::Tag;
use crate::server::Event;
use crate::session::play::Session;
use crate::session::windows::{Window, WindowKind};
use crate::text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Gear,
    Enhance { serial: Option<u32> },
    Smith { tab: usize, page: usize },
}

impl Page {
    fn title(&self) -> &'static str {
        match self {
            Page::Gear => "장비",
            Page::Enhance { .. } => "장비 강화",
            Page::Smith { .. } => "대장간",
        }
    }
}

pub const TABS: [&str; 5] = ["제련", "장비", "악세서리", "재료", "상점"];
const TAB_SLOTS: [usize; 5] = [2, 3, 4, 5, 6];
const TAB_ICONS: [&str; 5] = [
    "blast_furnace",
    "iron_chestplate",
    "gold_nugget",
    "amethyst_shard",
    "emerald",
];

// 장비 창
const ARMOR_SLOTS: [usize; 4] = [10, 19, 28, 37];
const WEAPON_SLOT: usize = 12;
const ACC_SLOTS: [usize; 3] = [14, 23, 32];
const STATS_SLOT: usize = 16;
const SETS_SLOT: usize = 25;
const POT_SLOT: usize = 34;
const TO_ENHANCE: usize = 48;
const TO_SMITH: usize = 50;

// 강화 창
const ENH_ITEM: usize = 13;
const ENH_INFO: usize = 29;
const ENH_BUTTON: usize = 31;
const ENH_NEXT: usize = 33;
const BACK: usize = 49;

// 대장간
const LIST_FIRST: usize = 9;
const LIST_PER_PAGE: usize = 36;
const PREV: usize = 45;
const NEXT: usize = 53;
const SMITH_INFO: usize = 49;

/// 창 아래 가방 칸(54..90) → 인벤토리 칸.
fn inventory_slot(slot: usize) -> Option<usize> {
    match slot {
        54..=80 => Some(slot - 54 + 9),
        81..=89 => Some(slot - 81 + 36),
        _ => None,
    }
}

fn span(t: &str, color: &str) -> Tag {
    Tag::Compound(vec![
        ("text".into(), Tag::String(t.to_owned())),
        ("color".into(), Tag::String(color.to_owned())),
        ("italic".into(), Tag::Byte(0)),
    ])
}

/// 조합법에 쓰는 바닐라 재료의 한글 이름.
fn vanilla_name(k: &str) -> String {
    match k {
        "stick" => "막대기",
        "amethyst_shard" => "자수정 조각",
        "gold_ingot" => "금 주괴",
        "gold_nugget" => "금 조각",
        "iron_ingot" => "철 주괴",
        "diamond" => "다이아몬드",
        "string" => "실",
        other => other,
    }
    .to_owned()
}

fn recipes_of(tab: usize) -> Vec<Recipe> {
    config::with(|c| {
        c.recipes
            .iter()
            .filter(|r| r.category == TABS[tab])
            .cloned()
            .collect()
    })
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    pub(crate) async fn open_equip(&mut self, page: Page) -> std::io::Result<()> {
        self.close_window(false).await?;
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        let title = page.title();
        self.window = Some(Window {
            id,
            kind: WindowKind::Equip(page),
            grid: [None; 9],
        });
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, menu::GENERIC_9X6, &text::plain(title)),
        )
        .await?;
        self.send_window().await
    }

    /// 진짜 아이템 모양에 다른 이름·설명을 붙인 아이콘 (조합법·버튼).
    fn item_icon(
        &self,
        slot: usize,
        base: Item,
        name: Tag,
        lore: Vec<Tag>,
        glint: bool,
    ) -> Option<Item> {
        let serial = self.icon_serial(slot);
        custom_items::set_look(
            serial,
            Look {
                name: None,
                lore: Vec::new(),
                glint: Some(glint),
                rich: Some((name, lore)),
            },
        );
        // 바닐라 아이템이면 메뉴 아이콘용 사용자 정의 아이템을 빌림 (겉모습만 바꾸려고)
        let custom = base
            .custom
            .or_else(|| custom_items::index_of(crate::plugins::lottery::ICON_ITEM));
        Some(Item {
            id: base.id,
            count: base.count.clamp(1, 64),
            custom,
            serial,
        })
    }

    pub(crate) fn equip_slots(&self, page: &Page) -> Vec<Option<Item>> {
        let mut slots: Vec<Option<Item>> = (0..54).map(|s| self.pane(s)).collect();
        match page {
            Page::Gear => self.gear_page(&mut slots),
            Page::Enhance { serial } => self.enhance_page(&mut slots, *serial),
            Page::Smith { tab, page } => self.smith_page(&mut slots, *tab, *page),
        }
        slots
    }

    fn gear_page(&self, slots: &mut [Option<Item>]) {
        for (i, &s) in ARMOR_SLOTS.iter().enumerate() {
            slots[s] = self.inventory[5 + i].or_else(|| {
                self.icon(
                    s,
                    "light_gray_stained_glass_pane",
                    &format!(
                        "{} 칸 (비었음)",
                        [Part::Helmet, Part::Chest, Part::Legs, Part::Boots][i].name()
                    ),
                    "gray",
                    vec!["가방에서 입으세요".into()],
                    false,
                )
            });
        }
        slots[WEAPON_SLOT] = self.main_hand().or_else(|| {
            self.icon(
                WEAPON_SLOT,
                "light_gray_stained_glass_pane",
                "손 (비었음)",
                "gray",
                Vec::new(),
                false,
            )
        });
        let worn = gears().read(|d| d.worn(self.profile.uuid));
        for (i, &s) in ACC_SLOTS.iter().enumerate() {
            let part = Part::ACCESSORIES[i];
            slots[s] = worn[i]
                .and_then(|serial| self.serial_item(serial))
                .or_else(|| {
                    self.icon(
                        s,
                        "purple_stained_glass_pane",
                        &format!("{} 칸", part.name()),
                        "light_purple",
                        vec![
                            format!("아래 가방의 {}을(를) 누르면 낍니다", part.name()),
                            "낀 것을 누르면 뺍니다".into(),
                        ],
                        false,
                    )
                });
        }
        // 능력치
        let stats = self.rpg_stats(self.hud.job_levels());
        let line =
            |k: &str, v: String, c: &str| vec![span(&format!("{k}  "), "#8c8c9a"), span(&v, c)];
        let join = |parts: Vec<Tag>| {
            Tag::Compound(vec![
                ("text".into(), Tag::String(String::new())),
                ("italic".into(), Tag::Byte(0)),
                ("extra".into(), Tag::List(parts)),
            ])
        };
        let lore = vec![
            join(line("레벨", stats.level.to_string(), "#ffd866")),
            join(line("공격력", number(stats.attack as i64), "white")),
            join(line("마법력", number(stats.magic as i64), "white")),
            join(line("방어력", number(stats.defense as i64), "white")),
            join(line("체력", number(stats.max_health as i64), "#ff7a7a")),
            join(line("마력", number(stats.max_mana as i64), "#7ab8ff")),
            join(line(
                "치명타 확률",
                format!("{:.1}%", stats.crit_chance),
                "white",
            )),
            join(line(
                "크리티컬 피해",
                format!("{:.0}%", stats.crit_damage),
                "white",
            )),
            join(line(
                "받는 피해",
                format!("{:.0}%", super::after_defense(100.0, stats.defense as f64)),
                "white",
            )),
            span(" ", "gray"),
            join(line("전투력", number(stats.combat_power()), "#ff9f43")),
        ];
        slots[STATS_SLOT] = self.item_icon(
            STATS_SLOT,
            Item::new(crate::data::items::by_name("writable_book").unwrap_or(1), 1),
            span("능력치", "gold"),
            lore,
            false,
        );
        // 세트 효과
        let (_, sets, _) = self.equip_bonus();
        let mut lore = Vec::new();
        config::with(|cfg| {
            for (id, n) in &sets {
                let Some(set) = cfg.set(id) else { continue };
                lore.push(span(&format!("◆ {} ({n}개)", set.name), &set.color));
                for (need, b) in &set.bonuses {
                    let on = n >= need;
                    let t = b
                        .entries()
                        .iter()
                        .map(|(k, v)| k.show(*v))
                        .collect::<Vec<_>>()
                        .join(" · ");
                    lore.push(span(
                        &format!("  {need}개  {t}"),
                        if on { "#7be07b" } else { "#5a5a66" },
                    ));
                }
            }
        });
        if lore.is_empty() {
            lore.push(span("같은 세트를 2개 이상 입으면 효과가 켜집니다", "gray"));
        }
        slots[SETS_SLOT] = self.item_icon(
            SETS_SLOT,
            Item::new(crate::data::items::by_name("nether_star").unwrap_or(1), 1),
            span("세트 효과", "aqua"),
            lore,
            !sets.iter().all(|(_, n)| *n < 2),
        );
        // 잠재능력 합
        let mut pot = super::Stats::default();
        gears().read(|d| {
            for s in d.worn(self.profile.uuid).iter().flatten() {
                if let Some(g) = d.gears.get(s) {
                    for &(k, v) in &g.potential {
                        pot[k] += v;
                    }
                }
            }
        });
        let mut lore: Vec<Tag> = pot
            .entries()
            .into_iter()
            .map(|(k, v)| span(&k.show(v), "#c77dff"))
            .collect();
        if lore.is_empty() {
            lore.push(span("주문서를 집어 악세서리 위에 놓으면 붙습니다", "gray"));
        }
        slots[POT_SLOT] = self.item_icon(
            POT_SLOT,
            Item::new(
                crate::data::items::by_name("amethyst_shard").unwrap_or(1),
                1,
            ),
            span("잠재능력 합계", "light_purple"),
            lore,
            false,
        );
        slots[TO_ENHANCE] = self.icon(
            TO_ENHANCE,
            "anvil",
            "강화",
            "gold",
            vec!["장비를 +30까지".into()],
            false,
        );
        slots[TO_SMITH] = self.icon(
            TO_SMITH,
            "smithing_table",
            "대장간",
            "gold",
            vec!["제련 · 제작 · 상점".into()],
            false,
        );
    }

    /// 일련번호로 가방(또는 낀 악세)의 그 장비 아이템.
    fn serial_item(&self, serial: u32) -> Option<Item> {
        let def = gears().read(|d| d.gears.get(&serial).map(|g| g.def.clone()))?;
        let mut it = item_of(&def, 1)?;
        it.serial = serial;
        Some(it)
    }

    fn enhance_page(&self, slots: &mut [Option<Item>], serial: Option<u32>) {
        slots[BACK] = self.icon(BACK, "arrow", "장비 창으로", "white", Vec::new(), false);
        let found = serial
            .and_then(|s| (9..46).find(|&i| self.inventory[i].is_some_and(|it| it.serial == s)));
        let Some(slot) = found else {
            slots[ENH_ITEM] = self.icon(
                ENH_ITEM,
                "light_gray_stained_glass_pane",
                "강화할 장비를 고르세요",
                "gray",
                vec!["아래 가방의 장비를 누르세요 (악세서리는 강화 없음)".into()],
                false,
            );
            return;
        };
        let item = self.inventory[slot].unwrap();
        slots[ENH_ITEM] = Some(item);
        let Some((def, gear)) = config::with(|c| gear_of(&item, c)) else {
            return;
        };
        let level = gear.as_ref().map_or(0, |g| g.enhance);
        let (chance, (money, stones), stone, per, mile) = config::with(|c| {
            (
                c.enhance_chance(level),
                c.enhance_cost(level),
                c.enhance.stone.clone(),
                c.enhance.per_level,
                c.enhance.milestone,
            )
        });
        let have = self.count_key(&stone);
        let maxed = level >= MAX_ENHANCE;
        let info = vec![
            span(
                &format!("+{level} → +{}", (level + 1).min(MAX_ENHANCE)),
                "#ffd866",
            ),
            span(&format!("성공 확률  {:.0}%", chance * 100.0), "white"),
            span(&format!("돈  {}원", number(money)), "white"),
            span(
                &format!("강화석  {have} / {stones}"),
                if have >= stones { "#7be07b" } else { "#ff5a5a" },
            ),
            span("실패해도 단계는 내려가지 않습니다", "dark_gray"),
        ];
        slots[ENH_INFO] = self.item_icon(
            ENH_INFO,
            Item::new(crate::data::items::by_name("paper").unwrap_or(1), 1),
            span("강화 정보", "gold"),
            info,
            false,
        );
        let now = enhance_multiplier(level, per, mile);
        let next = enhance_multiplier((level + 1).min(MAX_ENHANCE), per, mile);
        let preview: Vec<Tag> = def
            .stats
            .entries()
            .into_iter()
            .filter(|(k, _)| k.is_main())
            .map(|(k, v)| {
                span(
                    &format!("{}  {:.0} → {:.0}", k.name(), v * now, v * next),
                    "#7be07b",
                )
            })
            .collect();
        slots[ENH_NEXT] = self.item_icon(
            ENH_NEXT,
            Item::new(
                crate::data::items::by_name("experience_bottle").unwrap_or(1),
                1,
            ),
            span("성공하면", "green"),
            preview,
            false,
        );
        slots[ENH_BUTTON] = if maxed {
            self.icon(
                ENH_BUTTON,
                "nether_star",
                "최대 강화 (+30)",
                "gold",
                Vec::new(),
                true,
            )
        } else if def.part.is_accessory() {
            self.icon(
                ENH_BUTTON,
                "barrier",
                "악세서리는 강화할 수 없습니다",
                "red",
                Vec::new(),
                false,
            )
        } else {
            self.icon(
                ENH_BUTTON,
                "anvil",
                &format!("강화하기 ({:.0}%)", chance * 100.0),
                "gold",
                vec![format!("{}원 · 강화석 {stones}개", number(money))],
                true,
            )
        };
    }

    fn smith_page(&self, slots: &mut [Option<Item>], tab: usize, page: usize) {
        for (i, (&s, icon)) in TAB_SLOTS.iter().zip(TAB_ICONS).enumerate() {
            let on = i == tab;
            slots[s] = self.icon(
                s,
                icon,
                TABS[i],
                if on { "gold" } else { "gray" },
                Vec::new(),
                on,
            );
        }
        slots[SMITH_INFO] = self.icon(
            SMITH_INFO,
            "anvil",
            "대장간",
            "gold",
            vec![
                "대장장이 레벨이 높을수록 좋은 장비를 만듭니다".into(),
                "만들 때마다 대장장이 경험치".into(),
            ],
            false,
        );
        if TABS[tab] == "상점" {
            let shop = config::with(|c| {
                c.shop
                    .iter()
                    .map(|e| (e.clone(), c.item_name(&e.item).unwrap_or(e.item.clone())))
                    .collect::<Vec<_>>()
            });
            for (i, (e, name)) in shop.iter().enumerate().take(LIST_PER_PAGE) {
                let s = LIST_FIRST + i;
                let Some(base) = item_of(&e.item, 1) else {
                    continue;
                };
                slots[s] = self.item_icon(
                    s,
                    base,
                    span(name, "white"),
                    vec![
                        span(&format!("{}원", number(e.price)), "#ffd866"),
                        span("좌클릭: 1개 · Shift+좌클릭: 10개", "gray"),
                    ],
                    false,
                );
            }
            return;
        }
        let list = recipes_of(tab);
        for (i, r) in list
            .iter()
            .enumerate()
            .skip(page * LIST_PER_PAGE)
            .take(LIST_PER_PAGE)
        {
            let s = LIST_FIRST + i - page * LIST_PER_PAGE;
            let Some(base) = item_of(&r.result, r.count) else {
                continue;
            };
            let (name, color, extra) = config::with(|c| {
                let name = c.item_name(&r.result).unwrap_or(r.result.clone());
                let (color, extra) = match c.gear_def(&r.result) {
                    Some(g) => (
                        grade_color(&g.grade).to_owned(),
                        g.stats
                            .entries()
                            .into_iter()
                            .map(|(k, v)| k.show(v))
                            .collect::<Vec<_>>(),
                    ),
                    None => (
                        c.material(&r.result)
                            .map_or("#ffffff".into(), |m| m.color.clone()),
                        Vec::new(),
                    ),
                };
                (name, color, extra)
            });
            let mut lore = Vec::new();
            for l in extra {
                lore.push(span(&l, "#c8c8d2"));
            }
            lore.push(span("재료", "#8c8c9a"));
            for (k, n) in &r.inputs {
                let have = self.count_key(k);
                let label = config::with(|c| c.item_name(k)).unwrap_or_else(|| vanilla_name(k));
                lore.push(span(
                    &format!(
                        "  {} {label}  {have}/{n}",
                        if have >= *n { "✔" } else { "✘" }
                    ),
                    if have >= *n { "#7be07b" } else { "#ff5a5a" },
                ));
            }
            if r.money > 0 {
                lore.push(span(&format!("  돈 {}원", number(r.money)), "#ffd866"));
            }
            if r.smith_level > 0 {
                lore.push(span(
                    &format!("대장장이 Lv.{} 필요", r.smith_level),
                    "#c77dff",
                ));
            }
            lore.push(span("좌클릭: 만들기", "gray"));
            let title = if r.count > 1 {
                format!("{name} ×{}", r.count)
            } else {
                name
            };
            slots[s] = self.item_icon(s, base, span(&title, &color), lore, false);
        }
        if page > 0 {
            slots[PREV] = self.icon(PREV, "arrow", "이전 쪽", "white", Vec::new(), false);
        }
        if (page + 1) * LIST_PER_PAGE < list.len() {
            slots[NEXT] = self.icon(NEXT, "arrow", "다음 쪽", "white", Vec::new(), false);
        }
    }

    pub(crate) async fn equip_click(
        &mut self,
        page: Page,
        slot: i16,
        _button: i8,
        mode: i32,
    ) -> Result<(), FrameError> {
        let Ok(slot) = usize::try_from(slot) else {
            return Ok(());
        };
        let set = |s: &mut Self, p: Page| {
            if let Some(w) = &mut s.window {
                w.kind = WindowKind::Equip(p);
            }
        };
        match page {
            Page::Gear => match slot {
                TO_ENHANCE => set(self, Page::Enhance { serial: None }),
                TO_SMITH => set(self, Page::Smith { tab: 0, page: 0 }),
                s if ACC_SLOTS.contains(&s) => {
                    let i = ACC_SLOTS.iter().position(|x| *x == s).unwrap();
                    self.unequip_accessory(i).await?;
                }
                s => {
                    if let Some(inv) = inventory_slot(s) {
                        self.equip_accessory(inv).await?;
                    }
                }
            },
            Page::Enhance { serial } => match slot {
                BACK => set(self, Page::Gear),
                ENH_BUTTON => {
                    if let Some(s) = serial {
                        self.enhance(s).await?;
                    }
                }
                s => {
                    if let Some(inv) = inventory_slot(s)
                        && let Some(item) = self.inventory[inv]
                        && config::with(|c| gear_of(&item, c)).is_some()
                    {
                        // 일련번호가 없으면 붙여서 고름
                        let serial = self.ensure_serial(inv);
                        set(self, Page::Enhance { serial });
                    }
                }
            },
            Page::Smith { tab, page: p } => {
                if let Some(t) = TAB_SLOTS.iter().position(|x| *x == slot) {
                    set(self, Page::Smith { tab: t, page: 0 });
                } else if slot == PREV && p > 0 {
                    set(self, Page::Smith { tab, page: p - 1 });
                } else if slot == NEXT {
                    set(self, Page::Smith { tab, page: p + 1 });
                } else if (LIST_FIRST..LIST_FIRST + LIST_PER_PAGE).contains(&slot) {
                    let index = p * LIST_PER_PAGE + slot - LIST_FIRST;
                    if TABS[tab] == "상점" {
                        self.buy(index, if mode == 1 { 10 } else { 1 }).await?;
                    } else if let Some(r) = recipes_of(tab).get(index).cloned() {
                        self.craft(&r).await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// 가방 칸의 장비에 일련번호가 없으면 붙임.
    fn ensure_serial(&mut self, inv: usize) -> Option<u32> {
        let item = self.inventory[inv]?;
        if super::store::is_gear_serial(item.serial) {
            return Some(item.serial);
        }
        let def = config::with(|c| gear_of(&item, c).map(|(d, _)| d))?;
        let s = gears().write(|d| d.create(&def));
        self.inventory[inv] = Some(Item { serial: s, ..item });
        Some(s)
    }

    async fn equip_accessory(&mut self, inv: usize) -> Result<(), FrameError> {
        let Some(item) = self.inventory[inv] else {
            return Ok(());
        };
        let Some((def, _)) = config::with(|c| gear_of(&item, c)) else {
            return Ok(());
        };
        let Some(i) = def.part.accessory_index() else {
            return self
                .error("반지·목걸이·귀걸이만 낄 수 있습니다. 갑옷은 가방에서 입으세요.")
                .await;
        };
        let Some(serial) = self.ensure_serial(inv) else {
            return Ok(());
        };
        let uuid = self.profile.uuid;
        let old = gears().write(|d| {
            let w = d.worn.entry(uuid).or_default();
            w[i].replace(serial)
        });
        self.inventory[inv] = old.and_then(|s| self.serial_item(s));
        self.sync_slot(inv).await?;
        self.play_sound("item.armor.equip_diamond");
        self.message(&text::colored(
            &format!("{}을(를) 꼈습니다.", def.name),
            "green",
        ))
        .await?;
        Ok(())
    }

    async fn unequip_accessory(&mut self, i: usize) -> Result<(), FrameError> {
        let uuid = self.profile.uuid;
        let Some(serial) = gears().read(|d| d.worn(uuid)[i]) else {
            return Ok(());
        };
        let Some(item) = self.serial_item(serial) else {
            return Ok(());
        };
        if !(9..45).any(|s| self.inventory[s].is_none()) {
            return self.error("가방에 빈 칸이 없습니다.").await;
        }
        gears().write(|d| {
            if let Some(w) = d.worn.get_mut(&uuid) {
                w[i] = None;
            }
        });
        self.give(item).await?;
        Ok(())
    }

    async fn enhance(&mut self, serial: u32) -> Result<(), FrameError> {
        let Some(slot) = (9..46).find(|&i| self.inventory[i].is_some_and(|it| it.serial == serial))
        else {
            return self.error("그 장비가 가방에 없습니다.").await;
        };
        let item = self.inventory[slot].unwrap();
        let Some((def, Some(gear))) = config::with(|c| gear_of(&item, c)) else {
            return Ok(());
        };
        if def.part.is_accessory() {
            return self.error("악세서리는 강화할 수 없습니다.").await;
        }
        let level = gear.enhance;
        if level >= MAX_ENHANCE {
            return self.error("이미 최대 강화(+30)입니다.").await;
        }
        let ((money, stones), stone) =
            config::with(|c| (c.enhance_cost(level), c.enhance.stone.clone()));
        if self.count_key(&stone) < stones {
            return self
                .error(&format!("강화석이 모자랍니다 ({stones}개 필요)"))
                .await;
        }
        if !self.pay(money).await {
            return Ok(());
        }
        self.take_key(&stone, stones).await?;
        let roll = (crate::game::rand_u32() % 1_000_000) as f64 / 1_000_000.0;
        let ok = config::with(|c| super::store::enhance_succeeds(c, level, roll));
        if ok {
            gears().write(|d| {
                if let Some(g) = d.gears.get_mut(&serial) {
                    g.enhance = level + 1;
                }
            });
            self.sync_slot(slot).await?;
            self.play_sound("block.anvil.use");
            self.play_sound("entity.player.levelup");
            self.message(&text::colored(
                &format!("강화 성공! +{} {}", level + 1, def.name),
                "gold",
            ))
            .await?;
            if level + 1 >= 15 && (level + 1) % 5 == 0 {
                self.server.publish(Event::System(text::colored(
                    &format!(
                        "{}님이 {}을(를) +{} 강화에 성공했습니다!",
                        self.profile.name,
                        def.name,
                        level + 1
                    ),
                    "gold",
                )));
            }
        } else {
            self.play_sound("block.anvil.land");
            self.message(&text::colored(
                &format!("강화 실패… (+{level} 유지)"),
                "red",
            ))
            .await?;
        }
        Ok(())
    }

    async fn craft(&mut self, r: &Recipe) -> Result<(), FrameError> {
        let level = self.smith_level().await;
        if level < r.smith_level {
            return self
                .error(&format!(
                    "대장장이 Lv.{} 이 필요합니다 (지금 Lv.{level})",
                    r.smith_level
                ))
                .await;
        }
        for (k, n) in &r.inputs {
            if self.count_key(k) < *n {
                let name = config::with(|c| c.item_name(k)).unwrap_or(k.clone());
                return self
                    .error(&format!("재료가 모자랍니다: {name} {n}개"))
                    .await;
            }
        }
        if !self.pay(r.money).await {
            return Ok(());
        }
        for (k, n) in &r.inputs {
            self.take_key(k, *n).await?;
        }
        self.give_key(&r.result, r.count).await?;
        let job = config::with(|c| c.smith_job.clone());
        crate::plugins::rank::job_xp(self.profile.uuid, &job, r.xp).await;
        self.play_sound("block.smithing_table.use");
        let name = config::with(|c| c.item_name(&r.result)).unwrap_or(r.result.clone());
        self.message(&text::colored(
            &format!("{name}을(를) 만들었습니다. ({job} 경험치 +{:.0})", r.xp),
            "green",
        ))
        .await?;
        Ok(())
    }

    async fn buy(&mut self, index: usize, n: i32) -> Result<(), FrameError> {
        let Some(e) = config::with(|c| c.shop.get(index).cloned()) else {
            return Ok(());
        };
        if !self.pay(e.price * n as i64).await {
            return Ok(());
        }
        self.give_key(&e.item, n).await?;
        let name = config::with(|c| c.item_name(&e.item)).unwrap_or(e.item.clone());
        self.message(&text::colored(
            &format!("{name} ×{n}을(를) 샀습니다."),
            "green",
        ))
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_does_not_overlap() {
        let mut used = vec![
            STATS_SLOT,
            SETS_SLOT,
            POT_SLOT,
            TO_ENHANCE,
            TO_SMITH,
            WEAPON_SLOT,
        ];
        used.extend(ARMOR_SLOTS);
        used.extend(ACC_SLOTS);
        let mut d = used.clone();
        d.sort();
        d.dedup();
        assert_eq!(d.len(), used.len());
        assert_eq!(inventory_slot(54), Some(9));
        assert_eq!(inventory_slot(89), Some(44));
        assert_eq!(inventory_slot(53), None);
        assert!(TAB_SLOTS.iter().all(|s| *s < LIST_FIRST));
        assert_eq!(
            super::super::session::item_key(&Item::new(
                crate::data::items::by_name("stick").unwrap(),
                1
            )),
            "stick"
        );
        assert_eq!(vanilla_name("gold_nugget"), "금 조각");
    }
}
