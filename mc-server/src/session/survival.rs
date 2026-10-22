//! 서바이벌: 체력·배고픔, 낙하·용암·공허·굶주림 피해, 때리기, 죽음과 부활, 먹기.

use std::time::{Duration, Instant};

use tokio::io::AsyncWrite;

use super::play::Session;
use crate::data::{block, registry_id};
use crate::game::entities::THROWN_PICKUP_DELAY;
use crate::game::mobs;
use crate::packets::entity as pe;
use crate::packets::play::{self as p, GameMode};
use crate::packets::world::{self as pw, Item};
use crate::protocol::frame::FrameError;
use crate::server::{Difficulty, Event};
use crate::text;
use crate::world::Dimension;

pub const MAX_HEALTH: f32 = 20.0;
const INVULNERABLE: Duration = Duration::from_millis(500);
/// 먹는 데 걸리는 시간 (바닐라 기본 1.6초).
const EAT_TIME: Duration = Duration::from_millis(1600);
/// 생존 틱(250ms) 몇 번마다 한 번: 환경 피해 0.5초, 회복·굶주림 4초.
const ENV_EVERY: u32 = 2;
const REGEN_EVERY: u32 = 16;
const REACH: f64 = 4.5;
const CREATIVE_REACH: f64 = 6.0;

#[derive(Debug, Clone)]
pub struct Vitals {
    pub health: f32,
    pub food: i32,
    pub saturation: f32,
    exhaustion: f32,
    pub dead: bool,
    /// 공중에 뜬 뒤 가장 높았던 y.
    fall_peak: Option<f64>,
    was_on_ground: bool,
    invulnerable_until: Instant,
    pub sprinting: bool,
    pub sneaking: bool,
    /// 다음 착지까지 낙하 피해 없음 (도약의 깃털).
    pub fall_immune: bool,
    /// (보조 손인가, 아이템, 시작)
    eating: Option<(bool, i32, Instant)>,
    ticks: u32,
}

impl Default for Vitals {
    fn default() -> Self {
        Self {
            health: MAX_HEALTH,
            food: 20,
            saturation: 5.0,
            exhaustion: 0.0,
            dead: false,
            fall_peak: None,
            was_on_ground: true,
            invulnerable_until: Instant::now(),
            sprinting: false,
            sneaking: false,
            fall_immune: false,
            eating: None,
            ticks: 0,
        }
    }
}

impl Vitals {
    pub fn with(health: f32, food: i32, saturation: f32) -> Self {
        Self {
            health: health.clamp(0.0, MAX_HEALTH),
            food: food.clamp(0, 20),
            saturation,
            ..Self::default()
        }
    }

    fn exhaust(&mut self, amount: f32) {
        self.exhaustion += amount;
        while self.exhaustion >= 4.0 {
            self.exhaustion -= 4.0;
            if self.saturation > 0.0 {
                self.saturation = (self.saturation - 1.0).max(0.0);
            } else {
                self.food = (self.food - 1).max(0);
            }
        }
    }
}

fn death_message(name: &str, cause: &str, attacker: &str) -> String {
    match cause {
        "fall" => format!("{name}님이 높은 곳에서 떨어졌습니다"),
        "lava" => format!("{name}님이 용암에 빠졌습니다"),
        "out_of_world" => format!("{name}님이 세계 밖으로 떨어졌습니다"),
        "starve" => format!("{name}님이 굶어 죽었습니다"),
        "player_attack" | "mob_attack" => format!("{name}님이 {attacker}에게 죽었습니다"),
        "arrow" => format!("{name}님이 {attacker}의 화살에 맞아 죽었습니다"),
        "fireball" => format!("{name}님이 {attacker}의 화염구에 맞아 죽었습니다"),
        "explosion" | "player_explosion" => format!("{name}님이 폭발로 죽었습니다"),
        _ => format!("{name}님이 죽었습니다"),
    }
}

