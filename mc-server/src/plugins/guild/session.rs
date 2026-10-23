//! 접속한 사람 쪽: `/길드` 명령어, 창고 창, 땅 보호와 입장·퇴장 메시지, 퀘스트·직업 경험치 세기,
//! 신호기 효과 걸기, 길드원에게 알리기.
//!
//! ```text
//! /길드                         길드 창 (없으면 길드 목록)
//! /길드 만들기 <이름>           10,000원
//! /길드 가입 <이름> | 탈퇴 | 정보 [이름] | 목록
//! /길드 창고 | 스폰 | 스폰지정
//! /길드 땅 [사기|팔기]          지금 선 청크
//! /길드 설명|입장메시지|퇴장메시지 <글>
//! /길드 방문객 추가|제거 <이름>
//! /길드 강퇴|위임|부길드장 <이름>
//! /길드 해산                    (물어보는 창)
//! ```

use std::sync::Arc;

use tokio::io::AsyncWrite;

use super::beacon::{self, Bonus, Effect};
use super::gui::Page;
use super::{Actor, GuildEvent, GuildId, Land, Perm, guilds};
use crate::packets::effects as fx;
use crate::packets::world::{BlockPos, Item};
use crate::protocol::frame::FrameError;
use crate::server::{Event, Server};
use crate::session::play::Session;
use crate::session::windows::{Window, WindowKind};
use crate::text;

/// 담비 전역 버스에서 길드 알림이 오가는 이름.
pub const BUS: &str = "hoya:notice";

/// 세션마다 기억하는 길드 상태.
#[derive(Debug, Default)]
pub struct GuildState {
    /// 신호기 효과가 서버 계산에 주는 값.
    pub bonus: Bonus,
    /// 이 클라이언트에 건 효과 (효과, 단계).
    sent: Vec<(Effect, u8)>,
    speed_sent: f32,
    /// 지금 서 있는 땅의 길드.
    territory: Option<GuildId>,
    territory_name: String,
    ticks: u32,
    power_written: i64,
}

/// 길드원에게 알림 (다른 인스턴스·프로세스에 있어도).
pub fn notify(server: &Server, to: Vec<u128>, message: String) {
    if to.is_empty() {
        return;
    }
    if crate::dambi::child::active() {
        let list: Vec<String> = to.iter().map(|u| format!("{u:032x}")).collect();
        let payload = format!("{}\n{}", list.join(","), message);
        crate::dambi::child::publish(BUS, &payload);
    } else {
        server.publish_all(Event::Notice {
            to: Arc::new(to),
            message,
        });
    }
}

/// 담비 버스로 받은 알림.
pub fn receive(server: &Server, payload: &str) {
    let Some((list, message)) = payload.split_once('\n') else {
        return;
    };
    let to: Vec<u128> = list
        .split(',')
        .filter_map(|u| u128::from_str_radix(u, 16).ok())
        .collect();
    server.publish(Event::Notice {
        to: Arc::new(to),
        message: message.to_owned(),
    });
}

