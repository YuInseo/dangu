//! 인벤토리와 창: 플레이어 인벤토리(2×2 조합 포함), 작업대, 상자.
//!
//! 클릭은 클라이언트가 "이렇게 바뀌었다"고 보낸 칸들을 받아들이는 방식입니다 (바닐라처럼
//! 서버가 클릭 규칙을 다시 계산하지 않음). 대신 아이템이 늘어나는 클릭은 — 조합 결과를
//! 꺼낸 경우를 빼고 — 거부하고 창 전체를 다시 보내 복제를 막습니다.

use std::collections::HashMap;

use tokio::io::AsyncWrite;

use super::play::Session;
use crate::data::{items, menu};
use crate::game::containers::CHEST_SLOTS;
use crate::game::entities::THROWN_PICKUP_DELAY;
use crate::packets::container::{self as pc, Click};
use crate::packets::world::{self as pw, BlockPos, Item};
use crate::protocol::frame::FrameError;
use crate::server::Event;
use crate::text;
use crate::world::Dimension;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowKind {
    Crafting,
    /// 상자·통·화로·발사기 등: 칸 수와 함께.
    Chest {
        dimension: Dimension,
        pos: BlockPos,
        size: usize,
    },
    /// 주민 거래: 0·1 낼 것, 2 받을 것. 고른 거래 번호와 함께.
    Merchant {
        villager: i32,
        selected: usize,
    },
    /// 서버가 그리는 메뉴 (복권): 클릭은 버튼으로만 쓰고 아이템은 움직이지 않음.
    Lottery(crate::plugins::lottery::gui::Page),
    /// `/ia` 아이템 목록 (메뉴).
    Catalog(crate::plugins::contents::catalog::Catalog),
    /// 길드 메뉴.
    Guild(crate::plugins::guild::gui::Page),
    /// `/퀘스트` 메뉴.
    Quest(crate::plugins::quest::gui::Page),
    /// `/장비` · `/강화` · `/대장간` 메뉴.
    Equip(crate::plugins::equip::gui::Page),
    /// `/등급` 창 (메뉴).
    Rank,
    /// `/ec` 개인 엔더상자 27칸.
    Ender,
    /// 길드 창고 한 페이지: 위 45칸은 진짜 칸, 아래 줄은 버튼.
    GuildStorage {
        guild: u32,
        page: usize,
    },
}

#[derive(Debug, Clone)]
pub struct Window {
    pub id: i32,
    pub kind: WindowKind,
    /// 작업대의 3×3 조합 칸 (거래 창은 앞 두 칸이 낼 것).
    pub(crate) grid: [Option<Item>; 9],
}

/// 플레이어 인벤토리 메뉴의 칸: 0 결과, 1..5 조합, 5..9 갑옷, 9..36 가방, 36..45 핫바, 45 보조 손.
const INV_GRID: std::ops::Range<usize> = 1..5;

