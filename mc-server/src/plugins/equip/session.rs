//! 접속한 사람 쪽: 입은 장비의 능력치, 주문서 바르기(악세 위에 놓기), 광석 캐기, 보스, 명령어.
//!
//! ```text
//! /장비                    장비 창 (악세서리 칸 · 능력치 · 세트 효과)
//! /강화                    강화 창
//! /대장간                  제련 · 장비 · 악세서리 · 재료 · 상점
//! /장비 지급 <id> [개수]    (OP) 장비·재료 받기
//! /보스 소환 <id>           (OP) 보스를 앞에 소환 · /보스 목록
//! ```

use tokio::io::AsyncWrite;

use super::config::{self, EquipConfig, GearDef};
use super::store::{Gear, gears, is_gear_serial};
use super::{Part, Stats, gear_stats, set_bonuses};
use crate::data::items;
use crate::game::custom_items;
use crate::packets::world::Item;
use crate::protocol::frame::FrameError;
use crate::server::Event;
use crate::session::play::Session;
use crate::text;

/// 아이템 → 우리 것이면 `dangu:…`, 바닐라면 이름.
pub fn item_key(item: &Item) -> String {
    match item.custom.and_then(custom_items::get) {
        Some(c) => c.id,
        None => items::name(item.id).to_owned(),
    }
}

/// `dangu:…` 또는 바닐라 이름 → 아이템 하나 (일련번호 없음).
pub fn item_of(key: &str, count: i32) -> Option<Item> {
    if key.starts_with("dangu:") {
        let index = custom_items::index_of(key)?;
        let def = custom_items::get(index)?;
        Some(Item {
            id: def.base,
            count,
            custom: Some(index),
            serial: 0,
        })
    } else {
        Some(Item::new(items::by_name(key)?, count))
    }
}

/// 이 아이템이 장비면 (종류, 일련번호의 상태).
pub fn gear_of(item: &Item, cfg: &EquipConfig) -> Option<(GearDef, Option<Gear>)> {
    let def = cfg.gear_def(&item_key(item))?.clone();
    let gear = is_gear_serial(item.serial)
        .then(|| gears().read(|d| d.gears.get(&item.serial).cloned()))
        .flatten();
    Some((def, gear))
}

