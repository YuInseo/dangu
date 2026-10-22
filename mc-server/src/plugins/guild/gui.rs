//! 길드 창 (`/길드`): 상자 모양의 메뉴들. 아이템은 집히지 않고 누른 칸만 버튼으로 씁니다
//! (창고만 진짜 상자처럼 넣고 뺌 — [`crate::session::windows`]의 `GuildStorage`).
//!
//! ```text
//! 메인 ─┬ 창고 (페이지) ── 기록
//!       ├ 땅 ── 좌클릭 이동 / Shift+좌클릭 판매 → 확인
//!       ├ 업그레이드 ── 신호기
//!       ├ 퀘스트 (일일·주간)      ├ 2배 이벤트
//!       ├ 설정 (설명·입장/퇴장 메시지·비공개·스폰)
//!       ├ 길드원 ── 한 사람 (강퇴·부길드장·위임 → 확인)
//!       ├ 가입 신청 (좌클릭 수락 / 우클릭 거절)   ├ 방문객   ├ 역할 관리
//!       └ 탈퇴 / 해산 → 확인
//! ```
//!
//! 아직 없는 기능(길드 상점)과 열 수 없는 것은 회색 방벽으로 보입니다 (기획: "출시 안 된 느낌").

use tokio::io::AsyncWrite;

use super::beacon::{self, Effect};
use super::quest::Quest;
use super::{Actor, Guild, GuildEvent, Perm, Role, Upgrade, guilds};
use crate::data::menu;
use crate::packets::container as pc;
use crate::packets::world::Item;
use crate::protocol::frame::FrameError;
use crate::session::play::Session;
use crate::session::windows::{Window, WindowKind};
use crate::text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Main,
    /// 길드가 없을 때: 공개 길드 목록 (누르면 가입 신청).
    Browse {
        page: usize,
    },
    Log {
        page: usize,
    },
    Lands {
        page: usize,
    },
    SellLand {
        index: usize,
    },
    Upgrades,
    Beacon,
    Quests,
    Events,
    Settings,
    Members {
        page: usize,
    },
    Member {
        uuid: u128,
    },
    Transfer {
        uuid: u128,
    },
    Roles {
        officer: bool,
    },
    Requests {
        page: usize,
    },
    Visitors {
        page: usize,
    },
    Leave,
    Disband,
}

impl Page {
    pub fn size(&self) -> usize {
        match self {
            Page::SellLand { .. }
            | Page::Transfer { .. }
            | Page::Leave
            | Page::Disband
            | Page::Member { .. } => 27,
            _ => 54,
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Page::Main => "길드",
            Page::Browse { .. } => "길드 목록",
            Page::Log { .. } => "길드 창고 기록",
            Page::Lands { .. } => "길드 땅",
            Page::SellLand { .. } => "땅을 팔까요?",
            Page::Upgrades => "길드 업그레이드",
            Page::Beacon => "길드 신호기",
            Page::Quests => "길드 퀘스트",
            Page::Events => "길드 2배 이벤트",
            Page::Settings => "길드 설정",
            Page::Members { .. } => "길드원 관리",
            Page::Member { .. } => "길드원",
            Page::Transfer { .. } => "길드장을 넘길까요?",
            Page::Roles { officer: true } => "역할 관리: 부길드장",
            Page::Roles { officer: false } => "역할 관리: 길드원",
            Page::Requests { .. } => "가입 신청",
            Page::Visitors { .. } => "방문객 관리",
            Page::Leave => "길드에서 나갈까요?",
            Page::Disband => "정말 해산할까요? (되돌릴 수 없음)",
        }
    }
}

// 6줄 창의 아래 줄
const BACK: usize = 45;
const PREV: usize = 48;
const MIDDLE: usize = 49;
const NEXT: usize = 50;
const PER_PAGE: usize = 45;
// 3줄 확인 창
const YES: usize = 11;
const NO: usize = 15;

// 메인
const M_INFO: usize = 4;
const M_STORAGE: usize = 19;
const M_LANDS: usize = 20;
const M_UPGRADES: usize = 21;
const M_BEACON: usize = 22;
const M_QUESTS: usize = 23;
const M_EVENTS: usize = 24;
const M_SETTINGS: usize = 25;
const M_MEMBERS: usize = 29;
const M_REQUESTS: usize = 30;
const M_VISITORS: usize = 31;
const M_LOG: usize = 32;
const M_ROLES: usize = 33;
const M_SHOP: usize = 34;
const M_SPAWN: usize = 48;
const M_LEAVE: usize = 50;

const UPGRADE_SLOTS: [usize; 4] = [19, 21, 23, 25];
const U_BEACON: usize = 40;
const BEACON_SLOTS: [usize; 9] = [10, 11, 12, 13, 14, 15, 16, 21, 23];
const DAILY_SLOTS: [usize; 3] = [11, 13, 15];
const WEEKLY_SLOTS: [usize; 2] = [30, 32];
const EVENT_SLOTS: [usize; 2] = [20, 24];
const S_DESC: usize = 19;
const S_ENTER: usize = 20;
const S_LEAVE: usize = 21;
const S_PRIVATE: usize = 23;
const S_SPAWN: usize = 25;
const ROLE_SLOTS: [usize; 13] = [10, 11, 12, 13, 14, 15, 16, 19, 20, 21, 22, 23, 24];
const R_SWITCH: usize = 49;
const MEMBER_KICK: usize = 11;
const MEMBER_OFFICER: usize = 13;
const MEMBER_LEADER: usize = 15;
const MEMBER_BACK: usize = 22;

fn bar(progress: u32, target: u32) -> String {
    let filled = (progress as usize * 10)
        .checked_div(target as usize)
        .unwrap_or(0);
    format!(
        "{}{} {progress}/{target}",
        "▮".repeat(filled.min(10)),
        "▯".repeat(10 - filled.min(10))
    )
}

