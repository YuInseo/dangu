//! 블록 부수기·놓기, 인벤토리, 손에 든 것.

use tokio::io::AsyncWrite;

use super::play::Session;
use crate::data::{blocks, items};
use crate::game::rand_u32;
use crate::packets::play::GameMode;
use crate::packets::world::{
    self as pw, BlockPos, Item, PlayerAction, PlayerActionPacket, UseItemOn, add,
};
use crate::protocol::frame::FrameError;
use crate::server::Event;
use crate::world::placement::{self, Context};

/// 손이 닿는 거리 (블록 중심까지). 바닐라 기본 4.5~5에 여유를 둡니다.
const REACH: f64 = 6.0;

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    pub(crate) async fn player_action(&mut self, a: PlayerActionPacket) -> Result<(), FrameError> {
        // 크리에이티브는 누르자마자, 서바이벌은 다 캤다고 알려 올 때 부서집니다.
        // 채굴 속도 검사는 아직 하지 않습니다 (클라이언트를 믿습니다).
        let breaks = matches!(
            (self.gamemode, a.action),
            (GameMode::Creative, PlayerAction::StartDestroy)
                | (GameMode::Survival, PlayerAction::StopDestroy)
        );
        if breaks && !self.vitals.dead && self.can_reach(a.pos) && self.in_world(a.pos) {
            if self.may_build_at(a.pos) {
                self.break_block(a.pos).await?;
            } else {
                // 길드 땅: 부수지 않음 (아래 확인 응답으로 클라이언트가 되돌림)
                self.deny_build().await?;
            }
        }
        match a.action {
            // 26.3 번호: 4 = 묶음째 버리기, 5 = 하나 버리기, 6 = 아이템 쓰기 그만
            PlayerAction::Other(4) => self.drop_held(true).await?,
            PlayerAction::Other(5) => self.drop_held(false).await?,
            PlayerAction::Other(6) => self.stop_using_item(),
            PlayerAction::Other(7) => self.swap_hands().await?,
            _ => {}
        }
        if !matches!(
            a.action,
            PlayerAction::ChangeDestroyDirection | PlayerAction::Other(_)
        ) {
            self.send(
                pw::BLOCK_CHANGED_ACK_ID,
                &pw::encode_block_changed_ack(a.sequence),
            )
            .await?;
        }
        Ok(())
    }

    async fn break_block(&mut self, pos: BlockPos) -> std::io::Result<()> {
        let world = &self.server.world;
        let old = world.get(self.dimension, pos.0, pos.1, pos.2);
        if blocks::is_air(old)
            || old == crate::data::block::WATER
            || old == crate::data::block::LAVA
        {
            return Ok(());
        }
        // 서바이벌: 기반암은 못 부수고, 맞는 도구면 전리품이 떨어집니다.
        if self.gamemode == GameMode::Survival {
            if old == crate::data::block::BEDROCK {
                return Ok(());
            }
            self.exhaust(0.005);
            if let Some((custom, drops)) = crate::plugins::contents::block_item(old) {
                // 장비 광석은 원석을, 그 밖의 사용자 정의 블록은 그 아이템을 떨어뜨림
                if self.ore_mined(custom, pos).await? {
                } else if drops && let Some(def) = crate::game::custom_items::get(custom) {
                    let item = Item {
                        id: def.base,
                        count: 1,
                        custom: Some(custom),
                        serial: 0,
                    };
                    self.server
                        .entities
                        .drop_at_block(self.server, self.dimension, pos, item);
                }
            } else if items::can_harvest(blocks::block_of(old), self.main_hand())
                && let Some(drop) = items::block_drop(old, rand_u32() % 1000, rand_u32())
            {
                self.server
                    .entities
                    .drop_at_block(self.server, self.dimension, pos, drop);
            }
        }
        // 상자: 내용을 쏟고, 보던 사람의 창을 닫습니다.
        if matches!(
            blocks::name(blocks::block_of(old)),
            "chest"
                | "trapped_chest"
                | "barrel"
                | "furnace"
                | "blast_furnace"
                | "smoker"
                | "dispenser"
                | "dropper"
                | "hopper"
        ) {
            for item in self.server.containers.take_all(self.dimension, pos) {
                self.server
                    .entities
                    .drop_at_block(self.server, self.dimension, pos, item);
            }
            self.chest_removed(self.dimension, pos).await?;
        }
        // 물에 잠긴 블록을 부수면 물이 남습니다.
        let after = if blocks::property(old, "waterlogged") == Some("true") {
            crate::data::block::WATER
        } else {
            crate::data::block::AIR
        };
        self.set_block(pos, after, Some(old));
        if self.gamemode == GameMode::Survival {
            self.guild_block_broken(pos, old).await?;
            self.rank_autoplant(pos, old);
        }
        // 문·침대는 나머지 반쪽도 함께
        if let Some((other, other_state)) = placement::partner(pos, old)
            && world.get(self.dimension, other.0, other.1, other.2) == other_state
        {
            self.set_block(other, crate::data::block::AIR, None);
        }
        Ok(())
    }

    pub(crate) async fn use_item_on(&mut self, u: UseItemOn) -> Result<(), FrameError> {
        if self.vitals.dead {
            return Ok(());
        }
        // 길드 땅: 누른 블록과 놓일 자리 둘 다 허락돼야 (상자·문·놓기)
        if !self.may_build_at(u.pos) || !self.may_build_at(add(u.pos, u.face.offset())) {
            self.deny_build().await?;
            self.send(
                pw::BLOCK_CHANGED_ACK_ID,
                &pw::encode_block_changed_ack(u.sequence),
            )
            .await?;
            return Ok(());
        }
        // 작업대·상자를 누르면 엽니다 (웅크리고 있으면 그냥 놓기).
        let clicked = self
            .server
            .world
            .get(self.dimension, u.pos.0, u.pos.1, u.pos.2);
        if !self.vitals.sneaking && self.can_reach(u.pos) && self.gamemode != GameMode::Spectator {
            match blocks::name(blocks::block_of(clicked)) {
                "crafting_table" => {
                    self.send(
                        pw::BLOCK_CHANGED_ACK_ID,
                        &pw::encode_block_changed_ack(u.sequence),
                    )
                    .await?;
                    return Ok(self.open_crafting().await?);
                }
                "chest" | "barrel" | "trapped_chest" | "furnace" | "blast_furnace" | "smoker"
                | "dispenser" | "dropper" | "hopper" => {
                    self.send(
                        pw::BLOCK_CHANGED_ACK_ID,
                        &pw::encode_block_changed_ack(u.sequence),
                    )
                    .await?;
                    return Ok(self.open_chest(u.pos).await?);
                }
                _ => {
                    // 레버·버튼·문·다락문·울타리 문
                    if crate::game::mechanics::redstone::interact(
                        self.server,
                        self.dimension,
                        u.pos,
                        clicked,
                    ) {
                        self.send(
                            pw::BLOCK_CHANGED_ACK_ID,
                            &pw::encode_block_changed_ack(u.sequence),
                        )
                        .await?;
                        self.server.publish(Event::Swing {
                            uuid: self.profile.uuid,
                            entity_id: self.entity_id,
                            off_hand: u.off_hand,
                        });
                        return Ok(());
                    }
                }
            }
        }
        // 부싯돌로 TNT에 불 붙이기
        let held = if u.off_hand {
            self.off_hand()
        } else {
            self.main_hand()
        };
        if held.is_some_and(|i| items::name(i.id) == "flint_and_steel")
            && blocks::name(blocks::block_of(clicked)) == "tnt"
            && self.can_reach(u.pos)
            && self.gamemode != GameMode::Spectator
        {
            crate::game::mechanics::explosion::prime(self.server, self.dimension, u.pos);
            self.send(
                pw::BLOCK_CHANGED_ACK_ID,
                &pw::encode_block_changed_ack(u.sequence),
            )
            .await?;
            return Ok(());
        }
        // 부싯돌로 흑요석 틀에 불 붙이기 → 네더 차원문
        if held.is_some_and(|i| items::name(i.id) == "flint_and_steel")
            && self.can_reach(u.pos)
            && self.gamemode != GameMode::Spectator
            && crate::game::mechanics::portal::try_light(
                self.server,
                self.dimension,
                add(u.pos, u.face.offset()),
            )
        {
            self.send(
                pw::BLOCK_CHANGED_ACK_ID,
                &pw::encode_block_changed_ack(u.sequence),
            )
            .await?;
            return Ok(());
        }
        let placed = self.try_place(&u);
        // 서바이벌에서는 놓은 만큼 줄어듭니다.
        if placed && self.gamemode == GameMode::Survival {
            let slot = if u.off_hand { 45 } else { 36 + self.held };
            if let Some(item) = self.inventory[slot] {
                // 물·용암 양동이는 빈 양동이가 됩니다.
                let name = items::name(item.id);
                self.inventory[slot] = if name.ends_with("_bucket") {
                    items::by_name("bucket").map(|id| Item::new(id, 1))
                } else {
                    Some(Item {
                        count: item.count - 1,
                        ..item
                    })
                    .filter(|i| i.count > 0)
                };
                self.sync_slot(slot).await?;
                self.hands_changed();
            }
        }
        self.send(
            pw::BLOCK_CHANGED_ACK_ID,
            &pw::encode_block_changed_ack(u.sequence),
        )
        .await?;
        if placed {
            self.server.publish(Event::Swing {
                uuid: self.profile.uuid,
                entity_id: self.entity_id,
                off_hand: u.off_hand,
            });
        }
        Ok(())
    }

    fn try_place(&mut self, u: &UseItemOn) -> bool {
        if matches!(self.gamemode, GameMode::Adventure | GameMode::Spectator)
            || !self.can_reach(u.pos)
        {
            return false;
        }
        let item = if u.off_hand {
            self.off_hand()
        } else {
            self.main_hand()
        };
        // ItemsAdder식 사용자 정의 블록이면 그 상태로 (소리 블록 상태를 빌림)
        let custom_state = item
            .and_then(|i| i.custom)
            .and_then(crate::plugins::contents::block_state);
        let block = match custom_state {
            Some(s) => blocks::block_of(s),
            None => {
                let Some(b) = item
                    .filter(|i| {
                        i.custom.is_none()
                            || crate::game::custom_items::get(i.custom.unwrap())
                                .is_some_and(|c| c.model.is_none())
                    })
                    .and_then(|i| blocks::block_for_item(i.id))
                else {
                    return false;
                };
                b
            }
        };

        // 풀·물처럼 덮어써지는 블록을 눌렀으면 그 자리에, 아니면 누른 면 바로 앞에.
        let world = &self.server.world;
        let clicked = world.get(self.dimension, u.pos.0, u.pos.1, u.pos.2);
        let target = if blocks::is_replaceable(clicked) {
            u.pos
        } else {
            add(u.pos, u.face.offset())
        };
        if !self.in_world(target) {
            return false;
        }
        let replacing = world.get(self.dimension, target.0, target.1, target.2);
        if !blocks::is_replaceable(replacing) {
            return false;
        }

        let ctx = Context {
            face: u.face,
            cursor_y: u.cursor.1,
            yaw: self.yaw,
            pitch: self.pitch,
            replacing,
        };
        let state = custom_state.unwrap_or_else(|| placement::state_for(block, &ctx));

        // 두 칸짜리는 나머지 자리도 비어 있어야 합니다.
        let partner = placement::partner(target, state);
        if let Some((p, _)) = partner
            && (!self.in_world(p)
                || !blocks::is_replaceable(world.get(self.dimension, p.0, p.1, p.2)))
        {
            return false;
        }
        self.set_block(target, state, None);
        if let Some((p, s)) = partner {
            self.set_block(p, s, None);
        }
        true
    }

    pub(crate) fn set_block(&mut self, pos: BlockPos, state: u32, broken: Option<u32>) {
        self.server
            .world
            .set(self.dimension, pos.0, pos.1, pos.2, state);
        self.server.publish(Event::BlockChanged {
            by: self.profile.uuid,
            dimension: self.dimension,
            pos,
            state,
            broken,
        });
        // 물이 흘러들고, 모래가 떨어지고, 레드스톤이 다시 계산되도록
        crate::game::mechanics::neighbor_changed(self.server, self.dimension, pos);
        if broken.is_some_and(|b| {
            crate::game::mechanics::furnace::kind_of(blocks::name(blocks::block_of(b))).is_some()
        }) {
            self.server.furnaces.forget(self.dimension, pos);
        }
    }

    fn can_reach(&self, pos: BlockPos) -> bool {
        let eye = (self.position.0, self.position.1 + 1.62, self.position.2);
        let d = (
            pos.0 as f64 + 0.5 - eye.0,
            pos.1 as f64 + 0.5 - eye.1,
            pos.2 as f64 + 0.5 - eye.2,
        );
        d.0 * d.0 + d.1 * d.1 + d.2 * d.2 <= REACH * REACH
    }

    fn in_world(&self, pos: BlockPos) -> bool {
        let min = self.dimension.min_y();
        (min..min + self.dimension.height()).contains(&pos.1)
    }

    /// 크리에이티브 인벤토리에서 아이템을 옮김. 바닐라처럼 크리에이티브에서만 받습니다.
    pub(crate) fn set_creative_slot(&mut self, slot: i16, item: Option<Item>) {
        if self.gamemode != GameMode::Creative || !(1..46).contains(&slot) {
            return; // -1은 아이템을 던짐 — 아직 엔티티가 없어 무시
        }
        let item = item.filter(|i| i.count > 0 && i.count <= 99 && i.id > 0);
        let was = (self.main_hand(), self.off_hand());
        self.inventory[slot as usize] = item;
        if was != (self.main_hand(), self.off_hand()) {
            self.hands_changed();
        }
    }

    /// 크리에이티브 가운데 클릭: 그 블록을 손에 쥡니다.
    pub(crate) async fn pick_block(&mut self, pos: BlockPos) -> Result<(), FrameError> {
        if self.gamemode != GameMode::Creative || !self.can_reach(pos) {
            return Ok(());
        }
        let state = self.server.world.get(self.dimension, pos.0, pos.1, pos.2);
        let Some(item) = blocks::item_for_block(blocks::block_of(state)) else {
            return Ok(());
        };
        let item = Some(Item::new(item, 1));
        // 이미 핫바에 있으면 그 칸을 고르고, 없으면 지금 칸에 넣습니다.
        let existing = (0..9).find(|&i| self.inventory[36 + i].map(|x| x.id) == item.map(|x| x.id));
        match existing {
            Some(i) => self.held = i,
            None => {
                self.inventory[36 + self.held] = item;
                let body = pw::encode_player_inventory(self.held as i32, item);
                self.send(pw::SET_PLAYER_INVENTORY_ID, &body).await?;
            }
        }
        self.send(
            pw::SET_HELD_SLOT_ID,
            &pw::encode_held_slot(self.held as i32),
        )
        .await?;
        self.hands_changed();
        Ok(())
    }

    pub(crate) fn hands_changed(&mut self) {
        let (main, off, armor) = (self.main_hand(), self.off_hand(), self.shown_armor());
        self.server.update_player(self.profile.uuid, |s| {
            s.main_hand = main;
            s.off_hand = off;
            s.armor = armor;
        });
        self.server.publish(Event::HandsChanged {
            uuid: self.profile.uuid,
            entity_id: self.entity_id,
            main,
            off,
            armor,
        });
    }

    /// 남에게 보일 갑옷 (발·다리·몸·머리). 갑옷 숨기기면 비움.
    pub(crate) fn shown_armor(&self) -> [Option<Item>; 4] {
        if self.rank.hide_armor {
            return [None; 4];
        }
        [8, 7, 6, 5].map(|s| self.inventory[s])
    }
}
