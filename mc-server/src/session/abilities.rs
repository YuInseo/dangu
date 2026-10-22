//! 사용자 정의 아이템의 능력: 번개, 순간 이동, 도약, 회복.

use std::time::{Duration, Instant};

use tokio::io::AsyncWrite;

use super::play::Session;
use crate::data::{items, particle, sound};
use crate::game::custom_items::{self, Ability};
use crate::game::entities::{Vec3, dist2};
use crate::game::mobs;
use crate::packets::effects as fx;
use crate::packets::entity as pe;
use crate::protocol::frame::FrameError;
use crate::server::Event;

const LIGHTNING_RANGE: f64 = 40.0;
const LIGHTNING_RADIUS: f64 = 3.5;
const LIGHTNING_DAMAGE: f32 = 6.0;
const BLINK_RANGE: f64 = 10.0;

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    fn eye(&self) -> Vec3 {
        (self.position.0, self.position.1 + 1.62, self.position.2)
    }

    fn look(&self) -> Vec3 {
        let (yaw, pitch) = (self.yaw.to_radians() as f64, self.pitch.to_radians() as f64);
        (
            -yaw.sin() * pitch.cos(),
            -pitch.sin(),
            yaw.cos() * pitch.cos(),
        )
    }

    /// 눈에서 바라보는 쪽으로 걸어가며 처음 부딪힌 블록 바로 앞 (없으면 최대 거리).
    fn raycast(&self, range: f64) -> (Vec3, bool) {
        let (eye, dir) = (self.eye(), self.look());
        let mut last = eye;
        let mut t = 0.0;
        while t < range {
            t += 0.2;
            let p = (eye.0 + dir.0 * t, eye.1 + dir.1 * t, eye.2 + dir.2 * t);
            if self.server.world.is_solid(
                self.dimension,
                p.0.floor() as i32,
                p.1.floor() as i32,
                p.2.floor() as i32,
            ) {
                return (last, true);
            }
            last = p;
        }
        (last, false)
    }

    /// 손에 든 사용자 정의 아이템의 능력을 씁니다. 썼으면 true.
    pub(super) async fn use_ability(&mut self, off_hand: bool) -> Result<bool, FrameError> {
        let item = if off_hand {
            self.off_hand()
        } else {
            self.main_hand()
        };
        let Some(def) = item.and_then(|i| i.custom).and_then(custom_items::get) else {
            return Ok(false);
        };
        let Some(ability) = def.ability else {
            return Ok(false);
        };
        if self.vitals.dead || self.gamemode == crate::packets::play::GameMode::Spectator {
            return Ok(false);
        }
        let now = Instant::now();
        if self.ability_ready.get(&def.id).is_some_and(|&t| now < t) {
            return Ok(true); // 재사용 대기 중 (아이템 기본 동작도 막음)
        }
        self.ability_ready
            .insert(def.id.clone(), now + Duration::from_secs_f32(def.cooldown));
        let ticks = (def.cooldown * 20.0) as i32;
        if ticks > 0 {
            let group = format!("minecraft:{}", items::name(def.base));
            self.send(fx::COOLDOWN_ID, &fx::encode_cooldown(&group, ticks))
                .await?;
        }

        let dim = self.dimension;
        match ability {
            Ability::Lightning => {
                let (target, _) = self.raycast(LIGHTNING_RANGE);
                let ground = (target.0, target.1.floor(), target.2);
                let id = self.server.allocate_entity_id();
                self.server.publish(Event::Lightning {
                    dimension: dim,
                    pos: ground,
                    entity_id: id,
                });
                self.server.publish(Event::Sound {
                    dimension: dim,
                    pos: ground,
                    sound: sound::THUNDER,
                    source: fx::SOURCE_WEATHER,
                    volume: 4.0,
                    pitch: 1.0,
                });
                // 주변의 몹과 (나 말고) 플레이어에게 피해
                let mobs_near: Vec<i32> = self
                    .server
                    .entities
                    .snapshots_in(dim)
                    .into_iter()
                    .filter(|e| {
                        e.item.is_none()
                            && dist2(e.pos, ground) < LIGHTNING_RADIUS * LIGHTNING_RADIUS
                    })
                    .map(|e| e.id)
                    .collect();
                for m in mobs_near {
                    mobs::damage(
                        self.server,
                        m,
                        LIGHTNING_DAMAGE,
                        Some(ground),
                        Some(self.entity_id),
                    );
                }
                for p in self.server.players() {
                    if p.uuid != self.profile.uuid
                        && p.dimension == dim
                        && dist2(p.position, ground) < LIGHTNING_RADIUS * LIGHTNING_RADIUS
                    {
                        self.server.publish(Event::Damage {
                            target: p.uuid,
                            amount: LIGHTNING_DAMAGE,
                            damage_type: "lightning_bolt",
                            attacker: Some((self.entity_id, ground)),
                            attacker_name: self.profile.name.clone(),
                        });
                    }
                }
                // 번개는 1초 뒤 사라짐
                let (server, events) = (self.server.events_handle(), id);
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let _ = server.send(Event::EntitiesRemoved {
                        dimension: dim,
                        ids: vec![events],
                    });
                });
            }
            Ability::Blink => {
                let (mut target, _) = self.raycast(BLINK_RANGE);
                // 발이 닿을 자리: 눈높이만큼 내림
                target.1 -= 1.62;
                self.particles(particle::PORTAL, self.position, 40);
                self.teleport_and_load(target).await?;
                self.particles(particle::PORTAL, target, 40);
                self.server.publish(Event::Sound {
                    dimension: dim,
                    pos: target,
                    sound: sound::TELEPORT,
                    source: fx::SOURCE_PLAYER,
                    volume: 1.0,
                    pitch: 1.0,
                });
            }
            Ability::Leap => {
                let d = self.look();
                let vel = (d.0 * 1.4, (d.1 * 1.4).max(0.0) + 0.7, d.2 * 1.4);
                self.send(
                    pe::SET_ENTITY_MOTION_ID,
                    &pe::encode_motion(self.entity_id, vel),
                )
                .await?;
                self.vitals.fall_immune = true;
                self.particles(particle::CLOUD, self.position, 20);
                self.server.publish(Event::Sound {
                    dimension: dim,
                    pos: self.position,
                    sound: sound::LAUNCH,
                    source: fx::SOURCE_PLAYER,
                    volume: 1.0,
                    pitch: 1.2,
                });
            }
            Ability::Heal => {
                self.vitals.health = (self.vitals.health + 8.0).min(super::survival::MAX_HEALTH);
                self.send_health().await?;
                let at = (self.position.0, self.position.1 + 1.0, self.position.2);
                self.particles(particle::HEART, at, 8);
                self.server.publish(Event::Sound {
                    dimension: dim,
                    pos: at,
                    sound: sound::LEVEL_UP,
                    source: fx::SOURCE_PLAYER,
                    volume: 0.8,
                    pitch: 1.5,
                });
            }
        }
        Ok(true)
    }

    fn particles(&self, particle: i32, pos: Vec3, count: i32) {
        self.server.publish(Event::Particles {
            dimension: self.dimension,
            pos: (pos.0, pos.1 + 0.5, pos.2),
            particle,
            spread: 0.5,
            count,
        });
    }

    /// 때릴 때의 피해: 사용자 정의 아이템이면 그 값.
    pub(super) fn held_damage(&self) -> f32 {
        let item = self.main_hand();
        item.and_then(|i| i.custom)
            .and_then(custom_items::get)
            .and_then(|c| c.damage)
            .unwrap_or_else(|| items::attack_damage(item))
    }
}
