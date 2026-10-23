//! 등급 특혜를 접속에 붙이는 곳: 명령어(`/등급` `/ec` `/제작대` `/수리` `/셋홈` `/홈` `/갑옷숨기기`
//! `/플라이`), 날기·더블점프, 잠수 보상, 자동심기, 인벤세이브, 엔더상자 창.

use std::time::{Duration, Instant};

use tokio::io::AsyncWrite;

use super::{ENDER_SLOTS, Home, Perks, RankDef, active_rank, config, members, no_perks, now};
use crate::data::{blocks, menu};
use crate::packets::container as pc;
use crate::packets::entity as pe;
use crate::packets::play::{self as p, GameMode};
use crate::packets::world::{BlockPos, Item};
use crate::protocol::frame::FrameError;
use crate::session::play::Session;
use crate::session::windows::{Window, WindowKind};
use crate::text;
use crate::world::Dimension;

/// 능력 패킷의 "날 수 있음"·"날고 있음" 비트.
const ALLOW_FLY: u8 = 0x04;
const FLYING: u8 = 0x02;
/// 더블점프로 튀어 오르는 세기 (블록/틱).
const JUMP_UP: f64 = 0.75;
const JUMP_FORWARD: f64 = 0.55;
/// `/등급` 창 칸.
const STATUS: usize = 4;
const CARDS: [usize; 3] = [11, 13, 15];
const SHOP: usize = 22;
const CARD_ICONS: [&str; 3] = ["emerald", "gold_ingot", "diamond"];

#[derive(Debug, Default)]
pub struct RankState {
    /// 지금 등급 (1초마다 다시 읽음).
    pub rank: Option<RankDef>,
    pub perks: Perks,
    /// 남에게 갑옷을 숨김.
    pub hide_armor: bool,
    /// 지금 날아도 되나 (플라이 시간이 남았거나 우리 마을 안).
    may_fly: bool,
    /// 마지막으로 보낸 "날 수 있음" (None이면 다음 틱에 다시 보냄).
    allow_sent: Option<bool>,
    /// 이번에 공중에 떠서 더블점프를 썼음 (땅에 닿으면 다시).
    jumped: bool,
    /// 마지막으로 움직인 때 (잠수 판정).
    last_active: Option<Instant>,
    /// 잠수로 쌓인 초 (60초마다 포인트).
    afk_secs: u32,
    /// 아직 저장하지 않은 자동심기 사용 수와, 읽어 둔 남은 수.
    autoplant_used: u64,
    autoplant_left: u64,
    /// 다음에 `/홈`을 쓸 수 있는 때.
    tp_ready: Option<Instant>,
    /// 플라이가 1분 남았다고 알렸음.
    fly_warned: bool,
}

impl RankState {
    /// 게임 모드·차원이 바뀌어 클라이언트 능력이 처음으로 돌아감: 다음 틱에 다시 보냄.
    pub fn abilities_reset(&mut self) {
        self.allow_sent = None;
    }
}

use super::duration_text;

