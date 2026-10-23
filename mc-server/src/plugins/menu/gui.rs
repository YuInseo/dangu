//! 퀵메뉴 창들: 메인(이동 + 서브 기능), 워프, 던전, 내 정보, 생활 스탯, 모험가 스탯, 설정, 쓰레기통.

use tokio::io::AsyncWrite;

use super::Dest;
use crate::dambi::{child, shared};
use crate::data::menu;
use crate::packets::container as pc;
use crate::packets::world::Item;
use crate::protocol::frame::FrameError;
use crate::session::play::Session;
use crate::session::windows::{Window, WindowKind};
use crate::text;
use crate::world::{Dimension, terrain};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Warps,
    Dungeons,
    Info,
    Jobs,
    Adventurer,
    Settings,
}

impl Page {
    pub fn size(self) -> usize {
        match self {
            Page::Main | Page::Warps => 54,
            _ => 27,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Page::Main => "퀵메뉴",
            Page::Warps => "광장 워프",
            Page::Dungeons => "던전",
            Page::Info => "내 정보",
            Page::Jobs => "생활 스탯",
            Page::Adventurer => "모험가 스탯",
            Page::Settings => "설정",
        }
    }

    /// 뒤로 가기 버튼 칸과 돌아갈 곳.
    fn back(self) -> Option<(usize, Page)> {
        match self {
            Page::Main => None,
            Page::Warps => Some((49, Page::Main)),
            Page::Jobs | Page::Adventurer => Some((18, Page::Info)),
            _ => Some((18, Page::Main)),
        }
    }
}

/// 쓰레기통 칸 수.
pub const TRASH_SLOTS: usize = 27;

/// 메뉴가 쓰는 것: 쓰레기통 칸, 열 때 받아 둔 재화·직업.
#[derive(Debug, Default)]
pub struct MenuState {
    pub trash: Vec<Option<Item>>,
    shared: Option<shared::SharedData>,
}

/// 서브 기능 (왼쪽부터): 칸, 아이콘, 이름, 설명.
const SUB: [(usize, &str, &str, &str); 7] = [
    (
        46,
        "paper",
        "내 정보",
        "이름·길드·생활 스탯·모험가 스탯·도감",
    ),
    (47, "compass", "길라잡이", "처음이라면 여기부터"),
    (48, "writable_book", "우편함", "준비 중"),
    (49, "emerald", "후원", "후원 등급과 특혜"),
    (
        50,
        "book",
        "위키 · 디스코드 · 마인리스트",
        "누르면 채팅에 주소",
    ),
    (51, "comparator", "설정", "HUD · 갑옷 숨기기 · 채팅 채널"),
    (52, "cauldron", "쓰레기통", "넣고 닫으면 사라짐"),
];
const MAIN_INFO: usize = 4;
const WARP_SLOTS: [usize; 21] = [
    10, 11, 12, 13, 14, 15, 16, 19, 20, 21, 22, 23, 24, 25, 28, 29, 30, 31, 32, 33, 34,
];
const DUNGEON_SLOTS: [usize; 7] = [10, 11, 12, 13, 14, 15, 16];
const JOBS: [(&str, &str); 4] = [
    ("광부", "iron_pickaxe"),
    ("농부", "golden_hoe"),
    ("나무꾼", "iron_axe"),
    ("대장장이", "anvil"),
];

/// 메인 이동 칸: 개수에 맞춰 가운데 줄에.
fn main_slots(n: usize) -> Vec<usize> {
    let start = 22usize.saturating_sub(n.min(7) / 2);
    (start..start + n.min(7)).collect()
}