/// 곡괭이 단계 (나무·금 1, 돌 2, 철 3, 다이아 4, 네더라이트 5).
fn pickaxe_rank(name: &str) -> u8 {
    match name {
        "wooden" | "golden" => 1,
        "stone" => 2,
        "iron" => 3,
        "diamond" => 4,
        "netherite" => 5,
        _ => 0,
    }
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    /// 입은 장비·든 무기·낀 악세서리의 능력치 합과 켜진 세트 (세트, 개수), 든 장비 무기.
    pub(crate) fn equip_bonus(&self) -> (Stats, Vec<(String, u8)>, Option<Part>) {
        config::with(|cfg| {
            let mut total = Stats::default();
            let mut sets: Vec<String> = Vec::new();
            let mut weapon = None;
            let mut add = |item: Item, want: &dyn Fn(Part) -> bool| -> Option<Part> {
                let (def, gear) = gear_of(&item, cfg)?;
                if !want(def.part) {
                    return None;
                }
                total.add(&gear_stats(&def, gear.as_ref(), cfg));
                if let Some(s) = &def.set {
                    sets.push(s.clone());
                }
                Some(def.part)
            };
            for (slot, part) in
                [5, 6, 7, 8]
                    .into_iter()
                    .zip([Part::Helmet, Part::Chest, Part::Legs, Part::Boots])
            {
                if let Some(i) = self.inventory[slot] {
                    add(i, &|p| p == part);
                }
            }
            if let Some(i) = self.main_hand() {
                weapon = add(i, &|p| p.is_weapon());
            }
            let worn = gears().read(|d| {
                d.worn(self.profile.uuid)
                    .iter()
                    .flatten()
                    .filter_map(|s| d.gears.get(s).map(|g| (*s, g.clone())))
                    .collect::<Vec<_>>()
            });
            for (_, g) in worn {
                if let Some(def) = cfg.gear_def(&g.def) {
                    total.add(&gear_stats(def, Some(&g), cfg));
                }
            }
            let refs: Vec<&str> = sets.iter().map(String::as_str).collect();
            let (bonus, counts) = set_bonuses(&refs, cfg);
            total.add(&bonus);
            (total, counts, weapon)
        })
    }

    /// 장비면 바닐라 갑옷 점수를 세지 않도록 (장비는 RPG 방어력으로).
    pub(crate) fn is_gear(&self, item: &Item) -> bool {
        item.custom.is_some() && config::with(|cfg| gear_of(item, cfg).is_some())
    }

    // ---- 가방 ----

    /// 가방(9..45)에 그 아이템이 몇 개 (일련번호 있는 장비는 빼고).
    pub(crate) fn count_key(&self, key: &str) -> i32 {
        self.inventory[9..45]
            .iter()
            .flatten()
            .filter(|i| i.serial == 0 && item_key(i) == key)
            .map(|i| i.count)
            .sum()
    }

    /// 가방에서 그 아이템을 `n`개 꺼냄 (모자라면 아무것도 안 하고 false).
    pub(crate) async fn take_key(&mut self, key: &str, n: i32) -> std::io::Result<bool> {
        if self.count_key(key) < n {
            return Ok(false);
        }
        let mut left = n;
        for slot in (9..45).rev() {
            if left == 0 {
                break;
            }
            if let Some(it) = self.inventory[slot]
                && it.serial == 0
                && item_key(&it) == key
            {
                let k = it.count.min(left);
                left -= k;
                self.inventory[slot] = Some(it.with_count(it.count - k)).filter(|i| i.count > 0);
                self.sync_slot(slot).await?;
            }
        }
        Ok(true)
    }

    /// 아이템을 주고, 못 넣은 것은 발밑에.
    pub(crate) async fn give_or_drop(&mut self, item: Item) -> std::io::Result<()> {
        let given = self.give(item).await?;
        if given < item.count {
            self.throw(item.with_count(item.count - given));
        }
        Ok(())
    }

    /// 새 장비 하나 (일련번호를 붙여) 주기.
    pub(crate) async fn give_gear(&mut self, def: &GearDef) -> std::io::Result<Option<u32>> {
        let Some(mut item) = item_of(&def.id, 1) else {
            return Ok(None);
        };
        let serial = gears().write(|d| d.create(def));
        item.serial = serial;
        self.give_or_drop(item).await?;
        Ok(Some(serial))
    }

    /// `dangu:…`(장비면 하나씩 일련번호) 또는 바닐라 아이템 주기.
    pub(crate) async fn give_key(&mut self, key: &str, count: i32) -> std::io::Result<bool> {
        if let Some(def) = config::with(|c| c.gear_def(key).cloned()) {
            for _ in 0..count.clamp(1, 36) {
                self.give_gear(&def).await?;
            }
            return Ok(true);
        }
        let Some(item) = item_of(key, 1) else {
            return Ok(false);
        };
        let max = custom_items::max_stack(&item);
        let mut left = count.max(1);
        while left > 0 {
            let n = left.min(max);
            self.give_or_drop(item.with_count(n)).await?;
            left -= n;
        }
        Ok(true)
    }

    /// 대장장이 레벨.
    pub(crate) async fn smith_level(&self) -> u32 {
        let job = config::with(|c| c.smith_job.clone());
        crate::dambi::child::shared(self.profile.uuid)
            .await
            .and_then(|d| d.jobs.iter().find(|j| j.name == job).map(|j| j.level))
            .unwrap_or(0)
    }

    // ---- 주문서 바르기 ----

    /// 손에 든(커서) 주문서를 악세서리 위에 놓음. 처리했으면 true (클릭은 무시).
    pub(crate) async fn try_apply_scroll(&mut self, slot: usize) -> Result<bool, FrameError> {
        let Some(carried) = self.carried else {
            return Ok(false);
        };
        let key = item_key(&carried);
        let Some(scroll) = config::with(|c| c.scroll(&key).cloned()) else {
            return Ok(false);
        };
        let Some(target) = self.window_slot(slot) else {
            return Ok(false);
        };
        let Some((def, gear)) = config::with(|c| gear_of(&target, c)) else {
            return Ok(false);
        };
        if !def.part.is_accessory() {
            self.error("잠재능력은 반지·목걸이·귀걸이에만 바를 수 있습니다.")
                .await?;
            return Ok(true);
        }
        // 일련번호가 없던 것(지급 등)은 여기서 붙임
        let serial = match gear {
            Some(_) => target.serial,
            None => {
                let s = gears().write(|d| d.create(&def));
                self.set_window_slot(
                    slot,
                    Some(Item {
                        serial: s,
                        ..target
                    }),
                );
                s
            }
        };
        let rolled = config::with(|cfg| {
            let lines = super::store::roll_potential(cfg, &scroll, crate::game::rand_u32);
            gears().write(|d| {
                let g = d.gears.get_mut(&serial)?;
                if g.tries_left == 0 {
                    return Some(Err(()));
                }
                g.tries_left -= 1;
                g.potential = lines.clone();
                g.scroll = Some(scroll.name.clone());
                Some(Ok((lines, g.tries_left)))
            })
        });
        match rolled {
            Some(Ok((lines, left))) => {
                self.carried = Some(carried.with_count(carried.count - 1)).filter(|i| i.count > 0);
                let msg = format!(
                    "[{}] {}: {} (남은 횟수 {left})",
                    scroll.name,
                    def.name,
                    super::look::potential_text(&lines)
                );
                self.message(&text::colored(&msg, "light_purple")).await?;
                self.play_sound("block.enchantment_table.use");
            }
            Some(Err(())) => {
                self.error("이 장신구는 더 이상 잠재능력을 바를 수 없습니다. 새로 구하세요.")
                    .await?;
            }
            None => {}
        }
        self.send_window().await?;
        Ok(true)
    }

    /// 지금 창의 한 칸 (메뉴 번호 기준).
    fn window_slot(&self, slot: usize) -> Option<Item> {
        match &self.window {
            None => self.inventory.get(slot).copied().flatten(),
            Some(_) => {
                let top = self.window_size();
                (slot >= top)
                    .then(|| {
                        let i = slot - top;
                        let inv = if i < 27 { i + 9 } else { i - 27 + 36 };
                        self.inventory.get(inv).copied().flatten()
                    })
                    .flatten()
            }
        }
    }

    fn set_window_slot(&mut self, slot: usize, item: Option<Item>) {
        match &self.window {
            None => self.inventory[slot] = item,
            Some(_) => {
                let top = self.window_size();
                if slot >= top {
                    let i = slot - top;
                    let inv = if i < 27 { i + 9 } else { i - 27 + 36 };
                    self.inventory[inv] = item;
                }
            }
        }
    }

    pub(crate) fn play_sound(&self, name: &str) {
        if let Some(sound) = crate::data::sound_id(name) {
            self.server.publish(Event::Sound {
                dimension: self.dimension,
                pos: self.position,
                sound,
                source: 7,
                volume: 1.0,
                pitch: 1.0,
            });
        }
    }

    // ---- 광석 ----

    /// 우리 광석 블록을 캤을 때 (원석 · 광부 경험치). 광석이 아니면 false.
    pub(crate) async fn ore_mined(
        &mut self,
        custom: u16,
        pos: crate::packets::world::BlockPos,
    ) -> std::io::Result<bool> {
        let Some(id) = custom_items::get(custom).map(|c| c.id) else {
            return Ok(false);
        };
        let Some(ore) = config::with(|c| c.ore_by_block(&id).cloned()) else {
            return Ok(false);
        };
        let held = self
            .main_hand()
            .map(|i| items::name(i.id).to_owned())
            .unwrap_or_default();
        let rank = held.strip_suffix("_pickaxe").map_or(0, pickaxe_rank);
        if rank < pickaxe_rank(&ore.pickaxe) {
            let msg = format!(
                "{}은(는) {} 이상 곡괭이로 캐야 원석이 나옵니다.",
                ore.name,
                super::art::pickaxe_name(&ore.pickaxe)
            );
            self.message(&text::colored(&msg, "red")).await?;
            return Ok(true);
        }
        let count = 1 + i32::from(crate::game::rand_u32() % 100 < 30);
        if let Some(item) = item_of(&ore.drop, count) {
            self.server
                .entities
                .drop_at_block(self.server, self.dimension, pos, item);
        }
        crate::plugins::rank::job_xp(self.profile.uuid, "광부", ore.xp).await;
        self.quest_happened(crate::plugins::quest::Happened {
            kind: "ores",
            name: id.trim_start_matches("dangu:").to_owned(),
            amount: 1,
        })
        .await?;
        Ok(true)
    }

    // ---- 명령어 ----

    pub(crate) async fn equip_command(
        &mut self,
        name: &str,
        args: &[&str],
    ) -> Result<(), FrameError> {
        use super::gui::Page;
        match (name, args) {
            ("장비", ["지급", id, rest @ ..]) => {
                if !self
                    .server
                    .perms
                    .has(self.profile.uuid, "dangu.command.equip.give", 2)
                {
                    return self.error("권한이 없습니다.").await;
                }
                let id = if id.contains(':') {
                    (*id).to_owned()
                } else {
                    format!("dangu:{id}")
                };
                let n = rest.first().and_then(|s| s.parse().ok()).unwrap_or(1);
                if self.give_key(&id, n).await? {
                    self.message(&text::colored(
                        &format!("{id} ×{n}을(를) 받았습니다."),
                        "green",
                    ))
                    .await?;
                    Ok(())
                } else {
                    self.error(&format!("모르는 아이템: {id}")).await
                }
            }
            ("장비", _) => Ok(self.open_equip(Page::Gear).await?),
            ("강화", _) => Ok(self.open_equip(Page::Enhance { serial: None }).await?),
            ("대장간", [tab, ..]) => {
                let tab = super::gui::TABS.iter().position(|t| t == tab).unwrap_or(0);
                Ok(self.open_equip(Page::Smith { tab, page: 0 }).await?)
            }
            ("대장간", _) => Ok(self.open_equip(Page::Smith { tab: 0, page: 0 }).await?),
            ("보스", ["소환", id]) => self.summon_boss(id).await,
            ("보스", _) => {
                let list = config::with(|c| {
                    c.bosses
                        .iter()
                        .map(|b| {
                            format!(
                                "{} ({}) — 체력 {:.0}{}",
                                b.name,
                                b.id,
                                b.health * super::SCALE,
                                if b.raid { " · 레이드" } else { "" }
                            )
                        })
                        .collect::<Vec<_>>()
                });
                for l in list {
                    self.message(&text::colored(&l, "gold")).await?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn summon_boss(&mut self, id: &str) -> Result<(), FrameError> {
        if !self
            .server
            .perms
            .has(self.profile.uuid, "dangu.command.boss", 2)
        {
            return self.error("권한이 없습니다.").await;
        }
        let Some(mob) = config::with(|c| c.boss(id).map(boss_mob)).flatten() else {
            return self.error(&format!("모르는 보스: {id} (/보스 목록)")).await;
        };
        let (yaw, _) = (self.yaw.to_radians() as f64, 0.0);
        let pos = (
            self.position.0 - yaw.sin() * 4.0,
            self.position.1,
            self.position.2 + yaw.cos() * 4.0,
        );
        let name = mob.custom_name.clone().unwrap_or_default();
        crate::game::mobs::spawn_mob(self.server, mob, self.dimension, pos);
        self.server.publish(Event::System(text::colored(
            &format!("{name}이(가) 나타났습니다!"),
            "red",
        )));
        Ok(())
    }
}

/// 보스 몹 하나 (이름표·체력·공격 배율).
pub fn boss_mob(b: &config::BossDef) -> Option<crate::game::mobs::Mob> {
    let kind = crate::game::mobs::MobKind::by_name(&b.mob)?;
    let mut m = crate::game::mobs::Mob::new(kind);
    m.health = b.health;
    m.damage_mult = b.damage;
    m.boss = Some(b.id.clone());
    m.custom_name = Some(format!(
        "§{}§l[{}] §r§{}{}",
        if b.raid { '5' } else { 'c' },
        if b.raid { "레이드" } else { "보스" },
        if b.raid { 'd' } else { '6' },
        b.name
    ));
    Some(m)
}

/// 보스가 죽을 때 떨굴 것 ([`crate::game::mobs`]의 훅).
pub fn boss_loot(id: &str) -> Vec<Item> {
    config::with(|c| {
        let Some(b) = c.boss(id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for d in &b.drops {
            let roll = (crate::game::rand_u32() % 1_000_000) as f64 / 1_000_000.0;
            if roll >= d.chance {
                continue;
            }
            let n = d.min + (crate::game::rand_u32() % (d.max - d.min + 1).max(1) as u32) as i32;
            if let Some(def) = c.gear_def(&d.item) {
                // 장비는 하나씩 일련번호를 붙여
                for _ in 0..n.max(1) {
                    if let Some(mut it) = item_of(&def.id, 1) {
                        it.serial = gears().write(|g| g.create(def));
                        out.push(it);
                    }
                }
            } else if let Some(it) = item_of(&d.item, n.max(1)) {
                out.push(it);
            }
        }
        out
    })
}

/// 자연 스폰 몹이 보스로 바뀔지 ([`crate::game::mobs`]의 훅).
pub fn maybe_boss(kind: crate::game::mobs::MobKind) -> Option<crate::game::mobs::Mob> {
    config::with(|c| {
        c.bosses
            .iter()
            .filter(|b| b.spawn_chance > 0.0 && b.mob == kind.name())
            .find(|b| ((crate::game::rand_u32() % 1_000_000) as f64 / 1_000_000.0) < b.spawn_chance)
            .and_then(boss_mob)
    })
}

/// 서버를 켤 때: 설정·장비 파일, 겉모습·보스·광석 훅.
pub fn install() {
    custom_items::set_look_provider(super::look::look_of);
    crate::game::mobs::set_boss_hooks(boss_loot, maybe_boss);
}

/// 콘텐츠를 읽은 뒤: 광석 블록 상태를 월드 생성에 넘김.
pub fn install_ores() {
    let ores = config::with(|c| {
        c.ores
            .iter()
            .filter_map(|o| {
                let index = custom_items::index_of(&o.block)?;
                let state = crate::plugins::contents::block_state(index)?;
                Some(crate::world::features::ExtraOre {
                    state,
                    nether: o.nether,
                    min_y: o.min_y,
                    max_y: o.max_y,
                    tries: o.tries,
                    size: o.size,
                })
            })
            .collect::<Vec<_>>()
    });
    crate::world::features::set_extra_ores(ores);
}