/// 등급 카드 설명 (`k`번째 등급, 지금 내 등급인가).
pub fn card_lore(cfg: &super::RankConfig, k: usize, on: bool) -> Vec<String> {
    let r = &cfg.ranks[k];
    let mut lore = vec![
        format!("가격  {}원 / 월", crate::plugins::hud::number(r.price)),
        "---".into(),
        "특혜".into(),
    ];
    if k > 0 {
        lore.push(format!("(+ {}의 모든 특혜)", cfg.ranks[k - 1].name));
    }
    lore.extend(r.perks.lines().into_iter().map(|l| format!("  {l}")));
    lore.push("---".into());
    lore.push(if on {
        "§a✔ 지금 내 등급".into()
    } else {
        "▶ 클릭: 후원 안내".into()
    });
    lore
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    // ---- 1초마다 ----

    pub(crate) async fn rank_tick(&mut self) -> Result<(), FrameError> {
        let me = self.profile.uuid;
        let t = now();
        // 자동심기 사용분 저장
        let used = std::mem::take(&mut self.rank.autoplant_used);
        let m = if used > 0 {
            members().write(|d| {
                let m = d.member_mut(me);
                m.autoplant = m.autoplant.saturating_sub(used);
                m.clone()
            })
        } else {
            members().read(|d| d.member(me))
        };
        // 이름으로 찾을 수 있게 (처음이거나 이름이 바뀌었을 때만 씀)
        if m.name != self.profile.name {
            let name = self.profile.name.clone();
            members().write(|d| d.member_mut(me).name = name);
        }
        self.rank.autoplant_left = m.autoplant;
        let rank = config(|c| active_rank(&m, c, t));
        let perks = rank.as_ref().map_or_else(no_perks, |r| r.perks.clone());
        let hide = m.hide_armor && perks.hide_armor;
        let hide_changed = hide != self.rank.hide_armor;
        self.rank.rank = rank;
        self.rank.perks = perks;
        self.rank.hide_armor = hide;
        if hide_changed {
            self.hands_changed();
        }

        // 날기: 플라이 시간 또는 우리 마을(길드 땅) 안
        let fly_left = m.fly_until - t;
        let in_town = self.rank.perks.guild_fly && self.in_own_land();
        let may_fly = fly_left > 0 || in_town;
        if self.rank.may_fly && !may_fly && self.flying {
            self.vitals.fall_immune = true;
            self.message(&text::colored(
                "플라이가 끝났습니다. (다음 착지까지 낙하 피해 없음)",
                "yellow",
            ))
            .await?;
        }
        if (1..=60).contains(&fly_left) && !in_town && !self.rank.fly_warned {
            self.rank.fly_warned = true;
            self.message(&text::colored("플라이가 1분 남았습니다.", "yellow"))
                .await?;
        } else if fly_left > 60 {
            self.rank.fly_warned = false;
        }
        self.rank.may_fly = may_fly;
        self.sync_flight().await?;

        // 잠수 보상
        self.afk_tick().await?;
        Ok(())
    }

    /// 우리 길드 땅 위에 서 있나.
    fn in_own_land(&self) -> bool {
        let chunk = crate::plugins::guild::land::chunk_of(self.position.0, self.position.2);
        crate::plugins::guild::guilds().read(|d| {
            d.owner_of(&self.server.name, self.dimension, chunk)
                .is_some_and(|g| g.member(self.profile.uuid).is_some())
        })
    }

    /// 클라이언트의 "날 수 있음"을 맞춥니다 (서바이벌·모험에서만; 크리에이티브는 원래 남).
    async fn sync_flight(&mut self) -> std::io::Result<()> {
        if !matches!(self.gamemode, GameMode::Survival | GameMode::Adventure) {
            self.rank.allow_sent = None;
            return Ok(());
        }
        let allow = self.rank.may_fly || (self.rank.perks.double_jump && !self.rank.jumped);
        if !self.rank.may_fly {
            self.flying = false;
        }
        if self.rank.allow_sent == Some(allow) {
            return Ok(());
        }
        self.send_flight(allow).await
    }

    async fn send_flight(&mut self, allow: bool) -> std::io::Result<()> {
        let mut body = p::encode_abilities(self.gamemode);
        if allow {
            body[0] |= ALLOW_FLY;
        }
        if allow && self.flying && self.rank.may_fly {
            body[0] |= FLYING;
        }
        self.rank.allow_sent = Some(allow);
        self.send(p::PLAYER_ABILITIES_ID, &body).await
    }

    /// 서바이벌에서 클라이언트가 날기를 켜고 끔 (스페이스 두 번). 등급이 맡으면 true.
    /// 날 수 있으면 날고, 아니면 더블점프로 바꿔 한 번 튀어 오릅니다.
    pub(crate) async fn rank_fly_request(&mut self, flying: bool) -> Result<bool, FrameError> {
        if !matches!(self.gamemode, GameMode::Survival | GameMode::Adventure) {
            return Ok(false);
        }
        if self.rank.may_fly {
            self.flying = flying;
            if flying {
                self.reset_fall();
            }
            return Ok(true);
        }
        if !self.rank.perks.double_jump {
            return Ok(false);
        }
        if flying && !self.rank.jumped {
            self.rank.jumped = true;
            self.flying = false;
            // 날기를 끄고(= 땅에 닿을 때까지 다시 못 켬) 바라보는 쪽으로 튀어 오름
            self.send_flight(false).await?;
            let yaw = (self.yaw as f64).to_radians();
            let vel = (-yaw.sin() * JUMP_FORWARD, JUMP_UP, yaw.cos() * JUMP_FORWARD);
            self.send(
                pe::SET_ENTITY_MOTION_ID,
                &pe::encode_motion(self.entity_id, vel),
            )
            .await?;
            self.reset_fall();
            self.play_sound("entity.breeze.jump");
        } else {
            self.flying = false;
            self.send_flight(!self.rank.jumped).await?;
        }
        Ok(true)
    }

    /// 이동 패킷마다: 착지하면 더블점프를 다시, 움직였으면 잠수 아님.
    pub(crate) async fn rank_moved(
        &mut self,
        old: (f64, f64, f64),
        on_ground: bool,
    ) -> std::io::Result<()> {
        let new = self.position;
        if (new.0 - old.0).abs() + (new.1 - old.1).abs() + (new.2 - old.2).abs() > 0.05 {
            if self.rank.afk_secs >= config(|c| c.afk_after_secs) as u32 {
                self.action_bar(&text::colored("잠수에서 돌아왔습니다", "gray"))
                    .await?;
            }
            self.rank.last_active = Some(Instant::now());
            self.rank.afk_secs = 0;
        }
        if on_ground && self.rank.jumped {
            self.rank.jumped = false;
            self.sync_flight().await?;
        }
        Ok(())
    }

    async fn afk_tick(&mut self) -> std::io::Result<()> {
        let now = Instant::now();
        let since = *self.rank.last_active.get_or_insert(now);
        let (after, per_minute, here) = config(|c| {
            (
                c.afk_after_secs,
                c.afk_points_per_minute,
                c.afk_instances.is_empty() || c.afk_instances.contains(&self.server.name),
            )
        });
        if !here || self.vitals.dead || now.duration_since(since) < Duration::from_secs(after) {
            return Ok(());
        }
        if self.rank.afk_secs == after as u32 {
            let msg = format!(
                "잠수 중 — 1분마다 잠수 포인트 {}",
                (per_minute as f64 * self.rank.perks.afk_mult).round()
            );
            self.action_bar(&text::colored(&msg, "aqua")).await?;
        }
        self.rank.afk_secs += 1;
        // 잠수에 들어간 뒤 60초마다
        let counted = self.rank.afk_secs.saturating_sub(after as u32);
        if counted > 0 && counted.is_multiple_of(60) {
            let n = (per_minute as f64 * self.rank.perks.afk_mult).round() as i64;
            if n > 0 {
                crate::dambi::child::shared_add(self.profile.uuid, "afk", n).await;
            }
        }
        Ok(())
    }

    // ---- 캐기·죽기 ----

    /// 다 자란 작물을 캔 뒤: 자동심기가 남았으면 그 자리에 다시 심음.
    pub(crate) fn rank_autoplant(&mut self, pos: BlockPos, old: u32) {
        let left = self
            .rank
            .autoplant_left
            .saturating_sub(self.rank.autoplant_used);
        if left == 0 || !crate::plugins::guild::session::crop_is_mature(old) {
            return;
        }
        self.rank.autoplant_used += 1;
        self.set_block(pos, blocks::with_property(old, "age", "0"), None);
    }

    /// 죽을 때 가방을 지키나 (인벤세이브).
    pub(crate) fn rank_keeps_inventory(&self) -> bool {
        self.rank.perks.keep_inventory
    }

    // ---- 명령어 ----

    pub(crate) async fn rank_command(
        &mut self,
        name: &str,
        args: &[&str],
    ) -> Result<(), FrameError> {
        let perks = self.rank.perks.clone();
        let need = |ok: bool| -> Result<(), String> {
            if ok {
                Ok(())
            } else {
                Err("후원 등급 특혜입니다. (/등급)".to_owned())
            }
        };
        let r = match name {
            "등급" => return Ok(self.open_ranks().await?),
            "ec" => match need(perks.commands) {
                Ok(()) => return Ok(self.open_ender().await?),
                Err(e) => Err(e),
            },
            "제작대" => match need(perks.commands) {
                Ok(()) => return Ok(self.open_crafting().await?),
                Err(e) => Err(e),
            },
            "수리" => need(perks.commands)
                .map(|()| "이 서버의 장비는 닳지 않아 고칠 것이 없습니다.".to_owned()),
            "갑옷숨기기" => need(perks.hide_armor).map(|()| {
                let me = self.profile.uuid;
                let on = members().write(|d| {
                    let m = d.member_mut(me);
                    m.hide_armor = !m.hide_armor;
                    m.hide_armor
                });
                self.rank.hide_armor = on;
                self.hands_changed();
                if on {
                    "이제 다른 사람에게 내 갑옷이 보이지 않습니다.".to_owned()
                } else {
                    "다시 갑옷이 보입니다.".to_owned()
                }
            }),
            "플라이" => Ok(self.fly_status()),
            "셋홈" => self.set_home(args.first().copied().unwrap_or("집")),
            "홈삭제" => match args.first() {
                Some(h) => self.delete_home(h),
                None => Err("사용법: /홈삭제 <이름>".to_owned()),
            },
            "홈목록" => Ok(self.home_list()),
            "홈" => return self.go_home(args.first().copied()).await,
            _ => Err(format!("모르는 명령어: /{name}")),
        };
        match r {
            Ok(msg) => Ok(self.message(&text::colored(&msg, "green")).await?),
            Err(e) => self.error(&e).await,
        }
    }

    fn fly_status(&self) -> String {
        let left = members().read(|d| d.member(self.profile.uuid).fly_until) - now();
        let mut parts = Vec::new();
        if left > 0 {
            parts.push(format!("플라이 {} 남음", duration_text(left)));
        }
        if self.rank.perks.guild_fly {
            parts.push("마을(우리 길드 땅) 안에서는 언제나 날 수 있음".into());
        }
        if self.rank.perks.double_jump {
            parts.push("더블점프: 공중에서 스페이스 두 번".into());
        }
        if parts.is_empty() {
            "플라이 시간이 없습니다. (일일·주간 퀘스트 보상, 후원 등급)".into()
        } else {
            parts.join(" · ")
        }
    }

    fn max_homes(&self) -> usize {
        (config(|c| c.base_homes) + self.rank.perks.homes) as usize
    }

    fn set_home(&mut self, name: &str) -> Result<String, String> {
        let max = self.max_homes();
        let home = Home {
            name: name.chars().take(16).collect(),
            instance: self.server.name.clone(),
            dimension: self.dimension.name().to_owned(),
            pos: self.position,
            look: (self.yaw, self.pitch),
        };
        let me = self.profile.uuid;
        members().write(|d| {
            let homes = &mut d.member_mut(me).homes;
            if let Some(h) = homes.iter_mut().find(|h| h.name == home.name) {
                *h = home.clone();
                return Ok(format!("홈 '{}'을(를) 여기로 옮겼습니다.", home.name));
            }
            if homes.len() >= max {
                return Err(format!(
                    "홈은 {max}개까지입니다. (/홈삭제, 후원 등급이면 더)"
                ));
            }
            homes.push(home.clone());
            Ok(format!(
                "홈 '{}'을(를) 놓았습니다. ({}/{max})",
                home.name,
                homes.len()
            ))
        })
    }

    fn delete_home(&mut self, name: &str) -> Result<String, String> {
        let me = self.profile.uuid;
        members().write(|d| {
            let homes = &mut d.member_mut(me).homes;
            let before = homes.len();
            homes.retain(|h| h.name != name);
            if homes.len() == before {
                Err(format!("'{name}' 홈이 없습니다."))
            } else {
                Ok(format!("홈 '{name}'을(를) 지웠습니다."))
            }
        })
    }

    fn home_list(&self) -> String {
        let homes = members().read(|d| d.member(self.profile.uuid).homes);
        if homes.is_empty() {
            return "홈이 없습니다. (/셋홈 [이름])".into();
        }
        let list: Vec<String> = homes
            .iter()
            .map(|h| {
                format!(
                    "{} ({} {} {:.0},{:.0},{:.0})",
                    h.name, h.instance, h.dimension, h.pos.0, h.pos.1, h.pos.2
                )
            })
            .collect();
        format!(
            "홈 {}/{}: {}",
            homes.len(),
            self.max_homes(),
            list.join(" · ")
        )
    }

    /// 이동 대기 (등급마다 줄어듦).
    fn tp_cooldown(&self) -> Duration {
        let base = config(|c| c.tp_cooldown_secs) as f64;
        let cut = self.rank.perks.tp_cooldown_cut.clamp(0.0, 100.0);
        Duration::from_secs_f64(base * (1.0 - cut / 100.0))
    }

    async fn go_home(&mut self, name: Option<&str>) -> Result<(), FrameError> {
        let homes = members().read(|d| d.member(self.profile.uuid).homes);
        let home = match name {
            Some(n) => homes.iter().find(|h| h.name == n),
            None => homes.first(),
        };
        let Some(home) = home.cloned() else {
            let msg = if homes.is_empty() {
                "홈이 없습니다. (/셋홈 [이름])".to_owned()
            } else {
                self.home_list()
            };
            return self.error(&msg).await;
        };
        let now = Instant::now();
        if let Some(t) = self.rank.tp_ready
            && now < t
        {
            let left = (t - now).as_secs() + 1;
            return self
                .error(&format!("{left}초 뒤에 다시 이동할 수 있습니다."))
                .await;
        }
        if home.instance != self.server.name {
            return self
                .error(&format!(
                    "'{}' 홈은 {} 채널에 있습니다 (/이동 {})",
                    home.name, home.instance, home.instance
                ))
                .await;
        }
        let dim = Dimension::from_name(&home.dimension).unwrap_or(Dimension::Overworld);
        if dim != self.dimension {
            self.change_dimension_to(dim, Some(home.pos)).await?;
        } else {
            self.teleport_and_load(home.pos).await?;
        }
        self.rank.tp_ready = Some(now + self.tp_cooldown());
        let msg = format!("홈 '{}'(으)로 왔습니다.", home.name);
        Ok(self.message(&text::colored(&msg, "green")).await?)
    }

    // ---- 창 ----

    async fn open_menu_window(
        &mut self,
        kind: WindowKind,
        menu_id: i32,
        title: &str,
    ) -> std::io::Result<()> {
        self.close_window(false).await?;
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        self.window = Some(Window {
            id,
            kind,
            grid: [None; 9],
        });
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, menu_id, &text::plain(title)),
        )
        .await?;
        self.send_window().await
    }

    /// `/ec`: 개인 엔더상자 27칸 (모든 인스턴스에서 같음).
    pub(crate) async fn open_ender(&mut self) -> std::io::Result<()> {
        self.open_menu_window(WindowKind::Ender, menu::GENERIC_9X3, "엔더상자")
            .await
    }

    pub(crate) fn ender_slots(&self) -> Vec<Option<Item>> {
        let mut v = members().read(|d| d.member(self.profile.uuid).ender);
        v.resize(ENDER_SLOTS, None);
        v
    }

    /// 엔더상자 칸들을 저장합니다.
    pub(crate) fn commit_ender(&self, changes: &[(usize, Option<Item>)]) {
        let me = self.profile.uuid;
        members().write(|d| {
            let m = d.member_mut(me);
            for &(i, item) in changes {
                if let Some(s) = m.ender.get_mut(i) {
                    *s = item;
                }
            }
        });
    }

    /// `/등급`: 등급 카드 셋과 내 상태.
    pub(crate) async fn open_ranks(&mut self) -> std::io::Result<()> {
        self.open_menu_window(WindowKind::Rank, menu::GENERIC_9X3, "후원 등급")
            .await
    }

    pub(crate) fn rank_slots(&self) -> Vec<Option<Item>> {
        let mut slots = vec![None; 27];
        let cfg = super::get_config();
        let m = members().read(|d| d.member(self.profile.uuid));
        let mine = self.rank.rank.as_ref().map(|r| r.id.clone());
        let mut status = vec![match &self.rank.rank {
            Some(r) if m.until > 0 => {
                format!("등급 {} — {} 남음", r.name, duration_text(m.until - now()))
            }
            Some(r) => format!("등급 {} (영구)", r.name),
            None => "등급 없음".into(),
        }];
        status.push(format!(
            "홈 {}/{} · 자동심기 {}블록",
            m.homes.len(),
            self.max_homes(),
            crate::plugins::hud::number(m.autoplant as i64)
        ));
        status.push(self.fly_status());
        slots[STATUS] = self.icon(
            STATUS,
            "name_tag",
            &self.profile.name,
            "gold",
            status,
            false,
        );
        for (k, r) in cfg.ranks.iter().take(CARDS.len()).enumerate() {
            let slot = CARDS[k];
            let on = mine.as_deref() == Some(r.id.as_str());
            let lore = card_lore(&cfg, k, on);
            slots[slot] = self.icon(slot, CARD_ICONS[k], &r.name, &r.color, lore, on);
        }
        let shop = if cfg.shop_url.is_empty() {
            "후원은 관리자에게 문의하세요".to_owned()
        } else {
            cfg.shop_url.clone()
        };
        slots[SHOP] = self.icon(
            SHOP,
            "book",
            "후원 안내",
            "aqua",
            vec![shop, "누르면 채팅에 안내".into()],
            false,
        );
        slots
    }

    pub(crate) async fn rank_click(&mut self, slot: i16) -> Result<(), FrameError> {
        if slot as usize == SHOP || CARDS.contains(&(slot as usize)) {
            let url = config(|c| c.shop_url.clone());
            let msg = if url.is_empty() {
                "후원은 관리자에게 문의하세요.".to_owned()
            } else {
                format!("후원 안내: {url}")
            };
            self.message(&text::colored(&msg, "aqua")).await?;
        }
        Ok(())
    }
}