fn dungeon_slots(n: usize) -> Vec<usize> {
    if n <= 3 {
        [11, 13, 15][..n].to_vec()
    } else {
        DUNGEON_SLOTS[..n.min(7)].to_vec()
    }
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    /// 지금 이 인스턴스 이름.
    fn instance_here(&self) -> String {
        child::instance_name().map_or_else(|| self.server.name.clone(), str::to_owned)
    }

    fn instance_exists(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        if name == self.instance_here() {
            return true;
        }
        if child::active() {
            child::instances().iter().any(|(n, _)| n == name)
        } else {
            self.server.network().is_some_and(|n| n.get(name).is_some())
        }
    }

    /// `/메뉴` · Shift+F.
    pub(crate) async fn open_quick_menu(&mut self, page: Page) -> std::io::Result<()> {
        if matches!(page, Page::Info | Page::Jobs) {
            self.menu.shared = child::shared(self.profile.uuid).await;
        }
        self.close_window(false).await?;
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        self.window = Some(Window {
            id,
            kind: WindowKind::Menu(page),
            grid: [None; 9],
        });
        let kind = if page.size() == 54 {
            menu::GENERIC_9X6
        } else {
            menu::GENERIC_9X3
        };
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, kind, &text::plain(page.title())),
        )
        .await?;
        self.send_window().await
    }

    /// `/쓰레기통`: 넣고 닫으면 사라짐.
    pub(crate) async fn open_trash(&mut self) -> std::io::Result<()> {
        self.close_window(false).await?;
        self.menu.trash = vec![None; TRASH_SLOTS];
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        self.window = Some(Window {
            id,
            kind: WindowKind::Trash,
            grid: [None; 9],
        });
        let title = text::colored("쓰레기통 — 닫으면 사라집니다", "dark_red");
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, menu::GENERIC_9X3, &title),
        )
        .await?;
        self.send_window().await
    }

    /// 쓰레기통을 닫음: 넣은 것은 사라짐.
    pub(crate) fn trash_closed(&mut self) {
        self.menu.trash.clear();
    }

    fn dest_icon(&self, slot: usize, d: &Dest) -> Option<Item> {
        let here = !d.instance.is_empty() && d.instance == self.instance_here();
        let ready = !d.command.is_empty() || self.instance_exists(&d.instance);
        let mut lore = d.lore.clone();
        lore.push(String::new());
        if here {
            lore.push("● 지금 여기 (누르면 그 자리로)".into());
        } else if ready {
            lore.push(if d.command.starts_with('@') {
                "▶ 클릭: 열기".into()
            } else if d.command.is_empty() {
                format!("▶ 클릭: {}(으)로 이동", d.name)
            } else {
                "▶ 클릭: 바로 열기".into()
            });
        } else {
            lore.push("준비 중".into());
        }
        if d.shift_command == "@warps" {
            lore.push("▶ Shift+클릭: 자세한 워프".into());
        }
        let color = if here {
            "green"
        } else if ready {
            "gold"
        } else {
            "dark_gray"
        };
        let icon = if ready || here {
            d.icon.as_str()
        } else {
            "gray_dye"
        };
        self.icon(slot, icon, &d.name, color, lore, here)
    }

    pub(crate) fn menu_slots(&self, page: Page) -> Vec<Option<Item>> {
        let size = page.size();
        let mut slots: Vec<Option<Item>> = vec![None; size];
        // 테두리
        for (s, slot) in slots.iter_mut().enumerate() {
            let (row, col) = (s / 9, s % 9);
            if row == 0 || row + 1 == size / 9 || col == 0 || col == 8 {
                *slot = self.pane(s);
            }
        }
        if let Some((slot, _)) = page.back() {
            slots[slot] = self.icon(slot, "arrow", "◀ 뒤로", "white", Vec::new(), false);
        }
        let cfg = super::get();
        match page {
            Page::Main => {
                slots[MAIN_INFO] = self.icon(
                    MAIN_INFO,
                    "nether_star",
                    "퀵메뉴",
                    "aqua",
                    vec![
                        format!("지금 있는 곳: {}", self.instance_here()),
                        "Shift+F 로 언제든 열기 (/메뉴)".into(),
                    ],
                    true,
                );
                for (slot, d) in main_slots(cfg.main.len()).into_iter().zip(&cfg.main) {
                    slots[slot] = self.dest_icon(slot, d);
                }
                for (slot, icon, name, desc) in SUB {
                    slots[slot] = self.icon(slot, icon, name, "yellow", vec![desc.into()], false);
                }
            }
            Page::Warps => {
                for (&slot, d) in WARP_SLOTS.iter().zip(&cfg.warps) {
                    slots[slot] = self.dest_icon(slot, d);
                }
            }
            Page::Dungeons => {
                for (slot, d) in dungeon_slots(cfg.dungeons.len())
                    .into_iter()
                    .zip(&cfg.dungeons)
                {
                    slots[slot] = self.dest_icon(slot, d);
                }
            }
            Page::Info => self.info_slots(&mut slots),
            Page::Jobs => self.jobs_slots(&mut slots),
            Page::Adventurer => self.adventurer_slots(&mut slots),
            Page::Settings => self.settings_slots(&mut slots),
        }
        slots
    }

    fn info_slots(&self, slots: &mut [Option<Item>]) {
        let me = self.profile.uuid;
        let d = self.menu.shared.clone().unwrap_or_default();
        let mut lore = Vec::new();
        lore.push(match &self.rank.rank {
            Some(r) => format!("등급  {}", r.name),
            None => "등급  없음".into(),
        });
        let guild = crate::plugins::guild::guilds().read(|g| {
            g.of(me)
                .map(|g| format!("{} · {}", g.name, g.role_of(me).map_or("", |r| r.name())))
        });
        lore.push(format!("길드  {}", guild.unwrap_or_else(|| "없음".into())));
        let st = self.rpg_stats(self.hud.job_levels());
        lore.push(format!(
            "레벨  {} · 전투력 {}",
            st.level,
            crate::plugins::hud::number(st.combat_power())
        ));
        if let Some(j) = d.jobs.iter().max_by_key(|j| j.level) {
            lore.push(format!("대표 직업  {} Lv.{}", j.name, j.level));
        }
        lore.push(format!(
            "돈 {} · 캐시 {} · 잠수 {}",
            crate::plugins::hud::number(d.money),
            crate::plugins::hud::number(d.cash),
            crate::plugins::hud::number(d.afk)
        ));
        let mins = d.playtime_secs / 60;
        lore.push(format!("놀이 시간  {}시간 {}분", mins / 60, mins % 60));
        lore.push(format!("있는 곳  {}", self.instance_here()));
        slots[10] = self.icon(10, "paper", &self.profile.name, "gold", lore, true);
        slots[12] = self.icon(
            12,
            "golden_hoe",
            "생활 스탯",
            "green",
            vec![
                "광부 · 농부 · 나무꾼 · 대장장이".into(),
                "/직업 으로도 열 수 있습니다".into(),
            ],
            false,
        );
        slots[14] = self.icon(
            14,
            "iron_sword",
            "모험가 스탯",
            "red",
            vec![
                "공격력 · 마법력 · 방어력 · 치명타".into(),
                "/모험가 로도 열 수 있습니다".into(),
            ],
            false,
        );
        slots[16] = self.icon(
            16,
            "knowledge_book",
            "도감",
            "dark_gray",
            vec!["준비 중".into()],
            false,
        );
    }

    fn jobs_slots(&self, slots: &mut [Option<Item>]) {
        let d = self.menu.shared.clone().unwrap_or_default();
        let mut jobs: Vec<(String, &str)> =
            JOBS.iter().map(|(n, i)| ((*n).to_owned(), *i)).collect();
        for j in &d.jobs {
            if !jobs.iter().any(|(n, _)| *n == j.name) {
                jobs.push((j.name.clone(), "paper"));
            }
        }
        let at = [10, 11, 12, 13, 14, 15, 16];
        for (&slot, (name, icon)) in at.iter().zip(&jobs) {
            let (level, xp) = d
                .jobs
                .iter()
                .find(|j| j.name == *name)
                .map_or((0, 0.0), |j| (j.level, j.xp));
            let need = shared::xp_to_next(level);
            let lore = vec![
                format!("Lv.{level}"),
                crate::plugins::quest::gui::bar(xp as u32, need as u32),
                match name.as_str() {
                    "광부" => "광석을 캐면 오름".into(),
                    "농부" => "다 자란 작물을 거두면 오름".into(),
                    "나무꾼" => "통나무를 베면 오름".into(),
                    "대장장이" => "대장간에서 만들면 오름".into(),
                    _ => String::new(),
                },
            ];
            slots[slot] = self.icon(slot, icon, name, "green", lore, level > 0);
        }
        if self.rank.perks.xp_mult > 1.0 {
            slots[22] = self.icon(
                22,
                "experience_bottle",
                "경험치 부스트",
                "aqua",
                vec![format!("후원 등급: {:.0}배", self.rank.perks.xp_mult)],
                true,
            );
        }
    }

    fn adventurer_slots(&self, slots: &mut [Option<Item>]) {
        let st = self.rpg_stats(self.hud.job_levels());
        let n = |v: f32| crate::plugins::hud::number(v.round() as i64);
        let rows: [(usize, &str, &str, String); 8] = [
            (4, "nether_star", "전투력", n(st.combat_power() as f32)),
            (10, "experience_bottle", "레벨", st.level.to_string()),
            (11, "iron_sword", "공격력", n(st.attack)),
            (12, "blaze_rod", "마법력", n(st.magic)),
            (13, "apple", "체력", n(st.max_health)),
            (14, "lapis_lazuli", "마나", n(st.max_mana)),
            (15, "iron_chestplate", "방어력", n(st.defense)),
            (
                16,
                "spectral_arrow",
                "치명타",
                format!("{:.1}% · 피해 {:.0}%", st.crit_chance, st.crit_damage),
            ),
        ];
        for (slot, icon, name, value) in rows {
            slots[slot] = self.icon(slot, icon, name, "red", vec![value], slot == 4);
        }
        slots[22] = self.icon(
            22,
            "netherite_chestplate",
            "장비 창 열기",
            "gold",
            vec!["악세서리·세트 효과 (/장비)".into()],
            false,
        );
    }

    fn settings_slots(&self, slots: &mut [Option<Item>]) {
        let on_off = |b: bool| if b { "켜짐" } else { "꺼짐" };
        slots[11] = self.icon(
            11,
            "map",
            "HUD (재화·퀘스트·스탯)",
            "aqua",
            vec![on_off(!self.hud.hidden).into(), "누르면 바꿈".into()],
            !self.hud.hidden,
        );
        slots[13] = self.icon(
            13,
            "leather_chestplate",
            "갑옷 숨기기",
            if self.rank.perks.hide_armor {
                "aqua"
            } else {
                "dark_gray"
            },
            vec![
                on_off(self.rank.hide_armor).into(),
                if self.rank.perks.hide_armor {
                    "누르면 바꿈".into()
                } else {
                    "후원 등급 특혜".into()
                },
            ],
            self.rank.hide_armor,
        );
        slots[15] = self.icon(
            15,
            "feather",
            "플라이",
            "aqua",
            vec!["누르면 남은 시간".into()],
            false,
        );
        let prefs = crate::plugins::chat::prefs(self.profile.uuid);
        for (i, c) in crate::plugins::chat::Channel::ALL.into_iter().enumerate() {
            let slot = 20 + i;
            if slot > 25 {
                break;
            }
            let on = !prefs.muted.contains(&c);
            slots[slot] = self.icon(
                slot,
                if on { "lime_dye" } else { "gray_dye" },
                &format!("채팅: {}", c.name()),
                if on { "green" } else { "gray" },
                vec![on_off(on).into(), "누르면 받기/끄기".into()],
                false,
            );
        }
    }

    pub(crate) async fn menu_click(
        &mut self,
        page: Page,
        slot: i16,
        mode: i32,
    ) -> Result<(), FrameError> {
        let Ok(slot) = usize::try_from(slot) else {
            return Ok(());
        };
        if page.back().is_some_and(|(s, _)| s == slot) {
            return Ok(self.open_quick_menu(page.back().unwrap().1).await?);
        }
        let shift = mode == pc::MODE_QUICK_MOVE;
        let cfg = super::get();
        let pick = |list: &[Dest], at: Vec<usize>| {
            at.iter()
                .position(|&s| s == slot)
                .and_then(|i| list.get(i).cloned())
        };
        match page {
            Page::Main => {
                if let Some(d) = pick(&cfg.main, main_slots(cfg.main.len())) {
                    return self.go_dest(&d, shift).await;
                }
                match slot {
                    46 => Ok(self.open_quick_menu(Page::Info).await?),
                    47 => {
                        self.close_window(false).await?;
                        self.message(&text::colored("— 길라잡이 —", "gold")).await?;
                        for l in &cfg.guide {
                            self.message(&text::colored(l, "yellow")).await?;
                        }
                        Ok(())
                    }
                    48 => self.error("우편함은 준비 중입니다.").await,
                    49 => Ok(self.open_ranks().await?),
                    50 => {
                        self.close_window(false).await?;
                        self.send_links(&cfg).await
                    }
                    51 => Ok(self.open_quick_menu(Page::Settings).await?),
                    52 => Ok(self.open_trash().await?),
                    _ => Ok(()),
                }
            }
            Page::Warps => match pick(&cfg.warps, WARP_SLOTS.to_vec()) {
                Some(d) => self.go_dest(&d, shift).await,
                None => Ok(()),
            },
            Page::Dungeons => match pick(&cfg.dungeons, dungeon_slots(cfg.dungeons.len())) {
                Some(d) => self.go_dest(&d, shift).await,
                None => Ok(()),
            },
            Page::Info => match slot {
                12 => Ok(self.open_quick_menu(Page::Jobs).await?),
                14 => Ok(self.open_quick_menu(Page::Adventurer).await?),
                16 => self.error("도감은 준비 중입니다.").await,
                _ => Ok(()),
            },
            Page::Adventurer if slot == 22 => Ok(self
                .open_equip(crate::plugins::equip::gui::Page::Gear)
                .await?),
            Page::Jobs | Page::Adventurer => Ok(()),
            Page::Settings => {
                match slot {
                    11 => {
                        self.hud.hidden = !self.hud.hidden;
                        self.hud_tick().await?;
                    }
                    13 => self.rank_command("갑옷숨기기", &[]).await?,
                    15 => self.rank_command("플라이", &[]).await?,
                    20..=25 => {
                        if let Some(c) = crate::plugins::chat::Channel::ALL.get(slot - 20) {
                            let c = *c;
                            crate::plugins::chat::update_prefs(self.profile.uuid, |p| {
                                if !p.muted.remove(&c) {
                                    p.muted.insert(c);
                                }
                            });
                        }
                    }
                    _ => {}
                }
                Ok(())
            }
        }
    }

    /// 메뉴에서 고른 곳으로.
    async fn go_dest(&mut self, d: &Dest, shift: bool) -> Result<(), FrameError> {
        let command = if shift && !d.shift_command.is_empty() {
            d.shift_command.clone()
        } else {
            d.command.clone()
        };
        match command.as_str() {
            "@warps" => return Ok(self.open_quick_menu(Page::Warps).await?),
            "@dungeons" => return Ok(self.open_quick_menu(Page::Dungeons).await?),
            "" => {}
            c => {
                self.close_window(false).await?;
                return Box::pin(self.command(c)).await;
            }
        }
        if !self.instance_exists(&d.instance) {
            return self
                .error(&format!("{}은(는) 준비 중입니다.", d.name))
                .await;
        }
        self.close_window(false).await?;
        if d.instance != self.instance_here() {
            return self.move_command(Some(&d.instance)).await;
        }
        let to = d
            .pos
            .unwrap_or_else(|| terrain::spawn_point(Dimension::Overworld, self.server.seed));
        if self.dimension != Dimension::Overworld {
            self.change_dimension_to(Dimension::Overworld, Some(to))
                .await?;
        } else {
            self.teleport_and_load(to).await?;
        }
        let msg = format!("{}(으)로 왔습니다.", d.name);
        Ok(self.message(&text::colored(&msg, "green")).await?)
    }

    async fn send_links(&mut self, cfg: &super::MenuConfig) -> Result<(), FrameError> {
        let links = [
            ("위키", &cfg.wiki),
            ("디스코드", &cfg.discord),
            ("마인리스트", &cfg.minelist),
        ];
        if links.iter().all(|(_, u)| u.is_empty()) {
            return self.error("아직 등록된 주소가 없습니다.").await;
        }
        for (name, url) in links {
            if url.is_empty() {
                continue;
            }
            let line = text::join(vec![
                text::colored(&format!("{name}  "), "gold"),
                text::open_url(text::colored(url, "aqua"), url),
            ]);
            self.message(&line).await?;
        }
        Ok(())
    }

    /// `/메뉴` `/내정보` `/직업` `/모험가` `/쓰레기통`.
    pub(crate) async fn menu_command(&mut self, name: &str) -> Result<(), FrameError> {
        let page = match name {
            "내정보" => Page::Info,
            "직업" => Page::Jobs,
            "모험가" => Page::Adventurer,
            "쓰레기통" => return Ok(self.open_trash().await?),
            _ => Page::Main,
        };
        Ok(self.open_quick_menu(page).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_fit() {
        assert_eq!(main_slots(5), vec![20, 21, 22, 23, 24]);
        assert_eq!(dungeon_slots(3), vec![11, 13, 15]);
        for p in [Page::Warps, Page::Info, Page::Settings] {
            assert!(p.back().unwrap().0 < p.size());
        }
        // 서브 기능은 메인 창 마지막 줄 안
        assert!(SUB.iter().all(|s| (45..54).contains(&s.0)));
    }
}