fn when(t: i64) -> String {
    let kst = t + 9 * 3600;
    let (d, s) = (kst.div_euclid(86_400), kst.rem_euclid(86_400));
    // 날 번호 → 날짜 (그레고리력)
    let z = d + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{month}/{day} {:02}:{:02}", s / 3600, s / 60 % 60)
}

fn won(n: i64) -> String {
    format!("{}원", crate::plugins::hud::number(n))
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    fn me(&self) -> Actor {
        Actor {
            uuid: self.profile.uuid,
            name: self.profile.name.clone(),
        }
    }

    /// 아직 없는 것·열 수 없는 것: 회색 방벽.
    fn locked(&self, slot: usize, name: &str, lore: Vec<String>) -> Option<Item> {
        self.icon(slot, "barrier", name, "dark_gray", lore, false)
    }

    pub(crate) async fn open_guild(&mut self, page: Page) -> std::io::Result<()> {
        let page = match page {
            Page::Main if guilds().read(|d| d.of(self.profile.uuid).is_none()) => {
                Page::Browse { page: 0 }
            }
            p => p,
        };
        self.close_window(false).await?;
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        let menu_id = if page.size() == 27 {
            menu::GENERIC_9X3
        } else {
            menu::GENERIC_9X6
        };
        let title = page.title();
        self.window = Some(Window {
            id,
            kind: WindowKind::Guild(page),
            grid: [None; 9],
        });
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, menu_id, &text::plain(title)),
        )
        .await?;
        self.send_window().await
    }

    pub(crate) fn guild_slots(&self, page: &Page) -> Vec<Option<Item>> {
        let me = self.profile.uuid;
        let now = super::now();
        let (guild, browse) = guilds().read(|d| {
            let mut g = d.of(me).cloned();
            if let Some(g) = &mut g {
                g.quests.refresh(now);
            }
            let list: Vec<Guild> = d.guilds.iter().filter(|g| !g.private).cloned().collect();
            (g, list)
        });
        let mut slots: Vec<Option<Item>> = vec![None; page.size()];
        let paged = |slots: &mut Vec<Option<Item>>, page: usize, total: usize| {
            if page > 0 {
                slots[PREV] = self.icon(PREV, "arrow", "이전", "white", Vec::new(), false);
            }
            if (page + 1) * PER_PAGE < total {
                slots[NEXT] = self.icon(NEXT, "arrow", "다음", "white", Vec::new(), false);
            }
        };
        let back = |slots: &mut Vec<Option<Item>>| {
            slots[BACK] = self.icon(BACK, "dark_oak_door", "뒤로", "white", Vec::new(), false);
        };
        let confirm = |slots: &mut Vec<Option<Item>>, yes: &str, lore: Vec<String>| {
            slots[YES] = self.icon(YES, "lime_concrete", yes, "green", lore, false);
            slots[NO] = self.icon(NO, "red_concrete", "취소", "red", Vec::new(), false);
        };

        let Some(g) = guild else {
            // 길드가 없음: 목록
            let p = match page {
                Page::Browse { page } => *page,
                _ => 0,
            };
            for (i, g) in browse.iter().skip(p * PER_PAGE).take(PER_PAGE).enumerate() {
                slots[i] = self.icon(
                    i,
                    "white_banner",
                    &format!("{} (Lv.{})", g.name, g.level()),
                    "gold",
                    vec![
                        format!("길드장  {}", g.leader().name),
                        format!("인원  {}/{}", g.members.len(), g.max_members()),
                        format!("전투력  {}", crate::plugins::hud::number(g.power())),
                        g.description.clone(),
                        "누르면 가입 신청".into(),
                    ],
                    false,
                );
            }
            paged(&mut slots, p, browse.len());
            slots[MIDDLE] = self.icon(
                MIDDLE,
                "writable_book",
                "길드 만들기",
                "yellow",
                vec![
                    "/길드 만들기 <이름>".into(),
                    format!("값  {}", won(super::CREATE_COST)),
                ],
                false,
            );
            return slots;
        };

        let role = g.role_of(me).unwrap_or(Role::Member);
        let can = |p: Perm| g.can(me, p);
        let leader = role == Role::Leader;
        let gray_unless = |ok: bool, lore: &mut Vec<String>| {
            if !ok {
                lore.push("§8권한 없음".into());
            }
        };

        match page {
            Page::Main | Page::Browse { .. } => {
                let (cur, need) = super::level_progress(g.exp);
                slots[M_INFO] = self.icon(
                    M_INFO,
                    "white_banner",
                    &g.name,
                    "gold",
                    vec![
                        format!("레벨  {}  ({cur}/{need})", g.level()),
                        format!("길드장  {}", g.leader().name),
                        format!("인원  {}/{}", g.members.len(), g.max_members()),
                        format!("전투력  {}", crate::plugins::hud::number(g.power())),
                        format!("GP  {}", crate::plugins::hud::number(g.gp)),
                        format!("내 역할  {}", role.name()),
                        if g.description.is_empty() {
                            "(설명 없음)".into()
                        } else {
                            g.description.clone()
                        },
                    ],
                    true,
                );
                let pages = g.storage_pages();
                let entries: [(usize, &str, &str, Vec<String>); 12] = [
                    (
                        M_STORAGE,
                        "chest",
                        "길드 창고",
                        vec![format!("{pages}페이지")],
                    ),
                    (
                        M_LANDS,
                        "grass_block",
                        "길드 땅",
                        vec![format!("{}/{}청크", g.lands.len(), g.max_lands())],
                    ),
                    (M_UPGRADES, "anvil", "업그레이드", Vec::new()),
                    (
                        M_BEACON,
                        "beacon",
                        "신호기",
                        vec![format!("효과 {}개", g.beacon.len())],
                    ),
                    (M_QUESTS, "writable_book", "퀘스트 (일일·주간)", Vec::new()),
                    (
                        M_EVENTS,
                        "firework_rocket",
                        "2배 이벤트",
                        GuildEvent::ALL
                            .iter()
                            .filter(|e| g.event_active(**e, now))
                            .map(|e| format!("진행 중: {}", e.name()))
                            .collect(),
                    ),
                    (M_SETTINGS, "comparator", "설정", Vec::new()),
                    (
                        M_MEMBERS,
                        "player_head",
                        "길드원 관리",
                        vec![format!("{}명", g.members.len())],
                    ),
                    (
                        M_REQUESTS,
                        "paper",
                        "가입 신청",
                        vec![format!("{}건", g.requests.len())],
                    ),
                    (
                        M_VISITORS,
                        "oak_door",
                        "방문객 관리",
                        vec![format!("{}명", g.visitors.len())],
                    ),
                    (M_LOG, "book", "창고 기록", Vec::new()),
                    (M_ROLES, "name_tag", "역할 관리", vec!["길드장만".into()]),
                ];
                for (slot, item, name, lore) in entries {
                    slots[slot] = self.icon(slot, item, name, "yellow", lore, false);
                }
                slots[M_SHOP] = self.locked(
                    M_SHOP,
                    "길드 상점 (준비 중)",
                    vec!["GP·공헌도로 사는 상점 — 곧 열립니다".into()],
                );
                slots[M_SPAWN] = match &g.spawn {
                    Some(s) => self.icon(
                        M_SPAWN,
                        "ender_pearl",
                        "길드 스폰으로",
                        "aqua",
                        vec![format!(
                            "{} {} ({:.0}, {:.0}, {:.0})",
                            s.instance,
                            s.dimension.key(),
                            s.pos.0,
                            s.pos.1,
                            s.pos.2
                        )],
                        false,
                    ),
                    None => self.locked(M_SPAWN, "길드 스폰 없음", vec!["설정에서 지정".into()]),
                };
                slots[M_LEAVE] = if leader {
                    self.icon(
                        M_LEAVE,
                        "tnt",
                        "길드 해산",
                        "red",
                        vec!["물어본 뒤 해산".into()],
                        false,
                    )
                } else {
                    self.icon(M_LEAVE, "iron_door", "길드 탈퇴", "red", Vec::new(), false)
                };
            }
            Page::Log { page } => {
                if !can(Perm::StorageLog) {
                    slots[22] = self.locked(22, "권한 없음", vec![Perm::StorageLog.name().into()]);
                } else {
                    let log: Vec<_> = g.log.iter().rev().collect();
                    for (i, e) in log.iter().skip(page * PER_PAGE).take(PER_PAGE).enumerate() {
                        let (item, color, verb) = if e.delta > 0 {
                            ("lime_dye", "green", "넣음")
                        } else {
                            ("red_dye", "red", "뺌")
                        };
                        slots[i] = self.icon(
                            i,
                            item,
                            &format!("{} {verb}: {} ×{}", e.who, e.item, e.delta.abs()),
                            color,
                            vec![when(e.at), format!("{}페이지", e.page + 1)],
                            false,
                        );
                    }
                    paged(&mut slots, *page, log.len());
                }
                back(&mut slots);
            }
            Page::Lands { page } => {
                for (i, l) in g
                    .lands
                    .iter()
                    .enumerate()
                    .skip(page * PER_PAGE)
                    .take(PER_PAGE)
                {
                    let s = i - page * PER_PAGE;
                    let (x, z) = l.center();
                    let mut lore = vec![
                        format!("좌표  {x}, {z}"),
                        format!("{} · {}", l.instance, l.dimension.key()),
                        format!("산 날  {}", when(l.bought)),
                        "좌클릭: 이동".into(),
                    ];
                    if can(Perm::Land) {
                        lore.push(format!("Shift+좌클릭: 판매 ({} 돌려받음)", won(l.paid / 2)));
                    }
                    slots[s] = self.icon(
                        s,
                        "grass_block",
                        &format!("청크 ({}, {})", l.chunk.0, l.chunk.1),
                        "green",
                        lore,
                        false,
                    );
                }
                paged(&mut slots, *page, g.lands.len());
                let mut lore = vec![
                    format!("값  {}", won(super::land::PRICE)),
                    format!("{}/{}청크", g.lands.len(), g.max_lands()),
                    "처음이 아니면 길드 땅에 붙어 있어야 함".into(),
                ];
                gray_unless(can(Perm::Land), &mut lore);
                slots[MIDDLE] = self.icon(
                    MIDDLE,
                    "oak_sapling",
                    "지금 선 청크 사기",
                    "yellow",
                    lore,
                    false,
                );
                back(&mut slots);
            }
            Page::SellLand { index } => {
                let lore = g
                    .lands
                    .get(*index)
                    .map(|l| {
                        vec![
                            format!("청크 ({}, {})", l.chunk.0, l.chunk.1),
                            format!("돌려받음  {}", won(l.paid / 2)),
                        ]
                    })
                    .unwrap_or_default();
                confirm(&mut slots, "판매", lore);
            }
            Page::Upgrades => {
                for (u, slot) in Upgrade::ALL.into_iter().zip(UPGRADE_SLOTS) {
                    let level = g.upgrade(u);
                    let mut lore = vec![format!("지금  {}단계 — {}", level, u.effect(level))];
                    if level >= u.max_level() {
                        lore.push("끝 단계".into());
                        slots[slot] = self.icon(slot, "nether_star", u.name(), "gold", lore, true);
                        continue;
                    }
                    let (money, need) = u.cost(level);
                    lore.push(format!("다음  {}", u.effect(level + 1)));
                    lore.push(format!("값  {}", won(money)));
                    lore.push(format!("길드 레벨 {need} 필요"));
                    if g.level() < need {
                        slots[slot] = self.locked(slot, &format!("{} (잠김)", u.name()), lore);
                        continue;
                    }
                    gray_unless(can(Perm::Upgrade), &mut lore);
                    lore.push("누르면 올리기 (내 돈으로)".into());
                    let item = match u {
                        Upgrade::Land => "grass_block",
                        Upgrade::Members => "player_head",
                        Upgrade::Storage => "chest",
                        Upgrade::Growth => "wheat",
                    };
                    slots[slot] = self.icon(slot, item, u.name(), "yellow", lore, false);
                }
                slots[U_BEACON] = self.icon(
                    U_BEACON,
                    "beacon",
                    "신호기 시스템",
                    "aqua",
                    vec!["범위 제한 없음 · 무한 지속".into()],
                    true,
                );
                back(&mut slots);
            }
            Page::Beacon => {
                let next_cost = beacon::unlock_cost(g.beacon.len());
                for (e, slot) in Effect::ALL.into_iter().zip(BEACON_SLOTS) {
                    match g.beacon.iter().find(|o| o.effect == e) {
                        Some(o) => {
                            let mut lore = vec![format!(
                                "{} {}단계 (최대 {})",
                                e.name(),
                                o.tier,
                                e.max_tier()
                            )];
                            if o.tier < e.max_tier() {
                                let (money, ores) = e.tier_cost(o.tier);
                                lore.push(format!("올리기: {}", won(money)));
                                for (n, c) in ores {
                                    lore.push(format!("  {} ×{c}", label(n)));
                                }
                                gray_unless(can(Perm::Upgrade), &mut lore);
                                lore.push("누르면 단계 올리기".into());
                            }
                            slots[slot] = self.icon(
                                slot,
                                e.icon(),
                                &format!("{} {}", e.name(), roman(o.tier)),
                                "aqua",
                                lore,
                                true,
                            );
                        }
                        None => {
                            let mut lore = vec![format!("열기: 신호기 {next_cost}개")];
                            gray_unless(can(Perm::Upgrade), &mut lore);
                            lore.push("누르면 열기".into());
                            slots[slot] = self.locked(slot, &format!("{} (잠김)", e.name()), lore);
                        }
                    }
                }
                slots[MIDDLE] = self.icon(
                    MIDDLE,
                    "beacon",
                    "길드 신호기",
                    "aqua",
                    vec![
                        "접속한 길드원 모두에게, 거리 상관없이 늘".into(),
                        format!("다음 효과 열기: 신호기 {next_cost}개"),
                        "단계 올리기: 광물 + 돈".into(),
                    ],
                    true,
                );
                back(&mut slots);
            }
            Page::Quests => {
                let show = |q: &Quest, slot: usize, label: &str| {
                    let lore = vec![
                        bar(q.progress, q.target),
                        format!("보상  GP {}", q.reward),
                        if q.done {
                            "완료!".into()
                        } else {
                            "길드원 모두가 함께 채움".into()
                        },
                    ];
                    let item = if q.done { "lime_dye" } else { q.kind.icon() };
                    self.icon(
                        slot,
                        item,
                        &format!("{label}: {}", q.kind.name()),
                        if q.done { "green" } else { "yellow" },
                        lore,
                        q.done,
                    )
                };
                for (q, slot) in g.quests.daily.iter().zip(DAILY_SLOTS) {
                    slots[slot] = show(q, slot, "일일");
                }
                for (q, slot) in g.quests.weekly.iter().zip(WEEKLY_SLOTS) {
                    slots[slot] = show(q, slot, "주간");
                }
                slots[4] = self.icon(
                    4,
                    "clock",
                    "퀘스트",
                    "white",
                    vec![
                        "일일: 매일 0시 (한국 시각)".into(),
                        "주간: 월요일 0시".into(),
                    ],
                    false,
                );
                back(&mut slots);
            }
            Page::Events => {
                for (e, slot) in GuildEvent::ALL.into_iter().zip(EVENT_SLOTS) {
                    let i = GuildEvent::ALL.iter().position(|x| *x == e).unwrap();
                    let active = g.event_active(e, now);
                    let mut lore = if active {
                        vec![format!("진행 중 — {}까지", when(g.events[i]))]
                    } else {
                        vec![
                            format!("1시간, GP {}", GuildEvent::COST_GP),
                            format!("지금 GP  {}", g.gp),
                        ]
                    };
                    lore.push(match e {
                        GuildEvent::JobXp => "캐기·거두기·나무 베기로 얻는 직업 경험치 2배".into(),
                        GuildEvent::RareDrop => "광석을 캘 때 덤이 나올 확률 2배".into(),
                    });
                    gray_unless(can(Perm::Events), &mut lore);
                    let item = match e {
                        GuildEvent::JobXp => "experience_bottle",
                        GuildEvent::RareDrop => "diamond",
                    };
                    slots[slot] = self.icon(slot, item, e.name(), "light_purple", lore, active);
                }
                back(&mut slots);
            }
            Page::Settings => {
                let text_item = |slot: usize, name: &str, value: &str, cmd: &str, perm: Perm| {
                    let mut lore = vec![
                        if value.is_empty() {
                            "(없음)".into()
                        } else {
                            value.to_owned()
                        },
                        format!("바꾸기: {cmd}"),
                    ];
                    gray_unless(g.can(me, perm), &mut lore);
                    self.icon(slot, "writable_book", name, "yellow", lore, false)
                };
                slots[S_DESC] = text_item(
                    S_DESC,
                    "길드 설명",
                    &g.description,
                    "/길드 설명 <글>",
                    Perm::Description,
                );
                slots[S_ENTER] = text_item(
                    S_ENTER,
                    "입장 메시지",
                    &g.enter_message,
                    "/길드 입장메시지 <글>",
                    Perm::EnterMessage,
                );
                slots[S_LEAVE] = text_item(
                    S_LEAVE,
                    "퇴장 메시지",
                    &g.leave_message,
                    "/길드 퇴장메시지 <글>",
                    Perm::LeaveMessage,
                );
                let mut lore = vec![
                    if g.private {
                        "지금: 비공개".into()
                    } else {
                        "지금: 공개".into()
                    },
                    "비공개면 목록에 안 나오고 가입 신청을 받지 않음".into(),
                ];
                gray_unless(can(Perm::Privacy), &mut lore);
                slots[S_PRIVATE] = self.icon(
                    S_PRIVATE,
                    if g.private { "gray_dye" } else { "lime_dye" },
                    "비공개 여부",
                    "yellow",
                    lore,
                    false,
                );
                let mut lore = vec!["누르면 지금 선 자리를 길드 스폰으로".into()];
                gray_unless(can(Perm::Spawn), &mut lore);
                slots[S_SPAWN] = self.icon(S_SPAWN, "compass", "스폰 지정", "yellow", lore, false);
                back(&mut slots);
            }
            Page::Members { page } => {
                let online: Vec<u128> = self
                    .server
                    .players_anywhere()
                    .iter()
                    .map(|p| p.uuid)
                    .collect();
                let mut list = g.members.clone();
                list.sort_by_key(|m| (std::cmp::Reverse(m.role), m.name.to_lowercase()));
                for (i, m) in list.iter().skip(page * PER_PAGE).take(PER_PAGE).enumerate() {
                    let color = match m.role {
                        Role::Leader => "gold",
                        Role::Officer => "aqua",
                        Role::Member => "white",
                    };
                    slots[i] = self.icon(
                        i,
                        "player_head",
                        &m.name,
                        color,
                        vec![
                            m.role.name().into(),
                            format!("공헌도  {}", m.contribution),
                            format!("전투력  {}", m.power),
                            format!("가입  {}", when(m.joined)),
                            if online.contains(&m.uuid) {
                                "접속 중".into()
                            } else {
                                "접속 안 함".into()
                            },
                            "누르면 관리".into(),
                        ],
                        false,
                    );
                }
                paged(&mut slots, *page, list.len());
                back(&mut slots);
            }
            Page::Member { uuid } => {
                let Some(m) = g.member(*uuid) else {
                    return slots;
                };
                slots[4] = self.icon(
                    4,
                    "player_head",
                    &m.name,
                    "white",
                    vec![m.role.name().into()],
                    false,
                );
                let mut lore = vec!["길드에서 내보냄".into()];
                gray_unless(can(Perm::Kick), &mut lore);
                slots[MEMBER_KICK] = self.icon(MEMBER_KICK, "tnt", "강퇴", "red", lore, false);
                let officer = m.role == Role::Officer;
                let mut lore = vec![format!("부길드장은 {}명까지", super::MAX_OFFICERS)];
                gray_unless(leader, &mut lore);
                slots[MEMBER_OFFICER] = self.icon(
                    MEMBER_OFFICER,
                    "golden_helmet",
                    if officer {
                        "부길드장 해제"
                    } else {
                        "부길드장 임명"
                    },
                    "aqua",
                    lore,
                    false,
                );
                let mut lore = vec!["물어본 뒤 넘김".into()];
                gray_unless(leader, &mut lore);
                slots[MEMBER_LEADER] = self.icon(
                    MEMBER_LEADER,
                    "golden_apple",
                    "길드장 위임",
                    "gold",
                    lore,
                    false,
                );
                slots[MEMBER_BACK] = self.icon(
                    MEMBER_BACK,
                    "dark_oak_door",
                    "뒤로",
                    "white",
                    Vec::new(),
                    false,
                );
            }
            Page::Transfer { uuid } => {
                let name = g.member(*uuid).map_or(String::new(), |m| m.name.clone());
                confirm(
                    &mut slots,
                    &format!("{name}님에게 넘기기"),
                    vec!["나는 부길드장(자리가 없으면 길드원)이 됩니다".into()],
                );
            }
            Page::Roles { officer } => {
                let role = if *officer {
                    Role::Officer
                } else {
                    Role::Member
                };
                let bits = g.perms_of(role);
                for (p, slot) in Perm::ALL.into_iter().zip(ROLE_SLOTS) {
                    let on = bits & p.bit() != 0;
                    let mut lore = vec![if on { "허용".into() } else { "막음".into() }];
                    gray_unless(leader, &mut lore);
                    slots[slot] = self.icon(
                        slot,
                        if on { "lime_dye" } else { "gray_dye" },
                        p.name(),
                        if on { "green" } else { "gray" },
                        lore,
                        false,
                    );
                }
                slots[R_SWITCH] = self.icon(
                    R_SWITCH,
                    "name_tag",
                    if *officer {
                        "길드원 권한 보기"
                    } else {
                        "부길드장 권한 보기"
                    },
                    "yellow",
                    vec!["길드장 위임·부길드장 임명·역할 관리·해산은 길드장만".into()],
                    false,
                );
                back(&mut slots);
            }
            Page::Requests { page } => {
                for (i, r) in g
                    .requests
                    .iter()
                    .skip(page * PER_PAGE)
                    .take(PER_PAGE)
                    .enumerate()
                {
                    let mut lore = vec![when(r.at), "좌클릭: 수락 · 우클릭: 거절".into()];
                    gray_unless(can(Perm::AcceptRequests), &mut lore);
                    slots[i] = self.icon(i, "paper", &r.name, "yellow", lore, false);
                }
                paged(&mut slots, *page, g.requests.len());
                back(&mut slots);
            }
            Page::Visitors { page } => {
                for (i, (_, name)) in g
                    .visitors
                    .iter()
                    .skip(page * PER_PAGE)
                    .take(PER_PAGE)
                    .enumerate()
                {
                    let mut lore = vec!["길드 땅에서 짓기·상자 가능".into(), "누르면 빼기".into()];
                    gray_unless(can(Perm::Visitors), &mut lore);
                    slots[i] = self.icon(i, "player_head", name, "white", lore, false);
                }
                paged(&mut slots, *page, g.visitors.len());
                slots[MIDDLE] = self.icon(
                    MIDDLE,
                    "oak_sign",
                    "방문객 넣기",
                    "yellow",
                    vec!["/길드 방문객 추가 <이름>".into()],
                    false,
                );
                back(&mut slots);
            }
            Page::Leave => confirm(&mut slots, "탈퇴", vec![g.name.clone()]),
            Page::Disband => confirm(
                &mut slots,
                "해산",
                vec![
                    format!("{} 길드와 창고·땅·신호기가 모두 사라집니다", g.name),
                    "되돌릴 수 없습니다".into(),
                ],
            ),
        }
        slots
    }

    fn set_guild_page(&mut self, page: Page) {
        if let Some(w) = &mut self.window {
            w.kind = WindowKind::Guild(page);
        }
    }

    /// 결과를 채팅으로 알립니다.
    async fn report(&mut self, r: super::Outcome) -> std::io::Result<()> {
        match r {
            Ok(m) => self.message(&text::colored(&m, "green")).await,
            Err(e) => self.message(&text::colored(&e, "red")).await,
        }
    }

    pub(crate) async fn guild_click(
        &mut self,
        page: Page,
        slot: i16,
        button: i8,
        mode: i32,
    ) -> Result<(), FrameError> {
        let Ok(slot) = usize::try_from(slot) else {
            return Ok(());
        };
        let me = self.me();
        let now = super::now();
        let shift = mode == 1;
        let right = button == 1;
        let page_of = |p: &Page| match p {
            Page::Browse { page }
            | Page::Log { page }
            | Page::Lands { page }
            | Page::Members { page }
            | Page::Requests { page }
            | Page::Visitors { page } => *page,
            _ => 0,
        };
        let with_page = |p: &Page, n: usize| match p {
            Page::Browse { .. } => Page::Browse { page: n },
            Page::Log { .. } => Page::Log { page: n },
            Page::Lands { .. } => Page::Lands { page: n },
            Page::Members { .. } => Page::Members { page: n },
            Page::Requests { .. } => Page::Requests { page: n },
            Page::Visitors { .. } => Page::Visitors { page: n },
            p => p.clone(),
        };
        if page.size() == 54 && !matches!(page, Page::Main) {
            match slot {
                BACK => {
                    return Ok(self
                        .open_guild(match page {
                            Page::Beacon => Page::Upgrades,
                            _ => Page::Main,
                        })
                        .await?);
                }
                PREV if page_of(&page) > 0 => {
                    self.set_guild_page(with_page(&page, page_of(&page) - 1));
                    return Ok(());
                }
                NEXT => {
                    self.set_guild_page(with_page(&page, page_of(&page) + 1));
                    return Ok(());
                }
                _ => {}
            }
        }
        let index = page_of(&page) * PER_PAGE + slot;
        match page {
            Page::Browse { .. } => {
                if slot < PER_PAGE {
                    let name = guilds().read(|d| {
                        d.guilds
                            .iter()
                            .filter(|g| !g.private)
                            .nth(index)
                            .map(|g| g.name.clone())
                    });
                    if let Some(name) = name {
                        let r = guilds().write(|d| d.request_join(&me, &name, now));
                        self.notify_request(&name, r.is_ok());
                        self.report(r).await?;
                    }
                }
            }
            Page::Main => match slot {
                M_STORAGE => self.open_guild_storage(0).await?,
                M_LANDS => self.open_guild(Page::Lands { page: 0 }).await?,
                M_UPGRADES => self.open_guild(Page::Upgrades).await?,
                M_BEACON => self.open_guild(Page::Beacon).await?,
                M_QUESTS => self.open_guild(Page::Quests).await?,
                M_EVENTS => self.open_guild(Page::Events).await?,
                M_SETTINGS => self.open_guild(Page::Settings).await?,
                M_MEMBERS => self.open_guild(Page::Members { page: 0 }).await?,
                M_REQUESTS => self.open_guild(Page::Requests { page: 0 }).await?,
                M_VISITORS => self.open_guild(Page::Visitors { page: 0 }).await?,
                M_LOG => self.open_guild(Page::Log { page: 0 }).await?,
                M_ROLES => self.open_guild(Page::Roles { officer: true }).await?,
                M_SPAWN => {
                    self.close_window(false).await?;
                    self.guild_spawn().await?;
                }
                M_LEAVE => {
                    let leader = guilds().read(|d| d.of(me.uuid).and_then(|g| g.role_of(me.uuid)))
                        == Some(Role::Leader);
                    self.open_guild(if leader { Page::Disband } else { Page::Leave })
                        .await?;
                }
                _ => {}
            },
            Page::Lands { .. } => {
                if slot == MIDDLE {
                    self.close_window(false).await?;
                    return self.claim_here().await;
                }
                if slot < PER_PAGE {
                    let land =
                        guilds().read(|d| d.of(me.uuid).and_then(|g| g.lands.get(index).cloned()));
                    let Some(land) = land else { return Ok(()) };
                    if shift && !right {
                        self.open_guild(Page::SellLand { index }).await?;
                    } else if !shift && !right {
                        self.close_window(false).await?;
                        self.go_to_land(&land).await?;
                    }
                }
            }
            Page::SellLand { index } => match slot {
                YES => {
                    let r = guilds().write(|d| d.sell(&me, index));
                    match r {
                        Ok((msg, refund)) => {
                            crate::dambi::child::shared_add(me.uuid, "money", refund).await;
                            self.report(Ok(msg)).await?;
                        }
                        Err(e) => self.report(Err(e)).await?,
                    }
                    self.open_guild(Page::Lands { page: 0 }).await?;
                }
                NO => self.open_guild(Page::Lands { page: 0 }).await?,
                _ => {}
            },
            Page::Upgrades => {
                if slot == U_BEACON {
                    return Ok(self.open_guild(Page::Beacon).await?);
                }
                if let Some(i) = UPGRADE_SLOTS.iter().position(|s| *s == slot) {
                    self.buy_upgrade(Upgrade::ALL[i]).await?;
                }
            }
            Page::Beacon => {
                if let Some(i) = BEACON_SLOTS.iter().position(|s| *s == slot) {
                    self.beacon_action(Effect::ALL[i]).await?;
                }
            }
            Page::Events => {
                if let Some(i) = EVENT_SLOTS.iter().position(|s| *s == slot) {
                    let r = guilds().write(|d| d.start_event(&me, GuildEvent::ALL[i], now));
                    if let Ok(m) = &r {
                        self.tell_guild(&format!("[길드] {}님이 켬: {m}", me.name));
                    }
                    self.report(r).await?;
                }
            }
            Page::Settings => match slot {
                S_PRIVATE => {
                    let r = guilds().write(|d| d.toggle_private(&me));
                    self.report(r).await?;
                }
                S_SPAWN => {
                    let spot = super::Spot {
                        instance: self.server.name.clone(),
                        dimension: self.dimension,
                        pos: self.position,
                    };
                    let r = guilds().write(|d| d.set_spawn(&me, spot));
                    self.report(r).await?;
                }
                S_DESC | S_ENTER | S_LEAVE => {
                    let cmd = match slot {
                        S_DESC => "/길드 설명 ",
                        S_ENTER => "/길드 입장메시지 ",
                        _ => "/길드 퇴장메시지 ",
                    };
                    self.close_window(false).await?;
                    let hint = text::interactive(
                        text::colored(&format!("{cmd}<글> 로 바꿉니다 (눌러서 입력)"), "yellow"),
                        None,
                        Some(cmd),
                    );
                    self.message(&hint).await?;
                }
                _ => {}
            },
            Page::Members { .. } => {
                if slot < PER_PAGE {
                    let uuid = guilds().read(|d| {
                        d.of(me.uuid).and_then(|g| {
                            let mut list = g.members.clone();
                            list.sort_by_key(|m| {
                                (std::cmp::Reverse(m.role), m.name.to_lowercase())
                            });
                            list.get(index).map(|m| m.uuid)
                        })
                    });
                    if let Some(uuid) = uuid {
                        self.open_guild(Page::Member { uuid }).await?;
                    }
                }
            }
            Page::Member { uuid } => {
                let name = guilds().read(|d| {
                    d.of(me.uuid)
                        .and_then(|g| g.member(uuid))
                        .map(|m| (m.name.clone(), m.role))
                });
                let Some((name, role)) = name else {
                    return Ok(self.open_guild(Page::Members { page: 0 }).await?);
                };
                match slot {
                    MEMBER_KICK => {
                        let r = guilds().write(|d| d.kick(&me, &name));
                        if r.is_ok() {
                            self.tell_player(
                                uuid,
                                &format!("[길드] {}님이 길드에서 내보냈습니다.", me.name),
                            );
                        }
                        self.report(r).await?;
                        self.open_guild(Page::Members { page: 0 }).await?;
                    }
                    MEMBER_OFFICER => {
                        let r =
                            guilds().write(|d| d.set_officer(&me, &name, role != Role::Officer));
                        self.report(r).await?;
                    }
                    MEMBER_LEADER => self.open_guild(Page::Transfer { uuid }).await?,
                    MEMBER_BACK => self.open_guild(Page::Members { page: 0 }).await?,
                    _ => {}
                }
            }
            Page::Transfer { uuid } => match slot {
                YES => {
                    let name = guilds().read(|d| {
                        d.of(me.uuid)
                            .and_then(|g| g.member(uuid))
                            .map(|m| m.name.clone())
                    });
                    if let Some(name) = name {
                        let r = guilds().write(|d| d.transfer_leader(&me, &name));
                        if r.is_ok() {
                            self.tell_guild(&format!(
                                "[길드] {}님이 {name}님에게 길드장을 넘겼습니다.",
                                me.name
                            ));
                        }
                        self.report(r).await?;
                    }
                    self.open_guild(Page::Main).await?;
                }
                NO => self.open_guild(Page::Member { uuid }).await?,
                _ => {}
            },
            Page::Roles { officer } => {
                if slot == R_SWITCH {
                    self.set_guild_page(Page::Roles { officer: !officer });
                } else if let Some(i) = ROLE_SLOTS.iter().position(|s| *s == slot) {
                    let role = if officer { Role::Officer } else { Role::Member };
                    let r = guilds().write(|d| d.toggle_role_perm(&me, role, Perm::ALL[i]));
                    if r.is_err() {
                        self.report(r).await?;
                    }
                }
            }
            Page::Requests { .. } => {
                if slot < PER_PAGE {
                    let who = guilds().read(|d| {
                        d.of(me.uuid)
                            .and_then(|g| g.requests.get(index).map(|r| (r.uuid, r.name.clone())))
                    });
                    if let Some((uuid, name)) = who {
                        let r = guilds().write(|d| d.answer_request(&me, uuid, !right, now));
                        if r.is_ok() {
                            let gname = guilds()
                                .read(|d| d.of(me.uuid).map(|g| g.name.clone()))
                                .unwrap_or_default();
                            if !right {
                                self.tell_guild(&format!("[길드] {name}님이 길드에 들어왔습니다!"));
                                self.tell_player(
                                    uuid,
                                    &format!("[길드] {gname} 길드 가입이 수락되었습니다!"),
                                );
                            } else {
                                self.tell_player(
                                    uuid,
                                    &format!("[길드] {gname} 길드 가입 신청이 거절되었습니다."),
                                );
                            }
                        }
                        self.report(r).await?;
                    }
                }
            }
            Page::Visitors { .. } => {
                if slot < PER_PAGE {
                    let who = guilds()
                        .read(|d| d.of(me.uuid).and_then(|g| g.visitors.get(index).cloned()));
                    if let Some(v) = who {
                        let r = guilds().write(|d| d.visitor(&me, v, false));
                        self.report(r).await?;
                    }
                }
            }
            Page::Leave => match slot {
                YES => {
                    // 나가기 전에 알릴 사람들을 정해 둡니다
                    let others: Vec<u128> = guilds().read(|d| {
                        d.of(me.uuid)
                            .map(|g| {
                                g.members
                                    .iter()
                                    .map(|m| m.uuid)
                                    .filter(|u| *u != me.uuid)
                                    .collect()
                            })
                            .unwrap_or_default()
                    });
                    let r = guilds().write(|d| d.leave(&me));
                    if r.is_ok() {
                        super::notify(
                            self.server,
                            others,
                            format!("[길드] {}님이 길드를 떠났습니다.", me.name),
                        );
                    }
                    self.close_window(false).await?;
                    self.report(r).await?;
                }
                NO => self.open_guild(Page::Main).await?,
                _ => {}
            },
            Page::Disband => match slot {
                YES => {
                    let members: Vec<u128> = guilds().read(|d| {
                        d.of(me.uuid)
                            .map(|g| g.members.iter().map(|m| m.uuid).collect())
                            .unwrap_or_default()
                    });
                    let r = guilds().write(|d| d.disband(&me));
                    if let Ok(m) = &r {
                        for u in members {
                            if u != me.uuid {
                                self.tell_player(u, &format!("[길드] {m}"));
                            }
                        }
                    }
                    self.close_window(false).await?;
                    self.report(r).await?;
                }
                NO => self.open_guild(Page::Main).await?,
                _ => {}
            },
            Page::Quests | Page::Log { .. } => {}
        }
        Ok(())
    }

    /// 업그레이드: 내 돈을 먼저 받고, 안 되면 돌려줍니다.
    pub(crate) async fn buy_upgrade(&mut self, u: Upgrade) -> std::io::Result<()> {
        let me = self.me();
        let level = guilds().read(|d| d.of(me.uuid).map(|g| g.upgrade(u)));
        let Some(level) = level else { return Ok(()) };
        if level >= u.max_level() {
            return self.report(Err("이미 끝까지 올렸습니다.".into())).await;
        }
        let (money, _) = u.cost(level);
        if !self.pay(money).await {
            return Ok(());
        }
        let r = guilds().write(|d| d.upgrade(&me, u));
        if r.is_err() {
            crate::dambi::child::shared_add(me.uuid, "money", money).await;
        } else if let Ok(m) = &r {
            self.tell_guild(&format!("[길드] {}님이 업그레이드: {m}", me.name));
        }
        self.report(r).await
    }

    /// 돈 내기. 모자라면 알리고 false.
    pub(crate) async fn pay(&mut self, money: i64) -> bool {
        if money <= 0 {
            return true;
        }
        match crate::dambi::child::shared_add(self.profile.uuid, "money", -money).await {
            Some((true, _)) => true,
            Some((false, d)) => {
                let _ = self
                    .report(Err(format!(
                        "돈이 모자랍니다 ({} / {})",
                        won(d.money),
                        won(money)
                    )))
                    .await;
                false
            }
            None => {
                let _ = self
                    .report(Err("재화 정보를 받지 못했습니다.".into()))
                    .await;
                false
            }
        }
    }

    /// 신호기: 없으면 열고(신호기 n개), 있으면 단계 올리기(광물 + 돈).
    async fn beacon_action(&mut self, e: Effect) -> std::io::Result<()> {
        let me = self.me();
        let owned = guilds().read(|d| {
            d.of(me.uuid)
                .is_some_and(|g| g.beacon.iter().any(|o| o.effect == e))
        });
        let unlock = !owned;
        let plan = guilds().read(|d| d.beacon_plan(me.uuid, e, unlock));
        let cost = match plan {
            Ok(c) => c,
            Err(err) => return self.report(Err(err)).await,
        };
        let mut wants: Vec<(&str, i32)> = cost.items.clone();
        if cost.beacons > 0 {
            wants.push(("beacon", cost.beacons));
        }
        let mut inv = self.inventory.to_vec();
        let Some(changed) = beacon::take_from(&mut inv, 9, &wants) else {
            let need: Vec<String> = wants
                .iter()
                .map(|(n, c)| {
                    format!(
                        "{} {}/{c}",
                        label(n),
                        beacon::count_in(&self.inventory[9..], n)
                    )
                })
                .collect();
            return self
                .report(Err(format!("재료가 모자랍니다: {}", need.join(", "))))
                .await;
        };
        if !self.pay(cost.money).await {
            return Ok(());
        }
        let r = guilds().write(|d| d.beacon_apply(me.uuid, e, unlock));
        match &r {
            Ok(m) => {
                self.inventory.copy_from_slice(&inv);
                for s in changed {
                    self.sync_slot(s).await?;
                }
                self.tell_guild(&format!("[길드] {}님: {m}", me.name));
            }
            Err(_) => {
                crate::dambi::child::shared_add(me.uuid, "money", cost.money).await;
            }
        }
        self.report(r).await
    }
}

