//! 상점 창: 상점 목록(27칸) → 상점(54칸, 쪽마다 28개).

use tokio::io::AsyncWrite;

use super::{Currency, Entry, Shop, Special, with};
use crate::dambi::{child, shared::SharedData};
use crate::data::{blocks, items, menu};
use crate::game::custom_items::{self, Look};
use crate::packets::container as pc;
use crate::packets::world::Item;
use crate::protocol::frame::FrameError;
use crate::protocol::nbt::Tag;
use crate::session::play::Session;
use crate::session::windows::{Window, WindowKind};
use crate::text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    List,
    Shop { id: String, page: usize },
}

impl Page {
    pub fn size(&self) -> usize {
        match self {
            Page::List => 27,
            Page::Shop { .. } => 54,
        }
    }
}

/// 한 쪽의 물건 칸.
const ENTRY_SLOTS: [usize; 28] = [
    10, 11, 12, 13, 14, 15, 16, 19, 20, 21, 22, 23, 24, 25, 28, 29, 30, 31, 32, 33, 34, 37, 38, 39,
    40, 41, 42, 43,
];
pub const PER_PAGE: usize = ENTRY_SLOTS.len();
const BALANCE: usize = 4;
const PREV: usize = 45;
const HELP: usize = 47;
const BACK: usize = 49;
const NEXT: usize = 53;
const LIST_SLOTS: [usize; 7] = [10, 11, 12, 13, 14, 15, 16];

/// 사기 한 번에 Shift면 이만큼 묶음.
const SHIFT_UNITS: i64 = 10;

/// 상점 창이 쓰는 것: 열 때·사고팔 때 받아 둔 재화.
#[derive(Debug, Default)]
pub struct ShopState {
    data: Option<SharedData>,
}

fn n(v: i64) -> String {
    crate::plugins::hud::number(v)
}

/// 가방에 넣을 물건 (특수 물건이면 None).
fn item_of(e: &Entry, count: i32) -> Option<Item> {
    if e.item.starts_with('@') {
        return None;
    }
    crate::plugins::quest::item_by_name(&e.item, count)
}

/// 물건 이름 조각: 우리 아이템은 그 이름, 바닐라는 클라이언트 말(한국어)로.
fn name_tag(e: &Entry, color: &str) -> Tag {
    let styled = |mut f: Vec<(String, Tag)>| {
        f.push(("color".into(), Tag::String(color.to_owned())));
        f.push(("italic".into(), Tag::Byte(0)));
        Tag::Compound(f)
    };
    if !e.name.is_empty() {
        return styled(vec![("text".into(), Tag::String(e.name.clone()))]);
    }
    if let Some(it) = item_of(e, 1) {
        if let Some(c) = it.custom.and_then(custom_items::get) {
            return styled(vec![("text".into(), Tag::String(c.name))]);
        }
        let name = items::name(it.id);
        let block = blocks::block_for_item(it.id).is_some_and(|b| blocks::name(b) == name);
        let key = format!("{}.minecraft.{name}", if block { "block" } else { "item" });
        return styled(vec![("translate".into(), Tag::String(key))]);
    }
    styled(vec![("text".into(), Tag::String(e.item.clone()))])
}

