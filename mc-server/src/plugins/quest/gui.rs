//! `/퀘스트` 창: 위 줄은 탭(일반·주간·길드 주간), 가운데 4줄이 퀘스트, 아래 줄은 쪽 넘기기.
//!
//! - 일반: 좌클릭 수락 → 진행 중엔 좌클릭으로 HUD 따라다니기 켜고 끄기, Shift+우클릭 포기
//! - 주간·길드 주간: 저절로 진행, 좌클릭 HUD 켜고 끄기
//! - "내기" 퀘스트는 좌클릭하면 가방에서 그 아이템을 가져감

use tokio::io::AsyncWrite;

use super::config::{self, QuestDef};
use super::progress::Kind;
use super::session::{item_is, progress};
use crate::data::menu;
use crate::packets::container as pc;
use crate::packets::world::Item;
use crate::protocol::frame::FrameError;
use crate::session::play::Session;
use crate::session::windows::{Window, WindowKind};
use crate::text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub tab: Kind,
    pub page: usize,
}

const TABS: [(usize, Kind, &str); 3] = [
    (2, Kind::Normal, "writable_book"),
    (4, Kind::Weekly, "clock"),
    (6, Kind::Guild, "white_banner"),
];
const FIRST: usize = 9;
const PER_PAGE: usize = 36;
const PREV: usize = 48;
const INFO: usize = 49;
const NEXT: usize = 50;

fn icon_for(def: &QuestDef) -> &'static str {
    match def.objective.kind.as_str() {
        "mine" => "iron_pickaxe",
        "ores" => "iron_ore",
        "logs" => "oak_log",
        "harvest" => "wheat",
        "kill" => "iron_sword",
        "deliver" => "chest",
        _ => "paper",
    }
}

pub fn bar(progress: u32, target: u32) -> String {
    let filled = (progress as usize * 12)
        .checked_div(target as usize)
        .unwrap_or(0)
        .min(12);
    format!(
        "{}{} {progress}/{target}",
        "■".repeat(filled),
        "□".repeat(12 - filled)
    )
}

