//! 복권 창 (상자 모양의 가상 GUI): 판매소(3줄)와 번호 고르기(6줄).
//!
//! 번호 고르기는 기획 그림처럼 가운데 7×4칸에 종이 28장(1~28번). 누르면 고르고(반짝임), 다시
//! 누르면 뺍니다. 아래 줄: 고른 번호, 무작위, 구매, 다시 고르기, 뒤로. 판매소: 구매, 이번 회차
//! 정보, 당첨 복권 제출. 메뉴의 아이템은 집을 수 없고, 누른 칸만 버튼으로 씁니다.
//!
//! 아이콘의 이름·설명은 세션마다 다른 일련번호의 [`Look`]으로 그립니다.

use tokio::io::AsyncWrite;

use crate::dambi::child;
use crate::data::{items, menu};
use crate::game::custom_items::{self, Look};
use crate::packets::container as pc;
use crate::packets::world::Item;
use crate::plugins::lottery::{self, Claim, NUMBERS, PICK, numbers_text, won};
use crate::protocol::frame::FrameError;
use crate::session::play::Session;
use crate::session::windows::{Window, WindowKind};
use crate::text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    /// 판매소 (3줄).
    Hub,
    /// 번호 고르기 (6줄): 지금까지 고른 번호.
    Pick(Vec<u8>),
}

impl Page {
    pub fn size(&self) -> usize {
        match self {
            Page::Hub => 27,
            Page::Pick(_) => 54,
        }
    }
}

const HUB_BUY: usize = 11;
const HUB_INFO: usize = 13;
const HUB_SUBMIT: usize = 15;
const PICK_INFO: usize = 4;
const PICK_CHOSEN: usize = 45;
const PICK_RANDOM: usize = 47;
const PICK_BUY: usize = 49;
const PICK_RESET: usize = 51;
const PICK_BACK: usize = 53;

/// 6줄 창의 칸 → 번호 (가운데 7×4: 1~4줄, 1~7칸).
pub fn number_at(slot: usize) -> Option<u8> {
    let (row, col) = (slot / 9, slot % 9);
    ((1..=4).contains(&row) && (1..=7).contains(&col)).then(|| ((row - 1) * 7 + col) as u8)
}

