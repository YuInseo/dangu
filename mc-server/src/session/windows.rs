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
    Chest { dimension: Dimension, pos: BlockPos },
}

#[derive(Debug, Clone)]
pub struct Window {
    pub id: i32,
    pub kind: WindowKind,
    /// 작업대의 3×3 조합 칸.
    grid: [Option<Item>; 9],
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
    fn next_state_id(&mut self) -> i32 {
        self.state_id = self.state_id.wrapping_add(1) & 0x7FFF;
        self.state_id
    }

    /// 플레이어 인벤토리 전체를 다시 보냅니다.
    pub(super) async fn send_inventory(&mut self) -> std::io::Result<()> {
        self.inventory[0] = items::craft(&self.inventory[INV_GRID], 2);
        let state = self.next_state_id();
        let body = pc::encode_set_content(0, state, &self.inventory, self.carried);
        self.send(pc::CONTAINER_SET_CONTENT_ID, &body).await
    }

    /// 서버가 바꾼 인벤토리 한 칸을 알립니다 (어느 창이 열려 있든 통함).
    pub(super) async fn sync_slot(&mut self, menu_slot: usize) -> std::io::Result<()> {
        if let Some(i) = inventory_index(menu_slot) {
            let body = pw::encode_player_inventory(i, self.inventory[menu_slot]);
            self.send(pw::SET_PLAYER_INVENTORY_ID, &body).await?;
        }
        Ok(())
    }