/// 재료 이름 (창에 보일 한국어).
fn label(item: &str) -> &str {
    match item {
        "iron_ingot" => "철 주괴",
        "gold_ingot" => "금 주괴",
        "diamond" => "다이아몬드",
        "emerald" => "에메랄드",
        "beacon" => "신호기",
        other => other,
    }
}

fn roman(n: u8) -> &'static str {
    match n {
        1 => "I",
        2 => "II",
        3 => "III",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(bar(5, 10), "▮▮▮▮▮▯▯▯▯▯ 5/10");
        assert_eq!(bar(0, 0), "▯▯▯▯▯▯▯▯▯▯ 0/0");
        // 2026-10-22 14:00 KST
        assert_eq!(when(1_792_645_200), "10/22 14:00");
        for p in [Page::Main, Page::Leave, Page::Roles { officer: true }] {
            assert!(p.size() == 27 || p.size() == 54);
        }
        // 버튼 칸이 겹치지 않음
        let mut main = vec![
            M_INFO, M_STORAGE, M_LANDS, M_UPGRADES, M_BEACON, M_QUESTS, M_EVENTS, M_SETTINGS,
            M_MEMBERS, M_REQUESTS, M_VISITORS, M_LOG, M_ROLES, M_SHOP, M_SPAWN, M_LEAVE,
        ];
        main.sort_unstable();
        main.dedup();
        assert_eq!(main.len(), 16);
    }
}