/// 방어력으로 줄이는 공격 피해 종류.
const ATTACKS: [&str; 9] = [
    "mob_attack",
    "player_attack",
    "arrow",
    "mob_projectile",
    "explosion",
    "player_explosion",
    "fireball",
    "sting",
    "thorns",
];

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    pub(crate) async fn send_health(&mut self) -> std::io::Result<()> {
        let v = &self.vitals;
        let body = pe::encode_set_health(v.health, v.food, v.saturation);
        self.send(pe::SET_HEALTH_ID, &body).await
    }

    /// 이동 패킷마다: 낙하 거리와 배고픔.
    pub(crate) async fn after_move(
        &mut self,
        old: (f64, f64, f64),
        on_ground: bool,
    ) -> Result<(), FrameError> {
        if self.vitals.dead {
            return Ok(());
        }
        let new = self.position;
        if (old.0.floor() as i32 >> 4, old.2.floor() as i32 >> 4)
            != (new.0.floor() as i32 >> 4, new.2.floor() as i32 >> 4)
        {
            self.guild_moved().await?;
        }
        let horizontal = ((new.0 - old.0).powi(2) + (new.2 - old.2).powi(2)).sqrt();
        if horizontal < 10.0 && self.gamemode.takes_damage() {
            if self.vitals.sprinting {
                self.vitals.exhaust(0.1 * horizontal as f32);
            }
            // 땅에서 뛰어오름
            if self.vitals.was_on_ground && !on_ground && new.1 - old.1 > 0.2 {
                self.vitals
                    .exhaust(if self.vitals.sprinting { 0.2 } else { 0.05 });
            }
        }

        let feet = self.block_at(new, 0.0);
        if feet == block::WATER || !self.gamemode.takes_damage() {
            self.vitals.fall_peak = None;
        } else if !on_ground {
            let peak = self.vitals.fall_peak.get_or_insert(new.1);
            *peak = peak.max(new.1);
        } else if let Some(peak) = self.vitals.fall_peak.take() {
            let distance = peak - new.1;
            if std::mem::take(&mut self.vitals.fall_immune) {
                // 도약 뒤 첫 착지
            } else if distance > 3.0 {
                self.damage((distance - 3.0).floor() as f32, "fall", None, "")
                    .await?;
            }
        }
        self.vitals.was_on_ground = on_ground;
        self.rank_moved(old, on_ground).await?;
        // 압력판을 밟음
        let feet = (
            new.0.floor() as i32,
            new.1.floor() as i32,
            new.2.floor() as i32,
        );
        crate::game::mechanics::redstone::stepped_on(self.server, self.dimension, feet);
        Ok(())
    }

    fn block_at(&self, pos: (f64, f64, f64), dy: f64) -> u32 {
        let (x, y, z) = (
            pos.0.floor() as i32,
            (pos.1 + dy).floor() as i32,
            pos.2.floor() as i32,
        );
        self.server.world.get(self.dimension, x, y, z)
    }

    /// 텔레포트·부활 뒤에는 낙하 거리를 셈하지 않습니다.
    pub(crate) fn reset_fall(&mut self) {
        self.vitals.fall_peak = None;
    }

    /// 250ms마다: 환경 피해, 회복, 굶주림, 먹기 완료.
    pub(crate) async fn survival_tick(&mut self) -> Result<(), FrameError> {
        if self.vitals.dead {
            return Ok(());
        }
        self.vitals.ticks += 1;
        let t = self.vitals.ticks;

        if let Some((off_hand, item, started)) = self.vitals.eating
            && started.elapsed() >= EAT_TIME
        {
            self.vitals.eating = None;
            self.finish_eating(off_hand, item).await?;
        }

        // 차원문 안에 서 있으면 (크리에이티브는 바로, 아니면 4초 뒤) 건너감
        let feet = self.block_at(self.position, 0.0);
        if crate::world::blockset::name_of(feet) == "nether_portal" {
            self.portal_ticks += 5;
            if self.portal_ticks >= 80 || self.gamemode == GameMode::Creative {
                self.portal_ticks = 0;
                let (to, at) = crate::game::mechanics::portal::destination(
                    self.server,
                    self.dimension,
                    self.position,
                );
                self.change_dimension_to(to, Some(at)).await?;
                return Ok(());
            }
        } else {
            self.portal_ticks = 0;
        }

        let void_y = (self.dimension.min_y() - 64) as f64;
        if self.position.1 < void_y && !self.gamemode.takes_damage() {
            // 크리에이티브는 공허에 떨어지면 스폰으로
            let spawn = crate::world::terrain::spawn_point(self.dimension, self.server.seed);
            return self.teleport_and_load(spawn).await;
        }
        if !self.gamemode.takes_damage() {
            return Ok(());
        }
        if t.is_multiple_of(ENV_EVERY) {
            if self.position.1 < void_y {
                self.damage(4.0, "out_of_world", None, "").await?;
            } else if self.block_at(self.position, 0.0) == block::LAVA
                || self.block_at(self.position, 1.0) == block::LAVA
            {
                self.damage(4.0, "lava", None, "").await?;
            }
        }
        if t.is_multiple_of(REGEN_EVERY) {
            let v = &mut self.vitals;
            let mut changed = false;
            let difficulty = self.server.difficulty();
            if difficulty == Difficulty::Peaceful {
                // 평화로움: 배고픔 없이 저절로 회복
                changed = v.health < MAX_HEALTH || v.food < 20;
                v.health = (v.health + 1.0).min(MAX_HEALTH);
                v.food = (v.food + 1).min(20);
            } else if v.food >= 18 && v.health < MAX_HEALTH {
                v.health = (v.health + 1.0).min(MAX_HEALTH);
                v.exhaust(6.0);
                changed = true;
            } else if v.food == 0 && v.health > difficulty.starvation_floor() {
                self.damage(1.0, "starve", None, "").await?;
            }
            if changed {
                self.send_health().await?;
            }
        }
        Ok(())
    }

    /// 피해를 입습니다. 0.5초 안에 다시 맞은 것은 무시합니다. `amount`는 바닐라 단위.
    pub(crate) async fn damage(
        &mut self,
        amount: f32,
        cause: &'static str,
        attacker: Option<(i32, (f64, f64, f64))>,
        attacker_name: &str,
    ) -> Result<(), FrameError> {
        if self.vitals.dead || !self.gamemode.takes_damage() || amount <= 0.0 {
            return Ok(());
        }
        // 길드 신호기: 화염 저항·수중 호흡은 막고, 저항은 줄임 (공허·명령어는 그대로)
        let b = self.guild.bonus;
        if (b.fire_immune && matches!(cause, "lava" | "in_fire" | "on_fire"))
            || (b.water_breathing && cause == "drown")
        {
            return Ok(());
        }
        let amount = if matches!(cause, "out_of_world" | "generic") {
            amount
        } else {
            amount * (1.0 - b.resist)
        };
        // RPG: 맞는 공격은 방어력으로 줄이고, 바닐라 체력 20을 RPG 체력에 나눠 씀
        let amount = {
            let st = self.rpg_stats(self.hud.job_levels());
            let rpg = amount * crate::plugins::equip::SCALE;
            let rpg = if ATTACKS.contains(&cause) {
                crate::plugins::equip::after_defense(rpg as f64, st.defense as f64) as f32
            } else {
                rpg
            };
            rpg * MAX_HEALTH / st.max_health.max(1.0)
        };
        let now = Instant::now();
        if now < self.vitals.invulnerable_until {
            return Ok(());
        }
        self.vitals.invulnerable_until = now + INVULNERABLE;
        self.vitals.health = (self.vitals.health - amount).max(0.0);
        self.vitals.exhaust(0.1);
        self.server.publish(Event::Hurt {
            dimension: self.dimension,
            entity_id: self.entity_id,
            damage_type: registry_id("damage_type", cause),
            attacker: attacker.map(|a| a.0),
        });
        // 밀려나기: 때린 쪽 반대로
        if let Some((_, from)) = attacker {
            let (dx, dz) = (self.position.0 - from.0, self.position.2 - from.2);
            let len = (dx * dx + dz * dz).sqrt().max(0.01);
            let body = pe::encode_motion(self.entity_id, (dx / len * 0.4, 0.36, dz / len * 0.4));
            self.send(pe::SET_ENTITY_MOTION_ID, &body).await?;
        }
        self.send_health().await?;
        if self.vitals.health <= 0.0 {
            let msg = death_message(&self.profile.name, cause, attacker_name);
            self.die(msg).await?;
        }
        Ok(())
    }

    async fn die(&mut self, message: String) -> Result<(), FrameError> {
        self.vitals.dead = true;
        self.vitals.eating = None;
        self.server
            .update_player(self.profile.uuid, |s| s.dead = true);
        self.close_window(false).await?;

        // 가진 것을 모두 떨어뜨립니다 (인벤세이브 등급이면 그대로).
        let at = self.position;
        let keep = self.rank_keeps_inventory();
        for slot in (0..self.inventory.len()).filter(|_| !keep) {
            if let Some(item) = self.inventory[slot].take() {
                let vel = (
                    crate::game::rand_f64() * 0.4 - 0.2,
                    0.2,
                    crate::game::rand_f64() * 0.4 - 0.2,
                );
                self.server.entities.spawn_item(
                    self.server,
                    self.dimension,
                    (at.0, at.1 + 1.0, at.2),
                    vel,
                    item,
                    THROWN_PICKUP_DELAY,
                );
            }
        }
        if let Some(item) = self.carried.take() {
            self.server.entities.spawn_item(
                self.server,
                self.dimension,
                (at.0, at.1 + 1.0, at.2),
                (0.0, 0.2, 0.0),
                item,
                THROWN_PICKUP_DELAY,
            );
        }
        self.send_inventory().await?;
        self.hands_changed();

        self.server.publish(Event::EntityAnimation {
            dimension: self.dimension,
            entity_id: self.entity_id,
            event: pe::ENTITY_EVENT_DEATH,
        });
        self.server
            .publish(Event::System(text::colored(&message, "gray")));
        let body = pe::encode_combat_kill(self.entity_id, &text::plain(&message));
        self.send(pe::PLAYER_COMBAT_KILL_ID, &body).await?;
        Ok(())
    }

    /// 사망 화면에서 "부활"을 눌렀을 때. 오버월드 스폰으로 돌아갑니다.
    pub(crate) async fn respawn(&mut self) -> Result<(), FrameError> {
        if !self.vitals.dead {
            return Ok(());
        }
        self.vitals = Vitals::default();
        self.mana.current = self.mana.max;
        let from = self.dimension;
        self.dimension = Dimension::Overworld;
        self.server.update_player(self.profile.uuid, |s| {
            s.dead = false;
            s.dimension = Dimension::Overworld;
        });
        self.send(p::RESPAWN_ID, &p::encode_respawn(&self.spawn_info()))
            .await?;
        self.enter_world(None).await?;
        self.send_health().await?;
        self.server.publish(Event::Respawned {
            from,
            state: self.state(),
        });
        if self.hardcore_after_death() {
            self.set_gamemode(GameMode::Spectator).await?;
        }
        Ok(())
    }

    /// 왼클릭으로 엔티티를 때림 (플레이어든 몹이든).
    pub(crate) async fn attack(&mut self, target: i32) -> Result<(), FrameError> {
        if self.vitals.dead || self.gamemode == GameMode::Spectator || target == self.entity_id {
            return Ok(());
        }
        let reach = if self.gamemode == GameMode::Creative {
            CREATIVE_REACH
        } else {
            REACH
        } + 1.0;
        // RPG 공격력 (+치명타) → 바닐라 단위 피해
        let st = self.rpg_stats(self.hud.job_levels());
        let crit = (crate::game::rand_u32() % 10_000) as f32 / 100.0 < st.crit_chance;
        let rpg = st.attack * if crit { st.crit_damage / 100.0 } else { 1.0 };
        let amount = rpg / crate::plugins::equip::SCALE;
        if crit {
            self.play_sound("entity.player.attack.crit");
        }
        let me = (self.entity_id, self.position);
        if let Some(victim) = self
            .server
            .players()
            .into_iter()
            .find(|p| p.entity_id == target)
        {
            if victim.dimension == self.dimension
                && crate::game::entities::dist2(victim.position, self.position) <= reach * reach
            {
                self.server.publish(Event::Damage {
                    target: victim.uuid,
                    amount,
                    damage_type: "player_attack",
                    attacker: Some(me),
                    attacker_name: self.profile.name.clone(),
                });
                self.vitals.exhaust(0.1);
            }
        } else if let Some(mob) = self.server.entities.get(target)
            && mob.dimension == self.dimension
            && crate::game::entities::dist2(mob.pos, self.position) <= reach * reach
        {
            let (mob_name, boss) = match &mob.kind {
                crate::game::entities::Kind::Mob(m) => {
                    (m.kind.name(), m.boss.as_ref().and(m.custom_name.clone()))
                }
                _ => ("", None),
            };
            let killed = mobs::damage(
                self.server,
                target,
                amount,
                Some(self.position),
                Some(self.entity_id),
            );
            self.vitals.exhaust(0.1);
            if killed {
                if let Some(name) = boss {
                    self.server.publish(Event::System(crate::text::colored(
                        &format!("{}님이 {name}§r§6을(를) 쓰러뜨렸습니다!", self.profile.name),
                        "gold",
                    )));
                }
                self.quest_happened(crate::plugins::quest::Happened {
                    kind: "kill",
                    name: mob_name.to_owned(),
                    amount: 1,
                })
                .await?;
            }
        }
        Ok(())
    }

    /// 오른클릭(허공): 음식이면 먹기 시작.
    pub(crate) async fn use_item(
        &mut self,
        off_hand: bool,
        sequence: i32,
    ) -> Result<(), FrameError> {
        self.send(
            pw::BLOCK_CHANGED_ACK_ID,
            &pw::encode_block_changed_ack(sequence),
        )
        .await?;
        if self.use_ability(off_hand).await? || self.try_booster(off_hand).await? {
            return Ok(());
        }
        let item = if off_hand {
            self.off_hand()
        } else {
            self.main_hand()
        };
        if let Some(item) = item
            && crate::game::custom_items::food_of(&item).is_some()
            && (self.vitals.food < 20 || self.gamemode == GameMode::Creative)
        {
            self.vitals.eating = Some((off_hand, item.id, Instant::now()));
        }
        Ok(())
    }

    pub(crate) fn stop_using_item(&mut self) {
        self.vitals.eating = None;
    }

    async fn finish_eating(&mut self, off_hand: bool, item_id: i32) -> Result<(), FrameError> {
        let slot = if off_hand { 45 } else { 36 + self.held };
        let Some(item) = self.inventory[slot].filter(|i| i.id == item_id) else {
            return Ok(());
        };
        let Some((nutrition, saturation)) = crate::game::custom_items::food_of(&item) else {
            return Ok(());
        };
        let v = &mut self.vitals;
        v.food = (v.food + nutrition).min(20);
        v.saturation = (v.saturation + saturation).min(v.food as f32);
        if self.gamemode != GameMode::Creative {
            self.inventory[slot] = Some(Item {
                count: item.count - 1,
                ..item
            })
            .filter(|i| i.count > 0);
            self.sync_slot(slot).await?;
            self.hands_changed();
        }
        let body = pe::encode_entity_event(self.entity_id, pe::ENTITY_EVENT_USE_ITEM_FINISHED);
        self.send(pe::ENTITY_EVENT_ID, &body).await?;
        self.send_health().await?;
        Ok(())
    }

    pub(crate) fn exhaust(&mut self, amount: f32) {
        if self.gamemode.takes_damage() {
            self.vitals.exhaust(amount);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhaustion_eats_saturation_then_food() {
        let mut v = Vitals::with(20.0, 20, 1.0);
        v.exhaust(4.0);
        assert_eq!((v.food, v.saturation), (20, 0.0));
        v.exhaust(8.0);
        assert_eq!(v.food, 18);
    }

    #[test]
    fn death_messages() {
        assert_eq!(
            death_message("A", "player_attack", "B"),
            "A님이 B에게 죽었습니다"
        );
        assert_eq!(
            death_message("A", "fall", ""),
            "A님이 높은 곳에서 떨어졌습니다"
        );
    }
}
