//! 엔티티 우클릭(`Interact`): 주민과 거래, 소 젖 짜기, 양털 깎기.
//!
//! 거래 창(메뉴 `merchant`)은 칸 0·1에 낼 것, 2에 받을 것. 왼쪽 목록에서 고르면(`Select Trade`)
//! 서버가 가방에서 낼 것을 찾아 칸에 옮겨 줍니다. 받을 것을 꺼내면 [`Session::click`]이 낸 만큼
//! 빠졌는지 확인하고 거래 횟수를 셉니다. 거래하는 동안 주민은 서 있습니다.

use tokio::io::AsyncWrite;

use super::play::Session;
use super::windows::{Window, WindowKind};
use crate::data::{items, menu};
use crate::game::entities::{Kind, dist2};
use crate::game::mobs::{MobKind, villager_meta};
use crate::game::villagers::Villager;
use crate::packets::container::{self as pc, MerchantOffer};
use crate::packets::entity::Meta;
use crate::packets::play::GameMode;
use crate::packets::world::Item;
use crate::protocol::frame::FrameError;
use crate::server::Event;
use crate::text;

/// 몹에 손이 닿는 거리.
const ENTITY_REACH: f64 = 6.0;

/// 거래 하나를 창에 보이는 모양으로.
fn view(v: &Villager) -> Vec<MerchantOffer> {
    v.offers
        .iter()
        .filter_map(|o| {
            Some(MerchantOffer {
                wants: o.wants()?,
                extra: o.extra(),
                gives: o.gives()?,
                disabled: o.disabled(),
                uses: o.uses as i32,
                max_uses: o.trade.max_uses as i32,
                xp: o.trade.xp as i32,
                price_multiplier: o.trade.price_multiplier,
            })
        })
        .collect()
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    pub(super) fn villager(&self, id: i32) -> Option<Villager> {
        match self.server.entities.get(id)?.kind {
            Kind::Mob(m) if !m.dead() => m.villager,
            _ => None,
        }
    }

    /// 낸 두 칸으로 받을 것. 고른 거래가 맞으면 그것, 아니면 맞는 첫 거래 (바닐라처럼).
    /// (거래 번호, 받을 것, 낼 것, 두 번째로 낼 것).
    #[allow(clippy::type_complexity)]
    pub(super) fn merchant_result(
        &self,
        villager: i32,
        selected: usize,
        a: Option<Item>,
        b: Option<Item>,
    ) -> Option<(usize, Item, (i32, i32), Option<(i32, i32)>)> {
        let v = self.villager(villager)?;
        let order = std::iter::once(selected).chain(0..v.offers.len());
        for i in order {
            let Some(o) = v.offers.get(i) else { continue };
            // 칸 0·1을 바꿔 넣어도 됨
            if o.matches(a, b) || o.extra().is_none() && o.matches(b, None) && a.is_none() {
                return Some((i, o.gives()?, o.wants()?, o.extra()));
            }
        }
        None
    }

    pub(super) fn merchant_closed(&self, villager: i32) {
        self.server.entities.update(villager, |e| {
            if let Kind::Mob(m) = &mut e.kind {
                m.trading = false;
            }
        });
    }

    async fn send_offers(&mut self, window: i32, v: &Villager) -> std::io::Result<()> {
        let body = pc::encode_merchant_offers(window, &view(v), v.level as i32, v.xp as i32, true);
        self.send(pc::MERCHANT_OFFERS_ID, &body).await
    }

    async fn open_merchant(&mut self, villager: i32, v: Villager) -> std::io::Result<()> {
        self.close_window(false).await?;
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        self.window = Some(Window {
            id,
            kind: WindowKind::Merchant {
                villager,
                selected: 0,
            },
            grid: [None; 9],
        });
        let me = self.position;
        self.server.entities.update(villager, |e| {
            if let Kind::Mob(m) = &mut e.kind {
                m.trading = true;
            }
            let (dx, dz) = (me.0 - e.pos.0, me.2 - e.pos.2);
            e.yaw = (-dx.atan2(dz)).to_degrees() as f32;
        });
        let title = text::plain(v.display_name());
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, menu::MERCHANT, &title),
        )
        .await?;
        self.send_offers(id, &v).await?;
        self.send_window().await?;
        let pos = self.server.entities.get(villager).map(|e| e.pos);
        if let Some(p) = pos {
            self.sound_at(p, "entity.villager.trade");
        }
        Ok(())
    }

    /// 거래를 `times`번 함: 주민 경험치·레벨, 새 목록.
    pub(super) async fn after_trade(
        &mut self,
        villager: i32,
        offer: usize,
        times: u32,
    ) -> std::io::Result<()> {
        let result = self.server.entities.update(villager, |e| {
            let Kind::Mob(m) = &mut e.kind else {
                return None;
            };
            let v = m.villager.as_mut()?;
            let up = v.traded(offer, times);
            Some((v.clone(), up, e.dimension, e.pos))
        });
        let Some(Some((v, leveled, dimension, pos))) = result else {
            return Ok(());
        };
        if leveled {
            self.server.publish(Event::EntityData {
                dimension,
                id: villager,
                meta: vec![villager_meta(&v)],
            });
            self.sound_at(pos, "entity.villager.celebrate");
        } else {
            self.sound_at(pos, "entity.villager.yes");
        }
        if let Some(id) = self.window.as_ref().map(|w| w.id) {
            self.send_offers(id, &v).await?;
        }
        Ok(())
    }

    /// 왼쪽 목록에서 거래를 고름: 칸에 있던 것을 돌려받고, 가방에서 낼 것을 옮겨 놓습니다.
    pub(super) async fn select_trade(&mut self, index: i32) -> Result<(), FrameError> {
        let Some(Window {
            kind: WindowKind::Merchant { villager, .. },
            ..
        }) = self.window
        else {
            return Ok(());
        };
        let Some(v) = self.villager(villager) else {
            return Ok(());
        };
        let Some(offer) = usize::try_from(index).ok().and_then(|i| v.offers.get(i)) else {
            return Ok(());
        };
        let wants = [offer.wants(), offer.extra()];
        let w = self.window.as_mut().unwrap();
        w.kind = WindowKind::Merchant {
            villager,
            selected: index as usize,
        };
        let back: Vec<Item> = w.grid[..2].iter_mut().filter_map(Option::take).collect();
        for item in back {
            let taken = self.give(item).await?;
            if taken < item.count {
                self.throw(item.with_count(item.count - taken));
            }
        }
        for (slot, want) in wants.into_iter().enumerate() {
            let Some((id, _)) = want else { continue };
            let mut need = items::max_stack(id);
            let mut got = 0;
            for s in (36..45).chain(9..36) {
                if need == 0 {
                    break;
                }
                if let Some(have) = self.inventory[s]
                    && have.id == id
                    && have.custom.is_none()
                {
                    let n = have.count.min(need);
                    self.inventory[s] = (have.count > n).then(|| have.with_count(have.count - n));
                    need -= n;
                    got += n;
                }
            }
            if got > 0 {
                self.window.as_mut().unwrap().grid[slot] = Some(Item::new(id, got));
            }
        }
        self.send_window().await?;
        self.hands_changed();
        Ok(())
    }

    fn sound_at(&self, pos: (f64, f64, f64), name: &str) {
        if let Some(sound) = crate::data::sound_id(name) {
            self.server.publish(Event::Sound {
                dimension: self.dimension,
                pos,
                sound,
                source: 6, // neutral
                volume: 1.0,
                pitch: 1.0,
            });
        }
    }

    /// `Interact`: 엔티티를 우클릭.
    pub(super) async fn interact_entity(
        &mut self,
        target: i32,
        hand: i32,
        _sneaking: bool,
    ) -> Result<(), FrameError> {
        if self.vitals.dead || self.gamemode == GameMode::Spectator {
            return Ok(());
        }
        let Some(e) = self.server.entities.get(target) else {
            return Ok(());
        };
        if e.dimension != self.dimension
            || dist2(e.pos, self.position) > ENTITY_REACH * ENTITY_REACH
        {
            return Ok(());
        }
        let Kind::Mob(m) = &e.kind else {
            return Ok(());
        };
        if m.dead() {
            return Ok(());
        }
        let slot = if hand == 0 { 36 + self.held } else { 45 };
        let held = self.inventory[slot].filter(|i| i.custom.is_none());
        let held_name = held.map(|i| items::name(i.id));
        match m.kind {
            MobKind::Villager if hand == 0 => {
                if let Some(v) = m.villager.clone().filter(Villager::trades) {
                    self.open_merchant(target, v).await?;
                } else {
                    // 무직·멍청이는 고개만 젓습니다
                    self.sound_at(e.pos, "entity.villager.no");
                }
            }
            MobKind::Cow if held_name == Some("bucket") => {
                let Some(milk) = items::by_name("milk_bucket") else {
                    return Ok(());
                };
                let bucket = held.unwrap();
                if self.gamemode == GameMode::Creative {
                    self.give(Item::new(milk, 1)).await?;
                } else if bucket.count == 1 {
                    self.inventory[slot] = Some(Item::new(milk, 1));
                    self.sync_slot(slot).await?;
                } else {
                    self.inventory[slot] = Some(bucket.with_count(bucket.count - 1));
                    self.sync_slot(slot).await?;
                    let taken = self.give(Item::new(milk, 1)).await?;
                    if taken == 0 {
                        self.throw(Item::new(milk, 1));
                    }
                }
                self.hands_changed();
                self.sound_at(e.pos, "entity.cow.milk");
            }
            MobKind::Sheep if held_name == Some("shears") && !m.sheared => {
                let changed = self.server.entities.update(target, |e| match &mut e.kind {
                    Kind::Mob(m) if !m.sheared => {
                        m.sheared = true;
                        true
                    }
                    _ => false,
                });
                if changed != Some(true) {
                    return Ok(());
                }
                self.server.publish(Event::EntityData {
                    dimension: e.dimension,
                    id: target,
                    meta: vec![Meta::Byte(18, 0x10)],
                });
                if let Some(wool) = items::by_name("white_wool") {
                    let n = 1 + (crate::game::rand_u32() % 3) as i32;
                    let at = (
                        e.pos.0.floor() as i32,
                        e.pos.1.floor() as i32 + 1,
                        e.pos.2.floor() as i32,
                    );
                    self.server.entities.drop_at_block(
                        self.server,
                        e.dimension,
                        at,
                        Item::new(wool, n),
                    );
                }
                self.sound_at(e.pos, "entity.sheep.shear");
            }
            _ => {}
        }
        Ok(())
    }
}