/// 채팅의 길드 채널이 쓰는: 이 사람의 길드 이름.
pub fn name_of(uuid: u128) -> Option<String> {
    guilds().read(|d| d.of(uuid).map(|g| g.name.clone()))
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    fn actor(&self) -> Actor {
        Actor {
            uuid: self.profile.uuid,
            name: self.profile.name.clone(),
        }
    }

    async fn guild_say(&mut self, r: super::Outcome) -> Result<(), FrameError> {
        match r {
            Ok(m) => self.message(&text::colored(&m, "green")).await?,
            Err(e) => self.error(&e).await?,
        }
        Ok(())
    }

    pub(crate) fn tell_guild(&self, message: &str) {
        let to = guilds().read(|d| {
            d.of(self.profile.uuid)
                .map(|g| g.members.iter().map(|m| m.uuid).collect())
                .unwrap_or_default()
        });
        notify(self.server, to, message.to_owned());
    }

    pub(crate) fn tell_player(&self, uuid: u128, message: &str) {
        notify(self.server, vec![uuid], message.to_owned());
    }

    /// 가입 신청이 들어왔음을 받을 수 있는 사람들에게.
    pub(crate) fn notify_request(&self, guild: &str, ok: bool) {
        if !ok {
            return;
        }
        let to = guilds().read(|d| {
            d.by_name(guild)
                .map(|g| {
                    g.members
                        .iter()
                        .filter(|m| g.can(m.uuid, Perm::AcceptRequests))
                        .map(|m| m.uuid)
                        .collect()
                })
                .unwrap_or_default()
        });
        notify(
            self.server,
            to,
            format!(
                "[길드] {}님이 가입을 신청했습니다 (/길드 → 가입 신청)",
                self.profile.name
            ),
        );
    }

    pub(crate) async fn guild_command(&mut self, args: &[&str]) -> Result<(), FrameError> {
        let me = self.actor();
        let now = super::now();
        let rest = |from: usize| args.get(from..).unwrap_or_default().join(" ");
        match args {
            [] => Ok(self.open_guild(Page::Main).await?),
            ["만들기" | "create", name] => {
                if guilds().read(|d| d.of(me.uuid).is_some()) {
                    return self.error("이미 길드에 들어가 있습니다.").await;
                }
                if guilds().read(|d| d.by_name(name).is_some()) || !super::valid_name(name) {
                    let r = guilds().read(|d| d.clone()).create(&me, name, now);
                    return self.guild_say(r).await;
                }
                if !self.pay(super::config::with(|c| c.create_cost)).await {
                    return Ok(());
                }
                let r = guilds().write(|d| d.create(&me, name, now));
                if r.is_err() {
                    crate::dambi::child::shared_add(me.uuid, "money", super::config::with(|c| c.create_cost)).await;
                } else {
                    self.server.publish_all(Event::System(text::colored(
                        &format!("[길드] {}님이 {name} 길드를 세웠습니다!", me.name),
                        "gold",
                    )));
                }
                self.guild_say(r).await
            }
            ["가입" | "join", name] => {
                let r = guilds().write(|d| d.request_join(&me, name, now));
                self.notify_request(name, r.is_ok());
                self.guild_say(r).await
            }
            ["탈퇴" | "leave"] => Ok(self.open_guild(Page::Leave).await?),
            ["해산" | "disband"] => Ok(self.open_guild(Page::Disband).await?),
            ["목록" | "list"] => {
                let list: Vec<String> = guilds().read(|d| {
                    d.guilds
                        .iter()
                        .filter(|g| !g.private)
                        .map(|g| {
                            format!(
                                "{} Lv.{} · {}명 · 전투력 {}",
                                g.name,
                                g.level(),
                                g.members.len(),
                                g.power()
                            )
                        })
                        .collect()
                });
                if list.is_empty() {
                    return self.error("공개 길드가 없습니다.").await;
                }
                for l in list {
                    self.message(&text::colored(&l, "yellow")).await?;
                }
                Ok(())
            }
            ["정보" | "info", rest @ ..] => {
                let lines = guilds().read(|d| {
                    let g = match rest.first() {
                        Some(n) => d.by_name(n).filter(|g| !g.private || g.member(me.uuid).is_some()),
                        None => d.of(me.uuid),
                    }?;
                    let (cur, need) = super::level_progress(g.exp);
                    Some(vec![
                        format!("== {} 길드 ==", g.name),
                        format!("레벨 {} ({cur}/{need}) · GP {}", g.level(), g.gp),
                        format!("길드장 {} · 인원 {}/{}", g.leader().name, g.members.len(), g.max_members()),
                        format!("전투력 {} · 땅 {}/{}청크", g.power(), g.lands.len(), g.max_lands()),
                        g.description.clone(),
                    ])
                });
                match lines {
                    Some(ls) => {
                        for l in ls.into_iter().filter(|l| !l.is_empty()) {
                            self.message(&text::colored(&l, "gold")).await?;
                        }
                        Ok(())
                    }
                    None => self.error("그런 길드가 없습니다.").await,
                }
            }
            ["창고" | "storage"] => Ok(self.open_guild_storage(0).await?),
            ["스폰" | "spawn"] => self.guild_spawn().await,
            ["스폰지정" | "setspawn"] => {
                let spot = super::Spot {
                    instance: self.server.name.clone(),
                    dimension: self.dimension,
                    pos: self.position,
                };
                let r = guilds().write(|d| d.set_spawn(&me, spot));
                self.guild_say(r).await
            }
            ["땅" | "land"] => Ok(self.open_guild(Page::Lands { page: 0 }).await?),
            ["땅" | "land", "사기" | "claim"] => self.claim_here().await,
            ["땅" | "land", "팔기" | "sell"] => {
                let here = super::land::chunk_of(self.position.0, self.position.2);
                let index = guilds().read(|d| {
                    d.of(me.uuid).and_then(|g| {
                        g.lands
                            .iter()
                            .position(|l| l.is(&self.server.name, self.dimension, here))
                    })
                });
                match index {
                    Some(index) => Ok(self.open_guild(Page::SellLand { index }).await?),
                    None => self.error("여기는 우리 길드 땅이 아닙니다.").await,
                }
            }
            ["설명" | "desc", ..] => {
                let r = guilds().write(|d| d.set_text(&me, Perm::Description, &rest(1)));
                self.guild_say(r).await
            }
            ["입장메시지" | "enter", ..] => {
                let r = guilds().write(|d| d.set_text(&me, Perm::EnterMessage, &rest(1)));
                self.guild_say(r).await
            }
            ["퇴장메시지" | "exit", ..] => {
                let r = guilds().write(|d| d.set_text(&me, Perm::LeaveMessage, &rest(1)));
                self.guild_say(r).await
            }
            ["방문객" | "visitor", op @ ("추가" | "제거" | "add" | "remove"), name] => {
                let add = matches!(*op, "추가" | "add");
                let who = match self.server.find_player_anywhere(name) {
                    Some(p) => (p.uuid, p.name),
                    None => match guilds().read(|d| {
                        d.of(me.uuid)
                            .and_then(|g| g.visitors.iter().find(|v| v.1.eq_ignore_ascii_case(name)).cloned())
                    }) {
                        Some(v) => v,
                        None => return self.error(&format!("{name}님을 찾지 못했습니다 (접속해 있어야 넣을 수 있음).")).await,
                    },
                };
                let r = guilds().write(|d| d.visitor(&me, who, add));
                self.guild_say(r).await
            }
            ["강퇴" | "kick", name] => {
                let target = guilds().read(|d| d.of(me.uuid).and_then(|g| g.member_named(name)).map(|m| m.uuid));
                let r = guilds().write(|d| d.kick(&me, name));
                if let (Ok(_), Some(t)) = (&r, target) {
                    self.tell_player(t, &format!("[길드] {}님이 길드에서 내보냈습니다.", me.name));
                }
                self.guild_say(r).await
            }
            ["위임" | "transfer", name] => {
                let target = guilds().read(|d| d.of(me.uuid).and_then(|g| g.member_named(name)).map(|m| m.uuid));
                match target {
                    Some(uuid) => Ok(self.open_guild(Page::Transfer { uuid }).await?),
                    None => self.error("그런 길드원이 없습니다.").await,
                }
            }
            ["부길드장" | "officer", name] => {
                let is_officer = guilds().read(|d| {
                    d.of(me.uuid)
                        .and_then(|g| g.member_named(name))
                        .is_some_and(|m| m.role == super::Role::Officer)
                });
                let r = guilds().write(|d| d.set_officer(&me, name, !is_officer));
                self.guild_say(r).await
            }
            _ => {
                self.error("사용법: /길드 [만들기|가입|탈퇴|정보|목록|창고|스폰|스폰지정|땅|설명|입장메시지|퇴장메시지|방문객|강퇴|위임|부길드장|해산]")
                    .await
            }
        }
    }

    pub(crate) async fn guild_spawn(&mut self) -> Result<(), FrameError> {
        let spot = guilds().read(|d| d.of(self.profile.uuid).and_then(|g| g.spawn.clone()));
        let Some(s) = spot else {
            return self.error("길드 스폰이 없습니다 (설정에서 지정).").await;
        };
        if s.instance != self.server.name {
            return self
                .error(&format!(
                    "길드 스폰은 {} 채널에 있습니다 (/이동 {})",
                    s.instance, s.instance
                ))
                .await;
        }
        if s.dimension != self.dimension {
            self.change_dimension_to(s.dimension, Some(s.pos)).await?;
        } else {
            self.teleport_and_load(s.pos).await?;
        }
        self.message(&text::colored("길드 스폰으로 왔습니다.", "green"))
            .await?;
        Ok(())
    }

    pub(crate) async fn claim_here(&mut self) -> Result<(), FrameError> {
        let me = self.actor();
        let chunk = super::land::chunk_of(self.position.0, self.position.2);
        // 돈을 받기 전에 될지 미리 봅니다
        let check =
            guilds()
                .read(|d| d.clone())
                .claim(&me, &self.server.name, self.dimension, chunk, 0);
        if let Err(e) = check {
            return self.error(&e).await;
        }
        if !self.pay(super::land::price()).await {
            return Ok(());
        }
        let r = guilds()
            .write(|d| d.claim(&me, &self.server.name, self.dimension, chunk, super::now()));
        if r.is_err() {
            crate::dambi::child::shared_add(me.uuid, "money", super::land::price()).await;
        }
        self.guild.territory = None; // 입장 메시지를 다시
        self.guild_say(r).await
    }

    pub(crate) async fn go_to_land(&mut self, land: &Land) -> Result<(), FrameError> {
        if land.instance != self.server.name {
            return self
                .error(&format!(
                    "그 땅은 {} 채널에 있습니다 (/이동 {})",
                    land.instance, land.instance
                ))
                .await;
        }
        let (x, z) = land.center();
        let d = land.dimension;
        let top = d.min_y() + d.height() - 1;
        let mut y = top;
        while y > d.min_y() {
            let s = self.server.world.get(d, x, y, z);
            if !crate::data::blocks::is_air(s) {
                break;
            }
            y -= 1;
        }
        let pos = (x as f64 + 0.5, y as f64 + 1.0, z as f64 + 0.5);
        if d != self.dimension {
            self.change_dimension_to(d, Some(pos)).await?;
        } else {
            self.teleport_and_load(pos).await?;
        }
        Ok(())
    }

    pub(crate) async fn open_guild_storage(&mut self, page: usize) -> std::io::Result<()> {
        let me = self.profile.uuid;
        let info = guilds().read(|d| {
            d.of(me)
                .map(|g| (g.id, g.storage_pages(), g.can(me, Perm::StorageUse)))
        });
        if super::config::with(|c| c.storage_blocked_instances.contains(&self.server.name)) {
            return self
                .message(&text::colored(
                    "이 세션에서는 길드 창고를 쓸 수 없습니다.",
                    "red",
                ))
                .await;
        }
        let Some((guild, pages, allowed)) = info else {
            return self
                .message(&text::colored("길드에 들어가 있지 않습니다.", "red"))
                .await;
        };
        if !allowed {
            return self
                .message(&text::colored("권한이 없습니다: 창고 넣고 빼기", "red"))
                .await;
        }
        let page = page.min(pages - 1);
        self.close_window(false).await?;
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        self.window = Some(Window {
            id,
            kind: WindowKind::GuildStorage { guild, page },
            grid: [None; 9],
        });
        let title = format!("길드 창고 {}/{}", page + 1, pages);
        self.send(
            crate::packets::container::OPEN_SCREEN_ID,
            &crate::packets::container::encode_open_screen(
                id,
                crate::data::menu::GENERIC_9X6,
                &text::plain(&title),
            ),
        )
        .await?;
        self.send_window().await
    }

    /// 창고 창의 아래 줄 (뒤로·이전·페이지·다음·기록).
    pub(crate) fn storage_nav(&self, guild: GuildId, page: usize) -> Vec<Option<Item>> {
        let pages = guilds().read(|d| d.guild(guild).map_or(1, |g| g.storage_pages()));
        let mut row = vec![None; 9];
        row[0] = self.icon(45, "dark_oak_door", "길드 창", "white", Vec::new(), false);
        if page > 0 {
            row[3] = self.icon(48, "arrow", "이전 페이지", "white", Vec::new(), false);
        }
        row[4] = self.icon(
            49,
            "chest",
            &format!("{}/{} 페이지", page + 1, pages),
            "yellow",
            vec!["업그레이드로 페이지가 늘어납니다".into()],
            false,
        );
        if page + 1 < pages {
            row[5] = self.icon(50, "arrow", "다음 페이지", "white", Vec::new(), false);
        }
        row[8] = self.icon(53, "book", "창고 기록", "white", Vec::new(), false);
        row
    }

    pub(crate) async fn storage_nav_click(
        &mut self,
        page: usize,
        slot: usize,
    ) -> std::io::Result<()> {
        match slot {
            45 => self.open_guild(Page::Main).await,
            48 if page > 0 => self.open_guild_storage(page - 1).await,
            50 => self.open_guild_storage(page + 1).await,
            53 => self.open_guild(Page::Log { page: 0 }).await,
            _ => Ok(()),
        }
    }

    // ---- 땅 보호·입장 메시지 ----

    /// 이 블록을 부수거나 놓거나 써도 되나 (길드 땅 보호). 크리에이티브 OP는 언제나.
    pub(crate) fn may_build_at(&self, pos: BlockPos) -> bool {
        if self.gamemode == crate::packets::play::GameMode::Creative
            && self.server.perms.op_level(self.profile.uuid) >= 2
        {
            return true;
        }
        let chunk = (pos.0 >> 4, pos.2 >> 4);
        guilds().read(|d| d.may_build(self.profile.uuid, &self.server.name, self.dimension, chunk))
    }

    pub(crate) async fn deny_build(&mut self) -> std::io::Result<()> {
        let owner = guilds().read(|d| {
            let c = super::land::chunk_of(self.position.0, self.position.2);
            d.owner_of(&self.server.name, self.dimension, c)
                .map(|g| g.name.clone())
        });
        let msg = match owner {
            Some(n) => format!("{n} 길드의 땅입니다."),
            None => "다른 길드의 땅입니다.".into(),
        };
        self.action_bar(&text::colored(&msg, "red")).await
    }

    /// 움직인 뒤: 다른 길드 땅에 들어가거나 나왔으면 메시지.
    pub(crate) async fn guild_moved(&mut self) -> std::io::Result<()> {
        let chunk = super::land::chunk_of(self.position.0, self.position.2);
        let here = guilds().read(|d| {
            d.owner_of(&self.server.name, self.dimension, chunk)
                .map(|g| (g.id, g.name.clone(), g.enter_message.clone()))
        });
        let now = here.as_ref().map(|h| h.0);
        if now == self.guild.territory {
            return Ok(());
        }
        let left = self.guild.territory.take();
        if let Some(id) = left {
            let msg = guilds().read(|d| d.guild(id).map(|g| g.leave_message.clone()));
            let text = msg
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| format!("{} 길드 땅을 나왔습니다", self.guild.territory_name));
            self.action_bar(&text::colored(&text, "gray")).await?;
        }
        if let Some((id, name, enter)) = here {
            self.guild.territory = Some(id);
            self.guild.territory_name = name.clone();
            let title = if enter.is_empty() {
                format!("{name} 길드")
            } else {
                enter
            };
            self.show_title(&format!("{name}|{title}")).await?;
        }
        Ok(())
    }

    // ---- 캐기·잡기 ----

    /// 블록을 부순 뒤 (서바이벌): 생활 직업 경험치, 광석 덤, 퀘스트.
    pub(crate) async fn guild_block_broken(
        &mut self,
        pos: BlockPos,
        state: u32,
    ) -> std::io::Result<()> {
        let name = crate::data::blocks::name(crate::data::blocks::block_of(state));
        let mature = crop_is_mature(state);
        let me = self.profile.uuid;
        let now = super::now();
        // 생활 직업 경험치 (이벤트면 2배)
        let job = if name.ends_with("_ore") || name == "ancient_debris" {
            Some(("광부", 5.0))
        } else if mature {
            Some(("농부", 3.0))
        } else if name.ends_with("_log") || name.ends_with("_stem") {
            Some(("나무꾼", 2.0))
        } else {
            None
        };
        // 길드: 이벤트(2배)와 레벨 보너스
        let (events, level) = guilds().read(|d| {
            d.of(me)
                .map(|g| {
                    (
                        (
                            g.event_active(GuildEvent::JobXp, now),
                            g.event_active(GuildEvent::RareDrop, now),
                        ),
                        Some(g.level_row()),
                    )
                })
                .unwrap_or(((false, false), None))
        });
        if let Some((job, xp)) = job {
            let bonus = level.as_ref().map_or(0.0, |l| l.job_xp_bonus);
            let xp = xp * (1.0 + bonus / 100.0) * if events.0 { 2.0 } else { 1.0 };
            crate::plugins::rank::job_xp(me, job, xp).await;
        }
        // 높은 등급 재료: 광석이면 5% + 길드 레벨 보너스 (이벤트면 2배)로 한 번 더
        if name.ends_with("_ore") && self.gamemode == crate::packets::play::GameMode::Survival {
            let percent = (5.0 + level.as_ref().map_or(0.0, |l| l.rare_bonus))
                * if events.1 { 2.0 } else { 1.0 };
            if (crate::game::rand_u32() % 10_000) < (percent * 100.0) as u32
                && crate::data::items::can_harvest(
                    crate::data::blocks::block_of(state),
                    self.main_hand(),
                )
                && let Some(drop) = crate::data::items::block_drop(
                    state,
                    crate::game::rand_u32() % 1000,
                    crate::game::rand_u32(),
                )
            {
                self.server
                    .entities
                    .drop_at_block(self.server, self.dimension, pos, drop);
                self.action_bar(&text::colored("덤! 재료가 하나 더 나왔습니다", "aqua"))
                    .await?;
            }
        }
        // 퀘스트: 캐기 / 광석 / 통나무 / 다 자란 작물
        let mut kinds = vec!["mine"];
        if name.ends_with("_ore") || name == "ancient_debris" {
            kinds.push("ores");
        }
        if name.ends_with("_log") || name.ends_with("_stem") {
            kinds.push("logs");
        }
        if mature {
            kinds.push("harvest");
        }
        for kind in kinds {
            self.quest_happened(crate::plugins::quest::Happened {
                kind,
                name: name.to_owned(),
                amount: 1,
            })
            .await?;
        }
        Ok(())
    }

    // ---- 1초마다: 신호기, 재생, 전투력 ----

    pub(crate) async fn guild_tick(&mut self, power: i64) -> std::io::Result<()> {
        self.guild.ticks = self.guild.ticks.wrapping_add(1);
        let me = self.profile.uuid;
        // 재생 (신호기)
        if self.guild.bonus.regen > 0.0
            && !self.vitals.dead
            && self.vitals.health < 20.0
            && self.gamemode.takes_damage()
        {
            self.vitals.health = (self.vitals.health + self.guild.bonus.regen).min(20.0);
            self.send_health().await?;
        }
        if self.guild.ticks % 5 != 1 {
            return Ok(());
        }
        let (owned, stored_power) = guilds().read(|d| {
            d.of(me)
                .map(|g| (g.beacon.clone(), g.member(me).map_or(0, |m| m.power)))
                .unwrap_or_default()
        });
        self.guild.bonus = beacon::bonus(&owned);
        // 효과: 새로 생긴 것·단계가 바뀐 것은 걸고, 없어진 것은 뗌
        let want: Vec<(Effect, u8)> = owned.iter().map(|o| (o.effect, o.tier)).collect();
        for &(e, t) in &want {
            if !self.guild.sent.contains(&(e, t)) {
                let body = fx::encode_mob_effect(self.entity_id, e.network_id(), t as i32 - 1, -1);
                self.send(fx::UPDATE_MOB_EFFECT_ID, &body).await?;
            }
        }
        for &(e, _) in &self.guild.sent.clone() {
            if !want.iter().any(|w| w.0 == e) {
                let body = fx::encode_remove_mob_effect(self.entity_id, e.network_id());
                self.send(fx::REMOVE_MOB_EFFECT_ID, &body).await?;
            }
        }
        self.guild.sent = want;
        // 신속은 이동 속도 속성도 (효과의 속성은 서버가 보내야 함)
        if (self.guild.bonus.speed - self.guild.speed_sent).abs() > 1e-3 {
            let extra = (self.guild.bonus.speed - 1.0) as f64;
            let modifier = (extra > 0.0).then_some(("hoya:guild_beacon", extra, 2u8));
            let body =
                fx::encode_attribute(self.entity_id, fx::ATTRIBUTE_MOVEMENT_SPEED, 0.1, modifier);
            self.send(fx::UPDATE_ATTRIBUTES_ID, &body).await?;
            self.guild.speed_sent = self.guild.bonus.speed;
        }
        // 전투력은 바뀌었을 때만 적습니다 (30초에 한 번)
        if self.guild.ticks % 30 == 1 && power != stored_power && power != self.guild.power_written
        {
            guilds().write(|d| {
                if let Some(m) = d.of_mut(me).and_then(|g| g.member_mut(me)) {
                    m.power = power;
                    m.name.clone_from(&self.profile.name);
                }
            });
            self.guild.power_written = power;
        }
        Ok(())
    }
}