#[cfg(test)]
fn slot_of(number: u8) -> usize {
    let n = number as usize - 1;
    (n / 7 + 1) * 9 + n % 7 + 1
}

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    /// 이 세션의 메뉴 아이콘 일련번호 (복권 번호와 겹치지 않게 높은 범위).
    pub(crate) fn icon_serial(&self, slot: usize) -> u32 {
        0x8000_0000 | ((self.entity_id as u32 & 0x7_FFFF) << 6) | slot as u32
    }

    pub(crate) fn icon(
        &self,
        slot: usize,
        item: &str,
        name: &str,
        color: &str,
        lore: Vec<String>,
        glint: bool,
    ) -> Option<Item> {
        let serial = self.icon_serial(slot);
        custom_items::set_look(
            serial,
            Look {
                rich: None,
                name: Some((name.to_owned(), color.to_owned())),
                lore,
                glint: Some(glint),
            },
        );
        Some(Item {
            id: items::by_name(item)?,
            count: 1,
            custom: custom_items::index_of(lottery::ICON_ITEM),
            serial,
        })
    }

    pub(crate) fn pane(&self, slot: usize) -> Option<Item> {
        self.icon(
            slot,
            "gray_stained_glass_pane",
            " ",
            "white",
            Vec::new(),
            false,
        )
    }

    pub(crate) fn lottery_slots(&self, page: &Page) -> Vec<Option<Item>> {
        let l = &self.server.lottery;
        let c = l.config();
        let now = lottery::now();
        let today = l.bought_today(self.profile.uuid, now);
        let info_lore = || {
            let mut v = vec![
                format!("추첨  {}", lottery::format_short(l.next_draw())),
                format!("판매  {}장", l.sales()),
                format!("1등 예상 당첨금  {}", won(l.jackpot())),
                format!(
                    "2등 {} · 3등 {} · 4등 {}",
                    won(c.base_prizes[1]),
                    won(c.base_prizes[2]),
                    won(c.base_prizes[3])
                ),
            ];
            if let Some(d) = l.last_draw() {
                v.push(format!(
                    "지난 제{}회  {}",
                    d.round,
                    numbers_text(&d.numbers)
                ));
            }
            v
        };
        match page {
            Page::Hub => (0..27)
                .map(|s| match s {
                    HUB_BUY => self.icon(
                        s,
                        "paper",
                        "복권 사기",
                        "gold",
                        vec![
                            "1~28 가운데 번호 5개를 골라 삽니다".into(),
                            format!("가격  {}", won(c.price)),
                            format!("오늘 산 복권  {today}/{}장", c.daily_limit),
                        ],
                        true,
                    ),
                    HUB_INFO => self.icon(
                        s,
                        "clock",
                        &format!("제{}회 복권", l.round()),
                        "yellow",
                        info_lore(),
                        false,
                    ),
                    HUB_SUBMIT => self.icon(
                        s,
                        "chest",
                        "당첨 복권 내기",
                        "green",
                        vec![
                            "가방의 복권을 모두 확인해".into(),
                            "추첨이 끝난 것은 당첨금을 받습니다".into(),
                        ],
                        false,
                    ),
                    _ => self.pane(s),
                })
                .collect(),
            Page::Pick(chosen) => (0..54)
                .map(|s| {
                    if let Some(n) = number_at(s) {
                        let on = chosen.contains(&n);
                        return self.icon(
                            s,
                            "paper",
                            &format!("{}{n}번", if on { "✔ " } else { "" }),
                            if on { "green" } else { "white" },
                            vec![
                                if on {
                                    "눌러서 빼기"
                                } else {
                                    "눌러서 고르기"
                                }
                                .into(),
                            ],
                            on,
                        );
                    }
                    let full = chosen.len() == PICK;
                    match s {
                        PICK_INFO => self.icon(
                            s,
                            "gold_ingot",
                            &format!("제{}회 · 1등 {}", l.round(), won(l.jackpot())),
                            "gold",
                            info_lore(),
                            false,
                        ),
                        PICK_CHOSEN => self.icon(
                            s,
                            "book",
                            &format!("고른 번호 {}/{PICK}", chosen.len()),
                            "aqua",
                            vec![if chosen.is_empty() {
                                "아직 없음".into()
                            } else {
                                numbers_text(chosen)
                            }],
                            false,
                        ),
                        PICK_RANDOM => self.icon(
                            s,
                            "ender_eye",
                            "나머지 무작위로",
                            "light_purple",
                            vec!["남은 번호를 아무거나 채웁니다".into()],
                            false,
                        ),
                        PICK_BUY => self.icon(
                            s,
                            "emerald",
                            &format!("사기 — {}", won(c.price)),
                            if full { "green" } else { "gray" },
                            vec![
                                if full {
                                    numbers_text(chosen)
                                } else {
                                    format!("번호 {PICK}개를 골라야 합니다")
                                },
                                format!("오늘 산 복권  {today}/{}장", c.daily_limit),
                            ],
                            full,
                        ),
                        PICK_RESET => {
                            self.icon(s, "barrier", "다시 고르기", "red", Vec::new(), false)
                        }
                        PICK_BACK => self.icon(s, "arrow", "뒤로", "white", Vec::new(), false),
                        _ => self.pane(s),
                    }
                })
                .collect(),
        }
    }

    pub(crate) fn lottery_closed(&self) {
        for s in 0..54 {
            custom_items::remove_look(self.icon_serial(s));
        }
    }

    /// 판매소를 엽니다 (NPC 우클릭, `/복권`).
    pub(crate) async fn open_lottery(&mut self, page: Page) -> std::io::Result<()> {
        self.close_window(false).await?;
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        let (menu_id, title) = match page {
            Page::Hub => (menu::GENERIC_9X3, "복권 판매소".to_owned()),
            Page::Pick(_) => (
                menu::GENERIC_9X6,
                format!("번호 {PICK}개 고르기 (1~{NUMBERS})"),
            ),
        };
        self.window = Some(Window {
            id,
            kind: WindowKind::Lottery(page),
            grid: [None; 9],
        });
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, menu_id, &text::plain(&title)),
        )
        .await?;
        self.send_window().await
    }

    fn set_page(&mut self, page: Page) {
        if let Some(w) = &mut self.window {
            w.kind = WindowKind::Lottery(page);
        }
    }

    pub(crate) async fn lottery_click(&mut self, page: Page, slot: i16) -> Result<(), FrameError> {
        let Ok(slot) = usize::try_from(slot) else {
            return Ok(());
        };
        match page {
            Page::Hub => match slot {
                HUB_BUY => self.open_lottery(Page::Pick(Vec::new())).await?,
                HUB_SUBMIT => self.submit_tickets().await?,
                _ => {}
            },
            Page::Pick(mut chosen) => {
                if let Some(n) = number_at(slot) {
                    if let Some(i) = chosen.iter().position(|&c| c == n) {
                        chosen.remove(i);
                    } else if chosen.len() < PICK {
                        chosen.push(n);
                        chosen.sort_unstable();
                    } else {
                        self.error(&format!("번호는 {PICK}개까지 고릅니다."))
                            .await?;
                    }
                    self.set_page(Page::Pick(chosen));
                    return Ok(());
                }
                match slot {
                    PICK_RANDOM => {
                        self.set_page(Page::Pick(lottery::random_numbers(&chosen).to_vec()))
                    }
                    PICK_RESET => self.set_page(Page::Pick(Vec::new())),
                    PICK_BACK => self.open_lottery(Page::Hub).await?,
                    PICK_BUY if chosen.len() == PICK => {
                        let numbers: [u8; PICK] = chosen.clone().try_into().unwrap();
                        if self.buy_ticket(numbers).await? {
                            self.set_page(Page::Pick(Vec::new()));
                        }
                    }
                    PICK_BUY => {
                        self.error(&format!("번호 {PICK}개를 먼저 골라 주세요."))
                            .await?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// 돈을 받고 복권을 줍니다. 샀으면 true.
    async fn buy_ticket(&mut self, numbers: [u8; PICK]) -> Result<bool, FrameError> {
        let l = &self.server.lottery;
        let now = lottery::now();
        let price = l.config().price;
        if let Err(e) = l.can_buy(self.profile.uuid, now) {
            self.error(&e).await?;
            return Ok(false);
        }
        match child::shared_add(self.profile.uuid, "money", -price).await {
            Some((true, _)) => {}
            Some((false, d)) => {
                self.error(&format!(
                    "돈이 모자랍니다 (가진 돈 {}, 가격 {}).",
                    won(d.money),
                    won(price)
                ))
                .await?;
                return Ok(false);
            }
            None => {
                self.error("재화 서버가 답하지 않습니다.").await?;
                return Ok(false);
            }
        }
        let ticket = self
            .server
            .lottery
            .buy(self.profile.uuid, &self.profile.name, numbers, now);
        match ticket {
            Ok(item) => {
                if self.give(item).await? == 0 {
                    self.throw(item);
                }
                let line = format!(
                    "복권을 샀습니다: {} (제{}회, 아이템 번호 #{:06})",
                    numbers_text(&numbers),
                    self.server.lottery.round(),
                    item.serial
                );
                self.message(&text::colored(&line, "gold")).await?;
                Ok(true)
            }
            Err(e) => {
                // 사이에 제한에 걸림: 돈을 돌려줌
                child::shared_add(self.profile.uuid, "money", price).await;
                self.error(&e).await?;
                Ok(false)
            }
        }
    }

    /// 가방의 복권을 모두 냅니다.
    pub(crate) async fn submit_tickets(&mut self) -> Result<(), FrameError> {
        let Some(ticket) = lottery::ticket_index() else {
            return Ok(());
        };
        let mut total = 0i64;
        let mut lines = Vec::new();
        let mut waiting = 0;
        for slot in 9..46 {
            let Some(item) = self.inventory[slot] else {
                continue;
            };
            if item.custom != Some(ticket) || item.serial == 0 {
                continue;
            }
            match self.server.lottery.claim(item.serial) {
                Claim::Done {
                    round,
                    rank,
                    amount,
                } => {
                    self.inventory[slot] = None;
                    self.sync_slot(slot).await?;
                    total += amount;
                    lines.push(match rank {
                        Some(r) => (
                            format!("#{:06} 제{round}회 — {r}등 {}", item.serial, won(amount)),
                            "green",
                        ),
                        None => (format!("#{:06} 제{round}회 — 낙첨", item.serial), "gray"),
                    });
                }
                Claim::AlreadyClaimed => {
                    self.inventory[slot] = None;
                    self.sync_slot(slot).await?;
                    lines.push((
                        format!("#{:06} — 이미 받은 복권이라 회수했습니다", item.serial),
                        "red",
                    ));
                }
                Claim::NotDrawn { .. } => waiting += 1,
                Claim::Unknown => {
                    lines.push((
                        format!("#{:06} — 이 채널의 복권이 아닙니다", item.serial),
                        "red",
                    ));
                }
            }
        }
        self.hands_changed();
        if lines.is_empty() && waiting == 0 {
            return self.error("낼 복권이 없습니다.").await;
        }
        for (l, color) in lines {
            self.message(&text::colored(&l, color)).await?;
        }
        if waiting > 0 {
            let at = lottery::format_short(self.server.lottery.next_draw());
            self.message(&text::colored(
                &format!("추첨 전인 복권 {waiting}장은 그대로 둡니다 (추첨 {at})."),
                "yellow",
            ))
            .await?;
        }
        if total > 0 {
            match child::shared_add(self.profile.uuid, "money", total).await {
                Some((true, d)) => {
                    let line = format!(
                        "당첨금 {}을 받았습니다 (가진 돈 {}).",
                        won(total),
                        won(d.money)
                    );
                    self.message(&text::colored(&line, "gold")).await?;
                }
                _ => {
                    self.error("당첨금을 넣지 못했습니다. 관리자에게 알려 주세요.")
                        .await?
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_grid_matches_the_plan() {
        // 기획 그림: 6줄 상자의 가운데 7×4
        assert_eq!(number_at(10), Some(1));
        assert_eq!(number_at(16), Some(7));
        assert_eq!(number_at(19), Some(8));
        assert_eq!(number_at(43), Some(28));
        assert_eq!(number_at(9), None);
        assert_eq!(number_at(17), None);
        assert_eq!(number_at(46), None);
        for n in 1..=28 {
            assert_eq!(number_at(slot_of(n)), Some(n));
        }
    }
}