/// 이름을 글로 (채팅 알림용).
fn name_text(e: &Entry) -> String {
    if !e.name.is_empty() {
        return e.name.clone();
    }
    item_of(e, 1)
        .and_then(|it| it.custom.and_then(custom_items::get).map(|c| c.name))
        .unwrap_or_else(|| e.item.clone())
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    /// `/상점 [id]`.
    pub(crate) async fn shop_command(&mut self, args: &[&str]) -> Result<(), FrameError> {
        let page = match args.first() {
            None => Page::List,
            Some(id) => match with(|c| c.shop(id).map(|s| s.id.clone())) {
                Some(id) => Page::Shop { id, page: 0 },
                None => return self.error(&format!("{id} 상점이 없습니다. (/상점)")).await,
            },
        };
        Ok(self.open_shop(page).await?)
    }

    pub(crate) async fn open_shop(&mut self, page: Page) -> std::io::Result<()> {
        self.shop.data = child::shared(self.profile.uuid).await;
        self.close_window(false).await?;
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        let (kind, title) = match &page {
            Page::List => (menu::GENERIC_9X3, "상점".to_owned()),
            Page::Shop { id, .. } => (
                menu::GENERIC_9X6,
                with(|c| c.shop(id).map(|s| s.name.clone())).unwrap_or_default(),
            ),
        };
        self.window = Some(Window {
            id,
            kind: WindowKind::Shop(page),
            grid: [None; 9],
        });
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, kind, &text::plain(&title)),
        )
        .await?;
        self.send_window().await
    }

    fn balance_lore(&self) -> Vec<String> {
        let d = self.shop.data.clone().unwrap_or_default();
        Currency::ALL
            .iter()
            .map(|c| format!("{}  {}", c.name(), n(c.of(&d))))
            .collect()
    }

    pub(crate) fn shop_slots(&self, page: &Page) -> Vec<Option<Item>> {
        let size = page.size();
        let mut slots: Vec<Option<Item>> = vec![None; size];
        for (s, slot) in slots.iter_mut().enumerate() {
            let (row, col) = (s / 9, s % 9);
            if row == 0 || row + 1 == size / 9 || col == 0 || col == 8 {
                *slot = self.pane(s);
            }
        }
        let cfg = super::get();
        match page {
            Page::List => {
                slots[BALANCE] = self.icon(
                    BALANCE,
                    "gold_ingot",
                    "내 재화",
                    "gold",
                    self.balance_lore(),
                    false,
                );
                for (&slot, s) in LIST_SLOTS.iter().zip(&cfg.shops) {
                    let mut lore = vec![
                        format!("재화  {}", s.currency.name()),
                        format!("물건  {}가지", s.entries.len()),
                    ];
                    if !s.description.is_empty() {
                        lore.push(format!("({})", s.description));
                    }
                    lore.push("---".into());
                    lore.push("▶ 클릭: 열기".into());
                    let icon = if s.icon.is_empty() { "chest" } else { &s.icon };
                    slots[slot] = self.icon(slot, icon, &s.name, s.currency.color(), lore, false);
                }
            }
            Page::Shop { id, page } => {
                let Some(shop) = cfg.shop(id) else {
                    return slots;
                };
                let d = self.shop.data.clone().unwrap_or_default();
                slots[BALANCE] = self.icon(
                    BALANCE,
                    "gold_ingot",
                    &format!("내 {}: {}", shop.currency.name(), n(shop.currency.of(&d))),
                    shop.currency.color(),
                    self.balance_lore(),
                    true,
                );
                for (i, e) in shop
                    .entries
                    .iter()
                    .enumerate()
                    .skip(page * PER_PAGE)
                    .take(PER_PAGE)
                {
                    let slot = ENTRY_SLOTS[i - page * PER_PAGE];
                    slots[slot] = self.entry_icon(slot, shop, e);
                }
                if *page > 0 {
                    slots[PREV] = self.icon(PREV, "arrow", "◀ 이전 쪽", "white", Vec::new(), false);
                }
                if (page + 1) * PER_PAGE < shop.entries.len() {
                    slots[NEXT] = self.icon(NEXT, "arrow", "다음 쪽 ▶", "white", Vec::new(), false);
                }
                slots[BACK] = self.icon(BACK, "oak_door", "상점 목록", "white", Vec::new(), false);
                slots[HELP] = self.icon(
                    HELP,
                    "book",
                    "사고파는 법",
                    "aqua",
                    vec![
                        "좌클릭: 1묶음 사기".into(),
                        format!("Shift+좌클릭: {SHIFT_UNITS}묶음 사기"),
                        "우클릭: 1묶음 팔기".into(),
                        "Shift+우클릭: 가진 것 모두 팔기".into(),
                    ],
                    false,
                );
            }
        }
        slots
    }

    fn entry_icon(&self, slot: usize, shop: &Shop, e: &Entry) -> Option<Item> {
        let cur = shop.currency;
        let have = item_of(e, 1).map(|it| self.count_like(&it));
        let lore = entry_lore(shop, e, have);
        let serial = self.icon_serial(slot);
        let lore_tags = lore.iter().map(|l| text::lore_line(l)).collect();
        custom_items::set_look(
            serial,
            Look {
                rich: Some((name_tag(e, cur.color()), lore_tags)),
                name: None,
                lore: Vec::new(),
                glint: Some(e.special().is_some()),
            },
        );
        let (id, custom) = match (e.special(), item_of(e, 1)) {
            (Some(Special::Fly(_)), _) => (items::by_name("feather")?, None),
            (Some(Special::Autoplant(_)), _) => (items::by_name("wheat_seeds")?, None),
            (None, Some(it)) => (it.id, it.custom),
            (None, None) => (items::by_name("barrier")?, None),
        };
        Some(Item {
            id,
            count: e.count.clamp(1, 64),
            custom: custom.or_else(|| custom_items::index_of(crate::plugins::lottery::ICON_ITEM)),
            serial,
        })
    }
}

