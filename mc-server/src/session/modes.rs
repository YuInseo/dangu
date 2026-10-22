//! 게임 모드: F3+F4 전환, 비행, 관전(카메라·순간 이동), 손 바꾸기, 크리에이티브 던지기,
//! 서바이벌 아이템 고르기, 하드코어.

use tokio::io::AsyncWrite;

use super::play::Session;
use crate::admin::{self, Actor};
use crate::packets::hud;
use crate::packets::play::{self as p, GameMode};
use crate::packets::world::{self as pw, BlockPos, Item};
use crate::protocol::frame::FrameError;
use crate::server::Difficulty;
use crate::text;

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    fn may(&self, command: &str) -> bool {
        let actor = Actor {
            name: self.profile.name.clone(),
            uuid: Some(self.profile.uuid),
        };
        admin::spec(command).is_some_and(|s| actor.allowed(self.server, s))
    }

    /// F3+F4 메뉴. `/gamemode` 권한이 있어야 합니다 (바닐라도 OP 2단계).
    pub(crate) async fn request_game_mode(
        &mut self,
        mode: Option<GameMode>,
    ) -> Result<(), FrameError> {
        let Some(mode) = mode else { return Ok(()) };
        if !self.may("gamemode") {
            return Ok(self
                .message(&text::colored("게임 모드를 바꿀 권한이 없습니다.", "red"))
                .await?);
        }
        self.set_gamemode(mode).await?;
        let msg = format!("게임 모드: {}", mode.display_name());
        Ok(self.action_bar(&text::colored(&msg, "green")).await?)
    }

    /// 설정 화면의 난이도 버튼과 자물쇠.
    pub(crate) async fn request_difficulty(&mut self, id: i32) -> Result<(), FrameError> {
        if self.may("difficulty") && (0..4).contains(&id) {
            self.server.set_difficulty(Difficulty::from_id(id as u8));
        }
        // 거절됐으면 원래 값으로 되돌립니다.
        Ok(self.send_difficulty(self.server.difficulty()).await?)
    }

    pub(crate) async fn request_lock_difficulty(&mut self, locked: bool) -> Result<(), FrameError> {
        if locked && self.may("difficulty") {
            self.server.lock_difficulty();
        }
        Ok(self.send_difficulty(self.server.difficulty()).await?)
    }

    /// 클라이언트가 비행을 켜고 끔. 날 수 없는 모드면 능력을 다시 보내 끕니다.
    pub(crate) async fn player_abilities(&mut self, flags: u8) -> Result<(), FrameError> {
        let flying = flags & 0x02 != 0;
        let can_fly = matches!(self.gamemode, GameMode::Creative | GameMode::Spectator);
        if flying && !can_fly {
            self.send(p::PLAYER_ABILITIES_ID, &p::encode_abilities(self.gamemode))
                .await?;
            self.flying = false;
        } else {
            self.flying = flying;
        }
        if self.flying {
            self.reset_fall();
        }
        Ok(())
    }

    /// 관전 중 엔티티를 좌클릭: 그 눈으로 봅니다. 허공이면 자기 시점으로.
    pub(crate) async fn spectate(&mut self, target: Option<i32>) -> Result<(), FrameError> {
        if self.gamemode != GameMode::Spectator {
            return Ok(());
        }
        let target = target.filter(|id| self.sees_entity(*id));
        self.set_camera(target).await
    }

    pub(crate) async fn set_camera(&mut self, target: Option<i32>) -> Result<(), FrameError> {
        if self.camera == target {
            return Ok(());
        }
        self.camera = target;
        let id = target.unwrap_or(self.entity_id);
        Ok(self
            .send(hud::SET_CAMERA_ID, &hud::encode_camera(id))
            .await?)
    }

    /// 관전 메뉴(숫자 키)에서 다른 플레이어에게 순간 이동. 다른 차원이어도 갑니다.
    pub(crate) async fn spectator_teleport(&mut self, uuid: u128) -> Result<(), FrameError> {
        if self.gamemode != GameMode::Spectator {
            return Ok(());
        }
        let Some(target) = self.server.find_by_uuid(uuid) else {
            return Ok(());
        };
        if target.dimension != self.dimension {
            self.change_dimension(target.dimension).await?;
        }
        self.teleport_and_load(target.position).await
    }

    /// F 키: 주 손과 보조 손을 바꿉니다.
    pub(crate) async fn swap_hands(&mut self) -> Result<(), FrameError> {
        if self.gamemode == GameMode::Spectator || self.vitals.dead {
            return Ok(());
        }
        let main = 36 + self.held;
        self.inventory.swap(main, 45);
        self.sync_slot(main).await?;
        self.sync_slot(45).await?;
        self.hands_changed();
        Ok(())
    }

    /// 크리에이티브 인벤토리에서 밖으로 던진 아이템 (슬롯 -1).
    pub(crate) fn creative_drop(&mut self, item: Option<Item>) {
        if self.gamemode != GameMode::Creative {
            return;
        }
        if let Some(item) = item.filter(|i| i.count > 0 && i.id > 0) {
            self.throw(item);
        }
    }

    /// 가운데 클릭. 크리에이티브는 블록을 새로 쥐고, 서바이벌은 가진 것에서 찾아 핫바로.
    pub(crate) async fn pick_block_any_mode(&mut self, pos: BlockPos) -> Result<(), FrameError> {
        if self.gamemode == GameMode::Creative {
            return self.pick_block(pos).await;
        }
        if self.gamemode == GameMode::Spectator {
            return Ok(());
        }
        let state = self.server.world.get(self.dimension, pos.0, pos.1, pos.2);
        let Some(id) = crate::data::blocks::item_for_block(crate::data::blocks::block_of(state))
        else {
            return Ok(());
        };
        self.pick_from_inventory(id).await
    }

    /// 가진 아이템 `id`를 손에 쥡니다: 핫바에 있으면 그 칸, 가방에 있으면 지금 칸과 맞바꿈.
    pub(crate) async fn pick_from_inventory(&mut self, id: i32) -> Result<(), FrameError> {
        let has = |s: &Option<Item>| s.is_some_and(|i| i.id == id && i.custom.is_none());
        if let Some(i) = (0..9).find(|&i| has(&self.inventory[36 + i])) {
            self.held = i;
        } else if let Some(slot) = (9..36).find(|&s| has(&self.inventory[s])) {
            // 빈 핫바 칸이 있으면 그리로, 없으면 지금 칸과 맞바꿈
            if let Some(empty) = (0..9).find(|&i| self.inventory[36 + i].is_none()) {
                self.held = empty;
            }
            let hot = 36 + self.held;
            self.inventory.swap(slot, hot);
            self.sync_slot(slot).await?;
            self.sync_slot(hot).await?;
        } else {
            return Ok(());
        }
        self.send(
            pw::SET_HELD_SLOT_ID,
            &pw::encode_held_slot(self.held as i32),
        )
        .await?;
        self.hands_changed();
        Ok(())
    }

    /// 크리에이티브에서 엔티티를 가운데 클릭: 그 몹의 스폰 알을 쥡니다.
    pub(crate) async fn pick_entity(&mut self, entity_id: i32) -> Result<(), FrameError> {
        if self.gamemode != GameMode::Creative {
            return Ok(());
        }
        let Some(egg) = self
            .server
            .entities
            .get(entity_id)
            .and_then(|e| e.mob_kind())
            .and_then(|k| crate::data::items::by_name(&format!("{}_spawn_egg", k.name())))
        else {
            return Ok(());
        };
        let slot = 36 + self.held;
        if self.inventory[slot].is_some_and(|i| i.id == egg) {
            return Ok(());
        }
        let found = (0..9).find(|&i| self.inventory[36 + i].is_some_and(|x| x.id == egg));
        match found {
            Some(i) => self.held = i,
            None => {
                if let Some(empty) = (0..9).find(|&i| self.inventory[36 + i].is_none()) {
                    self.held = empty;
                }
                let slot = 36 + self.held;
                self.inventory[slot] = Some(Item::new(egg, 1));
                self.sync_slot(slot).await?;
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

    /// 하드코어에서 죽고 "관전하기"를 누르면 관전 모드로 남습니다.
    pub(crate) fn hardcore_after_death(&self) -> bool {
        self.server.config.hardcore
    }
}
