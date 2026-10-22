//! 길드 청크 지도 (9×5칸, 한 칸 = 한 청크, 위가 북쪽)와 작물 성장 촉진제.
//!
//! - 초록: 우리 길드 청크 (작물 단계가 있으면 밀, 개수 = 단계) · 빨강: 다른 길드 · 노랑: 살 수 있는
//!   빈 청크(우리 땅 옆) · 회색: 빈 청크 · 반짝임: 지금 서 있는 청크
//! - 우리 청크 Shift+좌클릭(또는 촉진제를 우클릭해 연 "쓰기 모드"에서 좌클릭): 촉진제 하나를 써서
//!   그 청크 작물 단계 +1
//! - 노란 청크 Shift+좌클릭: 그 청크 사기
//! - 아래 줄: 서·북·여기·남·동으로 옮기기

use std::collections::HashMap;

use tokio::io::AsyncWrite;

use super::gui::{BOOSTER, Page};
use super::{Actor, Guild, guilds};
use crate::packets::world::Item;
use crate::protocol::frame::FrameError;
use crate::session::play::Session;
use crate::text;

const COLS: i32 = 9;
const ROWS: i32 = 5;

fn chunk_at(center: (i32, i32), slot: usize) -> Option<(i32, i32)> {
    let slot = slot as i32;
    (slot < COLS * ROWS).then(|| {
        (
            center.0 + slot % COLS - COLS / 2,
            center.1 + slot / COLS - ROWS / 2,
        )
    })
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    fn booster_slot(&self) -> Option<usize> {
        let index = crate::game::custom_items::index_of(BOOSTER)?;
        (9..45)
            .rev()
            .find(|&s| self.inventory[s].is_some_and(|i| i.custom == Some(index)))
    }

    fn booster_count(&self) -> i32 {
        let Some(index) = crate::game::custom_items::index_of(BOOSTER) else {
            return 0;
        };
        self.inventory[9..45]
            .iter()
            .flatten()
            .filter(|i| i.custom == Some(index))
            .map(|i| i.count)
            .sum()
    }

    pub(crate) fn map_slots(
        &self,
        slots: &mut [Option<Item>],
        g: &Guild,
        center: (i32, i32),
        boost: bool,
    ) {
        // 이 인스턴스·차원의 모든 길드 땅
        let lands: HashMap<(i32, i32), (u32, String, u8)> = guilds().read(|d| {
            d.guilds
                .iter()
                .flat_map(|gu| {
                    gu.lands
                        .iter()
                        .filter(|l| l.instance == self.server.name && l.dimension == self.dimension)
                        .map(move |l| (l.chunk, (gu.id, gu.name.clone(), l.crop_level)))
                })
                .collect()
        });
        let here = super::land::chunk_of(self.position.0, self.position.2);
        let mine_touch = |c: (i32, i32)| {
            [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dz)| {
                lands
                    .get(&(c.0 + dx, c.1 + dz))
                    .is_some_and(|l| l.0 == g.id)
            })
        };
        for (s, slot) in slots.iter_mut().enumerate().take((COLS * ROWS) as usize) {
            let c = chunk_at(center, s).unwrap();
            let coords = format!(
                "블록 {}, {} ~ {}, {}",
                c.0 * 16,
                c.1 * 16,
                c.0 * 16 + 15,
                c.1 * 16 + 15
            );
            let (item, name, color, mut lore, count) = match lands.get(&c) {
                Some((id, _, level)) if *id == g.id => {
                    let mult = super::config::with(|cf| cf.crop_multiplier(*level));
                    (
                        if *level > 0 {
                            "wheat"
                        } else {
                            "lime_stained_glass_pane"
                        },
                        format!("우리 청크 ({}, {})", c.0, c.1),
                        "green",
                        vec![
                            coords,
                            format!("작물 성장 {}단계 (×{mult:.2})", level),
                            if boost {
                                "좌클릭: 촉진제 쓰기".into()
                            } else {
                                "Shift+좌클릭: 촉진제 쓰기".into()
                            },
                        ],
                        (*level).max(1) as i32,
                    )
                }
                Some((_, name, _)) => (
                    "red_stained_glass_pane",
                    format!("{name} 길드 ({}, {})", c.0, c.1),
                    "red",
                    vec![coords],
                    1,
                ),
                None if mine_touch(c) || g.lands.is_empty() => (
                    "yellow_stained_glass_pane",
                    format!("빈 청크 ({}, {})", c.0, c.1),
                    "yellow",
                    vec![
                        coords,
                        format!(
                            "Shift+좌클릭: 사기 ({}원)",
                            crate::plugins::hud::number(super::land::price())
                        ),
                    ],
                    1,
                ),
                None => (
                    "gray_stained_glass_pane",
                    format!("빈 청크 ({}, {})", c.0, c.1),
                    "gray",
                    vec![coords],
                    1,
                ),
            };
            let at_here = c == here;
            if at_here {
                lore.insert(0, "★ 지금 여기".into());
            }
            *slot = self
                .icon(s, item, &name, color, lore, at_here)
                .map(|i| i.with_count(count.clamp(1, 64)));
        }
        let arrow = |s: usize, name: &str| self.icon(s, "arrow", name, "white", Vec::new(), false);
        slots[MAP_WEST] = arrow(MAP_WEST, "← 서쪽");
        slots[MAP_NORTH] = arrow(MAP_NORTH, "↑ 북쪽");
        slots[MAP_SOUTH] = arrow(MAP_SOUTH, "↓ 남쪽");
        slots[MAP_EAST] = arrow(MAP_EAST, "→ 동쪽");
        slots[MAP_HERE] = self.icon(
            MAP_HERE,
            "compass",
            "지금 있는 곳으로",
            "aqua",
            vec![format!("가운데 청크 ({}, {})", center.0, center.1)],
            false,
        );
        slots[MAP_INFO] = self.icon(
            MAP_INFO,
            "bone_meal",
            "작물 성장 촉진제",
            "green",
            vec![
                format!("가진 개수  {}", self.booster_count()),
                "우리 청크에 쓰면 그 청크 작물만 빨리 자람".into(),
                format!("최고 {}단계", super::config::with(|c| c.max_crop_level())),
            ],
            boost,
        );
    }

    pub(crate) async fn map_click(
        &mut self,
        center: (i32, i32),
        boost: bool,
        slot: usize,
        shift: bool,
        right: bool,
    ) -> Result<(), FrameError> {
        let set = |s: &mut Self, center: (i32, i32)| {
            if let Some(w) = &mut s.window {
                w.kind = crate::session::windows::WindowKind::Guild(Page::Map { center, boost });
            }
        };
        match slot {
            MAP_WEST => {
                set(self, (center.0 - 4, center.1));
                return Ok(());
            }
            MAP_EAST => {
                set(self, (center.0 + 4, center.1));
                return Ok(());
            }
            MAP_NORTH => {
                set(self, (center.0, center.1 - 2));
                return Ok(());
            }
            MAP_SOUTH => {
                set(self, (center.0, center.1 + 2));
                return Ok(());
            }
            MAP_HERE => {
                let here = super::land::chunk_of(self.position.0, self.position.2);
                set(self, here);
                return Ok(());
            }
            _ => {}
        }
        let Some(chunk) = chunk_at(center, slot) else {
            return Ok(());
        };
        if right {
            return Ok(());
        }
        let me = Actor {
            uuid: self.profile.uuid,
            name: self.profile.name.clone(),
        };
        let owner = guilds().read(|d| {
            d.owner_of(&self.server.name, self.dimension, chunk)
                .map(|g| g.id)
        });
        let my_guild = guilds().read(|d| d.of(me.uuid).map(|g| g.id));
        match owner {
            Some(o) if Some(o) == my_guild && (shift || boost) => self.use_booster(chunk).await,
            None if shift => self.claim_chunk(chunk).await,
            _ => Ok(()),
        }
    }

    /// 촉진제 하나를 써서 그 청크 작물 단계 +1.
    async fn use_booster(&mut self, chunk: (i32, i32)) -> Result<(), FrameError> {
        let Some(slot) = self.booster_slot() else {
            return self.error("작물 성장 촉진제가 없습니다.").await;
        };
        let me = Actor {
            uuid: self.profile.uuid,
            name: self.profile.name.clone(),
        };
        let r = guilds().write(|d| d.crop_upgrade(&me, &self.server.name, self.dimension, chunk));
        match r {
            Ok(m) => {
                if let Some(it) = self.inventory[slot] {
                    self.inventory[slot] =
                        Some(it.with_count(it.count - 1)).filter(|i| i.count > 0);
                    self.sync_slot(slot).await?;
                }
                self.tell_guild(&format!("[길드] {}님: {m}", me.name));
                Ok(())
            }
            Err(e) => self.error(&e).await,
        }
    }

    /// 손의 촉진제를 우클릭: 지도를 쓰기 모드로 엽니다. 썼으면 true.
    pub(crate) async fn try_booster(&mut self, off_hand: bool) -> Result<bool, FrameError> {
        let item = if off_hand {
            self.off_hand()
        } else {
            self.main_hand()
        };
        let index = crate::game::custom_items::index_of(BOOSTER);
        if item.is_none_or(|i| i.custom.is_none() || i.custom != index) {
            return Ok(false);
        }
        if guilds().read(|d| d.of(self.profile.uuid).is_none()) {
            self.error("길드에 들어가 있어야 쓸 수 있습니다.").await?;
            return Ok(true);
        }
        let center = super::land::chunk_of(self.position.0, self.position.2);
        self.open_guild(Page::Map {
            center,
            boost: true,
        })
        .await?;
        Ok(true)
    }

    /// 지도에서 고른 청크 사기.
    async fn claim_chunk(&mut self, chunk: (i32, i32)) -> Result<(), FrameError> {
        let me = Actor {
            uuid: self.profile.uuid,
            name: self.profile.name.clone(),
        };
        let price = super::land::price();
        let check =
            guilds()
                .read(|d| d.clone())
                .claim(&me, &self.server.name, self.dimension, chunk, 0);
        if let Err(e) = check {
            return self.error(&e).await;
        }
        if !self.pay(price).await {
            return Ok(());
        }
        let r = guilds()
            .write(|d| d.claim(&me, &self.server.name, self.dimension, chunk, super::now()));
        if r.is_err() {
            crate::dambi::child::shared_add(me.uuid, "money", price).await;
        }
        match r {
            Ok(m) => self.message(&text::colored(&m, "green")).await?,
            Err(e) => self.error(&e).await?,
        }
        Ok(())
    }
}

use super::gui::{MAP_EAST, MAP_HERE, MAP_INFO, MAP_NORTH, MAP_SOUTH, MAP_WEST};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_maps_to_chunks() {
        assert_eq!(chunk_at((0, 0), 0), Some((-4, -2)));
        assert_eq!(chunk_at((0, 0), 22), Some((0, 0)));
        assert_eq!(chunk_at((10, 5), 44), Some((14, 7)));
        assert_eq!(chunk_at((0, 0), 45), None);
    }
}