/// 메뉴 칸 → `Set Player Inventory`의 칸 (0..9 핫바, 9..36 가방, 36..40 갑옷, 40 보조 손).
fn inventory_index(menu_slot: usize) -> Option<i32> {
    Some(match menu_slot {
        36..=44 => menu_slot as i32 - 36,
        9..=35 => menu_slot as i32,
        5..=8 => 39 - (menu_slot as i32 - 5),
        45 => 40,
        _ => return None,
    })
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    pub(crate) fn next_state_id(&mut self) -> i32 {
        self.state_id = self.state_id.wrapping_add(1) & 0x7FFF;
        self.state_id
    }

    /// 플레이어 인벤토리 전체를 다시 보냅니다.
    pub(crate) async fn send_inventory(&mut self) -> std::io::Result<()> {
        self.inventory[0] = items::craft(&self.inventory[INV_GRID], 2);
        let state = self.next_state_id();
        let body = pc::encode_set_content(0, state, &self.inventory, self.carried);
        self.send(pc::CONTAINER_SET_CONTENT_ID, &body).await
    }

    /// 서버가 바꾼 인벤토리 한 칸을 알립니다 (어느 창이 열려 있든 통함).
    pub(crate) async fn sync_slot(&mut self, menu_slot: usize) -> std::io::Result<()> {
        if let Some(i) = inventory_index(menu_slot) {
            let body = pw::encode_player_inventory(i, self.inventory[menu_slot]);
            self.send(pw::SET_PLAYER_INVENTORY_ID, &body).await?;
        }
        Ok(())
    }

    /// 주운 아이템을 넣습니다: 같은 것이 있는 칸부터, 그다음 빈 칸 (핫바 먼저).
    /// 넣은 개수를 돌려줍니다.
    pub(crate) async fn give(&mut self, item: Item) -> std::io::Result<i32> {
        let max = crate::game::custom_items::max_stack(&item);
        let order: Vec<usize> = (36..45).chain(9..36).collect();
        let mut left = item.count;
        let mut changed = Vec::new();
        for pass in 0..2 {
            for &s in &order {
                if left == 0 {
                    break;
                }
                match &mut self.inventory[s] {
                    Some(have) if pass == 0 && have.stacks_with(&item) && have.count < max => {
                        let n = (max - have.count).min(left);
                        have.count += n;
                        left -= n;
                        changed.push(s);
                    }
                    slot @ None if pass == 1 => {
                        let n = max.min(left);
                        *slot = Some(item.with_count(n));
                        left -= n;
                        changed.push(s);
                    }
                    _ => {}
                }
            }
        }
        for s in changed {
            self.sync_slot(s).await?;
        }
        if self.main_hand().map(|i| i.id) == Some(item.id) || self.main_hand().is_none() {
            self.hands_changed();
        }
        Ok(item.count - left)
    }

    /// 핫바에서 들고 있는 것을 버립니다 (Q).
    pub(crate) async fn drop_held(&mut self, whole_stack: bool) -> std::io::Result<()> {
        let slot = 36 + self.held;
        let Some(item) = self.inventory[slot] else {
            return Ok(());
        };
        let n = if whole_stack { item.count } else { 1 };
        self.inventory[slot] = Some(Item {
            count: item.count - n,
            ..item
        })
        .filter(|i| i.count > 0);
        self.throw(item.with_count(n));
        self.sync_slot(slot).await?;
        self.hands_changed();
        Ok(())
    }

    /// 바라보는 쪽으로 던집니다.
    pub(crate) fn throw(&self, item: Item) {
        let (yaw, pitch) = (self.yaw.to_radians() as f64, self.pitch.to_radians() as f64);
        let vel = (
            -yaw.sin() * pitch.cos() * 0.3,
            -pitch.sin() * 0.3 + 0.1,
            yaw.cos() * pitch.cos() * 0.3,
        );
        let pos = (self.position.0, self.position.1 + 1.32, self.position.2);
        self.server.entities.spawn_item(
            self.server,
            self.dimension,
            pos,
            vel,
            item,
            THROWN_PICKUP_DELAY,
        );
    }

    pub(crate) async fn open_crafting(&mut self) -> std::io::Result<()> {
        self.close_window(false).await?;
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        self.window = Some(Window {
            id,
            kind: WindowKind::Crafting,
            grid: [None; 9],
        });
        let title = text::plain("제작");
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, menu::CRAFTING, &title),
        )
        .await?;
        self.send_window().await
    }

    pub(crate) async fn open_chest(&mut self, pos: BlockPos) -> std::io::Result<()> {
        self.close_window(false).await?;
        self.server.fill_structure_loot(self.dimension, pos);
        let state = self.server.world.get(self.dimension, pos.0, pos.1, pos.2);
        let name = crate::world::blockset::name_of(state);
        let (size, menu_id, title) = match name {
            "furnace" => (3, menu::FURNACE, "화로"),
            "blast_furnace" => (3, menu::BLAST_FURNACE, "용광로"),
            "smoker" => (3, menu::SMOKER, "훈연기"),
            "dispenser" => (9, menu::GENERIC_3X3, "발사기"),
            "dropper" => (9, menu::GENERIC_3X3, "공급기"),
            "hopper" => (5, menu::HOPPER, "깔때기"),
            "barrel" => (CHEST_SLOTS, menu::GENERIC_9X3, "통"),
            _ => (CHEST_SLOTS, menu::GENERIC_9X3, "상자"),
        };
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        self.window = Some(Window {
            id,
            kind: WindowKind::Chest {
                dimension: self.dimension,
                pos,
                size,
            },
            grid: [None; 9],
        });
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, menu_id, &text::plain(title)),
        )
        .await?;
        self.send_window().await?;
        if crate::game::mechanics::furnace::kind_of(name).is_some() {
            self.server.furnaces.wake(self.dimension, pos);
            let values = self.server.furnaces.data(self.dimension, pos);
            self.container_data(self.dimension, pos, values).await?;
        }
        Ok(())
    }

    /// 화로의 진행 값을 창에 보냅니다.
    pub(crate) async fn container_data(
        &mut self,
        dimension: Dimension,
        pos: BlockPos,
        values: [i16; 4],
    ) -> std::io::Result<()> {
        let Some(w) = &self.window else { return Ok(()) };
        let WindowKind::Chest {
            dimension: d,
            pos: p,
            ..
        } = w.kind
        else {
            return Ok(());
        };
        if (d, p) != (dimension, pos) {
            return Ok(());
        }
        let id = w.id;
        for (i, v) in values.iter().enumerate() {
            let mut body = Vec::new();
            crate::protocol::varint::write_varint(&mut body, id);
            body.extend_from_slice(&(i as i16).to_be_bytes());
            body.extend_from_slice(&v.to_be_bytes());
            self.send(pc::CONTAINER_SET_DATA_ID, &body).await?;
        }
        Ok(())
    }

    /// 열린 창의 모든 칸 (창 번호 기준).
    fn window_slots(&self) -> Vec<Option<Item>> {
        let Some(w) = &self.window else {
            let mut v = self.inventory.to_vec();
            v[0] = items::craft(&self.inventory[INV_GRID], 2);
            return v;
        };
        let mut slots = match &w.kind {
            WindowKind::Crafting => {
                let mut v = vec![items::craft(&w.grid, 3)];
                v.extend_from_slice(&w.grid);
                v
            }
            WindowKind::Chest {
                dimension,
                pos,
                size,
            } => {
                let mut v = self.server.containers.get(*dimension, *pos);
                v.truncate(*size);
                v
            }
            WindowKind::Lottery(page) => self.lottery_slots(page),
            WindowKind::Catalog(page) => self.catalog_slots(page),
            WindowKind::Guild(page) => self.guild_slots(page),
            WindowKind::Quest(page) => self.quest_slots(page),
            WindowKind::Equip(page) => self.equip_slots(page),
            WindowKind::Rank => self.rank_slots(),
            WindowKind::Ender => self.ender_slots(),
            WindowKind::GuildStorage { guild, page } => {
                let mut v = crate::plugins::guild::guilds().read(|d| {
                    d.guild(*guild)
                        .and_then(|g| g.storage.get(*page).cloned())
                        .unwrap_or_else(|| vec![None; crate::plugins::guild::PAGE_SLOTS])
                });
                v.extend(self.storage_nav(*guild, *page));
                v
            }
            WindowKind::Merchant { villager, selected } => {
                let result = self
                    .merchant_result(*villager, *selected, w.grid[0], w.grid[1])
                    .map(|r| r.1);
                vec![w.grid[0], w.grid[1], result]
            }
        };
        slots.extend_from_slice(&self.inventory[9..45]);
        slots
    }

    pub(crate) async fn send_window(&mut self) -> std::io::Result<()> {
        let Some(id) = self.window.as_ref().map(|w| w.id) else {
            return self.send_inventory().await;
        };
        let slots = self.window_slots();
        let state = self.next_state_id();
        let body = pc::encode_set_content(id, state, &slots, self.carried);
        self.send(pc::CONTAINER_SET_CONTENT_ID, &body).await
    }

    /// 창을 닫습니다. 조합 칸에 남은 것은 인벤토리로 (자리가 없으면 떨어뜨림).
    pub(crate) async fn close_window(&mut self, by_client: bool) -> std::io::Result<()> {
        let leftovers: Vec<Item> = match self.window.take() {
            Some(w) => {
                if let WindowKind::Merchant { villager, .. } = w.kind {
                    self.merchant_closed(villager);
                }
                if matches!(
                    w.kind,
                    WindowKind::Lottery(_)
                        | WindowKind::Catalog(_)
                        | WindowKind::Guild(_)
                        | WindowKind::Quest(_)
                        | WindowKind::Equip(_)
                        | WindowKind::Rank
                        | WindowKind::GuildStorage { .. }
                ) {
                    self.lottery_closed();
                }
                if !by_client {
                    self.send(0x11, &{
                        let mut b = Vec::new();
                        crate::protocol::varint::write_varint(&mut b, w.id);
                        b
                    })
                    .await?;
                }
                w.grid.into_iter().flatten().collect()
            }
            None => INV_GRID.filter_map(|s| self.inventory[s].take()).collect(),
        };
        let carried = self.carried.take();
        for item in leftovers.into_iter().chain(carried) {
            let taken = self.give(item).await?;
            if taken < item.count {
                self.throw(Item {
                    count: item.count - taken,
                    ..item
                });
            }
        }
        self.send_inventory().await
    }

    /// 열린 창의 위쪽(창 자체) 칸 수. 그 뒤가 플레이어 가방 27칸 + 핫바 9칸.
    pub(crate) fn window_size(&self) -> usize {
        match &self.window {
            None => 0,
            Some(w) => match w.kind {
                WindowKind::Crafting => 10,
                WindowKind::Chest { size, .. } => size,
                WindowKind::Merchant { .. } => 3,
                WindowKind::Lottery(ref page) => page.size(),
                WindowKind::Catalog(_) => 54,
                WindowKind::Guild(ref page) => page.size(),
                WindowKind::Quest(_) | WindowKind::Equip(_) => 54,
                WindowKind::GuildStorage { .. } => 54,
                WindowKind::Rank | WindowKind::Ender => 27,
            },
        }
    }

    /// 창 번호의 칸 하나 읽기·쓰기.
    fn slot_mut(&mut self, slot: usize) -> Option<SlotRef<'_>> {
        let own = self.window_size();
        match &mut self.window {
            None => (slot < 46).then(|| SlotRef::Inventory(&mut self.inventory[slot])),
            Some(w) => {
                if slot < own {
                    match &w.kind {
                        WindowKind::Crafting => {
                            if slot == 0 {
                                Some(SlotRef::Result)
                            } else {
                                Some(SlotRef::Inventory(&mut w.grid[slot - 1]))
                            }
                        }
                        WindowKind::Chest { dimension, pos, .. } => {
                            Some(SlotRef::Chest(*dimension, *pos, slot))
                        }
                        WindowKind::Lottery(_)
                        | WindowKind::Catalog(_)
                        | WindowKind::Guild(_)
                        | WindowKind::Quest(_)
                        | WindowKind::Equip(_)
                        | WindowKind::Rank => None,
                        WindowKind::Ender => Some(SlotRef::Ender(slot)),
                        WindowKind::GuildStorage { guild, page } => (slot
                            < crate::plugins::guild::PAGE_SLOTS)
                            .then_some(SlotRef::Guild(*guild, *page, slot)),
                        WindowKind::Merchant { .. } => {
                            if slot == 2 {
                                Some(SlotRef::Result)
                            } else {
                                Some(SlotRef::Inventory(&mut w.grid[slot]))
                            }
                        }
                    }
                } else if slot < own + 36 {
                    Some(SlotRef::Inventory(&mut self.inventory[slot - own + 9]))
                } else {
                    None
                }
            }
        }
    }

    pub(crate) async fn click(&mut self, c: Click) -> Result<(), FrameError> {
        let window_id = self.window.as_ref().map_or(0, |w| w.id);
        if c.window != window_id {
            return Ok(()); // 이미 닫힌 창에 대한 클릭
        }
        let menu = self.window.as_ref().map(|w| w.kind.clone());
        // 주문서를 들고 악세서리 위에 놓기 (가방·상자 등 보통 창에서)
        if c.mode == pc::MODE_PICKUP
            && c.button == 0
            && let Ok(slot) = usize::try_from(c.slot)
            && self.try_apply_scroll(slot).await?
        {
            return Ok(());
        }
        if let Some(
            WindowKind::Lottery(_)
            | WindowKind::Catalog(_)
            | WindowKind::Guild(_)
            | WindowKind::Quest(_)
            | WindowKind::Equip(_)
            | WindowKind::Rank,
        ) = menu
        {
            // 메뉴: 무엇을 집든 되돌리고, 누른 칸만 버튼으로
            match menu {
                Some(WindowKind::Lottery(page)) => self.lottery_click(page, c.slot).await?,
                Some(WindowKind::Catalog(page)) => self.catalog_click(page, c.slot, c.mode).await?,
                Some(WindowKind::Guild(page)) => {
                    self.guild_click(page, c.slot, c.button, c.mode).await?
                }
                Some(WindowKind::Quest(page)) => {
                    self.quest_click(page, c.slot, c.button, c.mode).await?
                }
                Some(WindowKind::Equip(page)) => {
                    self.equip_click(page, c.slot, c.button, c.mode).await?
                }
                Some(WindowKind::Rank) => self.rank_click(c.slot).await?,
                _ => {}
            }
            if self.window.as_ref().is_some_and(|w| w.id == window_id) {
                self.send_window().await?;
            }
            return Ok(());
        }
        // 길드 창고: 아래 줄은 버튼, 권한이 없으면 보기만
        if let Some(WindowKind::GuildStorage { page, .. }) = menu {
            let nav = (crate::plugins::guild::PAGE_SLOTS as i16..54).contains(&c.slot);
            let allowed = crate::plugins::guild::guilds().read(|d| {
                d.of(self.profile.uuid).is_some_and(|g| {
                    g.can(self.profile.uuid, crate::plugins::guild::Perm::StorageUse)
                })
            });
            if nav || !allowed {
                if nav && !c.changed.iter().any(|(s, _)| *s < 45) {
                    self.storage_nav_click(page, c.slot as usize).await?;
                }
                if self.window.as_ref().is_some_and(|w| w.id == window_id) {
                    self.send_window().await?;
                }
                return Ok(());
            }
        }
        let craft_window = matches!(
            self.window.as_ref().map(|w| &w.kind),
            None | Some(WindowKind::Crafting)
        );
        let merchant = match self.window.as_ref().map(|w| &w.kind) {
            Some(&WindowKind::Merchant { villager, selected }) => Some((villager, selected)),
            _ => None,
        };
        // 만들어져 나오는 칸: 조합은 0, 거래는 2
        let result_slot = if craft_window {
            Some(0)
        } else if merchant.is_some() {
            Some(2)
        } else {
            None
        };
        let trade_before = merchant.and_then(|(v, sel)| {
            let g = self.window.as_ref().unwrap().grid;
            self.merchant_result(v, sel, g[0], g[1])
        });
        let result_before = match result_slot {
            Some(i) => self.window_slots()[i],
            None => None,
        };

        let before_slots = self.window_slots();
        let before = totals(&before_slots, self.carried, result_slot);
        let snapshot = (
            self.inventory,
            self.window.clone(),
            self.carried,
            self.window_slots(),
        );

        // 클라이언트가 말한 대로 적용. 해시된 슬롯을 우리 아이템으로 되돌립니다.
        // 이 클릭으로 자리를 떠난 것들 (바뀐 칸들의 원래 내용 + 손에 들고 있던 것): 처음 보는
        // 해시는 이 가운데 같은 종류로 봅니다.
        let moved: Vec<Item> = c
            .changed
            .iter()
            .filter_map(|&(s, _)| usize::try_from(s).ok())
            .filter_map(|s| before_slots.get(s).copied().flatten())
            .chain(self.carried)
            .collect();
        let all_hashed: Vec<Option<pc::Hashed>> = c
            .changed
            .iter()
            .map(|&(_, h)| h)
            .chain(std::iter::once(c.carried))
            .collect();
        let mut resolved = resolve_all(&all_hashed, &moved, &before_slots);
        let carried_after = resolved.pop().flatten();
        let mut chest_changes = Vec::new();
        let mut guild_changes = Vec::new();
        let mut ender_changes = Vec::new();
        for (&(slot, _), item) in c.changed.iter().zip(resolved) {
            let Ok(slot) = usize::try_from(slot) else {
                continue;
            };
            match self.slot_mut(slot) {
                Some(SlotRef::Inventory(s)) => *s = item,
                Some(SlotRef::Chest(d, p, i)) => chest_changes.push((d, p, i, item)),
                Some(SlotRef::Guild(g, p, i)) => guild_changes.push((g, p, i, item)),
                Some(SlotRef::Ender(i)) => ender_changes.push((i, item)),
                Some(SlotRef::Result) | None => {}
            }
        }
        self.carried = carried_after;
        let mut after_slots = self.window_slots();
        for &(_, _, i, item) in &chest_changes {
            after_slots[i] = item;
        }
        for &(_, _, i, item) in &guild_changes {
            after_slots[i] = item;
        }
        for &(i, item) in &ender_changes {
            after_slots[i] = item;
        }
        let after = totals(&after_slots, self.carried, result_slot);

        // 늘어난 아이템이 있으면: 조합·거래 결과를 꺼낸 것만 허용
        let took_result = result_slot.is_some_and(|r| c.slot == r as i16);
        let mut ok = true;
        for (&key, &n) in &after {
            let had = before.get(&key).copied().unwrap_or(0);
            if n > had {
                let allowed =
                    took_result && result_before.is_some_and(|r| (r.id, r.custom, r.serial) == key);
                ok &= allowed && self.gamemode != crate::packets::play::GameMode::Spectator;
            }
        }
        // 거래: 받은 만큼 정말 냈는지 (몇 번 거래했는지 서버가 셉니다)
        let mut traded = None;
        if ok
            && took_result
            && let (Some((villager, _)), Some((offer, gives, wants, extra))) =
                (merchant, trade_before)
        {
            let key = (gives.id, gives.custom, gives.serial);
            let gained =
                after.get(&key).copied().unwrap_or(0) - before.get(&key).copied().unwrap_or(0);
            if gained > 0 {
                let times = gained / gives.count;
                let paid = |(id, n): (i32, i32)| {
                    let k = (id, None, 0);
                    before.get(&k).copied().unwrap_or(0) - after.get(&k).copied().unwrap_or(0)
                        >= n * times
                };
                let fits = self
                    .villager(villager)
                    .and_then(|v| v.offers.get(offer).copied())
                    .is_some_and(|o| o.uses + times as u32 <= o.trade.max_uses as u32);
                ok = gained % gives.count == 0 && paid(wants) && extra.is_none_or(paid) && fits;
                traded = Some((villager, offer, times as u32));
            }
        }
        if !ok {
            (self.inventory, self.window, self.carried) = (snapshot.0, snapshot.1, snapshot.2);
            self.send_window().await?;
            return Ok(());
        }
        if let Some((villager, offer, times)) = traded {
            self.after_trade(villager, offer, times).await?;
        }

        if !guild_changes.is_empty() && !self.commit_guild_storage(&before_slots, &guild_changes) {
            // 그사이 다른 사람이 같은 칸을 바꿈: 되돌리고 다시 보냄
            (self.inventory, self.window, self.carried) = (snapshot.0, snapshot.1, snapshot.2);
            self.send_window().await?;
            return Ok(());
        }
        if !ender_changes.is_empty() {
            self.commit_ender(&ender_changes);
        }
        for (d, p, i, item) in chest_changes {
            self.server.containers.set_slot(d, p, i, item);
            self.server.furnaces.wake(d, p);
            self.server.publish(Event::ContainerChanged {
                dimension: d,
                pos: p,
                slot: i,
                item,
            });
        }

        // 던진 것: 줄어든 만큼 바닥에
        if c.slot == pc::OUTSIDE || c.mode == pc::MODE_THROW {
            for (&(id, custom, serial), &had) in &before {
                let now = after.get(&(id, custom, serial)).copied().unwrap_or(0);
                if had > now {
                    self.throw(Item {
                        id,
                        count: had - now,
                        custom,
                        serial,
                    });
                }
            }
        }

        // 조합 결과 다시 계산
        if craft_window {
            let result = self.window_slots()[0];
            if self.window.is_none() {
                self.inventory[0] = result;
            }
            let state = self.next_state_id();
            let body = pc::encode_set_slot(window_id, state, 0, result);
            self.send(pc::CONTAINER_SET_SLOT_ID, &body).await?;
        }
        if merchant.is_some() {
            let result = self.window_slots()[2];
            let state = self.next_state_id();
            let body = pc::encode_set_slot(window_id, state, 2, result);
            self.send(pc::CONTAINER_SET_SLOT_ID, &body).await?;
        }
        self.hands_changed();
        Ok(())
    }

    /// 다른 사람이 같은 상자를 바꿨을 때.
    pub(crate) async fn chest_changed(
        &mut self,
        dimension: Dimension,
        pos: BlockPos,
        slot: usize,
        item: Option<Item>,
    ) -> std::io::Result<()> {
        let Some(w) = &self.window else { return Ok(()) };
        if !matches!(w.kind, WindowKind::Chest { dimension: d, pos: p, .. } if (d, p) == (dimension, pos))
        {
            return Ok(());
        }
        let id = w.id;
        let state = self.next_state_id();
        let body = pc::encode_set_slot(id, state, slot as i16, item);
        self.send(pc::CONTAINER_SET_SLOT_ID, &body).await
    }

    /// 상자가 부서지면 보던 창을 닫습니다.
    pub(crate) async fn chest_removed(
        &mut self,
        dimension: Dimension,
        pos: BlockPos,
    ) -> std::io::Result<()> {
        if self
            .window
            .as_ref()
            .is_some_and(|w| matches!(w.kind, WindowKind::Chest { dimension: d, pos: p, .. } if (d, p) == (dimension, pos)))
        {
            self.close_window(false).await?;
        }
        Ok(())
    }
}