/// 물건 설명 줄들 (`have`: 가방에 가진 개수, 팔 수 있을 때 보임).
pub fn entry_lore(shop: &Shop, e: &Entry, have: Option<i32>) -> Vec<String> {
    let cur = shop.currency;
    let mut lore = Vec::new();
    if e.count > 1 {
        lore.push(format!("한 묶음  {}개", e.count));
    }
    if let Some(sp) = e.special() {
        lore.push(match sp {
            Special::Fly(s) => format!(
                "(플라이 {} — /플라이)",
                crate::plugins::rank::duration_text(s)
            ),
            Special::Autoplant(b) => {
                format!("(다 자란 작물을 캐면 다시 심김 {}블록)", n(b as i64))
            }
        });
    }
    lore.push("---".into());
    lore.push(if e.buy > 0 {
        format!("살 때  {} {}", n(e.buy), cur.name())
    } else {
        "살 때  —".into()
    });
    lore.push(if e.sell > 0 {
        format!("팔 때  {} {}", n(e.sell), cur.name())
    } else {
        "팔 때  —".into()
    });
    if e.sell > 0
        && let Some(h) = have
    {
        lore.push(format!("가진 개수  {h}"));
    }
    lore.push("---".into());
    if e.buy > 0 {
        lore.push("좌클릭: 1묶음 사기".into());
        lore.push(format!("Shift+좌클릭: {SHIFT_UNITS}묶음 사기"));
    }
    if e.sell > 0 {
        lore.push("우클릭: 1묶음 팔기".into());
        lore.push("Shift+우클릭: 모두 팔기".into());
    }
    lore
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    /// 가방(+핫바)에 이 아이템이 몇 개 (일련번호 있는 장비는 빼고).
    fn count_like(&self, it: &Item) -> i32 {
        self.inventory[9..45]
            .iter()
            .flatten()
            .filter(|i| i.serial == 0 && i.id == it.id && i.custom == it.custom)
            .map(|i| i.count)
            .sum()
    }

    pub(crate) async fn shop_click(
        &mut self,
        page: Page,
        slot: i16,
        button: i8,
        mode: i32,
    ) -> Result<(), FrameError> {
        let Ok(slot) = usize::try_from(slot) else {
            return Ok(());
        };
        let set = |s: &mut Self, p: Page| {
            if let Some(w) = &mut s.window {
                w.kind = WindowKind::Shop(p);
            }
        };
        match page {
            Page::List => {
                let id = with(|c| {
                    LIST_SLOTS
                        .iter()
                        .position(|&s| s == slot)
                        .and_then(|i| c.shops.get(i).map(|s| s.id.clone()))
                });
                if let Some(id) = id {
                    self.open_shop(Page::Shop { id, page: 0 }).await?;
                }
                Ok(())
            }
            Page::Shop { id, page } => {
                match slot {
                    BACK => return Ok(self.open_shop(Page::List).await?),
                    PREV if page > 0 => {
                        set(self, Page::Shop { id, page: page - 1 });
                        return Ok(());
                    }
                    NEXT => {
                        set(self, Page::Shop { id, page: page + 1 });
                        return Ok(());
                    }
                    _ => {}
                }
                let Some(k) = ENTRY_SLOTS.iter().position(|&s| s == slot) else {
                    return Ok(());
                };
                let Some((shop, e)) = with(|c| {
                    let s = c.shop(&id)?;
                    Some((s.clone(), s.entries.get(page * PER_PAGE + k)?.clone()))
                }) else {
                    return Ok(());
                };
                let shift = mode == pc::MODE_QUICK_MOVE;
                match button {
                    0 => {
                        self.shop_buy(&shop, &e, if shift { SHIFT_UNITS } else { 1 })
                            .await
                    }
                    1 => {
                        self.shop_sell(&shop, &e, if shift { i64::MAX } else { 1 })
                            .await
                    }
                    _ => Ok(()),
                }
            }
        }
    }

    async fn shop_buy(&mut self, shop: &Shop, e: &Entry, units: i64) -> Result<(), FrameError> {
        if e.buy <= 0 {
            return self.error("이 물건은 팔지 않습니다.").await;
        }
        let cur = shop.currency;
        let me = self.profile.uuid;
        let total = e.buy.saturating_mul(units);
        match child::shared_add(me, cur.field(), -total).await {
            Some((true, d)) => self.shop.data = Some(d),
            Some((false, d)) => {
                let msg = format!(
                    "{}이(가) 모자랍니다. ({} 필요, 가진 것 {})",
                    cur.name(),
                    n(total),
                    n(cur.of(&d))
                );
                return self.error(&msg).await;
            }
            None => return self.error("재화 정보를 받지 못했습니다.").await,
        }
        match e.special() {
            Some(Special::Fly(s)) => {
                crate::plugins::rank::add_fly_secs(me, s * units);
            }
            Some(Special::Autoplant(b)) => {
                crate::plugins::rank::add_autoplant(me, b * units as u64);
            }
            None => {
                let Some(it) = item_of(e, 1) else {
                    return self.error("아이템을 찾지 못했습니다.").await;
                };
                let want = e.count.saturating_mul(units as i32);
                let mut left = want;
                while left > 0 {
                    let chunk = left.min(64 * 36);
                    let given = self.give_many(it, chunk).await?;
                    if given < chunk {
                        // 가방이 차면 발밑에
                        let mut rest = chunk - given;
                        let max = custom_items::max_stack(&it);
                        while rest > 0 {
                            let k = rest.min(max);
                            self.throw(it.with_count(k));
                            rest -= k;
                        }
                    }
                    left -= chunk;
                }
            }
        }
        self.play_sound("entity.experience_orb.pickup");
        let left = self.shop.data.as_ref().map_or(0, |d| cur.of(d));
        let msg = format!(
            "[{}] {} ×{} 샀습니다  −{} {} (남은 {} {})",
            shop.name,
            name_text(e),
            e.count as i64 * units,
            n(total),
            cur.name(),
            cur.name(),
            n(left)
        );
        Ok(self.message(&text::colored(&msg, "green")).await?)
    }

    async fn shop_sell(&mut self, shop: &Shop, e: &Entry, units: i64) -> Result<(), FrameError> {
        if e.sell <= 0 {
            return self.error("이 물건은 사지 않습니다.").await;
        }
        let Some(it) = item_of(e, 1) else {
            return self.error("팔 수 있는 물건이 아닙니다.").await;
        };
        let have = self.count_like(&it);
        let units = units.min((have / e.count.max(1)) as i64);
        if units <= 0 {
            return self
                .error(&format!(
                    "{}이(가) {}개 있어야 팔 수 있습니다.",
                    name_text(e),
                    e.count
                ))
                .await;
        }
        // 가방에서 꺼냄 (핫바 먼저가 아니라 뒤에서부터 — 손에 든 것은 마지막에)
        let mut left = e.count * units as i32;
        for slot in (9..45).rev() {
            if left == 0 {
                break;
            }
            if let Some(i) = self.inventory[slot]
                && i.serial == 0
                && i.id == it.id
                && i.custom == it.custom
            {
                let k = i.count.min(left);
                left -= k;
                self.inventory[slot] = Some(i.with_count(i.count - k)).filter(|i| i.count > 0);
                self.sync_slot(slot).await?;
            }
        }
        self.hands_changed();
        let cur = shop.currency;
        let total = e.sell.saturating_mul(units);
        if let Some((_, d)) = child::shared_add(self.profile.uuid, cur.field(), total).await {
            self.shop.data = Some(d);
        }
        self.play_sound("block.note_block.bell");
        let now = self.shop.data.as_ref().map_or(0, |d| cur.of(d));
        let msg = format!(
            "[{}] {} ×{} 팔았습니다  +{} {} (가진 {} {})",
            shop.name,
            name_text(e),
            e.count as i64 * units,
            n(total),
            cur.name(),
            cur.name(),
            n(now)
        );
        Ok(self.message(&text::colored(&msg, "gold")).await?)
    }
}
