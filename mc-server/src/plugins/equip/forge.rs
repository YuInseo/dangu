//! 장비 손질 창 (`/강화`): 강화 · 연마 · 조율 · 전승 · 분해.
//!
//! 바닐라 상자 창을 [`forge_art`](super::forge_art) 그림과 [`crate::plugins::gui`] 제목 글로 덮어
//! 그립니다. 버튼은 그 자리의 (빈) 상자 칸이고, 장비는 아래 가방이나 왼쪽 "착용" 칸을 눌러 고릅니다.
//! 무엇이든 바뀌면 창을 같은 자리에 다시 엽니다 (제목은 열 때만 보낼 수 있어서).
//!
//! | 탭 | 하는 일 |
//! |---|---|
//! | 강화 | +30까지. 실패하면 **장인의 기운**이 (그 단계 확률 × `enhance.artisan`)만큼 쌓이고, 100%면 다음은 꼭 성공 |
//! | 연마 | 장신구에 주문서를 발라 잠재능력을 새로 (남은 횟수 1 소모) |
//! | 조율 | 잠재능력 줄은 그대로, 수치만 다시 (마력석 + 돈) |
//! | 전승 | 강화 단계를 같은 부위의 다른 장비로 옮김 (보낸 쪽은 +0) |
//! | 분해 | 장비를 부숴 만든 재료·강화석의 일부를 돌려받음 (두 번 눌러 확인) |

use tokio::io::AsyncWrite;

use super::config::{self, EquipConfig, GearDef};
use super::forge_art::{self as fa, FONT};
use super::gui::{Page, vanilla_name};
use super::session::{gear_of, item_key, item_of};
use super::store::{self, Gear, gears};
use super::{MAX_ENHANCE, Part, enhance_multiplier};
use crate::packets::world::Item;
use crate::plugins::gui::{self, Canvas, fit};
use crate::plugins::hud::art::Pal;
use crate::plugins::hud::number;
use crate::protocol::frame::FrameError;
use crate::protocol::nbt::Tag;
use crate::server::Event;
use crate::session::play::Session;
use crate::text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Enhance,
    Polish,
    Tune,
    Inherit,
    Salvage,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Enhance,
        Tab::Polish,
        Tab::Tune,
        Tab::Inherit,
        Tab::Salvage,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tab::Enhance => "강화",
            Tab::Polish => "연마",
            Tab::Tune => "조율",
            Tab::Inherit => "전승",
            Tab::Salvage => "분해",
        }
    }

    pub fn parse(s: &str) -> Option<Tab> {
        Tab::ALL.into_iter().find(|t| t.name() == s)
    }

    /// 장신구(잠재능력)를 다루는 탭인가.
    fn accessories(self) -> bool {
        matches!(self, Tab::Polish | Tab::Tune)
    }

    /// 이 탭에서 고를 수 있는 장비인가.
    fn accepts(self, part: Part) -> bool {
        match self {
            Tab::Enhance | Tab::Inherit => !part.is_accessory(),
            Tab::Polish | Tab::Tune => part.is_accessory(),
            Tab::Salvage => true,
        }
    }
}

/// 창의 상태 (고른 것들).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forge {
    pub tab: Tab,
    /// 손질할 장비 (전승이면 받는 쪽)
    pub target: Option<u32>,
    /// 전승: 보내는 장비
    pub source: Option<u32>,
    /// 연마: 고른 주문서
    pub scroll: Option<String>,
    /// 분해: 한 번 눌렀음 (한 번 더 누르면 부숨)
    pub confirm: bool,
}

impl Forge {
    pub fn new(tab: Tab) -> Self {
        Self {
            tab,
            target: None,
            source: None,
            scroll: None,
            confirm: false,
        }
    }
}

/// 장비가 있는 곳.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spot {
    /// 인벤토리 칸 (5..9 갑옷, 9..45 가방·핫바, 45 왼손)
    Inv(usize),
    /// 낀 장신구 (반지·목걸이·귀걸이)
    Worn(usize),
}

/// 고른 장비 하나의 모든 것.
struct Picked {
    spot: Spot,
    item: Item,
    def: GearDef,
    gear: Gear,
}

/// 등급 → 팔레트 색.
fn grade_pal(grade: &str) -> Pal {
    match grade {
        "일반" => Pal::Light,
        "고급" => Pal::Green,
        "희귀" => Pal::Blue,
        "영웅" | "상급" => Pal::Purple,
        "전설" => Pal::Orange,
        "신화" => Pal::Red,
        _ => Pal::White,
    }
}

fn percent(p: f64) -> String {
    let v = p * 100.0;
    if (v - v.round()).abs() < 0.05 {
        format!("{v:.0}%")
    } else {
        format!("{v:.1}%")
    }
}

