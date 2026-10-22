//! `/ia`: ItemsAdder처럼 사용자 정의 아이템을 목록(분류)별로 보여 주고, 누르면 줍니다.
//!
//! 6줄 창: 위 5줄에 분류 또는 아이템, 아래 줄에 뒤로·이전·다음. 아이템 칸은 진짜 아이템을
//! 그대로 보여 주므로 리소스 팩 그림이 보입니다. 누르면 1개, Shift+클릭이면 한 묶음.

use tokio::io::AsyncWrite;

use super::play::Session;
use super::windows::{Window, WindowKind};
use crate::contents;
use crate::data::menu;
use crate::packets::container as pc;
use crate::packets::world::Item;
use crate::protocol::frame::FrameError;
use crate::text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Catalog {
    Categories { page: usize },
    Items { category: usize, page: usize },
}

const PER_PAGE: usize = 45;
const BACK: usize = 45;
const PREV: usize = 48;
const NEXT: usize = 50;

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    pub(super) async fn open_catalog(&mut self, page: Catalog) -> std::io::Result<()> {
        self.close_window(false).await?;
        self.next_window = self.next_window % 100 + 1;
        let id = self.next_window;
        let title = match &page {
            Catalog::Categories { .. } => "사용자 정의 아이템".to_owned(),
            Catalog::Items { category, .. } => contents::current()
                .categories
                .get(*category)
                .map_or("아이템".into(), |c| c.name.clone()),
        };
        self.window = Some(Window {
            id,
            kind: WindowKind::Catalog(page),
            grid: [None; 9],
        });
        self.send(
            pc::OPEN_SCREEN_ID,
            &pc::encode_open_screen(id, menu::GENERIC_9X6, &text::plain(&title)),
        )
        .await?;
        self.send_window().await
    }

    fn catalog_item(id: &str) -> Option<Item> {
        contents::item(id, 1).or_else(|| {
            let v = crate::data::items::by_name(id.strip_prefix("minecraft:").unwrap_or(id))?;
            (v > 0).then(|| Item::new(v, 1))
        })
    }

    pub(super) fn catalog_slots(&self, page: &Catalog) -> Vec<Option<Item>> {
        let loaded = contents::current();
        let (entries, page_no, back): (Vec<Option<Item>>, usize, bool) = match page {
            Catalog::Categories { page } => (
                loaded
                    .categories
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let icon = Self::catalog_item(&c.icon).unwrap_or(Item::new(1, 1));
                        // 분류 아이콘은 이름·설명을 바꿔 보여 줌
                        self.icon(
                            i % PER_PAGE,
                            crate::data::items::name(icon.id),
                            &c.name,
                            "white",
                            vec![format!("아이템 {}개", c.items.len())],
                            false,
                        )
                        .map(|mut it| {
                            if icon.custom.is_some() {
                                // 사용자 정의 아이콘: 그 아이템 그대로 (그림이 보이도록)
                                it = icon;
                            }
                            it
                        })
                    })
                    .collect(),
                *page,
                false,
            ),
            Catalog::Items { category, page } => (
                loaded
                    .categories
                    .get(*category)
                    .map(|c| c.items.iter().map(|id| Self::catalog_item(id)).collect())
                    .unwrap_or_default(),
                *page,
                true,
            ),
        };
        let pages = entries.len().div_ceil(PER_PAGE).max(1);
        let mut slots: Vec<Option<Item>> = entries
            .into_iter()
            .skip(page_no * PER_PAGE)
            .take(PER_PAGE)
            .collect();
        slots.resize(PER_PAGE, None);
        for s in PER_PAGE..54 {
            slots.push(match s {
                BACK if back => self.icon(s, "arrow", "분류로", "white", Vec::new(), false),
                PREV if page_no > 0 => {
                    self.icon(s, "spectral_arrow", "이전 쪽", "white", Vec::new(), false)
                }
                NEXT if page_no + 1 < pages => {
                    self.icon(s, "spectral_arrow", "다음 쪽", "white", Vec::new(), false)
                }
                49 => self.icon(
                    s,
                    "book",
                    &format!("{}/{} 쪽", page_no + 1, pages),
                    "gray",
                    vec!["누르면 1개, Shift+클릭이면 한 묶음".into()],
                    false,
                ),
                _ => self.pane(s),
            });
        }
        slots
    }

    pub(super) async fn catalog_click(
        &mut self,
        page: Catalog,
        slot: i16,
        mode: i32,
    ) -> Result<(), FrameError> {
        let Ok(slot) = usize::try_from(slot) else {
            return Ok(());
        };
        let loaded = contents::current();
        let set = |s: &mut Self, p: Catalog| {
            if let Some(w) = &mut s.window {
                w.kind = WindowKind::Catalog(p);
            }
        };
        match page {
            Catalog::Categories { page } => match slot {
                PREV if page > 0 => set(self, Catalog::Categories { page: page - 1 }),
                NEXT => set(self, Catalog::Categories { page: page + 1 }),
                s if s < PER_PAGE => {
                    let i = page * PER_PAGE + s;
                    if i < loaded.categories.len() {
                        set(
                            self,
                            Catalog::Items {
                                category: i,
                                page: 0,
                            },
                        );
                    }
                }
                _ => {}
            },
            Catalog::Items { category, page } => match slot {
                BACK => set(self, Catalog::Categories { page: 0 }),
                PREV if page > 0 => set(
                    self,
                    Catalog::Items {
                        category,
                        page: page - 1,
                    },
                ),
                NEXT => set(
                    self,
                    Catalog::Items {
                        category,
                        page: page + 1,
                    },
                ),
                s if s < PER_PAGE => {
                    let id = loaded
                        .categories
                        .get(category)
                        .and_then(|c| c.items.get(page * PER_PAGE + s))
                        .cloned();
                    if let Some(id) = id {
                        let stack = mode == 1; // Shift+클릭
                        self.ia_give(&id, if stack { 64 } else { 1 }).await?;
                    }
                }
                _ => {}
            },
        }
        Ok(())
    }

    /// `/iaget`과 목록에서: 사용자 정의 아이템을 줍니다.
    pub(super) async fn ia_give(&mut self, id: &str, count: i32) -> Result<(), FrameError> {
        let Some(item) = Self::catalog_item(id) else {
            return self.error(&format!("모르는 아이템: {id}")).await;
        };
        let max = crate::game::custom_items::max_stack(&item);
        let item = item.with_count(count.clamp(1, max.max(1)));
        let taken = self.give(item).await?;
        if taken < item.count {
            self.throw(item.with_count(item.count - taken));
        }
        Ok(())
    }

    pub(super) async fn ia_command(&mut self, name: &str, args: &[&str]) -> Result<(), FrameError> {
        match name {
            "ia" => Ok(self.open_catalog(Catalog::Categories { page: 0 }).await?),
            "iaget" => {
                let Some(id) = args.first() else {
                    return self.error("/iaget <이름공간:아이디> [개수]").await;
                };
                let count = args.get(1).and_then(|c| c.parse().ok()).unwrap_or(1);
                self.ia_give(id, count).await?;
                self.message(&text::colored(&format!("{id} ×{count}"), "green"))
                    .await?;
                Ok(())
            }
            _ => {
                let fonts = contents::current().fonts.clone();
                if fonts.is_empty() {
                    return self
                        .error("이모지가 없습니다 (contents의 font_images).")
                        .await;
                }
                for f in fonts {
                    let short = f.id.split_once(':').map_or(f.id.as_str(), |(_, n)| n);
                    let line = format!("{} :{short}:", f.ch);
                    self.message(&text::plain(&line)).await?;
                }
                Ok(())
            }
        }
    }
}