enum SlotRef<'a> {
    Inventory(&'a mut Option<Item>),
    Chest(Dimension, BlockPos, usize),
    /// 길드 창고 (길드, 페이지, 칸).
    Guild(u32, usize, usize),
    /// 개인 엔더상자 칸.
    Ender(usize),
    Result,
}

/// (아이템, 사용자 정의, 일련번호)별 총 개수. 결과 칸(조합 0, 거래 2)은 아직 만들어지지 않은 것이라
/// 셈하지 않습니다.
fn totals(
    slots: &[Option<Item>],
    carried: Option<Item>,
    result_slot: Option<usize>,
) -> HashMap<(i32, Option<u16>, u32), i32> {
    let mut t = HashMap::new();
    let kept = slots
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != result_slot)
        .filter_map(|(_, s)| s.as_ref());
    for item in kept.chain(carried.iter()) {
        *t.entry((item.id, item.custom, item.serial)).or_insert(0) += item.count;
    }
    t
}

/// 해시된 슬롯들 → 우리 아이템. 컴포넌트가 없으면 바닐라 그대로. 있으면 `custom_data` 해시로
/// 찾고, 처음 보는 해시면 이 클릭으로 자리를 떠난 것(`moved`) 가운데 같은 종류로 봅니다.
/// 그 후보가 하나뿐이면 해시를 배워 둡니다 (복권처럼 일련번호가 다른 것도 한 장씩 구별).
fn resolve_all(
    hashed: &[Option<pc::Hashed>],
    moved: &[Item],
    before: &[Option<Item>],
) -> Vec<Option<Item>> {
    use crate::game::custom_items;
    let mut pool: Vec<Item> = moved.to_vec();
    let mut out: Vec<Option<Item>> = Vec::with_capacity(hashed.len());
    let mut pending = Vec::new();
    for (i, h) in hashed.iter().enumerate() {
        let item = h.map(|h| {
            if !h.has_components {
                return Some(h.plain());
            }
            let (custom, serial) = h.custom_data_hash.and_then(custom_items::by_hash)?;
            Some(Item {
                id: h.id,
                count: h.count,
                custom: Some(custom),
                serial,
            })
        });
        match item {
            Some(None) => {
                pending.push(i);
                out.push(None);
            }
            Some(Some(it)) => {
                if let Some(k) = pool.iter().position(|p| p.stacks_with(&it)) {
                    pool.remove(k);
                }
                out.push(Some(it));
            }
            None => out.push(None),
        }
    }
    for i in pending {
        let h = hashed[i].unwrap();
        let same = |p: &Item| p.id == h.id && p.custom.is_some();
        let candidates = pool.iter().filter(|p| same(p)).count();
        let pick = match pool.iter().position(same) {
            Some(k) => Some(pool.remove(k)),
            // 떠난 것 중에 없으면 (클라이언트가 예측만 한 것) 창 안의 같은 종류 — 일련번호 없이
            None => before
                .iter()
                .flatten()
                .find(|p| same(p))
                .map(|p| Item { serial: 0, ..*p }),
        };
        if let (Some(p), Some(hash), 1) = (pick, h.custom_data_hash, candidates) {
            custom_items::learn_hash(hash, p.custom.unwrap(), p.serial);
        }
        out[i] = Some(Item {
            id: h.id,
            count: h.count,
            custom: pick.and_then(|p| p.custom),
            serial: pick.map_or(0, |p| p.serial),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_to_inventory_index() {
        assert_eq!(inventory_index(36), Some(0));
        assert_eq!(inventory_index(44), Some(8));
        assert_eq!(inventory_index(9), Some(9));
        assert_eq!(inventory_index(45), Some(40));
        assert_eq!(inventory_index(5), Some(39)); // 투구
        assert_eq!(inventory_index(0), None);
    }
}