fn rewards_lore(def: &QuestDef) -> Vec<String> {
    let r = &def.rewards;
    let mut v = vec!["보상".to_owned()];
    if r.money > 0 {
        v.push(format!("  돈 {}", crate::plugins::hud::number(r.money)));
    }
    if r.cash > 0 {
        v.push(format!("  캐시 {}", r.cash));
    }
    if r.gp > 0 {
        v.push(format!("  길드 GP {}", r.gp));
    }
    for (j, x) in &r.job_xp {
        v.push(format!("  {j} 경험치 {x:.0}"));
    }
    for (i, c) in &r.items {
        let label = super::session::item_by_name(i, 1)
            .and_then(|it| {
                it.custom
                    .and_then(crate::game::custom_items::get)
                    .map(|c| c.name)
            })
            .unwrap_or_else(|| i.clone());
        v.push(format!("  {label} ×{c}"));
    }
    v
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    fn guild_of_me(&self) -> Option<u32> {
        crate::plugins::guild::guilds().read(|d| d.of(self.profile.uuid).map(|g| g.id))
    }

    pub(crate) async fn open_quests(&mut self, page: Page) -> std::io::Result<()> {
        self.close_window(false).await?;
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        self.window = Some(Window {
            id,
            kind: WindowKind::Quest(page),
            grid: [None; 9],
        });
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, menu::GENERIC_9X6, &text::plain("퀘스트")),
        )
        .await?;
        self.send_window().await
    }

    fn quest_list(&self, tab: Kind) -> Vec<(QuestDef, u32, bool, bool)> {
        let uuid = self.profile.uuid;
        let guild = self.guild_of_me();
        config::with(|cfg| progress().write(|p| p.list(uuid, guild, tab, cfg, super::now())))
    }

    pub(crate) fn quest_slots(&self, page: &Page) -> Vec<Option<Item>> {
        let mut slots: Vec<Option<Item>> = vec![None; 54];
        for (slot, kind, item) in TABS {
            let on = kind == page.tab;
            slots[slot] = self.icon(
                slot,
                item,
                kind.name(),
                if on { "gold" } else { "gray" },
                vec![if on {
                    "지금 보는 탭".into()
                } else {
                    "누르면 보기".into()
                }],
                on,
            );
        }
        let list = self.quest_list(page.tab);
        if page.tab == Kind::Guild && self.guild_of_me().is_none() {
            slots[22] = self.icon(
                22,
                "barrier",
                "길드에 들어가 있지 않습니다",
                "dark_gray",
                vec!["/길드 로 가입".into()],
                false,
            );
        }
        let uuid = self.profile.uuid;
        for (i, (def, prog, done, accepted)) in list
            .iter()
            .enumerate()
            .skip(page.page * PER_PAGE)
            .take(PER_PAGE)
        {
            let slot = FIRST + i - page.page * PER_PAGE;
            let tracked = progress().read(|p| p.is_tracked(uuid, page.tab, &def.id));
            let mut lore = vec![def.description.clone(), bar(*prog, def.target)];
            lore.extend(rewards_lore(def));
            let state = match (page.tab, *done, *accepted) {
                (_, true, _) => "완료",
                (Kind::Normal, false, false) => "좌클릭: 수락",
                (_, false, _) if def.objective.kind == "deliver" => "좌클릭: 가방의 아이템 내기",
                _ => "좌클릭: HUD에 띄우기/빼기",
            };
            lore.push(state.into());
            if page.tab == Kind::Normal && *accepted && !*done {
                lore.push("Shift+우클릭: 포기".into());
            }
            if tracked {
                lore.push("HUD에 표시 중".into());
            }
            if def.repeatable {
                lore.push("반복 가능".into());
            }
            let color = if *done {
                "green"
            } else if *accepted {
                "yellow"
            } else {
                "white"
            };
            slots[slot] = self.icon(
                slot,
                if *done { "lime_dye" } else { icon_for(def) },
                &format!("[{}] {}", page.tab.name(), def.name),
                color,
                lore,
                tracked || *done,
            );
        }
        if page.page > 0 {
            slots[PREV] = self.icon(PREV, "arrow", "이전 쪽", "white", Vec::new(), false);
        }
        if (page.page + 1) * PER_PAGE < list.len() {
            slots[NEXT] = self.icon(NEXT, "arrow", "다음 쪽", "white", Vec::new(), false);
        }
        slots[INFO] = self.icon(
            INFO,
            "book",
            "퀘스트",
            "gold",
            vec![
                "일반: 받아서 하는 퀘스트".into(),
                "주간: 월요일 0시(한국)에 새로".into(),
                "길드 주간: 길드원이 함께 채움".into(),
            ],
            false,
        );
        slots
    }

    pub(crate) async fn quest_click(
        &mut self,
        page: Page,
        slot: i16,
        button: i8,
        mode: i32,
    ) -> Result<(), FrameError> {
        let Ok(slot) = usize::try_from(slot) else {
            return Ok(());
        };
        let set = |s: &mut Self, p: Page| {
            if let Some(w) = &mut s.window {
                w.kind = WindowKind::Quest(p);
            }
        };
        if let Some((_, kind, _)) = TABS.iter().find(|t| t.0 == slot) {
            set(
                self,
                Page {
                    tab: *kind,
                    page: 0,
                },
            );
            return Ok(());
        }
        match slot {
            PREV if page.page > 0 => {
                set(
                    self,
                    Page {
                        page: page.page - 1,
                        ..page
                    },
                );
                return Ok(());
            }
            NEXT => {
                set(
                    self,
                    Page {
                        page: page.page + 1,
                        ..page
                    },
                );
                return Ok(());
            }
            _ => {}
        }
        if !(FIRST..FIRST + PER_PAGE).contains(&slot) {
            return Ok(());
        }
        let list = self.quest_list(page.tab);
        let Some((def, _, done, accepted)) = list.get(page.page * PER_PAGE + slot - FIRST).cloned()
        else {
            return Ok(());
        };
        let uuid = self.profile.uuid;
        let now = super::now();
        let shift = mode == 1;
        let right = button == 1;
        let r = if done {
            Err("이미 끝낸 퀘스트입니다.".to_owned())
        } else if page.tab == Kind::Normal && !accepted {
            config::with(|cfg| progress().write(|p| p.accept(uuid, &def.id, cfg, now)))
        } else if page.tab == Kind::Normal && shift && right {
            progress().write(|p| p.abandon(uuid, &def.id))
        } else if def.objective.kind == "deliver" {
            return self.deliver_quest(page.tab, &def).await;
        } else {
            config::with(|cfg| {
                progress().write(|p| p.toggle_track(uuid, page.tab, &def.id, cfg, now))
            })
        };
        match r {
            Ok(m) => self.message(&text::colored(&m, "green")).await?,
            Err(e) => self.message(&text::colored(&e, "red")).await?,
        }
        Ok(())
    }

    /// 가방에서 퀘스트 아이템을 냅니다.
    async fn deliver_quest(&mut self, tab: Kind, def: &QuestDef) -> Result<(), FrameError> {
        let wanted = |i: &Item| def.objective.filter.iter().any(|f| item_is(i, f));
        let have: u32 = self.inventory[9..45]
            .iter()
            .flatten()
            .filter(|i| wanted(i))
            .map(|i| i.count as u32)
            .sum();
        let uuid = self.profile.uuid;
        let guild = self.guild_of_me();
        let r = config::with(|cfg| {
            progress().write(|p| p.deliver(uuid, guild, tab, &def.id, have, cfg, super::now()))
        });
        let (took, done) = match r {
            Ok(x) => x,
            Err(e) => return self.error(&e).await,
        };
        // 가방에서 그만큼 꺼냄
        let mut left = took as i32;
        for slot in 9..45 {
            if left == 0 {
                break;
            }
            if let Some(it) = self.inventory[slot]
                && wanted(&it)
            {
                let n = it.count.min(left);
                left -= n;
                self.inventory[slot] = Some(it.with_count(it.count - n)).filter(|i| i.count > 0);
                self.sync_slot(slot).await?;
            }
        }
        self.message(&text::colored(&format!("{took}개를 냈습니다."), "green"))
            .await?;
        if let Some(c) = done {
            self.grant_quest(c).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_and_slots() {
        assert_eq!(bar(6, 12), "■■■■■■□□□□□□ 6/12");
        let mut used: Vec<usize> = TABS.iter().map(|t| t.0).collect();
        used.extend([PREV, INFO, NEXT]);
        assert!(used.iter().all(|s| !(FIRST..FIRST + PER_PAGE).contains(s)));
    }
}