/// 능력치 수치 (소수 한 자리까지).
fn num(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// 창 아래 가방 칸(54..90) → 인벤토리 칸.
fn inventory_slot(slot: usize) -> Option<usize> {
    match slot {
        54..=80 => Some(slot - 54 + 9),
        81..=89 => Some(slot - 81 + 36),
        _ => None,
    }
}

/// 판 안쪽 글 폭.
const SIDE_TEXT: i32 = 68;
/// 판의 글 줄 (윗변 y).
const LINE: i32 = 11;

impl<W: AsyncWrite + Unpin> Session<'_, W> {
    // ---- 찾기 ----

    fn locate_gear(&self, serial: u32) -> Option<(Spot, Item)> {
        if let Some(i) = (5..46).find(|&i| self.inventory[i].is_some_and(|it| it.serial == serial))
        {
            return Some((Spot::Inv(i), self.inventory[i]?));
        }
        let worn = gears().read(|d| d.worn(self.profile.uuid));
        let i = worn.iter().position(|w| *w == Some(serial))?;
        let def = gears().read(|d| d.gears.get(&serial).map(|g| g.def.clone()))?;
        let mut it = item_of(&def, 1)?;
        it.serial = serial;
        Some((Spot::Worn(i), it))
    }

    fn picked(&self, serial: Option<u32>) -> Option<Picked> {
        let (spot, item) = self.locate_gear(serial?)?;
        let (def, gear) = config::with(|c| gear_of(&item, c))?;
        Some(Picked {
            spot,
            item,
            def,
            gear: gear?,
        })
    }

    /// 왼쪽 "착용" 칸들: 장신구 탭이면 낀 장신구 셋, 아니면 입은 갑옷 넷.
    fn worn_cells(&self, tab: Tab) -> Vec<Option<Item>> {
        if tab.accessories() {
            let worn = gears().read(|d| d.worn(self.profile.uuid));
            worn.iter()
                .map(|s| {
                    let serial = (*s)?;
                    let def = gears().read(|d| d.gears.get(&serial).map(|g| g.def.clone()))?;
                    let mut it = item_of(&def, 1)?;
                    it.serial = serial;
                    Some(it)
                })
                .collect()
        } else {
            (5..9).map(|i| self.inventory[i]).collect()
        }
    }

    // ---- 칸 ----

    pub(crate) fn forge_slots(&self, f: &Forge) -> Vec<Option<Item>> {
        let mut slots: Vec<Option<Item>> = vec![None; 54];
        for (i, it) in self.worn_cells(f.tab).into_iter().enumerate() {
            slots[fa::WORN[i]] = it;
        }
        match f.tab {
            Tab::Inherit => {
                slots[fa::SOURCE] = self.picked(f.source).map(|p| p.item);
                slots[fa::TARGET] = self.picked(f.target).map(|p| p.item);
            }
            _ => slots[fa::CENTER] = self.picked(f.target).map(|p| p.item),
        }
        if f.tab == Tab::Polish
            && let Some(key) = &f.scroll
        {
            let have = self.count_key(key);
            slots[fa::SCROLL] = item_of(key, have.clamp(1, 64));
        }
        slots[fa::BACK] = self.icon(fa::BACK, "arrow", "장비 창으로", "white", Vec::new(), false);
        slots[fa::SMITH] = self.icon(
            fa::SMITH,
            "smithing_table",
            "대장간",
            "gold",
            vec!["제련 · 제작 · 상점".into()],
            false,
        );
        slots
    }

    // ---- 제목 (그림 + 글) ----

    pub(crate) async fn forge_title(&self, f: &Forge) -> Tag {
        let money = crate::dambi::child::shared(self.profile.uuid)
            .await
            .map_or(0, |d| d.money);
        // 설정은 복사해 씀: 그리는 동안 장비를 찾느라 설정을 다시 읽음 (잠금을 겹치지 않게)
        let cfg = config::get();
        self.forge_canvas(f, &cfg, money)
    }

    fn forge_canvas(&self, f: &Forge, cfg: &EquipConfig, money: i64) -> Tag {
        let mut c = Canvas::new();
        c.image(FONT, fa::glyph(fa::BG), fa::LEFT, 0);
        // 제목 줄
        let end = c.text(fa::LEFT + 8, 5, "장비 손질", Pal::Gold);
        c.text(end + 6, 5, f.tab.name(), Pal::Cream);
        c.text_right(
            fa::RIGHT - 8,
            5,
            &format!("골드 {}", number(money)),
            Pal::Yellow,
        );
        // 탭
        for (k, tab) in Tab::ALL.into_iter().enumerate() {
            let x = fa::tab_x(k);
            let on = tab == f.tab;
            if on {
                c.image(FONT, fa::glyph(fa::TAB_ON), x, 18);
            }
            c.text_center(
                x + fa::TAB_W / 2 + 1,
                22,
                tab.name(),
                if on { Pal::Cream } else { Pal::Gray },
            );
        }
        // 칸 테두리: 착용 칸 · 탭마다의 칸
        let worn = if f.tab.accessories() { 3 } else { 4 };
        for &s in &fa::WORN[..worn] {
            let (x, y) = gui::cell(s);
            c.image(FONT, fa::glyph(fa::SLOT), x - 1, y - 1);
        }
        match f.tab {
            Tab::Polish => {
                let (x, y) = gui::cell(fa::SCROLL);
                c.image(FONT, fa::glyph(fa::SLOT), x - 1, y - 1);
                c.text_center(x + 8, y - 10, "주문서", Pal::Light);
            }
            Tab::Inherit => {
                for (s, label) in [(fa::SOURCE, "보냄"), (fa::TARGET, "받음")] {
                    let (x, y) = gui::cell(s);
                    c.image(FONT, fa::glyph(fa::SLOT), x - 1, y - 1);
                    c.text_center(x + 8, y - 10, label, Pal::Light);
                }
                c.image(
                    FONT,
                    fa::glyph(fa::ARROW),
                    fa::DIAMOND.0 - 5,
                    fa::DIAMOND.1 - 4,
                );
            }
            _ => {}
        }
        // 판 머리줄
        let (l0, l1) = fa::SIDE_L;
        let (r0, r1) = fa::SIDE_R;
        let lc = (l0 + l1) / 2;
        let rc = (r0 + r1) / 2;
        let target = self.picked(f.target);
        let source = self.picked(f.source);
        let left_title = if f.tab == Tab::Inherit {
            "보내는 장비"
        } else {
            "장비 정보"
        };
        c.text_center(lc, 21, left_title, Pal::Cream);
        let shown = if f.tab == Tab::Inherit {
            source.as_ref()
        } else {
            target.as_ref()
        };
        self.info_panel(&mut c, cfg, shown, l0 + 4);
        // 탭마다: 가운데, 오른쪽 판, 버튼
        let ready = match f.tab {
            Tab::Enhance => self.enhance_view(&mut c, cfg, target.as_ref(), money),
            Tab::Polish => self.polish_view(&mut c, cfg, f, target.as_ref()),
            Tab::Tune => self.tune_view(&mut c, cfg, target.as_ref(), money),
            Tab::Inherit => self.inherit_view(&mut c, cfg, source.as_ref(), target.as_ref(), money),
            Tab::Salvage => self.salvage_view(&mut c, cfg, target.as_ref()),
        };
        let _ = (r1, rc);
        let (bx, by, _, _) = fa::BUTTON_RECT;
        if f.tab == Tab::Salvage && f.confirm {
            c.image(FONT, fa::glyph(fa::BUTTON_WARN), bx, by);
            c.text_center(88, 112, "정말 분해?", Pal::Cream);
        } else {
            if !ready {
                c.image(FONT, fa::glyph(fa::BUTTON_OFF), bx, by);
            }
            c.text_center(
                88,
                112,
                &format!("{}하기", f.tab.name()),
                if ready { Pal::Cream } else { Pal::Gray },
            );
        }
        // 아래 판: 도움말 · 가진 재료
        c.text_center(lc, 133, "도움말", Pal::Cream);
        let help: &[&str] = match f.tab {
            Tab::Enhance => &[
                "가방의 장비를",
                "누르면 골라요",
                "실패해도 단계",
                "는 그대로예요",
                "기운 100%면",
                "꼭 성공해요",
            ],
            Tab::Polish => &[
                "장신구와",
                "주문서를 골라",
                "잠재능력을",
                "새로 발라요",
                "(횟수 1 사용)",
            ],
            Tab::Tune => &["잠재능력 줄은", "그대로 두고", "수치만 다시", "굴려요"],
            Tab::Inherit => &[
                "먼저 고른 것이",
                "보내는 장비,",
                "다음이 받는",
                "장비예요",
                "보낸 쪽은 +0",
            ],
            Tab::Salvage => &[
                "장비를 부숴",
                "재료 일부를",
                "돌려받아요",
                "두 번 눌러야",
                "부서져요",
            ],
        };
        for (i, h) in help.iter().enumerate() {
            c.text(l0 + 4, 146 + LINE * i as i32, h, Pal::Light);
        }
        c.text_center(rc, 133, "가진 재료", Pal::Cream);
        let mut keys: Vec<String> = vec![cfg.enhance.stone.clone(), cfg.forge.tune_item.clone()];
        keys.extend(cfg.scrolls.iter().map(|s| s.item.clone()));
        keys.dedup();
        for (i, k) in keys.iter().take(7).enumerate() {
            // 주문서는 이름이 길어서 등급만
            let name = match cfg.scroll(k) {
                Some(s) => format!("{} 주문서", s.name),
                None => cfg.item_name(k).unwrap_or_else(|| vanilla_name(k)),
            };
            let y = 146 + LINE * i as i32;
            c.text(r0 + 4, y, &fit(&name, 44), Pal::Light);
            c.text_right(r1 - 4, y, &number(self.count_key(k) as i64), Pal::White);
        }
        c.finish()
    }

    /// 왼쪽 판: 이름 · 단계 · 부위 · 주요 능력치.
    fn info_panel(&self, c: &mut Canvas, cfg: &EquipConfig, p: Option<&Picked>, x: i32) {
        let Some(p) = p else {
            c.text(x, 34, "고른 장비 없음", Pal::Gray);
            return;
        };
        c.text(x, 34, &fit(&p.def.name, SIDE_TEXT), grade_pal(&p.def.grade));
        let mut line = format!("+{}  {}", p.gear.enhance, p.def.grade);
        if p.def.part.is_accessory() {
            line = format!("{}  {}", p.def.part.name(), p.def.grade);
        }
        c.text(x, 45, &line, Pal::Gold);
        let stats = super::gear_stats(&p.def, Some(&p.gear), cfg);
        let mut y = 58;
        for (k, v) in stats.entries().into_iter().take(6) {
            c.text(x, y, k.name(), Pal::Gray);
            c.text_right(x + SIDE_TEXT, y, &num(v), Pal::White);
            y += LINE;
        }
        if matches!(p.spot, Spot::Worn(_)) || matches!(p.spot, Spot::Inv(5..=8)) {
            c.text(x, 115, "착용 중", Pal::Mint);
        }
    }

    /// 오른쪽 판의 머리줄과 줄들을 쓰는 곳.
    fn right(&self) -> (i32, i32, i32) {
        let (r0, r1) = fa::SIDE_R;
        (r0 + 4, r1 - 4, (r0 + r1) / 2)
    }

    fn enhance_view(
        &self,
        c: &mut Canvas,
        cfg: &EquipConfig,
        t: Option<&Picked>,
        money: i64,
    ) -> bool {
        let (x0, x1, xc) = self.right();
        c.text_center(xc, 21, "예상 능력치", Pal::Cream);
        let Some(t) = t else {
            c.text_center(88, 92, "가방의 장비를 고르세요", Pal::Gray);
            return false;
        };
        let level = t.gear.enhance;
        if level >= MAX_ENHANCE {
            c.text_center(88, 74, "MAX", Pal::Gold);
            c.text(x0, 34, "최대 강화 +30", Pal::Gold);
            return false;
        }
        let chance = if t.gear.artisan >= 1.0 - 1e-9 {
            1.0
        } else {
            cfg.enhance_chance(level)
        };
        c.text_center(88, 74, &percent(chance), Pal::Yellow);
        // 장인의 기운
        let (bx0, by0, bx1, _) = fa::BAR;
        c.text(bx0, 91, "장인의 기운", Pal::Gold);
        c.text_right(bx1, 91, &percent(t.gear.artisan.min(1.0)), Pal::Cream);
        let inner = bx1 - bx0 - 2;
        let filled = (inner as f64 * t.gear.artisan.clamp(0.0, 1.0)).round() as i32;
        c.fill(FONT, fa::bar_pieces(), bx0 + 1, bx0 + 1 + filled, by0 + 1);
        // 예상 능력치
        c.text(x0, 34, &format!("+{level} > +{}", level + 1), Pal::Gold);
        let now = enhance_multiplier(level, cfg.enhance.per_level, cfg.enhance.milestone);
        let next = enhance_multiplier(level + 1, cfg.enhance.per_level, cfg.enhance.milestone);
        let mut y = 45;
        for (k, v) in t
            .def
            .stats
            .entries()
            .into_iter()
            .filter(|(k, _)| k.is_main())
            .take(3)
        {
            c.text(x0, y, k.name(), Pal::Gray);
            c.text_right(
                x1,
                y,
                &format!("{}>{}", num(v * now), num(v * next)),
                Pal::Green,
            );
            y += LINE;
        }
        // 필요 재료
        let (cost, stones) = cfg.enhance_cost(level);
        let have = self.count_key(&cfg.enhance.stone);
        c.text(x0, 80, "필요 재료", Pal::Cream);
        let stone_name = cfg
            .item_name(&cfg.enhance.stone)
            .unwrap_or_else(|| "강화석".into());
        c.text(x0, 91, &fit(&stone_name, 40), Pal::Light);
        c.text_right(
            x1,
            91,
            &format!("{have}/{stones}"),
            if have >= stones { Pal::Green } else { Pal::Red },
        );
        c.text(x0, 102, "골드", Pal::Light);
        c.text_right(
            x1,
            102,
            &number(cost),
            if money >= cost { Pal::Green } else { Pal::Red },
        );
        c.text(
            x0,
            113,
            &format!("실패 시 +{}", percent(store::artisan_gain(cfg, level))),
            Pal::Gray,
        );
        have >= stones && money >= cost
    }

    fn potential_lines(&self, c: &mut Canvas, g: &Gear, x0: i32, y: i32) -> i32 {
        let mut y = y;
        if g.potential.is_empty() {
            c.text(x0, y, "잠재능력 없음", Pal::Gray);
            return y + LINE;
        }
        for (k, v) in &g.potential {
            c.text(x0, y, &k.show(*v), Pal::Purple);
            y += LINE;
        }
        y
    }

    fn polish_view(
        &self,
        c: &mut Canvas,
        cfg: &EquipConfig,
        f: &Forge,
        t: Option<&Picked>,
    ) -> bool {
        let (x0, x1, xc) = self.right();
        c.text_center(xc, 21, "잠재능력", Pal::Cream);
        let Some(t) = t else {
            c.text_center(88, 92, "장신구를 고르세요", Pal::Gray);
            return false;
        };
        let y = self.potential_lines(c, &t.gear, x0, 34);
        c.text(x0, y.max(67), "남은 횟수", Pal::Light);
        c.text_right(
            x1,
            y.max(67),
            &format!("{}/{}", t.gear.tries_left, t.def.potential_tries),
            if t.gear.tries_left > 0 {
                Pal::Green
            } else {
                Pal::Red
            },
        );
        let scroll = f
            .scroll
            .as_ref()
            .and_then(|k| cfg.scroll(k).cloned().map(|s| (k.clone(), s)));
        c.text(x0, 80, "주문서", Pal::Cream);
        let Some((key, s)) = scroll else {
            c.text(x0, 91, "가방에서 고르세요", Pal::Gray);
            c.text_center(88, 92, "주문서를 고르세요", Pal::Gray);
            return false;
        };
        let have = self.count_key(&key);
        c.text(x0, 91, &format!("{} 주문서", s.name), Pal::Light);
        c.text_right(x1, 91, &format!("{have}개"), Pal::White);
        c.text(
            x0,
            102,
            &format!("{}~{}줄  x{}", s.min_lines, s.max_lines, num(s.mult)),
            Pal::Gray,
        );
        c.text_center(88, 92, "새 잠재능력을 발라요", Pal::Light);
        have > 0 && t.gear.tries_left > 0
    }

    fn tune_view(&self, c: &mut Canvas, cfg: &EquipConfig, t: Option<&Picked>, money: i64) -> bool {
        let (x0, x1, xc) = self.right();
        c.text_center(xc, 21, "잠재능력", Pal::Cream);
        let Some(t) = t else {
            c.text_center(88, 92, "장신구를 고르세요", Pal::Gray);
            return false;
        };
        self.potential_lines(c, &t.gear, x0, 34);
        let f = &cfg.forge;
        let have = self.count_key(&f.tune_item);
        let name = cfg
            .item_name(&f.tune_item)
            .unwrap_or_else(|| f.tune_item.clone());
        c.text(x0, 80, "필요 재료", Pal::Cream);
        c.text(x0, 91, &fit(&name, 40), Pal::Light);
        c.text_right(
            x1,
            91,
            &format!("{have}/{}", f.tune_count),
            if have >= f.tune_count {
                Pal::Green
            } else {
                Pal::Red
            },
        );
        c.text(x0, 102, "골드", Pal::Light);
        c.text_right(
            x1,
            102,
            &number(f.tune_money),
            if money >= f.tune_money {
                Pal::Green
            } else {
                Pal::Red
            },
        );
        if t.gear.potential.is_empty() {
            c.text_center(88, 92, "먼저 연마하세요", Pal::Red);
            return false;
        }
        c.text_center(88, 92, "수치만 다시 굴려요", Pal::Light);
        have >= f.tune_count && money >= f.tune_money
    }

    fn inherit_view(
        &self,
        c: &mut Canvas,
        cfg: &EquipConfig,
        s: Option<&Picked>,
        t: Option<&Picked>,
        money: i64,
    ) -> bool {
        let (x0, x1, xc) = self.right();
        c.text_center(xc, 21, "받는 장비", Pal::Cream);
        let Some(s) = s else {
            c.text_center(88, 92, "보낼 장비를 고르세요", Pal::Gray);
            c.text(x0, 34, "고른 장비 없음", Pal::Gray);
            return false;
        };
        let Some(t) = t else {
            c.text_center(88, 92, "받을 장비를 고르세요", Pal::Gray);
            c.text(x0, 34, "고른 장비 없음", Pal::Gray);
            return false;
        };
        c.text(
            x0,
            34,
            &fit(&t.def.name, SIDE_TEXT),
            grade_pal(&t.def.grade),
        );
        c.text(
            x0,
            45,
            &format!("+{} > +{}", t.gear.enhance, s.gear.enhance),
            Pal::Gold,
        );
        let cost = cfg.forge.inherit_money * (s.gear.enhance as i64).pow(2);
        c.text(x0, 80, "필요", Pal::Cream);
        c.text(x0, 91, "골드", Pal::Light);
        c.text_right(
            x1,
            91,
            &number(cost),
            if money >= cost { Pal::Green } else { Pal::Red },
        );
        c.text(x0, 102, "보낸 쪽 > +0", Pal::Gray);
        let ok = s.def.part == t.def.part && s.gear.enhance > t.gear.enhance;
        if s.def.part != t.def.part {
            c.text_center(88, 92, "같은 부위끼리만 돼요", Pal::Red);
        } else if s.gear.enhance <= t.gear.enhance {
            c.text_center(88, 92, "받는 쪽 단계가 더 높아요", Pal::Red);
        } else {
            c.text_center(
                88,
                92,
                &format!("+{} 을 옮겨요", s.gear.enhance),
                Pal::Light,
            );
        }
        ok && money >= cost
    }

    fn salvage_view(&self, c: &mut Canvas, cfg: &EquipConfig, t: Option<&Picked>) -> bool {
        let (x0, x1, xc) = self.right();
        c.text_center(xc, 21, "돌려받는 것", Pal::Cream);
        let Some(t) = t else {
            c.text_center(88, 92, "부술 장비를 고르세요", Pal::Gray);
            return false;
        };
        let back = store::salvage(cfg, &t.def, t.gear.enhance);
        for (i, (k, n)) in back.iter().take(7).enumerate() {
            let name = cfg.item_name(k).unwrap_or_else(|| vanilla_name(k));
            let y = 34 + LINE * i as i32;
            c.text(x0, y, &fit(&name, 48), Pal::Light);
            c.text_right(x1, y, &format!("x{n}"), Pal::White);
        }
        if !matches!(t.spot, Spot::Inv(9..=45)) {
            c.text_center(88, 92, "착용 중엔 못 부숴요", Pal::Red);
            return false;
        }
        c.text_center(88, 92, "부수면 되돌릴 수 없어요", Pal::Red);
        true
    }

    // ---- 누름 ----

    /// 장비 손질 창의 칸을 누름 → 다음에 열 창.
    pub(crate) async fn forge_click(&mut self, f: Forge, slot: usize) -> Result<Page, FrameError> {
        let mut f = f;
        match slot {
            0..=8 => {
                let tab = Tab::ALL[fa::TAB_OF_CELL[slot]];
                if tab != f.tab {
                    let keep = self
                        .picked(f.target)
                        .filter(|p| tab.accepts(p.def.part))
                        .and(f.target);
                    f = Forge::new(tab);
                    f.target = keep;
                }
            }
            fa::BACK => return Ok(Page::Gear),
            fa::SMITH => return Ok(Page::Smith { tab: 0, page: 0 }),
            s if fa::BUTTON.contains(&s) => self.forge_act(&mut f).await?,
            fa::CENTER if f.tab != Tab::Inherit => {
                f.target = None;
                f.confirm = false;
            }
            fa::SOURCE if f.tab == Tab::Inherit => f.source = None,
            fa::TARGET if f.tab == Tab::Inherit => f.target = None,
            fa::SCROLL if f.tab == Tab::Polish => f.scroll = None,
            s if fa::WORN.contains(&s) => {
                let i = fa::WORN.iter().position(|w| *w == s).unwrap();
                // 장신구 탭: 낀 장신구 셋 (넷째 칸은 비어 있음) · 그 밖: 입은 갑옷 넷
                let serial = if f.tab.accessories() {
                    gears().read(|d| d.worn(self.profile.uuid).get(i).copied().flatten())
                } else if self.inventory[5 + i].is_some() {
                    self.ensure_serial(5 + i)
                } else {
                    None
                };
                if let Some(serial) = serial {
                    self.pick_gear(&mut f, serial).await?;
                }
            }
            s => {
                if let Some(inv) = inventory_slot(s)
                    && let Some(item) = self.inventory[inv]
                {
                    let key = item_key(&item);
                    if f.tab == Tab::Polish && config::with(|c| c.scroll(&key).is_some()) {
                        f.scroll = Some(key);
                    } else if config::with(|c| gear_of(&item, c)).is_some()
                        && let Some(serial) = self.ensure_serial(inv)
                    {
                        self.pick_gear(&mut f, serial).await?;
                    }
                }
            }
        }
        Ok(Page::Forge(f))
    }

    async fn pick_gear(&mut self, f: &mut Forge, serial: u32) -> Result<(), FrameError> {
        let Some(p) = self.picked(Some(serial)) else {
            return Ok(());
        };
        if !f.tab.accepts(p.def.part) {
            let msg = if f.tab.accessories() {
                "잠재능력은 반지·목걸이·귀걸이에만 있습니다."
            } else {
                "장신구는 강화·전승할 수 없습니다."
            };
            return self.error(msg).await;
        }
        f.confirm = false;
        if f.tab == Tab::Inherit {
            if f.source.is_none() && f.target != Some(serial) {
                f.source = Some(serial);
            } else if f.source != Some(serial) {
                f.target = Some(serial);
            }
        } else {
            f.target = Some(serial);
        }
        Ok(())
    }

    /// 장비가 바뀌었으면 그 칸을 다시 보냄 (이름·설명이 바뀜).
    async fn refresh(&mut self, spot: Spot) -> std::io::Result<()> {
        if let Spot::Inv(i) = spot {
            self.sync_slot(i).await?;
        }
        Ok(())
    }

    async fn forge_act(&mut self, f: &mut Forge) -> Result<(), FrameError> {
        match f.tab {
            Tab::Enhance => {
                if let Some(s) = f.target {
                    self.enhance(s).await?;
                }
            }
            Tab::Polish => self.polish(f).await?,
            Tab::Tune => self.tune(f).await?,
            Tab::Inherit => self.inherit(f).await?,
            Tab::Salvage => {
                if f.target.is_some() && !f.confirm {
                    f.confirm = true;
                } else {
                    f.confirm = false;
                    self.salvage(f).await?;
                }
            }
        }
        Ok(())
    }

    /// 강화 한 번 (장인의 기운 포함).
    pub(crate) async fn enhance(&mut self, serial: u32) -> Result<(), FrameError> {
        let Some(p) = self.picked(Some(serial)) else {
            return self.error("그 장비를 찾을 수 없습니다.").await;
        };
        if p.def.part.is_accessory() {
            return self.error("장신구는 강화할 수 없습니다.").await;
        }
        let level = p.gear.enhance;
        if level >= MAX_ENHANCE {
            return self.error("이미 최대 강화(+30)입니다.").await;
        }
        let ((money, stones), stone) =
            config::with(|c| (c.enhance_cost(level), c.enhance.stone.clone()));
        if self.count_key(&stone) < stones {
            return self
                .error(&format!("강화석이 모자랍니다 ({stones}개 필요)"))
                .await;
        }
        if !self.pay(money).await {
            return Ok(());
        }
        self.take_key(&stone, stones).await?;
        let roll = (crate::game::rand_u32() % 1_000_000) as f64 / 1_000_000.0;
        let by_artisan = p.gear.artisan >= 1.0 - 1e-9;
        let (ok, gain) = config::with(|c| {
            (
                store::enhance_succeeds(c, level, p.gear.artisan, roll),
                store::artisan_gain(c, level),
            )
        });
        gears().write(|d| {
            if let Some(g) = d.gears.get_mut(&serial) {
                if ok {
                    g.enhance = level + 1;
                    g.artisan = 0.0;
                } else {
                    g.artisan = (g.artisan + gain).min(1.0);
                }
            }
        });
        self.refresh(p.spot).await?;
        if ok {
            self.play_sound("block.anvil.use");
            self.play_sound("entity.player.levelup");
            let how = if by_artisan {
                "장인의 기운으로 "
            } else {
                ""
            };
            self.message(&text::colored(
                &format!("{how}강화 성공! +{} {}", level + 1, p.def.name),
                "gold",
            ))
            .await?;
            if level + 1 >= 15 && (level + 1) % 5 == 0 {
                self.server.publish(Event::System(text::colored(
                    &format!(
                        "{}님이 {}을(를) +{} 강화에 성공했습니다!",
                        self.profile.name,
                        p.def.name,
                        level + 1
                    ),
                    "gold",
                )));
            }
        } else {
            self.play_sound("block.anvil.land");
            let now = (p.gear.artisan + gain).min(1.0);
            self.message(&text::colored(
                &format!("강화 실패… (+{level} 유지, 장인의 기운 {})", percent(now)),
                "red",
            ))
            .await?;
        }
        Ok(())
    }

    async fn polish(&mut self, f: &mut Forge) -> Result<(), FrameError> {
        let Some(p) = self.picked(f.target) else {
            return self.error("장신구를 먼저 고르세요.").await;
        };
        let Some(key) = f.scroll.clone() else {
            return self.error("가방에서 주문서를 고르세요.").await;
        };
        let Some(scroll) = config::with(|c| c.scroll(&key).cloned()) else {
            return Ok(());
        };
        if p.gear.tries_left == 0 {
            return self
                .error("이 장신구는 더 이상 잠재능력을 바를 수 없습니다.")
                .await;
        }
        if !self.take_key(&key, 1).await? {
            return self.error("주문서가 없습니다.").await;
        }
        let lines = config::with(|c| store::roll_potential(c, &scroll, crate::game::rand_u32));
        let serial = f.target.unwrap();
        let left = gears().write(|d| {
            let g = d.gears.get_mut(&serial)?;
            g.tries_left = g.tries_left.saturating_sub(1);
            g.potential = lines.clone();
            g.scroll = Some(scroll.name.clone());
            Some(g.tries_left)
        });
        self.refresh(p.spot).await?;
        if self.count_key(&key) == 0 {
            f.scroll = None;
        }
        self.play_sound("block.enchantment_table.use");
        self.message(&text::colored(
            &format!(
                "[{} 연마] {}: {} (남은 횟수 {})",
                scroll.name,
                p.def.name,
                super::look::potential_text(&lines),
                left.unwrap_or(0)
            ),
            "light_purple",
        ))
        .await?;
        Ok(())
    }

    async fn tune(&mut self, f: &mut Forge) -> Result<(), FrameError> {
        let Some(p) = self.picked(f.target) else {
            return self.error("장신구를 먼저 고르세요.").await;
        };
        if p.gear.potential.is_empty() {
            return self.error("잠재능력이 없습니다. 먼저 연마하세요.").await;
        }
        let rule = config::with(|c| c.forge.clone());
        if self.count_key(&rule.tune_item) < rule.tune_count {
            let name = config::with(|c| c.item_name(&rule.tune_item)).unwrap_or_default();
            return self
                .error(&format!(
                    "{name}이(가) 모자랍니다 ({}개 필요)",
                    rule.tune_count
                ))
                .await;
        }
        if !self.pay(rule.tune_money).await {
            return Ok(());
        }
        self.take_key(&rule.tune_item, rule.tune_count).await?;
        let lines = config::with(|c| {
            store::tune_potential(
                c,
                &p.gear.potential,
                p.gear.scroll.as_deref(),
                crate::game::rand_u32,
            )
        });
        let serial = f.target.unwrap();
        gears().write(|d| {
            if let Some(g) = d.gears.get_mut(&serial) {
                g.potential = lines.clone();
            }
        });
        self.refresh(p.spot).await?;
        self.play_sound("block.amethyst_block.chime");
        self.message(&text::colored(
            &format!(
                "[조율] {}: {}",
                p.def.name,
                super::look::potential_text(&lines)
            ),
            "light_purple",
        ))
        .await?;
        Ok(())
    }

    async fn inherit(&mut self, f: &mut Forge) -> Result<(), FrameError> {
        let (Some(s), Some(t)) = (self.picked(f.source), self.picked(f.target)) else {
            return self.error("보낼 장비와 받을 장비를 고르세요.").await;
        };
        if s.def.part != t.def.part {
            return self.error("같은 부위끼리만 전승할 수 있습니다.").await;
        }
        if s.gear.enhance <= t.gear.enhance {
            return self
                .error("받는 장비의 강화 단계가 같거나 더 높습니다.")
                .await;
        }
        let cost = config::with(|c| c.forge.inherit_money) * (s.gear.enhance as i64).pow(2);
        if !self.pay(cost).await {
            return Ok(());
        }
        let (from, to) = (f.source.unwrap(), f.target.unwrap());
        let level = s.gear.enhance;
        gears().write(|d| {
            if let Some(g) = d.gears.get_mut(&from) {
                g.enhance = 0;
                g.artisan = 0.0;
            }
            if let Some(g) = d.gears.get_mut(&to) {
                g.enhance = level;
                g.artisan = 0.0;
            }
        });
        self.refresh(s.spot).await?;
        self.refresh(t.spot).await?;
        self.play_sound("block.beacon.power_select");
        self.message(&text::colored(
            &format!(
                "전승 완료: {} +{level} → {} +{level} ({}은(는) +0)",
                s.def.name, t.def.name, s.def.name
            ),
            "gold",
        ))
        .await?;
        f.source = None;
        Ok(())
    }

    async fn salvage(&mut self, f: &mut Forge) -> Result<(), FrameError> {
        let Some(p) = self.picked(f.target) else {
            return Ok(());
        };
        let Spot::Inv(slot @ 9..=45) = p.spot else {
            return self.error("입거나 낀 장비는 벗고(빼고) 분해하세요.").await;
        };
        let back = config::with(|c| store::salvage(c, &p.def, p.gear.enhance));
        let serial = f.target.take().unwrap();
        self.inventory[slot] = None;
        self.sync_slot(slot).await?;
        gears().write(|d| {
            d.gears.remove(&serial);
        });
        let mut got = Vec::new();
        for (k, n) in &back {
            self.give_key(k, *n).await?;
            let name = config::with(|c| c.item_name(k)).unwrap_or_else(|| vanilla_name(k));
            got.push(format!("{name} x{n}"));
        }
        self.play_sound("block.grindstone.use");
        self.message(&text::colored(
            &format!("{}을(를) 분해했습니다: {}", p.def.name, got.join(", ")),
            "yellow",
        ))
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_and_cells() {
        for t in Tab::ALL {
            assert_eq!(Tab::parse(t.name()), Some(t));
        }
        assert!(Tab::Polish.accepts(Part::Ring) && !Tab::Polish.accepts(Part::Helmet));
        assert!(Tab::Enhance.accepts(Part::Sword) && !Tab::Enhance.accepts(Part::Earring));
        // 버튼·칸이 겹치지 않음
        let mut used = vec![
            fa::CENTER,
            fa::SCROLL,
            fa::SOURCE,
            fa::TARGET,
            fa::BACK,
            fa::SMITH,
        ];
        used.extend(fa::WORN);
        used.extend(fa::BUTTON);
        let n = used.len();
        used.sort();
        used.dedup();
        assert_eq!(used.len(), n);
        assert!(used.iter().all(|s| *s >= 9 && *s < 54));
        assert_eq!(percent(0.75), "75%");
        assert_eq!(percent(0.4418), "44.2%");
        assert_eq!(grade_pal("영웅"), Pal::Purple);
    }
}