    /// 주운 아이템을 넣습니다: 같은 것이 있는 칸부터, 그다음 빈 칸 (핫바 먼저).
    /// 넣은 개수를 돌려줍니다.
    pub(super) async fn give(&mut self, item: Item) -> std::io::Result<i32> {
        let max = items::max_stack(item.id);
        let order: Vec<usize> = (36..45).chain(9..36).collect();
        let mut left = item.count;
        let mut changed = Vec::new();
        for pass in 0..2 {
            for &s in &order {
                if left == 0 {
                    break;
                }
                match &mut self.inventory[s] {
                    Some(have) if pass == 0 && have.id == item.id && have.count < max => {
                        let n = (max - have.count).min(left);
                        have.count += n;
                        left -= n;
                        changed.push(s);
                    }
                    slot @ None if pass == 1 => {
                        let n = max.min(left);
                        *slot = Some(Item {
                            id: item.id,
                            count: n,
                        });
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
    pub(super) async fn drop_held(&mut self, whole_stack: bool) -> std::io::Result<()> {
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
        self.throw(Item {
            id: item.id,
            count: n,
        });
        self.sync_slot(slot).await?;
        self.hands_changed();
        Ok(())
    }

    /// 바라보는 쪽으로 던집니다.
    fn throw(&self, item: Item) {
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

    pub(super) async fn open_crafting(&mut self) -> std::io::Result<()> {
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

    pub(super) async fn open_chest(&mut self, pos: BlockPos) -> std::io::Result<()> {
        self.close_window(false).await?;
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        self.window = Some(Window {
            id,
            kind: WindowKind::Chest {
                dimension: self.dimension,
                pos,
            },
            grid: [None; 9],
        });
        let title = text::plain("상자");
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, menu::GENERIC_9X3, &title),
        )
        .await?;
        self.send_window().await
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
            WindowKind::Chest { dimension, pos } => self.server.containers.get(*dimension, *pos),
        };
        slots.extend_from_slice(&self.inventory[9..45]);
        slots
    }

    async fn send_window(&mut self) -> std::io::Result<()> {
        let Some(id) = self.window.as_ref().map(|w| w.id) else {
            return self.send_inventory().await;
        };
        let slots = self.window_slots();
        let state = self.next_state_id();
        let body = pc::encode_set_content(id, state, &slots, self.carried);
        self.send(pc::CONTAINER_SET_CONTENT_ID, &body).await
    }

    /// 창을 닫습니다. 조합 칸에 남은 것은 인벤토리로 (자리가 없으면 떨어뜨림).
    pub(super) async fn close_window(&mut self, by_client: bool) -> std::io::Result<()> {
        let leftovers: Vec<Item> = match self.window.take() {
            Some(w) => {
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

    /// 창 번호의 칸 하나 읽기·쓰기.
    fn slot_mut(&mut self, slot: usize) -> Option<SlotRef<'_>> {
        match &mut self.window {
            None => (slot < 46).then(|| SlotRef::Inventory(&mut self.inventory[slot])),
            Some(w) => {
                let own = match w.kind {
                    WindowKind::Crafting => 10,
                    WindowKind::Chest { .. } => CHEST_SLOTS,
                };
                if slot < own {
                    match &w.kind {
                        WindowKind::Crafting => {
                            if slot == 0 {
                                Some(SlotRef::Result)
                            } else {
                                Some(SlotRef::Inventory(&mut w.grid[slot - 1]))
                            }
                        }
                        WindowKind::Chest { dimension, pos } => {
                            Some(SlotRef::Chest(*dimension, *pos, slot))
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

    pub(super) async fn click(&mut self, c: Click) -> Result<(), FrameError> {
        let window_id = self.window.as_ref().map_or(0, |w| w.id);
        if c.window != window_id {
            return Ok(()); // 이미 닫힌 창에 대한 클릭
        }
        let craft_window = matches!(
            self.window.as_ref().map(|w| &w.kind),
            None | Some(WindowKind::Crafting)
        );
        let result_before = if craft_window {
            self.window_slots()[0]
        } else {
            None
        };

        let before = totals(&self.window_slots(), self.carried, craft_window);
        let snapshot = (
            self.inventory,
            self.window.clone(),
            self.carried,
            self.window_slots(),
        );

        // 클라이언트가 말한 대로 적용
        let mut chest_changes = Vec::new();
        for &(slot, item) in &c.changed {
            let Ok(slot) = usize::try_from(slot) else {
                continue;
            };
            match self.slot_mut(slot) {
                Some(SlotRef::Inventory(s)) => *s = item,
                Some(SlotRef::Chest(d, p, i)) => chest_changes.push((d, p, i, item)),
                Some(SlotRef::Result) | None => {}
            }
        }
        let carried_before = self.carried;
        self.carried = c.carried;
        let mut after_slots = self.window_slots();
        for &(_, _, i, item) in &chest_changes {
            after_slots[i] = item;
        }
        let after = totals(&after_slots, self.carried, craft_window);

        // 늘어난 아이템이 있으면: 조합 결과를 꺼낸 것만 허용
        let took_result = craft_window && c.slot == 0;
        let mut ok = true;
        for (&id, &n) in &after {
            let had = before.get(&id).copied().unwrap_or(0);
            if n > had {
                let allowed = took_result && result_before.is_some_and(|r| r.id == id);
                ok &= allowed && self.gamemode != crate::packets::play::GameMode::Spectator;
            }
        }
        if !ok {
            (self.inventory, self.window, self.carried) = (snapshot.0, snapshot.1, snapshot.2);
            self.send_window().await?;
            return Ok(());
        }

        for (d, p, i, item) in chest_changes {
            self.server.containers.set_slot(d, p, i, item);
            self.server.publish(Event::ContainerChanged {
                dimension: d,
                pos: p,
                slot: i,
                item,
            });
        }

        // 던진 것: 줄어든 만큼 바닥에
        if c.slot == pc::OUTSIDE || c.mode == pc::MODE_THROW {
            for (&id, &had) in &before {
                let now = after.get(&id).copied().unwrap_or(0);
                if had > now {
                    self.throw(Item {
                        id,
                        count: had - now,
                    });
                }
            }
        }
        let _ = carried_before;

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
        self.hands_changed();
        Ok(())
    }

    /// 다른 사람이 같은 상자를 바꿨을 때.
    pub(super) async fn chest_changed(
        &mut self,
        dimension: Dimension,
        pos: BlockPos,
        slot: usize,
        item: Option<Item>,
    ) -> std::io::Result<()> {
        let Some(w) = &self.window else { return Ok(()) };
        if w.kind != (WindowKind::Chest { dimension, pos }) {
            return Ok(());
        }
        let id = w.id;
        let state = self.next_state_id();
        let body = pc::encode_set_slot(id, state, slot as i16, item);
        self.send(pc::CONTAINER_SET_SLOT_ID, &body).await
    }

    /// 상자가 부서지면 보던 창을 닫습니다.
    pub(super) async fn chest_removed(
        &mut self,
        dimension: Dimension,
        pos: BlockPos,
    ) -> std::io::Result<()> {
        if self
            .window
            .as_ref()
            .is_some_and(|w| w.kind == (WindowKind::Chest { dimension, pos }))
        {
            self.close_window(false).await?;
        }
        Ok(())
    }
}

enum SlotRef<'a> {
    Inventory(&'a mut Option<Item>),
    Chest(Dimension, BlockPos, usize),
    Result,
}

/// 아이템별 총 개수. 조합 결과 칸(0)은 아직 만들어지지 않은 것이라 셈하지 않습니다.
fn totals(slots: &[Option<Item>], carried: Option<Item>, skip_result: bool) -> HashMap<i32, i32> {
    let mut t = HashMap::new();
    let start = if skip_result { 1 } else { 0 };
    for item in slots.iter().skip(start).flatten().chain(carried.iter()) {
        *t.entry(item.id).or_insert(0) += item.count;
    }
    t
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