/// 다 자란 작물인가 (밀·당근·감자 age 7, 비트 3, 네더 사마귀 3, 코코아 2).
pub(crate) fn crop_is_mature(state: u32) -> bool {
    use crate::data::blocks;
    let name = blocks::name(blocks::block_of(state));
    let max = match name {
        "wheat" | "carrots" | "potatoes" => "7",
        "beetroots" | "nether_wart" => "3",
        "cocoa" => "2",
        _ => return false,
    };
    blocks::property(state, "age") == Some(max)
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    /// 창고 칸 바꾸기를 저장에 반영합니다: 그 칸이 아직 `before`와 같을 때만 (아니면 false).
    /// 바뀐 만큼 기록을 남기고, 같은 페이지를 보는 사람들에게 알립니다.
    pub(crate) fn commit_guild_storage(
        &self,
        before: &[Option<Item>],
        changes: &[(u32, usize, usize, Option<Item>)],
    ) -> bool {
        let who = self.profile.name.clone();
        let now = super::now();
        let ok = guilds().write(|d| {
            let Some(&(guild, page, _, _)) = changes.first() else {
                return true;
            };
            let Some(g) = d.guild_mut(guild) else {
                return false;
            };
            let Some(slots) = g.storage.get(page) else {
                return false;
            };
            if changes
                .iter()
                .any(|&(_, _, i, _)| slots.get(i) != before.get(i))
            {
                return false;
            }
            // 칸별 증감을 아이템 종류별로 모아 기록
            let mut delta: std::collections::BTreeMap<String, i32> = Default::default();
            for &(_, _, i, item) in changes {
                let old = g.storage[page][i];
                for (it, sign) in [(old, -1), (item, 1)] {
                    if let Some(it) = it {
                        *delta.entry(item_label(&it)).or_default() += sign * it.count;
                    }
                }
                g.storage[page][i] = item;
            }
            for (item, n) in delta.into_iter().filter(|(_, n)| *n != 0) {
                g.push_log(super::LogEntry {
                    at: now,
                    who: who.clone(),
                    delta: n,
                    item,
                    page,
                });
            }
            true
        });
        if ok && let Some(&(guild, page, _, _)) = changes.first() {
            self.server
                .publish_all(Event::GuildStorageChanged { guild, page });
        }
        ok
    }
}

fn item_label(i: &Item) -> String {
    match i.custom.and_then(crate::game::custom_items::get) {
        Some(c) => c.name,
        None => crate::data::items::name(i.id).to_owned(),
    }
}
